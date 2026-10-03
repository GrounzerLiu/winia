//! The shared-transition machinery the toolkit drives.
//!
//! A flight is stored on the runtime's composer (`shared_flights`), its visual is written onto the
//! `LayoutNode` by the layout pass and read by the renderer, and the Modifier chain carries its
//! configuration — so none of this can live in the component that composes it. The component keeps
//! the composables and the `Modifier` builders and imports what is below from here.

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{AnimatableValue, AnimationSpec, KeyframesSpec, SpringSpec, TweenSpec};
use crate::layout::node::{
    scroll_offset_for_node, FlightMeasure, FlightMeasureFrame, LayoutNode, PaintDisposition,
};
use crate::graphics::{ContentScale, ImageAlignment};
use crate::graphics::{Color, GraphicsLayerParams};
use crate::modifier::{Modifier, ModifierElement};
use crate::graphics::{Shape};
use crate::runtime::state::State;
use std::collections::HashMap;
use std::sync::Arc;

/// Flight rect in window-logical pixels: origin + size.
///
/// `lerp` deliberately does NOT clamp `t`: overshoot (`t > 1`, `t < 0`) flies
/// past the endpoint, which is how spring progress renders (Phase 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SharedBounds {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Component-wise interpolation (origin and size independently).
    pub fn lerp(&self, to: &Self, t: f32) -> Self {
        Self {
            x: self.x + (to.x - self.x) * t,
            y: self.y + (to.y - self.y) * t,
            width: self.width + (to.width - self.width) * t,
            height: self.height + (to.height - self.height) * t,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Rect interpolation spec (Compose `BoundsTransform`).
///
/// The spec animates scalar progress 0→1; the rect is derived per frame.
/// Spring overshoot is preserved end-to-end (engine → progress → `lerp`).
#[derive(Debug, Clone)]
pub struct BoundsTransform {
    pub spec: AnimationSpec,
}

impl BoundsTransform {
    pub fn tween(spec: TweenSpec) -> Self {
        Self { spec: spec.into() }
    }

    pub fn spring(spec: SpringSpec) -> Self {
        Self { spec: spec.into() }
    }

    pub fn keyframes(spec: KeyframesSpec) -> Self {
        Self { spec: AnimationSpec::Keyframes(spec) }
    }
}

/// Content deformation during flight (Compose `ResizeMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeMode {
    /// Scale the STABLE content into the animated bounds (Compose
    /// `ResizeMode.scaleToBounds`), fitted by `content_scale` and placed by
    /// `alignment`. Clipping is NOT optional: the render always clips a transitioning
    /// node to its morph-shaped lerped rect, so the pair can never spill (this variant
    /// used to carry a `clip` flag that measured as dead and was deleted).
    ScaleToBounds {
        content_scale: ContentScale,
        alignment: ImageAlignment,
    },
    /// Re-measure at the lerped size every frame (Phase 4).
    RemeasureToBounds,
}

impl ResizeMode {
    /// Compose `scaleToBounds()`: `ContentScale.FillWidth` + `Alignment.Center` — the
    /// uniform default, which is deliberately NOT `Image`'s `Fit`.
    pub fn scale_to_bounds() -> Self {
        ResizeMode::ScaleToBounds {
            content_scale: ContentScale::FillWidth,
            alignment: ImageAlignment::Center,
        }
    }

    /// Compose `scaleToBounds(contentScale, alignment)`.
    pub fn scale_to_bounds_with(
        content_scale: ContentScale,
        alignment: ImageAlignment,
    ) -> Self {
        ResizeMode::ScaleToBounds { content_scale, alignment }
    }

    /// `ContentScale`/`ImageAlignment` of a scaling end; `None` for `RemeasureToBounds`
    /// (it never scales).
    pub(crate) fn scale_to_bounds_parts(self) -> Option<(ContentScale, ImageAlignment)> {
        match self {
            ResizeMode::ScaleToBounds { content_scale, alignment } => Some((content_scale, alignment)),
            ResizeMode::RemeasureToBounds => None,
        }
    }
}

/// Compose `OverlayClip`: how content rendered in the transition layer is clipped.
/// Compose's default derives the clip from the parent `sharedBounds`, which winia
/// expresses as `Bounds`; `Rectangle` and `RoundedCorner` mirror Compose's members, and
/// `None` is a winia addition (Compose reaches "no clip" only through a custom
/// `OverlayClip` returning null) — it is the escape hatch for content that must be allowed
/// to overflow the animated bounds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum OverlayClip {
    /// Clip to the flight's own resolved corner quad on the lerped rect — the default,
    /// and what winia did unconditionally before this parameter existed.
    #[default]
    Bounds,
    /// Clip to the lerped rect with square corners (Compose `OverlayClip.Rectangle`).
    Rectangle,
    /// Clip to the lerped rect with this DEVICE-space corner radius, resolved against the
    /// lerped rect (Compose `OverlayClip.RoundedCorner`).
    RoundedCorner(f32),
    /// No clip: the content may paint outside the lerped rect.
    None,
}

