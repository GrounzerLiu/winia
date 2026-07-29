//! 文本选择演示 — SelectionContainer + 选中高亮
//!
//! 用鼠标点击文本可选中单个字符，配合 SelectionContainer 高亮显示选中的位置。

use winia::prelude::*;
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 400.0)
                .title("Text Selection Demo")
                .build(ctx, |ctx| selection_demo_ui(ctx));
        });
    });
}

fn selection_demo_ui(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .padding(16.0))
        .spacing(16.0)
        .build(ctx, |ctx| {

            // ═══ 1. 可选中文本 ═══
            Text::new("■ Selectable text — click to select a character")
                .font_size(15.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .build(ctx, |ctx| {
                    Text::new("This text is inside a SelectionContainer. Click anywhere on this text to select a character. Selected characters will be highlighted in blue.")
                        .font_size(14.0)
                        .build(ctx);
                });

            // ═══ 2. 带样式的选中文本 ═══
            Text::new("■ Styled text with selection")
                .font_size(15.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .build(ctx, |ctx| {
                    RichText::new().build(ctx, |x| {
                        x.text("Rich text with ");
                        x.bold(|x| { x.text("bold"); });
                        x.text(", ");
                        x.italic(|x| { x.text("italic"); });
                        x.text(", and ");
                        x.color(Color::from_argb(255, 33, 150, 243), |x| {
                            x.bold(|x| { x.text("blue bold"); });
                        });
                        x.text(" text. Click to select.");
                    });
                });

            // ═══ 3. 图文混排 ═══
            Text::new("■ Text with images")
                .font_size(15.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .build(ctx, |ctx| {
                    RichText::new().build(ctx, |x| {
                        x.text("Text with inline ");
                        x.image(star_svg());
                        x.text(" icons and ");
                        x.image(check_svg());
                        x.text(" emoji-like elements.");
                    });
                });

            // ═══ 4. 说明 ═══
            Text::new("Note: Click on any text inside the blue-bordered boxes to highlight a character. Drag and keyboard selection (Shift+Arrow) coming soon.")
                .font_size(12.0)
                .color(Color::from_argb(255, 140, 140, 140))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);
        });
}

fn star_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg viewBox="0 0 24 24"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" fill="#FFC107"/></svg>"##,
        18.0, 18.0).expect("star")
}

fn check_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#4CAF50"/><path d="M8 12l3 3 5-5" stroke="white" stroke-width="2" fill="none"/></svg>"##,
        18.0, 18.0).expect("check")
}
