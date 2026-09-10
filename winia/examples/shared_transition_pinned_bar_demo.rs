//! Keeping pinned chrome on top of a shared-element flight — the Compose way:
//! `Modifier.renderInSharedTransitionScopeOverlay(zIndexInOverlay)`.
//!
//! The window is a Stack — the screen content first, then a **pinned app bar as
//! the last child**. The detail hero lands with its upper band under that bar,
//! and the flight climbs from the bottom of the window, so the band crosses
//! into the bar's strip near the end of the flight.
//!
//! - Bar opted in (switch ON, the default here): the bar joins the transition
//!   layer for as long as the scope is transitioning, with `zIndexInOverlay`
//!   1.0 against the shared elements' 0.0 — so **both** ends of the flight pass
//!   under it, exactly like ordinary content sliding under pinned chrome.
//! - Bar opted out (switch OFF): the flight is elevated into the layer while
//!   the bar stays in tree order, so the hero paints over the bar on the way in
//!   (and its leaving ghost washes over it — the leaving end is detached and
//!   cannot be covered by tree content).
//!
//! Turning the shared element's own `render_in_overlay` off is the other way to
//! keep the hero under the bar (see `shared_transition_demo`), but it only
//! works for the entering end and exposes that end to its ancestors' clips. The
//! chrome-side modifier below is what Compose recommends for chrome.
//!
//! Run: `cargo run -p winia --example shared_transition_pinned_bar_demo`

use letclone::clone;
use winia::animation::SpringSpec;
use winia::prelude::*;

const APP_BAR_H: f32 = 90.0;
const CONTROLS_Y: f32 = 452.0;
const BAR_BG: Color = Color { r: 42, g: 46, b: 66, a: 255 };

/// One marked hero box (the shared element). The flag is read HERE — the slot
/// that creates the marked node — so a runtime switch rewrites the arena
/// marker immediately (a flag read in an outer slot leaves this subtree
/// skipped and the marker keeps its old value).
#[composable]
fn hero_box(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    shape: Shape,
    scope: &SharedTransitionScope,
    in_overlay: &State<bool>,
) {
    let overlay = in_overlay.get();
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .background(color, shape)
                .shared_element(
                    scope.shared_content_state("hero"),
                    BoundsTransform::spring(SpringSpec { stiffness: 120.0, ..SpringSpec::default() }),
                    PlaceHolderSize::JumpCut,
                    PathMotion::Linear,
                    0.0,
                    overlay,
                ),
        )
        .build(ctx, |_| {});
}

/// List screen: the hero sits LOW, so the flight climbs toward the pinned bar.
#[composable]
fn list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: &State<bool>) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("List").font_size(24.0).build(ctx);
            spacer(ctx, 10.0, 250.0);
            hero_box(ctx, 150.0, 150.0, Color::RED, Shape::Circle, scope, in_overlay);
        });
}

/// Detail screen: the hero lands at the top of the window, with its upper band
/// under the pinned bar — ordinary content sliding under pinned chrome.
#[composable]
fn detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: &State<bool>) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            spacer(ctx, 10.0, 24.0);
            hero_box(ctx, 320.0, 170.0, Color::BLUE, Shape::Rectangle, scope, in_overlay);
        });
}

/// Plain spacer leaf (kept local so the two screens stay symmetrical).
fn spacer(ctx: &mut ComposeCtx, w: f32, h: f32) {
    let key = ctx.next_key();
    ctx.start_leaf(key, Modifier::new().size(w, h));
    ctx.end_node();
}

/// The chrome. `render_in_shared_transition_scope_overlay` is the Compose
/// `renderInSharedTransitionScopeOverlay(zIndexInOverlay)` equivalent: while
/// the scope has a flight this subtree renders in the layer after the flying
/// pair (z 1.0 > their 0.0), and outside a flight it is ordinary tree content
/// again.
#[composable]
fn pinned_bar(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, enabled: &State<bool>) {
    // Read in this slot: it owns the marked node, so flipping the switch
    // rewrites the marker before the next flight resolves.
    let on = enabled.get();
    let chrome = if on {
        Modifier::new().render_in_shared_transition_scope_overlay(scope, 1.0)
    } else {
        Modifier::new()
    };
    Column::new()
        .modifier(
            Modifier::new()
                .size(420.0, APP_BAR_H)
                .background(BAR_BG, Shape::Rectangle)
                .then(chrome),
        )
        .build(ctx, |ctx| {
            Text::new("Pinned app bar").font_size(14.0).build(ctx);
        });
}

#[composable]
fn pinned_bar_demo(ctx: &mut ComposeCtx) {
    let show_detail = ctx.remember(|| false);
    let in_overlay = ctx.remember(|| true);
    let chrome_on = ctx.remember(|| true);
    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        Stack::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                // Child 0: the screen. Children below could cover it.
                Column::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        if show_detail.get() {
                            detail_screen(ctx, &scope, &in_overlay);
                        } else {
                            list_screen(ctx, &scope, &in_overlay);
                        }
                    });
                // Child 1: the pinned bar (see `pinned_bar`).
                pinned_bar(ctx, &scope, &chrome_on);
                // Child 2: controls, offset to the bottom so they never move.
                Column::new()
                    .modifier(Modifier::new().fill_max_width().offset(0.0, CONTROLS_Y).padding(16.0))
                    .build(ctx, |ctx| {
                        let on = chrome_on.get();
                        Row::new()
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |ctx| {
                                Switch::new(on)
                                    .on_checked_change({
                                        clone!(chrome_on);
                                        move |v| chrome_on.set(v)
                                    })
                                    .build(ctx, |_| {});
                                Text::new(if on {
                                    "bar: renderInSharedTransitionScopeOverlay(1.0)"
                                } else {
                                    "bar: plain tree content — the flight covers it"
                                })
                                .font_size(14.0)
                                .build(ctx);
                            });
                        Button::new()
                            .on_click({
                                clone!(show_detail);
                                move || show_detail.set(!show_detail.get())
                            })
                            .modifier(Modifier::new().size(220.0, 36.0))
                            .build(ctx, |ctx| {
                                Text::new(if show_detail.get() { "Back to list" } else { "Fly into the bar" })
                                    .build(ctx);
                            });
                    });
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 560.0)
                .title("Pinned bar over a shared-element flight")
                .build(ctx, |ctx| {
                    pinned_bar_demo(ctx);
                });
        });
    });
}
