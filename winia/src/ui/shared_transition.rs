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

// Phase 1–2 skeleton gate: `register` (cross-composer matching) and the
// Tier 1/2 vocabulary gain their readers in Phase 4
// (`docs/shared-element-transition.md` §9) and are exercised by unit tests
// only until then — dead-code lints stay silenced file-wide. Phase 4 must
// delete this line.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;

use crate::animation::{
    push_animatable, AnimatableValue, AnimationSpec, KeyframesSpec, SpringSpec, TweenSpec,
};
use crate::core::composer::Composer;
use crate::core::composer::ComposeCtx;
use crate::core::composition_local::CompositionLocal;
use crate::core::state::State;
use crate::layout::node::{scroll_offset_for_node, LayoutNode};
use crate::modifier::{Modifier, ModifierElement, Shape};

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
}

impl TransitionVisual {
    pub(crate) fn lerped(&self) -> SharedBounds {
        self.start.lerp(&self.end, self.progress)
    }

    pub(crate) fn alpha(&self) -> f32 {
        match self.role {
            TransitionRole::Source => 1.0 - self.progress,
            TransitionRole::Target => self.progress,
            // Same-screen morph never touches opacity.
            TransitionRole::Morph => 1.0,
        }
        .clamp(0.0, 1.0)
    }

    pub(crate) fn radii(&self) -> [f32; 4] {
        let t = self.progress.clamp(0.0, 1.0);
        [0, 1, 2, 3].map(|i| self.radius_from[i] + (self.radius_to[i] - self.radius_from[i]) * t)
    }

