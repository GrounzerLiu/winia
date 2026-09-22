//! UI 测试 fixture：声明式多窗口（Window composable 生命周期）。
//! 场景：`Open sub window` 按钮点击 → 子窗口创建；再点（Close）→ 子窗口销毁；
//! 主窗口内容全程保持。
//!
//! One scenario of the `fixture_all` binary (see `fixture_all.rs`): the harness spawns it with this
//! scenario's name and drives it over the stdin/stdout pipe.

use winia::prelude::*;

#[composable]
fn ui(ctx: &mut ComposeCtx) {
    let show_window = ctx.remember(|| false);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Main window").font_size(24.0).build(ctx);

            Button::new().on_click({ let s = show_window.clone(); move || { s.update(|v| *v = !*v); } })
                .modifier(Modifier::new().size(200.0, 32.0).background(Color::from_argb(255, 180, 100, 200), Shape::rounded(4.0)))
                .build(ctx, |ctx| {
                    Text::new(if show_window.get() { "Close sub window" } else { "Open sub window" })
                        .color(Color::WHITE).font_size(12.0).build(ctx);
                });

            if show_window.get() {
                Window::new()
                    .size(250.0, 180.0)
                    .title("Sub Window")
                    .on_close({ let s = show_window.clone(); move || { s.update(|v| *v = false); } })
                    .build(ctx, |ctx| {
                        let sw_count = ctx.remember(|| 0i32);
                        Text::new(format!("Sub count: {}", sw_count.get()))
                            .font_size(18.0)
                            .modifier(Modifier::new().padding(8.0))
                            .build(ctx);
                    });
            }
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(400.0, 500.0)
            .title("ui fixture: subwindow")
            .build(ctx, |ctx| { ui(ctx); });
    });
}
