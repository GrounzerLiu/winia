//! 多种嵌套组合演示——验证 #[composable] 宏的语句级 key 注入在复杂嵌套下工作：
//! 嵌套函数、多级 if/else、match、for/while 循环、深层 content 闭包、
//! 结构切换（插入/移除分支）时 remember 状态保留（key 源码位置稳定不漂移）。
//!
//! ⚠ content 闭包约定：`build(ctx, |ctx| { ... })` 的内容闭包必须是**单参数且
//! 参数名为 ctx**（可带类型标注 `|ctx: &mut ComposeCtx|`）——宏据此注入语句级 key。
//! 其他单参数名为 ctx 的闭包（如延迟执行的异步回调）会被误注入——避免在
//! #[composable] 函数内使用"参数名为 ctx 的非 content 闭包"。

use winia::prelude::*;
use winia::app;
use winia::ComposeCtx;
use winia::core::state::State;

/// 深层 content 闭包：Column > Row > Stack > Button > Text（5 层）
#[composable]
fn deep_nest(ctx: &mut ComposeCtx, tag: &str) {
    Column::new()
        .modifier(Modifier::new().padding(8.0).background(Color::from_argb(40, 120, 200, 120), Shape::rounded(8.0)))
        .build(ctx, |ctx| {
            Text::new(format!("[{tag}] deep nest: Column > Row > Stack > Button > Text"))
                .font_size(12.0).color(Color::from_argb(180, 60, 60, 60))
                .build(ctx);
            Row::new().modifier(Modifier::new().padding(4.0)).build(ctx, |ctx| {
                Stack::new().modifier(Modifier::new().size(120.0, 36.0)).build(ctx, |ctx| {
                    Button::new().modifier(Modifier::new().size(120.0, 36.0).background(Color::from_argb(255, 70, 130, 200), Shape::rounded(4.0)))
                        .build(ctx, |ctx| {
                            Text::new(format!("[{tag}] button")).font_size(12.0).color(Color::WHITE).build(ctx);
                        });
                });
            });
        });
}

/// 多级 if/else（3 层）+ 嵌套闭包内 if——验证结构切换时 remember 状态保留
#[composable]
fn multi_if(ctx: &mut ComposeCtx, level: &State<i32>) {
    Text::new("── multi-level if/else ──").font_size(13.0).color(Color::from_argb(200, 0, 120, 0)).build(ctx);

    // remember 状态（结构切换后应保留——key 源码稳定）
    let clicks = ctx.remember(|| 0i32);
    let n = clicks.get();

    if level.get() == 1 {
        Text::new(format!("level 1 — clicks={n}")).font_size(12.0).build(ctx);
        Button::new().on_click({ let c = clicks.clone(); move || { c.update(|v| *v += 1); () } })
            .modifier(Modifier::new().size(140.0, 28.0))
            .build(ctx, |ctx| { Text::new("click me").font_size(12.0).build(ctx); });
        if level.get() > 0 {
            if n % 2 == 0 {
                Text::new("  → even").font_size(12.0).color(Color::from_argb(200, 0, 100, 150)).build(ctx);
            } else {
                Text::new("  → odd").font_size(12.0).color(Color::from_argb(200, 150, 100, 0)).build(ctx);
                if n > 2 {
                    Text::new("    → >2").font_size(12.0).build(ctx);
                } else {
                    Text::new("    → ≤2").font_size(12.0).build(ctx);
                }
            }
        }
    } else if level.get() == 2 {
        // 分支 2：结构完全不同的子树（切换时 key 稳定——clicks 保留）
        Column::new().modifier(Modifier::new().padding(4.0).background(Color::from_argb(30, 200, 150, 100), Shape::rounded(6.0)))
            .build(ctx, |ctx| {
                Text::new(format!("level 2 — clicks={n}")).font_size(12.0).build(ctx);
                Row::new().build(ctx, |ctx| {
                    Button::new().on_click({ let c = clicks.clone(); move || { c.update(|v| *v += 3); () } })
                        .modifier(Modifier::new().size(100.0, 28.0))
                        .build(ctx, |ctx| { Text::new("+3").font_size(12.0).build(ctx); });
                    Button::new().on_click({ let c = clicks.clone(); move || { c.update(|v| *v -= 1); () } })
                        .modifier(Modifier::new().size(100.0, 28.0))
                        .build(ctx, |ctx| { Text::new("-1").font_size(12.0).build(ctx); });
                });
            });
    } else {
        Text::new(format!("level other — clicks={n}")).font_size(12.0).build(ctx);
    }
}

