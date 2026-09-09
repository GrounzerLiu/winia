//! Shared element transition — matching registry + flight engine skeleton.
//!
//! Phase 1 of `docs/shared-element-transition.md`: pure data, pure logic and
//! the frozen public API. No behavior change — flights never start because the
//! coordinator is not wired into compose/layout yet (Phase 2).
//!
//! Layer map (see the design doc):
//! - Matching: [`SharedTransitionScope`], [`SharedContentState`], registry.
//! - Flight engine: [`Flight`] state machine — `on_event` is pure and tested.
//! - Content providers: [`ContentRef`] Tier 0/1/2 vocabulary (data only).
//!
//! Spring physics note: `BoundsTransform::spring` does NOT need a vector
//! spring. It springs scalar progress 0→1 (supported by the engine) and the
//! rect is derived per frame via [`SharedBounds::lerp`]; overshoot lerps past
//! the target, which is exactly the Compose spring look. Hence
//! `SharedBounds` keeps `supports_spring() == false` (Tween-exact) and spring
//! flights go through scalar progress (Phase 2 wiring).

// Phase 1 skeleton: the `pub(crate)` registry/flight items below gain their
// composer-side readers in Phase 2 (`docs/shared-element-transition.md` §9)
// and are exercised by unit tests only until then — dead-code lints stay
// silenced file-wide. Phase 2 must delete this line.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;

use crate::animation::{AnimatableValue, AnimationSpec, KeyframesSpec, SpringSpec, TweenSpec};
use crate::core::composer::ComposeCtx;
use crate::core::composition_local::CompositionLocal;
use crate::modifier::{Modifier, ModifierElement};

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
    Element,
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

/// One registered endpoint: a slot claiming (scope, key), bounds filled
/// post-layout (Phase 2 hook).
#[derive(Debug, Clone)]
pub(crate) struct Endpoint {
    pub slot_key: u64,
    pub composer_id: u64,
    pub bounds: Option<SharedBounds>,
    pub generation: u64,
}

/// (scope, key) → live endpoints. Guarded by the scope's shared mutex so
/// endpoints from any composer (main tree, overlays, other windows — all on
/// the event-loop thread) can register.
#[derive(Debug, Default)]
pub(crate) struct SharedRegistry {
    endpoints: HashMap<(u64, String), Vec<Endpoint>>,
    generation: u64,
}

impl SharedRegistry {
    /// Register (or refresh) one endpoint. Same slot re-registers idempotently
    /// across recompositions; the generation marks the frame for stale-read
    /// detection (Phase 2 coordinator).
    pub(crate) fn register(&mut self, scope_id: u64, key: &str, slot_key: u64, composer_id: u64) {
        self.generation += 1;
        let generation = self.generation;
        let list = self.endpoints.entry((scope_id, key.to_string())).or_default();
        if let Some(ep) = list.iter_mut().find(|ep| ep.slot_key == slot_key) {
            ep.composer_id = composer_id;
            ep.generation = generation;
        } else {
            list.push(Endpoint { slot_key, composer_id, bounds: None, generation });
        }
    }

    /// Drop every endpoint owned by a slot (truncate-path hook, Phase 2).
    /// Returns the removed endpoints so the coordinator can detect the
    /// disappearing side of a switch.
    pub(crate) fn unregister_slot(&mut self, slot_key: u64) -> Vec<((u64, String), Endpoint)> {
        let mut removed = Vec::new();
        self.endpoints.retain(|k, list| {
            let mut i = 0;
            while i < list.len() {
                if list[i].slot_key == slot_key {
                    removed.push((k.clone(), list.remove(i)));
                } else {
                    i += 1;
                }
            }
            !list.is_empty()
        });
        removed
    }

    /// Fill post-layout bounds (app-loop hook, Phase 2).
    pub(crate) fn set_bounds(&mut self, scope_id: u64, key: &str, slot_key: u64, bounds: SharedBounds) -> bool {
        if let Some(list) = self.endpoints.get_mut(&(scope_id, key.to_string())) {
            if let Some(ep) = list.iter_mut().find(|ep| ep.slot_key == slot_key) {
                ep.bounds = Some(bounds);
                return true;
            }
        }
        false
    }