/// Layout-space contract during flight (Compose `PlaceHolderSize` + explicit
/// cheap option).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceHolderSize {
    /// Layout snaps to end state immediately; the flying pair covers the pop.
    JumpCut,
    /// Source keeps its old layout space (free under Tier 0 retention).
    ContentSize,
    /// Container size follows progress (Phase 4, `ContentSizePolicy` pattern).
    AnimatedSize,
}

/// Position path during flight (Compose `ArcMode` equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMotion {
    Linear,
    ArcBelow,
    ArcAbove,
}

/// Marker kind stored in the modifier chain (Phase 2 reads it at `start_node`
/// to register the endpoint; render ignores it via the `_ =>` fallback).
#[derive(Debug, Clone, PartialEq)]
pub enum SharedKind {
    Element { placeholder: PlaceHolderSize },
    Bounds {
        resize: ResizeMode,
        placeholder: PlaceHolderSize,
        // Compose `overlayClip` is a marker parameter, so it travels with the kind.
        overlay_clip: OverlayClip,
    },
}

pub(crate) type FlightId = u64;

/// Owner of a flight-layout override: flight ids are per-composer, and a Tier-1
/// override lives on a PEER's node while its flight lives in the main map, so the
/// composer id has to travel with the id.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct FlightKey {
    pub cid: u64,
    pub id: FlightId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlightPhase {
    Idle,
    AwaitingBounds,
    Flying,
    /// Terminal-transient: dispatched cleanup, coordinator drops the flight.
    Finishing,
    /// Terminal: scope disposed or window closed mid-flight.
    Cancelled,
}

#[derive(Debug, Clone)]
pub(crate) enum FlightEvent {
    /// Matching pair complete, bounds pending.
    CounterpartAppeared { source_slot: u64, target_slot: u64 },
    /// Both bounds known (app-loop filled them).
    BoundsReady { start: SharedBounds, end: SharedBounds },
    /// Scalar progress reached 1.0.
    ProgressDone,
    // NOTE (dead-code allowed): Retarget/ScopeDisposed/WindowClosed are the
    // specified protocol (unit-tested in `on_event`) but the coordinator
    // shortcuts them today — retarget via cancel+reopen, disposal via the
    // dead-sweep/cancel paths. Route through events if those paths grow.
    #[allow(dead_code)]
    Retarget,
    #[allow(dead_code)]
    ScopeDisposed,
    #[allow(dead_code)]
    WindowClosed,
}

/// Actuator commands returned by the pure transition. Phase 2 interprets
/// `StartFlight`/`ReleaseRetained`/`FinishFlight`; Phase 4 adds tier
/// selection inside `StartFlight` handling.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlightAction {
    /// Entry of `AwaitingBounds`: ask the app loop to fill both bounds.
    CollectBounds { source_slot: u64, target_slot: u64 },
    /// Begin the dual render (same-composer Tier 0 in Phase 2; tier selection
    /// moves into the actuator in Phase 4).
    StartFlight { source_slot: u64, target_slot: u64, start: SharedBounds, end: SharedBounds },
    /// Free the retained source node (invisible at alpha 0 — glitch-free).
    ReleaseRetained { source_slot: u64 },
    /// Clear transition visuals, drop the flight.
    FinishFlight,
    /// Atomic cleanup after dispose/close.
    CancelFlight,
    Noop,
}

#[derive(Debug, Clone)]
pub(crate) struct Flight {
    // Self-identifying in debug output (map keys carry the live identity).
    #[allow(dead_code)]
    pub id: FlightId,
    pub scope_id: u64,
    pub key: String,
    pub phase: FlightPhase,
    pub start: Option<SharedBounds>,
    pub end: Option<SharedBounds>,
    /// Scalar progress mirror (the engine owns the `State<f32>` in Phase 2;
    /// the skeleton tracks the value for pure-logic tests).
    pub progress: f32,
    /// Bumped on every retarget; actuators ignore stale generations.
    pub generation: u64,
    pub source_slot: Option<u64>,
    pub target_slot: Option<u64>,
}

impl Flight {
    pub(crate) fn new(id: FlightId, scope_id: u64, key: String) -> Self {
        Self {
            id,
            scope_id,
            key,
            phase: FlightPhase::Idle,
            start: None,
            end: None,
            progress: 0.0,
            generation: 0,
            source_slot: None,
            target_slot: None,
        }
    }

