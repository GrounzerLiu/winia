//! `side_effect`: run a callback once, at the end of the composition that registered it.
//!
//! Compose calls this `SideEffect`. It is the seam between the composition and everything that is
//! not part of it — a window, an analytics sink, a platform object — and the ordering is the whole
//! point:
//!
//! - the callback runs **after** the slot tree has been materialized, so it sees the tree this
//!   frame actually built, not a half-built one;
//! - a `State` it writes is therefore scheduled for the **next** composition, not attributed to
//!   the one that is ending;
//! - there is **no cleanup** — use `DisposableEffect` when something has to be torn down.
//!
//! The demo makes the second point visible. `count` is composition state; `published` is written
//! only from the side effect, so it always trails `count` by exactly one composition. Press +1 and
//! watch the two numbers move one after the other — that gap is the frame boundary.

use letclone::clone;
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

    // ── The side effect ──
    // Registered here, runs at the end of this composition: after materialization, after the
    // dependency frame closed, after every transaction committed.
    side_effect(ctx, {
        clone!(count, published, analytics);
        move || {
            let value = count.peek();
            // Scheduled for the NEXT composition — see the trailing number in the UI.
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
                        caption(
                            ctx,
                            "Press +1 once: count moves immediately, published catches up on the \
                             next composition. The write is scheduled after the dependency frame \
                             closed, which is exactly why it lands one frame late.",
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
