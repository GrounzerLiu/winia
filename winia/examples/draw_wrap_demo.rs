//! DrawWrapNode visual verification (cf. Compose drawWithContent).
//!
//! Three cards side by side, each proving one slot:
//! - Left: plain red background (baseline, no node).
//! - Middle: enum red background + before-half blue overlay -> blue wins
//!   (before shares the DrawNode background-layer slot).
//! - Right: black text + after-half translucent green bar over the top rows ->
//!   green covers the text (after runs above children).
//!
//! Run: cargo run -p winia --example draw_wrap_demo

use winia::prelude::*;

#[derive(Debug)]
struct BeforeBlue;
impl winia::modifier::DrawWrapNode for BeforeBlue {
    fn draw_before(
        &self,
        canvas: &skia_safe::Canvas,
        rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
        winia::render::draw_background_for_node(
            canvas,
            rect,
            &Color::from_argb(255, 30, 30, 200),
            &winia::modifier::Shape::Rectangle,
        );
    }
    fn node_key(&self) -> String {
        "demo:before-blue".to_string()
    }
}

#[derive(Debug)]
struct AfterGreenBar;
impl winia::modifier::DrawWrapNode for AfterGreenBar {
    fn draw_after(
        &self,
        canvas: &skia_safe::Canvas,
        rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
        let bar = skia_safe::Rect::new(rect.left, rect.top, rect.right, rect.top + 30.0);
        winia::render::draw_background_for_node(
            canvas,
            bar,
            &Color::from_argb(255, 30, 200, 30),
            &winia::modifier::Shape::Rectangle,
        );
    }
    fn node_key(&self) -> String {
        "demo:after-green-bar".to_string()
    }
}

#[composable]
fn draw_wrap_demo(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("DrawWrapNode: before = background layer, after = above content")
                .font_size(16.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    // Baseline: plain red, no node.
                    Text::new("baseline red")
                        .font_size(14.0)
                        .color(Color::WHITE)
                        .modifier(
                            Modifier::new()
                                .size(120.0, 80.0)
                                .background(
                                    Color::from_argb(255, 200, 30, 30),
                                    winia::modifier::Shape::Rectangle,
                                ),
                        )
                        .build(ctx);
                    // Before covers enum background -> blue wins.
                    Text::new("before wins")
                        .font_size(14.0)
                        .color(Color::WHITE)
                        .modifier(
                            Modifier::new()
                                .size(120.0, 80.0)
                                .background(
                                    Color::from_argb(255, 200, 30, 30),
                                    winia::modifier::Shape::Rectangle,
                                )
                                .draw_wrap_node(BeforeBlue),
                        )
                        .build(ctx);
                    // After covers child text: parent holds the node, text is content.
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(140.0, 80.0)
                                .background(Color::WHITE, winia::modifier::Shape::Rectangle)
                                .draw_wrap_node(AfterGreenBar),
                        )
                        .build(ctx, |ctx| {
                            Text::new("CoverMe line1")
                                .font_size(14.0)
                                .color(Color::BLACK)
                                .build(ctx);
                            Text::new("CoverMe line2")
                                .font_size(14.0)
                                .color(Color::BLACK)
                                .build(ctx);
                        });
                });
            Text::new("Expect: red card | blue card | white card with green bar hiding text tops")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(520.0, 300.0)
                .title("DrawWrapNode Demo")
                .build(ctx, draw_wrap_demo);
        });
    });
}