    /// Pure state transition. Stray events are `Noop` (never panic — the
    /// coordinator may deliver late events from a previous generation).
    pub(crate) fn on_event(&mut self, ev: FlightEvent) -> Vec<FlightAction> {
        match (&self.phase, ev) {
            (FlightPhase::Idle, FlightEvent::CounterpartAppeared { source_slot, target_slot }) => {
                self.phase = FlightPhase::AwaitingBounds;
                self.source_slot = Some(source_slot);
                self.target_slot = Some(target_slot);
                vec![FlightAction::CollectBounds { source_slot, target_slot }]
            }
            (FlightPhase::AwaitingBounds, FlightEvent::BoundsReady { start, end }) => {
                self.phase = FlightPhase::Flying;
                self.start = Some(start);
                self.end = Some(end);
                self.progress = 0.0;
                vec![FlightAction::StartFlight {
                    source_slot: self.source_slot.unwrap_or(0),
                    target_slot: self.target_slot.unwrap_or(0),
                    start,
                    end,
                }]
            }
            // Retarget before launch: drop the pending pair; the coordinator
            // restarts matching from the current visual state.
            (FlightPhase::AwaitingBounds, FlightEvent::Retarget) => {
                self.phase = FlightPhase::Idle;
                self.source_slot = None;
                self.target_slot = None;
                vec![FlightAction::CancelFlight]
            }
            (FlightPhase::Flying, FlightEvent::ProgressDone) => {
                self.phase = FlightPhase::Finishing;
                self.progress = 1.0;
                vec![
                    FlightAction::ReleaseRetained { source_slot: self.source_slot.unwrap_or(0) },
                    FlightAction::FinishFlight,
                ]
            }
            // Mid-flight redirect: same slots, fresh generation, restart from
            // the current visual rect (actuator recomputes `start`).
            (FlightPhase::Flying, FlightEvent::Retarget) => {
                self.generation += 1;
                vec![FlightAction::StartFlight {
                    source_slot: self.source_slot.unwrap_or(0),
                    target_slot: self.target_slot.unwrap_or(0),
                    start: self.start.unwrap_or(SharedBounds::new(0.0, 0.0, 0.0, 0.0)),
                    end: self.end.unwrap_or(SharedBounds::new(0.0, 0.0, 0.0, 0.0)),
                }]
            }
            (phase, FlightEvent::ScopeDisposed | FlightEvent::WindowClosed)
                if matches!(phase, FlightPhase::AwaitingBounds | FlightPhase::Flying) =>
            {
                self.phase = FlightPhase::Cancelled;
                vec![FlightAction::CancelFlight]
            }
            (FlightPhase::Finishing, _) => {
                self.phase = FlightPhase::Idle;
                vec![FlightAction::Noop]
            }
            _ => vec![FlightAction::Noop],
        }
    }
}

/// Which end of the flight a node renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionRole {
    /// Leaving content: frozen detached node, fades 1→0 while morphing S→T.
    Source,
    /// Entering content: live in-tree node, fades 0→1 while morphing S→T.
    Target,
    /// Same-screen size morph (Phase 3 `animateBounds`): live in-tree node,
    /// opacity untouched, rect morphs old→new.
    Morph,
}

/// Per-frame render instruction, rewritten every frame by the coordinator
/// (Phase 2) from flight start/end + scalar progress. All fields are plain
/// data — render only `peek`s, never subscribes (zero-recomposition rule).
#[derive(Debug, Clone)]
pub(crate) struct TransitionVisual {
    pub start: SharedBounds,
    pub end: SharedBounds,
    /// Scalar progress snapshot (0→1, possibly overshooting under spring).
    pub progress: f32,
    pub role: TransitionRole,
    /// Opacity owned by a SCENE instead of by the flight (see [`Self::alpha`]): the visibility of the
    /// scene this end belongs to, for flights whose ends carry a scene id. `None` = the flight's own
    /// crossfade applies, which is right when no scene host is involved.
    pub scene_alpha: Option<f32>,
    /// Normalized corner radii [TL, TR, BR, BL] at both ends (see
    /// [`shared_shape_radii`]). Every `Shape` variant normalizes to this
    /// quad, so cross-kind morphs (pill→rect) stay continuous.
    pub radius_from: [f32; 4],
    pub radius_to: [f32; 4],
    /// Clip to the lerped bounds during flight (Bounds+clip only; Element
    /// content scales exactly into bounds — clipping would cut shadows).
    pub clip: bool,
    /// Hit-routing link (Phase 3): source visuals point at the target slot so
    /// clicks on the flying ghost descend into the live target subtree.
    /// Targets and morphs carry `None`.
    pub link_slot: Option<u64>,
    /// Ancestor scroll sum in this composer's canvas frame (frozen when the
    /// flight's ends resolve). Render's canvas carries `translate(-S)` from
    /// scrolled ancestors while `start`/`end` are scroll-corrected window
    /// coords — the flight transform adds S back so in-tree endpoints paint
    /// exactly on the lerped rect (hit test and clip already live in that
    /// frame). Detached sources render rootless, so theirs is always (0,0).
    pub scroll: (f32, f32),
    /// Owning flight id. Teardown clears a slot's visual only when the tag
    /// matches — slots are positional identities a successor flight can
    /// resurrect, and unconditional clearing would flicker one frame off the
    /// new flight's freshly written visual.
    pub flight: FlightId,
    /// Motion path (copied from the flight at write time).
    pub path: PathMotion,
    /// sharedBounds enter/exit pair (`None` for Element flights).
    pub bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
    /// Paint in the transition layer instead of in-tree (Compose
    /// `renderInOverlayDuringTransition`): `render_pass1` skips this subtree
    /// in the main walk and the coordinator re-renders it rootless from
    /// [`Composer::render_layer`], escaping ancestor clips and ancestor layer
    /// transforms. Detached sources are always elevated; targets follow their
    /// own marker. Elevated visuals freeze `scroll` at `(0,0)` — the layer
    /// canvas carries no ancestor translate, so the bounds are already the
    /// absolute ones.
    pub elevated: bool,
    /// Each END's corner kind (Compose percent vs fixed), captured at resolve
    /// from that end's own marker. Percent corners are resolved against the
    /// lerped rect before the two ends are mixed, so a `Circle` -> `Rectangle`
    /// flight still fades its corners out while staying aligned end to end.
    pub radius_from_auto: bool,
    pub radius_to_auto: bool,
    /// This end is being re-measured at the animated size (Compose
    /// `RemeasureToBounds`): the content already has the animated size, so
    /// render must NOT scale it again and hit testing maps 1:1.
    pub remeasure: bool,
    /// How a scaling end fits its STABLE content into the lerped rect and where the
    /// leftover axis sits (Compose `ContentScale` + `Alignment`). `None` for
    /// `RemeasureToBounds`, which never scales.
    pub scale_parts: Option<(ContentScale, ImageAlignment)>,
}

