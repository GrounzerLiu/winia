//! Box layout demo — the three Box behaviours the review fixed.
//!
//! 1. per-child alignment (Compose's `BoxScope.align`): each child can carry its own alignment,
//!    through `Modifier::content_align_self` (both axes) or `align_self` (axis-symmetric);
//! 2. an overflowing child: the box reports the CONTENT size instead of cropping to its parent's
//!    maximum (Compose never consults `mainAxisMax` in Box);
//! 3. `BoxWithConstraints`: its alignment places the content inside the box (it used to be a
//!    silent no-op).
//!
//! Run with `cargo run -p winia --example box_layout_demo`; the same UI is the `box_layout`
//! fixture scenario driven by `tests/ui_box_test.rs`.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

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
            Text::new(title)
                .modifier(Modifier::new().test_tag(format!("{tag}-title")))
                .font_size(14.0)
                .build(ctx);
            content(ctx);
        });
}

/// A small labelled block; `extra` carries the child's own alignment.
fn block(
    ctx: &mut ComposeCtx,
    tag: &'static str,
    label: &str,
    w: f32,
    h: f32,
    color: Color,
    extra: Modifier,
) {
    Stack::new()
        .content_alignment(ContentAlignment::CENTER)
        .modifier(
            Modifier::new()
                .size(w, h)
                .test_tag(tag)
                .background(color, Shape::rounded(6.0))
                .then(extra),
        )
        .build(ctx, |ctx| {
            Text::new(label).font_size(11.0).color(Color::WHITE).build(ctx);
        });
}

#[composable]
fn box_layout_demo(ctx: &mut ComposeCtx) {
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
            Text::new("Box layout — the three review cases").font_size(18.0).build(ctx);

            // 1. Per-child alignment inside one box.
            card(ctx, "box-align", "1. per-child alignment", |ctx| {
                Stack::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(180.0)
                            .test_tag("box-align-frame")
                            .background(Color::from_argb(30, 90, 160, 220), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx| {
                        block(
                            ctx, "box-child-start", "start", 80.0, 32.0,
                            Color::from_argb(255, 90, 160, 220), Modifier::new(),
                        );
                        block(
                            ctx, "box-child-center", "center", 80.0, 32.0,
                            Color::from_argb(255, 120, 200, 140),
                            Modifier::new().align_self(Alignment::Center),
                        );
                        block(
                            ctx, "box-child-end", "bottom-end", 80.0, 32.0,
                            Color::from_argb(255, 220, 150, 90),
                            Modifier::new().content_align_self(ContentAlignment::BOTTOM_END),
                        );
                    });
            });

            // 2. Overflow: the box reports its child's 500 dp width, the parent decides what to clip.
            card(ctx, "box-overflow-card", "2. overflow reports content", |ctx| {
                // A horizontally scrolling strip: its own box is clamped to the 408 dp viewport by the
                // engine, while the 500 dp content inside it keeps its size. A plain Box would grow to
                // 500 (Compose does the same — a Box never crops its child), and the card with it.
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(90.0)
                            .horizontal_scroll(ScrollState::new())
                            .test_tag("box-overflow-clip")
                            .background(Color::from_argb(30, 200, 120, 90), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx| {
                        // `required_size`, not `size`: `size` is coerced into the incoming range (Compose's
                        // `enforceIncoming`), so 500 would come out at the card's width and there would be
                        // nothing to overflow.
                        Stack::new()
                            .content_alignment(ContentAlignment::CENTER)
                            .modifier(
                                Modifier::new()
                                    .required_size(500.0, 40.0)
                                    .test_tag("box-overflow")
                                    .background(
                                        Color::from_argb(255, 200, 120, 90),
                                        Shape::rounded(6.0),
                                    ),
                            )
                            .build(ctx, |ctx| {
                                Text::new("500 dp child in a narrower box")
                                    .font_size(11.0)
                                    .color(Color::WHITE)
                                    .build(ctx);
                            });
                    });
            });

            // 3. BoxWithConstraints: the alignment places the content inside the forced box.
            card(ctx, "bwc-card", "3. BoxWithConstraints alignment", |ctx| {
                BoxWithConstraints::new()
                    .content_alignment(ContentAlignment::BOTTOM_END)
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(120.0)
                            .test_tag("bwc-box")
                            .background(Color::from_argb(30, 120, 90, 200), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx, scope| {
                        Text::new(format!("max width {:.0} dp", scope.max_width()))
                            .font_size(11.0)
                            .build(ctx);
                        block(
                            ctx, "bwc-child", "bottom-end", 100.0, 40.0,
                            Color::from_argb(255, 120, 90, 200), Modifier::new(),
                        );
                    });
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 760.0)
                .title("Box layout demo")
                .build(ctx, |ctx| {
                    settings::shell("Box Layout", ctx, box_layout_demo);
                });
        });
    });
}
