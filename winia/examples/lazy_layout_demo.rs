//! Lazy-list layout demo — the three cases the review fixed, with lists long enough to scroll.
//!
//! 1. RTL: Compose lays a `LazyRow` out from the RIGHT in RTL, so index 0 sits at the right edge and
//!    the items run leftward. winia's list mirrors its content coordinates (the policy mirrors the
//!    placement, the scroll modifier mirrors the translate) — the same pair of steps
//!    `reverseLayout` uses.
//! 2. An RTL COLUMN keeps its main axis (a column must not scroll upward) but mirrors its CROSS
//!    axis, so `content_padding_cross`'s "before" is the right edge and a narrow item hugs it.
//! 3. A lazy list inside a horizontally unbounded parent reports its WIDEST item instead of the
//!    `f32::MAX` sentinel (3.4e38 would poison the scroll container's content width, hit testing
//!    and any scrollbar built on it).
//!
//! Every list here holds `ITEMS` entries, far more than fits: a lazy list that cannot scroll does
//! not exercise the window, the anchor or the mirror translate.
//!
//! Run with `cargo run -p winia --example lazy_layout_demo`; the same UI is the `lazy_layout`
//! fixture scenario driven by `tests/ui_lazy_test.rs`.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Long enough to scroll in every card, small enough to stay cheap to build.
const ITEMS: usize = 200;

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

fn tile(ctx: &mut ComposeCtx, tag: String, label: String, w: f32, h: f32, color: Color) {
    Stack::new()
        .content_alignment(ContentAlignment::CENTER)
        .modifier(
            Modifier::new()
                .size(w, h)
                .test_tag(tag)
                .background(color, Shape::rounded(6.0)),
        )
        .build(ctx, |ctx| {
            Text::new(label).font_size(11.0).color(Color::WHITE).build(ctx);
        });
}

// `Color::from_argb` is not a `const fn`, so the palette is a function, not constants.
fn palette(i: usize) -> Color {
    if i % 2 == 0 {
        Color::from_argb(255, 90, 160, 220)
    } else {
        Color::from_argb(255, 120, 200, 140)
    }
}

fn accent(i: usize) -> Color {
    if i % 2 == 0 {
        Color::from_argb(255, 220, 150, 90)
    } else {
        Color::from_argb(255, 200, 110, 60)
    }
}

#[composable]
fn lazy_layout_demo(ctx: &mut ComposeCtx) {
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
            Text::new("Lazy list layout — RTL and the unbounded cross axis")
                .font_size(18.0)
                .build(ctx);

            // 1. RTL row: item 0 at the right edge at offset 0, running leftward, 200 tiles so it
            // scrolls. The modifier carries the direction, so the demo does not depend on the
            // window's local (the shell flips that too, and both routes end at the same resolution).
            card(ctx, "lazy-rtl-row-card", "1. LazyRow under RTL (scroll it)", |ctx| {
                LazyRow::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(60.0)
                            .layout_direction(LayoutDirection::Rtl)
                            .test_tag("lazy-rtl-row"),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(ctx, format!("lazy-rtl-item-{i}"), format!("{i}"), 100.0, 44.0, palette(i));
                    })
                    .build(ctx);
            });

            // 2. RTL column with cross content padding: the main axis still runs downward, the cross
            // axis mirrors (start padding 16 on the right, 4 on the left), so a 120 dp item hugs the
            // right edge.
            card(ctx, "lazy-rtl-col-card", "2. LazyColumn under RTL (scroll it)", |ctx| {
                LazyColumn::new()
                    .content_padding_cross(16.0, 4.0)
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(150.0)
                            .layout_direction(LayoutDirection::Rtl)
                            .test_tag("lazy-rtl-column"),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(
                            ctx,
                            format!("lazy-rtl-col-item-{i}"),
                            format!("{i}"),
                            120.0,
                            36.0,
                            palette(i),
                        );
                    })
                    .build(ctx);
            });

            // 3. Unbounded cross axis: the strip is a HORIZONTAL scroll, so the lazy column's width
            // axis has no maximum (f32::MAX). It must report 50 + 6 + 4 = 60 dp, not 3.4e38. Three
            // items are enough — this card is about measurement, not scrolling.
            card(ctx, "lazy-unbounded-card", "3. unbounded cross axis", |ctx| {
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(90.0)
                            .horizontal_scroll(ScrollState::new())
                            .test_tag("lazy-unbounded-clip")
                            .background(Color::from_argb(30, 200, 120, 90), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx| {
                        LazyColumn::new()
                            .content_padding_cross(6.0, 4.0)
                            .modifier(Modifier::new().test_tag("lazy-unbounded-column"))
                            .items(3, |i| i as u64, |ctx, i| {
                                tile(
                                    ctx,
                                    format!("lazy-unbounded-item-{i}"),
                                    format!("{i}"),
                                    50.0,
                                    30.0,
                                    accent(i),
                                );
                            })
                            .build(ctx);
                    });
            });

            // 4. The LTR control: nothing above may move a list that is not mirrored.
            card(ctx, "lazy-ltr-row-card", "4. LTR control (scroll it)", |ctx| {
                LazyRow::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(60.0)
                            .test_tag("lazy-ltr-row"),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(ctx, format!("lazy-ltr-item-{i}"), format!("{i}"), 100.0, 44.0, accent(i));
                    })
                    .build(ctx);
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 860.0)
                .title("Lazy layout demo")
                .build(ctx, |ctx| {
                    settings::shell("Lazy Layout", ctx, lazy_layout_demo);
                });
        });
    });
}