    /// Screen-frame clip rect — call BEFORE the flight canvas transform
    /// (clip is captured in the pre-transform space, like the GL clip rule).
    pub(crate) fn screen_rrect(&self) -> skia_safe::RRect {
        let l = self.lerped();
        let r = self.radii();
        skia_safe::RRect::new_rect_radii(
            skia_safe::Rect::new(l.x, l.y, l.x + l.width, l.y + l.height),
            &rrect_vectors([(r[0], r[0]), (r[1], r[1]), (r[2], r[2]), (r[3], r[3])]),
        )
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
}

pub(crate) fn find_shared_marker(modifier: &Modifier) -> Option<SharedMarker> {
    modifier.elements().iter().find_map(|el| match el {
        ModifierElement::SharedTransition { scope_id, key, kind, transform, .. } => {
            Some(SharedMarker {
                scope_id: *scope_id,
                key: key.clone(),
                kind: kind.clone(),
                transform: transform.clone(),
            })
        }
        _ => None,
    })
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
    /// Exact start bounds (retained-frame upward walk — positions untouched
    /// since layout; no lookahead needed).
    pub start: SharedBounds,
    pub radius_from: [f32; 4],
    pub radius_to: [f32; 4],
    pub clip: bool,
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

    /// Detached retained source roots (absolute coords, rendered after main tree).
    pub(crate) fn transition_roots(&self) -> &[usize] {
        &self.transition_layer
    }

    /// Compose-tail hook (MUST run after materialize, before the prev drain):
    /// detect switches, cancel dead/retargeted flights, retain+detach sources.
    pub(crate) fn retain_shared_sources(&mut self) {
        let live = self.shared_live_map();
        if live.is_empty() && self.prev_shared_endpoints.is_empty() && self.shared_flights.is_empty() {
            return; // fast path: no shared content anywhere
        }
        // Cancel flights whose key left the live tree with no counterpart
        // (screen torn down around a flight).
        let live_keys: HashSet<(u64, String)> = live.keys().cloned().collect();
        let dead: Vec<FlightId> = self
            .shared_flights
            .iter()
            .filter(|(_, a)| {
                !is_terminal(a.flight.phase)
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
            // retained node), restart from the current visual rect.
            let mut start_override: Option<(SharedBounds, [f32; 4])> = None;
            if let Some(id) = self
                .shared_flights
                .iter()
                .find(|(_, a)| {
                    !is_terminal(a.flight.phase)
                        && a.flight.scope_id == c.scope_id
                        && a.flight.key == c.key
                })
                .map(|(id, _)| *id)
            {
                if let Some(a) = self.shared_flights.get(&id) {
                    let p = a.progress.peek().clamp(0.0, 1.0);
                    let end = a.flight.end.unwrap_or(a.start);
                    let s = a.start.lerp(&end, p);
                    let (rf, rt) = (a.radius_from, a.radius_to);
                    start_override = Some((s, [0, 1, 2, 3].map(|i| rf[i] + (rt[i] - rf[i]) * p)));
                }
                self.cancel_flight(id);
            }
            self.begin_flight(c, start_override);
        }
        self.prev_shared_endpoints = live;
    }

    /// Detach the source node (freeze) and open an AwaitingBounds flight.
    /// `start_override`: retarget visual continuity; otherwise the exact
    /// upward-walk bounds (no lookahead needed).
    fn begin_flight(&mut self, c: SwitchCandidate, start_override: Option<(SharedBounds, [f32; 4])>) {
        let Some(src_idx) = self.prev_node_by_key.remove(&c.old_slot) else {
            // Node object repurposed by the new tree (same-key reuse) — nothing to fly.
            return;
        };
        // Unlink from any still-referencing old parent (objects alive pre-drain;
        // reused ancestors already dropped the ref via children.clear()).
        for pidx in self.prev_node_by_key.values() {
            self.arena.nodes[*pidx].children.retain(|&x| x != src_idx);
        }
        // Belt-and-braces: the drain below must never reclaim it.
        self.reused_nodes.insert(src_idx);
        let (start, radius_from) = match start_override {
            Some(v) => v,
            None => {
                let id_to_idx: HashMap<u64, usize> =
                    self.arena.nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
                let b = abs_rect_upward(&self.arena.nodes, &id_to_idx, src_idx);
                let r = {
                    let n = &self.arena.nodes[src_idx];
                    shared_shape_radii(&n.modifier, n.measured_size.width, n.measured_size.height)
                };
                (b, r)
            }
        };
        // Detach: absolute position, rootless, transition-layer owned.
        {
            let n = &mut self.arena.nodes[src_idx];
            n.position = crate::layout::node::Point::new(start.x, start.y);
            n.parent_id = None;
        }
        self.transition_layer.push(src_idx);
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
                radius_from,
                radius_to: radius_from,
                clip: false,
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
            self.clear_transition_for_slot(slot);
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

    fn clear_transition_for_slot(&mut self, slot: u64) {
        if let Some(root) = self.arena.root {
            if let Some(idx) = find_idx_by_slot(&self.arena.nodes, root, slot) {
                self.arena.nodes[idx].transition = None;
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
    }

    fn poll_one_flight(&mut self, id: FlightId) {
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
                let end = SharedBounds::new(tx, ty, tw, th);
                let marker = find_shared_marker(&self.arena.nodes[tidx].modifier);
                let (spec, clip) = match marker {
                    Some(m) => {
                        let clip = matches!(
                            m.kind,
                            SharedKind::Bounds { resize: ResizeMode::ScaleToBounds { clip: true }, .. }
                        );
                        (m.transform.spec.clone(), clip)
                    }
                    None => (BoundsTransform::default().spec, false),
                };
                let radius_to = {
                    let n = &self.arena.nodes[tidx];
                    shared_shape_radii(&n.modifier, tw, th)
                };
                let progress = State::new(0.0f32);
                push_animatable(progress.clone(), 1.0, spec.clone());
                let (start, acts) = {
                    let a = self.shared_flights.get_mut(&id).expect("polled flight exists");
                    a.spec = spec;
                    a.progress = progress;
                    a.radius_to = radius_to;
                    a.clip = clip;
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
                if p >= 0.999 {
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
                        self.clear_transition_for_slot(slot);
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
            )
        });
        let (start, end, p, rf, rt, clip, sslot, tslot, sidx) = match snapshot {
            Some(v) => v,
            None => return,
        };
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
                });
            }
        }
        if let Some(slot) = tslot {
            if let Some(root) = self.arena.root {
                if let Some(tidx) = find_idx_by_slot(&self.arena.nodes, root, slot) {
                    self.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start,
                        end,
                        progress: p,
                        role: if sslot == tslot {
                            // Same-screen size morph: opacity untouched.
                            TransitionRole::Morph
                        } else {
                            TransitionRole::Target
                        },
                        radius_from: rf,
                        radius_to: rt,
                        clip,
                        link_slot: None,
                    });
                }
            }
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
    fn flight_for_key(&self, scope_id: u64, key: &str) -> Option<FlightId> {
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
        let (spec, clip) = match marker {
            Some(m) => {
                let clip = matches!(
                    m.kind,
                    SharedKind::Bounds { resize: ResizeMode::ScaleToBounds { clip: true }, .. }
                );
                (m.transform.spec.clone(), clip)
            }
            None => (BoundsTransform::default().spec, false),
        };
        let (tw, th, modifier) = {
            let n = &self.arena.nodes[tidx];
            (n.measured_size.width, n.measured_size.height, n.modifier.clone())
        };
        let radius_to = shared_shape_radii(&modifier, tw, th);
        let radius_from =
            radius_from_override.unwrap_or_else(|| shared_shape_radii(&modifier, start.width, start.height));
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
                radius_from,
                radius_to,
                clip,
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
            let cur = SharedBounds::new(ax, ay, w, h);
            if let Some(prev) = self.shared_last_bounds.get(slot) {
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
                                let p = a.progress.peek().clamp(0.0, 1.0);
                                let end_prev = a.flight.end.unwrap_or(a.start);
                                let s = a.start.lerp(&end_prev, p);
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
            self.shared_last_bounds.insert(*slot, cur);
        }
        // Drop baselines for vanished slots (bounded memory).
        let live_slots: HashSet<u64> = live.values().copied().collect();
        self.shared_last_bounds.retain(|slot, _| live_slots.contains(slot));
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

    /// Plain box leaf with a shared-element marker (no text — keeps the
    /// mechanics test headless-simple; paint movement is asserted via raster).
    fn hero_leaf(ctx: &mut ComposeCtx, w: f32, h: f32, color: Color, scope: &SharedTransitionScope) {
        let key = ctx.next_key();
        ctx.start_leaf(
            key,
            Modifier::new()
                .size(w, h)
                .background(color, Shape::rounded(8.0))
                .shared_element(scope.shared_content_state("hero"), BoundsTransform::default()),
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
        hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
    }

    #[crate::composable]
    fn detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope) {
        gap_leaf(ctx, 400.0, 100.0);
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
    }

    /// One app-loop step: compose → layout → post-layout flight poll.
    /// One app-loop step: compose → layout → post-layout flight poll.
    fn frame(composer: &mut Composer, show: &State<bool>) {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx, scope| {
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

    /// Mirror the app loop: main tree, then detached transition roots.
    fn render_heads(composer: &Composer) -> skia_safe::Surface {
        let mut surface =
            skia_safe::surfaces::raster_n32_premul((400, 400)).expect("raster surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        let nodes = composer.arena_nodes();
        if let Some(root) = composer.layout_root_idx() {
            crate::render::render(nodes, root, surface.canvas());
            for &t in composer.transition_roots() {
                crate::render::render(nodes, t, surface.canvas());
            }
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
                .shared_element(scope.shared_content_state("morph"), BoundsTransform::default()),
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
            SharedTransitionLayout::new().build(ctx, |ctx, scope| {
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
        assert_ne!(
            plain.last().copied(),
            Some(target_idx),
            "legacy hit test does not route (documents the behavior delta)"
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
        hero_leaf(ctx, 120.0, 80.0, Color::RED, scope);
    }

    #[crate::composable]
    fn mixed_detail_screen(ctx: &mut ComposeCtx, scope: &SharedTransitionScope, w: &State<f32>) {
        morph_leaf(ctx, w, scope);
        gap_leaf(ctx, 400.0, 100.0);
        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, scope);
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
                SharedTransitionLayout::new().build(ctx, |ctx, scope| {
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
}
