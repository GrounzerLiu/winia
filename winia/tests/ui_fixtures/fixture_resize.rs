//! UI 测试 fixture：窗口 resize 后自适应内容更新。
//!
//! 展示 `window_size()`（自适应 API）随 resize 事件刷新——验证
//! SurfaceResized → window_size_state.set() → 依赖方重组 的通路。

use winia::prelude::*;

#[composable]
fn resize_fixture(ctx: &mut ComposeCtx) {
    let (w, h) = window_size();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("Resize fixture").font_size(20.0).build(ctx);
            Text::new(format!("window-size: {w:.0}x{h:.0}")).build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 300.0)
                .title("Resize Fixture")
                .build(ctx, |ctx| resize_fixture(ctx));
        });
    });
}
