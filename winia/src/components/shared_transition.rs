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

// The flight machinery lives a layer down (`crate::transition`): the runtime stores flights,
// the layout pass writes their visuals and the renderer reads them, so none of it may live in
// this component. The composables and the `Modifier` builders stay here.
use crate::transition::*;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use crate::animation::{
    push_animatable, AnimatableValue, AnimationSpec, TweenSpec,
};
use crate::runtime::composer::Composer;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::composition_local::CompositionLocal;
use crate::runtime::state::State;
use crate::layout::node::{
    LayoutNode, PaintDisposition,
};
use crate::modifier::{Modifier, ModifierElement};
use crate::components::animated_visibility::VisibilityTransition;

// ═══════════════════════════════════════════════════════════
// SharedBounds — exact animatable rect
// ═══════════════════════════════════════════════════════════


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

/// The scaling model a `ScaleToBounds` end uses is Compose's `ContentScale` —
/// `androidx.compose.ui.layout.ContentScale`, the SAME type `Image` takes, not a parallel
/// copy of it. `ImageAlignment` is Compose's 9-position `Alignment`.
pub use crate::graphics::{ContentScale, ImageAlignment};


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

/// One scene published by a scene host: an id that is stable for that scene across frames, plus a
/// handle that reads its visibility whenever the flight system asks (1 = fully in the scene).
///
/// The visibility is a CLOSURE, not a snapshot: a scene host may drive its transition from the
/// render path (winia's nav uses a `graphics_layer` closure), so its compose does not re-run every
/// frame and anything captured at compose time would go stale exactly while the transition runs.
#[derive(Clone)]
pub struct NavSceneInfo {
    /// Per-LAYER id (`layer_scene_id(host, scene key, is_prev)`): it names this published layer, so it
    /// changes when the layer's role flips.
    pub id: u64,
    /// Stable id of the SCENE itself (the host's scene key), unchanged when the layer's role flips. The
    /// layer id cannot answer "did THIS scene's opacity jump?", because a role flip renames the layer while
    /// the two layers swap their visibilities: the observer needs an identity that does not move.
    pub scene_key: u64,
    pub visibility: std::sync::Arc<dyn Fn() -> f32>,
    /// Is this the LEAVING scene of the host's transition? Two live ends of one shared key (a scene
    /// host composes both scenes at once) are told apart by this, not by comparing visibilities: the
    /// leaving scene starts at visibility 1 and the entering one at 0, so "the more visible end" picks
    /// the LEAVING end at the switch frame — measured on the nav demo, where the flight then paired the
    /// 96x96 list hero with itself and never grew (the detail hero's 320x220 was never its target).
    /// Which scene is leaving is stable for the whole transition, so the pairing is too.
    pub is_prev: bool,
}

impl std::fmt::Debug for NavSceneInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NavSceneInfo")
            .field("id", &self.id)
            .field("is_prev", &self.is_prev)
            .field("visibility", &(self.visibility)())
            .finish()
    }
}

