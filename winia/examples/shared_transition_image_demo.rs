//! Shared-element flight over a real photograph: the `ScaleToBounds` path, on screen.
//!
//! The other shared-element demos fly `shared_element` — the `Element` kind, which is
//! hard-wired to `RemeasureToBounds` — so `ResizeMode::scale_to_bounds()` and its
//! `ContentScale` were unreachable from a running app until this demo. Here
//! `assets/landscape.jpg` flies from a list card to a detail hero marked with
//! `shared_bounds`, and the buttons pick the knobs, so every combination can be A/B'd by
//! flying the same photo twice. The list card is a **centre-cropped circle** (a square box
//! clipped to `Shape::Circle`, with the photo drawn `Crop` + centre inside it) and the
//! detail is a rounded rectangle, so the flight also morphs the corner radius from Compose's
//! percent-50 down to a fixed 12px — the `CircleShape` semantics winia was aligned with.
//!
//! - `ContentScale`: Crop / Fit / FillWidth / FillBounds. With a photograph the difference
//!   is unmistakable — Crop fills the animated rect uniformly and clips, Fit letterboxes,
//!   FillWidth keeps the aspect ratio and overflows the short axis, FillBounds is the
//!   non-uniform stretch winia used to hard-code.
//! - resize mode: `scale_to_bounds` (content measured once, then scaled — Compose's advice
//!   for text and its default for `sharedBounds`) vs `RemeasureToBounds` (the content is
//!   measured again at the animated size every frame, so the photo's own layout responds).
//! - the marker's `overlayClip`: `Bounds` (the default: the flight is clipped to its own
//!   morphed corner quad) vs `None` (a winia addition: the scaled photo is allowed to spill
//!   past the animated bounds and lands by sliding back inside). Flip it DURING a flight to
//!   see the difference live.
//!
//! Run: `cargo run -p winia --example shared_transition_image_demo`

use letclone::clone;
use winia::animation::{SpringSpec, TweenSpec};
use winia::prelude::*;

const LANDSCAPE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/landscape.jpg");

/// Flight shaping. A spring's speed follows `sqrt(stiffness / mass)`, so the slow-motion
/// spring keeps the same motion character while stretching the flight — which is what makes
/// the scale/clip differences watchable. Measured: stiffness 120 -> ~1.0s, 13 -> ~2.3s
/// (same numbers as the other hero demo).
fn flight_spec(slow: bool) -> BoundsTransform {
    let stiffness = if slow { 13.0 } else { 120.0 };
    BoundsTransform::spring(SpringSpec { stiffness, ..SpringSpec::default() })
}

/// One marked photo box. EVERY parameter a State feeds is read HERE, inside the composable
/// that creates the marked node — a value read in an outer slot leaves this subtree skipped
/// (Skip reuses the cached modifier) and the marker silently keeps its old settings until
/// something else rebuilds the screen. Both other demos document the same trap.
#[composable]
fn photo_box(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    shape: Shape,
    scope: &SharedTransitionScope,
    content_scale: &State<ContentScale>,
    remeasure: &State<bool>,
    spill: &State<bool>,
    slow: &State<bool>,
) {
    let resize = if remeasure.get() {
        ResizeMode::RemeasureToBounds
    } else {
        ResizeMode::scale_to_bounds_with(content_scale.get(), ImageAlignment::Center)
    };
    let clip = if spill.get() { OverlayClip::None } else { OverlayClip::Bounds };
    let spec = flight_spec(slow.get());
    // The CLIP and the marker live on a container, and the photo sits inside it. `Image` is a
    // leaf with its own draw node, so a clip in ITS modifier chain does not wrap that draw —
    // the container's does, which is the same structure the other hero demo uses.
    Column::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                // The flight reads the corner kind from the nearest Clip/Background/Border
                // shape: a `Circle` end resolves to Compose's percent-50 corner (min/2 against
                // the box it is drawn in), so a square box is a circle and the flight lerps
                // that radius into the target's fixed 12px on the same rect.
                .clip(shape)
                .shared_bounds_with_overlay_clip(
                    scope.shared_content_state("photo"),
                    VisibilityTransition::fade_in(TweenSpec::default()),
                    VisibilityTransition::fade_out(TweenSpec::default()),
                    spec,
                    resize,
                    // AnimatedSize reports the animated size to the parent, so the rows
                    // around the photo follow the flight. That policy used to expose a
                    // first-frame dip — the layout override is attached by the post-layout
                    // poll, so the switch frame's layout reported the hero's NATURAL 240px to
                    // the parent and the rows snapped back up before following the flight.
                    // The app loop now re-lays-out once when a flight attaches its override
                    // (winia's stand-in for Compose's lookahead), so this is safe here.
                    PlaceHolderSize::AnimatedSize,
                    PathMotion::ArcBelow,
                    0.0,
                    true,
                    clip,
                ),
        )
        .build(ctx, |ctx| {
            // The photo fills its box the way Compose's `Image` would (Crop + centre, so a
            // square box centre-crops); the flight then decides how THAT result is scaled into
            // the animated bounds.
            Image::file(LANDSCAPE)
                .modifier(Modifier::new().fill_max_size())
                .content_scale(ContentScale::Crop)
                .build(ctx);
        });
}

