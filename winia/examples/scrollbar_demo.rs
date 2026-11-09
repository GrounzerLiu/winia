//! Scrollbar demo - every behaviour of the scrollbar in one place.
//!
//! Cards: the default bar (fades in on scroll, THICKENS and TINTS while hovered or dragged, and a
//! press on the empty track jumps the thumb under the pointer), an always-visible bar, a horizontal
//! bar, a lazy list with the lazy adapter (drag the thumb to jump the list), a reversed vertical
//! container (`vertical_scroll_reverse`, the bar mirrors its geometry), and a custom look.
//!
//! Everything here is the public API. Each card owns its own state, so the readouts show that card's
//! numbers only.
//!
//! Try: scroll a card so its bar fades in, hover the bar (it thickens and darkens), press the empty
//! track below the thumb (the thumb jumps so its centre lands under the pointer), drag the thumb, and
//! compare the always-visible card with the fading one.
//!
//! Run with: cargo run -p winia --example scrollbar_demo

use winia::components::{HorizontalScrollbar, LazyScrollbar, VerticalScrollbar};
use winia::layout::lazy::LazyListState;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

const CARD_BG: Color = Color {
    r: 0x2A,
    g: 0x27,
    b: 0x2E,
    a: 255,
};
const ROW_BG: Color = Color {
    r: 0x35,
    g: 0x31,
    b: 0x3B,
    a: 255,
};
const ROW_ALT: Color = Color {
    r: 0x3E,
    g: 0x39,
    b: 0x45,
    a: 255,
};
const MUTED: Color = Color {
    r: 0x9E,
    g: 0x9A,
    b: 0xA6,
    a: 255,
};
const ACCENT: Color = Color {
    r: 0xD0,
    g: 0xBC,
    b: 0xFF,
    a: 255,
};
const WARM: Color = Color {
    r: 0xFF,
    g: 0xB4,
    b: 0xAB,
    a: 255,
};

/// One titled card with an optional caption.
#[composable]
fn card(ctx: &mut ComposeCtx, title: &str, caption: &str, body: impl FnOnce(&mut ComposeCtx)) {
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
            Text::new(caption).font_size(10.0).color(MUTED).build(ctx);
        });
}

/// A striped band of rows, so motion is easy to see.
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

/// A vertical scroll band with a bar on its right, the arrangement every card uses.
#[composable]
fn vertical_band(
    ctx: &mut ComposeCtx,
    scroll: ScrollState,
    height: f32,
    rows_count: usize,
    prefix: &str,
    bar: impl FnOnce(VerticalScrollbar) -> VerticalScrollbar,
) {
    Row::new()
        .spacing(8.0)
        .modifier(Modifier::new().fill_max_width().height(height))
        .build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .fill_max_height()
                        .layout_weight(1.0)
                        .vertical_scroll(scroll.clone()),
                )
                .build(ctx, |ctx| rows(ctx, rows_count, 30.0, prefix));
            bar(VerticalScrollbar::new(scroll)).build(ctx);
        });
}

