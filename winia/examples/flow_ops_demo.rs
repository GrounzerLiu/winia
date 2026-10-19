//! The flow layer end to end: a search box that debounces, searches asynchronously and reports
//! whether it is still waiting for you to stop typing.
//!
//! Four APIs, one story:
//! - State::changes() — the stream the two below are built on (bottom card: the raw edit log).
//! - debounce         — the query the search actually runs on, 300 ms after you stop typing.
//! - produce_state    — an async search keyed on that debounced query.
//! - snapshot_flow    — a derived boolean (query != debounced), emitted only when it flips.

use futures_util::StreamExt;
use letclone::clone;
use std::time::Duration;
use winia::components::{TextField, TextFieldValue};
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

const CORPUS: &[&str] = &[
    "alpha", "alphabet", "alphanumeric", "beta", "betamax", "gamma", "gamma-ray", "delta",
    "delta-force", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
];

/// Stands in for a network round trip: filter the corpus.
fn search(query: &str) -> Vec<String> {
    let needle = query.to_lowercase();
    CORPUS
        .iter()
        .filter(|item| needle.is_empty() || item.contains(&needle))
        .map(|item| item.to_string())
        .collect()
}

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(2.0))
        .build(ctx);
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 660.0)
                .title("Flow Ops Demo")
                .build(ctx, |ctx| {
                    settings::shell("Flow Ops Demo", ctx, |ctx| flow_demo_ui(ctx));
                });
        });
    });
}

#[composable]
fn flow_demo_ui(ctx: &mut ComposeCtx) {
    // The text field value, and the plain string every stream below follows.
    let field = ctx.remember(|| TextFieldValue::new(""));
    let query = ctx.remember(|| String::new());

    // ── 1. State::changes(): every edit ──
    // A bounded log. This is what a stream feeding UI state looks like: the effect owns the
    // stream, the UI reads a State.
    let edit_log = ctx.remember(|| Vec::<String>::new());
    LaunchedEffect::new(()).build(ctx, {
        clone!(query, edit_log);
        move |_| async move {
            let mut edits = Box::pin(query.changes());
            while let Some(text) = edits.next().await {
                edit_log.update(|log| {
                    log.push(text);
                    if log.len() > 3 {
                        log.remove(0);
                    }
                });
            }
        }
    });

    // ── 2. debounce: the query the search actually runs on ──
    let debounced = ctx.remember(|| String::new());
    LaunchedEffect::new(()).build(ctx, {
        clone!(query, debounced);
        move |_| async move {
            let mut quiet = Box::pin(debounce(query.changes(), Duration::from_millis(300)));
            while let Some(text) = quiet.next().await {
                debounced.set(text);
            }
        }
    });

    // ── 3. snapshot_flow: a derived value, emitted only when it flips ──
    let typing = ctx.remember(|| false);
    LaunchedEffect::new(()).build(ctx, {
        clone!(query, debounced, typing);
        move |_| async move {
            let pending = snapshot_flow(move || query.get() != debounced.get());
            let mut it = Box::pin(pending);
            while let Some(flag) = it.next().await {
                typing.set(flag);
            }
        }
    });

    // ── 4. produce_state: an async source keyed on the debounced query ──
    let key = debounced.get();
    let search_for = key.clone();
    let results = produce_state(ctx, Vec::<String>::new(), key, move |state| async move {
        // A real producer would talk to a service; the delay just makes the state visible.
        tokio::time::sleep(Duration::from_millis(250)).await;
        state.set(search(&search_for));
    });

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_width())
        .spacing(10.0)
        .build(ctx, |ctx| {
            TextField::new(field)
                .filled()
                .label(|ctx| Text::new("Query").build(ctx))
                .placeholder(|ctx| Text::new("Type to search").build(ctx))
                .on_value_change({
                    clone!(query);
                    move |value: TextFieldValue| query.set(value.text)
                })
                .build(ctx);

            Card::elevated().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .spacing(6.0)
                    .build(ctx, |ctx| {
                        Text::new(format!("query: {:?}", query.get())).font_size(13.0).build(ctx);
                        Text::new(format!("debounced: {:?}", debounced.get()))
                            .font_size(13.0)
                            .build(ctx);
                        Text::new(if typing.get() { "typing..." } else { "settled" })
                            .font_size(13.0)
                            .color(WiniaTheme::colors().primary)
                            .build(ctx);
                        caption(ctx, "debounced follows query 300 ms after the last keystroke; the line above is snapshot_flow(|| query != debounced) and only flips on the transition.");
                    });
            });

            Card::elevated().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .spacing(4.0)
                    .build(ctx, |ctx| {
                        Text::new(format!("results for {:?}", debounced.get()))
                            .font_size(13.0)
                            .build(ctx);
                        let found = results.get();
                        if found.is_empty() {
                            caption(ctx, "nothing matched");
                        }
                        for item in found {
                            Text::new(item).font_size(15.0).build(ctx);
                        }
                        caption(ctx, "produce_state: an async producer keyed on the debounced query.");
                    });
            });

            Card::outlined().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .spacing(4.0)
                    .build(ctx, |ctx| {
                        Text::new("last edits (State::changes)").font_size(13.0).build(ctx);
                        for entry in edit_log.get() {
                            Text::new(format!("  {entry:?}"))
                                .font_size(12.0)
                                .color(WiniaTheme::colors().on_surface_variant)
                                .build(ctx);
                        }
                    });
            });
        });
}
