//! Scroll demo - one card per behaviour: plain scrolling with a scrollbar, the programmatic
//! `scroll_by` API, nested scroll (an inner list inside an outer one), the overscroll rubber-band
//! (drag and fling), keyboard scrolling, and a paging list.
//!
//! Everything here is the public API. Each card owns its own `ScrollState`, so the readouts below it
//! are that card's numbers and nothing else's.
//!
//! Try: drag the strip on the right of the first card, press its buttons, scroll the outer list of
//! the nested card and then the inner strip, drag past an edge of the overscroll card (and fling
//! hard into it), click the keyboard card and press arrows / PageUp / PageDown / Home / End, then
//! wheel over the pager.
//!
//! Run with: cargo run -p winia --example scroll_demo

use letclone::clone;
use winia::components::{HorizontalPager, PagerState, VerticalScrollbar};
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

const CARD_BG: Color = Color { r: 0x2A, g: 0x27, b: 0x2E, a: 255 };
const ROW_BG: Color = Color { r: 0x35, g: 0x31, b: 0x3B, a: 255 };
const ROW_ALT: Color = Color { r: 0x3E, g: 0x39, b: 0x45, a: 255 };
const MUTED: Color = Color { r: 0x9E, g: 0x9A, b: 0xA6, a: 255 };
const ACCENT: Color = Color { r: 0xD0, g: 0xBC, b: 0xFF, a: 255 };
const WARM: Color = Color { r: 0xFF, g: 0xB4, b: 0xAB, a: 255 };

/// One titled card.
#[composable]
fn card(ctx: &mut ComposeCtx, title: &str, body: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .spacing(8.0)
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding(12.0)
                .background(CARD_BG, Shape::rounded(12.0)),
        )
        .build(ctx, |ctx| {
            Text::new(title).font_size(12.0).color(ACCENT).build(ctx);
            body(ctx);
        });
}

/// A band of a fixed height, striped, so motion is easy to see.
#[composable]
fn rows(ctx: &mut ComposeCtx, count: usize, row_height: f32, prefix: &str) {
    Column::new().spacing(4.0).build(ctx, |ctx| {
        for i in 0..count {
            let bg = if i % 2 == 0 { ROW_BG } else { ROW_ALT };
            Text::new(format!("{prefix} {i}"))
                .font_size(13.0)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(row_height)
                        .background(bg, Shape::rounded(6.0))
                        .padding_sides(8.0, 0.0, 8.0, 0.0),
                )
                .build(ctx);
        }
    });
}

