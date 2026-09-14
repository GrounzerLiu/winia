//! TEMPORARY probe (branch `exp/searchbar-shared-morph`, step 2 of the plan): can a SearchBar-shaped
//! expansion be driven by a shared-element flight across composers?
//!
//! It reproduces the two ends of the real SearchBar layout without touching `ui/search_bar.rs`:
//!   - the collapsed pill lives in the MAIN tree (240x56, `Shape::Pill`), marked `shared_bounds`;
//!   - the expanded panel is rendered by a `Dialog` (its own composer, an OVERLAY) and marks the SAME key,
//!     filling the window (`Shape::Rectangle`).
//! Two pill variants are selectable at compile time via `PILL_STAYS_COMPOSED`, because the probe's first
//! question is whether the collapsed end disappearing is what makes the cross-composer pair:
//!   - `true`  (the real SearchBar shape: the pill is always composed and merely covered)
//!   - `false` (the pill is removed while expanded, so its disappearance can stash a cross source)
//!
//! Run with `WINIA_ANIM_TRACE=tmp/searchbar_probe.ndjson` and read the trace, or drive it over the debug
//! server. This file is deleted before the branch is finished — it exists to produce evidence.

use letclone::clone;
use winia::prelude::*;
use winia::ui::overlay::{Dialog, OverlayAnimSpec};
use winia::ui::shared_transition::{current_shared_scope, OverlayClip, SharedTransitionLayout};

const KEY: &str = "searchbar";

/// What the collapsed pill does while expanded. The real SearchBar keeps it composed (it is merely
/// covered by the dialog), which is variant C: the pill keeps its slot so the layout does not move, but
/// it stops carrying the shared marker, so the key leaves the composer's live map and the cross-composer
/// pairing can stash it as a source.
#[derive(Clone, Copy, PartialEq)]
enum PillMode {
    /// Keep composing AND keep the shared marker (the shape measured as producing NO flight).
    Stay,
    /// Stop composing it entirely.
    Remove,
    /// Keep composing it, but drop the shared marker while expanded.
    Unmark,
}

const PILL_MODE: PillMode = PillMode::Unmark;

#[composable]
fn probe_ui(ctx: &mut ComposeCtx) {
    let open: State<bool> = ctx.remember(|| false);
    SharedTransitionLayout::new().build(ctx, |ctx| {
        // BOTH ends must be composed inside the scope's provides: the Dialog registers its body here and
        // replays the CompositionLocal snapshot later, so a panel opened from outside this closure would
        // find no scope at all (measured while writing this probe).
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Spacer::vertical(40.0).build(ctx);
                collapsed_pill(ctx, open.clone());
                if open.get() {
                    expanded_panel(ctx);
                }
            });
    });
}

#[composable]
fn collapsed_pill(ctx: &mut ComposeCtx, open: State<bool>) {
    if open.get() && PILL_MODE == PillMode::Remove {
        return;
    }
    let scope = current_shared_scope().expect("scope");
    let st = open.clone();
    let marked = !(open.get() && PILL_MODE == PillMode::Unmark);
    let mut modifier = Modifier::new().width(240.0).height(56.0).clip(Shape::Pill);
    if marked {
        modifier = modifier.shared_bounds(
            scope.shared_content_state(KEY),
            VisibilityTransition::empty(),
            VisibilityTransition::empty(),
            BoundsTransform::default(),
            ResizeMode::scale_to_bounds(),
            PlaceHolderSize::AnimatedSize,
            PathMotion::Linear,
            0.0,
            true,
        );
    }
    Column::new()
        .modifier(modifier)
        .build(ctx, |ctx| {
            winia::ui::surface::Surface::new()
                .shape(Shape::Pill)
                .color(Color::from_argb(255, 220, 225, 235))
                .on_click(move || st.update(|v| *v = !*v))
                .modifier(Modifier::new().fill_max_size())
                .build(ctx, |ctx| {
                    Text::new("tap to expand").font_size(14.0).build(ctx);
                });
        });
}

#[composable]
fn expanded_panel(ctx: &mut ComposeCtx) {
    Dialog::new(true)
        .enter_animation(Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(
            200,
        ))))
        .exit_animation(Some(OverlayAnimSpec::fade_only(std::time::Duration::from_millis(
            200,
        ))))
        .build(ctx, |ctx| {
            // The dialog body runs in the overlay composer, which inherits the scope through the
            // CompositionLocal snapshot; re-reading it here also keeps this closure `Fn`.
            panel_body(ctx);
        });
}

#[composable]
fn panel_body(ctx: &mut ComposeCtx) {
    let scope = current_shared_scope().expect("scope");
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .clip(Shape::Rectangle)
                .shared_bounds_with_overlay_clip(
                    scope.shared_content_state(KEY),
                    VisibilityTransition::empty(),
                    VisibilityTransition::empty(),
                    BoundsTransform::default(),
                    ResizeMode::RemeasureToBounds,
                    PlaceHolderSize::JumpCut,
                    PathMotion::Linear,
                    0.0,
                    true,
                    OverlayClip::Bounds,
                ),
        )
        .build(ctx, |ctx| {
            winia::ui::surface::Surface::new()
                .shape(Shape::Rectangle)
                .color(Color::from_argb(255, 245, 247, 250))
                .modifier(Modifier::new().fill_max_size())
                .build(ctx, |ctx| {
                    Text::new("expanded panel").font_size(16.0).build(ctx);
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(720.0, 560.0)
                .title("SearchBar shared-morph probe")
                .build(ctx, probe_ui);
        });
    });
}
