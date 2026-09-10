//! Shared element transition hero demo: list ↔ detail morph.
//!
//! Deliberately large deltas on every axis so the flight is unmistakable:
//! list hero is a red circle at top-left (150x150, radius 75), detail hero is
//! a sharp blue rectangle lower-right (320x170, radius 0). Position, size,
//! aspect ratio AND corner radii all morph with a spring, crossfading 1→0 /
//! 0→1. Three launch buttons fly Linear / ArcBelow / ArcAbove for A/B
//! comparison (Back returns to the list).
//!
//! The detail hero also sits inside a CLIPPED box the size of its final rect,
//! which demonstrates the overlay pass (`renderInOverlayDuringTransition`):
//! in-tree painting would cut the whole flight to that box from frame one,
//! while the layer lets the hero fly free and slide back inside the clip as
//! it lands. Use the switch at the bottom to flip the pass on/off at runtime
//! and fly the same transition both ways — with the toggle off the entering
//! hero is cut off by a hard rectangular edge for most of the flight.
//! Run: `cargo run -p winia --example shared_transition_demo`

use letclone::clone;
use winia::animation::SpringSpec;
use winia::prelude::*;

/// Flight shaping. A spring's speed follows `sqrt(stiffness / mass)`, so the
/// slow-motion spring keeps the same motion character. Measured flight
/// duration: stiffness 120 → ~1.0s, stiffness 13 → ~2.3s (easier to watch the
/// clip edge with the overlay off).
fn hero_spec(slow: bool) -> BoundsTransform {
    let stiffness = if slow { 13.0 } else { 120.0 };
    BoundsTransform::spring(SpringSpec { stiffness, ..SpringSpec::default() })
}

/// One marked hero box. The overlay flag and the flight spec are read HERE —
/// inside the composable that creates the marked node — so flipping either
/// switch rewrites the arena marker immediately. A value read in an outer slot
/// leaves this subtree skipped (Skip keeps the cached modifier) and the marker
/// silently keeps its old value until something else rebuilds the screen.
#[composable]
fn hero_box(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    color: Color,
    shape: Shape,
    path: PathMotion,
    scope: &SharedTransitionScope,
    in_overlay: &State<bool>,
    slow: &State<bool>,
) {
    let overlay = in_overlay.get();
    let spec = hero_spec(slow.get());
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .background(color, shape)
                .shared_element(
                    scope.shared_content_state("hero"),
                    spec,
                    PlaceHolderSize::JumpCut,
                    path,
                    0.0,
                    overlay,
                ),
        )
        .build(ctx, |_| {});
}

#[composable]
fn hero_demo(ctx: &mut ComposeCtx) {
    let show_detail = ctx.remember(|| false);
    let fly_path = ctx.remember(|| PathMotion::ArcBelow);
    let in_overlay = ctx.remember(|| true);
    let slow = ctx.remember(|| false);
    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        let path = fly_path.get();
        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0))
            .build(ctx, |ctx| {
                if show_detail.get() {
                    Text::new("Detail").font_size(24.0).build(ctx);
                    // Spacer pushes the detail hero down so the flight has an
                    // obvious y delta on top of the size/aspect change.
                    Column::new()
                        .modifier(Modifier::new().size(10.0, 140.0))
                        .build(ctx, |_| {});
                    // Leading spacer pushes the hero right for an obvious x delta.
                    // Escape demo: the hero sits inside a CLIPPED box whose rect
                    // is the hero's final rect, so without the overlay pass the
                    // whole flight would be cut to that box from frame one (you
                    // would only ever see the landing rectangle). With the
                    // overlay pass the hero flies in free and slides back inside
                    // the clip exactly as it lands.
                    Row::new()
                        .modifier(Modifier::new().fill_max_width())
                        .build(ctx, |ctx| {
                            Column::new()
                                .modifier(Modifier::new().size(40.0, 10.0))
                                .build(ctx, |_| {});
                            Column::new()
                                .modifier(Modifier::new().clip(Shape::Rectangle))
                                .build(ctx, |ctx| {
                                    hero_box(
                                        ctx,
                                        320.0,
                                        170.0,
                                        Color::BLUE,
                                        Shape::Rectangle,
                                        path,
                                        &scope,
                                        &in_overlay,
                                        &slow,
                                    );
                                });
                        });
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            move || show_detail.set(false)
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Back").build(ctx);
                        });
                } else {
                    Text::new("List (pick a flight path)").font_size(24.0).build(ctx);
                    hero_box(ctx, 150.0, 150.0, Color::RED, Shape::Circle, path, &scope, &in_overlay, &slow);
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            clone!(fly_path);
                            move || {
                                fly_path.set(PathMotion::Linear);
                                show_detail.set(true)
                            }
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Fly linear").build(ctx);
                        });
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            clone!(fly_path);
                            move || {
                                fly_path.set(PathMotion::ArcBelow);
                                show_detail.set(true)
                            }
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Fly arc below").build(ctx);
                        });
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            clone!(fly_path);
                            move || {
                                fly_path.set(PathMotion::ArcAbove);
                                show_detail.set(true)
                            }
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Fly arc above").build(ctx);
                        });
                }
                // A/B controls. They sit OUTSIDE the if/else so they never
                // move; the values are picked up when the next flight resolves
                // — flip them, then launch a flight.
                Row::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        let overlay = in_overlay.get();
                        Switch::new(overlay)
                            .on_checked_change({
                                clone!(in_overlay);
                                move |v| in_overlay.set(v)
                            })
                            .build(ctx, |_| {});
                        Text::new(if overlay {
                            "render_in_overlay = true  (Compose default)"
                        } else {
                            "render_in_overlay = false (in-tree: gets clipped)"
                        })
                        .font_size(14.0)
                        .build(ctx);
                    });
                Row::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        let is_slow = slow.get();
                        Switch::new(is_slow)
                            .on_checked_change({
                                clone!(slow);
                                move |v| slow.set(v)
                            })
                            .build(ctx, |_| {});
                        Text::new(if is_slow {
                            "slow motion: spring 13, flight ~2.3s"
                        } else {
                            "normal speed: spring 120, flight ~1.0s"
                        })
                        .font_size(14.0)
                        .build(ctx);
                    });
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 560.0)
                .title("Shared Transition Hero")
                .build(ctx, |ctx| {
                    hero_demo(ctx);
                });
        });
    });
}
