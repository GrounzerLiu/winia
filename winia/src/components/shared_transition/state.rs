use super::*;

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
    /// `push_animatable`). Flights animate `FlightEdges` (four components) on a spring and keep this
    /// rect type for the renderer and the tests.
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
    /// Compose's default flight shaping: `SharedTransitionDefaults.BoundsTransform` is
    /// `spring(stiffness = StiffnessMediumLow, visibilityThreshold = Rect.VisibilityThreshold)`
    /// (`animation/SharedTransitionScope.kt:1534`, `:1608`; `StiffnessMediumLow` is 400,
    /// `Rect.VisibilityThreshold` is one pixel per edge, `VisibilityThresholds.kt:39-42`).
    ///
    /// The threshold is one PIXEL because the spec now animates the flight's four edges, which are
    /// window pixels — the same units Compose's `Rect.VectorConverter` hands its animation. (It used to
    /// be 0.01 in progress units, which was the same stop band only for a hundred-pixel flight: a
    /// thousand-pixel one used to stop ten pixels short and then jump.)
    fn default() -> Self {
        Self {
            spec: SpringSpec {
                damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
                stiffness: SpringSpec::STIFFNESS_MEDIUM,
                mass: 1.0,
                threshold: Self::VISIBILITY_THRESHOLD_PX,
            }
            .into(),
        }
    }
}

/// Default values for the shared-transition API (Compose
/// `SharedTransitionDefaults`). User-facing entry point for "no opinion"
/// call sites — internal code keeps using the concrete defaults directly
/// so behavior never depends on this object drifting.
pub struct SharedTransitionDefaults;

impl SharedTransitionDefaults {
    /// Default flight shaping: Compose's spring, the same object
    /// [`BoundsTransform::default`] hands out — `spring(1.0, 400)` with a one-pixel-equivalent
    /// progress threshold, not a 300 ms tween.
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


// ═══════════════════════════════════════════════════════════
// Matching layer: scope, content state, registry
// ═══════════════════════════════════════════════════════════

/// Pairing handle: which element flies with which (Compose
/// `SharedContentState`). Equality is (scope, key): same key in different
/// scopes never matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SharedContentState {
    pub(crate) scope_id: u64,
    pub(crate) key: String,
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
    pub(crate) scope_id: u64,
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

pub(crate) static LOCAL_SHARED_SCOPE: LazyLock<CompositionLocal<Option<SharedTransitionScope>>> =
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
    crate::debug::animation::note_scene(scene);
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
pub(crate) static SCOPE_ACTIVE: LazyLock<Mutex<HashMap<u64, State<bool>>>> =
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
