//! UI 测试 fixture：条件结构切换（if/else 分支）。
//! 场景：`Show Alt` 按钮点击 → 分支 A（Alternative）↔ 分支 B（Add 10）互斥切换。
//!
//! 由 `[[test]] harness = false` 编译为独立 exe，测试通过 stdin/stdout 管道驱动。

use winia::prelude::*;

#[composable]
fn ui(ctx: &mut ComposeCtx) {
    let show_alt = ctx.remember(|| false);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new().on_click({ let s = show_alt.clone(); move || { s.update(|v| *v = !*v); } })
                .modifier(Modifier::new().size(200.0, 32.0))
                .build(ctx, |ctx| {
                    Text::new(if show_alt.get() { "Show Normal" } else { "Show Alt" })
                        .font_size(12.0).build(ctx);
                });

            if show_alt.get() {
                // 分支 A：静态文本（隐藏分支 B）
                Text::new("✳ Alternative content")
                    .font_size(18.0)
                    .modifier(Modifier::new().size(200.0, 60.0).padding(8.0))
                    .build(ctx);
            } else {
                // 分支 B：按钮（隐藏分支 A）
                Button::new()
                    .modifier(Modifier::new().size(200.0, 60.0))
                    .build(ctx, |ctx| { Text::new("Add 10").font_size(14.0).build(ctx); });
            }
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(400.0, 500.0)
            .title("ui fixture: toggle")
            .build(ctx, |ctx| { ui(ctx); });
    });
}
