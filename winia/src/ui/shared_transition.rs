//! Shared element transition — matching, flight engine and the transition
//! layer.
//!
//! Public API lives on [`SharedTransitionScope`] / [`SharedTransitionLayout`]
//! and the two marker methods (`Modifier::shared_element`,
//! `Modifier::shared_bounds`, plus `render_in_shared_transition_scope_overlay`
//! for chrome). The coordinator (the `impl Composer` blocks below) runs at the
//! compose tail, after layout, and once more per frame before render:
//! match endpoints, drive progress, write per-frame visuals, and decide which
//! nodes the layer paints. See `docs/shared-element-transition.md` for the
//! architecture and `docs/shared-element-usage.md` for the guide.
//!
//! Layer map (see the design doc):
//! - Matching: [`SharedTransitionScope`], [`SharedContentState`], live maps.
//! - Flight engine: [`Flight`] state machine — `on_event` is pure and tested.
//! - Content providers: Tier 0 (same composer) / Tier 1 (main ↔ overlay);
//!   Tier 2 bitmap deliberately unbuilt.
//!
//! Spring physics note: `BoundsTransform::spring` does NOT need a vector
//! spring. It springs scalar progress 0→1 (supported by the engine) and the
//! rect is derived per frame via [`SharedBounds::lerp`]; overshoot lerps past
//! the target, which is exactly the Compose spring look. Hence
//! `SharedBounds` keeps `supports_spring() == false` (Tween-exact) and spring
//! flights go through scalar progress (Phase 2 wiring).

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use crate::animation::{
    push_animatable, AnimatableValue, AnimationSpec, KeyframesSpec, SpringSpec, TweenSpec,
};
use crate::core::composer::Composer;
use crate::core::composer::ComposeCtx;
use crate::core::composition_local::CompositionLocal;
use crate::core::state::State;
use crate::layout::node::{scroll_offset_for_node, LayoutNode};
use crate::modifier::{Modifier, ModifierElement, Shape};
use crate::ui::animated_visibility::{
    ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
};

// ═══════════════════════════════════════════════════════════
// SharedBounds — exact animatable rect
// ═══════════════════════════════════════════════════════════

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

impl AnimatableValue for SharedBounds {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        SharedBounds::lerp(self, to, t.clamp(0.0, 1.0))
    }

    /// Scalar-engine placeholder (area). Only exercised by Spring/Decay
    /// paths, which never run for `SharedBounds`: `supports_spring()` is
    /// false so the engine degrades to Tween (pure `lerp`, see
    /// `push_animatable`). Spring flights go through scalar progress instead.
    fn to_f32(&self) -> f32 {
        self.width * self.height
    }

    /// Scalar-engine placeholder (square at origin). See `to_f32`.
    fn from_f32(v: f32) -> Self {
        let s = v.max(0.0).sqrt();
        Self { x: 0.0, y: 0.0, width: s, height: s }
    }
}

// ═══════════════════════════════════════════════════════════
// Flight shaping vocabulary (frozen API)
// ═══════════════════════════════════════════════════════════

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

impl Default for BoundsTransform {
    fn default() -> Self {
        Self { spec: TweenSpec::default().into() }
    }
}

/// Default values for the shared-transition API (Compose
/// `SharedTransitionDefaults`). User-facing entry point for "no opinion"
/// call sites — internal code keeps using the concrete defaults directly
/// so behavior never depends on this object drifting.
pub struct SharedTransitionDefaults;

impl SharedTransitionDefaults {
    /// Default flight shaping: 300ms linear tween (same as
    /// [`BoundsTransform::default`]).
    pub fn bounds_transform() -> BoundsTransform {
        BoundsTransform::default()
    }

    /// Default overlay z-order for flying pairs (Compose `zIndexInOverlay`).
    /// The transition layer sorts back-to-front by each root's marker z, and
    /// both ends of a flight are layer roots unless the end opted out.
    pub fn z_index_in_overlay() -> f32 {
        0.0
    }

    /// Whether flying pairs render in the transition layer by default
    /// (Compose `renderInOverlayDuringTransition`): an elevated end escapes
    /// ancestor clips and ancestor layer transforms and paints above
    /// non-shared content. The leaving end is always detached, so this governs
    /// the entering end in practice.
    pub fn render_in_overlay() -> bool {
        true
    }
}

/// Content deformation during flight (Compose `ResizeMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeMode {
    /// Scale the whole content to the lerped bounds (Phase 2 default).
    ScaleToBounds { clip: bool },
    /// Re-measure at the lerped size every frame (Phase 4).
    RemeasureToBounds,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedKind {
    Element { placeholder: PlaceHolderSize },
    Bounds { resize: ResizeMode, placeholder: PlaceHolderSize },
}

// ═══════════════════════════════════════════════════════════
// Matching layer: scope, content state, registry
// ═══════════════════════════════════════════════════════════

/// Pairing handle: which element flies with which (Compose
/// `SharedContentState`). Equality is (scope, key): same key in different
/// scopes never matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SharedContentState {
    scope_id: u64,
    key: String,
}

impl SharedContentState {
    pub fn scope_id(&self) -> u64 {
        self.scope_id
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

/// A key whose old endpoint vanished while a new one appeared in the same
/// frame — a switch candidate. Pure data; the coordinator (Phase 2) turns it
/// into a [`Flight`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SwitchCandidate {
    pub scope_id: u64,
    pub key: String,
    pub old_slot: u64,
    pub new_slot: u64,
}

/// Pure matching: `(scope, key) → slot` before vs now. A candidate is a key
/// present on both sides under *different* slots (disappeared + appeared).
/// Unrelated churn (same slot, unknown keys) yields nothing.
pub(crate) fn detect_switch(
    prev: &HashMap<(u64, String), u64>,
    live: &HashMap<(u64, String), u64>,
) -> Vec<SwitchCandidate> {
    let mut out = Vec::new();
    for (k, old_slot) in prev {
        if let Some(new_slot) = live.get(k) {
            if *new_slot != *old_slot {
                out.push(SwitchCandidate {
                    scope_id: k.0,
                    key: k.1.clone(),
                    old_slot: *old_slot,
                    new_slot: *new_slot,
                });
            }
        }
    }
    out.sort_by(|a, b| (a.scope_id, &a.key).cmp(&(b.scope_id, &b.key)));
    out
}

/// Transition scope handle (Compose `SharedTransitionScope`). Cheap to clone;
/// passed explicitly (no implicit receiver in Rust). Identity is `scope_id` —
/// cross-composer matching keys on it (overlay content inherits the scope
/// through the CompositionLocal snapshot, so no shared registry is needed).
#[derive(Debug, Clone)]
pub struct SharedTransitionScope {
    scope_id: u64,
}

impl SharedTransitionScope {
    pub(crate) fn new(scope_id: u64) -> Self {
        Self { scope_id }
    }

    /// Pairing handle for one shared element (Compose
    /// `rememberSharedContentState`). Stable for the scope's lifetime.
    pub fn shared_content_state(&self, key: impl Into<String>) -> SharedContentState {
        SharedContentState { scope_id: self.scope_id, key: key.into() }
    }

    /// Whether any flight in this scope is currently non-terminal (Compose
    /// `SharedTransitionScope.isTransitionActive`). Subscribable — reading
    /// it in composition re-runs on transitions (dimming, input gating,
    /// overlay-elevation patterns). Both tiers feed it: Tier 1 flights live
    /// in the main composer map under the same `scope_id`.
    pub fn is_transition_active(&self) -> State<bool> {
        SCOPE_ACTIVE
            .lock()
            .unwrap()
            .entry(self.scope_id)
            .or_insert_with(|| State::new(false))
            .clone()
    }

    pub fn scope_id(&self) -> u64 {
        self.scope_id
    }
}

static LOCAL_SHARED_SCOPE: LazyLock<CompositionLocal<Option<SharedTransitionScope>>> =
    LazyLock::new(|| CompositionLocal::new(|| None));

/// Per-scope transition activity (Compose `isTransitionActive`), keyed by
/// `scope_id` so main and overlay composers sharing a scope observe one
/// flag. Entries are created on first read and synced by the coordinator
/// polls — never removed (one small entry per scope call-site, bounded by
/// app structure).
static SCOPE_ACTIVE: LazyLock<Mutex<HashMap<u64, State<bool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Test isolation for [`SCOPE_ACTIVE`] (global, like the animation tables).
#[cfg(test)]
pub(crate) fn clear_scope_active_states() {
    SCOPE_ACTIVE.lock().unwrap().clear();
}

/// Innermost enclosing shared-transition scope, if any.
pub fn current_shared_scope() -> Option<SharedTransitionScope> {
    LOCAL_SHARED_SCOPE.try_current().flatten()
}

/// Outermost container providing the scope (Compose `SharedTransitionLayout`).
/// Children access it explicitly via the `scope` parameter or implicitly via
/// [`current_shared_scope`].
#[derive(Debug, Default)]
pub struct SharedTransitionLayout;

impl SharedTransitionLayout {
    pub fn new() -> Self {
        Self
    }

    /// Build the scoped subtree. `scope_id` is a stable per-call-site key, so
    /// the scope survives recomposition; `remember_at_key` additionally
    /// immunizes it against statement-order drift.
    ///
    /// NOTE: the content closure takes ONLY `ctx` (single param) — this is
    /// load-bearing, not style. The `#[composable]` macro only injects
    /// statement ids into closures whose sole parameter is the compose
    /// context; a `|ctx, scope|` closure is opaque to it, degrading every
    /// nested key to positional fallback (sibling branches collide → no
    /// switch flight, stale content). Read the scope inside via
    /// [`current_shared_scope`].
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let scope_id = ctx.next_key();
        let scope = ctx.remember_at_key(scope_id, || SharedTransitionScope::new(scope_id)).get();
        LOCAL_SHARED_SCOPE.provides(Some(scope), || content(ctx));
    }
}

// ═══════════════════════════════════════════════════════════
// Flight engine: pure state machine
// ═══════════════════════════════════════════════════════════

pub(crate) type FlightId = u64;

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

// ═══════════════════════════════════════════════════════════
// Modifier builders (frozen API)
// ═══════════════════════════════════════════════════════════

impl Modifier {
    /// Mark the shared element (same content on both ends — flies + crossfades).
    /// The transform rides scalar progress (see module docs); render ignores
    /// this marker until Phase 2 wires registration at `start_node`.
    /// `placeholder` selects the layout-space contract; only `JumpCut` is
    /// implemented today (other values degrade to it with a log).
    /// `path` selects the motion path (Compose `ArcMode` equivalent —
    /// Winia applies a flight-level quarter-ellipse port instead of
    /// Compose's per-keyframe `using ArcMode`).
    /// `z_index` orders flying pairs back-to-front in the transition layer
    /// (Compose `zIndexInOverlay`, default 0).
    /// `render_in_overlay` (Compose `renderInOverlayDuringTransition`,
    /// default `true`) paints this endpoint in the transition layer while it
    /// flies: it escapes ancestor clips and ancestor layer transforms (alpha,
    /// scale) and renders above non-shared content. `false` keeps in-tree
    /// painting, where ancestors still clip it. The leaving end is always
    /// detached, so it always renders in the layer (documented no-op there).
    pub fn shared_element(
        self,
        state: SharedContentState,
        transform: BoundsTransform,
        placeholder: PlaceHolderSize,
        path: PathMotion,
        z_index: f32,
        render_in_overlay: bool,
    ) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Element { placeholder },
            transform,
            path,
            z_index,
            enter: None,
            exit: None,
            render_in_overlay,
        })
    }

    /// Compose `Modifier.renderInSharedTransitionScopeOverlay(zIndexInOverlay)`:
    /// while this scope has a flight, render this **non-shared** subtree in the
    /// transition layer instead of the tree walk, so it keeps its spatial
    /// relationship with the flying shared elements (pinned app bars, FABs).
    /// Outside a flight the subtree is just ordinary tree content again — the
    /// elevation only lasts as long as the scope is transitioning.
    ///
    /// `z_index` is Compose's `zIndexInOverlay`: shared endpoints default to
    /// `0.0`, so a bar that must stay on top passes something larger. At equal
    /// z the chrome renders after the flights (i.e. above them).
    pub fn render_in_shared_transition_scope_overlay(
        self,
        scope: &SharedTransitionScope,
        z_index: f32,
    ) -> Self {
        self.push(ModifierElement::SharedScopeOverlay { scope_id: scope.scope_id, z_index })
    }

    /// Mark shared bounds (different content — container morphs + crossfades).
    /// `path` selects the motion path, same as in [`shared_element`](Self::shared_element).
    /// `z_index` orders flying pairs back-to-front, same as above.
    /// `render_in_overlay` is Compose `renderInOverlayDuringTransition`,
    /// same as above (default `true`).
    /// `enter` plays on the appearing end, `exit` on the disappearing end
    /// (Compose `enter`/`exit` — fade channels claimed by these replace the
    /// flight crossfade per-end; slide/scale/expand compose on top at flight
    /// progress; Morph role skips both).
    pub fn shared_bounds(
        self,
        state: SharedContentState,
        enter: VisibilityTransition,
        exit: VisibilityTransition,
        transform: BoundsTransform,
        resize: ResizeMode,
        placeholder: PlaceHolderSize,
        path: PathMotion,
        z_index: f32,
        render_in_overlay: bool,
    ) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Bounds { resize, placeholder },
            transform,
            path,
            z_index,
            enter: Some(enter),
            exit: Some(exit),
            render_in_overlay,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_lerp_endpoints_and_midpoint() {
        let a = SharedBounds::new(0.0, 10.0, 100.0, 50.0);
        let b = SharedBounds::new(200.0, 110.0, 300.0, 150.0);
        assert_eq!(a.lerp(&b, 0.0), a, "t=0 is the start rect");
        assert_eq!(a.lerp(&b, 1.0), b, "t=1 is the end rect");
        assert_eq!(
            a.lerp(&b, 0.5),
            SharedBounds::new(100.0, 60.0, 200.0, 100.0),
            "t=0.5 interpolates origin and size independently"
        );
    }

    #[test]
    fn bounds_lerp_overshoot_is_unclamped() {
        // Spring progress overshoots past 1.0 — the rect must follow (the
        // Compose spring look), not clamp.
        let a = SharedBounds::new(0.0, 0.0, 100.0, 100.0);
        let b = SharedBounds::new(100.0, 0.0, 100.0, 100.0);
        let past = a.lerp(&b, 1.25);
        assert_eq!(past.x, 125.0, "overshoot flies past the target");
    }

    #[test]
    fn bounds_animatable_trait_is_tween_exact() {
        let a = SharedBounds::new(0.0, 0.0, 10.0, 20.0);
        let b = SharedBounds::new(30.0, 40.0, 50.0, 60.0);
        // Trait entry clamps (engine contract); inherent `lerp` does not.
        assert_eq!(AnimatableValue::lerp(&a, &b, 0.5), SharedBounds::new(15.0, 20.0, 30.0, 40.0));
        assert!(!<SharedBounds as AnimatableValue>::supports_spring(), "vector spring unsupported — spring rides scalar progress");
        assert!(<SharedBounds as AnimatableValue>::same_target(&a, &a));
    }

    #[test]
    fn transform_constructors_preserve_spec() {
        let t = BoundsTransform::spring(SpringSpec::default());
        assert!(matches!(t.spec, AnimationSpec::Spring(_)));
        let t = BoundsTransform::default();
        assert!(matches!(t.spec, AnimationSpec::Tween(_)));
    }

    #[test]
    fn shared_transition_defaults_match_hardcoded_behavior() {
        let t = SharedTransitionDefaults::bounds_transform();
        match &t.spec {
            AnimationSpec::Tween(spec) => assert_eq!(
                spec.duration,
                std::time::Duration::from_millis(300),
                "default flight shaping is the 300ms tween"
            ),
            other => panic!("default bounds transform must be a tween, got {other:?}"),
        }
        assert_eq!(SharedTransitionDefaults::z_index_in_overlay(), 0.0);
        assert!(SharedTransitionDefaults::render_in_overlay());
    }

    #[test]
    fn shared_clip_for_kind_degrades_unimplemented_modes() {
        // Marker round-trip: shared_element carries the placeholder.
        let m = Modifier::new()
            .shared_element(
                SharedContentState { scope_id: 1, key: "k".to_string() },
                BoundsTransform::default(),
                PlaceHolderSize::AnimatedSize,
                PathMotion::Linear,
                0.0,
                true,
            );
        assert_eq!(
            find_shared_marker(&m).map(|x| x.kind),
            Some(SharedKind::Element { placeholder: PlaceHolderSize::AnimatedSize }),
            "element marker preserves the placeholder"
        );
        // Clip extraction with graceful degradation (logs, no behavior change).
        assert!(!shared_clip_for_kind(&SharedKind::Element {
            placeholder: PlaceHolderSize::JumpCut
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Element {
            placeholder: PlaceHolderSize::AnimatedSize
        }));
        assert!(shared_clip_for_kind(&SharedKind::Bounds {
            resize: ResizeMode::ScaleToBounds { clip: true },
            placeholder: PlaceHolderSize::JumpCut,
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Bounds {
            resize: ResizeMode::RemeasureToBounds,
            placeholder: PlaceHolderSize::ContentSize,
        }));
    }

    #[test]
    fn arc_lerped_endpoints_sides_and_degenerates() {
        // 2D travel: start (0,0,100,100) → end (200,100,100,100),
        // centers (50,50) → (250,150).
        let mk = |path, progress| TransitionVisual {
            start: SharedBounds::new(0.0, 0.0, 100.0, 100.0),
            end: SharedBounds::new(200.0, 100.0, 100.0, 100.0),
            progress,
            role: TransitionRole::Target,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path,
            bounds_fx: None,
            elevated: false,
        };
        // Linear midpoint is exactly the component-wise lerp.
        assert_eq!(
            mk(PathMotion::Linear, 0.5).lerped(),
            SharedBounds::new(100.0, 50.0, 100.0, 100.0)
        );
        // Arc midpoints bend off the straight center (150,100) to opposite
        // sides (quarter ellipse, arc-length parameterized — assert side +
        // magnitude, not LUT-exact digits).
        let dev_of = |path| {
            let l = mk(path, 0.5).lerped();
            (l.x + 50.0 - 150.0, l.y + 50.0 - 100.0)
        };
        let (bx, by) = dev_of(PathMotion::ArcBelow);
        let bmag = (bx * bx + by * by).sqrt();
        assert!(bmag > 20.0, "below-arc bends substantially, got ({bx}, {by})");
        let (ax, ay) = dev_of(PathMotion::ArcAbove);
        let amag = (ax * ax + ay * ay).sqrt();
        assert!(amag > 20.0, "above-arc bends substantially, got ({ax}, {ay})");
        assert!(
            bx * ax + by * ay < 0.0,
            "arcs bulge to opposite sides: below ({bx}, {by}) vs above ({ax}, {ay})"
        );
        // Arc endpoints are exact up to f32 trig at the table ends
        // (cos(π/2) ≈ -4e-8 — AOSP shares this; its tests use tolerance too).
        let close_bounds = |a: SharedBounds, b: SharedBounds| {
            (a.x - b.x).abs() < 1e-4
                && (a.y - b.y).abs() < 1e-4
                && (a.width - b.width).abs() < 1e-4
                && (a.height - b.height).abs() < 1e-4
        };
        assert!(
            close_bounds(
                mk(PathMotion::ArcBelow, 0.0).lerped(),
                SharedBounds::new(0.0, 0.0, 100.0, 100.0)
            ),
            "arc start endpoint"
        );
        assert!(
            close_bounds(
                mk(PathMotion::ArcBelow, 1.0).lerped(),
                SharedBounds::new(200.0, 100.0, 100.0, 100.0)
            ),
            "arc end endpoint"
        );
        assert!(
            close_bounds(
                mk(PathMotion::ArcAbove, 1.0).lerped(),
                SharedBounds::new(200.0, 100.0, 100.0, 100.0)
            ),
            "above-arc end endpoint"
        );
        // Compose degenerate rule: either-dimension travel falls back to
        // linear (axis-aligned flights stay straight — no sideways bulge).
        let flat = |sx: f32, sy: f32, ex: f32, ey: f32| {
            let v = TransitionVisual {
                start: SharedBounds::new(sx, sy, 100.0, 100.0),
                end: SharedBounds::new(ex, ey, 100.0, 100.0),
                ..mk(PathMotion::ArcBelow, 0.5)
            };
            v.lerped()
        };
        assert_eq!(flat(0.0, 0.0, 200.0, 0.0), SharedBounds::new(100.0, 0.0, 100.0, 100.0));
        assert_eq!(flat(0.0, 0.0, 0.0, 200.0), SharedBounds::new(0.0, 100.0, 100.0, 100.0));
        assert_eq!(flat(50.0, 50.0, 50.0, 50.0), SharedBounds::new(50.0, 50.0, 100.0, 100.0));
    }

    #[test]
    fn bounds_alpha_matrix_claims_fade_channels() {
        let mk = |role, fx: Option<(VisibilityTransition, VisibilityTransition)>| TransitionVisual {
            start: SharedBounds::new(0.0, 0.0, 100.0, 100.0),
            end: SharedBounds::new(200.0, 0.0, 100.0, 100.0),
            progress: 0.25,
            role,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path: PathMotion::Linear,
            bounds_fx: fx,
            elevated: false,
        };
        let fade_in = VisibilityTransition::fade_in(TweenSpec::default());
        let fade_out = VisibilityTransition::fade_out(TweenSpec::default());
        let empty = VisibilityTransition::empty();
        // Element: classic crossfade, always.
        assert_eq!(mk(TransitionRole::Target, None).alpha(), 0.25);
        assert_eq!(mk(TransitionRole::Source, None).alpha(), 0.75);
        // Bounds + fade enter/exit: identical numbers (fade defaults claim
        // the crossfade they already produce).
        assert_eq!(
            mk(TransitionRole::Target, Some((fade_in.clone(), fade_out.clone()))).alpha(),
            0.25
        );
        assert_eq!(
            mk(TransitionRole::Source, Some((fade_in.clone(), fade_out.clone()))).alpha(),
            0.75
        );
        // Bounds + fade-less customs ride opaque (Compose rule).
        assert_eq!(
            mk(TransitionRole::Target, Some((empty.clone(), fade_out.clone()))).alpha(),
            1.0
        );
        assert_eq!(
            mk(TransitionRole::Source, Some((fade_in.clone(), empty.clone()))).alpha(),
            1.0
        );
        // Morphs never touch opacity, pair or not.
        assert_eq!(mk(TransitionRole::Morph, None).alpha(), 1.0);
        assert_eq!(
            mk(TransitionRole::Morph, Some((fade_in.clone(), fade_out.clone()))).alpha(),
            1.0
        );
    }

    #[test]
    fn detect_switch_finds_pair_under_different_slots() {
        let prev: HashMap<(u64, String), u64> =
            [((7, "hero".to_string()), 100), ((7, "static".to_string()), 101)].into_iter().collect();
        let live: HashMap<(u64, String), u64> =
            [((7, "hero".to_string()), 200), ((7, "static".to_string()), 101)].into_iter().collect();
        let found = detect_switch(&prev, &live);
        assert_eq!(found.len(), 1, "only the re-slotted key is a switch");
        assert_eq!(found[0].scope_id, 7);
        assert_eq!(found[0].key, "hero");
        assert_eq!((found[0].old_slot, found[0].new_slot), (100, 200));
    }

    #[test]
    fn detect_switch_ignores_unrelated_churn() {
        let prev: HashMap<(u64, String), u64> = [((1, "a".to_string()), 10)].into_iter().collect();
        // Brand-new key (no old endpoint) and vanished key (no new endpoint).
        let live: HashMap<(u64, String), u64> = [((1, "b".to_string()), 20)].into_iter().collect();
        assert!(detect_switch(&prev, &live).is_empty(), "appear-without-disappear is not a switch");
        assert!(detect_switch(&prev, &prev).is_empty(), "identical frames never switch");
    }

    #[test]
    fn flight_full_lifecycle() {
        let mut f = Flight::new(1, 7, "hero".to_string());
        let acts = f.on_event(FlightEvent::CounterpartAppeared { source_slot: 100, target_slot: 200 });
        assert_eq!(f.phase, FlightPhase::AwaitingBounds);
        assert_eq!(acts, vec![FlightAction::CollectBounds { source_slot: 100, target_slot: 200 }]);

        let (s, e) = (SharedBounds::new(0.0, 0.0, 50.0, 50.0), SharedBounds::new(200.0, 0.0, 150.0, 150.0));
        let acts = f.on_event(FlightEvent::BoundsReady { start: s, end: e });
        assert_eq!(f.phase, FlightPhase::Flying);
        assert_eq!(acts, vec![FlightAction::StartFlight { source_slot: 100, target_slot: 200, start: s, end: e }]);

        let acts = f.on_event(FlightEvent::ProgressDone);
        assert_eq!(f.phase, FlightPhase::Finishing);
        assert_eq!(
            acts,
            vec![
                FlightAction::ReleaseRetained { source_slot: 100 },
                FlightAction::FinishFlight,
            ],
            "source freed at alpha 0 — glitch-free removal"
        );

        let acts = f.on_event(FlightEvent::Retarget);
        assert_eq!((f.phase, acts), (FlightPhase::Idle, vec![FlightAction::Noop]));
    }

    #[test]
    fn flight_retarget_bumps_generation() {
        let mut f = Flight::new(2, 7, "hero".to_string());
        f.on_event(FlightEvent::CounterpartAppeared { source_slot: 1, target_slot: 2 });
        f.on_event(FlightEvent::BoundsReady {
            start: SharedBounds::new(0.0, 0.0, 10.0, 10.0),
            end: SharedBounds::new(50.0, 0.0, 10.0, 10.0),
        });
        let prev_gen = f.generation;
        let acts = f.on_event(FlightEvent::Retarget);
        assert_eq!(f.generation, prev_gen + 1, "actuators ignore stale generations");
        assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]), "restart from current visual");
        assert_eq!(f.phase, FlightPhase::Flying, "still flying after redirect");
    }

    #[test]
    fn flight_cancel_on_dispose_and_close() {
        for ev in [FlightEvent::ScopeDisposed, FlightEvent::WindowClosed] {
            let mut f = Flight::new(3, 7, "k".to_string());
            f.on_event(FlightEvent::CounterpartAppeared { source_slot: 1, target_slot: 2 });
            let acts = f.on_event(ev);
            assert_eq!(f.phase, FlightPhase::Cancelled);
            assert_eq!(acts, vec![FlightAction::CancelFlight]);
            // Terminal: further events are harmless.
            let acts = f.on_event(FlightEvent::ProgressDone);
            assert_eq!(acts, vec![FlightAction::Noop]);
        }
    }

    #[test]
    fn flight_stray_events_are_noop() {
        let mut f = Flight::new(4, 7, "k".to_string());
        assert_eq!(f.on_event(FlightEvent::ProgressDone), vec![FlightAction::Noop]);
        assert_eq!(f.on_event(FlightEvent::BoundsReady {
            start: SharedBounds::new(0.0, 0.0, 1.0, 1.0),
            end: SharedBounds::new(0.0, 0.0, 1.0, 1.0),
        }), vec![FlightAction::Noop]);
        assert_eq!(f.phase, FlightPhase::Idle);
    }

    #[test]
    fn scope_state_key_isolation() {
        let a = SharedTransitionScope::new(1);
        let b = SharedTransitionScope::new(2);
        assert_ne!(
            a.shared_content_state("hero"),
            b.shared_content_state("hero"),
            "same key in different scopes must never match"
        );
        assert_eq!(a.shared_content_state("hero"), a.shared_content_state("hero"));
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 2: flight visuals (render-side data)
// ═══════════════════════════════════════════════════════════

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
}