thread_local! {
    /// Scene stack for the compose in progress: a scene host (winia's nav) pushes one entry around
    /// the content it composes so the marker builders record WHICH SCENE an end belongs to.
    static NAV_SCENE_STACK: std::cell::RefCell<Vec<NavSceneInfo>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Latest handle per scene id, kept beyond the compose so the post-layout polls can resolve a
    /// duplicated shared key by "which end is becoming visible".
    static NAV_SCENE_VIS: std::cell::RefCell<HashMap<u64, NavSceneInfo>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Publish `scene` for the duration of `f` (scene hosts call this around the content they compose).
/// The nav uses it per transition layer, which is what lets two LIVE ends of one shared key be told
/// apart: without a scene, the winner is whichever end the tree walk visited last, and during a nav
/// transition that alternates between the outgoing and incoming scene every frame — so the flight
/// system reads a fresh switch each frame, starting and retargeting flights instead of flying once.
pub fn with_nav_scene<R>(scene: NavSceneInfo, f: impl FnOnce() -> R) -> R {
    // The guard exists BEFORE anything is published, so a panic during publication cannot leave the stack
    // entry behind (markers composed after such a panic would be attributed to a scene that no longer
    // exists — the stale attribution the ancestry lookup was written to avoid).
    //
    // On NORMAL completion it pops the stack but KEEPS the registry entry: the entry has to outlive this
    // call so a layer that Skips composing still resolves its scene while a transition runs. Only during an
    // unwind is the entry dropped too, because a panicking scene host publishes a scene that nobody is
    // composing any more.
    struct SceneGuard {
        id: u64,
        pushed: bool,
    }
    impl Drop for SceneGuard {
        fn drop(&mut self) {
            if self.pushed {
                NAV_SCENE_STACK.with(|s| {
                    s.borrow_mut().pop();
                });
            }
            if std::thread::panicking() {
                NAV_SCENE_VIS.with(|v| {
                    v.borrow_mut().remove(&self.id);
                });
            }
        }
    }
    let mut guard = SceneGuard { id: scene.id, pushed: false };
    NAV_SCENE_STACK.with(|s| s.borrow_mut().push(scene.clone()));
    guard.pushed = true;
    NAV_SCENE_VIS.with(|v| {
        v.borrow_mut().insert(scene.id, scene.clone());
    });
    // anim-trace: hand the handle to the trace so the frame barrier records this scene's visibility
    // (it is a closure over the host's progress, so it has to be sampled once per frame rather than
    // read here). No-op unless the tracing feature is on.
    crate::anim_trace::note_scene(scene);
    f()
}

/// The scene the caller is composing inside, if any (see [`with_nav_scene`]).
pub fn current_nav_scene() -> Option<NavSceneInfo> {
    NAV_SCENE_STACK.with(|s| s.borrow().last().cloned())
}

/// Visibility announced for a scene id, read NOW (the handle keeps tracking the host's progress).
pub fn nav_scene_visibility(id: u64) -> Option<f32> {
    NAV_SCENE_VIS.with(|v| v.borrow().get(&id).map(|i| (i.visibility)()))
}

/// Is this the LEAVING scene of its host's transition? `None` when the scene is unknown. The pairing
/// rule for two live ends of one key uses this instead of comparing visibilities — see `NavSceneInfo`.
pub fn nav_scene_is_prev(id: u64) -> Option<bool> {
    NAV_SCENE_VIS.with(|v| v.borrow().get(&id).map(|i| i.is_prev))
}

/// Scene id of the subtree a node belongs to: the nearest ancestor (or the node itself) carrying a
/// `SceneTag`, which scene hosts put on the wrapper they build for each scene.
///
/// Read from ancestry at query time on purpose. Storing the id on each shared marker does not work:
/// the marker's modifier element is created once and then reused across frames, so the captured id
/// freezes — measured on the nav demo, one end reported `scene=None` and the other reported the
/// LEAVING scene, so the two live ends could not be told apart and the flight paired the leaving hero
/// with itself (both 96x96, alpha-only crossfade, no growth).
pub(crate) fn scene_of_node(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> Option<u64> {
    let id_to_idx: HashMap<u64, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    scene_of_node_with(nodes, &id_to_idx, idx)
}

/// [`scene_of_node`] with a caller-provided id→index map.
///
/// The map is the whole point: building it per call costs O(nodes) and the per-frame walk calls this
/// once per marked node, which measured as 2.68 ms per call on a 12 000-node tree (50 marked nodes →
/// ~134 ms per frame). Build the map once per pass and call this.
pub(crate) fn scene_of_node_with(
    nodes: &[crate::layout::node::LayoutNode],
    id_to_idx: &HashMap<u64, usize>,
    idx: usize,
) -> Option<u64> {
    let mut cur = Some(idx);
    // Bounded: a malformed parent chain must not hang a frame.
    for _ in 0..512 {
        let i = cur?;
        if let Some(id) = nodes.get(i).and_then(|n| {
            n.modifier.elements().iter().find_map(|el| match el {
                ModifierElement::SceneTag { id } => Some(*id),
                _ => None,
            })
        }) {
            return Some(id);
        }
        cur = nodes.get(i)?.parent_id.and_then(|pid| id_to_idx.get(&pid).copied());
    }
    None
}

#[cfg(test)]
pub(crate) fn clear_nav_scenes() {
    NAV_SCENE_STACK.with(|s| s.borrow_mut().clear());
    NAV_SCENE_VIS.with(|v| v.borrow_mut().clear());
}

/// Drop scene entries whose scene no longer exists in the tree.
///
/// The registry is written when a scene host composes a layer, and a layer that Skips does not compose
/// again — so without this a long-lived display keeps one entry (and one `Arc` visibility closure) per
/// distinct scene it has ever visited. A scene is live exactly when its `SceneTag` is present in the arena,
/// which is what a scene host puts on the wrapper it composes per layer.
pub(crate) fn prune_nav_scenes(live: &HashSet<u64>) {
    NAV_SCENE_VIS.with(|v| {
        let mut vis = v.borrow_mut();
        // Cheap guard: the registry is usually smaller than the set of tags, and `retain` is the only
        // allocation-free way to walk it anyway.
        if vis.len() > live.len() {
            vis.retain(|id, _| live.contains(id));
        }
    });
}

/// Entries in the scene registry (a cheap length read for the prune threshold).
pub(crate) fn nav_scene_registry_len() -> usize {
    NAV_SCENE_VIS.with(|v| v.borrow().len())
}

/// How many entries the registry may hold before an IDLE frame pays for the arena scan that prunes it.
/// Entries are added by scene hosts while they compose; a host whose layers Skip does not compose again, so
/// without this the registry would only ever shrink while something was animating.
pub(crate) const NAV_SCENE_PRUNE_THRESHOLD: usize = 32;

/// Prune the scene registry with the live set of the WHOLE WINDOW.
///
/// The registry is shared by every composer on the thread while an arena belongs to one of them, so the
/// live set has to be the union: pruning from a single composer's walk (the first version) let a peer
/// without scene tags delete the entries of the composer that has them — and the pairing then fell back to
/// last-wins, which is the "paired the leaving end with itself / never grew" failure this branch exists to
/// fix. Called once per frame from `Composer::poll_cross_flights`, which is the only place that has every
/// composer.
pub(crate) fn prune_scene_registry_window(all: &[&Composer]) {
    let len = nav_scene_registry_len();
    if len == 0 {
        return;
    }
    // An idle frame with a small registry skips the scan: the registry only grows when a scene host
    // composes, which is not an idle frame for that composer.
    let busy = all.iter().any(|c| {
        c.paint_dirty
            || !c.scope_overlay_roots.is_empty()
            || c.shared_flights.values().any(|a| !is_terminal(a.flight.phase))
    });
    if !busy && len <= NAV_SCENE_PRUNE_THRESHOLD {
        return;
    }
    let mut live: HashSet<u64> = HashSet::new();
    for c in all {
        live.extend(
            c.arena
                .nodes
                .iter()
                .flat_map(|n| n.modifier.elements().iter())
                .filter_map(|el| match el {
                    ModifierElement::SceneTag { id } => Some(*id),
                    _ => None,
                }),
        );
    }
    prune_nav_scenes(&live);
}

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


// ═══════════════════════════════════════════════════════════
// Modifier builders (frozen API)
// ═══════════════════════════════════════════════════════════

impl Modifier {
    /// Mark the shared element (same content on both ends — flies + crossfades).
    /// Compose's `sharedElement` has no resize parameter and always re-measures,
    /// so this marker resolves to `RemeasureToBounds`; `placeholder` selects
    /// what the parent observes while the flight runs (`PlaceHolderSize`).
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
            scene: current_nav_scene().map(|s| s.id),
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

    /// Tag this subtree as the content composed for scene `id` (scene hosts call it on the wrapper
    /// they build per scene, e.g. one nav transition layer). Shared-element pairing reads it from the
    /// ANCESTOR chain at query time: which of two live ends of one key is leaving and which is
    /// entering is decided by their scenes' visibilities, and a marker cannot carry that itself (its
    /// modifier element is built once and reused, so a captured id freezes).
    pub fn scene_tag(self, id: u64) -> Self {
        self.push(ModifierElement::SceneTag { id })
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
        // Compose's `overlayClip` defaults to the bounds' own resolved clip, which is this
        // crate's long-standing behaviour — so the 9-argument form keeps its signature and
        // the clip is opt-in through the sibling below.
        self.shared_bounds_with_overlay_clip(
            state,
            enter,
            exit,
            transform,
            resize,
            placeholder,
            path,
            z_index,
            render_in_overlay,
            OverlayClip::Bounds,
        )
    }

    /// [`shared_bounds`](Self::shared_bounds) with Compose's `overlayClip` slot.
    #[allow(clippy::too_many_arguments)]
    pub fn shared_bounds_with_overlay_clip(
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
        overlay_clip: OverlayClip,
    ) -> Self {
        self.push(ModifierElement::SharedTransition {
            scope_id: state.scope_id,
            key: state.key,
            kind: SharedKind::Bounds { resize, placeholder, overlay_clip },
            transform,
            path,
            z_index,
            enter: Some(enter),
            exit: Some(exit),
            render_in_overlay,
            scene: current_nav_scene().map(|s| s.id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::SpringSpec;

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

    /// The `ContentScale`/`ImageAlignment` math, pinned against Compose's documented
    /// semantics: `scaleToBounds()` defaults to `FillWidth` + `Center` (NOT `Image`'s
    /// `Fit`, and not `FillBounds` — which is what winia hard-coded before).
    #[test]
    fn content_scale_factors_match_compose() {
        let content = (150.0, 150.0);
        let bounds = (300.0, 150.0);
        assert_eq!(
            ContentScale::FillWidth.scale_factors(content, bounds),
            (2.0, 2.0),
            "FillWidth is uniform and matches the WIDTH (Compose's default)"
        );
        assert_eq!(
            ContentScale::FillHeight.scale_factors(content, bounds),
            (1.0, 1.0),
            "FillHeight is uniform and matches the HEIGHT"
        );
        assert_eq!(
            ContentScale::FillBounds.scale_factors(content, bounds),
            (2.0, 1.0),
            "FillBounds is the non-uniform stretch winia used to hard-code"
        );
        assert_eq!(ContentScale::Fit.scale_factors(content, bounds), (1.0, 1.0));
        assert_eq!(ContentScale::Crop.scale_factors(content, bounds), (2.0, 2.0));
        assert_eq!(
            ContentScale::Inside.scale_factors((500.0, 500.0), bounds),
            (0.3, 0.3),
            "Inside is Fit — both axes are limited by the tighter ratio"
        );
        assert_eq!(
            ContentScale::Inside.scale_factors((100.0, 100.0), bounds),
            (1.0, 1.0),
            "Inside never scales UP"
        );
        assert_eq!(
            ContentScale::None.scale_factors(content, bounds),
            (1.0, 1.0)
        );
        assert_eq!(
            ContentScale::FillWidth.scale_factors(content, (0.0, 150.0)),
            (1.0, 1.0),
            "degenerate bounds must not produce NaN/inf"
        );
        // A uniform FillWidth scale leaves the leftover axis to the alignment.
        let off = |a: ImageAlignment| ContentScale::align_offset(a, (300.0, 150.0), (300.0, 250.0), false);
        assert_eq!(off(ImageAlignment::Center), (0.0, 50.0));
        assert_eq!(off(ImageAlignment::BottomCenter), (0.0, 100.0));
        assert_eq!(off(ImageAlignment::TopStart), (0.0, 0.0));
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
    fn shared_clip_and_layout_contract_resolution() {
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
        // Clip extraction: ALWAYS false now — the render clips every transitioning
        // node unconditionally, so no marker can opt in or out (the `clip` flag that
        // used to feed this measured as dead and was deleted).
        assert!(!shared_clip_for_kind(&SharedKind::Element {
            placeholder: PlaceHolderSize::JumpCut
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Element {
            placeholder: PlaceHolderSize::AnimatedSize
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Bounds {
            resize: ResizeMode::scale_to_bounds(),
            placeholder: PlaceHolderSize::JumpCut,
            overlay_clip: OverlayClip::Bounds,
        }));
        assert!(!shared_clip_for_kind(&SharedKind::Bounds {
            resize: ResizeMode::RemeasureToBounds,
            placeholder: PlaceHolderSize::ContentSize,
            overlay_clip: OverlayClip::Bounds,
        }));
        // The marker's `overlayClip` reaches the render verbatim, and an Element end
        // (which has no such parameter) keeps the morph-shaped default.
        assert_eq!(
            shared_overlay_clip_for_kind(&SharedKind::Bounds {
                resize: ResizeMode::scale_to_bounds(),
                placeholder: PlaceHolderSize::JumpCut,
                overlay_clip: OverlayClip::None,
            }),
            OverlayClip::None
        );
        assert_eq!(
            shared_overlay_clip_for_kind(&SharedKind::Element {
                placeholder: PlaceHolderSize::JumpCut
            }),
            OverlayClip::Bounds
        );
        // Layout contract: the marker's resize/placeholder reach the flight
        // verbatim (no degradation is left in the pipeline).
        assert!(matches!(
            shared_resize_for_kind(&SharedKind::Bounds {
                resize: ResizeMode::RemeasureToBounds,
                placeholder: PlaceHolderSize::AnimatedSize,
                overlay_clip: OverlayClip::Bounds,
            }),
            ResizeMode::RemeasureToBounds
        ));
        assert_eq!(
            shared_placeholder_for_kind(&SharedKind::Bounds {
                resize: ResizeMode::RemeasureToBounds,
                placeholder: PlaceHolderSize::AnimatedSize,
                overlay_clip: OverlayClip::Bounds,
            }),
            PlaceHolderSize::AnimatedSize
        );
        // Compose's `sharedElement` always re-measures (its KDoc: "will
        // re-measure and relayout its child layout using fixed constraints
        // derived from its animated size") — Element markers must therefore
        // resolve to RemeasureToBounds, not to a scale.
        assert!(
            matches!(
                shared_resize_for_kind(&SharedKind::Element {
                    placeholder: PlaceHolderSize::JumpCut
                }),
                ResizeMode::RemeasureToBounds
            ),
            "sharedElement re-measures, like Compose"
        );
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
            scene_alpha: None,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path,
            bounds_fx: None,
            elevated: false,
            remeasure: false,
            scale_parts: None,
            radius_from_auto: false,
            radius_to_auto: false,
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
            scene_alpha: None,
            radius_from: [0.0; 4],
            radius_to: [0.0; 4],
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 0,
            path: PathMotion::Linear,
            bounds_fx: fx,
            elevated: false,
            remeasure: false,
            scale_parts: Some((ContentScale::FillBounds, ImageAlignment::TopStart)),
            radius_from_auto: false,
            radius_to_auto: false,
        };
        let fade_in = VisibilityTransition::fade_in(TweenSpec::default());
        let fade_out = VisibilityTransition::fade_out(TweenSpec::default());
        let empty = VisibilityTransition::empty();
        // Element: classic crossfade, always.
        assert_eq!(mk(TransitionRole::Target, None).alpha(), 0.25);
        assert_eq!(mk(TransitionRole::Source, None).alpha(), 0.75);
        // A SCENE owns opacity when the end carries one: alpha() is the scene's visibility, NOT the
        // flight's crossfade. Compose splits it the same way (the scene transition fades, the shared
        // bounds animation moves), and fading an element twice would dim it twice as fast. Checked
        // with a fade pair attached too, so the scene value wins over the channels.
        for role in [TransitionRole::Source, TransitionRole::Target, TransitionRole::Morph] {
            for (vis, want) in [(0.0, 0.0), (0.4, 0.4), (1.0, 1.0)] {
                let mut v = mk(role.clone(), Some((fade_in.clone(), fade_out.clone())));
                v.scene_alpha = Some(vis);
                assert_eq!(v.alpha(), want, "scene visibility must own opacity for {role:?}");
                let mut v = mk(role.clone(), None);
                v.scene_alpha = Some(vis);
                assert_eq!(v.alpha(), want, "…and with no enter/exit either");
            }
        }
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


/// Quarter-ellipse arc center (Compose `ArcSpline.Arc` math): `X = Cx +
/// A·sin θ`, `Y = Cy + B·cos θ`, θ swept 0→π/2 through an arc-length table
/// so travel is uniform along the curve. Orientation follows travel
/// direction + Above/Below exactly like AOSP (vertical time runs backward;
/// `Below→DownArc`, `Above→UpArc`). Either-dimension travel below epsilon
/// falls back to linear (Compose rule — axis-aligned flights stay straight).
/// Progress outside [0,1] (spring overshoot) pins the center at the nearer
/// endpoint — a deliberate deviation from AOSP extrapolation: Winia
/// overshoot rides scalar progress, and size/radii still overshoot visibly.


/// Opacity a SCENE owns for a flight end, if that end belongs to one (see
/// [`TransitionVisual::alpha`]): the visibility of the scene the end was composed in, read right
/// now. During a nav transition the scenes crossfade as layers, so the flight must not fade the same
/// element a second time — Compose splits it the same way (the scene transition owns opacity, the
/// shared-bounds animation owns the rect).
///
/// `leaving_end` matters because the two ends are painted from different places: a detached source
/// ghost is rendered from the transition layer, where the scene's own layer fade no longer applies,
/// so the flight carries that fade; a target that stays in tree is already faded by its scene's
/// layer, so the flight keeps it opaque unless the scene host elevates it into the layer too.
fn scene_alpha_for_end(nodes: &[LayoutNode], idx: usize, leaving_end: bool) -> Option<f32> {
    let marker = find_shared_marker(&nodes.get(idx)?.modifier)?;
    // Scene membership from the ANCESTOR chain, like the pairing path: the marker's own `scene` field is
    // captured when its modifier element is built and then reused across frames, so it freezes (measured:
    // one end reported None while the other reported the LEAVING scene). Reading the frozen field here
    // meant an in-tree end could double-fade (None while its scene also fades it) or never fade (a stale
    // Some(1.0) after its scene stopped fading it).
    scene_of_node(nodes, idx)?;
    if leaving_end || marker.render_in_overlay {
        // The flight paints this end from the transition layer, and the scene's own layer fade does
        // NOT apply there — so the flight has to fade it, and it must do so on the FLIGHT's clock so
        // that rect and opacity stay in step. Measured before this: the detached ghost faded on the
        // nav's 800 ms clock while its rect followed a 1.3 s spring, so the leaving end disappeared
        // before it had finished growing and the flight read as "only the entering end animates".
        None
    } else {
        // Painted in tree: its scene already fades it, so the flight keeps it opaque.
        Some(1.0)
    }
}


/// Per-frame layout instruction for a flight endpoint (Compose `ResizeMode` /
/// `PlaceHolderSize`), shared by the Tier 0 and Tier 1 writers.
fn placeholder_frame(
    remeasure: bool,
    animated: crate::unit::Size,
    placeholder: PlaceHolderSize,
    target_size: Option<crate::unit::Size>,
) -> FlightMeasureFrame {
    FlightMeasureFrame {
        // RemeasureToBounds: animated fixed constraints → the subtree reflows.
        content: remeasure.then_some(animated),
        // What the parent observes: AnimatedSize reflows the siblings,
        // ContentSize / JumpCut keep the target size (Compose's default).
        reported: match placeholder {
            PlaceHolderSize::AnimatedSize => Some(animated),
            PlaceHolderSize::ContentSize | PlaceHolderSize::JumpCut => {
                if remeasure {
                    target_size
                } else {
                    None
                }
            }
        },
    }
}

/// The lerped rect's size (the arc bends the centre only, never the size).
fn animated_size(
    start: &SharedBounds,
    end: &SharedBounds,
    p: f32,
    path: PathMotion,
) -> crate::unit::Size {
    let l = lerp_flight_rect(start, end, p, path);
    crate::unit::Size::new(l.width, l.height)
}


// ═══════════════════════════════════════════════════════════
// Phase 2: Tier-0 coordinator (compose/layout/app-loop hooks)
// ═══════════════════════════════════════════════════════════


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


/// Test-only: mark a scope as transitioning (or idle) without a real flight.
///
/// A morph now only opens while its scope has a running transition (Compose ties the two together), so
/// tests that used to start one from a bare layout change mark the scope active first. The coordinator's
/// end-of-poll full sync clears it again, which is fine: the morph opens during that same poll and
/// continues through the ungated reopen path.
#[cfg(test)]
pub(crate) fn set_scope_transition_active_for_test(scope_id: u64, active: bool) {
    SCOPE_ACTIVE
        .lock()
        .unwrap()
        .entry(scope_id)
        .or_insert_with(|| State::new(false))
        .set(active);
}

/// Is a transition running in this scope right now — i.e. does any non-terminal flight belong to it?
///
/// The same question [`SharedTransitionScope::is_transition_active`] answers for composition, asked by
/// [`SharedTransitionScope`]-less code (the coordinator's morph detection) from the synced global table,
/// so both tiers agree. Scopes nobody has read yet count as idle.
pub(crate) fn scope_transition_active(scope_id: u64) -> bool {
    SCOPE_ACTIVE
        .lock()
        .unwrap()
        .get(&scope_id)
        .map(|s| s.get())
        .unwrap_or(false)
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
        // Two LIVE ends of one key happen when a scene host keeps the outgoing and the incoming
        // scene composed at once (winia's nav). Which of them is "the" live end decides the whole
        // flight: the tree walk's order is not meaningful, and during a nav transition it alternates
        // between the two scenes every frame, so the flight system reads a fresh switch each frame
        // and starts/retargets flights instead of flying once (measured: 2 `begin_flight` calls per
        // navigate, 131-143 duplicate warnings).
        //
        // The winner is the end in the scene that is NOT leaving (`nav_scene_is_prev`), which is stable for
        // the whole transition. It is deliberately NOT "the more visible end": a leaving scene starts at
        // visibility 1 and the entering one at 0, so comparing visibilities picks the LEAVING end at the
        // switch frame (measured: the flight's target was the 96x96 list hero and it never grew). With no
        // scene information at all, the last one wins as before.
        let mut out: HashMap<(u64, String), (u64, Option<bool>)> = HashMap::new();
        if let Some(root) = self.arena.root {
            // Built ONCE for the whole walk: `scene_of_node` used to build this map per call, and this
            // walk calls it once per marked node (measured: 2.68 ms per call on a 12 000-node tree, so
            // 50 marked nodes cost ~134 ms per frame).
            let id_to_idx: HashMap<u64, usize> = self
                .arena
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id, i))
                .collect();
            let mut stack = vec![root];
            while let Some(idx) = stack.pop() {
                let node = &self.arena.nodes[idx];
                if let Some(m) = find_shared_marker(&node.modifier) {
                    // Scene membership comes from the ANCESTOR chain, not from the marker: a marker's
                    // modifier element is built once and reused, so an id stored there freezes its
                    // first value (measured: one end `scene=None`, the other the LEAVING scene, which
                    // made a nav flight pair the leaving hero with itself).
                    //
                    // The winner is the end in the scene that is NOT leaving — the surviving side. It is
                    // deliberately not "the more visible end": a leaving scene starts at visibility 1
                    // and the entering one at 0, so comparing visibilities picks the LEAVING end at the
                    // switch frame (measured: the flight's target was the 96x96 list hero, so it
                    // crossfaded without ever growing while a same-screen morph supplied the growth).
                    let scene = scene_of_node_with(&self.arena.nodes, &id_to_idx, idx);
                    let is_prev = scene.and_then(nav_scene_is_prev);
                    let key = (m.scope_id, m.key.clone());
                    match out.get(&key) {
                        Some(&(_, prev_is_prev)) => {
                            let replace = match (prev_is_prev, is_prev) {
                                // Prefer the surviving scene; if both are the same side (or a scene
                                // host is not involved at all), keep the last one, as before.
                                (Some(true), Some(false)) => true,
                                (Some(false), Some(true)) => false,
                                _ => true,
                            };
                            if replace {
                                out.insert(key, (node.slot_key, is_prev));
                            }
                        }
                        None => {
                            out.insert(key, (node.slot_key, is_prev));
                        }
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        out.into_iter().map(|(k, (slot, _))| (k, slot)).collect()
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
                n.paint = PaintDisposition::InTree;
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
                        // Keep descending: a chrome marker nested inside another one gets its OWN layer entry
                        // (`rebuild_layer_order` includes every entry of this list, and `render_layer` draws
                        // each one with `layer_root = true`), which is what keeps its own `zIndexInOverlay`
                        // in the sort and lets it escape its ancestor's clip. An earlier version skipped it
                        // on the theory that it would be skipped twice and paint nowhere; that was wrong (it
                        // was painted exactly once, as its own entry) and suppressing it silently downgraded
                        // the nested bar to an ordinary child of its ancestor's draw.
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        for &idx in &marked {
            self.arena.nodes[idx].paint = PaintDisposition::InLayer;
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
            if n.transition.is_none() && n.paint != PaintDisposition::InLayer {
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
            self.cancel_flight(id, "key_left_live_tree");
        }
        // Switches (fresh + retarget-lite).
        for c in detect_switch(&self.prev_shared_endpoints, &live) {
            if crate::anim_trace::enabled() {
                crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
                    format!("flight:{}", c.key),
                    "candidate",
                    format!("old={:#x} new={:#x}", c.old_slot, c.new_slot),
                ));
            }
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
                // Two shapes of candidate for an ALREADY-FLYING key, and they need opposite handling:
                //
                // * The end that moved is this flight's target: the element is the same one (same scope,
                //   same key) and it was merely re-slotted inside its scene while the flight runs — a
                //   scene host re-arranges layers mid-transition and winia slots are positional. Compose's
                //   identity here is the KEY (`rememberSharedContentState(key)`), not the position, and a
                //   position change updates the node in place. Retargeting this case cancelled a running
                //   crossfade and then bailed in `detach_source` (the old slot was already gone), so
                //   nothing flew and a same-screen morph — opacity-immune by design — took over.
                //   Rebind: keep the progress and the start rect, re-point the target slot, clear the
                //   stale node's visuals.
                // * The candidate's slots are this flight's own two ends REVERSED: that is a genuine
                //   navigation back (measured: clicking Back while the push flight still runs produces
                //   exactly `old=target new=source`), so it must go through retarget below.
                let target_moved = self
                    .shared_flights
                    .get(&id)
                    .is_some_and(|a| {
                        a.flight.target_slot == Some(c.old_slot)
                            && a.flight.source_slot != Some(c.new_slot)
                            && a.source_idx.is_some()
                    });
                if target_moved {
                    if let Some(a) = self.shared_flights.get_mut(&id) {
                        a.flight.target_slot = Some(c.new_slot);
                    }
                    let key = self
                        .shared_flights
                        .get(&id)
                        .map(|a| a.flight.key.clone())
                        .unwrap_or_default();
                    self.clear_transition_for_slot(c.old_slot, FlightKey { cid: self.composer_id, id });
                    if crate::anim_trace::enabled() {
                        crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
                            format!("flight:{key}"),
                            "rebind",
                            format!("target {:#x} -> {:#x}", c.old_slot, c.new_slot),
                        ));
                    }
                    continue;
                }
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
                self.cancel_flight(id, "retarget_same_key");
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
                    radius_from_auto: self
                        .arena
                        .nodes
                        .get(src_idx)
                        .is_some_and(|n| shared_shape_radius_is_auto(&n.modifier)),
                });
            }
        }
        self.prev_shared_endpoints = live;
        // The listing prune used to be called here, which quietly tied an arena invariant to this
        // hook's fast path (`return` above when nothing is shared). It now runs in `compose()`
        // right after this call — same position in the frame, but for every composer.
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
        // A ghost must not own what the NEW tree already took over. `reused_nodes` at this point
        // holds exactly this frame's reuse decisions (materialize inserts them and the set is
        // cleared at the end of each compose), so a child listed there has been re-parented into
        // the frame's tree. Keeping it would make the ghost paint a LIVE node and then free it:
        // teardown releases a ghost's subtree with an empty skip set (probe from review:
        // `live_parent.children=[2] live_child.slot_key=0x0 pool=[2, 3]`). Compose never shares a
        // node between the outgoing and incoming content, so dropping it is also the aligned
        // behaviour; a true fix would freeze a copy of the subtree.
        {
            let taken: Vec<usize> = self.arena.nodes[src_idx]
                .children
                .iter()
                .copied()
                .filter(|c| self.reused_nodes.contains(*c))
                .collect();
            if !taken.is_empty() {
                crate::debug_log!(
                    "[detach] ghost {src_idx} drops {} child(ren) the new tree reused: {taken:?}",
                    taken.len()
                );
                self.arena.nodes[src_idx]
                    .children
                    .retain(|c| !taken.contains(c));
            }
        }
        // …and neither may it reclaim any DESCENDANT. Sheltering only the root let the pool hand
        // this subtree's indices to nodes of the new tree, so the frozen ghost listed a child
        // that had been recycled into a fresh, not-yet-measured node: measured, the ghost's only
        // child was `0x0` while the ghost's own rect, alpha and layer order were all correct — a
        // card that flies with nothing inside it, i.e. the reported "the animation starts fully
        // transparent". A frozen ghost owns its subtree for as long as it is in the layer.
        {
            let mut stack = self.arena.nodes[src_idx].children.clone();
            while let Some(c) = stack.pop() {
                if self.reused_nodes.insert(c) {
                    stack.extend(self.arena.nodes[c].children.iter().copied());
                }
            }
        }
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
            n.position = crate::unit::Offset::new(b.x - ox, b.y - oy);
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
        // anim-trace lifecycle event: which key opened a flight, and between which slots.
        let scene_of = |slot: u64| {
            self.arena
                .root
                .and_then(|r| find_idx_by_slot(&self.arena.nodes, r, slot))
                .map(|i| scene_of_node(&self.arena.nodes, i))
                .unwrap_or(None)
        };
        crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
            format!("flight:{}", c.key),
            "start",
            format!(
                "id={id} old={:#x}(scene={:?}) new={:#x}(scene={:?})",
                c.old_slot,
                scene_of(c.old_slot),
                c.new_slot,
                scene_of(c.new_slot)
            ),
        ));
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
                // Source kind is known now; the target's is filled at resolve
                // (same moment as clip/resize/placeholder).
                radius_from_auto: self
                    .arena
                    .nodes
                    .get(src_idx)
                    .is_some_and(|n| shared_shape_radius_is_auto(&n.modifier)),
                radius_to_auto: false,
                clip: false,
                // Filled when the end resolves (AwaitingBounds poll).
                target_in_overlay: true,
                // Layout contract: frozen at resolve from the target marker.
                resize: ResizeMode::scale_to_bounds(),
                placeholder: PlaceHolderSize::JumpCut,
                target_size: None,
                measure: State::new(FlightMeasureFrame::IDLE),
                source_cid: self.composer_id,
                target_cid: self.composer_id,
            },
        );
    }

    /// Free a retained source subtree + drop its slot (fires on_remove —
    /// removal semantic) + clear the target's visuals if still present.
    /// Is a transition running in this scope NOW?
    ///
    /// Checks this composer's own map first, so a flight opened EARLIER IN THIS SAME FRAME counts (the
    /// switch handling runs before the morph detection): the globally synced table is only updated at the
    /// end of a poll, so it would say "idle" on the very frame a transition starts. The global table is
    /// still consulted for Tier 1, whose flights live in the main composer's map.
    fn scope_is_transitioning(&self, scope_id: u64) -> bool {
        self.shared_flights
            .values()
            .any(|a| a.flight.scope_id == scope_id && !is_terminal(a.flight.phase))
            || scope_transition_active(scope_id)
    }

    fn cancel_flight(&mut self, id: FlightId, reason: &'static str) {
        let Some(a) = self.shared_flights.remove(&id) else {
            return;
        };
        // anim-trace: a cancelled flight is the usual reason a crossfade stops mid-way, and nothing
        // else in a trace says so — record the key, the progress it died at, and WHICH call site did
        // it (five of them can, and they mean different bugs). Gated: with the feature off `record` is a
        // no-op but its arguments would still be built.
        if crate::anim_trace::enabled() {
            crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
                format!("flight:{}", a.flight.key),
                "cancel",
                format!(
                    "reason={reason} id={id} has_source={} p={:.4} flight_slots=({:?},{:?})",
                    a.source_idx.is_some(),
                    a.progress.peek(),
                    a.flight.source_slot,
                    a.flight.target_slot
                ),
            ));
        }
        if let (Some(slot), Some(idx)) = (a.flight.source_slot, a.source_idx) {
            self.free_retained_source(idx, slot);
        }
        if let Some(slot) = a.flight.target_slot {
            self.clear_transition_for_slot(slot, FlightKey { cid: self.composer_id, id });
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
            let mut visited = crate::layout::node::NodeMarks::default();
            self.arena.free_node_skip(idx, &crate::layout::node::NodeMarks::default(), &mut visited);
        }
        self.transition_layer.retain(|&x| x != idx);
    }

    /// Clear a slot's visual only when it still belongs to `id`. Slots are
    /// positional identities a successor flight can resurrect between this
    /// flight's last write and its teardown — unconditional clearing would
    /// flicker one frame off the new visual (self-heals next poll, but
    /// avoidable for one tag comparison).
    fn clear_transition_for_slot(&mut self, slot: u64, owner: FlightKey) {
        // `arena.root` is None on an empty composition, but the nodes (and the
        // key→index map) can outlive it, so a rootless tree must still be swept
        // — otherwise an orphan override rides a key-reused node.
        let found = match self.arena.root {
            Some(root) => find_idx_by_slot(&self.arena.nodes, root, slot),
            None => self.arena.nodes.iter().position(|n| n.slot_key == slot),
        };
        if let Some(idx) = found {
            let owned = self.arena.nodes[idx]
                .transition
                .as_ref()
                .is_some_and(|t| t.flight == owner.id);
            if owned {
                self.arena.nodes[idx].transition = None;
            }
            // The layout override lives exactly as long as the flight it belongs
            // to: drop it (only when it is still THIS flight's) and seed one last
            // invalidation so the natural size comes back next pass — otherwise
            // the fold keeps the last animated size forever.
            if self.arena.nodes[idx]
                .flight_measure
                .as_ref()
                .is_some_and(|f| f.owner == owner)
            {
                let key = self.arena.nodes[idx].slot_key;
                self.arena.nodes[idx].flight_measure = None;
                self.layout_dirty_keys.insert(key);
            }
        }
        // …AND an arena-wide sweep, because the tree walk above cannot reach a
        // node that was DETACHED into the transition layer in the same frame
        // (retarget/replace): the ghost would keep its dead flight's override.
        // That is harmless today only because detached nodes are never measured —
        // an accident, not an invariant.
        let mut orphan_keys: Vec<u64> = Vec::new();
        for n in self.arena.nodes.iter_mut() {
            if n.flight_measure.as_ref().is_some_and(|f| f.owner == owner) {
                n.flight_measure = None;
                orphan_keys.push(n.slot_key);
            }
        }
        for k in orphan_keys {
            self.layout_dirty_keys.insert(k);
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
        // Paint dispositions follow from both of the above, so they are the last thing computed each
        // frame (including which marked copies are placeholders a flight already paints).
        self.refresh_paint_dispositions();
        self.trace_marked_nodes();
        sync_scope_active_states(&[self]);
    }

    /// Recompute [`crate::layout::node::PaintDisposition`] for every node, once per frame.
    ///
    /// One place decides where each node's pixels come from, instead of three independent booleans read
    /// at the render site:
    /// - an end the flight lifted into the transition layer → `InLayer`;
    /// - chrome that opted into the scope overlay this frame → `InLayer`;
    /// - a marked copy of a (scope, key) a running flight already owns, which is not itself an end →
    ///   `Placeholder` (the flight paints the animated rect; a second static copy must not appear);
    /// - everything else → `InTree`.
    ///
    /// Assigning unconditionally each frame is what keeps it transient: a node whose flight ended goes
    /// straight back to `InTree` without any per-flight cleanup.
    ///
    /// `paint_dirty` makes the idle case cheap: with no flights, no chrome and nothing left to clear, the
    /// whole-arena walk is skipped. It is set whenever the pass assigns anything other than `InTree`, so the
    /// pass always runs at least once more after the last non-`InTree` frame (that is what clears them).
    fn refresh_paint_dispositions(&mut self) {
        let flying: Vec<(u64, String)> = self
            .shared_flights
            .values()
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
            .collect();
        self.refresh_paint_dispositions_with(&flying);
    }

    /// The pass with a caller-provided flight set. The window-level caller passes the UNION of every
    /// composer's non-terminal flights, because a Tier1 flight lives only in the main composer's map: a peer
    /// that asked its own map would keep painting a second copy of a key that is flying.
    fn refresh_paint_dispositions_with(&mut self, flying: &[(u64, String)]) {
        if flying.is_empty() && self.scope_overlay_roots.is_empty() && !self.paint_dirty {
            // NOTE: the scene-registry prune deliberately does NOT happen here. The registry is shared by
            // every composer on the thread, so its live set has to be the union of their arenas; pruning from
            // one composer's walk let a peer WITHOUT scene tags delete the entries of the composer that has
            // them (a peer with its own flight runs this pass with an empty tag set), which degraded the
            // pairing to last-wins for the rest of a transition. The window-scoped prune is
            // `prune_scene_registry_window`, called from `Composer::poll_cross_flights` where every composer
            // is at hand.
            return;
        }
        let chrome: HashSet<usize> = self.scope_overlay_roots.iter().copied().collect();
        let mut dirty = false;
        for (idx, n) in self.arena.nodes.iter_mut().enumerate() {
            // Compared against a small Vec rather than a HashSet of owned keys: probing the set needs an
            // owned `(u64, String)` per marked node (an allocation per node per frame), and the flight list
            // is a handful of entries at most.
            let placeholder = !chrome.contains(&idx)
                && n.transition.is_none()
                && find_shared_marker(&n.modifier)
                    .is_some_and(|m| flying.iter().any(|(s, k)| *s == m.scope_id && k == &m.key));
            n.paint = if chrome.contains(&idx) {
                PaintDisposition::InLayer
            } else if placeholder {
                PaintDisposition::Placeholder
            } else {
                PaintDisposition::InTree
            };
            dirty |= n.paint != PaintDisposition::InTree;
        }
        self.paint_dirty = dirty;
    }


    /// anim-trace: record EVERY marked node this frame — not just the ends of a flight. "How many
    /// copies of one key are painted, and where" is otherwise only answerable by looking at pixels, and
    /// a duplicate (a scene host re-composing a copy of a key that a flight already owns) shows up here
    /// as two records with the same `key`, different rects, one of them with `role: null`.
    fn trace_marked_nodes(&self) {
        if !crate::anim_trace::enabled() {
            return;
        }
        // One id→index map for the whole pass (see `scene_of_node_with`).
        let id_to_idx: HashMap<u64, usize> = self
            .arena
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id, i))
            .collect();
        for (idx, n) in self.arena.nodes.iter().enumerate() {
            let Some(m) = find_shared_marker(&n.modifier) else {
                continue;
            };
            let role = n.transition.as_ref().map(|t| match t.role {
                TransitionRole::Source => "Source",
                TransitionRole::Target => "Target",
                TransitionRole::Morph => "Morph",
            });
            let mut rec = crate::anim_trace::TraceRecord::flight(format!(
                "mark:{}#{}",
                m.key,
                role.unwrap_or("plain")
            ));
            rec.kind = crate::anim_trace::TraceKind::Node;
            rec.scope = Some(m.scope_id);
            rec.key = Some(m.key.clone());
            rec.role = role;
            rec.phase = Some(match n.paint_disposition() {
                PaintDisposition::InLayer => "layer",
                PaintDisposition::Placeholder => "placeholder",
                PaintDisposition::InTree => "tree",
            });
            rec.scene = scene_of_node_with(&self.arena.nodes, &id_to_idx, idx);
            // Absolute (canvas) coords: `node.position` is parent-relative, so reporting it raw made a
            // copy look like it sat at (0,0) and could not be compared with a flight's lerped rect
            // (measured, then fixed here).
            let (ax, ay) = self
                .arena
                .root
                .map(|r| crate::app::node_abs_position(&self.arena.nodes, r, n.id))
                .unwrap_or((n.position.x, n.position.y));
            rec.layout = Some(crate::anim_trace::TraceRect::new(
                ax,
                ay,
                n.measured_size.width,
                n.measured_size.height,
            ));
            // What this node paints by itself: the flight's lerped rect while it is an end, otherwise
            // its own content box at its own layout origin.
            let (rect, alpha) = match n.transition.as_ref() {
                Some(t) => {
                    let l = t.lerped();
                    (
                        crate::anim_trace::TraceRect::new(l.x, l.y, l.width, l.height),
                        t.alpha(),
                    )
                }
                None => {
                    let cb = n.content_box();
                    (
                        crate::anim_trace::TraceRect::new(ax, ay, cb.width, cb.height),
                        1.0,
                    )
                }
            };
            rec.painted = Some(rect);
            rec.alpha = Some(alpha);
            rec.effective_alpha = Some(alpha);
            crate::anim_trace::record(rec);
        }
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
                        self.cancel_flight(id, "awaiting_no_root");
                        return;
                    }
                };
                let tidx = match target_slot.and_then(|s| find_idx_by_slot(&self.arena.nodes, root, s)) {
                    Some(i) => i,
                    None => {
                        self.cancel_flight(id, "awaiting_target_not_found");
                        return;
                    }
                };
                let tid = self.arena.nodes[tidx].id;
                let (tx, ty) = crate::app::node_abs_position(&self.arena.nodes, root, tid);
                let (tw, th) = {
                    let n = &self.arena.nodes[tidx];
                    // The target's own content box, NOT `measured_size`: while a flight reports a
                    // placeholder size the parent sees the animated size, so reading `measured_size`
                    // here can capture the flight's own current rect as the destination. Measured with
                    // anim-trace on the nav demo: with `measured_size` both ends reported a painted
                    // width of 96 for the whole flight, i.e. the crossfade faded without ever growing,
                    // and a separate same-screen morph (opaque by design) supplied the visible growth.
                    let cb = n.content_box();
                    (cb.width, cb.height)
                };
                // Canonicalize to window coords (overlay-local + screen origin).
                let (ox, oy) = self.screen_origin;
                // Gated: with the feature off `record` is a no-op but its arguments (including a
                // `scene_of_node` ancestry walk) would still be evaluated.
                if crate::anim_trace::enabled() {
                    crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
                        format!(
                            "flight:{}",
                            find_shared_marker(&self.arena.nodes[tidx].modifier)
                                .map(|m| m.key)
                                .unwrap_or_default()
                        ),
                        "resolve",
                        format!(
                            // `id=` lets a report pair this announcement with the resolved end's records
                            // (the event's subject carries only the key, not the role or the flight).
                            "id={id} tidx={tidx} own_size={:?} measured={:?} content_box={:?} scene={:?} end=({tw:.0},{th:.0})",
                            self.arena.nodes[tidx].modifier.elements().iter().find_map(|el| match el {
                                ModifierElement::Size { .. } => Some(format!("{el:?}")),
                                _ => None,
                            }),
                            self.arena.nodes[tidx].measured_size,
                            self.arena.nodes[tidx].content_box(),
                            scene_of_node(&self.arena.nodes, tidx),
                        ),
                    ));
                }
                let end = SharedBounds::new(tx + ox, ty + oy, tw, th);
                let marker = find_shared_marker(&self.arena.nodes[tidx].modifier);
                let (
                    spec,
                    clip,
                    path,
                    enter,
                    target_is_bounds,
                    target_in_overlay,
                    resize,
                    placeholder,
                ) = match marker {
                    Some(m) => (
                        m.transform.spec.clone(),
                        shared_clip_for_kind(&m.kind),
                        m.path,
                        m.enter,
                        matches!(m.kind, SharedKind::Bounds { .. }),
                        m.render_in_overlay,
                        shared_resize_for_kind(&m.kind),
                        shared_placeholder_for_kind(&m.kind),
                    ),
                    None => (
                        BoundsTransform::default().spec,
                        false,
                        PathMotion::Linear,
                        None,
                        false,
                        true,
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                    ),
                };
                // Enter from the target marker, exit from the retained source
                // marker — each side declares its own. Only Bounds flights carry
                // the pair (an Element flight crossfades both ends in winia,
                // which is a recorded deviation from Compose's sharedElement);
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
                        // The pair exists for the log below, which folds away without the feature.
                        let (_scope, _key) = self
                            .shared_flights
                            .get(&id)
                            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
                            .unwrap_or((0, String::new()));
                        crate::debug_log!(
                            "[shared] mixed-kind pair scope={_scope} key={_key} — flight follows the target side"
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
                    a.resize = resize.clone();
                    a.placeholder = placeholder;
                    // The target was measured naturally in this frame's layout
                    // (the flight had no bounds yet), so this IS its resting
                    // size — the baseline the placeholder policies compare to.
                    a.target_size = Some(self.arena.nodes[tidx].measured_size);
                    // Corner KIND of the target end (percent vs fixed): the two
                    // ends are resolved separately and then mixed by progress.
                    a.radius_to_auto =
                        shared_shape_radius_is_auto(&self.arena.nodes[tidx].modifier);
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
                        self.clear_transition_for_slot(slot, FlightKey { cid: self.composer_id, id });
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
                a.resize.clone(),
                a.placeholder,
                a.target_size,
                a.measure.clone(),
                a.radius_from_auto,
                a.radius_to_auto,
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
            resize,
            placeholder,
            target_size,
            measure_state,
            radius_from_auto,
            radius_to_auto,
        ) = match snapshot {
            Some(v) => v,
            None => return,
        };
        // Identity of the endpoint this flight is chasing, for the target-slot
        // re-check below (slots are positional; a shift can hand the key away).
        let flight_key = self
            .shared_flights
            .get(&id)
            .map(|a| (a.flight.scope_id, a.flight.key.clone()));
        let Some(flight_key) = flight_key else { return };
        // Flight bounds are canonical window coords; visuals render in this
        // composer's canvas frame (main renders untranslated; overlays render
        // translated by screen_pos).
        let (ox, oy) = self.screen_origin;
        start.x -= ox;
        start.y -= oy;
        end.x -= ox;
        end.y -= oy;
        if let (Some(slot), Some(idx)) = (sslot, sidx) {
            let matched = self.arena.nodes.get(idx).is_some_and(|n| n.slot_key == slot);
            if matched {
                self.arena.nodes[idx].transition = Some(TransitionVisual {
                    start,
                    end,
                    progress: p,
                    scene_alpha: scene_alpha_for_end(&self.arena.nodes, idx, true),
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
                    // Frozen ghost: no live layout to re-measure.
                    remeasure: false,
                    scale_parts: None,
                    radius_from_auto,
                    radius_to_auto,
                });
            }
        }
        if let Some(slot) = tslot {
            if let Some(root) = self.arena.root {
                if let Some(tidx) = find_idx_by_slot(&self.arena.nodes, root, slot) {
                    // IDENTITY RE-CHECK (review 2, R2-F5): slots are positional, so a
                    // recomposition that shifts the target's position can hand this
                    // frozen key to an unrelated element — which would then take the
                    // flight's visual AND its layout override (measured: an unrelated
                    // 20x20 leaf laid out at the flight's reported size for one pass).
                    // Tier 1 has the equivalent guard (its staleness check); Tier 0
                    // re-checks the marker instead.
                    let still_ours = find_shared_marker(&self.arena.nodes[tidx].modifier)
                        .is_some_and(|m| {
                            m.scope_id == flight_key.0 && m.key == flight_key.1
                        });
                    if !still_ours {
                        return;
                    }
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
                    // Layout contract (Compose `ResizeMode` / `PlaceHolderSize`).
                    // Only written when it overrides something, so the default
                    // path (ScaleToBounds + JumpCut/ContentSize) never touches
                    // the layout.
                    let remeasure = role == TransitionRole::Target
                        && matches!(resize, ResizeMode::RemeasureToBounds);
                    let frame = placeholder_frame(
                        remeasure,
                        animated_size(&start, &end, p, path),
                        placeholder,
                        target_size,
                    );
                    measure_state.set(frame);
                    let key = self.arena.nodes[tidx].slot_key;
                    let mine = FlightKey { cid: self.composer_id, id };
                    // Never clobber (or clear) a DIFFERENT flight's override:
                    // ids are per-composer, so another flight can legitimately
                    // own this node's override while this one is being torn down.
                    let foreign = self.arena.nodes[tidx]
                        .flight_measure
                        .as_ref()
                        .is_some_and(|f| f.owner != mine);
                    let had_override =
                        !foreign && self.arena.nodes[tidx].flight_measure.is_some();
                    if !foreign {
                        self.arena.nodes[tidx].flight_measure = if frame.is_idle() {
                            None
                        } else {
                            Some(FlightMeasure { frame: measure_state, owner: mine })
                        };
                    }
                    // `layout()` resets `layout_dirty` on the whole tree at the
                    // start of every pass, so an override has to be re-seeded
                    // EVERY frame — otherwise the folded parent never descends
                    // into it. The seed is skipped when nothing changed: the
                    // default contract (no override attached, none to drop)
                    // must not force a re-measure of the ancestor chain.
                    if !frame.is_idle() || had_override {
                        self.layout_dirty_keys.insert(key);
                    }
                    // FIRST attach (nothing was there before): the layout that just ran
                    // reported the target's NATURAL size to its parent, because the override
                    // can only be attached here — after layout — since resolving the flight
                    // needs the target's measured rect. The parent is therefore stale WITHIN
                    // this frame, which is what made the rows below a starting flight dip
                    // and snap back. The app loop consumes this flag and re-lays out once, so
                    // the frame renders coherent (see `take_layout_override_fresh`).
                    if !frame.is_idle() && !had_override {
                        self.layout_override_fresh = true;
                    }
                    self.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start,
                        end,
                        progress: p,
                        scene_alpha: scene_alpha_for_end(&self.arena.nodes, tidx, false),
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
                        remeasure,
                        scale_parts: resize.scale_to_bounds_parts(),
                        radius_from_auto,
                        radius_to_auto,
                    });
                    if elevated {
                        self.elevated_roots.push(tidx);
                    }
                }
            }
        }
        // anim-trace: one record per end per frame — `layout` is the node's measured rect, `painted`
        // is the flight's lerped rect (what the flight draws), and opacity is recorded in three parts
        // (`alpha` = the end's own, `effective_alpha` = what the draw composes, `scene_visibility` =
        // the scene host's value) so "who faded it" is answerable without guessing.
        if crate::anim_trace::enabled() {
            let (scope_id, key) = self
                .shared_flights
                .get(&id)
                .map(|a| (a.flight.scope_id, a.flight.key.clone()))
                .unwrap_or((0, String::new()));
            let svis = self
                .shared_flights
                .get(&id)
                .and_then(|a| a.source_idx)
                .and_then(|i| self.arena.nodes.get(i))
                .and_then(|n| find_shared_marker(&n.modifier))
                .and_then(|m| m.scene)
                .and_then(nav_scene_visibility);
            for (idx, role) in self
                .arena
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| {
                    let t = n.transition.as_ref()?;
                    if t.flight != id {
                        return None;
                    }
                    Some((i, t.role.clone()))
                })
                .collect::<Vec<_>>()
            {
                let n = &self.arena.nodes[idx];
                let t = match n.transition.as_ref() {
                    Some(t) => t,
                    None => continue,
                };
                let lerped = t.lerped();
                let mut rec = crate::anim_trace::TraceRecord::flight(match role {
                    TransitionRole::Source => format!("flight:{key}#Source"),
                    TransitionRole::Target => format!("flight:{key}#Target"),
                    TransitionRole::Morph => format!("flight:{key}#Morph"),
                });
                rec.scope = Some(scope_id);
                rec.key = Some(key.clone());
                rec.role = Some(match role {
                    TransitionRole::Source => "Source",
                    TransitionRole::Target => "Target",
                    TransitionRole::Morph => "Morph",
                });
                rec.flight = Some(id);
                rec.phase = Some(if t.progress >= 1.0 { "settled" } else { "flying" });
                rec.progress = Some(t.progress);
                rec.layout = Some(crate::anim_trace::TraceRect::new(
                    n.position.x,
                    n.position.y,
                    n.measured_size.width,
                    n.measured_size.height,
                ));
                rec.painted = Some(crate::anim_trace::TraceRect::new(
                    lerped.x, lerped.y, lerped.width, lerped.height,
                ));
                rec.alpha = Some(t.alpha());
                // A detached/elevated end is drawn from the transition layer, where the scene's own
                // layer fade does not apply; an in-tree end of a scene is multiplied by it.
                let scene_vis = if t.elevated { None } else { svis };
                rec.scene_visibility = scene_vis;
                rec.effective_alpha = Some(t.alpha() * scene_vis.unwrap_or(1.0));
                rec.radii = Some(t.radii());
                rec.clip = Some(t.clip);
                rec.composer = Some(self.composer_id);
                crate::anim_trace::record(rec);
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
            // Still publish the scope-activity union for the WHOLE frame. Syncing per composer inside
            // `poll_shared_flights` lets the last composer's poll overwrite the flags of every scope it
            // does not own (the app polls the main composer first, overlays after), so
            // `is_transition_active()` reported false during a Tier-0 transition whenever any overlay was
            // present — and the morph gate's fallback read the same stale value. This call is the last
            // poll of the frame, so its set is authoritative.
            let refs: Vec<&Composer> = all.iter().map(|c| &**c).collect();
            sync_scope_active_states(&refs);
            prune_scene_registry_window(&refs);
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
        // The union of the FLIGHTS, not just their scopes: `refresh_paint_dispositions` decides placeholders
        // from the (scope, key) pairs, and a Tier1 flight lives only in the main composer's map, so a peer
        // would otherwise keep painting a second copy of a key that is flying.
        let flying_union: Vec<(u64, String)> = all
            .iter()
            .flat_map(|c| c.shared_flights.values())
            .filter(|a| !is_terminal(a.flight.phase))
            .map(|a| (a.flight.scope_id, a.flight.key.clone()))
            .collect();
        for c in all.iter_mut() {
            c.refresh_scope_overlay_roots(&union);
            c.rebuild_layer_order();
            // Recompute the dispositions, because both of the above just changed their inputs: a Tier1
            // flight that started or completed in this cross-poll would otherwise leave a duplicate copy
            // painted (or a placeholder hidden) for one frame, since each composer's own poll ran earlier.
            c.refresh_paint_dispositions_with(&flying_union);
        }
        let refs: Vec<&Composer> = all.iter().map(|c| &**c).collect();
        prune_scene_registry_window(&refs);
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
                    let mut visited = crate::layout::node::NodeMarks::default();
                    owner.arena.free_node_skip(idx, &crate::layout::node::NodeMarks::default(), &mut visited);
                }
                owner.transition_layer.retain(|&x| x != idx);
            }
            if let Some(slot) = tslot {
                // Route through the shared teardown so the peer's layout
                // override (Compose `ResizeMode`/`PlaceHolderSize`) is dropped
                // and its natural size restored — clearing only the visual
                // would freeze the peer at its last animated size forever.
                // The override carries the OWNER's identity (the composer whose
                // map holds the flight = main), not the peer's: that is what the
                // cross-writer stamps on the node.
                let owner_key = FlightKey { cid: all[0].composer_id, id };
                let peer = &mut all[ti];
                peer.clear_transition_for_slot(slot, owner_key);
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
                    let mut visited = crate::layout::node::NodeMarks::default();
                    owner.arena.free_node_skip(idx, &crate::layout::node::NodeMarks::default(), &mut visited);
                }
                owner.transition_layer.retain(|&x| x != idx);
            }
        }
        if let Some(slot) = a.flight.target_slot {
            // Same as the completion path: the peer's layout override must go
            // with the flight (see `clear_transition_for_slot`). Verified
            // load-bearing: reverting both Tier-1 clears makes
            // `stale_cancel_drops_the_surviving_peers_layout_override` fail with
            // the override still attached. Owner = the composer that held the
            // flight, i.e. the one whose map it was removed from — `all[0]`, the
            // same key the writer STAMPS. Using the source's cid here was wrong for a
            // PEER-SOURCED flight (overlay -> main, the "hero returns" direction): it
            // looked for a key nobody wrote, so the override was never cleared and the
            // element stayed frozen at the outgoing size forever (review round 3,
            // reproduced with a live override dump).
            let owner_key = FlightKey { cid: all[0].composer_id, id };
            if let Some(peer) = all.iter_mut().find(|c| c.composer_id == a.target_cid) {
                peer.clear_transition_for_slot(slot, owner_key);
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
                a.resize.clone(),
                a.placeholder,
                a.target_size,
                a.measure.clone(),
                a.radius_from_auto,
                a.radius_to_auto,
            )
        });
        let (
            start,
            end,
            pr,
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
            in_overlay,
            resize,
            placeholder,
            target_size,
            measure_state,
            radius_from_auto,
            radius_to_auto,
        ) = match snapshot {
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
                    // Tier 1 (cross-composer) ends carry no scene id yet: the scene host API is
                    // published per composer, so a flight spanning main and an overlay has no shared
                    // scene visibility to read here. Keep the flight's own crossfade.
                    scene_alpha: None,
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
                    remeasure: false,
                    scale_parts: None,
                    radius_from_auto,
                    radius_to_auto,
                });
            }
        }
        if let Some(slot) = tslot {
            // Ownership key BEFORE the mutable borrow: the flight lives in the
            // MAIN map, so its composer id is what namespaces the override.
            let owner_cid = all[0].composer_id;
            let peer = &mut all[peer_idx];
            if let Some(root) = peer.arena.root {
                if let Some(tidx) = find_idx_by_slot(&peer.arena.nodes, root, slot) {
                    // Layout contract for the peer end, identical to Tier 0.
                    let remeasure = matches!(resize, ResizeMode::RemeasureToBounds);
                    let frame = placeholder_frame(
                        remeasure,
                        animated_size(&start, &end, pr, path),
                        placeholder,
                        target_size,
                    );
                    measure_state.set(frame);
                    let key = peer.arena.nodes[tidx].slot_key;
                    let mine = FlightKey { cid: owner_cid, id };
                    // Same guard as the Tier-0 writer: a different flight's
                    // override on this node must survive (ids are per-composer).
                    let foreign = peer.arena.nodes[tidx]
                        .flight_measure
                        .as_ref()
                        .is_some_and(|f| f.owner != mine);
                    let had_override = !foreign && peer.arena.nodes[tidx].flight_measure.is_some();
                    if !foreign {
                        peer.arena.nodes[tidx].flight_measure = if frame.is_idle() {
                            None
                        } else {
                            Some(FlightMeasure { frame: measure_state, owner: mine })
                        };
                    }
                    if !frame.is_idle() || had_override {
                        peer.layout_dirty_keys.insert(key);
                    }
                    // Same first-attach signal as the Tier-0 writer, but on the PEER: the
                    // peer's own parent is the one that saw the natural size this frame, so
                    // the peer composer is the one that needs the extra pass.
                    if !frame.is_idle() && !had_override {
                        peer.layout_override_fresh = true;
                    }
                    peer.arena.nodes[tidx].transition = Some(TransitionVisual {
                        start: off_origin(start, to),
                        end: off_origin(end, to),
                        progress: pr,
                        // See the Tier 1 source comment above: no scene id across composers yet.
                        scene_alpha: None,
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
                        remeasure,
                        scale_parts: resize.scale_to_bounds_parts(),
                        radius_from_auto,
                        radius_to_auto,
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
                /// Peer target's layout contract (Compose `ResizeMode` /
                /// `PlaceHolderSize`) and its natural size.
                resize: ResizeMode,
                placeholder: PlaceHolderSize,
                target_size: crate::unit::Size,
                /// Peer target's corner kind (Compose percent vs fixed).
                radius_to_auto: bool,
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
                let (
                    spec,
                    clip,
                    path,
                    enter,
                    target_is_bounds,
                    target_in_overlay,
                    resize,
                    placeholder,
                ) = match marker {
                    Some(m) => (
                        m.transform.spec.clone(),
                        shared_clip_for_kind(&m.kind),
                        m.path,
                        m.enter,
                        matches!(m.kind, SharedKind::Bounds { .. }),
                        m.render_in_overlay,
                        shared_resize_for_kind(&m.kind),
                        shared_placeholder_for_kind(&m.kind),
                    ),
                    None => (
                        BoundsTransform::default().spec,
                        false,
                        PathMotion::Linear,
                        None,
                        false,
                        true,
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
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
                    resize: resize.clone(),
                    placeholder,
                    target_size: peer.arena.nodes[tidx].measured_size,
                    radius_to_auto: shared_shape_radius_is_auto(&peer.arena.nodes[tidx].modifier),
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
                    // Source kind was captured at stash time; the target's
                    // comes from the peer marker (see `Hit`).
                    radius_from_auto: p.radius_from_auto,
                    radius_to_auto: h.radius_to_auto,
                    clip: h.clip,
                    path: h.path,
                    bounds_fx: h.bounds_fx,
                    target_in_overlay: h.target_in_overlay,
                    resize: h.resize,
                    placeholder: h.placeholder,
                    // Resolved at match time: the peer node was just laid out.
                    target_size: Some(h.target_size),
                    measure: State::new(FlightMeasureFrame::IDLE),
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
                let mut visited = crate::layout::node::NodeMarks::default();
                owner.arena.free_node_skip(p.src_idx, &crate::layout::node::NodeMarks::default(), &mut visited);
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
                // A morph has ONE node, so both ends share its corner kind.
                radius_from_auto: shared_shape_radius_is_auto(&modifier),
                radius_to_auto: shared_shape_radius_is_auto(&modifier),
                clip,
                path,
                bounds_fx,
                target_in_overlay: true,
                resize: ResizeMode::scale_to_bounds(),
                placeholder: PlaceHolderSize::JumpCut,
                target_size: None,
                measure: State::new(FlightMeasureFrame::IDLE),
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
            // Baselines key on endpoint identity (scope, key), not the slot: a
            // conditional key-swap reusing one call-site slot must not inherit the
            // previous key's rect as its morph start.
            let bkey = (k.0, k.1.clone());
            // A flight OWNS this node's reported size right now (Compose
            // `PlaceHolderSize`), so a size delta here is the FLIGHT, not a layout
            // morph. Opening one would be wrong twice over: a Tier-1 flight lives in
            // the MAIN composer's map, so this composer's `flight_for_key` cannot see
            // it and would open a phantom T0 morph EVERY frame — and that morph's
            // idle frame (morphs never attach an override) deletes the flight's
            // override, which the cross-poll then rewrites, i.e. per-frame
            // drop/rewrite churn plus a phantom flight.
            //
            // The BASELINE must still track the override-driven size, though: the
            // baseline is the pre-flight rect, so freezing it here made the flight's
            // own layout change look like a fresh morph the moment the override was
            // dropped — the landed hero then replayed a whole animation. So this
            // skips the morph DECISION only; the insert below still runs.
            let overridden = self.arena.nodes[idx].flight_measure.is_some();
            if !overridden {
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
                        None => {
                            // Compose ties a shared element's morph to a TRANSITION: the element
                            // animates when the transition animating it changes its bounds, not merely
                            // because a layout pass changed them. Without this gate a window resize
                            // opened a morph on every marked node whose rect changed (measured:
                            // `morph_open flight:entry:… fresh 420x524 -> 430x604` right after a
                            // resize), i.e. dragging the window animated the UI.
                            //
                            // The gate skips only the DECISION — the baseline insert below still runs, as
                            // the older `overridden` guard does. `continue`-ing here left the baseline
                            // stale, so an idle resize was replayed as a morph on the first transitioning
                            // frame instead: the same artefact, one navigation later.
                            if self.scope_is_transitioning(k.0) {
                                pending.push(Pending::Fresh {
                                    scope_id: k.0,
                                    key: k.1.clone(),
                                    slot: *slot,
                                    start: *prev,
                                    end: cur,
                                });
                            }
                        }
                    }
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
                    // anim-trace: morph opens are otherwise invisible in a trace (no `start` event, only
                    // per-frame records), which made "what did the resize animate?" hard to answer.
                    crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
                        format!("flight:{key}"),
                        "morph_open",
                        format!(
                            "fresh {:.0}x{:.0} -> {:.0}x{:.0} at ({:.0},{:.0})",
                            start.width, start.height, end.width, end.height, end.x, end.y
                        ),
                    ));
                    self.begin_morph(scope_id, key, slot, start, end, None);
                }
                Pending::Reopen { id, start, radii, end } => {
                    let meta = self
                        .shared_flights
                        .get(&id)
                        .map(|a| (a.flight.scope_id, a.flight.key.clone(), a.flight.target_slot));
                    self.cancel_flight(id, "morph_reopen");
                    if let Some((scope_id, key, Some(slot))) = meta.map(|(s, k, t)| (s, k, t)) {
                        crate::anim_trace::record(crate::anim_trace::TraceRecord::event(
                            format!("flight:{key}"),
                            "morph_open",
                            format!(
                                "reopen {:.0}x{:.0} -> {:.0}x{:.0}",
                                start.width, start.height, end.width, end.height
                            ),
                        ));
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
#[cfg(test)]
mod tier0_tests;