    pub(crate) fn endpoints_for(&self, scope_id: u64, key: &str) -> &[Endpoint] {
        self.endpoints
            .get(&(scope_id, key.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
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

/// Transition scope handle (Compose `SharedTransitionScope`). Cheap `Arc`
/// clone; passed explicitly (no implicit receiver in Rust).
#[derive(Debug, Clone)]
pub struct SharedTransitionScope {
    scope_id: u64,
    registry: Arc<Mutex<SharedRegistry>>,
}

impl SharedTransitionScope {
    pub(crate) fn new(scope_id: u64) -> Self {
        Self { scope_id, registry: Arc::new(Mutex::new(SharedRegistry::default())) }
    }

    /// Pairing handle for one shared element (Compose
    /// `rememberSharedContentState`). Stable for the scope's lifetime.
    pub fn shared_content_state(&self, key: impl Into<String>) -> SharedContentState {
        SharedContentState { scope_id: self.scope_id, key: key.into() }
    }

    pub fn scope_id(&self) -> u64 {
        self.scope_id
    }

    pub(crate) fn registry(&self) -> &Arc<Mutex<SharedRegistry>> {
        &self.registry
    }
}

static LOCAL_SHARED_SCOPE: LazyLock<CompositionLocal<Option<SharedTransitionScope>>> =
    LazyLock::new(|| CompositionLocal::new(|| None));

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
    /// the registry (held in `remember`) survives recomposition; `remember_at_key`
    /// additionally immunizes it against statement-order drift.
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx, SharedTransitionScope)) {
        let scope_id = ctx.next_key();
        let scope = ctx.remember_at_key(scope_id, || SharedTransitionScope::new(scope_id)).get();
        LOCAL_SHARED_SCOPE.provides(Some(scope.clone()), || content(ctx, scope));
    }
}

// ═══════════════════════════════════════════════════════════
// Flight engine: content tiers + pure state machine
// ═══════════════════════════════════════════════════════════

/// Pixel-source tier for one flight end (frozen vocabulary; actuators land in
/// Phase 2 for Tier 0, Phase 4 for Tier 1/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentTier {
    /// Live node in the same composer, detached + frozen (source) or normally
    /// composed (target).
    Tier0Live,
    /// `DescNode` subtree transplanted into an overlay composer.
    Tier1Transplant,
    /// Bitmap fallback (last resort only — see design doc §3.5).
    Tier2Snapshot,
}

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
    /// Reverse or redirect mid-flight (new flight starts from current visual).
    Retarget,
    /// Owning layout left composition.
    ScopeDisposed,
    /// A participating window closed.
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
    pub fn shared_element(self, state: SharedContentState, transform: BoundsTransform) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Element,
            transform,
            path: PathMotion::Linear,
        })
    }

    /// Mark shared bounds (different content — container morphs + crossfades).
    pub fn shared_bounds(
        self,
        state: SharedContentState,
        transform: BoundsTransform,
        resize: ResizeMode,
        placeholder: PlaceHolderSize,
    ) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Bounds { resize, placeholder },
            transform,
            path: PathMotion::Linear,
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

    #[test]
    fn registry_register_set_bounds_roundtrip() {
        let mut reg = SharedRegistry::default();
        reg.register(7, "hero", 100, 9);
        reg.register(7, "hero", 100, 9); // idempotent re-register across recompositions
        assert_eq!(reg.endpoints_for(7, "hero").len(), 1);
        let b = SharedBounds::new(1.0, 2.0, 30.0, 40.0);
        assert!(reg.set_bounds(7, "hero", 100, b));
        assert_eq!(reg.endpoints_for(7, "hero")[0].bounds, Some(b));
        assert!(!reg.set_bounds(7, "hero", 999, b), "unknown slot rejected");
        let removed = reg.unregister_slot(100);
        assert_eq!(removed.len(), 1, "truncate hook recovers the disappearing side");
        assert!(reg.endpoints_for(7, "hero").is_empty());
    }
}
