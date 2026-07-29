//! 文本选中演示 — SelectionContainer + 拖拽选择
//! 选中文本会自动高亮（半透明蓝底）

use winia::prelude::*;
use winia::app;

fn selection_ui(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Text Selection Demo")
                .font_size(24.0).font_weight(FontWeight::BOLD)
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            Text::new("Drag to select text below. The selection highlights in blue.")
                .font_size(12.0)
                .color(Color::from_argb(200, 128, 128, 128))
                .build(ctx);

            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(Color::from_argb(30, 200, 200, 100), Shape::rounded(8.0)))
                .build(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new("The SelectionContainer makes all text inside selectable.")
                            .font_size(16.0).modifier(Modifier::new().padding(4.0))
                            .build(ctx);

                        Text::new("Try dragging your mouse across this paragraph to select words.")
                            .font_size(14.0)
                            .color(Color::from_argb(200, 60, 60, 60))
                            .modifier(Modifier::new().padding(4.0))
                            .build(ctx);

                        Text::new("You can select across multiple lines! The blue highlight follows the mouse as you drag. Release to finalize the selection.")
                            .font_size(13.0)
                            .modifier(Modifier::new().padding(4.0))
                            .build(ctx);
                    });
                });

            Text::new("Blue highlight shows selected text range above.")
                .font_size(12.0)
                .color(Color::from_argb(150, 100, 100, 100))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(500.0, 500.0)
                .title("Text Selection Demo")
                .build(ctx, selection_ui);
        });
    });
}
