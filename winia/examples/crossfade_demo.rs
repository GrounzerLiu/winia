//! Crossfade demo — the switch behaviours that make it a different component from `AnimatedContent`.
//!
//! Run: `cargo run -p winia --example crossfade_demo`
//!
//! What each section shows:
//! 1. A plain switch with Compose's default (`tween()`, 300 ms, FastOutSlowIn).
//! 2. Switch again WHILE it is still fading: every generation that has not finished stays on screen, each
//!    with its own alpha (Compose's `currentlyVisible` list) — nothing is hard cut. The "Play rapid" button
//!    fires A→B→C about 90 ms apart so three generations overlap.
//! 3. `content_key`: pages that share a key are the SAME content, so moving between them swaps the value with
//!    no animation at all (and keeps their slots).
//! 4. Two specs side by side on one shared state, so the clocks can be compared.
//! 5. No size animation: the container is the content's size and jumps when the content's size changes —
//!    that is the whole difference from `AnimatedContent`.

use letclone::clone;
use winia::animation::{interpolator, AnimationSpec, SnapSpec, TweenSpec};
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

use std::time::Duration;

fn label_color() -> Color {
    WiniaTheme::colors().on_surface_variant
}

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(4.0))
        .build(ctx);
}

/// One page: a coloured card that remembers its own slot index, so a screenshot can tell which generation
/// is which (the label says the page, the marker says the slot).
fn page_card(ctx: &mut ComposeCtx, page: u32, w: f32, h: f32) {
    let slot = ctx.remember(|| {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    });
    let slot = slot.peek();
    let (colour, name) = match page {
        0 => (Color::from_argb(255, 76, 175, 80), "Page A"),
        1 => (Color::from_argb(255, 255, 152, 0), "Page B"),
        _ => (Color::from_argb(255, 233, 30, 99), "Page C"),
    };
    Stack::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .clip(Shape::rounded(12.0))
                .background(colour, Shape::rounded(12.0)),
        )
        .build(ctx, |ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_size())
                .arrangement(Arrangement::Center)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new(name).font_size(15.0).color(Color::WHITE).build(ctx);
                    Text::new(format!("slot {slot}"))
                        .font_size(10.0)
                        .color(Color::from_argb(220, 255, 255, 255))
                        .build(ctx);
                });
        });
}

