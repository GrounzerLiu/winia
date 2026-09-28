//! Custom `TabRow` indicators — Compose's `indicator` slot, composed at measure time.
//!
//! Two cases, both built from the positions the row measured this frame:
//! 1. a rounded RED bar the caller draws through `Canvas` (colour chosen to be unique, so a pixel
//!    probe can find it), on a fixed `TabRow`;
//! 2. the same slot on a `ScrollableTabRow`, where the positions live in the scrolling content's
//!    space, so the indicator scrolls with the tabs.

use letclone::clone;
use winia::prelude::*;

/// Unique on purpose: a pixel probe can search for exactly this colour.
const MARKER: (u8, u8, u8) = (0xE5, 0x00, 0x2E);

#[composable]
fn tab_indicator_demo(ctx: &mut ComposeCtx) {
    let fixed_sel = ctx.remember(|| 2usize);
    let scroll_sel = ctx.remember(|| 3usize);
    let scroll_state = ctx.remember(|| ScrollState::new()).get();
    // Diagnostic switch (a demo-only knob): "fixed" builds just the fixed row, "scroll" just the
    // scrollable one, anything else both. One build, three configurations — the cheapest way to tell
    // which of them is the one that keeps the loop awake.
    let mode = ctx.remember(|| std::env::var("WINIA_TAB_DEMO").unwrap_or_default()).get();
    let want_fixed = mode != "scroll";
    let want_scroll = mode != "fixed";

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            if want_fixed {
            Text::new("TabRow + custom indicator (red bar drawn by the caller)")
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            let sel = fixed_sel.clone();
            let row = TabRow::new(sel.get(), {
                clone!(sel);
                move |ctx| {
                    for i in 0..3 {
                        let label = format!("Tab {}", i + 1);
                        Tab::new({ clone!(sel); sel.get() == i }, { clone!(sel); move || sel.set(i) })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                }
            });
            // `plain` mode keeps the row's OWN indicator, so the two can be compared directly.
            let row = if mode == "plain" { row } else { row.indicator({
                move |ctx, scope| {
                    // NOTE: no `sel.get()` here on purpose. A read inside the subcomposition registers on
                    // the INNER composer, and this example used to keep the loop awake with one.
                    let (x, w) = match scope.selected_position() {
                        Some(p) => (p.left + (p.width - p.content_width) / 2.0, p.content_width),
                        None => (0.0, 0.0),
                    };
                    // `Canvas`, not a `Spacer`: a Spacer's own `size(0, h)` is applied first and a later
                    // `.width(w)` cannot widen it (measured: the bar came out 0x4 and the row inherited
                    // that box). A Canvas has no intrinsic size, so the modifier's box is the bar.
                    Canvas::new()
                        .modifier(Modifier::new().width(w).height(4.0).offset(x, 44.0))
                        .build(ctx, move |ds| {
                            // Canvas coordinates: draw inside the node's own rect (see the note on the
                            // scrollable row's indicator below).
                            let r = ds.rect();
                            ds.draw_round_rect(
                                skia_safe::Rect::from_xywh(r.left, r.top, r.width(), r.height()),
                                r.height() / 2.0,
                                Color::from_argb(255, MARKER.0, MARKER.1, MARKER.2),
                            );
                        });
                }
            }) };
            row.build(ctx);

            Spacer::vertical(8.0);
            }

            if want_scroll {
            Text::new("ScrollableTabRow + custom indicator (scrolls with the tabs)")
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            ScrollableTabRow::new(scroll_sel.get(), {
                clone!(scroll_sel);
                move |ctx| {
                    for i in 0..8 {
                        let label = format!("S{}", i + 1);
                        Tab::new({ clone!(scroll_sel); scroll_sel.get() == i }, { clone!(scroll_sel); move || scroll_sel.set(i) })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                }
            })
            .scroll_state(scroll_state.clone())
            .indicator({
                clone!(scroll_sel);
                move |ctx, scope| {
                    let _ = scroll_sel.get();
                    let (x, w) = match scope.selected_position() {
                        Some(p) => (p.left + (p.width - p.content_width) / 2.0, p.content_width),
                        None => (0.0, 0.0),
                    };
                    Canvas::new()
                        .modifier(Modifier::new().width(w).height(4.0).offset(x, 0.0))
                        .build(ctx, move |ds| {
                            // `ds.rect()` is the node's own rect IN CANVAS COORDINATES: the primitives
                            // take canvas coordinates, so a caller that draws at (0,0) paints at the
                            // window's origin (measured: the bar landed there until this was fixed).
                            let r = ds.rect();
                            ds.draw_round_rect(
                                skia_safe::Rect::from_xywh(r.left, r.top, r.width(), r.height()),
                                r.height() / 2.0,
                                Color::from_argb(255, MARKER.0, MARKER.1, MARKER.2),
                            );
                        });
                }
            })
            .build(ctx);
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        // The theme wraps the WINDOW, as every other example does: with it inside the window's build
        // closure, window-level resolution (surface background, the paint path) runs before any theme
        // is in scope. This example had it the wrong way round and the caller's indicator came out
        // BLACK instead of the colour asked for — the framework's paint was reading an unset colour.
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 260.0)
                .title("Tab Indicator Demo")
                .build(ctx, |ctx| tab_indicator_demo(ctx));
        });
    });
}
