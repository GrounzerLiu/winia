use super::*;

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

/// The four edges of a flight, in window-logical pixels — the value a flight's spec actually animates.
///
/// This is Compose's shape: `Rect.VectorConverter` hands the animation
/// `AnimationVector4D(left, top, right, bottom)` (`ui/unit/VectorConverters.kt` `RectToVector`), so
/// each edge runs its own spring with `Rect.VisibilityThreshold` — one PIXEL per edge
/// (`ui/unit/VisibilityThresholds.kt:39-42`) — and each edge inherits its own velocity when the flight
/// is retargeted mid-way. winia animated one scalar progress and derived the rect from it, which
/// behaved the same only while all edges moved together: with a scalar, a 300 px edge and a 3 px edge
/// settled at the same moment, and the flight's relative stop band (0.01 of the progress) let a
/// 1000 px flight stop 10 px short and then jump.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlightEdges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl FlightEdges {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    /// The edges of a rect (`right`/`bottom` are edge coordinates, like Compose's `Rect`).
    pub fn of(bounds: &SharedBounds) -> Self {
        Self::new(bounds.x, bounds.y, bounds.right(), bounds.bottom())
    }

    /// Back to the rect the flight paints, clipped to a non-negative size (an overshooting spring may
    /// briefly cross its own edges on a degenerate end).
    pub fn bounds(&self) -> SharedBounds {
        SharedBounds::new(
            self.left,
            self.top,
            (self.right - self.left).max(0.0),
            (self.bottom - self.top).max(0.0),
        )
    }

    /// Component-wise interpolation, unclamped so a Tween's overshoot or a spring's rebounds survive.
    pub fn lerp(&self, to: &Self, t: f32) -> Self {
        Self::new(
            self.left + (to.left - self.left) * t,
            self.top + (to.top - self.top) * t,
            self.right + (to.right - self.right) * t,
            self.bottom + (to.bottom - self.bottom) * t,
        )
    }

    /// How far `self` has travelled from `start` towards `end`, as a scalar — the value the flight's
    /// trace, its retarget continuity and the role crossfade read. Compose has no such scalar (its
    /// content transitions carry their own specs and its rect is four independent springs); winia
    /// derives one by averaging the axes that actually move, so an edge that never moves cannot drag
    /// the mix away from its partner.
    pub fn progress_towards(&self, start: &Self, end: &Self) -> f32 {
        fn axis(start: f32, end: f32, cur: f32) -> Option<f32> {
            let span = end - start;
            if span.abs() > 0.01 {
                Some((cur - start) / span)
            } else {
                None
            }
        }
        let horizontal =
            axis(start.left, end.left, self.left).or_else(|| axis(start.right, end.right, self.right));
        let vertical =
            axis(start.top, end.top, self.top).or_else(|| axis(start.bottom, end.bottom, self.bottom));
        match (horizontal, vertical) {
            (Some(h), Some(v)) => 0.5 * (h + v),
            (Some(h), None) => h,
            (None, Some(v)) => v,
            (None, None) => 1.0,
        }
    }
}

impl crate::animation::AnimatableValue for FlightEdges {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        FlightEdges::lerp(self, to, t)
    }

    /// The mean edge, for the scalar consumers that remain (spring de-duplication, the trace).
    fn to_f32(&self) -> f32 {
        (self.left + self.top + self.right + self.bottom) / 4.0
    }

    /// A degenerate one-edge value at that coordinate. Never reached through the flight paths — every
    /// one of them goes through [`Self::write_components`] — but the trait asks for the inverse.
    fn from_f32(v: f32) -> Self {
        Self::new(v, v, v, v)
    }

    fn supports_spring() -> bool {
        true
    }

    fn write_components(&self, out: &mut [f32; crate::animation::MAX_ANIMATION_COMPONENTS]) -> usize {
        out[0] = self.left;
        out[1] = self.top;
        out[2] = self.right;
        out[3] = self.bottom;
        4
    }

    fn from_components(components: &[f32]) -> Self {
        Self::new(components[0], components[1], components[2], components[3])
    }
}

///
/// The spec runs on the flight's four EDGES ([`FlightEdges`], window-logical pixels) — the same shape
/// Compose animates: its `BoundsTransform` returns a `FiniteAnimationSpec<Rect>` and the flight runs it
/// through `Rect.VectorConverter`, so the four edges are four springs, each with a one-pixel visibility
/// threshold, and each keeps its own velocity across a retarget. A `PathMotion::Arc*` flight bends the
/// rect's centre along the arc afterwards (a winia addition Compose has no counterpart for), which is
/// why the flown edges are post-processed rather than painted verbatim.
#[derive(Debug, Clone)]
pub struct BoundsTransform {
    pub spec: AnimationSpec,
}

impl BoundsTransform {
    /// Compose's `Rect.VisibilityThreshold`: one pixel per edge
    /// (`ui/unit/VisibilityThresholds.kt:39-42`). In pixels, because the spec animates the flight's
    /// edges (`FlightEdges`) — the same units every spring on a `Rect` uses in Compose.
    pub const VISIBILITY_THRESHOLD_PX: f32 = 1.0;

    pub fn tween(spec: TweenSpec) -> Self {
        Self { spec: spec.into() }
    }

    pub fn spring(spec: SpringSpec) -> Self {
        Self { spec: spec.into() }
    }

    pub fn keyframes(spec: KeyframesSpec) -> Self {
        Self { spec: AnimationSpec::Keyframes(spec) }
    }

    /// The spec for the flight's progress CLOCK — the scalar 0→1 the content crossfade rides.
    ///
    /// Its threshold is in progress units, not pixels: a one-pixel stop band is meaningless on a
    /// 0→1 ramp (it would mark the clock finished on its first frame, which is exactly what happened
    /// before this split). Compose runs its content enter/exit transitions on their own specs; winia
    /// approximates them with the flight's spec, so the clock keeps the old 0.01 while the GEOMETRY
    /// (`FlightEdges`) uses Compose's one pixel per edge.
    pub fn clock_spec(spec: &AnimationSpec) -> AnimationSpec {
        match spec {
            AnimationSpec::Spring(spring) => AnimationSpec::Spring(SpringSpec {
                threshold: 0.01,
                ..spring.clone()
            }),
            other => other.clone(),
        }
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

