//! Effect API 演示 — LaunchedEffect / DisposableEffect / remember_coroutine_scope

use letclone::clone;
use winia::prelude::*;
use winia::app;
use winia::effect::{LaunchedEffect, DisposableEffect, remember_coroutine_scope};
use std::time::Duration;

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 520.0)
                .title("Effect API Demo")
                .build(ctx, |ctx| {
                    effect_demo_ui(ctx);
                });
        });
    });
}

/// effect 演示主界面（#[composable] = 函数级 scope）
#[composable]
fn effect_demo_ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let show_counter = ctx.remember(|| false);

    // ── 协程作用域（生命周期绑定当前 composable）──
    let scope = remember_coroutine_scope(ctx);

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {

            Text::new("Effect API Demo")
                .font_size(20.0)
                .build(ctx);

            // ═══════════════════════════════════════
            // LaunchedEffect: count 变化时启动异步任务
            // ═══════════════════════════════════════
            LaunchedEffect::new(count.get()).build(ctx, {
                clone!(count);
                move |_scope| async move {
                    println!("[LaunchedEffect] count changed to: {}", count.get());
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    println!("[LaunchedEffect] async work done for count={}", count.get());
                }
            });

            Text::new(format!("Count: {}", count.get()))
                .font_size(28.0)
                .build(ctx);

            Row::new().spacing(8.0).build(ctx, |ctx| {
                // 普通点击
                Button::new()
                    .on_click({ clone!(count); move || count.update(|v| *v += 1) })
                    .modifier(Modifier::new().size(60.0, 36.0))
                    .build(ctx, |ctx| { Text::new("+1").build(ctx); });

                // 通过协程作用域延迟更新
                let spawn_count = ctx.remember(|| 0i32);
                Button::new()
                    .on_click({
                        clone!(count, scope, spawn_count);
                        move || {
                            let (c2, sc2) = (count.clone(), spawn_count.clone());
                            scope.spawn(async move {
                                sc2.update(|v| *v += 1);
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                c2.update(|v| *v += 10);
                                sc2.update(|v| *v -= 1);
                            });
                        }
                    })
                    .style(ButtonStyle::Tonal)
                    .modifier(Modifier::new().size(160.0, 36.0))
                    .build(ctx, |ctx| {
                        let label = format!("+10 (active: {})", spawn_count.get());
                        Text::new(label).build(ctx);
                    });
            });

            // ═══════════════════════════════════════
            // DisposableEffect: 条件渲染 + 清理
            // ═══════════════════════════════════════
            Button::new()
                .on_click({ clone!(show_counter); move || show_counter.update(|v| *v = !*v) })
                .style(ButtonStyle::Outlined)
                .modifier(Modifier::new().size(200.0, 36.0))
                .build(ctx, |ctx| {
                    Text::new(if show_counter.get() { "Hide sub component" } else { "Show sub component" })
                        .build(ctx);
                });

            // 条件渲染——进入/离开时 DisposableEffect 自动 setup/cleanup
            if show_counter.get() {
                sub_component(ctx, count.clone());
            }
        });
}

/// 子组件（#[composable] = 独立函数级 scope：local_count 变化只重跑本函数）
#[composable]
fn sub_component(ctx: &mut ComposeCtx, parent_count: State<i32>) {
    let local_count = ctx.remember(|| 0i32);

    // DisposableEffect: 进入时 setup，离开时 cleanup
    DisposableEffect::new(()).build(ctx, |_| {
        println!("[DisposableEffect] sub component mounted");
        move || {
            println!("[DisposableEffect] sub component disposed");
        }
    });

    Column::new()
        .modifier(Modifier::new().padding(12.0).background(Color::from_argb(255, 240, 240, 255), Shape::rounded(8.0)))
        .spacing(8.0)
        .build(ctx, |ctx| {

            Text::new("🧩 Sub Component (DisposableEffect)")
                .font_size(14.0)
                .build(ctx);

            Text::new(format!("Parent: {}  Local: {}", parent_count.get(), local_count.get()))
                .font_size(16.0)
                .build(ctx);

            Row::new().spacing(8.0).build(ctx, |ctx| {
                Button::new()
                    .on_click({ clone!(local_count); move || local_count.update(|v| *v += 1) })
                    .style(ButtonStyle::Tonal)
                    .modifier(Modifier::new().size(80.0, 32.0))
                    .build(ctx, |ctx| { Text::new("Local +1").build(ctx); });

                Button::new()
                    .on_click({ clone!(parent_count); move || parent_count.update(|v| *v += 1) })
                    .style(ButtonStyle::Outlined)
                    .modifier(Modifier::new().size(80.0, 32.0))
                    .build(ctx, |ctx| { Text::new("Parent +1").build(ctx); });
            });
        });
}