#[composable]
fn crossfade_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let page = ctx.remember(|| 0u32);
    let keyed = ctx.remember(|| false);
    let size_page = ctx.remember(|| 0u32);
    // The demo's own clock for the rapid sequence: while `playing`, one tween drives 0 -> 1 over 270 ms and
    // the pages advance as it crosses its thresholds; when the run ends the clock SNAPS back to 0 (a different
    // spec per push), so the next click starts a fresh tween. Reading the clock in this scope re-runs it every
    // frame — that is the framework's own frame signal, no timer needed.
    let playing = ctx.remember(|| false);
    let seq = ctx.animate_float_as_state(
        if playing.get() { 1.0 } else { 0.0 },
        if playing.get() {
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(270), interpolator::Linear::new()))
        } else {
            AnimationSpec::Snap(SnapSpec::default())
        },
    );

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            // ── 1 + 2: switch, and switch-while-fading ──
            section_title(ctx, "1. Switch (default spec: 300 ms tween)");
            Row::new().spacing(8.0).build(ctx, |ctx| {
                for (i, name) in ["Page A", "Page B", "Page C"].iter().enumerate() {
                    let p = page.clone();
                    Button::new()
                        .style(ButtonStyle::Tonal)
                        .on_click(move || p.set(i as u32))
                        .build(ctx, |ctx| {
                            Text::new(*name).build(ctx);
                        });
                }
                let p2 = page.clone();
                let run = playing.clone();
                Button::new()
                    .on_click({
                        clone!(run);
                        move || {
                            p2.set(0);
                            run.set(true);
                        }
                    })
                    .build(ctx, |ctx| {
                        Text::new("Play rapid A→B→C").build(ctx);
                    });
            });
            Crossfade::new(page.clone())
                .modifier(Modifier::new().padding_top(8.0))
                .build(ctx, |ctx, p| {
                    // Each page draws a DIFFERENT size, stacked at the top-left by the container, so the
                    // generations that overlap mid-switch are visible as nested rectangles — with one size
                    // they would blend into a single card and nothing of the overlap would show.
                    let (w, h) = match p {
                        0 => (160.0, 60.0),
                        1 => (260.0, 90.0),
                        _ => (360.0, 120.0),
                    };
                    page_card(ctx, p, w, h);
                });

            // The sequence: read the clock and step the page as it climbs; stop at the end and let the clock
            // snap back so the button can run it again.
            let progress = seq.get();
            if playing.get() {
                if progress >= 0.99 {
                    playing.set(false);
                } else {
                    let wanted = if progress < 0.33 {
                        1
                    } else if progress < 0.66 {
                        2
                    } else {
                        page.peek()
                    };
                    if page.peek() != wanted {
                        page.set(wanted);
                    }
                }
            }
            caption(
                ctx,
                "'Play rapid' switches A→B→C about 90 ms apart: all three cards are composed at once, each on \
                 its own alpha, and the ones that were leaving keep fading instead of being cut.",
            );

            // ── 3: content_key ──
            section_title(ctx, "2. content_key: same key, no animation");
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Checkbox::new(keyed.get())
                    .on_checked_change({
                        clone!(keyed);
                        move |v| keyed.set(v)
                    })
                    .build(ctx);
                Text::new("Treat pages A and B as the same content (key = page / 2)")
                    .font_size(12.0)
                    .color(label_color())
                    .build(ctx);
            });
            let crossfade = Crossfade::new(page.clone());
            let crossfade = if keyed.get() {
                crossfade.content_key(|p: &u32| (*p / 2) as u64)
            } else {
                crossfade
            };
            crossfade
                .modifier(Modifier::new().padding_top(8.0))
                .build(ctx, |ctx, p| {
                    page_card(ctx, p, 320.0, 80.0);
                });
            caption(
                ctx,
                "With the box ticked, A↔B changes the value with no fade at all (the entry is replaced in \
                 place, keeping its alpha and its slots); A↔C still fades.",
            );

            // ── 4: two specs on one state ──
            section_title(ctx, "3. Two specs, one state");
            Row::new().spacing(12.0).build(ctx, |ctx| {
                Crossfade::new(page.clone()).build(ctx, |ctx, p| {
                    page_card(ctx, p, 220.0, 90.0);
                });
                Crossfade::new(page.clone())
                    .animation(TweenSpec::new(
                        Duration::from_millis(700),
                        winia::animation::interpolator::EaseOutCubic::new(),
                    ))
                    .build(ctx, |ctx, p| {
                        page_card(ctx, p, 220.0, 90.0);
                    });
            });
            caption(ctx, "Left: 300 ms FastOutSlowIn. Right: 700 ms EaseOutCubic — the same target, a slower clock.");

            // ── 5: no size animation ──
            section_title(ctx, "4. The container is the content's size (no size animation)");
            Button::new()
                .style(ButtonStyle::Tonal)
                .on_click({
                    clone!(size_page);
                    move || size_page.set(if size_page.peek() == 0 { 1 } else { 0 })
                })
                .build(ctx, |ctx| {
                    Text::new("Toggle.").build(ctx);
                });
            let size = size_page.get();
            Crossfade::new(size_page.clone())
                .modifier(Modifier::new().padding_top(8.0))
                .build(ctx, |ctx, p| {
                    let (w, h) = if p == 0 { (90.0, 90.0) } else { (320.0, 44.0) };
                    page_card(ctx, p + 1, w, h);
                });
            let _ = size;
            caption(
                ctx,
                "The card's size changes with the page and the container follows it in one step — Crossfade \
                 has no size animation; animating the box as well is what AnimatedContent adds.",
            );
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(640.0, 900.0)
                .title("Crossfade Demo")
                .build(ctx, |ctx| {
                    settings::shell("Crossfade Demo", ctx, crossfade_demo);
                });
        });
    });
}