#[composable]
fn controls(
    ctx: &mut ComposeCtx,
    content_scale: &State<ContentScale>,
    remeasure: &State<bool>,
    spill: &State<bool>,
    slow: &State<bool>,
    show_detail: &State<bool>,
) {
    Row::new().modifier(Modifier::new().fill_max_width()).spacing(8.0).build(ctx, |ctx| {
        let modes = [
            ("Crop", ContentScale::Crop),
            ("Fit", ContentScale::Fit),
            ("FillWidth", ContentScale::FillWidth),
            ("FillBounds", ContentScale::FillBounds),
        ];
        for (label, mode) in modes {
            // Loops that emit more than one node need a key per iteration: the slot table
            // derives a call-site position from the loop body, and without `ctx.key` the
            // framework can hand the same slot to two nodes when something else about the
            // composition changes (here: flipping between the two screens) — which surfaced
            // as a `[dup-key]` panic on Back. `flow_demo` keys its loops for the same reason.
            ctx.key(label, |ctx| {
                Button::new()
                    .on_click({
                        clone!(content_scale);
                        clone!(remeasure);
                        move || {
                            content_scale.set(mode);
                            remeasure.set(false);
                        }
                    })
                    .modifier(Modifier::new().height(34.0))
                    .build(ctx, |ctx| {
                        Text::new(label).font_size(12.0).build(ctx);
                    });
            });
        }
        Button::new()
            .on_click({
                clone!(remeasure);
                clone!(show_detail);
                move || {
                    remeasure.set(true);
                    show_detail.set(true);
                }
            })
            .modifier(Modifier::new().height(34.0))
            .build(ctx, |ctx| {
                Text::new("Re-measure").font_size(12.0).build(ctx);
            });
    });
    Row::new().modifier(Modifier::new().fill_max_width()).spacing(8.0).build(ctx, |ctx| {
        let label = if spill.get() { "overlayClip: None (may spill)" } else { "overlayClip: Bounds" };
        Button::new()
            .on_click({
                clone!(spill);
                move || spill.set(!spill.get())
            })
            .modifier(Modifier::new().height(34.0))
            .build(ctx, |ctx| {
                Text::new(label).font_size(12.0).build(ctx);
            });
        // Slow motion stays flippable DURING a flight, like the clip toggle: it applies to
        // the next flight, so a running one is unaffected.
        let slow_label = if slow.get() { "slow motion: ON" } else { "slow motion: off" };
        Button::new()
            .on_click({
                clone!(slow);
                move || slow.set(!slow.get())
            })
            .modifier(Modifier::new().height(34.0))
            .build(ctx, |ctx| {
                Text::new(slow_label).font_size(12.0).build(ctx);
            });
    });
}

#[composable]
fn image_flight_demo(ctx: &mut ComposeCtx) {
    let show_detail = ctx.remember(|| false);
    let content_scale = ctx.remember(|| ContentScale::Crop);
    let remeasure = ctx.remember(|| false);
    let spill = ctx.remember(|| false);
    let slow = ctx.remember(|| false);

    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
                // Each branch gets its OWN key. The two screens have different child counts
                // and different nodes at the same call-site positions, and winia's slot keys
                // are derived from that position: without a key at this structural change the
                // branch flip can hand one node's key to another, and the framework panics
                // with `[dup-key]` (reproduced here on Back: a 388px-wide node — a controls
                // row or the hint — collided with itself at the same position). Compose keys
                // conditional groups the same way; winia requires it explicitly, which its
                // own panic message says.
                if show_detail.get() {
                    ctx.key("detail", |ctx| {
                        Text::new("Detail").font_size(22.0).build(ctx);
                        photo_box(ctx, 344.0, 240.0, Shape::rounded(12.0), &scope, &content_scale, &remeasure, &spill, &slow);
                        controls(ctx, &content_scale, &remeasure, &spill, &slow, &show_detail);
                        Button::new()
                            .on_click({
                                clone!(show_detail);
                                move || show_detail.set(false)
                            })
                            .modifier(Modifier::new().size(200.0, 36.0))
                            .build(ctx, |ctx| {
                                Text::new("Back").build(ctx);
                            });
                    });
                } else {
                    ctx.key("list", |ctx| {
                        Text::new("List").font_size(22.0).build(ctx);
                        // A SQUARE box clipped to a circle, with the photo itself drawn Crop +
                        // Center inside it: that is a centre-cropped circle (Compose's
                        // `Image(contentScale = Crop, alignment = Center)` inside a circular
                        // clip), and the flight then morphs its 50%-radius into the detail's
                        // rounded rectangle.
                        photo_box(ctx, 96.0, 96.0, Shape::Circle, &scope, &content_scale, &remeasure, &spill, &slow);
                        controls(ctx, &content_scale, &remeasure, &spill, &slow, &show_detail);
                        Button::new()
                            .on_click({
                                clone!(show_detail);
                                move || show_detail.set(true)
                            })
                            .modifier(Modifier::new().size(200.0, 36.0))
                            .build(ctx, |ctx| {
                                Text::new("Fly to detail").build(ctx);
                            });
                        // The hint goes LAST, below everything the flight moves: putting it
                        // above the card would make the two screens differ in height above the
                        // hero, so the rows below would shift the moment the switch happened.
                        Text::new(
                            "Pick a mode, then tap Fly to detail: the same photo flies with \
                             that scale and you can compare runs. Re-measure re-lays the photo \
                             out at the animated size every frame instead of scaling it.",
                        )
                        .font_size(12.0)
                        .build(ctx);
                    });
                }
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 620.0)
                .title("Shared Transition — Image")
                .build(ctx, |ctx| {
                    image_flight_demo(ctx);
                });
        });
    });
}
