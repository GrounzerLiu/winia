//! Shared-element flight over a real photograph: the `ScaleToBounds` path, on screen.
//!
//! The other shared-element demos fly `shared_element` — the `Element` kind, which is
//! hard-wired to `RemeasureToBounds` — so `ResizeMode::scale_to_bounds()` and its
//! `ContentScale` were unreachable from a running app until this demo. Here
//! `assets/landscape.jpg` flies from a list card to a detail hero marked with
//! `shared_bounds`, and the buttons pick the knobs, so every combination can be A/B'd by
//! flying the same photo twice:
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

/// One marked photo box. EVERY parameter a State feeds is read HERE, inside the composable
/// that creates the marked node — a value read in an outer slot leaves this subtree skipped
/// (Skip reuses the cached modifier) and the marker silently keeps its old settings until
/// something else rebuilds the screen. Both other demos document the same trap.
#[composable]
fn photo_box(
    ctx: &mut ComposeCtx,
    w: f32,
    h: f32,
    scope: &SharedTransitionScope,
    content_scale: &State<ContentScale>,
    remeasure: &State<bool>,
    spill: &State<bool>,
) {
    let resize = if remeasure.get() {
        ResizeMode::RemeasureToBounds
    } else {
        ResizeMode::scale_to_bounds_with(content_scale.get(), ImageAlignment::Center)
    };
    let clip = if spill.get() { OverlayClip::None } else { OverlayClip::Bounds };
    Image::file(LANDSCAPE)
        .modifier(
            Modifier::new()
                .size(w, h)
                .clip(Shape::rounded(12.0))
                .shared_bounds_with_overlay_clip(
                    scope.shared_content_state("photo"),
                    VisibilityTransition::fade_in(TweenSpec::default()),
                    VisibilityTransition::fade_out(TweenSpec::default()),
                    BoundsTransform::spring(SpringSpec { stiffness: 120.0, ..SpringSpec::default() }),
                    resize,
                    // AnimatedSize reports the animated size to the parent, so the rows
                    // around the photo move with the flight; JumpCut would hold the
                    // target size instead.
                    PlaceHolderSize::AnimatedSize,
                    PathMotion::ArcBelow,
                    0.0,
                    true,
                    clip,
                ),
        )
        // The photo fills its own box the way Compose's `Image` would; the flight then
        // decides how THAT result is scaled into the animated bounds.
        .content_scale(ContentScale::Crop)
        .build(ctx);
}

#[composable]
fn controls(
    ctx: &mut ComposeCtx,
    content_scale: &State<ContentScale>,
    remeasure: &State<bool>,
    spill: &State<bool>,
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
    });
}

#[composable]
fn image_flight_demo(ctx: &mut ComposeCtx) {
    let show_detail = ctx.remember(|| false);
    let content_scale = ctx.remember(|| ContentScale::Crop);
    let remeasure = ctx.remember(|| false);
    let spill = ctx.remember(|| false);

    SharedTransitionLayout::new().build(ctx, |ctx| {
        let scope = current_shared_scope().expect("inside SharedTransitionLayout");
        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
                if show_detail.get() {
                    Text::new("Detail").font_size(22.0).build(ctx);
                    photo_box(ctx, 344.0, 240.0, &scope, &content_scale, &remeasure, &spill);
                    controls(ctx, &content_scale, &remeasure, &spill, &show_detail);
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
                    Text::new("List").font_size(22.0).build(ctx);
                    Text::new(
                        "Pick a mode, then tap the card: the same photo flies with that \
                         scale and you can compare runs.",
                    )
                    .font_size(12.0)
                    .build(ctx);
                    photo_box(ctx, 132.0, 92.0, &scope, &content_scale, &remeasure, &spill);
                    controls(ctx, &content_scale, &remeasure, &spill, &show_detail);
                    Button::new()
                        .on_click({
                            clone!(show_detail);
                            move || show_detail.set(true)
                        })
                        .modifier(Modifier::new().size(200.0, 36.0))
                        .build(ctx, |ctx| {
                            Text::new("Fly to detail").build(ctx);
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