impl TransitionVisual {
    pub(crate) fn lerped(&self) -> SharedBounds {
        lerp_flight_rect(&self.start, &self.end, self.progress, self.path)
    }

    pub(crate) fn alpha(&self) -> f32 {
        // A SCENE owns opacity when the end belongs to one. During a nav transition the scenes
        // crossfade as layers, so the flight must not fade the same end a second time: its alpha is
        // the visibility of the scene the end came from (Compose's split — the scene transition owns
        // opacity, the shared bounds animation owns the rect). Only set for flights whose ends carry
        // a scene id; everything else keeps the crossfade below.
        if let Some(a) = self.scene_alpha {
            return a.clamp(0.0, 1.0);
        }
        match (&self.role, &self.bounds_fx) {
            // sharedBounds: fade channels claimed by enter/exit — fade
            // defaults reproduce the crossfade exactly; fade-less customs
            // ride opaque (Compose rule). Morphs never disappear.
            (TransitionRole::Target, Some((enter, _))) => {
                if enter.fade {
                    self.progress
                } else {
                    1.0
                }
            }
            (TransitionRole::Source, Some((_, exit))) => {
                if exit.fade {
                    1.0 - self.progress
                } else {
                    1.0
                }
            }
            _ => match self.role {
                TransitionRole::Source => 1.0 - self.progress,
                TransitionRole::Target => self.progress,
                // Same-screen morph never touches opacity.
                TransitionRole::Morph => 1.0,
            },
        }
        .clamp(0.0, 1.0)
    }

    pub(crate) fn radii(&self) -> [f32; 4] {
        // Unclamped like lerped(): spring overshoot carries corner radii past
        // the end value for one consistent flight-t (alpha stays clamped).
        // Floored at zero: shrinking radii would extrapolate negative under
        // overshoot, which Skia RRects reject.
        //
        // Compose interpolates RESOLVED corner sizes: a percent corner
        // (`Circle`/`Pill`) is resolved against the box being painted — the
        // lerped rect — before the two ends are mixed. Resolving it against the
        // endpoint's own (frozen) box would freeze it; keeping the resolved
        // percent as the end's value instead of mixing it would also diverge
        // from the other end (a `Circle` -> `Rectangle` flight must still fade
        // its corners out).
        let t = self.progress;
        let l = self.lerped();
        let percent = [l.width.min(l.height) / 2.0; 4];
        let from = if self.radius_from_auto { percent } else { self.radius_from };
        let to = if self.radius_to_auto { percent } else { self.radius_to };
        [0, 1, 2, 3].map(|i| (from[i] + (to[i] - from[i]) * t).max(0.0))
    }

    /// Clip rect at an explicit canvas offset — call BEFORE the flight
    /// canvas transform (clip is captured in the pre-transform space, like
    /// the GL clip rule).
    fn clip_rrect_at(&self, ox: f32, oy: f32) -> skia_safe::RRect {
        let l = self.lerped();
        let r = self.radii();
        skia_safe::RRect::new_rect_radii(
            skia_safe::Rect::new(l.x + ox, l.y + oy, l.x + ox + l.width, l.y + oy + l.height),
            &rrect_vectors([(r[0], r[0]), (r[1], r[1]), (r[2], r[2]), (r[3], r[3])]),
        )
    }

    /// Device-frame clip rect (zero offset — the frame hit test uses).
    #[allow(dead_code)]
    pub(crate) fn screen_rrect(&self) -> skia_safe::RRect {
        self.clip_rrect_at(0.0, 0.0)
    }

    /// Canvas-frame clip rect: `screen_rrect` translated by the frozen
    /// ancestor scroll sum. The clip call site sits INSIDE the ancestors'
    /// `translate(-S)`, so canvas coords must add S back to land on the
    /// lerped rect (same add-back as the flight translate below).
    pub(crate) fn canvas_rrect(&self) -> skia_safe::RRect {
        self.clip_rrect_at(self.scroll.0, self.scroll.1)
    }

