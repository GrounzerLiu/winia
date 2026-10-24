//! Flex layout regression demo — the shapes the review found broken, on one screen.
//!
//! Run it with `cargo run -p winia --example flex_layout_demo`. Every card is a case that used to
//! measure wrong:
//!
//! 1. a weighted child inside a `vertical_scroll` Column (used to hand it a `f32::MAX` share, which
//!    made the scrolling content 3.4e38 tall);
//! 2. `SpaceBetween` with a single child (used to centre it, Compose packs it at the start);
//! 3. a zero weight child (used to collapse to zero instead of measuring like a sibling);
//! 4. a `required_size` child that overflows its Column (the container used to report the incoming
//!    maximum instead of its content).
//!
//! Each card labels the expected geometry, so a regression shows up on screen. `tests/flex_layout.rs`
//! pins the same cases with numbers and no window.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// A labelled card whose content is the case under test.
fn card(ctx: &mut ComposeCtx, tag: &'static str, title: &str, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .test_tag(tag)
                .background(Color::from_argb(14, 60, 90, 140), Shape::rounded(8.0))
                .padding(10.0),
        )
        .spacing(6.0)
        .build(ctx, |ctx| {
            // The title carries a tag of its own: a test that needs the card's content origin has
            // to read the title's measured height instead of hard-coding a font metric.
            Text::new(title)
                .modifier(Modifier::new().test_tag(format!("{tag}-title")))
                .font_size(14.0)
                .build(ctx);
            content(ctx);
        });
}

fn bar(ctx: &mut ComposeCtx, tag: &str, label: &str, h: f32, color: Color) {
    Stack::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(h)
                .test_tag(tag)
                .background(color, Shape::rounded(4.0)),
        )
        .build(ctx, |ctx| {
            Text::new(label).font_size(11.0).build(ctx);
        });
}

#[composable]
fn flex_layout_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .spacing(14.0)
        .build(ctx, |ctx| {
            Text::new("Flex layout — the four cases from the review").font_size(18.0).build(ctx);

            // 1. Weighted child inside the scrolling content. In Compose an unbounded main axis gives
            //    a weighted child nothing; the point of the card is that the page stays finite.
            card(ctx, "scroll-weight", "1. weight inside the scroller", |ctx| {
                // Remembered, not constructed inline: a new handle on every recomposition is the
                // one shape the state-handle rule forbids (docs/state-handles.md).
                let inner_scroll = ctx.remember(|| ScrollState::new()).get();
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(80.0)
                            .vertical_scroll(inner_scroll),
                    )
                    .build(ctx, |ctx| {
                        Column::new()
                            .modifier(Modifier::new().test_tag("scroll-content"))
                            .build(ctx, |ctx| {
                            bar(ctx, "scroll-fixed", "fixed 40", 40.0, Color::from_argb(255, 90, 160, 220));
                            Text::new("weight(1) below — height 0 here, as in Compose")
                                .modifier(Modifier::new().layout_weight(1.0))
                                .build(ctx);
                        });
                    });
            });

            // 2. SpaceBetween with one child: at the start edge, not centred.
            card(ctx, "space-between-one", "2. SpaceBetween, one child", |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().height(60.0))
                    .arrangement(Arrangement::SpaceBetween)
                    .build(ctx, |ctx| {
                        // A fixed-width block, so the RTL mirror is observable: a `fill_max_width`
                        // child sits at the same x in both directions.
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .width(120.0)
                                    .height(28.0)
                                    .test_tag("space-between-bar")
                                    .background(Color::from_argb(255, 220, 150, 90), Shape::rounded(4.0)),
                            )
                            .build(ctx, |ctx| {
                                Text::new("start-aligned").font_size(11.0).build(ctx);
                            });
                    });
            });

            // 3. weight(0) measures like any other child instead of collapsing to zero.
            card(ctx, "zero-weight", "3. weight(0) is not a collapse", |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().height(90.0))
                    .build(ctx, |ctx| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .fill_max_width()
                                    .height(30.0)
                                    .layout_weight(0.0)
                                    .test_tag("zero-weight-child")
                                    .background(Color::from_argb(255, 120, 200, 140), Shape::rounded(4.0)),
                            )
                            .build(ctx, |ctx| Text::new("weight(0) keeps its 30").font_size(11.0).build(ctx));
                        bar(ctx, "zero-sibling", "the sibling", 30.0, Color::from_argb(255, 90, 160, 220));
                    });
            });

            // 4. Overflow: the inner Column reports its 4 x 45 content height; the 100 dp Stack
            //    around it decides what to clip (the card itself stays 100 + chrome tall).
            card(ctx, "overflow", "4. overflow reports content", |ctx| {
                Stack::new()
                    .modifier(Modifier::new().fill_max_width().height(100.0).clip(Shape::Rectangle))
                    .build(ctx, |ctx| {
                        Column::new()
                            .modifier(Modifier::new().test_tag("overflow-content"))
                            .build(ctx, |ctx| {
                            for i in 0..4 {
                                Stack::new()
                                    .modifier(
                                        Modifier::new()
                                            .required_size(120.0, 45.0)
                                            .test_tag("overflow-row")
                                            .background(
                                                Color::from_argb(200, 200, 120 + i * 30, 120),
                                                Shape::Rectangle,
                                            ),
                                    )
                                    .build(ctx, |ctx| {
                                        Text::new(format!("row {i} — 45 tall, clipped at 100"))
                                            .font_size(11.0)
                                            .build(ctx);
                                    });
                            }
                        });
                    });
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 760.0)
                .title("Flex layout demo")
                .build(ctx, |ctx| {
                    // The shared demo chrome: theme + direction + the top app bar that opens the
                    // settings sheet. Its body must emit exactly one `fill_max_size` root — the
                    // scrolling Column in `flex_layout_demo` is that root.
                    settings::shell("Flex Layout", ctx, flex_layout_demo);
                });
        });
    });
}