#[composable]
fn scrollbar_demo(ctx: &mut ComposeCtx) {
    // 1. Default: fades in, thickens and tints on hover, press the track to jump.
    let fading = ctx.remember(|| ScrollState::new()).get();
    card(
        ctx,
        "1. Default bar: fade, hover thickening, tint, track jump",
        "scroll to make the bar appear; hover it (thicker + darker); press the empty track to jump the thumb there; drag the thumb.",
        |ctx| vertical_band(ctx, fading.clone(), 200.0, 24, "row", |b| b),
    );

    // 2. Always visible, so the fade of card 1 has a reference.
    let always = ctx.remember(|| ScrollState::new()).get();
    card(
        ctx,
        "2. always_show(true): visible at rest",
        "the same bar with the fade disabled - the thumb is there before the first scroll.",
        |ctx| {
            vertical_band(ctx, always.clone(), 130.0, 14, "stay", |b| {
                b.always_show(true)
            })
        },
    );

    // 3. Horizontal bar under a wide row.
    let horizontal = ctx.remember(|| ScrollState::new()).get();
    card(
        ctx,
        "3. Horizontal bar",
        "a wide row scrolled sideways, with the bar underneath and the same hover and track behaviour.",
        |ctx| {
            Column::new().spacing(6.0).build(ctx, |ctx| {
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(70.0)
                            .horizontal_scroll(horizontal.clone()),
                    )
                    .build(ctx, |ctx| {
                        for i in 0..14 {
                            let bg = if i % 2 == 0 { ROW_BG } else { ROW_ALT };
                            Text::new(format!("cell {i}"))
                                .font_size(13.0)
                                .modifier(
                                    Modifier::new()
                                        .width(110.0)
                                        .fill_max_height()
                                        .background(bg, Shape::rounded(8.0))
                                        .padding(10.0),
                                )
                                .build(ctx);
                        }
                    });
                HorizontalScrollbar::new(horizontal.clone())
                    .always_show(true)
                    .build(ctx);
            });
        },
    );

    // 4. Lazy list with the adapter: the thumb drags the LIST, not a ScrollState.
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    card(
        ctx,
        "4. Lazy column with LazyScrollbar",
        "the adapter reads the list's own offset and limit; dragging the thumb jumps a list of 200 items.",
        |ctx| {
            Row::new()
                .spacing(8.0)
                .modifier(Modifier::new().fill_max_width().height(170.0))
                .build(ctx, |ctx| {
                    LazyColumn::new()
                        .state(lazy_state.clone())
                        .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                        .items(
                            200,
                            |i| i as u64,
                            |ctx, i| {
                                let bg = if i % 2 == 0 { ROW_BG } else { ROW_ALT };
                                Text::new(format!("item {i}"))
                                    .font_size(13.0)
                                    .modifier(
                                        Modifier::new()
                                            .fill_max_width()
                                            .height(28.0)
                                            .background(bg, Shape::rounded(6.0))
                                            .padding_sides(8.0, 0.0, 8.0, 0.0),
                                    )
                                    .build(ctx);
                            },
                        )
                        .build(ctx);
                    LazyScrollbar::new(lazy_state.clone())
                        .always_show(true)
                        .build(ctx);
                });
        },
    );

    // 5. Reversed vertical: offset 0 is the END, so the bar's geometry mirrors.
    let reversed = ctx.remember(|| ScrollState::new()).get();
    card(
        ctx,
        "5. Reversed vertical (vertical_scroll_reverse)",
        "offset 0 shows the last row; the bar is told so and mirrors its geometry - showing the end puts the thumb at the bottom, like a chat.",
        |ctx| {
            Row::new()
                .spacing(8.0)
                .modifier(Modifier::new().fill_max_width().height(150.0))
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_height()
                                .layout_weight(1.0)
                                .vertical_scroll(reversed.clone())
                                .vertical_scroll_reverse(true),
                        )
                        .build(ctx, |ctx| rows(ctx, 16, 30.0, "rev"));
                    VerticalScrollbar::new(reversed.clone())
                        .always_show(true)
                        .scroll_reverse(Some(true))
                        .build(ctx);
                });
        },
    );

    // 6. Custom look: thicker bar, warm colours, taller minimum thumb.
    let custom = ctx.remember(|| ScrollState::new()).get();
    card(
        ctx,
        "6. Custom style",
        "thickness 10 (grows to 16 on hover), a warm thumb, a faint track, and a 40 px minimum thumb.",
        |ctx| {
            vertical_band(ctx, custom.clone(), 140.0, 18, "style", |b| {
                b.thickness(10.0)
                    .hover_thickness(16.0)
                    .thumb_color(WARM)
                    .hover_color(ACCENT)
                    .track_color(Color::from_argb(40, 0xD0, 0xBC, 0xFF))
                    .thumb_min_length(40.0)
                    .always_show(true)
            });
        },
    );

    // Readouts: the numbers behind the bars above.
    card(ctx, "7. Readouts", "the card 1 state, live.", |ctx| {
        // `f32::MAX` is the "not measured yet" sentinel (Compose uses Int.MAX_VALUE for the same
        // thing); printing it raw reads as a bug, so the card says so instead.
        let limit = fading.max_value_subscribed();
        let limit = if limit >= f32::MAX {
            "not measured yet".to_string()
        } else {
            format!("{limit:.0}")
        };
        Text::new(format!(
            "offset {:.0} / limit {} - scrolling {} - forward {} backward {}",
            fading.offset.get(),
            limit,
            fading.is_scroll_in_progress.get(),
            fading.can_scroll_forward(),
            fading.can_scroll_backward(),
        ))
        .font_size(12.0)
        .color(MUTED)
        .build(ctx);
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1500.0)
                .title("Scrollbar demo")
                .build(ctx, |ctx| {
                    settings::shell("Scrollbar", ctx, |ctx| {
                        let page = ctx.remember(|| ScrollState::new()).get();
                        Column::new()
                            .spacing(12.0)
                            .modifier(
                                Modifier::new()
                                    .fill_max_size()
                                    .vertical_scroll(page)
                                    .padding(16.0),
                            )
                            .build(ctx, |ctx| scrollbar_demo(ctx));
                    });
                });
        });
    });
}