    /// Hit-test remap (Phase 3): test the lerped visual rect; on hit, return
    /// the point remapped into the node's layout space for child descent
    /// (children keep layout positions — the flight transform is inverted
    /// here). `None` = visual miss → pass through (v1 skip behavior).
    /// `(nx, ny)` is the node's layout-space origin, `(w, h)` its layout size.
    pub(crate) fn remap_hit(
        &self,
        x: f32,
        y: f32,
        nx: f32,
        ny: f32,
        w: f32,
        h: f32,
    ) -> Option<(f32, f32)> {
        let l = self.lerped();
        if x < l.x || x > l.x + l.width || y < l.y || y > l.y + l.height {
            return None;
        }
        let (mut sx, mut sy) = self.paint_scale(w, h);
        if sx.abs() <= f32::EPSILON {
            sx = 1e-6;
        }
        if sy.abs() <= f32::EPSILON {
            sy = 1e-6;
        }
        // Mirrors the render transform exactly: scaled into the lerped rect from the
        // node origin, then placed by `paint_offset`.
        let (off_x, off_y) = self.paint_offset(w, h);
        // Clamp into layout bounds (float-safe: a visual hit must route
        // somewhere, never vanish at the edge).
        let lx = (nx + (x - l.x) / sx - off_x).clamp(nx.min(nx + w), nx.max(nx + w));
        let ly = (ny + (y - l.y) / sy - off_y).clamp(ny.min(ny + h), ny.max(ny + h));
        Some((lx, ly))
    }

    /// Layout-space radii pairs for bg/border/clip-element overrides (drawn
    /// The scale the render applies to this end's content box. `RemeasureToBounds`
    /// must NOT scale: the content was already laid out at the animated size, and that
    /// box trails `lerped()` by one poll (writers run after layout), so scaling by
    /// `l/box` stretches the freshly re-flowed content by the frame delta instead of
    /// leaving it re-laid-out.
    pub(crate) fn paint_scale(&self, box_w: f32, box_h: f32) -> (f32, f32) {
        if self.remeasure {
            return (1.0, 1.0);
        }
        let l = self.lerped();
        match self.scale_parts {
            Some((content_scale, _)) => {
                content_scale.scale_factors((box_w, box_h), (l.width, l.height))
            }
            // TEST-ONLY fallback: only the hand-built `TransitionVisual`s in the test
            // module leave this unset, and they want the pre-`ScaleToBounds` geometry,
            // i.e. `FillBounds`. Expressed through the same rule as everything else so
            // there is exactly one place that turns a mode into scales.
            None => ContentScale::FillBounds.scale_factors((box_w, box_h), (l.width, l.height)),
        }
    }

    /// NODE-space offset the render adds so the scaled content is placed by Compose
    /// `Alignment` (the render applies it inside `canvas.scale`, so it is the
    /// device-space leftover divided by the scale). Zero for `RemeasureToBounds`, for
    /// a mode that fills both axes, and for the historical top-left default.
    pub(crate) fn paint_offset(&self, box_w: f32, box_h: f32) -> (f32, f32) {
        if self.remeasure {
            return (0.0, 0.0);
        }
        let Some((_, alignment)) = self.scale_parts else {
            return (0.0, 0.0);
        };
        let (sx, sy) = self.paint_scale(box_w, box_h);
        let l = self.lerped();
        let (ox, oy) = ContentScale::align_offset(
            alignment,
            (box_w * sx, box_h * sy),
            (l.width, l.height),
            // The flight path does not carry a layout direction, so Start/End are
            // interpreted LTR here (Compose mirrors them with the environment).
            false,
        );
        (
            if sx.abs() > f32::EPSILON { ox / sx } else { 0.0 },
            if sy.abs() > f32::EPSILON { oy / sy } else { 0.0 },
        )
    }

    /// UNDER the flight transform — screen radii pre-divided by the PAINT scales).
    pub(crate) fn radii_pairs(&self, node_w: f32, node_h: f32) -> [(f32, f32); 4] {
        let l = self.lerped();
        // MUST be `paint_scale`, not the plain axis ratios: the render draws these
        // inside `canvas.scale(paint_scale)`, and under `ContentScale::FillWidth` the
        // two axes scale by the SAME factor while `l.h/node_h` is a different number.
        // Dividing by the axis ratios there painted an elliptical corner (measured
        // 8x24 device px for an intended 8x8 round corner on a 100x100 -> 300x100
        // flight, and an 8.7% vertical stretch in the live width-preserving case).
        let (mut sx, mut sy) = self.paint_scale(node_w, node_h);
        if sx.abs() < 1e-6 {
            sx = 1e-6;
        }
        if sy.abs() < 1e-6 {
            sy = 1e-6;
        }
        // Percent corners (Pill/Circle) are resolved inside `radii()` against
        // the lerped rect — each end first, then mixed by progress — so a plain
        // `radii()` is already correct here.
        // A DEVICE radius must not exceed half the smaller side of the rect it is
        // painted into, which a spring overshoot can drive past (a percent end
        // resolved against the animated box, or a fixed end extrapolating). Compose
        // scales such corners down proportionally in `createOutline`; Skia instead
        // silently clamps at construction, so without this the corner would jump
        // rather than follow the overshoot.
        let cap = l.width.min(l.height) / 2.0;
        let r = self.radii().map(|v| v.min(cap));
        [
            (r[0] / sx, r[0] / sy),
            (r[1] / sx, r[1] / sy),
            (r[2] / sx, r[2] / sy),
            (r[3] / sx, r[3] / sy),
        ]
    }
}

