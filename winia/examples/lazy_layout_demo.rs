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
//! Every list here holds `kit::ITEMS` entries, far more than fits: a lazy list that cannot scroll
//! does not exercise the window, the anchor or the mirror translate. The button-driven API page is
//! its own example (`lazy_ops_demo`).
//!
//! Run with `cargo run -p winia --example lazy_layout_demo`; the same UI is the `lazy_layout`
//! fixture scenario driven by `tests/ui_lazy_test.rs`.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;
#[path = "common/lazy_kit.rs"]
mod kit;

use kit::{accent, card, list_name, palette, tile, ITEMS};

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
                            .test_tag("lazy-rtl-row")
                            .then(list_name("RTL row")),
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
                            .test_tag("lazy-rtl-column")
                            .then(list_name("RTL column")),
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

            // 4. The LTR control: nothing above may move a list that is not mirrored. It also shows
            //    its own `layoutInfo` — the visible range as the measure pass reported it, which is
            //    Compose's `LazyListState.layoutInfo` read from a composable.
            card(ctx, "lazy-ltr-row-card", "4. LTR control (scroll it)", |ctx| {
                let state = ctx.remember(|| LazyListState::new()).get();
                LazyRow::new()
                    .state(state.clone())
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(60.0)
                            .test_tag("lazy-ltr-row")
                            .then(list_name("LTR row")),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(ctx, format!("lazy-ltr-item-{i}"), format!("{i}"), 100.0, 44.0, accent(i));
                    })
                    .build(ctx);
                // Rendered unconditionally: the first composition runs before the first measure, so
                // the placeholder is what shows on frame one and the real range from frame two.
                let info = state.layout_info();
                let label = match (info.visible_items.first(), info.visible_items.last()) {
                    (Some(first), Some(last)) => {
                        format!("items {}-{} of {}", first.index, last.index, info.total_items)
                    }
                    _ => "items pending".to_string(),
                };
                Text::new(label)
                    .modifier(Modifier::new().test_tag("lazy-ltr-info"))
                    .font_size(11.0)
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