impl TransitionVisual {
    pub(crate) fn lerped(&self) -> SharedBounds {
        lerp_flight_rect(&self.start, &self.end, self.progress, self.path)
    }

    pub(crate) fn alpha(&self) -> f32 {
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
        let t = self.progress;
        [0, 1, 2, 3].map(|i| {
            (self.radius_from[i] + (self.radius_to[i] - self.radius_from[i]) * t).max(0.0)
        })
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
        let sx = if w > 0.0 { l.width / w } else { 1.0 }.max(1e-6);
        let sy = if h > 0.0 { l.height / h } else { 1.0 }.max(1e-6);
        // Clamp into layout bounds (float-safe: a visual hit must route
        // somewhere, never vanish at the edge).
        let lx = (nx + (x - l.x) / sx).clamp(nx.min(nx + w), nx.max(nx + w));
        let ly = (ny + (y - l.y) / sy).clamp(ny.min(ny + h), ny.max(ny + h));
        Some((lx, ly))
    }

    /// Layout-space radii pairs for bg/border/clip-element overrides (drawn
    /// UNDER the flight transform — screen radii pre-divided by axis scales).
    pub(crate) fn radii_pairs(&self, node_w: f32, node_h: f32) -> [(f32, f32); 4] {
        let l = self.lerped();
        let sx = if node_w > 0.0 { l.width / node_w } else { 1.0 }.max(1e-6);
        let sy = if node_h > 0.0 { l.height / node_h } else { 1.0 }.max(1e-6);
        let r = self.radii();
        [
            (r[0] / sx, r[0] / sy),
            (r[1] / sx, r[1] / sy),
            (r[2] / sx, r[2] / sy),
            (r[3] / sx, r[3] / sy),
        ]
    }
}

/// Quarter-ellipse arc center (Compose `ArcSpline.Arc` math): `X = Cx +
/// A·sin θ`, `Y = Cy + B·cos θ`, θ swept 0→π/2 through an arc-length table
/// so travel is uniform along the curve. Orientation follows travel
/// direction + Above/Below exactly like AOSP (vertical time runs backward;
/// `Below→DownArc`, `Above→UpArc`). Either-dimension travel below epsilon
/// falls back to linear (Compose rule — axis-aligned flights stay straight).
/// Progress outside [0,1] (spring overshoot) pins the center at the nearer
/// endpoint — a deliberate deviation from AOSP extrapolation: Winia
/// overshoot rides scalar progress, and size/radii still overshoot visibly.
fn arc_center(sx: f32, sy: f32, ex: f32, ey: f32, t: f32, below: bool) -> (f32, f32) {
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
}

