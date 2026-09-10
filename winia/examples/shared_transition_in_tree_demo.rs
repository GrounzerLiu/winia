//! Correct use of `render_in_overlay = false`: keep the flight IN the tree so
//! chrome painted later still covers it.
//!
//! The window is a Stack — the screen content first, then a **pinned app bar as
//! the last child**, so the bar is painted after the whole screen. The detail
//! hero lands with its upper band under that bar, and the flight climbs from
//! the bottom of the window, so the band crosses into the bar's strip near the
//! end of the flight:
//!
//! - `render_in_overlay = true` (Compose default): the entering hero becomes a
//!   layer root, so it paints OVER the pinned bar on the way in and then snaps
//!   back under it the moment the flight ends.
//! - `render_in_overlay = false`: the hero keeps its tree position and slides
//!   under the bar like ordinary content. That is what this flag is for.
//!
//! Nothing here clips, so this is the flag's intended use — unlike
//! `shared_transition_demo`, which puts a clipping container on the destination
//! and therefore needs the default.
//!
//! Run: `cargo run -p winia --example shared_transition_in_tree_demo`

use letclone::clone;
use winia::animation::SpringSpec;
use winia::prelude::*;

const APP_BAR_H: f32 = 90.0;
const CONTROLS_Y: f32 = 452.0;
const BAR_BG: Color = Color { r: 42, g: 46, b: 66, a: 255 };

/// One marked hero box. The overlay flag is read HERE (this composable's own
/// slot) so flipping the switch rewrites the arena marker immediately — a flag
/// read in an outer slot leaves this subtree skipped and the marker keeps its
/// old value.
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
            child_spacer(ctx, 10.0, 250.0);
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
            child_spacer(ctx, 10.0, 24.0);
            hero_box(ctx, 320.0, 170.0, Color::BLUE, Shape::Rectangle, scope, in_overlay);
        });
}

/// Plain spacer leaf (kept local so the two screens stay symmetrical).
fn child_spacer(ctx: &mut ComposeCtx, w: f32, h: f32) {
    let key = ctx.next_key();
    ctx.start_leaf(key, Modifier::new().size(w, h));
    ctx.end_node();
}

#[composable]
fn in_tree_demo(ctx: &mut ComposeCtx) {
    let show_detail = ctx.remember(|| false);
    let in_overlay = ctx.remember(|| false);
    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        Stack::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                // Child 0: the screen. Children below paint over it.
                Column::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        if show_detail.get() {
                            detail_screen(ctx, &scope, &in_overlay);
                        } else {
                            list_screen(ctx, &scope, &in_overlay);
                        }
                    });
                // Child 1: pinned app bar — painted AFTER the screen, so tree
                // order alone decides who is on top.
                Column::new()
                    .modifier(
                        Modifier::new()
                            .size(420.0, APP_BAR_H)
                            .background(BAR_BG, Shape::Rectangle),
                    )
                    .build(ctx, |ctx| {
                        Text::new("Pinned app bar (painted after the content)")
                            .font_size(14.0)
                            .build(ctx);
                    });
                // Child 2: the controls, offset to the bottom so they never
                // move with the screens.
                Column::new()
                    .modifier(Modifier::new().fill_max_width().offset(0.0, CONTROLS_Y).padding(16.0))
                    .build(ctx, |ctx| {
                        let overlay = in_overlay.get();
                        Row::new()
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |ctx| {
                                Switch::new(overlay)
                                    .on_checked_change({
                                        clone!(in_overlay);
                                        move |v| in_overlay.set(v)
                                    })
                                    .build(ctx, |_| {});
                                Text::new(if overlay {
                                    "render_in_overlay = true  (hero covers the bar)"
                                } else {
                                    "render_in_overlay = false (hero slides under it)"
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
                .title("In-tree flight (render_in_overlay = false)")
                .build(ctx, |ctx| {
                    in_tree_demo(ctx);
                });
        });
    });
}
