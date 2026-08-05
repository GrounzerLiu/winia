//! UI 测试 fixture：点击计数（State 更新 → 增量重组渲染）。
//! 场景：`+1` 按钮点击 → `Count` 文本更新；10 个静态行保持（点击后不塌缩）。
//!
//! 由 `[[bin]]` 注册编译为独立 exe，测试通过 stdin/stdout 管道驱动。

use winia::prelude::*;

#[composable]
fn ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get()))
                .font_size(24.0)
                .build(ctx);

            Button::new().on_click({ let c = count.clone(); move || { c.update(|v| *v += 1); } })
                .modifier(Modifier::new().size(200.0, 36.0))
                .build(ctx, |ctx| { Text::new("+1").font_size(12.0).build(ctx); });

            // 静态行——验证点击后结构不塌缩
            for i in 0..10 {
                Row::new().modifier(Modifier::new().size(200.0, 24.0))
                    .build(ctx, |ctx| { Text::new(format!("Line {i}")).font_size(14.0).build(ctx); });
            }
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(400.0, 500.0)
            .title("ui fixture: click")
            .build(ctx, |ctx| { ui(ctx); });
    });
}
