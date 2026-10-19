//! `side_effect`: run a callback once, at the end of the composition that registered it.
//!
//! Compose calls this `SideEffect`. It is the seam between the composition and everything that is
//! not part of it — a window, an analytics sink, a platform object — and the ordering is the whole
//! point:
//!
//! - the callback runs **after** the slot tree has been materialized, so it sees the tree this
//!   frame actually built, not a half-built one;
//! - a `State` it writes is therefore consumed by the **next pass**, not attributed to the one
//!   that is ending — and `recompose_until_stable` runs that pass before layout, so the screen
//!   never shows the intermediate value;
//! - there is **no cleanup** — use `DisposableEffect` when something has to be torn down.
//!
//! That last point has a consequence worth stating plainly: `published` below does NOT trail
//! `count` on screen. Both settle inside the same frame, which is why the interesting number here
//! is `runs` — the effect runs once per pass, and a click costs two passes (the one that ran it
//! and the one its write scheduled).

use letclone::clone;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// Something outside the composition.
///
/// A real application would have a window handle, a file or a network client here. What matters is
/// the shape: it is plain data behind an `Arc<Mutex<_>>`, **not** a `State`, so nothing recomposes
/// when it changes and reading it during composition is just a read.
#[derive(Default)]
struct Analytics {
    published: Vec<i32>,
}

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(4.0))
        .build(ctx);
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 600.0)
                .title("side_effect Demo")
                .build(ctx, |ctx| {
                    settings::shell("side_effect Demo", ctx, |ctx| {
                        side_effect_demo_ui(ctx);
                    });
                });
        });
    });
}

#[composable]
fn side_effect_demo_ui(ctx: &mut ComposeCtx) {
    // Composition state: the value the user edits.
    let count = ctx.remember(|| 0i32);
    // Written ONLY from the side effect below, so the ordering is visible on screen.
    let published = ctx.remember(|| -1i32);
    // The sink, outside the composition. `peek` because the slot is a container, never a value.
    let analytics: Arc<Mutex<Analytics>> = ctx
        .remember(|| Arc::new(Mutex::new(Analytics::default())))
        .peek();
    // How many times the effect has run. A plain atomic, not a State: it is an observation of the
    // effect, not something the composition should react to.
    let runs: Arc<AtomicUsize> = ctx.remember(|| Arc::new(AtomicUsize::new(0))).peek();

    // ── The side effect ──
    // Registered here, runs at the end of this composition: after materialization, after the
    // dependency frame closed, after every transaction committed.
    side_effect(ctx, {
        clone!(count, published, analytics, runs);
        move || {
            let value = count.peek();
            // Lands on the NEXT PASS: this one already read `published`, and the pass the write
            // schedules is what renders the new number - before layout, so nothing flickers.
            runs.fetch_add(1, Ordering::SeqCst);
            published.set(value);
            analytics.lock().unwrap().published.push(value);
            println!("[side_effect] published {value}");
        }
    });

    let (recorded, last) = {
        let sink = analytics.lock().unwrap();
        (sink.published.len(), sink.published.last().copied())
    };

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_width())
        .spacing(8.0)
        .build(ctx, |ctx| {
            section_title(ctx, "Composition state");

            Card::elevated().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .spacing(10.0)
                    .build(ctx, |ctx| {
                        Text::new(format!("count: {}", count.get())).font_size(24.0).build(ctx);
                        Row::new().spacing(8.0).build(ctx, |ctx| {
                            Button::filled()
                                .on_click({ clone!(count); move || count.update(|v| *v += 1) })
                                .build(ctx, |ctx| Text::new("+1").build(ctx));
                            Button::outlined()
                                .on_click({ clone!(count); move || count.set(0) })
                                .build(ctx, |ctx| Text::new("Reset").build(ctx));
                        });
                    });
            });

            section_title(ctx, "Outside the composition");

            Card::elevated().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .spacing(6.0)
                    .build(ctx, |ctx| {
                        Text::new(format!("published: {}", published.get()))
                            .font_size(24.0)
                            .build(ctx);
                        Text::new(format!("records in the sink: {recorded}"))
                            .font_size(12.0)
                            .build(ctx);
                        Text::new(format!("last recorded: {last:?}"))
                            .font_size(12.0)
                            .color(WiniaTheme::colors().on_surface_variant)
                            .build(ctx);
                        Text::new(format!("side-effect runs: {}", runs.load(Ordering::SeqCst)))
                            .font_size(12.0)
                            .build(ctx);
                        caption(
                            ctx,
                            "Press +1: count changes, and published follows it before anything is \
                             drawn - the write lands on the next PASS of the same frame, which \
                             recompose_until_stable runs before layout. So the two numbers agree \
                             on screen; what a click costs is the extra pass, and runs goes up by two.",
                        );
                    });
            });

            section_title(ctx, "What it is for");

            caption(
                ctx,
                "Publish to things that are not part of the composition: a window, a platform \
                 object, an analytics sink. It runs once per composition, has no cleanup and no \
                 rollback, and a composition that never commits drops it unrun.",
            );
        });
}
