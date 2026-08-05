//! UI 测试 fixture：滚动容器（vertical_scroll）。
//! 场景：30 行内容在 150px 高的滚动容器内；`offset:` 文本实时显示滚动偏移——
//! 滚动命令生效时 offset 变化（树 JSON 可断言——真实滚动行为验证）。
//!
//! 由 `[[bin]]` 注册编译为独立 exe，测试通过 stdin/stdout 管道驱动。

use winia::prelude::*;

#[composable]
fn ui(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Scroll area").font_size(24.0).build(ctx);
            // 滚动偏移实时显示——树 JSON 断言滚动行为（ScrollState.offset 是公开 State）
            Text::new(format!("offset: {:.0}", scroll_y.offset.get()))
                .font_size(14.0)
                .build(ctx);

            Column::new()
                .modifier(Modifier::new().size(200.0, 150.0).vertical_scroll(scroll_y))
                .build(ctx, |ctx| {
                    for i in 0..30 {
                        Row::new().modifier(Modifier::new().size(200.0, 24.0))
                            .build(ctx, |ctx| {
                                Text::new(format!("Line {i}")).font_size(14.0).build(ctx);
                            });
                    }
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(400.0, 500.0)
            .title("ui fixture: scroll")
            .build(ctx, |ctx| { ui(ctx); });
    });
}
