//! Text demo — the parameters `Text` resolves, and where Compose's rules show up in the result:
//! the type scale, letter spacing, `max_lines`, and the two nowrap overflows (an ELLIPSIS truncates
//! the line to the box, `Visible` paints past it — the box is 140 dp and the string is longer).
//!
//! Run with cargo run -p winia --example text_demo.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

const LONG: &str = "the quick brown fox jumps over the lazy dog";

fn card(ctx: &mut ComposeCtx, tag: &str, title: &str, body: impl FnOnce(&mut ComposeCtx)) {
    Stack::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding(16.0)
                .background(
                    Color { r: 0x2A, g: 0x27, b: 0x2E, a: 255 },
                    Shape::rounded(12.0),
                )
                .test_tag(format!("tx-{tag}-card")),
        )
        .build(ctx, |ctx| {
            Column::new()
                .spacing(8.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Text::new(title)
                        .font_size(12.0)
                        .color(Color { r: 0x9E, g: 0x9A, b: 0xA6, a: 255 })
                        .build(ctx);
                    body(ctx);
                });
        });
}

fn text_demo(ctx: &mut ComposeCtx) {
    Column::new()
        .spacing(12.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(ScrollState::new()),
        )
        .build(ctx, |ctx| {
            // 1. The type scale: the measured height follows the font size.
            card(ctx, "scale", "type scale", |ctx| {
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for (index, size) in [14.0, 18.0, 22.0].into_iter().enumerate() {
                            Text::new("Ag")
                                .font_size(size)
                                .modifier(Modifier::new().test_tag(format!("tx-scale-{index}")))
                                .build(ctx);
                        }
                    });
            });

            // 2. Letter spacing widens the same string.
            card(ctx, "spacing", "letter spacing", |ctx| {
                Text::new("AAAA")
                    .font_size(18.0)
                    .modifier(Modifier::new().test_tag("tx-space-0"))
                    .build(ctx);
                Text::new("AAAA")
                    .font_size(18.0)
                    .letter_spacing(4.0)
                    .modifier(Modifier::new().test_tag("tx-space-1"))
                    .build(ctx);
            });

            // 3. max_lines caps the height; both are the same string in the same box.
            card(ctx, "lines", "max lines", |ctx| {
                for (index, lines) in [1usize, 3].into_iter().enumerate() {
                    Text::new(LONG)
                        .font_size(15.0)
                        .max_lines(lines)
                        .modifier(
                            Modifier::new()
                                // Narrow enough that the string really wraps: at 300 the whole line
                                // fits and max_lines would have nothing to cap.
                                .width(180.0)
                                .test_tag(format!("tx-lines-{index}")),
                        )
                        .build(ctx);
                }
            });

            // 4. The nowrap pair: an ellipsis truncates to the 140 dp box, Visible paints past it.
            card(ctx, "nowrap", "nowrap overflow", |ctx| {
                Row::new()
                    .spacing(24.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Text::new(LONG)
                            .font_size(15.0)
                            .soft_wrap(false)
                            .max_lines(1)
                            .overflow(TextOverflow::Ellipsis)
                            .modifier(Modifier::new().width(140.0).test_tag("tx-nowrap-0"))
                            .build(ctx);
                        Text::new(LONG)
                            .font_size(15.0)
                            .soft_wrap(false)
                            .max_lines(1)
                            .overflow(TextOverflow::Visible)
                            .modifier(Modifier::new().width(140.0).test_tag("tx-nowrap-1"))
                            .build(ctx);
                    });
            });

            // 4b. A CENTRED nowrap ellipsis: the ellipsized line is as wide as its box, so there is no
            // slack to centre. A negative one (the render used the PRE-ellipsis intrinsic width)
            // would shift the line out of the box — exactly what the UI test samples.
            card(ctx, "nowrap-center", "nowrap ellipsis, centred", |ctx| {
                Text::new("the quick brown fox jumps over the lazy dog and keeps on running")
                    .font_size(15.0)
                    .soft_wrap(false)
                    .max_lines(1)
                    .overflow(TextOverflow::Ellipsis)
                    .align(TextAlign::Center)
                    .modifier(Modifier::new().width(300.0).test_tag("tx-nowrap-center"))
                    .build(ctx);
            });

            // 4c. min lines, a decoration, and what onTextLayout reports about the measure pass.
            card(ctx, "minlines", "min lines / decoration", |ctx| {
                Row::new()
                    .spacing(16.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Text::new("one line")
                            .font_size(15.0)
                            .modifier(Modifier::new().test_tag("tx-one-line"))
                            .build(ctx);
                        Text::new("one line")
                            .font_size(15.0)
                            .min_lines(3)
                            .modifier(Modifier::new().test_tag("tx-minlines"))
                            .build(ctx);
                        Text::new("underlined")
                            .font_size(15.0)
                            .text_decoration(TextDecoration::UNDERLINE)
                            .modifier(Modifier::new().test_tag("tx-underline"))
                            .build(ctx);
                    });
            });

            // 5. Alignment inside a wide box.
            card(ctx, "align", "alignment", |ctx| {
                for (index, align) in [TextAlign::Left, TextAlign::Center, TextAlign::Right]
                    .into_iter()
                    .enumerate()
                {
                    Text::new("aligned")
                        .font_size(15.0)
                        .align(align)
                        .modifier(
                            Modifier::new()
                                .width(300.0)
                                .test_tag(format!("tx-align-{index}")),
                        )
                        .build(ctx);
                }
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 800.0)
                .title("Text demo")
                .build(ctx, |ctx| {
                    settings::shell("Text", ctx, text_demo);
                });
        });
    });
}
