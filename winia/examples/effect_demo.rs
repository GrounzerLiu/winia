//! Effect API 演示 — LaunchedEffect / DisposableEffect / remember_coroutine_scope

use winia::prelude::*;
use winia::app;
use winia::effect::{LaunchedEffect, DisposableEffect, remember_coroutine_scope};
use std::time::Duration;

fn main() {
    // 启动 tokio 运行时（供 LaunchedEffect / CoroutineScope 使用）
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter(); // 进入 runtime 上下文，使 Handle::try_current() 可用

    app::run_app(|ctx| {
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
            let c1 = count.clone();
            LaunchedEffect::new(count.get()).build(ctx, move |_scope| async move {
                println!("[LaunchedEffect] count changed to: {}", c1.get());
                tokio::time::sleep(Duration::from_secs(1)).await;
                println!("[LaunchedEffect] async work done for count={}", c1.get());
            });

            Text::new(format!("Count: {}", count.get()))
                .font_size(28.0)
                .build(ctx);

            Row::new().spacing(8.0).build(ctx, |ctx| {
                // 普通点击
                Button::new()
                    .on_click({ let c = count.clone(); move || c.update(|v| *v += 1) })
                    .modifier(Modifier::new().size(60.0, 36.0))
                    .build(ctx, |ctx| { Text::new("+1").build(ctx); });

                // 通过协程作用域延迟更新
                let spawn_count = ctx.remember(|| 0i32);
                Button::new()
                    .on_click({
                        let c = count.clone();
                        let s = scope.clone();
                        let sc = spawn_count.clone();
                        move || {
                            let c2 = c.clone();
                            let sc2 = sc.clone();
                            s.spawn(async move {
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
                .on_click({ let s = show_counter.clone(); move || s.update(|v| *v = !*v) })
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
                    .on_click({ let c = local_count.clone(); move || c.update(|v| *v += 1) })
                    .style(ButtonStyle::Tonal)
                    .modifier(Modifier::new().size(80.0, 32.0))
                    .build(ctx, |ctx| { Text::new("Local +1").build(ctx); });

                Button::new()
                    .on_click({ let c = parent_count; move || c.update(|v| *v += 1) })
                    .style(ButtonStyle::Outlined)
                    .modifier(Modifier::new().size(80.0, 32.0))
                    .build(ctx, |ctx| { Text::new("Parent +1").build(ctx); });
            });
        });
}
