//! UI 测试 fixture：Popup overlay 开关——验证 overlay 树条目随 visible 出现/消失。

use winia::prelude::*;
use winia::ui::overlay::{Popup, PopupPosition};

#[composable]
fn overlay_fixture(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("Overlay fixture").font_size(20.0).build(ctx);
            let s = show.clone();
            Button::text()
                .on_click(move || s.update(|v| *v = !*v))
                .modifier(Modifier::new().test_tag("toggle-overlay"))
                .build(ctx, |ctx| Text::new("Toggle Popup").build(ctx));
            let visible = show.get();
            if visible {
                Text::new("popup-open: yes").build(ctx);
            } else {
                Text::new("popup-open: no").build(ctx);
            }
            let s_close = show.clone();
            Popup::new(visible)
                .position(PopupPosition::BottomLeft)
                // 外部点击 dismiss 时同步状态（Popup 默认 dismiss_on_outside=true：
                // 点 overlay 外 = dismiss，事件不进入主树——按钮收不到点击）
                .on_dismiss_request(move || s_close.set(false))
                .build(ctx, |ctx| {
                    Text::new("Popup content").build(ctx);
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 300.0)
                .title("Overlay Fixture")
                .build(ctx, |ctx| overlay_fixture(ctx));
        });
    });
}