#[composable]
fn scroll_demo(ctx: &mut ComposeCtx) {
    Column::new().spacing(12.0).build(ctx, |ctx| {
        Text::new("Scroll layout")
            .font_size(20.0)
            .color(WARM)
            .build(ctx);
        Text::new(
            "One card per behaviour. Numbers are that card's own state: offset, limit and whether \
it can still move.",
        )
        .font_size(11.0)
        .color(MUTED)
        .build(ctx);

        // ---- 1. plain scrolling + scrollbar + the programmatic API -----------------
        card(ctx, "1. Plain scroll, scrollbar and scroll_by", |ctx| {
            let scroll = ctx.remember(|| ScrollState::new()).get();
            let last = ctx.remember(|| State::new(String::from("(no scroll_by yet)"))).get();
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Column::new()
                    .modifier(
                        Modifier::new()
                            .width(300.0)
                            .height(150.0)
                            .vertical_scroll(scroll.clone()),
                    )
                    .build(ctx, |ctx| {
                        rows(ctx, 12, 36.0, "row");
                    });
                Column::new()
                    .modifier(Modifier::new().height(150.0).width(10.0))
                    .build(ctx, |ctx| {
                        // `always_show` so a still screenshot shows it: the default is "only while scrolling",
                        // matching Compose's `alwaysShowScrollBar = false`.
                        VerticalScrollbar::new(scroll.clone()).always_show(true).build(ctx);
                    });
            });
            Text::new(format!(
                "offset {:.0} / {:.0} — forward {} backward {}",
                scroll.offset.get(),
                scroll.max_value(),
                scroll.can_scroll_forward(),
                scroll.can_scroll_backward()
            ))
            .font_size(12.0)
            .color(WARM)
            .modifier(Modifier::new().test_tag("scroll-readout"))
            .build(ctx);
            Text::new(last.get())
                .font_size(11.0)
                .color(MUTED)
                .modifier(Modifier::new().test_tag("scroll-last-by"))
                .build(ctx);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                {
                    clone!(scroll, last);
                    Button::filled()
                        .on_click(move || {
                            let moved = scroll.scroll_by(96.0);
                            last.set(format!("scroll_by(+96) consumed {moved:.0}"));
                        })
                        .build(ctx, |ctx| {
                            Text::new("by +96").font_size(12.0).build(ctx);
                        });
                }
                {
                    clone!(scroll, last);
                    Button::filled()
                        .on_click(move || {
                            let moved = scroll.scroll_by(-96.0);
                            last.set(format!("scroll_by(-96) consumed {moved:.0}"));
                        })
                        .build(ctx, |ctx| {
                            Text::new("by -96").font_size(12.0).build(ctx);
                        });
                }
                {
                    clone!(scroll, last);
                    Button::filled()
                        .on_click(move || {
                            scroll.scroll_to(0.0, scroll.max_value());
                            last.set("scroll_to(0)".to_string());
                        })
                        .build(ctx, |ctx| {
                            Text::new("home").font_size(12.0).build(ctx);
                        });
                }
                {
                    clone!(scroll, last);
                    Button::filled()
                        .on_click(move || {
                            let max = scroll.max_value();
                            scroll.scroll_to(max, max);
                            last.set("scroll_to(end)".to_string());
                        })
                        .build(ctx, |ctx| {
                            Text::new("end").font_size(12.0).build(ctx);
                        });
                }
            });
        });

        // ---- 2. nested scroll ------------------------------------------------------
        card(ctx, "2. Nested: an inner strip inside the outer list", |ctx| {
            let outer = ctx.remember(|| ScrollState::new()).get();
            let inner = ctx.remember(|| ScrollState::new()).get();
            Column::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(150.0)
                        .vertical_scroll(outer.clone()),
                )
                .spacing(6.0)
                .build(ctx, |ctx| {
                    rows(ctx, 3, 30.0, "outer");
                    Row::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .height(56.0)
                                .horizontal_scroll(inner.clone()),
                        )
                        .spacing(6.0)
                        .build(ctx, |ctx| {
                            for i in 0..14 {
                                Text::new(format!("cell {i}"))
                                    .font_size(12.0)
                                    .modifier(
                                        Modifier::new()
                                            .width(72.0)
                                            .height(56.0)
                                            .background(ROW_ALT, Shape::rounded(6.0))
                                            .padding_sides(8.0, 0.0, 8.0, 0.0),
                                    )
                                    .build(ctx);
                            }
                        });
                    rows(ctx, 6, 30.0, "outer");
                });
            // Offsets only: they are States and re-render this line. A plain container's limit is a
            // Backchannel the measure writes back, so reading `max_value()` here would print the
            // open-ended sentinel forever - card 1 shows it because its readout also reads the offset,
            // which changes whenever the limit does.
            Text::new(format!(
                "outer offset {:.0} — inner offset {:.0}"
                , outer.offset.get(), inner.offset.get()
            ))
            .font_size(12.0)
            .color(WARM)
            .modifier(Modifier::new().test_tag("scroll-nested-readout"))
            .build(ctx);
            Text::new("A wheel or drag over the strip moves the strip first; at its end the delta \
falls through to the outer list, in that order (pre -> child -> post).")
                .font_size(11.0)
                .color(MUTED)
                .build(ctx);
        });

        // ---- 3. overscroll ---------------------------------------------------------
        card(ctx, "3. Overscroll: drag or fling past an edge", |ctx| {
            let scroll = ctx.remember(|| ScrollState::new()).get();
            let effect = ctx.remember(|| winia::overscroll::OverscrollEffect::new()).get();
            Column::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(130.0)
                        .vertical_scroll(scroll)
                        .overscroll(&effect),
                )
                .build(ctx, |ctx| {
                    rows(ctx, 5, 34.0, "bend");
                });
            Text::new(
                "The content shifts past the edge (120 px cap) and springs back 60 ms after the last \
pull. A fling that dies at an edge bends by velocity/40 px instead of just stopping.",
            )
            .font_size(11.0)
            .color(MUTED)
            .build(ctx);
        });

        // ---- 4. keyboard scrolling --------------------------------------------------
        card(ctx, "4. Keyboard scrolling", |ctx| {
            let scroll = ctx.remember(|| ScrollState::new()).get();
            Column::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(130.0)
                        .vertical_scroll(scroll.clone()),
                )
                .build(ctx, |ctx| {
                    rows(ctx, 9, 32.0, "key");
                    // Focus targets at the BOTTOM of the band: tabbing to one scrolls the band
                    // (focus reveal), the half of keyboard usability scrolling alone does not give.
                    for i in 0..3 {
                        // A button with no action is not focusable, and a focus target is the point
                        // of this row.
                        Button::filled().on_click(|| {}).build(ctx, |ctx| {
                            Text::new(format!("focus me {i}")).font_size(12.0).build(ctx);
                        });
                    }
                });
            Text::new(format!("offset {:.0}", scroll.offset.get()))
                .font_size(12.0)
                .color(WARM)
                .modifier(Modifier::new().test_tag("scroll-key-readout"))
                .build(ctx);
            Text::new(
                "Press inside the band first (focus or the last press picks the target), then arrows \
move 48 px, PageUp/PageDown one viewport, Home/End an end. At an end the key falls through to \
focus navigation instead of being swallowed.",
            )
            .font_size(11.0)
            .color(MUTED)
            .build(ctx);
        });

        // ---- 5. paging ---------------------------------------------------------------
        card(ctx, "5. Paging: one wheel notch is one page", |ctx| {
            let pager = PagerState::remember(ctx, 5);
            HorizontalPager::new(pager.clone())
                .modifier(Modifier::new().fill_max_width().height(120.0))
                .build(ctx, |ctx, page| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_size()
                                .background(if page % 2 == 0 { ROW_BG } else { ROW_ALT }, Shape::rounded(8.0)),
                        )
                        .build(ctx, |ctx| {
                            Text::new(format!("page {page}"))
                                .font_size(16.0)
                                .color(ACCENT)
                                .build(ctx);
                        });
                });
            Text::new(format!("current page {}", pager.current_page()))
                .font_size(12.0)
                .color(WARM)
                .modifier(Modifier::new().test_tag("scroll-page-readout"))
                .build(ctx);
            Text::new(
                "A paged container steps a whole page per wheel notch (a deliberate winia deviation: \
Compose nudges it a few pixels and the wheel reads as broken).",
            )
            .font_size(11.0)
            .color(MUTED)
            .build(ctx);
        });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(720.0, 1500.0)
                .title("Scroll demo")
                .build(ctx, |ctx| {
                    settings::shell("Scroll", ctx, |ctx| {
                        let page = ctx.remember(|| ScrollState::new()).get();
                        Column::new()
                            .spacing(12.0)
                            .modifier(
                                Modifier::new()
                                    .fill_max_size()
                                    .vertical_scroll(page)
                                    .padding(16.0),
                            )
                            .build(ctx, |ctx| scroll_demo(ctx));
                    });
                });
        });
    });
}