pub(crate) fn find_shared_marker(modifier: &Modifier) -> Option<SharedMarker> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::SharedTransition {
            scope_id, key, kind, transform, path, z_index, enter, exit, render_in_overlay,
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

/// Clip flag from the marker kind. Render implements ScaleToBounds only —
/// RemeasureToBounds degrades to scale (logged, once per flight start)
/// until per-frame remeasure lands. Non-`JumpCut` placeholders likewise
/// degrade to `JumpCut` (logged) until the layout-space contract lands.
pub(crate) fn shared_clip_for_kind(kind: &SharedKind) -> bool {
    if matches!(kind, SharedKind::Bounds { resize: ResizeMode::RemeasureToBounds, .. }) {
        crate::debug_log!("[shared] RemeasureToBounds unimplemented — degrading to scale");
    }
    let placeholder = match kind {
        SharedKind::Element { placeholder } => *placeholder,
        SharedKind::Bounds { placeholder, .. } => *placeholder,
    };
    if placeholder != PlaceHolderSize::JumpCut {
        crate::debug_log!("[shared] non-JumpCut placeholder unimplemented — degrading to JumpCut");
    }
    match kind {
        SharedKind::Bounds { resize: ResizeMode::ScaleToBounds { clip }, .. } => *clip,
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 2: Tier-0 coordinator (compose/layout/app-loop hooks)
// ═══════════════════════════════════════════════════════════

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
    /// sharedBounds enter/exit pair, resolved at end-resolve time (`None`
    /// for Element flights, which always crossfade). Enter comes from the
    /// target marker, exit from the source marker — each side declares its
    /// own, like Compose.
    pub bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
    pub radius_from: [f32; 4],
    pub radius_to: [f32; 4],
    pub clip: bool,
    /// Target-end `renderInOverlayDuringTransition`, frozen when the end
    /// resolves (from the target marker; `true` when the marker is gone).
    /// The source end is always detached, so it is always in the layer.
    pub target_in_overlay: bool,
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
    let (w, h) = (n.measured_size.width, n.measured_size.height);
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

/// Canvas scroll translation applied by a node's STRICT ancestors.
///
/// Mirrors the render scroll block (`render_pass1`): each scroll container
/// translates its children by `-offset` (`reverseLayout` mirrored). The
/// node's own offset is excluded (it only shifts descendants). Rootless
/// (detached retained sources) sum to `(0,0)`.
///
/// NOTE: this deliberately follows the render convention, not
/// `scroll_offset_for_node` (which leaves vertical-reverse unmirrored): the
/// sum is added back onto the render canvas, so it must equal what the
/// canvas carries. The pre-existing vertical-reverse hit/render divergence
/// is out of scope.
pub(crate) fn ancestor_scroll_sum(
    nodes: &[LayoutNode],
    id_to_idx: &HashMap<u64, usize>,
    idx: usize,
) -> (f32, f32) {
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut cur = idx;
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        let p = &nodes[pidx];
        for el in p.modifier.elements() {
            match el {
                ModifierElement::VerticalScroll { state } => {
                    let mut off = state.offset.get();
                    if p.scroll_reverse {
                        off = (p.scroll_content_height - p.scroll_viewport_height - off).max(0.0);
                    }
                    // Accumulate: scroll containers nest additively on the
                    // canvas (each ancestor translates its children).
                    sy += off;
                }
                ModifierElement::HorizontalScroll { state, .. } => {
                    let mut off = state.offset.get();
                    if p.scroll_reverse {
                        off = (p.scroll_content_width - p.scroll_viewport_width - off).max(0.0);
                    }
                    sx += off;
                }
                _ => {}
            }
        }
        cur = pidx;
    }
    (sx, sy)
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

fn is_terminal(phase: FlightPhase) -> bool {
    matches!(phase, FlightPhase::Finishing | FlightPhase::Cancelled)
}

/// Sync per-scope activity flags from flight maps (both tiers — Tier 1
/// flights live in the main map under the same `scope_id`). Called at the
/// end of every coordinator poll so the flag tracks opens and teardowns
/// within one frame. Only scopes somebody has read (map entries) are
/// touched — flights never create entries by themselves.
/// NOTE: deliberately a full sync (not true-only) per composer — headless
/// Tier 0 polls must clear flags alone, and the transient cross-poll flap
/// (overlay poll clears, union poll re-sets) is invisible: no render runs
/// between polls, and steady-state `set` dedups without notify.
pub(crate) fn sync_scope_active_states(all: &[&Composer]) {
    let mut active: HashSet<u64> = HashSet::new();
    for c in all {
        active.extend(
            c.shared_flights
                .values()
                .filter(|a| !is_terminal(a.flight.phase))
                .map(|a| a.flight.scope_id),
        );
    }
    let map = SCOPE_ACTIVE.lock().unwrap();
    for (id, state) in map.iter() {
        state.set(active.contains(id));
    }
}

/// Flight completion gate (MAJOR #2): while the animation engine still owns
/// the progress state the flight stays alive — even past `p >= 0.999`, whose
/// first passage precedes the overshoot peak under bouncy spring specs
/// (snapping there would swallow the overshoot). Once the engine releases
/// the state it has settled exactly at 1.0, so the threshold decides; the
/// same threshold covers manually-driven progress with no engine entry
/// (headless tests).
///
/// NOTE: flight specs must be finite (Tween/Spring/Keyframes settle and
/// release). An infinite Repeatable spec would hold the flight open forever
/// — the old engine-agnostic threshold completed those; the gate does not.
fn flight_progress_done(a: &ActiveFlight, p: f32) -> bool {
    if crate::animation::has_animation_for_state(a.progress.state_id()) {
        return false;
    }
    p >= 0.999
}

impl Composer {
    /// (scope, key) → slot for marked nodes in the CURRENT tree
    /// (post-materialize). First registration wins on duplicates (user error).
    pub(crate) fn shared_live_map(&self) -> HashMap<(u64, String), u64> {
        let mut out = HashMap::new();
        if let Some(root) = self.arena.root {
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                let node = &self.arena.nodes[idx];
                if let Some(m) = find_shared_marker(&node.modifier) {
                    if out.insert((m.scope_id, m.key.clone()), node.slot_key).is_some() {
                        crate::debug_log!("[shared] duplicate live endpoint scope={} key={}", m.scope_id, m.key);
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        out
    }

    /// Transition-layer render order (detached sources + elevated in-tree
    /// endpoints, z-sorted back-to-front). Painted after the main tree, hit
    /// tested before it — one list, so paint order and hit order cannot drift.
    pub(crate) fn transition_roots(&self) -> &[usize] {
        &self.layer_order
    }

    /// Mark (or clear) the non-shared subtrees that must render above the
    /// flights while their scope is transitioning — Compose
    /// `Modifier.renderInSharedTransitionScopeOverlay`.
    ///
    /// `active` is the set of scopes with a non-terminal flight, supplied by
    /// the caller: a composer's own poll passes its own flight scopes (exact
    /// for the main composer, which owns Tier 1 too), and the window-level
    /// cross-poll passes the union across all composers, because a peer
    /// composer cannot see a Tier1 flight (it lives in the main map). Reading
    /// the app-facing `is_transition_active()` state instead would tie
    /// elevation to whether *somebody happened to subscribe*.
    ///
    /// Idle frames (empty `active`) return before the arena walk.
    fn refresh_scope_overlay_roots(&mut self, active: &HashSet<u64>) {
        for &idx in &self.scope_overlay_roots {
            if let Some(n) = self.arena.nodes.get_mut(idx) {
                n.in_scope_overlay = false;
            }
        }
        self.scope_overlay_roots.clear();
        if active.is_empty() {
            return; // idle: no arena walk
        }
        let Some(root) = self.arena.root else { return };
        let mut marked: Vec<usize> = Vec::new();
        {
            let nodes = &self.arena.nodes;
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                let node = &nodes[idx];
                if let Some((sid, _)) = find_scope_overlay_marker(&node.modifier) {
                    if active.contains(&sid) {
                        marked.push(idx);
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        for &idx in &marked {
            self.arena.nodes[idx].in_scope_overlay = true;
        }
        self.scope_overlay_roots = marked;
    }

    /// Rebuild [`Self::transition_roots`] from live state: every elevated
    /// in-tree endpoint written this frame ([`Self::elevated_roots`]), every
    /// detached source ([`Self::transition_layer`]), and the chrome that opted
    /// into the scope overlay ([`Self::scope_overlay_roots`]).
    /// Stable z-sort (`zIndexInOverlay`) — equal z keeps that insertion order,
    /// i.e. the entering target is painted first (under its own ghost, exactly
    /// like the in-tree + ghost compositing the overlay pass replaces) and
    /// chrome last (a bar that opts in expects to sit on top). Flights that
    /// never touch `z_index` therefore render as they did before the pass
    /// existed. When one node carries both markers the SHARED marker's z wins.
    pub(crate) fn rebuild_layer_order(&mut self) {
        let mut order: Vec<usize> = self.elevated_roots.clone();
        order.extend(self.transition_layer.iter().copied());
        // Chrome last at equal z: a bar that opts into the overlay expects to
        // sit on top of the flying pair unless it asks for less.
        order.extend(self.scope_overlay_roots.iter().copied());
        // Dedup (a detached source can be reached from both lists) while
        // preserving first-seen order, then z-sort back-to-front.
        let mut seen = HashSet::new();
        order.retain(|idx| seen.insert(*idx));
        let z_of = |nodes: &[LayoutNode], idx: usize| -> f32 {
            let Some(n) = nodes.get(idx) else { return 0.0 };
            if let Some(m) = find_shared_marker(&n.modifier) {
                m.z_index
            } else {
                find_scope_overlay_marker(&n.modifier).map(|(_, z)| z).unwrap_or(0.0)
            }
        };
        order.sort_by(|&a, &b| {
            z_of(&self.arena.nodes, a)
                .partial_cmp(&z_of(&self.arena.nodes, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.layer_order = order;
    }

    /// Draw the transition layer on `canvas` (the same canvas frame the main
    /// tree was drawn in). Each root is re-rendered rootless at its absolute
    /// position, so ancestor clips and ancestor layer transforms — which are
    /// still on the real canvas for the tree — do not apply: the Compose
    /// overlay elevation. Flight transforms themselves come from
    /// `render_pass1`'s `TransitionVisual` block, unchanged.
    pub(crate) fn render_layer(&self, canvas: &skia_safe::Canvas) {
        if self.layer_order.is_empty() {
            return;
        }
        let Some(root) = self.arena.root else { return };
        let nodes = &self.arena.nodes;
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        for &idx in &self.layer_order {
            let Some(n) = nodes.get(idx) else { continue };
            if n.transition.is_none() && !n.in_scope_overlay {
                continue;
            }
            let abs = abs_rect_upward(nodes, &id_to_idx, idx);
            crate::render::render_node_at(nodes, root, idx, canvas, abs.x, abs.y);
        }
    }

    /// Compose-tail hook (MUST run after materialize, before the prev drain):
    /// detect switches, cancel dead/retargeted flights, retain+detach sources.
    pub(crate) fn retain_shared_sources(&mut self) {
        let live = self.shared_live_map();
        // Fresh appearances this frame (cross-composer Tier1 matching reads
        // these; overwritten every frame — peer prev maps absorb appearances
        // before cross-poll runs, so this is the only freshness source).
        self.fresh_shared = live
            .iter()
            .filter(|(k, _)| !self.prev_shared_endpoints.contains_key(k))
            .map(|(k, s)| ((k.0, k.1.clone()), *s))
            .collect();
        if live.is_empty() && self.prev_shared_endpoints.is_empty() && self.shared_flights.is_empty() {
            return; // fast path: no shared content anywhere
        }
        // Cancel flights whose key left the live tree with no counterpart
        // (screen torn down around a flight). Tier1 (cross-composer, main map)
        // is cross-owned — per-composer sweeps must not touch it.
        let live_keys: HashSet<(u64, String)> = live.keys().cloned().collect();
        let dead: Vec<FlightId> = self
            .shared_flights
            .iter()
            .filter(|(_, a)| {
                !is_terminal(a.flight.phase)
                    && a.source_cid == self.composer_id
                    && a.target_cid == self.composer_id
                    && !live_keys.contains(&(a.flight.scope_id, a.flight.key.clone()))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            self.cancel_flight(id);
        }
        // Switches (fresh + retarget-lite).
        for c in detect_switch(&self.prev_shared_endpoints, &live) {
            // Retarget-lite: same key already flying → cancel old (freeing its
            // retained node), restart from the current visual rect. Own-map
            // Tier0 only — Tier1 retargets resolve in cross-poll (which sees
            // both composers); touching them here would strand remote visuals.
            let mut start_override: Option<(SharedBounds, [f32; 4])> = None;
            if let Some(id) = self
                .shared_flights
                .iter()
                .find(|(_, a)| {
                    !is_terminal(a.flight.phase)
                        && a.source_cid == self.composer_id
                        && a.target_cid == self.composer_id
                        && a.flight.scope_id == c.scope_id
                        && a.flight.key == c.key
                })
                .map(|(id, _)| *id)
            {
                if let Some(a) = self.shared_flights.get(&id) {
                    // Unclamped + path-aware: retarget continuity follows the
                    // true visual rect, including spring overshoot past the
                    // old end and any arc bend. NOTE: enter/exit channel
                    // offsets (slide/scale) restart from full — a mid-flight
                    // redirect of a slide-bounds flight may snap by the live
                    // offset ( folding scale pivots into a rect is
                    // ill-defined; Element retargets stay seamless).
                    let p = a.progress.peek();
                    let end = a.flight.end.unwrap_or(a.start);
                    let s = lerp_flight_rect(&a.start, &end, p, a.path);
                    let (rf, rt) = (a.radius_from, a.radius_to);
                    start_override = Some((s, [0, 1, 2, 3].map(|i| rf[i] + (rt[i] - rf[i]) * p)));
                }
                self.cancel_flight(id);
            }
            self.begin_flight(c, start_override);
        }
        // Stash unmatched disappearances for cross-composer matching (Tier1).
        // Freed by cross-poll same frame when unmatched — invisible either way.
        // (Tier1-active keys are cross-owned; never re-stash them. Fresh Tier0
        // targets above are live, not disappeared.)
        let gone: Vec<((u64, String), u64)> = self
            .prev_shared_endpoints
            .iter()
            .filter(|(k, _)| !live.contains_key(k))
            .map(|(k, s)| ((k.0, k.1.clone()), *s))
            .filter(|(k, _)| {
                !self.shared_flights.values().any(|a| {
                    !is_terminal(a.flight.phase) && a.flight.scope_id == k.0 && a.flight.key == k.1
                })
            })
            .collect();
        for ((scope_id, key), old_slot) in gone {
            if let Some((src_idx, start, radius_from)) = self.detach_source(old_slot) {
                self.pending_cross.push(PendingSource {
                    scope_id,
                    key,
                    old_slot,
                    src_idx,
                    start,
                    radius_from,
                });
            }
        }
        self.prev_shared_endpoints = live;
    }

    /// Detach a vanished marked node (freeze): unlink from old parents, pin to
    /// absolute OWNER-canvas position, shelter from the drain. Returns arena
    /// index + exact start bounds (WINDOW coords) + start radii, or `None`
    /// when the node object was repurposed by the new tree (same-key reuse).
    fn detach_source(&mut self, old_slot: u64) -> Option<(usize, SharedBounds, [f32; 4])> {
        let src_idx = self.prev_node_by_key.remove(&old_slot)?;
        // Unlink from any still-referencing old parent (objects alive pre-drain;
        // reused ancestors already dropped the ref via children.clear()).
        for pidx in self.prev_node_by_key.values() {
            self.arena.nodes[*pidx].children.retain(|&x| x != src_idx);
        }
        // Belt-and-braces: the drain below must never reclaim it.
        self.reused_nodes.insert(src_idx);
        let id_to_idx: HashMap<u64, usize> =
            self.arena.nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut b = abs_rect_upward(&self.arena.nodes, &id_to_idx, src_idx);
        let (ox, oy) = self.screen_origin;
        b.x += ox;
        b.y += oy;
        let r = {
            let n = &self.arena.nodes[src_idx];
            shared_shape_radii(&n.modifier, n.measured_size.width, n.measured_size.height)
        };
        // Detach: absolute position IN THE OWNER'S CANVAS FRAME, rootless,
        // transition-layer owned.
        {
            let n = &mut self.arena.nodes[src_idx];
            n.position = crate::layout::node::Point::new(b.x - ox, b.y - oy);
            n.parent_id = None;
        }
        self.transition_layer.push(src_idx);
        self.sort_transition_layer_by_z();
        Some((src_idx, b, r))
    }

    /// Stable z-order for retained ghosts (Compose `zIndexInOverlay`):
    /// transition roots render back-to-front, so ascending z paints
    /// higher-z pairs on top. Stable — equal z keeps detach (push) order,
    /// which is exactly today's behavior when nobody sets z. Read lazily
    /// off each retained node's own marker (no flight plumbing needed);
    /// in-tree targets keep tree order (documented Tier 0 limitation).
    fn sort_transition_layer_by_z(&mut self) {
        let z_of = |nodes: &[LayoutNode], idx: usize| -> f32 {
            nodes
                .get(idx)
                .and_then(|n| find_shared_marker(&n.modifier))
                .map(|m| m.z_index)
                .unwrap_or(0.0)
        };
        self.transition_layer.sort_by(|&a, &b| {
            z_of(&self.arena.nodes, a)
                .partial_cmp(&z_of(&self.arena.nodes, b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Detach the source node (freeze) and open an AwaitingBounds flight.
    /// `start_override`: retarget visual continuity (WINDOW coords); otherwise
    /// the exact upward-walk bounds (no lookahead needed).
    fn begin_flight(&mut self, c: SwitchCandidate, start_override: Option<(SharedBounds, [f32; 4])>) {
        let (src_idx, walked, walked_r) = match self.detach_source(c.old_slot) {
            Some(v) => v,
            None => return,
        };
        let (start, radius_from) = start_override.unwrap_or((walked, walked_r));
        let id = self.next_flight_id;
        self.next_flight_id += 1;
        let mut flight = Flight::new(id, c.scope_id, c.key.clone());
        let acts = flight.on_event(FlightEvent::CounterpartAppeared {
            source_slot: c.old_slot,
            target_slot: c.new_slot,
        });
        debug_assert_eq!(
            acts,
            vec![FlightAction::CollectBounds { source_slot: c.old_slot, target_slot: c.new_slot }]
        );
        self.shared_flights.insert(
            id,
            ActiveFlight {
                flight,
                // Placeholder until StartFlight wires the real driven progress.
                progress: State::new(0.0f32),
                spec: BoundsTransform::default().spec,
                source_idx: Some(src_idx),
                start,
                // Detached sources render rootless — no ancestor scroll.
                start_scroll: (0.0, 0.0),
                // Filled when the end resolves (AwaitingBounds poll).
                end_scroll: (0.0, 0.0),
                // Ditto for the motion path (target marker wins).
                path: PathMotion::Linear,
                // Ditto for enter/exit (resolved below for Bounds flights).
                bounds_fx: None,
                radius_from,
                radius_to: radius_from,
                clip: false,
                // Filled when the end resolves (AwaitingBounds poll).
                target_in_overlay: true,
                source_cid: self.composer_id,
                target_cid: self.composer_id,
            },
        );
    }

    /// Free a retained source subtree + drop its slot (fires on_remove —
    /// removal semantic) + clear the target's visuals if still present.
    fn cancel_flight(&mut self, id: FlightId) {
        let Some(a) = self.shared_flights.remove(&id) else {
            return;
        };
        if let (Some(slot), Some(idx)) = (a.flight.source_slot, a.source_idx) {
            self.free_retained_source(idx, slot);
        }
        if let Some(slot) = a.flight.target_slot {
            self.clear_transition_for_slot(slot, id);
        }
    }

    fn free_retained_source(&mut self, idx: usize, slot: u64) {
        // Index + slot double-guarded: arena indices recycle, so the index
        // alone proves nothing.
        // NOTE: no slot-tree surgery here (see `cancel_flight`): keys are
        // positional identities resurrected on navigate-back; key-based
        // removal murders the live successor. Stale slots self-clean via
        // truncate/Enter-prune.
        if self.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
            let mut visited = HashSet::new();
            self.arena.free_node_skip(idx, &HashSet::new(), &mut visited);
        }
        self.transition_layer.retain(|&x| x != idx);
    }

    /// Clear a slot's visual only when it still belongs to `id`. Slots are
    /// positional identities a successor flight can resurrect between this
    /// flight's last write and its teardown — unconditional clearing would
    /// flicker one frame off the new visual (self-heals next poll, but
    /// avoidable for one tag comparison).
    fn clear_transition_for_slot(&mut self, slot: u64, id: FlightId) {
        if let Some(root) = self.arena.root {
            if let Some(idx) = find_idx_by_slot(&self.arena.nodes, root, slot) {
                let owned = self.arena.nodes[idx].transition.as_ref().is_some_and(|t| t.flight == id);
                if owned {
                    self.arena.nodes[idx].transition = None;
                }
            }
        }
    }

    /// Post-layout hook (app loop, every frame): fill ends, start flights,
    /// rewrite per-frame visual snapshots, reap completed flights. Render
    /// reads plain f32 snapshots — never subscribes (zero-recomposition).
    /// Post-layout hook (app loop, every frame): fill ends, start flights,
    /// rewrite per-frame visual snapshots, reap completed flights, detect
    /// same-screen size morphs. Render reads plain f32 snapshots — never
    /// subscribes (zero-recomposition).
    pub(crate) fn poll_shared_flights(&mut self) {
        // Layer membership is rebuilt from this frame's writes (Tier0 writes
        // here, Tier1 writes land in the same frame's cross-poll below).
        self.elevated_roots.clear();
        // Chrome elevation (Compose renderInSharedTransitionScopeOverlay) is a
        // per-frame decision, so it is recomputed here as well — from THIS
        // composer's flights; the cross-poll re-runs it with the window union.
        let own: HashSet<u64> = self
            .shared_flights
            .values()
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| a.flight.scope_id)
            .collect();
        self.refresh_scope_overlay_roots(&own);
        // Live marker map doubles for flight polling and morph detection —
        // one arena walk per frame.
        let live = self.shared_live_map();
        if !self.shared_flights.is_empty() {
            // Snapshot ids: actuators mutate the map.
            let ids: Vec<FlightId> = self.shared_flights.keys().copied().collect();
            for id in ids {
                self.poll_one_flight(id);
            }
        }
        self.poll_layout_morphs(&live);
        // Layer membership is derived from the visuals this poll just wrote.
        self.rebuild_layer_order();
        sync_scope_active_states(&[self]);
    }

    fn poll_one_flight(&mut self, id: FlightId) {
        // Tier1 flights live in the main map but span composers — driven by
        // cross-poll, never here.
        let mine = match self.shared_flights.get(&id) {
            Some(a) => a.source_cid == self.composer_id && a.target_cid == self.composer_id,
            None => return,
        };
        if !mine {
            return;
        }
        let phase = match self.shared_flights.get(&id) {
            Some(a) => a.flight.phase,
            None => return,
        };
        match phase {
            FlightPhase::AwaitingBounds => {
                // Resolve target in the CURRENT tree (layout rebuilt the map).
                let target_slot = self.shared_flights.get(&id).and_then(|a| a.flight.target_slot);
                let root = match self.arena.root {
                    Some(r) => r,
                    None => {
                        self.cancel_flight(id);
                        return;
                    }
                };
                let tidx = match target_slot.and_then(|s| find_idx_by_slot(&self.arena.nodes, root, s)) {
                    Some(i) => i,
                    None => {
                        self.cancel_flight(id);
                        return;
                    }
                };
                let tid = self.arena.nodes[tidx].id;
                let (tx, ty) = crate::app::node_abs_position(&self.arena.nodes, root, tid);
                let (tw, th) = {
                    let n = &self.arena.nodes[tidx];
                    (n.measured_size.width, n.measured_size.height)
                };
                // Canonicalize to window coords (overlay-local + screen origin).
                let (ox, oy) = self.screen_origin;
                let end = SharedBounds::new(tx + ox, ty + oy, tw, th);
                let marker = find_shared_marker(&self.arena.nodes[tidx].modifier);
                let (spec, clip, path, enter, target_is_bounds, target_in_overlay) = match marker {
                    Some(m) => (
                        m.transform.spec.clone(),
                        shared_clip_for_kind(&m.kind),
                        m.path,
                        m.enter,
                        matches!(m.kind, SharedKind::Bounds { .. }),
                        m.render_in_overlay,
                    ),
                    None => (
                        BoundsTransform::default().spec,
                        false,
                        PathMotion::Linear,
                        None,
                        false,
                        true,
                    ),
                };
                // Enter from the target marker, exit from the retained source
                // marker — each side declares its own (Compose rule). Only
                // Bounds flights carry the pair (Element always crossfades);
                // a missing source exit falls back to fade-out (today's look).
                let source_marker = self
                    .shared_flights
                    .get(&id)
                    .and_then(|a| a.source_idx)
                    .and_then(|sidx| self.arena.nodes.get(sidx))
                    .and_then(|n| find_shared_marker(&n.modifier));
                // Mixed-kind pairing (Element one side, Bounds the other)
                // resolves silently toward the target side — log it.
                if let Some(ref sm) = source_marker {
                    let source_is_bounds = matches!(sm.kind, SharedKind::Bounds { .. });
                    if source_is_bounds != target_is_bounds {
                        let (scope, key) = self
                            .shared_flights
                            .get(&id)
                            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
                            .unwrap_or((0, String::new()));
                        crate::debug_log!(
                            "[shared] mixed-kind pair scope={scope} key={key} — flight follows the target side"
                        );
                    }
                }
                let bounds_fx = match enter {
                    Some(enter) => {
                        let exit = source_marker
                            .and_then(|m| m.exit)
                            .unwrap_or_default();
                        Some((enter, exit))
                    }
                    None => None,
                };
                let radius_to = {
                    let n = &self.arena.nodes[tidx];
                    shared_shape_radii(&n.modifier, tw, th)
                };
                // Freeze the target's ancestor scroll sum (canvas frame) so
                // the flight transform can add it back at render (BLOCKER #1).
                let end_scroll = {
                    let id_to_idx: HashMap<u64, usize> = self
                        .arena
                        .nodes
                        .iter()
                        .enumerate()
                        .map(|(i, n)| (n.id, i))
                        .collect();
                    ancestor_scroll_sum(&self.arena.nodes, &id_to_idx, tidx)
                };
                let progress = State::new(0.0f32);
                push_animatable(progress.clone(), 1.0, spec.clone());
                let (start, acts) = {
                    let a = self.shared_flights.get_mut(&id).expect("polled flight exists");
                    a.spec = spec;
                    a.progress = progress;
                    a.radius_to = radius_to;
                    a.end_scroll = end_scroll;
                    a.clip = clip;
                    a.path = path;
                    a.bounds_fx = bounds_fx;
                    a.target_in_overlay = target_in_overlay;
                    let start = a.start;
                    let acts = a.flight.on_event(FlightEvent::BoundsReady { start, end });
                    (start, acts)
                };
                debug_assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]));
                let _ = start;
                self.write_flight_visuals(id);
            }
            FlightPhase::Flying => {
                let p = self.shared_flights.get(&id).map(|a| a.progress.peek()).unwrap_or(1.0);
                if let Some(a) = self.shared_flights.get_mut(&id) {
                    a.flight.progress = p;
                }
                self.write_flight_visuals(id);
                let done = self.shared_flights.get(&id).is_some_and(|a| flight_progress_done(a, p));
                if done {
                    let acts = self
                        .shared_flights
                        .get_mut(&id)
                        .map(|a| a.flight.on_event(FlightEvent::ProgressDone))
                        .unwrap_or_default();
                    debug_assert_eq!(acts.len(), 2);
                    // Interpret [ReleaseRetained, FinishFlight].
                    let (src_slot, src_idx) = self
                        .shared_flights
                        .get(&id)
                        .map(|a| (a.flight.source_slot, a.source_idx))
                        .unwrap_or((None, None));
                    if let (Some(slot), Some(idx)) = (src_slot, src_idx) {
                        self.free_retained_source(idx, slot);
                    }
                    if let Some(slot) = self.shared_flights.get(&id).and_then(|a| a.flight.target_slot) {
                        self.clear_transition_for_slot(slot, id);
                    }
                    self.shared_flights.remove(&id);
                }
            }
            // Terminal-transient: the coordinator drops on dispatch; belt-and-braces.
            FlightPhase::Finishing | FlightPhase::Cancelled | FlightPhase::Idle => {
                self.shared_flights.remove(&id);
            }
        }
    }

    /// Rewrite both ends' render snapshots from current scalar progress.
    fn write_flight_visuals(&mut self, id: FlightId) {
        let snapshot = self.shared_flights.get(&id).map(|a| {
            (
                a.start,
                a.flight.end.unwrap_or(a.start),
                a.flight.progress,
                a.radius_from,
                a.radius_to,
                a.clip,
                a.flight.source_slot,
                a.flight.target_slot,
                a.source_idx,
                a.start_scroll,
                a.end_scroll,
                a.path,
                a.bounds_fx.clone(),
                a.target_in_overlay,
            )
        });
        let (
            mut start,
            mut end,
            p,
            rf,
            rt,
            clip,
            sslot,
            tslot,
            sidx,
            sscroll,
            escroll,
            path,
            fx,
            target_in_overlay,
        ) = match snapshot {
            Some(v) => v,
            None => return,
        };
        // Flight bounds are canonical window coords; visuals render in this
        // composer's canvas frame (main renders untranslated; overlays render
        // translated by screen_pos).
        let (ox, oy) = self.screen_origin;
        start.x -= ox;
        start.y -= oy;
        end.x -= ox;
        end.y -= oy;
        if let (Some(slot), Some(idx)) = (sslot, sidx) {
            if self.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                self.arena.nodes[idx].transition = Some(TransitionVisual {
                    start,
                    end,
                    progress: p,
                    role: TransitionRole::Source,
                    radius_from: rf,
                    radius_to: rt,
                    clip,
                    // Clicking the ghost routes into the live target (Phase 3).
                    link_slot: tslot,
                    scroll: sscroll,
                    flight: id,
                    path,
                    bounds_fx: fx.clone(),
                    // The leaving end is detached (no longer in any tree), so
                    // the layer is its only home — always elevated.
                    elevated: true,
                });
            }
        }
        if let Some(slot) = tslot {
            if let Some(root) = self.arena.root {
                if let Some(tidx) = find_idx_by_slot(&self.arena.nodes, root, slot) {
                    let role = if sslot == tslot {
                        // Same-screen size morph: opacity untouched.
                        TransitionRole::Morph
                    } else {
                        TransitionRole::Target
                    };
                    // Same-screen morphs are layout-driven resizes, not
                    // cross-composable flights — they stay in-tree like
                    // Compose's animateBounds (never escape their container).
                    let elevated = target_in_overlay && role == TransitionRole::Target;
                    self.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start,
                        end,
                        progress: p,
                        role,
                        radius_from: rf,
                        radius_to: rt,
                        clip,
                        link_slot: None,
                        // Elevated targets render rootless in the layer, whose
                        // canvas carries no ancestor translate — the frozen
                        // ancestor sum must not be added back there.
                        scroll: if elevated { (0.0, 0.0) } else { escroll },
                        flight: id,
                        path,
                        bounds_fx: fx,
                        elevated,
                    });
                    if elevated {
                        self.elevated_roots.push(tidx);
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 4: cross-composer Tier1 (main tree ↔ overlays)
// ═══════════════════════════════════════════════════════════

/// Window-canonical bounds into a composer canvas frame.
fn off_origin(b: SharedBounds, o: (f32, f32)) -> SharedBounds {
    SharedBounds::new(b.x - o.0, b.y - o.1, b.width, b.height)
}

impl Composer {
    /// Any non-terminal Tier1 flight in this map (z-order + idle gating).
    pub(crate) fn has_cross_flights(&self) -> bool {
        self.shared_flights.values().any(|a| {
            !is_terminal(a.flight.phase) && a.source_cid != a.target_cid
        })
    }

    /// Window-level Tier1 matcher + driver (app loop, after ALL composers laid
    /// out, before render). `all[0]` is the MAIN composer — Tier1 flights
    /// always live in its map (main outlives overlays). Pairs stashed sources
    /// with freshly-appeared counterparts in OTHER composers; frees unmatched
    /// stashes same-frame (invisible); drives active Tier1 (progress, both-end
    /// visuals, completion with cross-arena teardown).
    pub(crate) fn poll_cross_flights(all: &mut [&mut Composer]) {
        if all.is_empty() {
            return;
        }
        // Fast path: no stashes anywhere and no Tier1 in main map.
        let busy = all.iter().any(|c| !c.pending_cross.is_empty()) || all[0].has_cross_flights();
        if !busy {
            // Still reconcile Tier1 completions?? No — no Tier1 exists and no
            // stash can open one. Cheap return (zero walks when idle).
            return;
        }
        // 0. Drive active Tier1 (staleness-cancel → progress → complete).
        let tier1: Vec<FlightId> = all[0]
            .shared_flights
            .iter()
            .filter(|(_, a)| !is_terminal(a.flight.phase) && a.source_cid != a.target_cid)
            .map(|(id, _)| *id)
            .collect();
        for id in tier1 {
            Self::poll_one_cross_flight(&mut *all, id);
        }
        // 1. Match stashed sources in order (main first, then peers).
        for ci in 0..all.len() {
            let stashed: Vec<PendingSource> = std::mem::take(&mut all[ci].pending_cross);
            for p in stashed {
                Self::match_pending_source(&mut *all, ci, p);
            }
        }
        let refs: Vec<&Composer> = all.iter().map(|c| &**c).collect();
        sync_scope_active_states(&refs);
        // Chrome membership is a WINDOW-level decision: a peer composer cannot
        // see a Tier1 flight (it lives in the main map), so each composer's own
        // poll cannot decide for itself. Push the union to every participant —
        // otherwise a pinned bar inside the dialog would never elevate for the
        // hero flying into it. When nothing is cross-busy this function returns
        // early and the per-composer polls remain authoritative.
        let union: HashSet<u64> = all
            .iter()
            .flat_map(|c| c.shared_flights.values())
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| a.flight.scope_id)
            .collect();
        for c in all.iter_mut() {
            c.refresh_scope_overlay_roots(&union);
            c.rebuild_layer_order();
        }
    }

    /// Drive one Tier1 flight: staleness-cancel, progress, both-end visuals
    /// (each in its writer's canvas frame), completion with cross-arena teardown.
    fn poll_one_cross_flight(all: &mut [&mut Composer], id: FlightId) {
        // Snapshot endpoints (main map).
        let (s_cid, t_cid, scope, key) = match all[0].shared_flights.get(&id) {
            Some(a) => (a.source_cid, a.target_cid, a.flight.scope_id, a.flight.key.clone()),
            None => return,
        };
        // Resolve composer indices (endpoint composer gone ⟹ cancel).
        let (Some(si), Some(ti)) = (
            all.iter().position(|c| c.composer_id == s_cid),
            all.iter().position(|c| c.composer_id == t_cid),
        ) else {
            Self::cancel_cross_flight(all, id);
            return;
        };
        // Staleness: target key missing-or-changed in its composer?
        let stale = {
            let t = &all[ti];
            match t.shared_live_map().get(&(scope, key.clone())) {
                Some(&slot) => {
                    Some(slot) != all[0].shared_flights.get(&id).and_then(|a| a.flight.target_slot)
                }
                None => true,
            }
        };
        // Superseded: a newer same-key Tier0 took over anywhere visible?
        // (Tier1 yields — the Tier0 owns the endpoints now.)
        let superseded = all.iter().any(|c| {
            c.shared_flights.iter().any(|(fid, a)| {
                *fid != id
                    && !is_terminal(a.flight.phase)
                    && a.flight.scope_id == scope
                    && a.flight.key == key
                    && a.source_cid == a.target_cid
            })
        });
        if stale || superseded {
            Self::cancel_cross_flight(all, id);
            return;
        }
        // Drive progress + visuals.
        let p = all[0].shared_flights.get(&id).map(|a| a.progress.peek()).unwrap_or(1.0);
        if let Some(a) = all[0].shared_flights.get_mut(&id) {
            a.flight.progress = p;
        }
        Self::write_cross_visuals(&mut *all, si, ti, id);
        let done = all[0].shared_flights.get(&id).is_some_and(|a| flight_progress_done(a, p));
        if done {
            let acts = all[0]
                .shared_flights
                .get_mut(&id)
                .map(|a| a.flight.on_event(FlightEvent::ProgressDone))
                .unwrap_or_default();
            debug_assert_eq!(acts.len(), 2);
            // Interpret [ReleaseRetained, FinishFlight] across composers.
            let (sslot, sidx, tslot) = all[0]
                .shared_flights
                .get(&id)
                .map(|a| (a.flight.source_slot, a.source_idx, a.flight.target_slot))
                .unwrap_or((None, None, None));
            if let (Some(slot), Some(idx)) = (sslot, sidx) {
                let owner = &mut all[si];
                if owner.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                    let mut visited = HashSet::new();
                    owner.arena.free_node_skip(idx, &HashSet::new(), &mut visited);
                }
                owner.transition_layer.retain(|&x| x != idx);
            }
            if let Some(slot) = tslot {
                let peer = &mut all[ti];
                if let Some(root) = peer.arena.root {
                    if let Some(tidx) = find_idx_by_slot(&peer.arena.nodes, root, slot) {
                        let owned =
                            peer.arena.nodes[tidx].transition.as_ref().is_some_and(|t| t.flight == id);
                        if owned {
                            peer.arena.nodes[tidx].transition = None;
                        }
                    }
                }
            }
            all[0].shared_flights.remove(&id);
        }
    }

    /// Full Tier1 teardown across composers (stale/cancel paths). Owner arena
    /// may be gone (overlay closed with retained node) — then the arena died
    /// wholesale and there is nothing to free or leak.
    fn cancel_cross_flight(all: &mut [&mut Composer], id: FlightId) {
        let Some(a) = all[0].shared_flights.remove(&id) else {
            return;
        };
        if let (Some(slot), Some(idx)) = (a.flight.source_slot, a.source_idx) {
            if let Some(owner) = all.iter_mut().find(|c| c.composer_id == a.source_cid) {
                if owner.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                    let mut visited = HashSet::new();
                    owner.arena.free_node_skip(idx, &HashSet::new(), &mut visited);
                }
                owner.transition_layer.retain(|&x| x != idx);
            }
        }
        if let Some(slot) = a.flight.target_slot {
            if let Some(peer) = all.iter_mut().find(|c| c.composer_id == a.target_cid) {
                if let Some(root) = peer.arena.root {
                    if let Some(tidx) = find_idx_by_slot(&peer.arena.nodes, root, slot) {
                        let owned =
                            peer.arena.nodes[tidx].transition.as_ref().is_some_and(|t| t.flight == id);
                        if owned {
                            peer.arena.nodes[tidx].transition = None;
                        }
                    }
                }
            }
        }
    }

    /// Write both ends' visuals for a Tier1 flight, each in its writer's
    /// canvas frame (window coords minus writer origin).
    ///
    /// NOTE (Tier1 limitation): overlay enter/exit animations (scale/offset
    /// around `screen_pos`) are NOT folded into the visuals — while the panel
    /// animates (~200ms) the flight leads/lags the panel transform by that
    /// delta. Fixing it means plumbing the overlay progress into cross-poll.
    fn write_cross_visuals(all: &mut [&mut Composer], owner_idx: usize, peer_idx: usize, id: FlightId) {
        let snapshot = all[0].shared_flights.get(&id).map(|a| {
            (
                a.start,
                a.flight.end.unwrap_or(a.start),
                a.flight.progress,
                a.radius_from,
                a.radius_to,
                a.clip,
                a.flight.source_slot,
                a.flight.target_slot,
                a.source_idx,
                a.start_scroll,
                a.end_scroll,
                a.path,
                a.bounds_fx.clone(),
                a.target_in_overlay,
            )
        });
        let (start, end, pr, rf, rt, clip, sslot, tslot, sidx, sscroll, escroll, path, fx, in_overlay) =
            match snapshot {
                Some(v) => v,
                None => return,
            };
        let (so, to) = (all[owner_idx].screen_origin, all[peer_idx].screen_origin);
        if let (Some(slot), Some(idx)) = (sslot, sidx) {
            let owner = &mut all[owner_idx];
            if owner.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot) {
                owner.arena.nodes[idx].transition = Some(TransitionVisual {
                    start: off_origin(start, so),
                    end: off_origin(end, so),
                    progress: pr,
                    role: TransitionRole::Source,
                    radius_from: rf,
                    radius_to: rt,
                    clip,
                    link_slot: tslot,
                    scroll: sscroll,
                    flight: id,
                    path,
                    bounds_fx: fx.clone(),
                    // Detached leaving end: the layer is its only home.
                    elevated: true,
                });
            }
        }
        if let Some(slot) = tslot {
            let peer = &mut all[peer_idx];
            if let Some(root) = peer.arena.root {
                if let Some(tidx) = find_idx_by_slot(&peer.arena.nodes, root, slot) {
                    peer.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start: off_origin(start, to),
                        end: off_origin(end, to),
                        progress: pr,
                        role: TransitionRole::Target,
                        radius_from: rf,
                        radius_to: rt,
                        clip,
                        link_slot: None,
                        // Rootless in the peer's layer — no ancestor translate.
                        scroll: if in_overlay { (0.0, 0.0) } else { escroll },
                        flight: id,
                        path,
                        bounds_fx: fx,
                        elevated: in_overlay,
                    });
                    if in_overlay {
                        peer.elevated_roots.push(tidx);
                    }
                }
            }
        }
    }

    /// Pair one stashed source with a freshly-appeared counterpart in another
    /// composer, or free it same-frame when unmatched (invisible removal).
    /// Freshness (peer lacked the key last frame) plus Tier0-busy guards keep
    /// stable duplicates (same hero shown in two places) from ever pairing.
    fn match_pending_source(all: &mut [&mut Composer], owner_idx: usize, p: PendingSource) {
        let kk = (p.scope_id, p.key.clone());
        for peer_idx in 0..all.len() {
            if peer_idx == owner_idx {
                continue;
            }
            // --- reads (shared, sequential — never aliased) ---
            struct Hit {
                slot: u64,
                end: SharedBounds,
                end_scroll: (f32, f32),
                radius_to: [f32; 4],
                spec: AnimationSpec,
                clip: bool,
                path: PathMotion,
                bounds_fx: Option<(VisibilityTransition, VisibilityTransition)>,
                peer_cid: u64,
                /// Peer target's `renderInOverlayDuringTransition`.
                target_in_overlay: bool,
            }
            let hit: Option<Hit> = (|| {
                let peer = &all[peer_idx];
                // Fresh appearance THIS frame only (stable duplicates never
                // pair — peer prev maps absorb appearances before cross runs).
                let slot = peer
                    .fresh_shared
                    .iter()
                    .find(|((s, k), _)| *s == kk.0 && *k == kk.1)
                    .map(|(_, sl)| *sl)?;
                // Tier0-active keys belong to Tier0 (both maps).
                if peer.shared_flights.values().any(|a| {
                    !is_terminal(a.flight.phase) && a.flight.scope_id == kk.0 && a.flight.key == kk.1
                }) {
                    return None;
                }
                if all[0].shared_flights.values().any(|a| {
                    !is_terminal(a.flight.phase) && a.flight.scope_id == kk.0 && a.flight.key == kk.1
                }) {
                    return None;
                }
                let root = peer.arena.root?;
                let tidx = find_idx_by_slot(&peer.arena.nodes, root, slot)?;
                let tid = peer.arena.nodes[tidx].id;
                let (lx, ly) = crate::app::node_abs_position(&peer.arena.nodes, root, tid);
                let (po_x, po_y) = peer.screen_origin;
                let (tw, th, marker) = {
                    let n = &peer.arena.nodes[tidx];
                    (
                        n.measured_size.width,
                        n.measured_size.height,
                        find_shared_marker(&n.modifier),
                    )
                };
                let (spec, clip, path, enter, target_is_bounds, target_in_overlay) = match marker {
                    Some(m) => (
                        m.transform.spec.clone(),
                        shared_clip_for_kind(&m.kind),
                        m.path,
                        m.enter,
                        matches!(m.kind, SharedKind::Bounds { .. }),
                        m.render_in_overlay,
                    ),
                    None => (
                        BoundsTransform::default().spec,
                        false,
                        PathMotion::Linear,
                        None,
                        false,
                        true,
                    ),
                };
                let radius_to = {
                    let n = &peer.arena.nodes[tidx];
                    shared_shape_radii(&n.modifier, tw, th)
                };
                // Freeze the peer target's ancestor scroll sum (BLOCKER #1).
                let peer_id_to_idx: HashMap<u64, usize> = peer
                    .arena
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.id, i))
                    .collect();
                let end_scroll = ancestor_scroll_sum(&peer.arena.nodes, &peer_id_to_idx, tidx);
                // Exit from the owner-side retained source marker (fade-out
                // fallback when absent — same rule as Tier 0).
                let source_marker = all[owner_idx]
                    .arena
                    .nodes
                    .get(p.src_idx)
                    .and_then(|n| find_shared_marker(&n.modifier));
                if let Some(ref sm) = source_marker {
                    let source_is_bounds = matches!(sm.kind, SharedKind::Bounds { .. });
                    if source_is_bounds != target_is_bounds {
                        crate::debug_log!(
                            "[shared] mixed-kind pair scope={} key={} — flight follows the target side",
                            kk.0,
                            kk.1
                        );
                    }
                }
                let bounds_fx = match enter {
                    Some(enter) => {
                        let exit = source_marker
                            .and_then(|m| m.exit)
                            .unwrap_or_default();
                        Some((enter, exit))
                    }
                    None => None,
                };
                Some(Hit {
                    slot,
                    end: SharedBounds::new(lx + po_x, ly + po_y, tw, th),
                    end_scroll,
                    radius_to,
                    spec,
                    clip,
                    path,
                    bounds_fx,
                    peer_cid: peer.composer_id,
                    target_in_overlay,
                })
            })();
            let Some(h) = hit else { continue };
            // --- writes (sequential &mut, never aliased) ---
            let progress = State::new(0.0f32);
            push_animatable(progress.clone(), 1.0, h.spec.clone());
            let owner_cid = all[owner_idx].composer_id;
            let id = all[0].next_flight_id;
            all[0].next_flight_id += 1;
            let mut flight = Flight::new(id, p.scope_id, p.key.clone());
            let acts = flight.on_event(FlightEvent::CounterpartAppeared {
                source_slot: p.old_slot,
                target_slot: h.slot,
            });
            debug_assert_eq!(
                acts,
                vec![FlightAction::CollectBounds { source_slot: p.old_slot, target_slot: h.slot }]
            );
            let acts = flight.on_event(FlightEvent::BoundsReady { start: p.start, end: h.end });
            debug_assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]));
            all[0].shared_flights.insert(
                id,
                ActiveFlight {
                    flight,
                    progress,
                    spec: h.spec,
                    source_idx: Some(p.src_idx),
                    start: p.start,
                    // Detached sources render rootless — no ancestor scroll.
                    start_scroll: (0.0, 0.0),
                    end_scroll: h.end_scroll,
                    radius_from: p.radius_from,
                    radius_to: h.radius_to,
                    clip: h.clip,
                    path: h.path,
                    bounds_fx: h.bounds_fx,
                    target_in_overlay: h.target_in_overlay,
                    source_cid: owner_cid,
                    target_cid: h.peer_cid,
                },
            );
            Self::write_cross_visuals(&mut *all, owner_idx, peer_idx, id);
            return;
        }
        // Unmatched: free retained same-frame (invisible plain removal).
        {
            let owner = &mut all[owner_idx];
            if owner.arena.nodes.get(p.src_idx).is_some_and(|n| n.slot_key == p.old_slot) {
                let mut visited = HashSet::new();
                owner.arena.free_node_skip(p.src_idx, &HashSet::new(), &mut visited);
            }
            owner.transition_layer.retain(|&x| x != p.src_idx);
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 3: same-screen size morph (animateBounds)
// ═══════════════════════════════════════════════════════════

/// Size-delta threshold for morph detection (layout pixels). Position-only
/// moves (scroll, sibling shifts) never trigger — scroll immunity by
/// construction, since scrolling never changes measured sizes.
pub(crate) const MORPH_EPS: f32 = 0.5;

fn size_delta(a: &SharedBounds, b: &SharedBounds) -> f32 {
    (a.width - b.width).abs().max((a.height - b.height).abs())
}

impl Composer {
    /// Active non-terminal flight for (scope, key), if any.
    pub(crate) fn flight_for_key(&self, scope_id: u64, key: &str) -> Option<FlightId> {
        self.shared_flights
            .iter()
            .find(|(_, a)| {
                !is_terminal(a.flight.phase) && a.flight.scope_id == scope_id && a.flight.key == key
            })
            .map(|(id, _)| *id)
    }

    /// Open a same-screen morph flight (no detach — the node stays live, so
    /// `source_idx` stays `None` and completion only clears visuals). The pure
    /// machine runs CounterpartAppeared + BoundsReady back-to-back with both
    /// slots identical; render picks [`TransitionRole::Morph`] from that.
    /// `radius_from_override`: seamless reopen continuity (else resolved from
    /// the modifier at the start size).
    fn begin_morph(
        &mut self,
        scope_id: u64,
        key: String,
        slot: u64,
        start: SharedBounds,
        end: SharedBounds,
        radius_from_override: Option<[f32; 4]>,
    ) {
        let root = match self.arena.root {
            Some(r) => r,
            None => return,
        };
        let tidx = match find_idx_by_slot(&self.arena.nodes, root, slot) {
            Some(i) => i,
            None => return,
        };
        let marker = find_shared_marker(&self.arena.nodes[tidx].modifier);
        let (spec, clip, path, bounds_fx) = match marker {
            Some(m) => {
                // Same node both ends — render ignores channels for Morph
                // role anyway; stored for uniformity.
                let fx = m
                    .enter
                    .clone()
                    .map(|enter| (enter, m.exit.clone().unwrap_or_default()));
                (m.transform.spec.clone(), shared_clip_for_kind(&m.kind), m.path, fx)
            }
            None => (BoundsTransform::default().spec, false, PathMotion::Linear, None),
        };
        let (tw, th, modifier) = {
            let n = &self.arena.nodes[tidx];
            (n.measured_size.width, n.measured_size.height, n.modifier.clone())
        };
        let radius_to = shared_shape_radii(&modifier, tw, th);
        let radius_from =
            radius_from_override.unwrap_or_else(|| shared_shape_radii(&modifier, start.width, start.height));
        // Same node, same frame for both ends — one frozen scroll sum
        // (BLOCKER #1); morphs stay rigid under mid-flight scroll.
        let scroll = {
            let id_to_idx: HashMap<u64, usize> = self
                .arena
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id, i))
                .collect();
            ancestor_scroll_sum(&self.arena.nodes, &id_to_idx, tidx)
        };
        let progress = State::new(0.0f32);
        push_animatable(progress.clone(), 1.0, spec.clone());
        let id = self.next_flight_id;
        self.next_flight_id += 1;
        let mut flight = Flight::new(id, scope_id, key);
        let acts = flight.on_event(FlightEvent::CounterpartAppeared { source_slot: slot, target_slot: slot });
        debug_assert_eq!(
            acts,
            vec![FlightAction::CollectBounds { source_slot: slot, target_slot: slot }]
        );
        let acts = flight.on_event(FlightEvent::BoundsReady { start, end });
        debug_assert!(matches!(acts.as_slice(), [FlightAction::StartFlight { .. }]));
        self.shared_flights.insert(
            id,
            ActiveFlight {
                flight,
                progress,
                spec,
                source_idx: None,
                start,
                start_scroll: scroll,
                end_scroll: scroll,
                radius_from,
                radius_to,
                clip,
                path,
                bounds_fx,
                target_in_overlay: true,
                source_cid: self.composer_id,
                target_cid: self.composer_id,
            },
        );
        self.write_flight_visuals(id);
    }

    /// Same-screen morph detection (post-layout, every frame): live marked
    /// slots whose SIZE changed beyond epsilon open (or seamlessly reopen) a
    /// morph flight. Baselines refresh every frame, so settled frames stay
    /// quiet; render-phase morphs never feed back into layout (no loops).
    fn poll_layout_morphs(&mut self, live: &HashMap<(u64, String), u64>) {
        let Some(root) = self.arena.root else {
            self.shared_last_bounds.clear();
            return;
        };
        enum Pending {
            Fresh { scope_id: u64, key: String, slot: u64, start: SharedBounds, end: SharedBounds },
            Reopen { id: FlightId, start: SharedBounds, radii: [f32; 4], end: SharedBounds },
        }
        let mut pending: Vec<Pending> = Vec::new();
        for (k, slot) in live {
            let Some(idx) = find_idx_by_slot(&self.arena.nodes, root, *slot) else {
                continue;
            };
            let nid = self.arena.nodes[idx].id;
            let (ax, ay) = crate::app::node_abs_position(&self.arena.nodes, root, nid);
            let (w, h) = {
                let n = &self.arena.nodes[idx];
                (n.measured_size.width, n.measured_size.height)
            };
            // Canonicalize to window coords (overlay-local + screen origin).
            let (ox, oy) = self.screen_origin;
            let cur = SharedBounds::new(ax + ox, ay + oy, w, h);
            // Baselines key on endpoint identity (scope, key), not the slot:
            // a conditional key-swap reusing one call-site slot must not
            // inherit the previous key's rect as its morph start.
            let bkey = (k.0, k.1.clone());
            if let Some(prev) = self.shared_last_bounds.get(&bkey) {
                if size_delta(prev, &cur) > MORPH_EPS {
                    match self.flight_for_key(k.0, &k.1) {
                        Some(fid) => {
                            // Active flight: only morphs reopen (switch flights
                            // own their ends — documented v1).
                            let morph = self.shared_flights.get(&fid).is_some_and(|a| {
                                a.source_idx.is_none() && a.flight.source_slot == a.flight.target_slot
                            });
                            if morph {
                                let a = &self.shared_flights[&fid];
                                // Unclamped + path-aware like the render path:
                                // reopen continuity includes spring overshoot
                                // and any arc bend.
                                let p = a.progress.peek();
                                let end_prev = a.flight.end.unwrap_or(a.start);
                                let s = lerp_flight_rect(&a.start, &end_prev, p, a.path);
                                let (rf, rt) = (a.radius_from, a.radius_to);
                                pending.push(Pending::Reopen {
                                    id: fid,
                                    start: s,
                                    radii: [0, 1, 2, 3].map(|i| rf[i] + (rt[i] - rf[i]) * p),
                                    end: cur,
                                });
                            }
                        }
                        None => pending.push(Pending::Fresh {
                            scope_id: k.0,
                            key: k.1.clone(),
                            slot: *slot,
                            start: *prev,
                            end: cur,
                        }),
                    }
                }
            }
            self.shared_last_bounds.insert(bkey, cur);
        }
        // Drop baselines for vanished endpoints (bounded memory).
        let live_keys: HashSet<(u64, String)> = live.keys().cloned().collect();
        self.shared_last_bounds.retain(|k, _| live_keys.contains(k));
        for p in pending {
            match p {
                Pending::Fresh { scope_id, key, slot, start, end } => {
                    self.begin_morph(scope_id, key, slot, start, end, None);
                }
                Pending::Reopen { id, start, radii, end } => {
                    let meta = self
                        .shared_flights
                        .get(&id)
                        .map(|a| (a.flight.scope_id, a.flight.key.clone(), a.flight.target_slot));
                    self.cancel_flight(id);
                    if let Some((scope_id, key, Some(slot))) = meta.map(|(s, k, t)| (s, k, t)) {
                        self.begin_morph(scope_id, key, slot, start, end, Some(radii));
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Phase 2: Tier-0 integration tests (composer-driven flights)
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tier0_tests {
    use super::*;
    use crate::core::composer::ComposeCtx;
    use crate::core::state::State;
    use crate::layout::constraints::Constraints;
    use crate::modifier::Color;
    use crate::ui::Column;
    use crate::ui::Row;
    use crate::ui::Stack;

    /// Plain box leaf with a shared-element marker (no text — keeps the
    /// mechanics test headless-simple; paint movement is asserted via raster).
    /// `in_overlay` is Compose `renderInOverlayDuringTransition`.
    fn hero_leaf(
        ctx: &mut ComposeCtx,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
        in_overlay: bool,
    ) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(scope.shared_content_state("hero"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, in_overlay),
        );
        ctx.end_node();
    }

    /// Layout spacer (occupies space, paints nothing).
    fn gap_leaf(ctx: &mut ComposeCtx, w: f32, h: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(w, h));
        ctx.end_node();
    }

    // NOTE: screens are #[composable] like production code. Plain closures
    // would collapse in cfg(test) key fallback (path-hash only, no statement
    // ids): the detail gap (Column-child-0) would collide with the list hero
    // (same position) and reuse its node, aborting the flight. Macro'd
    // screens carry distinct source hashes — mirroring production.
    #[crate::composable]
    fn list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, true);
    }

    #[crate::composable]
    fn detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        gap_leaf(ctx, 400.0, 100.0);
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
    }

    /// One app-loop step: compose → layout → post-layout flight poll.
    /// One app-loop step: compose → layout → post-layout flight poll.
    fn frame(composer: &mut Composer, show: &State<bool>) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        list_screen(ctx, &scope);
                    } else {
                        detail_screen(ctx, &scope);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn advance(composer: &mut Composer, show: &State<bool>) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        frame(composer, show);
    }

    /// Marked node indices in the CURRENT tree.
    fn marked_indices(composer: &Composer) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(root) = composer.layout_root_idx() {
            let nodes = composer.arena_nodes();
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                if find_shared_marker(&nodes[idx].modifier).is_some() {
                    out.push(idx);
                }
                stack.extend(nodes[idx].children.iter().copied());
            }
        }
        out
    }

    /// Mirror the app loop: main tree, then the transition layer
    /// (detached sources + elevated endpoints, absolute coords).
    fn render_heads(composer: &Composer) -> skia_safe::Surface {
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((400, 400)).expect("raster surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        let nodes = composer.arena_nodes();
        if let Some(root) = composer.layout_root_idx() {
            crate::render::render(nodes, root, surface.canvas());
            composer.render_layer(surface.canvas());
        }
        surface
    }

    fn pixel_rgb(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8) {
        let mut px = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        surface.read_pixels(&info, &mut px, 4, (x, y));
        (px[0], px[1], px[2])
    }

    fn close_enough(c: (u8, u8, u8), e: (u8, u8, u8), tol: u8) -> bool {
        c.0.abs_diff(e.0) <= tol && c.1.abs_diff(e.1) <= tol && c.2.abs_diff(e.2) <= tol
    }

    fn node_center(composer: &Composer, idx: usize) -> (i32, i32) {
        let n = &composer.arena_nodes()[idx];
        ((n.position.x + n.measured_size.width / 2.0) as i32, (n.position.y + n.measured_size.height / 2.0) as i32)
    }

    fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
        crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn tier0_flight_completes_end_to_end() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        frame(&mut composer, &show);
        assert!(composer.shared_flights.is_empty(), "no flight before any switch");
        assert!(composer.transition_layer.is_empty());

        // Switch list → detail.
        show.set(false);
        frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "switch opens exactly one flight");
        let fid = *composer.shared_flights.keys().next().unwrap();
        assert_eq!(
            composer.shared_flights[&fid].flight.phase,
            FlightPhase::Flying,
            "target resolved in the same post-layout poll"
        );
        assert_eq!(composer.transition_layer.len(), 1, "source retained + detached");
        // Exactly one live marked node (the target); it carries Target visuals.
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1, "one live endpoint (target)");
        let tvis = composer.arena_nodes()[marked[0]].transition.clone();
        assert!(matches!(tvis, Some(ref v) if v.role == TransitionRole::Target));
        // Retained source carries Source visuals at progress ~0.
        let src_idx = composer.transition_layer[0];
        let svis = composer.arena_nodes()[src_idx].transition.clone();
        assert!(matches!(svis, Some(ref v) if v.role == TransitionRole::Source && v.progress < 0.5));

        // p=0 frame paint: source opaque at its start center.
        let (sx, sy) = node_center(&composer, src_idx);
        let mut surf0 = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf0, sx, sy), (255, 0, 0), 30),
            "p=0 must match the pre-switch frame (source opaque)"
        );

        // Advance into a visible mid state (poll — timing-immune).
        let mut mid_seen = false;
        for _ in 0..40 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let p = composer.shared_flights.values().next().map(|a| a.progress.peek()).unwrap_or(1.0);
            if p > 0.2 && p < 0.95 {
                mid_seen = true;
                break;
            }
            advance(&mut composer, &show);
        }
        assert!(mid_seen, "flight must pass through a visible mid state");
        // Mid-flight paint moved/faded at the start center.
        let mut surf_mid = render_heads(&composer);
        let c = pixel_rgb(&mut surf_mid, sx, sy);
        assert!(!close_enough(c, (255, 0, 0), 30), "mid-flight paint moved/faded, got {c:?}");

        // Run to completion.
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "flight completes");
        assert!(composer.transition_layer.is_empty(), "retained source freed");
        for idx in marked_indices(&composer) {
            assert!(
                composer.arena_nodes()[idx].transition.is_none(),
                "target visuals cleared — natural render resumes"
            );
        }
        // End-state paint: detail hero center BLUE.
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let (ex, ey) = node_center(&composer, marked[0]);
        let mut surf_end = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf_end, ex, ey), (0, 0, 255), 30),
            "end state shows the detail hero"
        );
        crate::animation::clear_all_animations();
    }

    /// Scrolled screens: the hero lives inside a vertically scrolled
    /// container so both flight ends sit under an ancestor scroll offset.
    #[crate::composable]
    fn scrolled_list_screen(
        ctx: &mut ComposeCtx,
        scope: &SharedTransitionScope,
        scroll: &crate::modifier::ScrollState,
    ) {
        Column::new()
            .modifier(Modifier::new().height(200.0).vertical_scroll(scroll.clone()))
            .build(ctx, |ctx| {
                hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, true);
                gap_leaf(ctx, 400.0, 400.0);
            });
    }

    #[crate::composable]
    fn scrolled_detail_screen(
        ctx: &mut ComposeCtx,
        scope: &SharedTransitionScope,
        scroll: &crate::modifier::ScrollState,
    ) {
        Column::new()
            .modifier(Modifier::new().height(200.0).vertical_scroll(scroll.clone()))
            .build(ctx, |ctx| {
                gap_leaf(ctx, 400.0, 140.0);
                hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
            });
    }

    /// One app-loop step for the scrolled screens.
    fn scroll_frame(composer: &mut Composer, show: &State<bool>, scroll: &crate::modifier::ScrollState) {
        let s = show.clone();
        let sc = scroll.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        scrolled_list_screen(ctx, &scope, &sc);
                    } else {
                        scrolled_detail_screen(ctx, &scope, &sc);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn scroll_advance(composer: &mut Composer, show: &State<bool>, scroll: &crate::modifier::ScrollState) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        scroll_frame(composer, show, scroll);
    }

    /// Pure position accumulation root → node (no scroll subtraction — the
    /// frame render's `(x, y)` uses at the flight node).
    fn pure_accumulation(composer: &Composer, idx: usize) -> (f32, f32) {
        let nodes = composer.arena_nodes();
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let (mut ax, mut ay) = (nodes[idx].position.x, nodes[idx].position.y);
        let mut cur = idx;
        while let Some(pid) = nodes[cur].parent_id {
            let Some(&pidx) = id_to_idx.get(&pid) else { break };
            ax += nodes[pidx].position.x;
            ay += nodes[pidx].position.y;
            cur = pidx;
        }
        (ax, ay)
    }

    #[test]
    fn tier0_flight_scroll_addback_exact() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let scroll = crate::modifier::ScrollState::new();

        scroll_frame(&mut composer, &show, &scroll);
        // Scroll AFTER first layout so the offset does not disturb settling.
        scroll.offset.set(40.0);
        scroll_frame(&mut composer, &show, &scroll);

        // Switch list → detail under a live (0, 40) ancestor scroll.
        show.set(false);
        scroll_frame(&mut composer, &show, &scroll);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened under scroll");

        // Frozen scroll bookkeeping: detached source sums to zero, the live
        // target freezes the (0, 40) ancestor sum.
        {
            let a = composer.shared_flights.values().next().expect("flight");
            assert_eq!(a.start_scroll, (0.0, 0.0), "detached source is rootless");
            assert_eq!(a.end_scroll, (0.0, 40.0), "target freezes the ancestor scroll");
            let e = a.flight.end.expect("end resolved after first poll");
            // Render invariant: lerped end + add-back == pure layout origin,
            // i.e. the flight transform paints exactly on the lerped rect.
            let tslot = a.flight.target_slot.expect("target slot");
            let root = composer.layout_root_idx().expect("root");
            let tidx = find_idx_by_slot(composer.arena_nodes(), root, tslot).expect("target");
            let (px, py) = pure_accumulation(&composer, tidx);
            assert!(
                (e.x + a.end_scroll.0 - px).abs() < 1e-3
                    && (e.y + a.end_scroll.1 - py).abs() < 1e-3,
                "end + scroll == pure layout origin ({} + {} vs {px},{py})",
                e.x,
                e.y
            );
        }
        // Visuals: the source is detached (rootless) and the target is
        // elevated (renderInOverlay defaults to true), so BOTH ends render
        // rootless from the layer and carry a zero add-back. The layer
        // supplies each end's scroll-corrected absolute origin instead, and
        // the paint still lands exactly on the lerped rect (pixel proof below).
        {
            let nodes = composer.arena_nodes();
            let a = composer.shared_flights.values().next().expect("flight");
            let sslot = a.flight.source_slot.expect("source slot");
            let sidx = a.source_idx.expect("retained source");
            assert!(nodes.get(sidx).is_some_and(|n| n.slot_key == sslot));
            let svis = nodes[sidx].transition.as_ref().expect("source visual");
            assert!(svis.elevated, "detached sources always fly in the layer");
            assert_eq!(svis.scroll, (0.0, 0.0));
            let tslot = a.flight.target_slot.expect("target slot");
            let root = composer.layout_root_idx().expect("root");
            let tidx = find_idx_by_slot(nodes, root, tslot).expect("target");
            let tvis = nodes[tidx].transition.as_ref().expect("target visual");
            assert!(tvis.elevated, "renderInOverlay defaults to true");
            assert_eq!(
                tvis.scroll,
                (0.0, 0.0),
                "layer canvas carries no ancestor translate — no add-back"
            );
            // The layer paints this node at its scroll-corrected absolute
            // origin, which differs from the pure layout accumulation by
            // exactly the frozen ancestor sum the in-tree path adds back.
            let id_to_idx: HashMap<u64, usize> =
                nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
            let abs = abs_rect_upward(nodes, &id_to_idx, tidx);
            let pure = pure_accumulation(&composer, tidx);
            assert!(
                (abs.y - (pure.1 - a.end_scroll.1)).abs() < 1e-3,
                "layer origin compensates the ancestor scroll exactly ({} vs {})",
                abs.y,
                pure.1 - a.end_scroll.1
            );
            let l = tvis.lerped();
            let clip = tvis.canvas_rrect();
            assert!(
                (clip.rect().left - (l.x + tvis.scroll.0)).abs() < 1e-3
                    && (clip.rect().top - (l.y + tvis.scroll.1)).abs() < 1e-3,
                "canvas clip lands on the lerped rect"
            );
            let dev = tvis.screen_rrect();
            assert!(
                (dev.rect().left - l.x).abs() < 1e-3 && (dev.rect().top - l.y).abs() < 1e-3,
                "device clip lands on the lerped rect (hit-test frame)"
            );
        }

        // Pixel proof: the flight paints exactly on the lerped rect even though
        // the hero's ancestor is scrolled (with elevation the layer supplies
        // the absolute origin; the in-tree path supplies it as an add-back —
        // same pixels either way).
        let mut painted = false;
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let probe = composer.shared_flights.values().next().map(|a| {
                let p = a.progress.peek();
                let e = a.flight.end.expect("end resolved after first poll");
                (p, a.start.lerp(&e, p))
            });
            if let Some((p, l)) = probe {
                if p > 0.3 && p < 0.7 && l.y > 24.0 {
                    let mut surf = render_heads(&composer);
                    let cx = (l.x + l.width / 2.0) as i32;
                    let cy = (l.y + l.height / 2.0) as i32;
                    let c = pixel_rgb(&mut surf, cx, cy);
                    assert!(
                        c.0 > 140 && c.2 > 50,
                        "lerped center shows blended flight paint, got {c:?}"
                    );
                    let mut surf2 = render_heads(&composer);
                    let above = pixel_rgb(&mut surf2, cx, (l.y - 12.0) as i32);
                    assert!(
                        close_enough(above, (255, 255, 255), 40),
                        "no flight paint above the lerped rect, got {above:?}"
                    );
                    painted = true;
                    break;
                }
            }
            scroll_advance(&mut composer, &show, &scroll);
        }
        assert!(painted, "flight must pass through the scroll-alignment window");

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            scroll_advance(&mut composer, &show, &scroll);
        }
        assert!(composer.shared_flights.is_empty(), "scrolled flight completes");
        crate::animation::clear_all_animations();
    }

    // ── Overlay escape (Compose `renderInOverlayDuringTransition`) ──
    //
    // The entering hero lives inside a scroll viewport that sits LOW in the
    // window while the leaving hero sits at the very top, so mid-flight the
    // lerped rect is ABOVE that viewport — outside the target's ancestor clip.
    // In-tree painting is cut there (the ancestor clip is on the canvas and a
    // descendant can never un-set it); the transition layer paints it anyway.

    #[crate::composable]
    fn escape_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: bool) {
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new().size(60.0, 60.0))
                    .build(ctx, |ctx| {
                        hero_leaf(ctx, 60.0, 60.0, Color::RED, scope, in_overlay);
                    });
            });
    }

    #[crate::composable]
    fn escape_detail_screen(
        ctx: &mut ComposeCtx,
        scope: &SharedTransitionScope,
        scroll: &crate::modifier::ScrollState,
        in_overlay: bool,
    ) {
        Column::new()
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                // Pushes the clipped viewport down to y ≈ 240.
                gap_leaf(ctx, 10.0, 240.0);
                Column::new()
                    .modifier(Modifier::new().size(200.0, 160.0).vertical_scroll(scroll.clone()))
                    .build(ctx, |ctx| {
                        gap_leaf(ctx, 10.0, 30.0);
                        hero_leaf(ctx, 140.0, 80.0, Color::BLUE, scope, in_overlay);
                    });
            });
    }

    fn escape_frame(
        composer: &mut Composer,
        show: &State<bool>,
        scroll: &crate::modifier::ScrollState,
        in_overlay: bool,
    ) {
        let s = show.clone();
        let sc = scroll.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        escape_list_screen(ctx, &scope, in_overlay);
                    } else {
                        escape_detail_screen(ctx, &scope, &sc, in_overlay);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn escape_advance(
        composer: &mut Composer,
        show: &State<bool>,
        scroll: &crate::modifier::ScrollState,
        in_overlay: bool,
    ) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        escape_frame(composer, show, scroll, in_overlay);
    }

    /// (progress, lerped rect) of the single active flight.
    fn flight_probe(composer: &Composer) -> Option<(f32, SharedBounds)> {
        composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let end = a.flight.end.unwrap_or(a.start);
            (p, a.start.lerp(&end, p))
        })
    }

    /// Absolute rect (canvas frame) of the nearest scrolled ancestor viewport.
    fn nearest_viewport_abs(composer: &Composer, idx: usize) -> Option<(f32, f32, f32, f32)> {
        let nodes = composer.arena_nodes();
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut cur = idx;
        while let Some(pid) = nodes[cur].parent_id {
            let Some(&pidx) = id_to_idx.get(&pid) else { return None };
            let n = &nodes[pidx];
            if n.scroll_viewport_height > 0.0 {
                let b = abs_rect_upward(nodes, &id_to_idx, pidx);
                return Some((b.x, b.y, n.scroll_viewport_width, n.scroll_viewport_height));
            }
            cur = pidx;
        }
        None
    }

    /// Drive the escape scenario to mid-flight and return
    /// (progress, lerped rect, target index, target ancestor viewport).
    #[allow(clippy::type_complexity)]
    fn escape_to_midflight(
        composer: &mut Composer,
        show: &State<bool>,
        scroll: &crate::modifier::ScrollState,
        in_overlay: bool,
    ) -> (f32, SharedBounds, usize, (f32, f32, f32, f32)) {
        escape_frame(composer, show, scroll, in_overlay);
        show.set(false);
        escape_frame(composer, show, scroll, in_overlay);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");
        for _ in 0..200 {
            if let Some((p, l)) = flight_probe(composer) {
                if (0.4..=0.6).contains(&p) {
                    let tslot = composer
                        .shared_flights
                        .values()
                        .next()
                        .and_then(|a| a.flight.target_slot)
                        .expect("target slot");
                    let nodes = composer.arena_nodes();
                    let root = composer.layout_root_idx().expect("root");
                    let tidx = find_idx_by_slot(nodes, root, tslot).expect("target node");
                    let vp = nearest_viewport_abs(composer, tidx).expect("clipped ancestor");
                    return (p, l, tidx, vp);
                }
            }
            if composer.shared_flights.is_empty() {
                break;
            }
            escape_advance(composer, show, scroll, in_overlay);
        }
        panic!("flight must pass through the mid-flight window");
    }

    #[test]
    fn tier0_overlay_escape_paints_outside_ancestor_clip() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let scroll = crate::modifier::ScrollState::new();

        let (p, l, tidx, (_, vp_y, _, _)) = escape_to_midflight(&mut composer, &show, &scroll, true);
        // The probe point is genuinely OUTSIDE the target's ancestor viewport —
        // i.e. paint there can only come from the elevation pass.
        let cy = l.y + l.height / 2.0;
        assert!(
            cy < vp_y,
            "mid-flight rect center ({cy}) must be above the target viewport ({vp_y})"
        );

        // The claim first: paint lands OUTSIDE the ancestor clip. In-tree
        // painting cannot do this — the ancestor clip is on the canvas and a
        // descendant can never un-set it (this is what fails without the layer).
        let mut surf = render_heads(&composer);
        let c = pixel_rgb(&mut surf, (l.x + l.width / 2.0) as i32, cy as i32);
        // Both ends paint (red ghost over blue target). Ghost-only would leave
        // G ≈ 255·p ≈ 127; with the target underneath it drops to ≈ 255·p·(1−p).
        assert!(
            c.1 < 90,
            "target escaped the ancestor clip and paints under the ghost at p={p}, got {c:?}"
        );

        // Then the mechanism that makes it possible.
        let tvis = composer.arena_nodes()[tidx]
            .transition
            .as_ref()
            .expect("target visual")
            .clone();
        assert!(tvis.elevated, "target flies in the layer by default");
        assert!(
            composer.transition_roots().contains(&tidx),
            "elevated target is a layer root"
        );

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            escape_advance(&mut composer, &show, &scroll, true);
        }
        assert!(composer.shared_flights.is_empty(), "flight completes");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier0_overlay_opt_out_paints_in_tree_and_stays_clipped() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let scroll = crate::modifier::ScrollState::new();

        let (p, l, tidx, _) = escape_to_midflight(&mut composer, &show, &scroll, false);
        let tvis = composer.arena_nodes()[tidx]
            .transition
            .as_ref()
            .expect("target visual")
            .clone();
        assert!(!tvis.elevated, "renderInOverlay=false keeps the target in-tree");
        // Both ends paint the SAME lerped rect — opting out changes only the
        // clip, never the position. A mismatched-looking pair on screen is the
        // ancestor clip edge over the flying shape, not a transform divergence:
        // the entering end is cut, the detached leaving end cannot be.
        let svis = composer.arena_nodes()[composer.transition_layer[0]]
            .transition
            .as_ref()
            .expect("source visual")
            .clone();
        assert_eq!(
            svis.lerped(),
            tvis.lerped(),
            "leaving ghost and entering target share one flight rect"
        );
        // In-tree means the frozen ancestor sum is added back by the flight
        // transform (the layer path does not use it at all).
        let a = composer.shared_flights.values().next().expect("flight");
        assert_eq!(
            tvis.scroll, a.end_scroll,
            "opt-out target still carries its frozen ancestor scroll"
        );
        assert!(
            !composer.transition_roots().contains(&tidx),
            "opt-out target stays out of the layer"
        );

        // Ancestors still clip it: only the ghost shows through at the probe.
        let mut surf = render_heads(&composer);
        let c = pixel_rgb(
            &mut surf,
            (l.x + l.width / 2.0) as i32,
            (l.y + l.height / 2.0) as i32,
        );
        assert!(
            c.1 > 100 && c.0 > 200,
            "ghost-only paint at p={p} (target clipped in-tree), got {c:?}"
        );

        // ...and the target is NOT parked at its own layout rect: the flying
        // content only ever covers the LERPED rect, so a mid-flight probe over
        // the target's resting place shows background. This is what separates
        // "clipped while flying" from "stopped flying".
        let nodes = composer.arena_nodes();
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let rest = abs_rect_upward(nodes, &id_to_idx, tidx);
        assert!(rest.y > l.y, "the target rests below the mid-flight rect");
        let mut surf2 = render_heads(&composer);
        let c2 = pixel_rgb(
            &mut surf2,
            (rest.x + rest.width / 2.0) as i32,
            (rest.y + rest.height / 2.0) as i32,
        );
        assert!(
            close_enough(c2, (255, 255, 255), 40),
            "nothing paints at the target's resting rect mid-flight, got {c2:?}"
        );

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            escape_advance(&mut composer, &show, &scroll, false);
        }
        assert!(composer.shared_flights.is_empty(), "flight completes");
        crate::animation::clear_all_animations();
    }

    /// Tier1 pair where the PEER (overlay) target sits inside a clipped
    /// viewport low in the window — the same escape geometry, cross-composer.
    fn escape_cross_frame(
        a: &mut Composer,
        b: &mut Composer,
        show_a: &State<bool>,
        show_b: &State<bool>,
        scope: &SharedTransitionScope,
        scroll: &crate::modifier::ScrollState,
    ) {
        let (sa, sb) = (show_a.clone(), show_b.clone());
        let (sca, scb) = (scope.clone(), scope.clone());
        let sc = scroll.clone();
        cross_frame(
            a,
            b,
            |ctx| shell(ctx, |ctx| {
                if sa.get() {
                    hero_leaf(ctx, 60.0, 60.0, Color::RED, &sca, true);
                }
            }),
            |ctx| shell(ctx, |ctx| {
                if sb.get() {
                    gap_leaf(ctx, 10.0, 240.0);
                    Column::new()
                        .modifier(Modifier::new().size(200.0, 160.0).vertical_scroll(sc.clone()))
                        .build(ctx, |ctx| {
                            gap_leaf(ctx, 10.0, 30.0);
                            hero_leaf(ctx, 140.0, 80.0, Color::BLUE, &scb, true);
                        });
                }
            }),
        );
    }

    /// Tier1 hit routing: the peer's own hit test must reach the flying target
    /// at its animated rect even where the peer's ancestors reject the point.
    /// This is the case the cross-composer ghost cannot cover — its live target
    /// lives in an arena the single-arena ghost search cannot see — so before
    /// the overlay pass the tap was simply lost (`hit_overlay` probes exactly
    /// this call for the overlay's local point).
    #[test]
    fn tier1_elevated_target_hittable_outside_its_ancestors() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(88);
        let show_a = State::new(true);
        let show_b = State::new(false);
        let scroll = crate::modifier::ScrollState::new();

        escape_cross_frame(&mut a, &mut b, &show_a, &show_b, &scope, &scroll);
        show_a.set(false);
        show_b.set(true);
        escape_cross_frame(&mut a, &mut b, &show_a, &show_b, &scope, &scroll);
        assert_eq!(a.shared_flights.len(), 1, "Tier1 opens in the main map");

        let bt = marked_in(&b);
        assert_eq!(bt.len(), 1, "one peer endpoint");
        let tidx = bt[0];

        let mut probed = None;
        for _ in 0..200 {
            if let Some(p) = a.shared_flights.values().next().map(|f| f.progress.peek()) {
                if (0.4..=0.6).contains(&p) {
                    let vis = b.arena_nodes()[tidx]
                        .transition
                        .clone()
                        .expect("peer target visual");
                    let vp = nearest_viewport_abs(&b, tidx).expect("clipped ancestor");
                    probed = Some((p, vis.lerped(), vp, vis));
                    break;
                }
            }
            if a.shared_flights.is_empty() {
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        let (p, l, (_, vp_y, _, _), vis) = probed.expect("flight passes mid-way");
        let (cx, cy) = (l.x + l.width / 2.0, l.y + l.height / 2.0);
        assert!(cy < vp_y, "probe is above the peer viewport ({cy} < {vp_y})");

        let bn = b.arena_nodes();
        let root = b.layout_root_idx().expect("peer root");
        let plain = crate::layout::node::hit_test(bn, root, cx, cy);
        assert!(
            !plain.contains(&tidx),
            "the plain walk cannot reach the clipped target (path={plain:?})"
        );
        // The claim: the layer-aware walk does reach it, at the animated rect.
        let path = hit_test_with_flights(bn, root, b.transition_roots(), cx, cy);
        assert!(
            path.contains(&tidx),
            "layer-aware hit test reaches the flying target at p={p} (path={path:?})"
        );
        // Then the mechanism that makes it possible.
        assert!(vis.elevated, "peer target flies in its own layer");

        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        assert!(a.shared_flights.is_empty(), "Tier1 completes");
        crate::animation::clear_all_animations();
    }

    /// Marker flags (and specs) are sampled when a flight resolves, so the
    /// arena's marker must be fresh. Freshness follows *where* the flag is
    /// read: a state read in the slot that builds the marker recomposes that
    /// slot and rewrites the node's modifier, while a read in an OUTER slot
    /// leaves this subtree skipped and keeps the cached modifier — the runtime
    /// toggle then silently does nothing until something else rebuilds the
    /// screen. Build markers with their flags read in their own slot.
    #[test]
    fn marker_flag_refreshes_only_when_read_in_the_marker_slot() {
        let _g = lock_serial();
        let marker_flag = |composer: &Composer| -> Option<bool> {
            composer
                .arena_nodes()
                .iter()
                .find_map(|n| find_shared_marker(&n.modifier).map(|m| m.render_in_overlay))
        };

        // Read INSIDE the marker's slot → refreshed.
        let flag = State::new(true);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            let f = flag.clone();
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        let inner = f.get();
                        hero_leaf(ctx, 120.0, 80.0, Color::RED, &scope, inner);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        assert_eq!(marker_flag(&composer), Some(true), "initial flag");
        flag.set(false);
        build(&mut composer);
        assert_eq!(
            marker_flag(&composer),
            Some(false),
            "a flag read in the marker's own slot must reach the arena"
        );

        // Read in an OUTER slot → this is the trap, documented as such.
        let flag = State::new(true);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            let f = flag.clone();
            composer.compose(|ctx| {
                let outer = f.get();
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        hero_leaf(ctx, 120.0, 80.0, Color::RED, &scope, outer);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        flag.set(false);
        build(&mut composer);
        assert_eq!(
            marker_flag(&composer),
            Some(true),
            "outer read leaves the subtree skipped — the marker keeps its cached flag \
             (this is why the demo reads the toggle inside the screen slot)"
        );

        // Nested builders, exactly the demo's shape, with the flag read inside
        // the composable that creates the marked node (the demo's `hero_box`
        // helper). That is the pattern that keeps the arena marker fresh.
        let flag = State::new(true);
        let show = State::new(false);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            let (f, s) = (flag.clone(), show.clone());
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        if s.get() {
                            gap_leaf(ctx, 10.0, 60.0);
                            flag_hero(ctx, &f, &scope, 60.0, 60.0, Color::RED);
                        } else {
                            Row::new()
                                .modifier(Modifier::new().fill_max_width())
                                .build(ctx, |ctx| {
                                    gap_leaf(ctx, 40.0, 10.0);
                                    Column::new()
                                        .modifier(Modifier::new().clip(Shape::Rectangle))
                                        .build(ctx, |ctx| {
                                            flag_hero(ctx, &f, &scope, 120.0, 80.0, Color::BLUE);
                                        });
                                });
                        }
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        assert_eq!(marker_flag(&composer), Some(true), "nested initial");
        flag.set(false);
        build(&mut composer);
        assert_eq!(
            marker_flag(&composer),
            Some(false),
            "a flag read inside the composable that builds the marker reaches it \
             through any nesting depth"
        );
    }

    /// Marker leaf whose flag is read in THIS composable's slot — the pattern
    /// the demo's `hero_box` helper uses.
    #[crate::composable]
    fn flag_hero(
        ctx: &mut ComposeCtx,
        overlay: &State<bool>,
        scope: &SharedTransitionScope,
        w: f32,
        h: f32,
        color: Color,
    ) {
        let in_overlay = overlay.get();
        hero_leaf(ctx, w, h, color, scope, in_overlay);
    }

    /// One shared hero composable, called from BOTH screen arms — the shape the
    /// demo uses after switching to a `hero_box` helper. A key collision across
    /// the arms would silently degrade the screen switch into a same-screen
    /// morph, so the demo's structure is pinned here.
    #[crate::composable]
    fn shared_hero_box(
        ctx: &mut ComposeCtx,
        scope: &SharedTransitionScope,
        w: f32,
        h: f32,
        color: Color,
        in_overlay: bool,
    ) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(scope.shared_content_state("hero"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, in_overlay),
        );
        ctx.end_node();
    }

    #[test]
    fn demo_shape_shared_hero_composable_still_opens_a_switch() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let build = |composer: &mut Composer| {
            let s = show.clone();
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        if s.get() {
                            shared_hero_box(ctx, &scope, 60.0, 60.0, Color::RED, true);
                        } else {
                            // Nested like the demo's Row → clipped Column.
                            Column::new()
                                .modifier(Modifier::new().clip(Shape::Rectangle))
                                .build(ctx, |ctx| {
                                    shared_hero_box(ctx, &scope, 300.0, 160.0, Color::BLUE, true);
                                });
                        }
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        build(&mut composer);
        show.set(false);
        build(&mut composer);
        assert_eq!(composer.shared_flights.len(), 1, "a switch flight opens");
        let f = composer.shared_flights.values().next().unwrap();
        assert_ne!(
            f.flight.source_slot, f.flight.target_slot,
            "distinct slots ⇒ a real switch, not a same-screen morph"
        );
        assert_eq!(composer.transition_layer.len(), 1, "leaving hero retained");
        crate::animation::clear_all_animations();
    }

    /// Pixel delta between two frames of the same size (channel threshold 2 to
    /// ignore antialiasing noise). Backing helper for the same-frame overlay
    /// A/B checks, where only one flag changes between the two renders.
    fn surface_diff(a: &mut skia_safe::Surface, b: &mut skia_safe::Surface) -> (usize, u8) {
        const W: usize = 400;
        const H: usize = 400;
        let info = skia_safe::ImageInfo::new(
            (W as i32, H as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let mut pa = vec![0u8; W * H * 4];
        let mut pb = vec![0u8; W * H * 4];
        a.read_pixels(&info, &mut pa, W * 4, (0, 0));
        b.read_pixels(&info, &mut pb, W * 4, (0, 0));
        let mut n = 0usize;
        let mut maxd = 0u8;
        for i in (0..pa.len()).step_by(4) {
            let d = pa[i]
                .abs_diff(pb[i])
                .max(pa[i + 1].abs_diff(pb[i + 1]))
                .max(pa[i + 2].abs_diff(pb[i + 2]));
            if d > 2 {
                n += 1;
            }
            maxd = maxd.max(d);
        }
        (n, maxd)
    }

    /// Same-frame A/B of the opt-out: render as-is, flip ONLY the target's
    /// render pass, render again, report the delta. Everything else (progress,
    /// rects, radii, scroll) is identical, so any difference is the pass.
    fn opt_out_vs_elevated_delta(composer: &mut Composer) -> (usize, u8) {
        let mut before = render_heads(composer);
        let tslot = composer
            .shared_flights
            .values()
            .next()
            .and_then(|a| a.flight.target_slot);
        if let Some(slot) = tslot {
            let root = composer.layout_root_idx().expect("root");
            let tidx = find_idx_by_slot(composer.arena_nodes(), root, slot).expect("target");
            assert!(
                !composer.arena.nodes[tidx].transition.as_ref().unwrap().elevated,
                "A/B starts from the opt-out state"
            );
            composer.arena.nodes[tidx].transition.as_mut().unwrap().elevated = true;
            composer.elevated_roots.push(tidx);
            composer.rebuild_layer_order();
        }
        let mut after = render_heads(composer);
        surface_diff(&mut before, &mut after)
    }

    /// Drive to mid-flight in the given direction and A/B the render pass.
    fn opt_out_ab_at_midflight(reverse: bool) -> (f32, usize, u8) {
        let mut composer = Composer::new();
        let show = State::new(!reverse); // forward starts on the list screen
        let scroll = crate::modifier::ScrollState::new();
        escape_frame(&mut composer, &show, &scroll, false);
        show.set(reverse);
        escape_frame(&mut composer, &show, &scroll, false);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");
        let mut p = 0.0;
        for _ in 0..200 {
            match flight_probe(&composer) {
                Some((prog, _)) if prog >= 0.5 => {
                    p = prog;
                    break;
                }
                None => break,
                _ => {}
            }
            escape_advance(&mut composer, &show, &scroll, false);
        }
        let (n, maxd) = opt_out_vs_elevated_delta(&mut composer);
        crate::animation::clear_all_animations();
        (p, n, maxd)
    }

    /// The opt-out artifact is DIRECTIONAL, and that is not obvious from a
    /// running window (it cost a long debugging round): only the leg whose
    /// DESTINATION clips shows the step, because only the entering end can be
    /// clipped at all. Into the clipped screen → the two passes disagree;
    /// flying back OUT of it → they are pixel-identical.
    #[test]
    fn opt_out_artifact_is_directional() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();

        // Forward: the destination screen clips its hero.
        let (p, n, maxd) = opt_out_ab_at_midflight(false);
        assert!(
            n > 0,
            "into a clipping destination the pass must matter (p={p}, n={n}, max_delta={maxd})"
        );

        // Back: the destination screen (list) has no clipping ancestor.
        let (p, n, maxd) = opt_out_ab_at_midflight(true);
        assert_eq!(
            n, 0,
            "back out of the container the flag must be a no-op (p={p}, n={n}, max_delta={maxd})"
        );
        crate::animation::clear_all_animations();
    }

    /// The two screens of the under-bar scenario. Macro'd composables on
    /// purpose: the cfg(test) key fallback is path-hash based, so plain
    /// branches let one screen's node take over the other's slot (the detail
    /// hero ended up with the list gap's measurements) — the same trap the
    /// list/detail screens document.
    #[crate::composable]
    fn under_bar_list(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: bool) {
        gap_leaf(ctx, 10.0, 300.0);
        hero_leaf(ctx, 150.0, 150.0, Color::RED, scope, in_overlay);
    }

    #[crate::composable]
    fn under_bar_detail(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, in_overlay: bool) {
        // Lands at the top of the window, so its upper band sits under the bar.
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, in_overlay);
    }

    /// The correct use of `render_in_overlay = false` on the ENTERING side:
    /// keep the flight in tree order so chrome painted later still covers it
    /// ("content sliding under a pinned bar"). With the flag on (Compose
    /// default) the endpoint becomes a layer root and paints OVER that chrome
    /// instead — which is the artifact this flag exists to avoid when the
    /// element belongs under a bar.
    ///
    /// Tree: Stack { Column { screen }, bar } — the bar is the LAST child, so
    /// it paints after the whole screen and tree order alone decides whether
    /// the flying hero passes over it or under it.
    #[test]
    fn opt_out_keeps_the_entering_end_under_later_siblings() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        // The switch is read INSIDE the slot that builds the branches: reading
        // it a level up leaves this subtree skipped and the screen never
        // changes (same trap as a marker flag read in the wrong slot).
        let frame = |composer: &mut Composer| {
            let s = show.clone();
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_size())
                            .build(ctx, |ctx| {
                                if s.get() {
                                    under_bar_list(ctx, &scope, false);
                                } else {
                                    under_bar_detail(ctx, &scope, false);
                                }
                            });
                        // Pinned chrome, painted last → above the whole screen.
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .size(400.0, 60.0)
                                    .background(Color::GREEN, Shape::Rectangle),
                            )
                            .build(ctx, |_| {});
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        frame(&mut composer);
        show.set(false);
        frame(&mut composer);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");

        // Late in the flight the hero's upper band reaches the bar's strip.
        // The sample is captured INSIDE the loop: a 300ms tween stepped at
        // ~16ms gives only a few frames in the window, so a stalled machine can
        // step over it — that must fail with a clear message, never as a panic
        // on an empty flight map.
        let mut sample: Option<(f32, u64, usize)> = None;
        for _ in 0..200 {
            match flight_probe(&composer) {
                Some((p, _)) if p >= 0.85 => {
                    let tslot = composer
                        .shared_flights
                        .values()
                        .next()
                        .and_then(|a| a.flight.target_slot)
                        .expect("flight still alive in the window");
                    let root = composer.layout_root_idx().expect("root");
                    let tidx = find_idx_by_slot(composer.arena_nodes(), root, tslot)
                        .expect("target node");
                    sample = Some((p, tslot, tidx));
                    break;
                }
                None => break,
                _ => {}
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            frame(&mut composer);
        }
        let (p_at_probe, _tslot, tidx) =
            sample.expect("flight must be sampled inside its late window (machine load?)");

        // Same frame, one flag flipped: opt-out (as built) vs elevated.
        let (probe_x, probe_y) = (150.0, 40.0);
        let mut in_tree = render_heads(&composer);
        let c_tree = pixel_rgb(&mut in_tree, probe_x as i32, probe_y as i32);
        assert!(
            c_tree.1 > 180 && c_tree.2 < 80,
            "opt-out: pinned bar still covers the hero, got {c_tree:?}"
        );

        let tslot = _tslot;
        let root = composer.layout_root_idx().expect("root");
        let tidx = find_idx_by_slot(composer.arena_nodes(), root, tslot).expect("target");
        composer.arena.nodes[tidx].transition.as_mut().unwrap().elevated = true;
        composer.elevated_roots.push(tidx);
        composer.rebuild_layer_order();
        let mut elevated = render_heads(&composer);
        let c_elev = pixel_rgb(&mut elevated, probe_x as i32, probe_y as i32);
        assert!(
            c_elev.2 > 150 && c_elev.1 < 120,
            "elevated: hero paints over the pinned bar, got {c_elev:?}"
        );
        // The ghost really is in flight at the sampled moment (otherwise the
        // "the bar stays on top" claim above would be vacuous): sample a point
        // the ghost covers but the bar does not. Over the white surface the
        // ghost pulls green down (255·p), where an untilted background is 255.
        let ghost = pixel_rgb(&mut elevated, probe_x as i32, 70);
        assert!(
            ghost.1 < 250,
            "the leaving ghost must tint the strip at p={p_at_probe}, got {ghost:?}"
        );
        // ...and over the bar itself the wash is bounded by the ghost's alpha.
        let wash = 255.0 * (1.0 - p_at_probe);
        assert!(
            c_tree.0 as f32 <= wash + 30.0,
            "at most the ghost wash (1-p)={wash:.0} shows over the bar, got red={} at p={p_at_probe}",
            c_tree.0
        );

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            frame(&mut composer);
        }
        crate::animation::clear_all_animations();
    }

    /// Under-bar scenario with the bar opting into the scope overlay (Compose
    /// `Modifier.renderInSharedTransitionScopeOverlay`).
    fn chrome_frame(
        composer: &mut Composer,
        show: &State<bool>,
        chrome_z: f32,
        hero_in_overlay: bool,
    ) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        if s.get() {
                            under_bar_list(ctx, &scope, hero_in_overlay);
                        } else {
                            under_bar_detail(ctx, &scope, hero_in_overlay);
                        }
                    });
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(400.0, 60.0)
                                .background(Color::GREEN, Shape::Rectangle)
                                .render_in_shared_transition_scope_overlay(&scope, chrome_z),
                        )
                        .build(ctx, |_| {});
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn chrome_bar_index(composer: &Composer) -> usize {
        composer
            .arena_nodes()
            .iter()
            .position(|n| find_scope_overlay_marker(&n.modifier).is_some())
            .expect("chrome bar node")
    }

    /// Compose `Modifier.renderInSharedTransitionScopeOverlay`: chrome that
    /// opts in keeps its spatial relationship while the flight runs, which is
    /// the piece the shared-element flag cannot do — it covers BOTH ends,
    /// including the detached leaving ghost.
    #[test]
    fn scope_overlay_chrome_covers_both_flight_ends() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        // Heroes elevated: the A/B must isolate the CHROME elevation, so the
        // flight has to be in the layer too (otherwise the bar covers it by
        // plain tree order and the comparison proves nothing).
        chrome_frame(&mut composer, &show, 1.0, true);
        show.set(false);
        chrome_frame(&mut composer, &show, 1.0, true);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");
        let bar = chrome_bar_index(&composer);

        for _ in 0..200 {
            match flight_probe(&composer) {
                Some((p, _)) if p >= 0.9 => break,
                None => break,
                _ => {}
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            chrome_frame(&mut composer, &show, 1.0, true);
        }

        // The claim first: chrome stays on top of the flight. Without the
        // chrome elevation the flying pair (both ends are layer roots) paints
        // over the bar instead — this is what fails pre-feature.
        let (probe_x, probe_y) = (150.0, 40.0);
        let p_at_probe = flight_probe(&composer).map(|(p, _)| p).unwrap_or(1.0);
        let mut covered = render_heads(&composer);
        let c_cov = pixel_rgb(&mut covered, probe_x as i32, probe_y as i32);
        assert!(
            c_cov.1 > 180 && c_cov.2 < 80,
            "chrome stays on top of both ends, got {c_cov:?}"
        );
        // The one thing chrome cannot hide is the leaving ghost's own opacity:
        // the ghost is a layer root too, but at z 0 the bar is drawn after it,
        // so what remains is a wash bounded by (1 - p) over the bar's colour.
        let wash = 255.0 * (1.0 - p_at_probe);
        assert!(
            c_cov.0 as f32 <= wash + 45.0,
            "at most the ghost wash (1-p) shows over the bar, got red={} at p={p_at_probe}",
            c_cov.0
        );

        // Paint order = hit order: the bar is painted last, so a tap inside the
        // overlap must reach the BAR, not the hero flying under it (a pinned
        // bar's buttons would otherwise be dead for the whole flight).
        {
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("root");
            let path = hit_test_with_flights(
                nodes,
                root,
                composer.transition_roots(),
                probe_x,
                probe_y,
            );
            assert!(
                path.contains(&bar),
                "tap in the overlap must reach the chrome, got {path:?}"
            );
        }

        // Then the mechanism.
        assert!(
            composer.arena_nodes()[bar].in_scope_overlay,
            "chrome elevates while the scope is transitioning"
        );
        assert!(
            composer.transition_roots().contains(&bar),
            "chrome joins the layer"
        );

        // Same frame, one thing removed: without the elevation the flight wins
        // again — on screen AND for input.
        composer.arena.nodes[bar].in_scope_overlay = false;
        composer.scope_overlay_roots.clear();
        composer.rebuild_layer_order();
        let mut bare = render_heads(&composer);
        let c_bare = pixel_rgb(&mut bare, probe_x as i32, probe_y as i32);
        assert!(
            c_bare.2 > 120,
            "without the chrome elevation the flight covers the bar, got {c_bare:?}"
        );
        {
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("root");
            let path = hit_test_with_flights(
                nodes,
                root,
                composer.transition_roots(),
                probe_x,
                probe_y,
            );
            assert!(
                !path.contains(&bar),
                "unelevated chrome gets no special hit routing, got {path:?}"
            );
        }

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            chrome_frame(&mut composer, &show, 1.0, true);
        }
        assert!(composer.shared_flights.is_empty(), "flight completes");
        crate::animation::clear_all_animations();
    }

    /// The elevation lasts exactly as long as the scope is transitioning —
    /// outside a flight the chrome is ordinary tree content again.
    #[test]
    fn scope_overlay_chrome_follows_the_transition_window() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        chrome_frame(&mut composer, &show, 1.0, false);
        let bar = chrome_bar_index(&composer);
        assert!(!composer.arena_nodes()[bar].in_scope_overlay, "idle: in tree");
        assert!(
            !composer.transition_roots().contains(&bar),
            "idle: not a layer root"
        );
        assert!(composer.transition_roots().is_empty(), "no layer when idle");

        // ...and after the flight finishes it goes back to the tree.
        show.set(false);
        chrome_frame(&mut composer, &show, 1.0, false);
        assert!(composer.arena_nodes()[bar].in_scope_overlay, "flying: elevated");
        for _ in 0..300 {
            if composer.shared_flights.is_empty() {
                break;
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            chrome_frame(&mut composer, &show, 1.0, false);
        }
        assert!(composer.shared_flights.is_empty(), "flight completes");
        // Membership is decided by the poll, so the settled state is the NEXT
        // frame's (the completing frame still draws the previous decision —
        // same pixels, just a different pass).
        chrome_frame(&mut composer, &show, 1.0, false);
        let bar = chrome_bar_index(&composer);
        assert!(
            !composer.arena_nodes()[bar].in_scope_overlay,
            "settled: back in tree order"
        );
        crate::animation::clear_all_animations();
    }

    /// `z_index` orders the chrome against the flights (Compose
    /// `zIndexInOverlay`): shared endpoints default to 0, so a bar on top
    /// passes a larger value and one that wants to stay under passes a
    /// smaller one.
    #[test]
    fn scope_overlay_z_orders_chrome_against_flights() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        for (chrome_z, expect_bar_last) in [(-1.0f32, false), (1.0, true)] {
            let mut composer = Composer::new();
            let show = State::new(true);
            chrome_frame(&mut composer, &show, chrome_z, false);
            show.set(false);
            chrome_frame(&mut composer, &show, chrome_z, false);
            assert_eq!(composer.shared_flights.len(), 1, "flight opened");
            let bar = chrome_bar_index(&composer);
            let order = composer.transition_roots().to_vec();
            assert_eq!(
                order.last().copied() == Some(bar),
                expect_bar_last,
                "chrome z={chrome_z} in layer order {order:?}"
            );
            crate::animation::clear_all_animations();
        }
    }

    /// Chrome in a PEER composer (the case a composer's own poll cannot decide:
    /// a Tier1 flight lives in the MAIN map, so the dialog composer's
    /// `shared_flights` is empty). The window-level cross-poll pushes the union.
    #[test]
    fn scope_overlay_chrome_elevates_in_a_peer_composer() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(99);
        let show_a = State::new(true);
        let show_b = State::new(false);

        let sa = show_a.clone();
        let sb = show_b.clone();
        let (sca, scb) = (scope.clone(), scope.clone());
        // Owner: the leaving hero. Peer (dialog): the arriving hero plus a
        // pinned bar marked for the same scope.
        let frame = |a: &mut Composer, b: &mut Composer| {
            let (x, y) = (sa.clone(), sb.clone());
            let (ka, kb) = (sca.clone(), scb.clone());
            a.compose(|ctx| {
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if x.get() {
                        hero_leaf(ctx, 60.0, 60.0, Color::RED, &ka, true);
                    }
                });
            });
            b.compose(|ctx| {
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if y.get() {
                        Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                            hero_leaf(ctx, 300.0, 160.0, Color::BLUE, &kb, true);
                            Column::new()
                                .modifier(
                                    Modifier::new()
                                        .size(400.0, 60.0)
                                        .background(Color::GREEN, Shape::Rectangle)
                                        .render_in_shared_transition_scope_overlay(&kb, 1.0),
                                )
                                .build(ctx, |_| {});
                        });
                    }
                });
            });
            a.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            a.poll_shared_flights();
            b.poll_shared_flights();
            let mut all: Vec<&mut Composer> = vec![a, b];
            Composer::poll_cross_flights(&mut all);
        };

        frame(&mut a, &mut b);
        show_a.set(false);
        show_b.set(true);
        frame(&mut a, &mut b);
        assert_eq!(a.shared_flights.len(), 1, "Tier1 flight opened");
        assert!(b.shared_flights.is_empty(), "peer owns no flight map entry");

        let bar = b
            .arena_nodes()
            .iter()
            .position(|n| find_scope_overlay_marker(&n.modifier).is_some())
            .expect("peer chrome bar");
        assert!(
            b.arena_nodes()[bar].in_scope_overlay,
            "peer chrome must elevate for a flight it does not own"
        );
        assert!(
            b.transition_roots().contains(&bar),
            "peer chrome joins the peer's layer"
        );

        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            frame(&mut a, &mut b);
        }
        assert!(a.shared_flights.is_empty(), "Tier1 completes");
        frame(&mut a, &mut b);
        let bar = b
            .arena_nodes()
            .iter()
            .position(|n| find_scope_overlay_marker(&n.modifier).is_some())
            .expect("peer chrome bar");
        assert!(
            !b.arena_nodes()[bar].in_scope_overlay,
            "peer chrome returns to tree order when the flight ends"
        );
        crate::animation::clear_all_animations();
    }

    /// Equal-z ties are decided by insertion order, and that order is part of
    /// the contract: the entering target sits under its ghost (pre-overlay
    /// compositing) and chrome sits under nothing (pinned chrome wins ties).
    #[test]
    fn scope_overlay_equal_z_order_is_target_then_ghost_then_chrome() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        chrome_frame(&mut composer, &show, 0.0, true);
        show.set(false);
        chrome_frame(&mut composer, &show, 0.0, true);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");

        let bar = chrome_bar_index(&composer);
        let nodes = composer.arena_nodes();
        let order = composer.transition_roots().to_vec();
        let pos = |idx: usize| order.iter().position(|&i| i == idx);
        let bar_at = pos(bar).expect("chrome in the layer");
        assert_eq!(
            bar_at,
            order.len() - 1,
            "chrome is painted last at equal z, got {order:?}"
        );
        let target = composer
            .arena_nodes()
            .iter()
            .position(|n| {
                n.transition
                    .as_ref()
                    .is_some_and(|t| t.role == TransitionRole::Target && t.elevated)
            })
            .expect("elevated target");
        assert!(
            pos(target).expect("target in the layer") < bar_at,
            "the entering end sits under the ghost and the chrome, got {order:?}"
        );
        let _ = nodes;
        crate::animation::clear_all_animations();
    }

    /// The `Morph` exemption: a same-screen resize marked `in_overlay = true`
    /// stays in the tree — it is not a cross-composable flight, so escaping its
    /// container would be a regression, not a feature.
    #[test]
    fn morph_never_elevates_even_with_the_flag_on() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let w = State::new(80.0f32);
        let morph_frame = |composer: &mut Composer| {
            let ww = w.clone();
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .size(ww.get(), 60.0)
                                    .background(Color::BLUE, Shape::rounded(8.0))
                                    .shared_element(
                                        scope.shared_content_state("hero"),
                                        BoundsTransform::default(),
                                        PlaceHolderSize::JumpCut,
                                        PathMotion::Linear,
                                        0.0,
                                        true,
                                    ),
                            )
                            .build(ctx, |_| {});
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        morph_frame(&mut composer);
        w.set(200.0);
        morph_frame(&mut composer);
        assert_eq!(composer.shared_flights.len(), 1, "morph flight opened");
        let idx = marked_in(&composer)[0];
        let vis = composer.arena_nodes()[idx].transition.clone().expect("morph visual");
        assert_eq!(vis.role, TransitionRole::Morph, "same-screen resize");
        assert!(!vis.elevated, "a morph must stay in-tree");
        assert!(
            !composer.transition_roots().contains(&idx),
            "a morph must not join the layer"
        );
        crate::animation::clear_all_animations();
    }

    /// The in-tree SKIP is what keeps a layer root from being painted twice.
    /// An opaque bar hides a double draw, so this uses a translucent one: the
    /// composite over the white background tells one draw from two.
    #[test]
    fn layered_chrome_is_not_painted_twice() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let translucent = |composer: &mut Composer, show: &State<bool>| {
            let s = show.clone();
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_size())
                            .build(ctx, |ctx| {
                                if s.get() {
                                    under_bar_list(ctx, &scope, true);
                                } else {
                                    under_bar_detail(ctx, &scope, true);
                                }
                            });
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .size(400.0, 60.0)
                                    .background(
                                        Color { r: 0, g: 255, b: 0, a: 128 },
                                        Shape::Rectangle,
                                    )
                                    .render_in_shared_transition_scope_overlay(&scope, 1.0),
                            )
                            .build(ctx, |_| {});
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        translucent(&mut composer, &show);
        show.set(false);
        translucent(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");

        // Inside the bar, clear of the flight's lerped rect (which stays left).
        let (px, py) = (370i32, 30i32);
        let mut surf = render_heads(&composer);
        let c = pixel_rgb(&mut surf, px, py);
        // One translucent green over white: 0.5*green + 0.5*white.
        assert!(
            c.0 > 100 && c.2 > 100,
            "a single draw leaves the background showing through, got {c:?}"
        );
        assert!(
            !close_enough(c, (63, 255, 63), 20),
            "the bar must not be composited twice, got {c:?}"
        );
        crate::animation::clear_all_animations();
    }

    /// Bouncy hero leaf (spring overshoot must render past the end rect).
    fn spring_hero_leaf(
        ctx: &mut ComposeCtx,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
        path: PathMotion,
    ) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(
                    scope.shared_content_state("hero"),
                    BoundsTransform::spring(SpringSpec::bouncy()),
                    PlaceHolderSize::JumpCut,
                    path,
                    0.0,
                    true,
                ),
        );
        ctx.end_node();
    }

    #[crate::composable]
    fn spring_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, path: PathMotion) {
        spring_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, path);
    }

    #[crate::composable]
    fn spring_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, path: PathMotion) {
        gap_leaf(ctx, 400.0, 100.0);
        spring_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, path);
    }

    /// App-loop step for the bouncy screens.
    fn spring_frame(composer: &mut Composer, show: &State<bool>, path: PathMotion) {
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if show.get() {
                        spring_list_screen(ctx, &scope, path);
                    } else {
                        spring_detail_screen(ctx, &scope, path);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn spring_advance(composer: &mut Composer, show: &State<bool>, path: PathMotion) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        spring_frame(composer, show, path);
    }

    #[test]
    fn tier0_bouncy_spring_overshoot_renders_then_settles() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        spring_frame(&mut composer, &show, PathMotion::Linear);
        show.set(false);
        spring_frame(&mut composer, &show, PathMotion::Linear);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");

        // The old `p >= 0.999` gate reaped the flight on first passage —
        // before the overshoot peak — so no live frame ever exceeded 1.0.
        let mut max_p = 0.0f32;
        let mut overshoot_painted = false;
        for _ in 0..400 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let probe = composer.shared_flights.values().next().map(|a| {
                let p = a.progress.peek();
                let e = a.flight.end.expect("end resolved after first poll");
                (p, a.start.lerp(&e, p))
            });
            if let Some((p, l)) = probe {
                max_p = max_p.max(p);
                // First live frame past the end: the target (alpha clamped
                // to 1, source gone) paints the overshot rect — its center
                // must read BLUE, proving paint follows the overshoot.
                if p > 1.0 && !overshoot_painted {
                    let mut surf = render_heads(&composer);
                    let cx = (l.x + l.width / 2.0) as i32;
                    let cy = (l.y + l.height / 2.0) as i32;
                    let c = pixel_rgb(&mut surf, cx, cy);
                    assert!(
                        c.2 > 150 && c.0 < 120,
                        "overshot rect paints the arrived target, got {c:?} at p={p}"
                    );
                    overshoot_painted = true;
                }
            }
            spring_advance(&mut composer, &show, PathMotion::Linear);
        }
        assert!(max_p > 1.0, "bouncy spring overshoots past 1.0 while alive, got {max_p}");
        assert!(overshoot_painted, "overshoot frames must render before settle");
        assert!(composer.shared_flights.is_empty(), "flight settles after overshoot");
        assert!(composer.transition_layer.is_empty(), "retained source freed");
        // Settle lands exactly on the detail hero.
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let (ex, ey) = node_center(&composer, marked[0]);
        let mut surf = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
            "settled end state shows the detail hero"
        );
        crate::animation::clear_all_animations();
    }

    /// Small arc hero (40px): the bend (≈43px) exceeds the half-size, so
    /// edge probes discriminate arc paint from straight paint.
    fn arc_hero_leaf(ctx: &mut ComposeCtx, w: f32, h: f32, color: Color, scope: &SharedTransitionScope) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(
                    scope.shared_content_state("hero"),
                    BoundsTransform::spring(SpringSpec::bouncy()),
                    PlaceHolderSize::JumpCut,
                    PathMotion::ArcBelow,
                    0.0,
                    true,
                ),
        );
        ctx.end_node();
    }

    #[crate::composable]
    fn arc_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        Column::new().build(ctx, |ctx| {
            gap_leaf(ctx, 400.0, 40.0);
            Row::new().build(ctx, |ctx| {
                arc_hero_leaf(ctx, 40.0, 40.0, Color::RED, scope);
                gap_leaf(ctx, 260.0, 40.0);
            });
        });
    }

    #[crate::composable]
    fn arc_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        Column::new().build(ctx, |ctx| {
            gap_leaf(ctx, 400.0, 140.0);
            Row::new().build(ctx, |ctx| {
                gap_leaf(ctx, 260.0, 40.0);
                arc_hero_leaf(ctx, 40.0, 40.0, Color::BLUE, scope);
            });
        });
    }

    fn arc_frame(composer: &mut Composer, show: &State<bool>) {
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if show.get() {
                        arc_list_screen(ctx, &scope);
                    } else {
                        arc_detail_screen(ctx, &scope);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn arc_advance(composer: &mut Composer, show: &State<bool>) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        arc_frame(composer, show);
    }

    #[test]
    fn tier0_arc_flight_paints_off_the_straight_line() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        arc_frame(&mut composer, &show);
        show.set(false);
        arc_frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "arc flight opened");
        {
            let a = composer.shared_flights.values().next().expect("flight");
            assert_eq!(a.path, PathMotion::ArcBelow, "path resolves from the target marker");
        }

        // Diagonal travel (260px + 100px centers) bends substantially at
        // mid. The hero is only 40px, so probes discriminate paint: the
        // arc's outer edge is inside arc paint but outside straight paint
        // (and vice versa). A render path silently painting straight fails
        // both probes.
        let mut bent = false;
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let probe = composer.shared_flights.values().next().map(|a| {
                let p = a.progress.peek();
                let e = a.flight.end.expect("end resolved after first poll");
                let straight = a.start.lerp(&e, p);
                let nodes = composer.arena_nodes();
                let tslot = a.flight.target_slot.expect("target");
                let root = composer.layout_root_idx().expect("root");
                let tidx = find_idx_by_slot(nodes, root, tslot).expect("target");
                let arced = nodes[tidx].transition.as_ref().expect("visual").lerped();
                (p, straight, arced)
            });
            if let Some((p, straight, arced)) = probe {
                if p > 0.4 && p < 0.6 {
                    let (scx, scy) = (
                        straight.x + straight.width / 2.0,
                        straight.y + straight.height / 2.0,
                    );
                    let (acx, acy) = (
                        arced.x + arced.width / 2.0,
                        arced.y + arced.height / 2.0,
                    );
                    let (dx, dy) = (acx - scx, acy - scy);
                    let dev = (dx * dx + dy * dy).sqrt();
                    assert!(
                        dev > 20.0,
                        "arc bends off the straight line, deviation {dev} at p={p}"
                    );
                    // Unit bend direction, 8px inside the paint edge (clear of
                    // the 8px rounded corners at the edge middle).
                    let (ux, uy) = (dx / dev, dy / dev);
                    let m = arced.height / 2.0 - 8.0;
                    let mut surf = render_heads(&composer);
                    let outer = pixel_rgb(
                        &mut surf,
                        (acx + ux * m) as i32,
                        (acy + uy * m) as i32,
                    );
                    assert!(
                        outer.0 > 140 && outer.1 < 150 && outer.2 > 50,
                        "arc outer edge paints hero blend, got {outer:?} at p={p}"
                    );
                    let mut surf2 = render_heads(&composer);
                    let inner = pixel_rgb(
                        &mut surf2,
                        (scx - ux * m) as i32,
                        (scy - uy * m) as i32,
                    );
                    assert!(
                        close_enough(inner, (255, 255, 255), 40),
                        "straight-side mirror is background, got {inner:?} at p={p}"
                    );
                    bent = true;
                    break;
                }
            }
            arc_advance(&mut composer, &show);
        }
        assert!(bent, "flight must pass through the bend window");

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            arc_advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "arc flight completes");
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        // Absolute center (node_center is layout-relative — the hero nests
        // inside a Row here).
        let root = composer.layout_root_idx().expect("root");
        let nid = composer.arena_nodes()[marked[0]].id;
        let (ax, ay) = crate::app::node_abs_position(composer.arena_nodes(), root, nid);
        let (ex, ey) = (
            (ax + composer.arena_nodes()[marked[0]].measured_size.width / 2.0) as i32,
            (ay + composer.arena_nodes()[marked[0]].measured_size.height / 2.0) as i32,
        );
        let mut surf = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
            "settled end state shows the detail hero"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn ancestor_scroll_sum_accumulates_nested_scrollers() {
        use crate::layout::node::LayoutNode;
        let outer_v = crate::modifier::ScrollState::new();
        let inner_v = crate::modifier::ScrollState::new();
        let inner_h = crate::modifier::ScrollState::new();
        outer_v.offset.set(25.0);
        inner_v.offset.set(15.0);
        inner_h.offset.set(10.0);
        let root = LayoutNode::leaf(Modifier::new());
        let mut outer = LayoutNode::leaf(Modifier::new().vertical_scroll(outer_v));
        let mut inner = LayoutNode::leaf(
            Modifier::new().vertical_scroll(inner_v).horizontal_scroll(inner_h),
        );
        let mut leaf = LayoutNode::leaf(Modifier::new());
        let (rid, oid, iid) = (root.id, outer.id, inner.id);
        outer.parent_id = Some(rid);
        inner.parent_id = Some(oid);
        leaf.parent_id = Some(iid);
        let nodes = vec![root, outer, inner, leaf];
        let id_to_idx: HashMap<u64, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        // Canvas nests additively: 25 + 15 vertical, 10 horizontal.
        assert_eq!(ancestor_scroll_sum(&nodes, &id_to_idx, 3), (10.0, 40.0));
        // The node's own offset never counts (only strict ancestors).
        assert_eq!(ancestor_scroll_sum(&nodes, &id_to_idx, 2), (0.0, 25.0));
        // Rootless (detached) nodes sum to zero.
        assert_eq!(ancestor_scroll_sum(&nodes, &id_to_idx, 0), (0.0, 0.0));
    }

    #[test]
    fn scope_is_transition_active_tracks_flight() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        clear_scope_active_states();
        let mut composer = Composer::new();
        let show = State::new(true);

        frame(&mut composer, &show);
        show.set(false);
        frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");
        // Same global entry the coordinator syncs (keyed by scope_id).
        // Created on first read (false) — the next poll flips it, which is
        // exactly the subscription semantics users observe in composition.
        let scope_id = composer.shared_flights.values().next().expect("flight").flight.scope_id;
        let active = SharedTransitionScope::new(scope_id).is_transition_active();
        advance(&mut composer, &show);
        assert!(active.get(), "flag true while the flight is non-terminal");

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "flight completes");
        assert!(!active.get(), "flag false after teardown");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier0_flight_pivot_alignment_mid_flight() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        frame(&mut composer, &show);
        show.set(false);
        frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");

        // Drive into a mid-flight window where a pivot error is unambiguous:
        // the lerped rect must leave a clear margin above it. The old
        // translate-then-scale order scaled about the canvas origin, shifting
        // paint by pos*(scale-1) — the target painted ~25px too high here.
        let mut aligned = false;
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let probe = composer.shared_flights.values().next().map(|a| {
                let p = a.progress.peek();
                let e = a.flight.end.expect("end resolved after first poll");
                (p, a.start.lerp(&e, p))
            });
            if let Some((p, l)) = probe {
                if p > 0.3 && p < 0.7 && l.height < 128.0 && l.y > 24.0 {
                    // Lerped center must show blended hero paint (both ends
                    // coincide here by construction).
                    let mut surf = render_heads(&composer);
                    let cx = (l.x + l.width / 2.0) as i32;
                    let cy = (l.y + l.height / 2.0) as i32;
                    let c = pixel_rgb(&mut surf, cx, cy);
                    assert!(
                        c.0 > 140 && c.2 > 50,
                        "lerped center shows blended flight paint, got {c:?}"
                    );
                    // Above the lerped top must be background: with the pivot
                    // bug the target painted into this strip.
                    let mut surf2 = render_heads(&composer);
                    let above = pixel_rgb(&mut surf2, cx, (l.y - 12.0) as i32);
                    assert!(
                        close_enough(above, (255, 255, 255), 40),
                        "no flight paint above the lerped rect, got {above:?}"
                    );
                    aligned = true;
                    break;
                }
            }
            advance(&mut composer, &show);
        }
        assert!(aligned, "flight must pass through the alignment window");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier0_retarget_restarts_from_visual() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        frame(&mut composer, &show);
        show.set(false);
        frame(&mut composer, &show);
        for _ in 0..3 {
            advance(&mut composer, &show);
        }
        assert_eq!(composer.shared_flights.len(), 1, "mid-flight");
        // Navigate back mid-flight: old flight cancelled, exactly one fresh
        // flight opens, exactly one node retained (no leak, no double).
        show.set(true);
        frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "retarget replaces, never duplicates");
        assert_eq!(composer.transition_layer.len(), 1, "old retained freed, new retained");

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "retargeted flight completes");
        assert!(composer.transition_layer.is_empty());
        // Back on the list: RED hero at its natural spot.
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let (ex, ey) = node_center(&composer, marked[0]);
        let mut surf = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (255, 0, 0), 30),
            "navigated back to the list hero"
        );
        crate::animation::clear_all_animations();
    }

    // ── Phase 3 tests ──

    use crate::layout::node::{hit_test, hit_test_with_flights};

    /// Marked box with layout-driven width (registers a LAYOUT dep only —
    /// size changes remeasure without recomposition, mirroring the app loop).
    fn morph_leaf(ctx: &mut ComposeCtx, w: &State<f32>, scope: &SharedTransitionScope) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, 80.0)
                .background(Color::GREEN, Shape::Rectangle)
                .shared_element(scope.shared_content_state("morph"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
        );
        ctx.end_node();
    }

    #[crate::composable]
    fn morph_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
        morph_leaf(ctx, w, scope);
    }

    fn morph_compose(composer: &mut Composer, w: &State<f32>) {
        let ww = w.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    morph_screen(ctx, &scope, &ww);
                });
            });
        });
    }

    /// Layout-only advance (no compose — mirrors the app loop when no compose
    /// deps fire; proves morphs need zero recomposition).
    fn advance_layout_only(composer: &mut Composer) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    #[test]
    fn morph_size_change_flies_without_slot_churn() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let w = State::new(120.0f32);

        morph_compose(&mut composer, &w);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
        assert!(composer.shared_flights.is_empty(), "steady size opens nothing");
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let slot = composer.arena_nodes()[marked[0]].slot_key;

        // Layout-only size change (no compose call at all).
        w.set(300.0);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
        assert_eq!(composer.shared_flights.len(), 1, "size delta opens a morph flight");
        let fid = *composer.shared_flights.keys().next().unwrap();
        {
            let a = &composer.shared_flights[&fid];
            assert!(a.source_idx.is_none(), "morph detaches nothing");
            assert_eq!((a.flight.source_slot, a.flight.target_slot), (Some(slot), Some(slot)));
            assert_eq!(a.flight.phase, FlightPhase::Flying);
        }
        // Same slot, Morph role, opacity untouched.
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        assert_eq!(composer.arena_nodes()[marked[0]].slot_key, slot, "no slot churn");
        let vis = composer.arena_nodes()[marked[0]].transition.clone().expect("morph visuals");
        assert_eq!(vis.role, TransitionRole::Morph);
        assert_eq!(vis.alpha(), 1.0);

        // Run to completion with layout-only advances (zero compose calls).
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            advance_layout_only(&mut composer);
        }
        assert!(composer.shared_flights.is_empty(), "morph completes");
        for idx in marked_indices(&composer) {
            assert!(composer.arena_nodes()[idx].transition.is_none(), "visuals cleared");
        }
        // End paint: 300-wide GREEN covers x=290.
        let mut surf = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf, 290, 40), (0, 255, 0), 30),
            "grown box paints at the new size"
        );
        crate::animation::clear_all_animations();
    }

    /// Marked container with a plain child (container morphs must keep
    /// their subtree hittable mid-flight — MAJOR #3).
    #[crate::composable]
    fn morph_parent_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
        Column::new()
            .modifier(
                Modifier::new()
                    .size(w, 80.0)
                    .background(Color::GREEN, Shape::Rectangle)
                    .shared_element(scope.shared_content_state("box"), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
            )
            .build(ctx, |ctx| {
                gap_leaf(ctx, 50.0, 50.0);
            });
    }

    fn morph_parent_compose(composer: &mut Composer, w: &State<f32>) {
        let ww = w.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    morph_parent_screen(ctx, &scope, &ww);
                });
            });
        });
    }

    #[test]
    fn morph_container_child_hittable_mid_flight() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let w = State::new(120.0f32);

        morph_parent_compose(&mut composer, &w);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
        assert!(composer.shared_flights.is_empty(), "steady size opens nothing");

        // Layout-only size change (no compose call at all).
        w.set(300.0);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
        assert_eq!(composer.shared_flights.len(), 1, "size delta opens a morph flight");

        // Tap the plain child through the flight transform: at progress ~0
        // the 300-wide layout is scaled 0.4x into the 120-wide lerped rect,
        // so visual (10, 25) maps to layout (25, 25) — the gap child's heart.
        let nodes = composer.arena_nodes();
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let container = marked[0];
        assert_eq!(
            nodes[container].transition.as_ref().map(|t| t.role),
            Some(TransitionRole::Morph),
            "container carries the morph visual"
        );
        let child = nodes[container].children[0];
        let root = composer.layout_root_idx().expect("root");
        let path = hit_test_with_flights(nodes, root, composer.transition_roots(), 10.0, 25.0);
        assert_eq!(
            path.last().copied(),
            Some(child),
            "tap descends into the morphing container's child, got {path:?}"
        );
        crate::animation::clear_all_animations();
    }

    /// Keyed hero leaf (morph baselines key on (scope, key) — MINOR #5).
    fn keyed_hero_leaf(
        ctx: &mut ComposeCtx,
        key: &str,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
    ) {
        let slot = ctx.next_key();
        ctx.start_leaf(
            slot,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(scope.shared_content_state(key), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
        );
        ctx.end_node();
    }

    /// Data-driven key at ONE statement (same slot, different marker key —
    /// morph baselines must key on (scope, key), MINOR #5).
    #[crate::composable]
    fn keyed_item_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, use_a: &State<bool>) {
        let (k, w, h, c) = if use_a.get() {
            ("ka", 120.0, 80.0, Color::RED)
        } else {
            ("kb", 300.0, 160.0, Color::BLUE)
        };
        keyed_hero_leaf(ctx, k, w, h, c, scope);
    }

    fn keyed_frame(composer: &mut Composer, show: &State<bool>) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    keyed_item_screen(ctx, &scope, &s);
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    #[test]
    fn morph_baseline_keys_on_endpoint_identity() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        // Settle key "ka" (writes its baseline).
        keyed_frame(&mut composer, &show);
        keyed_frame(&mut composer, &show);
        assert!(composer.shared_flights.is_empty());
        let slot_a = composer.arena_nodes()[marked_indices(&composer)[0]].slot_key;

        // Swap to a DIFFERENT key at the same call-site slot with a
        // different size. Slot-keyed baselines would seed a spurious morph
        // from "ka"'s rect; identity-keyed baselines start clean.
        show.set(false);
        keyed_frame(&mut composer, &show);
        let slot_b = composer.arena_nodes()[marked_indices(&composer)[0]].slot_key;
        assert_eq!(slot_a, slot_b, "same call-site slot reused across the key swap (test premise)");
        assert!(
            composer.shared_flights.is_empty(),
            "key swap opens no morph flight"
        );
        for idx in marked_indices(&composer) {
            assert!(
                composer.arena_nodes()[idx].transition.is_none(),
                "fresh key carries no morph visual"
            );
        }
        crate::animation::clear_all_animations();
    }

    /// z-ordered hero leaf (retained ghosts sort back-to-front by this).
    fn z_hero_leaf(
        ctx: &mut ComposeCtx,
        key: &str,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
        z: f32,
    ) {
        let slot = ctx.next_key();
        ctx.start_leaf(
            slot,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(
                    scope.shared_content_state(key),
                    BoundsTransform::default(),
                    PlaceHolderSize::JumpCut,
                    PathMotion::Linear,
                    z,
                    true,
                ),
        );
        ctx.end_node();
    }

    #[crate::composable]
    fn z_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        // k1 carries the higher z but composes FIRST — detach order must
        // not decide paint order.
        z_hero_leaf(ctx, "k1", 120.0, 80.0, Color::RED, scope, 1.0);
        z_hero_leaf(ctx, "k2", 120.0, 80.0, Color::GREEN, scope, 0.0);
    }

    #[crate::composable]
    fn z_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        z_hero_leaf(ctx, "k1", 300.0, 160.0, Color::BLUE, scope, 1.0);
        z_hero_leaf(ctx, "k2", 300.0, 160.0, Color::BLUE, scope, 0.0);
    }

    fn z_frame(composer: &mut Composer, show: &State<bool>) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        z_list_screen(ctx, &scope);
                    } else {
                        z_detail_screen(ctx, &scope);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    #[test]
    fn retained_ghosts_sort_back_to_front_by_z() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        z_frame(&mut composer, &show);
        z_frame(&mut composer, &show);
        show.set(false);
        z_frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 2, "one flight per key");
        assert_eq!(composer.transition_layer.len(), 2, "both sources retained");

        // Key per retained slot, from the flights (detach order is a
        // HashMap iteration — assert order, not identity sequence).
        let key_of: HashMap<u64, String> = composer
            .shared_flights
            .values()
            .filter_map(|a| a.flight.source_slot.map(|s| (s, a.flight.key.clone())))
            .collect();
        let ordered: Vec<&str> = composer.transition_layer
            .iter()
            .map(|&idx| {
                key_of
                    .get(&composer.arena_nodes()[idx].slot_key)
                    .map(|s| s.as_str())
                    .unwrap_or("?")
            })
            .collect();
        assert_eq!(ordered, vec!["k2", "k1"], "ascending z paints k1 on top, got {ordered:?}");

        // The sort reads z lazily off the retained markers.
        for &idx in &composer.transition_layer {
            let z = find_shared_marker(&composer.arena_nodes()[idx].modifier)
                .expect("retained marker")
                .z_index;
            let key = &key_of[&composer.arena_nodes()[idx].slot_key];
            assert_eq!(z, if key == "k1" { 1.0 } else { 0.0 }, "z rides the retained node");
        }
        crate::animation::clear_all_animations();
    }

    /// sharedBounds hero (tween flight; enter slides from above, exit
    /// slides downward — both faded, Compose container-transform shape).
    /// Slide distance 160px: at p≈0.5 the ends sit ±80px off the lerped
    /// center (onscreen, non-overlapping — decisive probes).
    fn bounds_hero_leaf(
        ctx: &mut ComposeCtx,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
    ) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_bounds(
                    scope.shared_content_state("hero"),
                    VisibilityTransition::slide_in_offset(
                        SlideDirection::Up,
                        SlideOffset::Fixed(160.0),
                        TweenSpec::default(),
                    )
                    .with_fade(),
                    VisibilityTransition::slide_out_offset(
                        SlideDirection::Down,
                        SlideOffset::Fixed(160.0),
                        TweenSpec::default(),
                    )
                    .with_fade(),
                    BoundsTransform::default(),
                    ResizeMode::ScaleToBounds { clip: false },
                    PlaceHolderSize::JumpCut,
                    PathMotion::Linear,
                    0.0,
                    true,
                ),
        );
        ctx.end_node();
    }

    #[crate::composable]
    fn bounds_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        bounds_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
    }

    #[crate::composable]
    fn bounds_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        gap_leaf(ctx, 400.0, 100.0);
        bounds_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
    }

    fn bounds_frame(composer: &mut Composer, show: &State<bool>) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        bounds_list_screen(ctx, &scope);
                    } else {
                        bounds_detail_screen(ctx, &scope);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn bounds_advance(composer: &mut Composer, show: &State<bool>) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        bounds_frame(composer, show);
    }

    #[test]
    fn tier0_shared_bounds_enter_exit_slide_with_flight() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        bounds_frame(&mut composer, &show);
        show.set(false);
        bounds_frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "bounds flight opened");
        {
            let a = composer.shared_flights.values().next().expect("flight");
            let (enter, exit) = a.bounds_fx.clone().expect("bounds pair carried");
            assert!(enter.fade && exit.fade, "test pair is slide+fade");
            assert!(enter.slide.is_some() && exit.slide.is_some());
        }

        // Tween-linear flight: at p≈0.5 the target sits 80px above the
        // lerped center (slide-in from Up) and the ghost 80px below it
        // (slide-out Down) — three decisive probes, no overlap.
        let mut slid = false;
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let probe = composer.shared_flights.values().next().map(|a| {
                let p = a.progress.peek();
                let e = a.flight.end.expect("end resolved after first poll");
                let l = a.start.lerp(&e, p);
                ((l.x + l.width / 2.0) as i32, (l.y + l.height / 2.0) as i32, p)
            });
            if let Some((cx, cy, p)) = probe {
                if p > 0.45 && p < 0.55 {
                    // Target paint (faded blue) 80px above the lerped center.
                    let mut surf = render_heads(&composer);
                    let up = pixel_rgb(&mut surf, cx, cy - 80);
                    assert!(
                        up.2 > 150 && up.0 < 150,
                        "enter slide lifts target paint above, got {up:?} at p={p}"
                    );
                    // Ghost paint (faded red) 80px below it.
                    let mut surf2 = render_heads(&composer);
                    let down = pixel_rgb(&mut surf2, cx, cy + 80);
                    assert!(
                        down.0 > 200 && down.2 < 200,
                        "exit slide drops ghost paint below, got {down:?} at p={p}"
                    );
                    // The lerped center itself is background (both moved away).
                    let mut surf3 = render_heads(&composer);
                    let mid = pixel_rgb(&mut surf3, cx, cy);
                    assert!(
                        close_enough(mid, (255, 255, 255), 40),
                        "lerped center vacated by both slides, got {mid:?} at p={p}"
                    );
                    slid = true;
                    break;
                }
            }
            bounds_advance(&mut composer, &show);
        }
        assert!(slid, "flight must pass through the slide window");

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            bounds_advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "bounds flight completes");
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let (ex, ey) = node_center(&composer, marked[0]);
        let mut surf = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
            "settled end state shows the detail hero"
        );
        crate::animation::clear_all_animations();
    }

    /// sharedBounds hero with expand enter/exit (wipe-in from the top
    /// edge, wipe-out toward it — both faded).
    fn expand_hero_leaf(
        ctx: &mut ComposeCtx,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
    ) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_bounds(
                    scope.shared_content_state("hero"),
                    VisibilityTransition::expand_in(TweenSpec::default()).with_fade(),
                    VisibilityTransition::shrink_out(TweenSpec::default()).with_fade(),
                    BoundsTransform::default(),
                    ResizeMode::ScaleToBounds { clip: false },
                    PlaceHolderSize::JumpCut,
                    PathMotion::Linear,
                    0.0,
                    true,
                ),
        );
        ctx.end_node();
    }

    #[crate::composable]
    fn expand_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        expand_hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
    }

    #[crate::composable]
    fn expand_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        gap_leaf(ctx, 400.0, 100.0);
        expand_hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
    }

    fn expand_frame(composer: &mut Composer, show: &State<bool>) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        expand_list_screen(ctx, &scope);
                    } else {
                        expand_detail_screen(ctx, &scope);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    }

    fn expand_advance(composer: &mut Composer, show: &State<bool>) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        expand_frame(composer, show);
    }

    #[test]
    fn tier0_shared_bounds_expand_wipes_from_edge() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        expand_frame(&mut composer, &show);
        show.set(false);
        expand_frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "expand flight opened");

        // Tween-linear flight at p≈0.5: the target covers the TOP half of
        // the lerped rect (grown down from the top edge); the bottom half
        // is background. Ghost mirrors from the same edge.
        let mut wiped = false;
        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            let probe = composer.shared_flights.values().next().map(|a| {
                let p = a.progress.peek();
                let e = a.flight.end.expect("end resolved after first poll");
                let l = a.start.lerp(&e, p);
                (p, (l.x + l.width / 2.0) as i32, l.y as i32, l.height)
            });
            if let Some((p, cx, top, h)) = probe {
                if p > 0.45 && p < 0.55 {
                    // Just inside the top edge: grown paint — ghost red over
                    // target blue (both wipe from the top edge, both faded).
                    let mut surf = render_heads(&composer);
                    let grown = pixel_rgb(&mut surf, cx, top + 10);
                    assert!(
                        grown.0 > 140 && grown.1 < 120 && grown.2 > 100,
                        "top edge shows grown paint, got {grown:?} at p={p}"
                    );
                    // Just inside the bottom edge: not yet grown.
                    let mut surf2 = render_heads(&composer);
                    let pending = pixel_rgb(&mut surf2, cx, (top as f32 + h - 10.0) as i32);
                    assert!(
                        close_enough(pending, (255, 255, 255), 40),
                        "bottom edge still background, got {pending:?} at p={p}"
                    );
                    wiped = true;
                    break;
                }
            }
            expand_advance(&mut composer, &show);
        }
        assert!(wiped, "flight must pass through the wipe window");

        for _ in 0..200 {
            if composer.shared_flights.is_empty() {
                break;
            }
            expand_advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "expand flight completes");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn remap_hit_identity_endpoints_miss_outside() {
        let vis = TransitionVisual {
            start: SharedBounds::new(0.0, 0.0, 100.0, 50.0),
            end: SharedBounds::new(200.0, 0.0, 100.0, 50.0),
            progress: 0.0,
            role: TransitionRole::Target,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path: PathMotion::Linear,
            bounds_fx: None,
            elevated: false,
        };
        assert_eq!(
            vis.remap_hit(10.0, 10.0, 0.0, 0.0, 100.0, 50.0),
            Some((10.0, 10.0)),
            "p=0 remaps identically"
        );
        assert_eq!(
            vis.remap_hit(500.0, 500.0, 0.0, 0.0, 100.0, 50.0),
            None,
            "visual miss passes through"
        );
        let mut done = vis.clone();
        done.progress = 1.0;
        assert_eq!(
            done.remap_hit(210.0, 10.0, 0.0, 0.0, 100.0, 50.0),
            Some((10.0, 10.0)),
            "p=1 maps the end rect back into layout space"
        );
    }

    #[test]
    fn hit_routing_reaches_target_mid_flight() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        frame(&mut composer, &show);
        show.set(false);
        frame(&mut composer, &show);
        advance(&mut composer, &show);
        advance(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "mid-flight");

        // Click inside the lerped ghost but outside the target natural rect
        // (early flight: ghost ≈ start (0,0,120,80), target at (0,100,…)).
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().unwrap();
        let routed = hit_test_with_flights(nodes, root, &composer.transition_roots(), 60.0, 40.0);
        let plain = hit_test(nodes, root, 60.0, 40.0);
        // Target hero index for comparison.
        let target = marked_indices(&composer);
        assert_eq!(target.len(), 1);
        let target_idx = target[0];
        assert_eq!(
            routed.last().copied(),
            Some(target_idx),
            "ghost/target-visual click routes into the live target subtree"
        );
        // MAJOR #3: the legacy walk reaches the live target through its own
        // remap too — no input blackout on transitioning endpoints (the old
        // v1 skip deliberately murdered this; the ghost prefix above only
        // adds detached-source routing on top).
        assert_eq!(
            plain.last().copied(),
            Some(target_idx),
            "plain walk reaches the live target via its own visual remap"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier0_double_retarget_settles() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);

        frame(&mut composer, &show);
        show.set(false);
        frame(&mut composer, &show);
        for _ in 0..2 {
            advance(&mut composer, &show);
        }
        show.set(true);
        frame(&mut composer, &show);
        for _ in 0..2 {
            advance(&mut composer, &show);
        }
        show.set(false);
        frame(&mut composer, &show);
        assert_eq!(composer.shared_flights.len(), 1, "exactly one flight after double retarget");
        assert!(composer.transition_layer.len() <= 1, "never more than one retained node");

        for _ in 0..300 {
            if composer.shared_flights.is_empty() {
                break;
            }
            advance(&mut composer, &show);
        }
        assert!(composer.shared_flights.is_empty(), "settles");
        assert!(composer.transition_layer.is_empty(), "nothing retained");
        let marked = marked_indices(&composer);
        assert_eq!(marked.len(), 1);
        let (ex, ey) = node_center(&composer, marked[0]);
        let mut surf = render_heads(&composer);
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
            "ends on the detail hero"
        );
        crate::animation::clear_all_animations();
    }

    /// Morph box persistent across a screen switch + disappearing/appearing
    /// hero pair: a morph flight and a switch flight coexist, then both
    /// complete cleanly.
    #[crate::composable]
    fn mixed_list_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
        morph_leaf(ctx, w, scope);
        hero_leaf(ctx, 120.0, 80.0, Color::RED, scope, true);
    }

    #[crate::composable]
    fn mixed_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
        morph_leaf(ctx, w, scope);
        gap_leaf(ctx, 400.0, 100.0);
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
    }

    #[test]
    fn morph_and_switch_coexist() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let show = State::new(true);
        let w = State::new(120.0f32);

        let mixed_frame = |composer: &mut Composer| {
            let (s, ww) = (show.clone(), w.clone());
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        if s.get() {
                            mixed_list_screen(ctx, &scope, &ww);
                        } else {
                            mixed_detail_screen(ctx, &scope, &ww);
                        }
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        let mixed_advance = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            mixed_frame(composer);
        };

        mixed_frame(&mut composer);
        // Open a morph (layout-only size change, no compose).
        w.set(300.0);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
        assert_eq!(composer.shared_flights.len(), 1, "morph opens");
        mixed_advance(&mut composer);
        // Switch screens mid-morph: switch flight opens alongside.
        show.set(false);
        mixed_frame(&mut composer);
        assert_eq!(composer.shared_flights.len(), 2, "morph + switch coexist");

        for _ in 0..300 {
            if composer.shared_flights.is_empty() {
                break;
            }
            mixed_advance(&mut composer);
        }
        assert!(composer.shared_flights.is_empty(), "both complete");
        assert!(composer.transition_layer.is_empty(), "nothing retained");
        for idx in marked_indices(&composer) {
            assert!(composer.arena_nodes()[idx].transition.is_none(), "all visuals cleared");
        }
        crate::animation::clear_all_animations();
    }

    // ── Phase 4 Tier1 tests (two-composer harness: A = main, B = overlay) ──

    /// Column shell with caller content (keeps slot paths aligned frames).
    fn shell(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, content);
    }

    /// Two-composer app-loop mirror (same canvas): compose + layout + polls,
    /// then cross-poll. Contents are plain closures with no same-position
    /// swaps within one composer (test-fallback keys stay sound).
    fn cross_frame(
        a: &mut Composer,
        b: &mut Composer,
        ca: impl FnOnce(&mut ComposeCtx),
        cb: impl FnOnce(&mut ComposeCtx),
    ) {
        a.compose(ca);
        b.compose(cb);
        a.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        a.poll_shared_flights();
        b.poll_shared_flights();
        let mut all: Vec<&mut Composer> = vec![a, b];
        Composer::poll_cross_flights(&mut all);
    }

    /// List/detail pair across composers sharing one scope handle (production:
    /// the overlay inherits the scope via the CompositionLocal snapshot).
    fn xframe(
        a: &mut Composer,
        b: &mut Composer,
        show_a: &State<bool>,
        show_b: &State<bool>,
        scope: &SharedTransitionScope,
    ) {
        let (sa, sb) = (show_a.clone(), show_b.clone());
        let (sca, scb) = (scope.clone(), scope.clone());
        cross_frame(
            a,
            b,
            |ctx| shell(ctx, |ctx| {
                if sa.get() {
                    hero_leaf(ctx, 120.0, 80.0, Color::RED, &sca, true);
                }
            }),
            |ctx| shell(ctx, |ctx| {
                if sb.get() {
                    hero_leaf(ctx, 300.0, 160.0, Color::BLUE, &scb, true);
                }
            }),
        );
    }

    fn xadvance(
        a: &mut Composer,
        b: &mut Composer,
        show_a: &State<bool>,
        show_b: &State<bool>,
        scope: &SharedTransitionScope,
    ) {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        xframe(a, b, show_a, show_b, scope);
    }

    /// Render both arenas onto one surface (mirrors app: main pass, then the
    /// overlay pass translated by its origin).
    fn render_cross(a: &Composer, b: &Composer, b_origin: (f32, f32)) -> skia_safe::Surface {
        let mut surface = skia_safe::surfaces::raster_n32_premul((400, 400)).expect("raster surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        let an = a.arena_nodes();
        if let Some(root) = a.layout_root_idx() {
            crate::render::render(an, root, surface.canvas());
            a.render_layer(surface.canvas());
        }
        let bn = b.arena_nodes();
        if let Some(root) = b.layout_root_idx() {
            surface.canvas().save();
            surface.canvas().translate((b_origin.0, b_origin.1));
            crate::render::render(bn, root, surface.canvas());
            b.render_layer(surface.canvas());
            surface.canvas().restore();
        }
        surface
    }

    fn marked_in(composer: &Composer) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(root) = composer.layout_root_idx() {
            let nodes = composer.arena_nodes();
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                if find_shared_marker(&nodes[idx].modifier).is_some() {
                    out.push(idx);
                }
                stack.extend(nodes[idx].children.iter().copied());
            }
        }
        out
    }

    #[test]
    fn tier1_main_to_overlay_opens_and_completes() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(77);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert!(a.shared_flights.is_empty() && b.shared_flights.is_empty());
        assert!(a.pending_cross.is_empty() && b.pending_cross.is_empty());

        // Switch: A drops, B shows.
        show_a.set(false);
        show_b.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1, "Tier1 opens in the main map");
        assert!(b.shared_flights.is_empty(), "nothing stored peer-side");
        let fid = *a.shared_flights.keys().next().unwrap();
        {
            let f = &a.shared_flights[&fid];
            assert_eq!((f.source_cid, f.target_cid), (a.composer_id, b.composer_id));
            assert_eq!(f.flight.phase, FlightPhase::Flying, "resolved same frame");
        }
        assert_eq!(a.transition_layer.len(), 1, "source retained in owner arena");
        assert!(a.has_cross_flights(), "z-order helper sees it");
        // Target visuals live in B in B's frame.
        let bt = marked_in(&b);
        assert_eq!(bt.len(), 1);
        assert!(matches!(
            b.arena_nodes()[bt[0]].transition.clone(),
            Some(ref v) if v.role == TransitionRole::Target
        ));

        // p=0 frame paint: source opaque at A's hero center.
        let (sx, sy) = node_center(&a, a.transition_layer[0]);
        let mut surf0 = render_cross(&a, &b, (0.0, 0.0));
        assert!(
            close_enough(pixel_rgb(&mut surf0, sx, sy), (255, 0, 0), 30),
            "p=0 matches the pre-switch frame"
        );

        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        assert!(a.shared_flights.is_empty(), "Tier1 completes");
        assert!(a.transition_layer.is_empty(), "retained source freed in owner arena");
        assert!(b.transition_layer.is_empty());
        for idx in marked_in(&b) {
            assert!(b.arena_nodes()[idx].transition.is_none(), "target visuals cleared");
        }
        // End paint: BLUE detail hero in B.
        let bt = marked_in(&b);
        assert_eq!(bt.len(), 1);
        let (ex, ey) = node_center(&b, bt[0]);
        let mut surf = render_cross(&a, &b, (0.0, 0.0));
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
            "ends on the overlay hero"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_scope_flag_tracks_cross_flight() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        clear_scope_active_states();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(90);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        show_a.set(false);
        show_b.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1, "Tier1 opens in the main map");

        // Same global entry the union sync writes (keyed by scope_id).
        let active = scope.is_transition_active();
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        assert!(active.get(), "flag true while the cross flight is non-terminal");

        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        assert!(a.shared_flights.is_empty(), "Tier1 completes");
        assert!(!active.get(), "flag false after cross teardown");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_reverse_overlay_to_main() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(78);
        // Dialog open with hero; main heroless.
        let show_a = State::new(false);
        let show_b = State::new(true);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        // Reverse: B drops, A shows.
        show_b.set(false);
        show_a.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1, "Tier1 lives in the main map either way");
        let fid = *a.shared_flights.keys().next().unwrap();
        {
            let f = &a.shared_flights[&fid];
            assert_eq!((f.source_cid, f.target_cid), (b.composer_id, a.composer_id));
        }
        // Retained in the OVERLAY arena, not main.
        assert!(a.transition_layer.is_empty());
        assert_eq!(b.transition_layer.len(), 1);

        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        assert!(a.shared_flights.is_empty());
        assert!(b.transition_layer.is_empty(), "overlay-retained freed via owner ref");
        // Ends RED in main.
        let at = marked_in(&a);
        assert_eq!(at.len(), 1);
        let (ex, ey) = node_center(&a, at[0]);
        let mut surf = render_cross(&a, &b, (0.0, 0.0));
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (255, 0, 0), 30),
            "reverse ends on the main hero"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_cancel_when_target_vanishes() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(79);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        show_a.set(false);
        show_b.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1);
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        // Target vanishes with no counterpart (dialog torn down around flight).
        show_b.set(false);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert!(a.shared_flights.is_empty(), "stale Tier1 cancelled");
        assert!(a.transition_layer.is_empty(), "retained freed");
        assert!(b.shared_flights.is_empty() && b.transition_layer.is_empty());
        // Main tree carries no visuals.
        if let Some(root) = a.layout_root_idx() {
            let nodes = a.arena_nodes();
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                assert!(nodes[idx].transition.is_none(), "no stale visuals");
                stack.extend(nodes[idx].children.iter().copied());
            }
        }
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_unmatched_stash_freed_same_frame() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(80);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        // Plain removal: A drops its hero, B never shows a counterpart.
        show_a.set(false);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert!(a.shared_flights.is_empty(), "no flight without counterpart");
        assert!(a.pending_cross.is_empty(), "stash drained");
        assert!(a.transition_layer.is_empty(), "retained freed same frame (invisible)");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_origin_offsets_end() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        // Overlay composited at an offset (mirrors screen_pos translation).
        b.screen_origin = (50.0, 60.0);
        let scope = SharedTransitionScope::new(81);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        show_a.set(false);
        show_b.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1);
        let fid = *a.shared_flights.keys().next().unwrap();
        let f = &a.shared_flights[&fid];
        // B-local (0,0,300,160) mapped to window coords.
        let end = f.flight.end.expect("end resolved");
        assert!(
            (end.x - 50.0).abs() < 0.01 && (end.y - 60.0).abs() < 0.01,
            "flight end in window coords, got ({}, {})",
            end.x,
            end.y
        );
        // …while B's own visuals stay in B's canvas frame: window origin
        // (0,0) renders at (-50,-60) in B's translated pass.
        let bt = marked_in(&b);
        assert_eq!(bt.len(), 1);
        let vis = b.arena_nodes()[bt[0]].transition.clone().expect("target visuals");
        assert!(
            (vis.start.x + 50.0).abs() < 0.01 && (vis.start.y + 60.0).abs() < 0.01,
            "writer-frame visuals subtract the origin, got ({}, {})",
            vis.start.x,
            vis.start.y
        );
        crate::animation::clear_all_animations();
    }

    /// Keyed hero leaf (multi-pair flights).
    fn hero_keyed_leaf(
        ctx: &mut ComposeCtx,
        w: f32,
        h: f32,
        color: Color,
        scope: &SharedTransitionScope,
        key: &str,
    ) {
        let k = ctx.next_key();
        ctx.start_leaf(
            k,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::Rectangle)
                .shared_element(scope.shared_content_state(key), BoundsTransform::default(), PlaceHolderSize::JumpCut, PathMotion::Linear, 0.0, true),
        );
        ctx.end_node();
    }

    #[test]
    fn two_keys_fly_together() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(82);
        let show_a = State::new(true);
        let show_b = State::new(false);

        let pair_frame = |a: &mut Composer, b: &mut Composer| {
            let (sa, sb) = (show_a.clone(), show_b.clone());
            let (sca, scb) = (scope.clone(), scope.clone());
            cross_frame(
                a,
                b,
                |ctx| shell(ctx, |ctx| {
                    if sa.get() {
                        hero_keyed_leaf(ctx, 100.0, 60.0, Color::RED, &sca, "k1");
                        hero_keyed_leaf(ctx, 100.0, 60.0, Color::GREEN, &sca, "k2");
                    }
                }),
                |ctx| shell(ctx, |ctx| {
                    if sb.get() {
                        hero_keyed_leaf(ctx, 200.0, 120.0, Color::BLUE, &scb, "k1");
                        hero_keyed_leaf(ctx, 200.0, 120.0, Color::BLUE, &scb, "k2");
                    }
                }),
            );
        };
        let pair_advance = |a: &mut Composer, b: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            pair_frame(a, b);
        };

        pair_frame(&mut a, &mut b);
        show_a.set(false);
        show_b.set(true);
        pair_frame(&mut a, &mut b);
        assert_eq!(a.shared_flights.len(), 2, "one Tier1 flight per key");
        assert_eq!(a.transition_layer.len(), 2, "both sources retained");

        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            pair_advance(&mut a, &mut b);
        }
        assert!(a.shared_flights.is_empty(), "both complete");
        assert!(a.transition_layer.is_empty(), "both freed");
        for idx in marked_in(&b) {
            assert!(b.arena_nodes()[idx].transition.is_none());
        }
        crate::animation::clear_all_animations();
    }

    #[crate::composable]
    fn b_plain(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
    }

    #[crate::composable]
    fn b_shifted(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        gap_leaf(ctx, 400.0, 100.0);
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope, true);
    }

    #[test]
    fn tier1_superseded_by_overlay_tier0() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(83);
        let show_a = State::new(true);
        let show_b = State::new(false);
        let shift_b = State::new(false);

        // B content with an optional gap above the hero (content switch).
        let bframe = |a: &mut Composer, b: &mut Composer| {
            let (sa, sb, sh) = (show_a.clone(), show_b.clone(), shift_b.clone());
            let (sca, scb) = (scope.clone(), scope.clone());
            cross_frame(
                a,
                b,
                |ctx| shell(ctx, |ctx| {
                    if sa.get() {
                        hero_leaf(ctx, 120.0, 80.0, Color::RED, &sca, true);
                    }
                }),
                |ctx| shell(ctx, |ctx| {
                    // Macro'd screens (like list/detail): plain closures would
                    // collide gap-over-hero at one position in test-fallback keys.
                    if sb.get() {
                        if sh.get() {
                            b_shifted(ctx, &scb);
                        } else {
                            b_plain(ctx, &scb);
                        }
                    }
                }),
            );
        };
        let badvance = |a: &mut Composer, b: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            bframe(a, b);
        };

        bframe(&mut a, &mut b);
        show_a.set(false);
        show_b.set(true);
        bframe(&mut a, &mut b);
        assert_eq!(a.shared_flights.len(), 1, "Tier1 main→overlay opens");
        badvance(&mut a, &mut b);
        badvance(&mut a, &mut b);
        // Overlay switches its own content mid-Tier1: overlay Tier0 opens,
        // Tier1 yields (staleness-cancel, no leak, no double ghost).
        shift_b.set(true);
        bframe(&mut a, &mut b);
        assert!(a.shared_flights.is_empty(), "Tier1 yields to the newer Tier0");
        assert_eq!(b.shared_flights.len(), 1, "overlay Tier0 owns the key now");
        assert!(a.transition_layer.is_empty(), "Tier1 retained freed on yield");

        for _ in 0..200 {
            if b.shared_flights.is_empty() {
                break;
            }
            badvance(&mut a, &mut b);
        }
        assert!(b.shared_flights.is_empty(), "Tier0 completes");
        assert!(b.transition_layer.is_empty());
        // End paint: shifted BLUE hero (gap pushed it to y=100).
        let bt = marked_in(&b);
        assert_eq!(bt.len(), 1);
        let (ex, ey) = node_center(&b, bt[0]);
        let mut surf = render_cross(&a, &b, (0.0, 0.0));
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
            "overlay Tier0 end state paints"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_reverse_flies_back() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(84);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        show_a.set(false);
        show_b.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1);
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        // Reverse mid-flight: the old Tier1 cancels AND a reverse Tier1 opens
        // in the same cross-pass (stashed B-source × fresh A-target) — no snap.
        show_b.set(false);
        show_a.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        assert_eq!(a.shared_flights.len(), 1, "reverse Tier1 replaces the cancelled one");
        let fid = *a.shared_flights.keys().next().unwrap();
        {
            let f = &a.shared_flights[&fid];
            assert_eq!((f.source_cid, f.target_cid), (b.composer_id, a.composer_id));
            assert_eq!(f.flight.phase, FlightPhase::Flying);
        }
        assert_eq!(b.transition_layer.len(), 1, "B-side source retained");
        for _ in 0..200 {
            if a.shared_flights.is_empty() {
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        assert!(a.shared_flights.is_empty() && b.shared_flights.is_empty());
        assert!(a.transition_layer.is_empty() && b.transition_layer.is_empty());
        // Ends RED in main.
        let at = marked_in(&a);
        assert_eq!(at.len(), 1);
        assert!(a.arena_nodes()[at[0]].transition.is_none());
        let (ex, ey) = node_center(&a, at[0]);
        let mut surf = render_cross(&a, &b, (0.0, 0.0));
        assert!(
            close_enough(pixel_rgb(&mut surf, ex, ey), (255, 0, 0), 30),
            "reversed back to the list hero"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn tier1_midflight_progress_visible() {
        let _g = lock_serial();
        crate::animation::clear_all_animations();
        let mut a = Composer::new();
        let mut b = Composer::new();
        let scope = SharedTransitionScope::new(85);
        let show_a = State::new(true);
        let show_b = State::new(false);

        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        show_a.set(false);
        show_b.set(true);
        xframe(&mut a, &mut b, &show_a, &show_b, &scope);
        let mut mid_seen = false;
        for _ in 0..40 {
            if a.shared_flights.is_empty() {
                break;
            }
            let p = a.shared_flights.values().next().map(|f| f.progress.peek()).unwrap_or(1.0);
            if p > 0.2 && p < 0.95 {
                mid_seen = true;
                // Both ends carry same-progress visuals across composers.
                for idx in marked_in(&b) {
                    let v = b.arena_nodes()[idx].transition.clone().expect("target visuals");
                    assert!((v.progress - p).abs() < 0.001);
                }
                break;
            }
            xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
        }
        assert!(mid_seen, "Tier1 passes through a visible mid state");
        crate::animation::clear_all_animations();
    }
}