/// Flight rect at scalar progress: size lerps linearly, the rect center
/// follows the motion path (linear or Compose `ArcSpline.Arc`
/// quarter-ellipse, arc-length uniform). Single home for paint, clip, hit
/// and retarget continuity so an arc can never silently straighten on one
/// consumer.
pub(crate) fn lerp_flight_rect(
    start: &SharedBounds,
    end: &SharedBounds,
    progress: f32,
    path: PathMotion,
) -> SharedBounds {
    let t = progress;
    let w = start.width + (end.width - start.width) * t;
    let h = start.height + (end.height - start.height) * t;
    let (sx, sy) = (start.x + start.width / 2.0, start.y + start.height / 2.0);
    let (ex, ey) = (end.x + end.width / 2.0, end.y + end.height / 2.0);
    let (cx, cy) = match path {
        PathMotion::Linear => (sx + (ex - sx) * t, sy + (ey - sy) * t),
        PathMotion::ArcBelow => arc_center(sx, sy, ex, ey, t, true),
        PathMotion::ArcAbove => arc_center(sx, sy, ex, ey, t, false),
    };
    SharedBounds::new(cx - w / 2.0, cy - h / 2.0, w, h)
}

pub(crate) fn rrect_vectors(pairs: [(f32, f32); 4]) -> [skia_safe::Vector; 4] {
    [
        skia_safe::Vector::new(pairs[0].0, pairs[0].1),
        skia_safe::Vector::new(pairs[1].0, pairs[1].1),
        skia_safe::Vector::new(pairs[2].0, pairs[2].1),
        skia_safe::Vector::new(pairs[3].0, pairs[3].1),
    ]
}

/// Corner radii quad [TL, TR, BR, BL] resolved from the nearest
/// Background/Border/Clip shape — same precedence as the render focus ring.
/// Pill/Circle resolve against the given (flight-start) size. No shape at
/// all → plain rect.
/// True when the marker's nearest shape derives its corners from the box
/// (`Pill`/`Circle`): those must be evaluated on the painted box instead of
/// being lerped between the endpoint radii.
pub(crate) fn shared_shape_radius_is_auto(modifier: &Modifier) -> bool {
    modifier
        .elements()
        .iter()
        .rev()
        .find_map(|el| match el {
            ModifierElement::Background { shape, .. }
            | ModifierElement::Border { shape, .. }
            | ModifierElement::BorderDynamic { shape, .. }
            | ModifierElement::Clip { shape } => Some(shape),
            _ => None,
        })
        .is_some_and(|s| matches!(s, Shape::Pill | Shape::Circle))
}

pub(crate) fn shared_shape_radii(modifier: &Modifier, w: f32, h: f32) -> [f32; 4] {
    let shape = modifier.elements().iter().rev().find_map(|el| match el {
        ModifierElement::Background { shape, .. }
        | ModifierElement::Border { shape, .. }
        | ModifierElement::BorderDynamic { shape, .. }
        | ModifierElement::Clip { shape } => Some(shape),
        _ => None,
    });
    match shape {
        None => [0.0; 4],
        Some(Shape::Rectangle) => [0.0; 4],
        Some(Shape::RoundedRect { corner_radius }) => [*corner_radius; 4],
        Some(Shape::TopRoundedRect { radius }) => [*radius, *radius, 0.0, 0.0],
        // Corner order matches `RRect::new_rect_radii`: UL, UR, LR, LL.
        Some(Shape::RightRoundedRect { radius }) => [0.0, *radius, *radius, 0.0],
        Some(Shape::LeftRoundedRect { radius }) => [*radius, 0.0, 0.0, *radius],
        // Already per-corner: Compose's (top-start, top-end, bottom-end, bottom-start) in geometric
        // corners, which is the same order as `RRect::new_rect_radii`.
        Some(Shape::Corners { top_left, top_right, bottom_right, bottom_left }) => {
            [*top_left, *top_right, *bottom_right, *bottom_left]
        }
        Some(Shape::Pill) | Some(Shape::Circle) => {
            let m = w.min(h) / 2.0;
            [m; 4]
        }
    }
}

/// Cloned marker payload for coordinator use (built at most once per flight
/// end — the per-frame render path reads [`TransitionVisual`] instead).
#[derive(Debug, Clone)]
pub(crate) struct SharedMarker {
    pub scope_id: u64,
    pub key: String,
    pub kind: SharedKind,
    pub transform: BoundsTransform,
    pub path: PathMotion,
    pub z_index: f32,
    /// sharedBounds enter/exit (`None` on sharedElement markers).
    pub enter: Option<VisibilityTransition>,
    pub exit: Option<VisibilityTransition>,
    /// Compose `renderInOverlayDuringTransition` (default true).
    pub render_in_overlay: bool,
    /// Scene this end was composed inside, when a scene host published one (see
    /// [`with_nav_scene`]). Used to pair two LIVE ends of one key by "which scene is becoming
    /// visible" instead of by tree-walk order.
    pub scene: Option<u64>,
}

pub(crate) fn find_shared_marker(modifier: &Modifier) -> Option<SharedMarker> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::SharedTransition {
            scope_id, key, kind, transform, path, z_index, enter, exit, render_in_overlay, scene,
        } => Some(SharedMarker {
            scope_id: *scope_id,
            key: key.clone(),
            kind: kind.clone(),
            transform: transform.clone(),
            path: *path,
            z_index: *z_index,
            enter: enter.clone(),
            exit: exit.clone(),
            render_in_overlay: *render_in_overlay,
            scene: *scene,
        }),
        _ => None,
    })
}

