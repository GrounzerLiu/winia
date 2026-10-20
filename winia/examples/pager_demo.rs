//! The pager, three ways, plus the lazy row it is built on.
//!
//! - A full-bleed `HorizontalPager` (PageSize::Fill) with a dot indicator and animate-to-page
//!   buttons: the everyday carousel.
//! - A `HorizontalPager` with `PageSize::Fixed` + `SnapPosition::Center`, so the neighbouring pages
//!   peek in at the edges - the shape a cover-flow or a card carousel has.
//! - A `VerticalPager` inside a fixed-height box: the page size is the box's height.
//! - A `LazyRow` under them, to show the horizontal lazy list is the same engine.
//!
//! Every pager here is the same state type; the readout under the second one prints
//! `current_page` + `current_page_offset_fraction`, which is the continuous position.

use winia::components::{HorizontalPager, PageSize, PagerState, SnapPosition, VerticalPager};
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// One container/content pair per page, cycled: enough to tell two neighbours apart at a glance,
/// and taken from the theme so light and dark both read correctly.
fn page_colors(page: usize, t: &ThemeColors) -> (Color, Color) {
    match page % 4 {
        0 => (t.primary_container, t.on_primary_container),
        1 => (t.secondary_container, t.on_secondary_container),
        2 => (t.tertiary_container, t.on_tertiary_container),
        _ => (t.surface_variant, t.on_surface_variant),
    }
}

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(2.0))
        .build(ctx);
}

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .modifier(Modifier::new().padding_top(16.0).padding_bottom(6.0))
        .build(ctx);
}

/// One page: a big number on its own colour.
#[composable]
fn page_card(ctx: &mut ComposeCtx, page: usize, label: &str) {
    let theme = WiniaTheme::colors();
    let (color, on_color) = page_colors(page, &theme);
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .fill_max_height()
                .background(color, Shape::rounded(16.0)),
        )
        .alignment(Alignment::Center)
        .arrangement(Arrangement::Center)
        .build(ctx, |ctx| {
            Text::new(format!("{}", page + 1))
                .font_size(48.0)
                .color(on_color)
                .build(ctx);
            Text::new(label)
                .font_size(13.0)
                .color(on_color)
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 900.0)
                .title("Pager Demo")
                .build(ctx, |ctx| {
                    settings::shell("Pager Demo", ctx, |ctx| pager_demo_ui(ctx));
                });
        });
    });
}

#[composable]
fn pager_demo_ui(ctx: &mut ComposeCtx) {
    let pages = 5usize;
    // One state per pager: it is the handle the buttons below drive.
    let fill_state = PagerState::remember(ctx, pages);
    let peek_state = PagerState::remember(ctx, 8);
    let vertical_state = PagerState::remember(ctx, 4);

    Column::new()
        .modifier(Modifier::new().fill_max_width().padding(16.0))
        .build(ctx, |ctx| {
            section_title(ctx, "HorizontalPager - PageSize::Fill");
            caption(
                ctx,
                "Every page is exactly the viewport, so every page boundary is a snap position.",
            );
            HorizontalPager::new(fill_state.clone())
                .modifier(Modifier::new().fill_max_width().height(190.0))
                .page_spacing(0.0)
                .build(ctx, |ctx, page| page_card(ctx, page, "full viewport"));

            // Dot indicator: reads current_page, so it recomposes when the counter flips.
            let current = fill_state.current_page();
            caption(
                ctx,
                &format!(
                    "page {} of {} - offset fraction {:+.2}",
                    current,
                    pages,
                    fill_state.current_page_offset_fraction()
                ),
            );
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_top(10.0))
                .arrangement(Arrangement::Center)
                .build(ctx, |ctx| {
                    for i in 0..pages {
                        let theme = WiniaTheme::colors();
                        let color = if i == current { theme.primary } else { theme.outline_variant };
                        Column::new()
                            .modifier(Modifier::new().size(8.0, 8.0).background(color, Shape::pill()).padding_end(6.0))
                            .build(ctx, |_ctx| {});
                    }
                });

            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_top(8.0))
                .arrangement(Arrangement::Center)
                .build(ctx, |ctx| {
                    Button::filled()
                        .on_click({
                            let s = fill_state.clone();
                            move || {
                                let prev = s.current_page().saturating_sub(1);
                                s.animate_scroll_to_page(prev, 0.0);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Prev").build(ctx));
                    Button::filled()
                        .on_click({
                            let s = fill_state.clone();
                            move || {
                                let next = (s.current_page() + 1).min(pages - 1);
                                s.animate_scroll_to_page(next, 0.0);
                            }
                        })
                        .modifier(Modifier::new().padding_start(8.0))
                        .build(ctx, |ctx| Text::new("Next").build(ctx));
                });

            section_title(ctx, "HorizontalPager - PageSize::Fixed(180) + SnapPosition::Center");
            caption(
                ctx,
                "A page smaller than the viewport: the neighbours peek in, and a finished fling settles on a page boundary.",
            );
            HorizontalPager::new(peek_state.clone())
                .modifier(Modifier::new().fill_max_width().height(130.0))
                .page_size(PageSize::Fixed(180.0))
                .snap_position(SnapPosition::Center)
                .page_spacing(12.0)
                .beyond_viewport_page_count(2)
                .build(ctx, |ctx, page| page_card(ctx, page, "fixed 180"));
            caption(
                ctx,
                &format!(
                    "current_page = {}, offset fraction = {:+.2}",
                    peek_state.current_page(),
                    peek_state.current_page_offset_fraction()
                ),
            );

            section_title(ctx, "VerticalPager - the page size is the box's height");
            VerticalPager::new(vertical_state.clone())
                .modifier(Modifier::new().fill_max_width().height(110.0))
                .build(ctx, |ctx, page| page_card(ctx, page, "vertical"));

            section_title(ctx, "LazyRow - the horizontal lazy list underneath");
            LazyRow::new()
                .modifier(Modifier::new().fill_max_width().height(44.0))
                .spacing(8.0)
                .items_plain(24, |ctx, i| {
                    let theme = WiniaTheme::colors();
                    Text::new(format!("item {}", i))
                        .font_size(12.0)
                        .color(theme.on_secondary_container)
                        .modifier(
                            Modifier::new()
                                .padding(10.0)
                                .background(theme.secondary_container, Shape::rounded(20.0)),
                        )
                        .build(ctx);
                })
                .build(ctx);
            caption(ctx, "24 items, only the visible window composes - drag it sideways.");
        });
}