/// match 分支 + for/while 循环嵌套
#[composable]
fn match_and_loops(ctx: &mut ComposeCtx, mode: &State<i32>) {
    Text::new("── match + loops ──").font_size(13.0).color(Color::from_argb(200, 120, 0, 120)).build(ctx);

    match mode.get() {
        0 => {
            Text::new("mode 0 — for loop 5 rows").font_size(12.0).build(ctx);
            // for 循环（语句 id 固定——循环内 counter 区分）
            for i in 0..5 {
                Row::new().modifier(Modifier::new().size(180.0, 20.0))
                    .build(ctx, |ctx| {
                        Text::new(format!("row {i}")).font_size(12.0).build(ctx);
                    });
            }
        }
        1 => {
            Text::new("mode 1 — while loop 3 rows").font_size(12.0).build(ctx);
            let mut i = 0;
            while i < 3 {
                Row::new().modifier(Modifier::new().size(180.0, 20.0).background(Color::from_argb(30, 100, 150, 200), Shape::rounded(3.0)))
                    .build(ctx, |ctx| {
                        Text::new(format!("while row {i}")).font_size(12.0).build(ctx);
                    });
                i += 1;
            }
        }
        _ => {
            Text::new("mode other — nested for × for").font_size(12.0).build(ctx);
            // 嵌套 for（外层 3 × 内层 2——嵌套语句注入）
            for outer in 0..3 {
                Row::new().build(ctx, |ctx| {
                    for inner in 0..2 {
                        Text::new(format!("[{outer},{inner}]")).font_size(11.0).build(ctx);
                    }
                });
            }
        }
    }
}

/// 顶层结构切换（if 分支插入/移除——后续节点 key 不漂移）
#[composable]
fn toggler(ctx: &mut ComposeCtx, show_extra: &State<bool>) {
    Button::new().on_click({ let s = show_extra.clone(); move || { s.update(|v| { *v = !*v; }); () } })
        .modifier(Modifier::new().size(200.0, 30.0).background(Color::from_argb(255, 150, 100, 180), Shape::rounded(4.0)))
        .build(ctx, |ctx| {
            Text::new(if show_extra.get() { "Hide extra" } else { "Show extra" }).font_size(12.0).color(Color::WHITE).build(ctx);
        });

    // 条件插入分支（结构变化——后面的节点 key 应稳定）
    if show_extra.get() {
        deep_nest(ctx, "extra");
    }
    // 稳定节点（结构变化后仍复用/状态保留）
    let stable = ctx.remember(|| 0i32);
    Button::new().on_click({ let c = stable.clone(); move || { c.update(|v| { *v += 1; }); () } })
        .modifier(Modifier::new().size(120.0, 28.0))
        .build(ctx, |ctx| {
            Text::new(format!("stable counter: {}", stable.get())).font_size(12.0).build(ctx);
        });
}

/// 顶层组合函数（#[composable]——函数级 scope key 源码稳定；最佳实践：
/// 组合入口函数也应标注，使函数内语句的 key 有独立 scope 基）
#[composable]
fn nest_demo(ctx: &mut ComposeCtx) {
    let level = ctx.remember(|| 1i32);
    let mode = ctx.remember(|| 0i32);
    let show_extra = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size().vertical_scroll(ctx.remember(|| ScrollState::new()).get()))
        .build(ctx, |ctx| {
            Text::new("Nesting Demo — #[composable] statement key injection")
                .font_size(16.0).color(Color::from_argb(220, 30, 30, 30)).build(ctx);

            // 控制区
            Row::new().build(ctx, |ctx| {
                Button::new().on_click({ let s = level.clone(); move || { s.update(|v| { *v = (*v % 3) + 1; }); } })
                    .modifier(Modifier::new().size(110.0, 28.0))
                    .build(ctx, |ctx| { Text::new("switch if-level").font_size(12.0).build(ctx); });
                Button::new().on_click({ let s = mode.clone(); move || { s.update(|v| { *v = (*v + 1) % 3; }); () } })
                    .modifier(Modifier::new().size(110.0, 28.0))
                    .build(ctx, |ctx| { Text::new("switch mode").font_size(12.0).build(ctx); });
                Button::new().on_click({ let s = show_extra.clone(); move || { s.update(|v| { *v = !*v; }); () } })
                    .modifier(Modifier::new().size(110.0, 28.0))
                    .build(ctx, |ctx| { Text::new("toggle extra").font_size(12.0).build(ctx); });
            });

            // 各嵌套场景（#[composable] 函数——语句级注入）
            multi_if(ctx, &level);
            match_and_loops(ctx, &mode);
            toggler(ctx, &show_extra);
            deep_nest(ctx, "tail");
        });
}

fn main() {
    // panic hook: 写入文件以便诊断崩溃
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {:?}", info);
        eprintln!("{}", msg);
        let _ = std::fs::write("D:/Projects/winia/crash.log", &msg);
    }));

    // 启动 tokio 运行时（供 debug WS server 使用）
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 640.0)
                .title("Nesting Demo")
                .build(ctx, |ctx| {
                    nest_demo(ctx);
                });
        });
    });
}