/// Compose `Modifier.renderInSharedTransitionScopeOverlay` marker:
/// `(scope_id, z_index)` of a non-shared subtree that elevates while its scope
/// transitions.
pub(crate) fn find_scope_overlay_marker(modifier: &Modifier) -> Option<(u64, f32)> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::SharedScopeOverlay { scope_id, z_index } => Some((*scope_id, *z_index)),
        _ => None,
    })
}

/// `ResizeMode` declared by a marker. Compose resolves it per end; only the end
/// with live layout (the entering one) can honour it.
pub(crate) fn shared_resize_for_kind(kind: &SharedKind) -> ResizeMode {
    match kind {
        SharedKind::Bounds { resize, .. } => resize.clone(),
        // Compose's `sharedElement` has no resize parameter and always
        // re-measures: "During the bounds transform, sharedElement will
        // re-measure and relayout its child layout using fixed constraints
        // derived from its animated size, similar to RemeasureToBounds"
        // (SharedTransitionScope KDoc). Element content is identical on both
        // ends, so reflowing it is what keeps text and rows correct.
        SharedKind::Element { .. } => ResizeMode::RemeasureToBounds,
    }
}

/// `PlaceHolderSize` declared by a marker (Compose `PlaceholderSize`).
pub(crate) fn shared_placeholder_for_kind(kind: &SharedKind) -> PlaceHolderSize {
    match kind {
        SharedKind::Bounds { placeholder, .. } => *placeholder,
        SharedKind::Element { placeholder } => *placeholder,
    }
}

/// Clip flag for a flight end. Always `false` now: the render clips EVERY
/// transitioning node to its morph-shaped lerped rect unconditionally, so a marker
/// cannot opt into (or out of) clipping — the `ResizeMode::ScaleToBounds` flag that
/// used to feed this measured as having no effect and was deleted.
pub(crate) fn shared_clip_for_kind(_kind: &SharedKind) -> bool {
    false
}

/// Compose `overlayClip` of a marker kind. `Element` ends have no clip parameter, so
/// they keep the morph-shaped default.
pub(crate) fn shared_overlay_clip_for_kind(kind: &SharedKind) -> OverlayClip {
    match kind {
        SharedKind::Bounds { overlay_clip, .. } => *overlay_clip,
        SharedKind::Element { .. } => OverlayClip::Bounds,
    }
}

/// The `overlayClip` a node's own marker declares (Compose reads it per end, and each
/// end renders with its own). Falls back to the default when the node carries none.
pub(crate) fn overlay_clip_of(modifier: &crate::modifier::Modifier) -> OverlayClip {
    find_shared_marker(modifier)
        .map(|m| shared_overlay_clip_for_kind(&m.kind))
        .unwrap_or_default()
}

/// Live flight: pure machine + engine handles. Non-terminal flights are polled
/// every frame post-layout; render reads plain f32 snapshots (never subscribes).
#[derive(Debug)]
pub(crate) struct ActiveFlight {
    pub flight: Flight,
    /// Scalar progress (0→1) driven by `push_animatable` with the flight spec.
    pub progress: State<f32>,
    pub spec: AnimationSpec,
    /// Detached retained source arena index (frozen content).
    pub source_idx: Option<usize>,
    /// Exact start bounds (WINDOW coords — canonical flight frame; writer
    /// composers subtract their `screen_origin` when emitting visuals).
    pub start: SharedBounds,
    /// Frozen ancestor scroll sums (canvas frame) for each end, captured
    /// when the ends resolve. Source ends are detached (rootless) so
    /// `start_scroll` is always `(0,0)`; `end_scroll` is the live target's
    /// ancestor sum at resolve time. Freezing (not per-frame refresh) keeps
    /// the visual rigid under mid-flight scroll — it shifts exactly like
    /// normal content instead of pinning to the window.
    pub start_scroll: (f32, f32),
    pub end_scroll: (f32, f32),
    /// Motion path, resolved from the target marker when the end resolves
    /// (same rule as the animation spec — one flight, one path).
    pub path: PathMotion,
    /// sharedBounds enter/exit pair, resolved at end-resolve time (`None` for Element flights,
    /// which always crossfade in winia). Enter comes from the target marker, exit from the source
    /// marker — each side declares its own. DEVIATION from Compose: winia draws BOTH ends of an
    /// Element flight and crossfades them, while Compose's `sharedElement` installs no enter/exit
    /// and (per its KDoc) renders only the copy that is becoming visible — see the open deviation
    /// entry in `docs/shared-element-gaps.md`.
    pub bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
    pub radius_from: [f32; 4],
    pub radius_to: [f32; 4],
    /// Corner KIND of each end (Compose percent vs fixed), frozen at resolve —
    /// see `TransitionVisual::radii`.
    pub radius_from_auto: bool,
    pub radius_to_auto: bool,
    pub clip: bool,
    /// Target-end `renderInOverlayDuringTransition`, frozen when the end
    /// resolves (from the target marker; `true` when the marker is gone).
    /// The source end is always detached, so it is always in the layer.
    pub target_in_overlay: bool,
    /// `ResizeMode` / `PlaceHolderSize` from the target marker, frozen when the
    /// end resolves. Compose resolves both per end; only the entering end has
    /// live layout, so it is the end these can act on.
    pub resize: ResizeMode,
    pub placeholder: PlaceHolderSize,
    /// The target's natural (resting) size in its own layout, captured the
    /// frame the end resolves — the `ContentSize` answer, and the size the
    /// parent keeps seeing while the flight runs.
    pub target_size: Option<crate::layout::node::Size>,
    /// Per-frame layout override handed to the target node.
    pub measure: State<FlightMeasureFrame>,
    /// Endpoint owner composers (Phase 4 Tier1). Equal ⟺ Tier0, driven by the
    /// owner poll; differing ⟺ Tier1, driven by cross-poll. The flight lives
    /// in the MAIN composer map (main outlives overlays).
    pub source_cid: u64,
    pub target_cid: u64,
}

/// Detached source awaiting a cross-composer counterpart (Phase 4 Tier1).
/// Stashed by the owner retain hook, consumed or freed by cross-poll in the
/// SAME frame — never survives to the next frame.
#[derive(Debug, Clone)]
pub(crate) struct PendingSource {
    pub scope_id: u64,
    pub key: String,
    pub old_slot: u64,
    pub src_idx: usize,
    /// Start bounds + radii in WINDOW coords.
    pub start: SharedBounds,
    pub radius_from: [f32; 4],
    /// Corner kind of the leaving end (Compose percent vs fixed).
    pub radius_from_auto: bool,
}

/// Exact last-frame absolute rect via upward parent walk. Valid only at
/// compose-tail time (pre-layout): every cached position is still the old one.
/// `id_to_idx` must cover the whole arena (reused ancestors keep their objects).
/// Shared with hit-routing (node.rs) — keep the math identical to
/// `node_abs_position` (app.rs descent).
pub(crate) fn abs_rect_upward(
    nodes: &[LayoutNode],
    id_to_idx: &HashMap<u64, usize>,
    idx: usize,
) -> SharedBounds {
    let n = &nodes[idx];
    // Content box: a ghost's fraction map must land on the box the target
    // actually paints (its content), not on the placeholder size the target's
    // parent was told.
    let (w, h) = {
        let cb = n.content_box();
        (cb.width, cb.height)
    };
    let (mut ax, mut ay) = (n.position.x, n.position.y);
    let mut cur = idx;
    // Ascent mirrors the descent in `node_abs_position` (app.rs): each level
    // adds its position minus its own scroll offset. Keep the two in sync.
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        let p = &nodes[pidx];
        let (dx, dy) = scroll_offset_for_node(p);
        ax += p.position.x - dx;
        ay += p.position.y - dy;
        cur = pidx;
    }
    SharedBounds::new(ax, ay, w, h)
}

pub(crate) fn find_idx_by_slot(nodes: &[LayoutNode], root: usize, slot: u64) -> Option<usize> {
    let mut stack = vec![root];
    while let Some(idx) = stack.pop() {
        if nodes[idx].slot_key == slot {
            return Some(idx);
        }
        stack.extend(nodes[idx].children.iter().copied());
    }
    None
}

pub(crate) fn arc_center(sx: f32, sy: f32, ex: f32, ey: f32, t: f32, below: bool) -> (f32, f32) {
    const EPS: f32 = 0.001;
    const LUT: usize = 101;
    const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;
    let (dx, dy) = (ex - sx, ey - sy);
    if dx.abs() < EPS || dy.abs() < EPS {
        return (sx + dx * t, sy + dy * t);
    }
    let is_vertical = if below { dy > 0.0 } else { dy < 0.0 };
    let vertical = if is_vertical { -1.0 } else { 1.0 };
    let (a, b) = (dx / vertical, dy / -vertical);
    let (cx, cy) = (
        if is_vertical { ex } else { sx },
        if is_vertical { sy } else { ey },
    );
    // Arc-length table over θ ∈ [0, π/2] (stack, no alloc).
    let mut lut = [0.0f32; LUT];
    let (mut qx, mut qy) = (cx, cy + b);
    for i in 1..LUT {
        let th = HALF_PI * i as f32 / (LUT - 1) as f32;
        let (rx, ry) = (cx + a * th.sin(), cy + b * th.cos());
        lut[i] = lut[i - 1] + (rx - qx).hypot(ry - qy);
        qx = rx;
        qy = ry;
    }
    let total = lut[LUT - 1];
    let percent = if is_vertical { 1.0 - t } else { t };
    let target = percent.clamp(0.0, 1.0) * total;
    // Invert: angle fraction whose cumulative length brackets the target.
    let k = lut.partition_point(|&l| l < target);
    let frac = if k == 0 {
        0.0
    } else if k >= LUT {
        1.0
    } else {
        let (l0, l1) = (lut[k - 1], lut[k]);
        let f = if l1 > l0 { (target - l0) / (l1 - l0) } else { 0.0 };
        ((k - 1) as f32 + f) / (LUT - 1) as f32
    };
    let ang = HALF_PI * frac;
    (cx + a * ang.sin(), cy + b * ang.cos())
}
