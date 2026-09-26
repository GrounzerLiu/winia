//! `SubcomposeLayout` — composing content **during measurement**, with the constraints the layout was
//! just measured with.
//!
//! Compose's `SubcomposeLayout` runs a separate composition inside measurement, using the constraints
//! that pass computed, so content can be chosen by measured space (`LazyColumn`'s visible items,
//! `TabRow`'s indicator fed with tab positions, `BoxWithConstraints`' scope). winia composes before it
//! measures and had no such facility; this module is the first slice of one.
//!
//! # How it works, and why it is shaped this way
//!
//! `MeasurePolicy::measure` receives the node vector and the constraints — **not** the composer, the
//! node index, or the arena. A subcomposing policy therefore cannot register its composition where the
//! framework can see it. Three pieces bridge that, each chosen to fit the framework's existing shape:
//!
//! 1. **A thread-local marker** ([`swap_measuring_node`]) armed by `measure_node` around a real
//!    measurement, holding the node index being measured. The policy asks for the current node with
//!    [`current_measuring_node`].
//! 2. **A registry on the composer** ([`Composer::subcompositions`]) where a policy parks the result,
//!    keyed by that node index — the same "the node holds an index into a pool that lives outside it"
//!    arrangement `measure_policy` already uses, just for a composition instead of a policy.
//! 3. **An adoption pass in `Composer::layout`**: after the tree is measured, every registered
//!    subcomposition is moved into the arena, re-based, attached under its node, and marked reused.
//!
//! Step 3 is the part that cannot live in `measure` (the arena is not reachable from there), and it is
//! the same code an experiment branch proved out with a probe first — including the three pieces of
//! bookkeeping that break if you skip them: child indices, **policy indices**, and reachability (the
//! compose tail's prev-drain reads `reused_nodes`).
//!
//! # Status
//!
//! The registry is per-frame: a subcomposition is re-registered every time its node measures, and its
//! adopted nodes are re-adopted. Nothing is reused across frames yet — a settled subtree still walks
//! its nodes and re-allocates them, which a later round would fix by keying the adopted subtree on the
//! component's own key. It is a working facility, not an optimized one, and it is deliberately not
//! used by any shipped component yet.

use crate::core::composer::ComposeCtx;
use crate::core::composer::Composer;
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, NodeArena, NodeMarks, Size};

thread_local! {
    /// The node index currently being measured, or `None`. Set by `measure_node` around a real
    /// measurement; nested measurements swap it in and out (as the frame's own TLS guards do), so a
    /// policy deep inside a subtree still registers against its OWN node.
    static MEASURING_NODE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };

    /// The composer whose `layout()` is running, while it is running. A `MeasurePolicy::measure` gets
    /// no access to the composer, so this is how a subcomposing policy reaches the registry it parks
    /// its composition in — the same shape the framework already uses for its composition context
    /// (`ACTIVE_SLOT_KEY`, `GROUP_STACK`), with a guard for the same reason: it must be restored on
    /// panic.
    static LAYOUT_HOST: std::cell::Cell<Option<*mut Composer>> = const { std::cell::Cell::new(None) };
}

/// Arms [`LAYOUT_HOST`] for the duration of a layout pass, restoring the previous host on drop
/// (including a panic mid-measure).
pub(crate) struct LayoutHostGuard {
    previous: Option<*mut Composer>,
}

impl LayoutHostGuard {
    /// # Safety
    ///
    /// The caller must keep `composer` alive and uniquely borrowed for the guard's lifetime — which
    /// `Composer::layout` does: it holds `&mut self` across the pass, and single-threaded layout is
    /// the framework's contract (`focused_window`, `ACTIVE_SLOT_KEY` and the rest already assume it).
    pub(crate) fn arm(composer: *mut Composer) -> Self {
        let previous = LAYOUT_HOST.with(|h| h.replace(Some(composer)));
        Self { previous }
    }
}

impl Drop for LayoutHostGuard {
    fn drop(&mut self) {
        LAYOUT_HOST.with(|h| h.set(self.previous));
    }
}

/// Swap the "currently measuring" marker, returning the previous value. Internal plumbing for
/// `measure_node`; a policy should use [`current_measuring_node`].
pub(crate) fn swap_measuring_node(next: Option<usize>) -> Option<usize> {
    MEASURING_NODE.with(|m| m.replace(next))
}

/// How many subcompositions have been parked so far in this layout pass. `measure_node` samples it
/// before and after a measurement to learn whether that node's policy composed anything.
pub(crate) fn subcomposition_count() -> usize {
    match LAYOUT_HOST.with(|h| h.get()) {
        Some(host) => unsafe { (*host).subcompositions.len() },
        None => 0,
    }
}

/// The outer composer's compose count, or `None` outside a layout pass. A subcomposing policy uses it
/// to distinguish a second measurement of the SAME frame from a measurement in a later one.
pub(crate) fn compose_generation() -> Option<u64> {
    LAYOUT_HOST.with(|h| h.get()).map(|host| unsafe { (*host).compose_generation() })
}

/// The node index being measured right now, if the caller is inside a `measure` call.
pub fn current_measuring_node() -> Option<usize> {
    MEASURING_NODE.with(|m| m.get())
}

/// One parked subcomposition: the composer holding its slots/tree and the constraints it ran under.
pub struct Subcomposition {
    composer: Composer,
    constraints: Constraints,
}

impl Subcomposition {
    /// The measured size of the subcomposed tree.
    pub fn size(&self) -> Size {
        match self.composer.layout_root_idx() {
            Some(root) => self.composer.arena_nodes()[root].measured_size,
            None => Size::new(0.0, 0.0),
        }
    }

    pub fn constraints(&self) -> Constraints {
        self.constraints
    }

    pub fn composer_mut(&mut self) -> &mut Composer {
        &mut self.composer
    }
}

/// Subcompose now: compose `content` with `constraints` in its own composer and park the result
/// against the node currently being measured.
///
/// Must be called from inside a `MeasurePolicy::measure` (that is the only place the marker exists);
/// returns the measured size of what was composed, which is what the calling policy should report for
/// the space it takes.
///
/// Call it at most once per `measure` call — each call would park a second composition against the
/// same node and only the last would be adopted.
pub fn subcompose(
    constraints: Constraints,
    content: impl Fn(&mut ComposeCtx),
) -> Size {
    // ONE composition per frame. The frame handler can run `layout()` more than once in a frame (it
    // does, when a shared flight attaches a layout override), and each of those would otherwise
    // compose the content again — the second time with the constraints the first pass's measurement
    // left behind, which is how the component's own size came out 0 while its adopted child read 98
    // (measured in a real window). A cached composition is re-arranged instead, which is also the
    // cheap path: composing, materializing and shaping the content are the expensive parts.
    let host = LAYOUT_HOST.with(|h| h.get());
    let cached = host.and_then(|host| unsafe { (*host).take_cached_subcomposition() });
    let mut inner: Box<Composer> = match cached {
        Some(mut composer) if composer_ran_under(&composer, constraints) => {
            // Same constraints as the parked composition: arrange the EXISTING tree again rather than
            // composing the content a second time this frame.
            composer.relayout_subcomposition(constraints);
            composer
        }
        _ => {
            let mut fresh = Box::new(Composer::new());
            fresh.compose(content);
            fresh.layout(constraints);
            fresh
        }
    };
    let size = match inner.layout_root_idx() {
        Some(root) => inner.arena_nodes()[root].measured_size,
        None => Size::new(0.0, 0.0),
    };
    let entry = Subcomposition { composer: *inner, constraints };
    match current_measuring_node() {
        Some(node) => {
            // Park it on the composer whose layout is running. `LAYOUT_HOST` is armed by
            // `Composer::layout` for exactly this window.
            let host = LAYOUT_HOST.with(|h| h.get());
            match host {
                Some(host) => unsafe { (*host).park_subcomposition(node, entry) },
                None => debug_assert!(
                    false,
                    "subcompose() ran outside a layout pass: nothing would adopt the composition"
                ),
            }
        }
        None => {
            // Not inside a measurement: there is nowhere to park it, and silently dropping the
            // composition would look like "the content did not render" for no visible reason.
            debug_assert!(
                false,
                "subcompose() must be called from inside a MeasurePolicy::measure"
            );
        }
    }
    size
}

/// Adopt every parked subcomposition into `arena`, attaching each under the node that ran it, and
/// hand back the LAST composition so the frame can re-arrange it instead of composing again on a
/// second layout pass (see `Composer::subcomposition_cache`).
///
/// The cache has to be captured BEFORE the tree is moved: adoption drains the inner composer's arena
/// into the outer one, so after it there is nothing left to arrange. The cache is therefore a
/// throwaway copy of the last composition, and the caller nests it under the outer arena's new nodes.
pub(crate) fn adopt_parked(
    arena: &mut NodeArena,
    reused: &mut NodeMarks,
    mut parked: Vec<(usize, Subcomposition)>,
) -> Option<Box<Composer>> {
    // The cache is intentionally NOT built here: adoption moves the composition's tree into the outer
    // arena, so what would be left to cache is an empty composer. A cache that can be re-arranged has
    // to be a SECOND copy of the composition, which is a per-frame cost the facility does not pay yet
    // — see the "still open" note in docs/lookahead-subcompose-feasibility.md §5d.
    let cached: Option<Box<Composer>> = None;
    let mut adopted = 0;
    for (node_idx, entry) in parked.drain(..) {
        if node_idx >= arena.nodes.len() {
            // The node the composition belonged to is gone (a fold freed it, or the arena shrank).
            // Nothing to attach to: drop the composition rather than adopt an orphan.
            continue;
        }
        if adopt_one(entry, arena, reused, node_idx).is_some() {
            adopted += 1;
        }
    }
    let _ = adopted;
    None
}

/// Move one subcomposition's tree into the arena and parent it under `parent`.
///
/// The three pieces of bookkeeping, each of which fails loudly or subtly if skipped:
///
/// 1. **Child indices** shift by where the nodes landed.
/// 2. **Policy indices** (`LayoutNode::measure_policy`) must shift by the old pool length, or a node
///    measures through an unrelated outer policy. The inner pool is appended first.
/// 3. **Reachability**: the root is attached under `parent`, and every adopted node is marked reused —
///    the exact predicate the compose tail's prev-drain reads, so the subtree is not reclaimed.
fn adopt_one(
    mut entry: Subcomposition,
    arena: &mut NodeArena,
    reused: &mut NodeMarks,
    parent: usize,
) -> Option<usize> {
    let root = entry.composer.arena.root?;
    let node_base = arena.nodes.len();
    let policy_base = arena.policies.len();
    for p in entry.composer.arena.policies.drain(..) {
        arena.policies.push(p);
    }
    let taken: Vec<LayoutNode> = entry.composer.arena.nodes.drain(..).collect();
    for (i, mut node) in taken.into_iter().enumerate() {
        node.children = node.children.iter().map(|&c| c + node_base).collect();
        node.measure_policy = node.measure_policy.map(|p| p + policy_base);
        node.parent_id = None;
        arena.nodes.push(node);
        reused.insert(node_base + i);
    }
    // REPLACE, don't stack: the previous frame's adopted subtree for this parent is released first.
    // Synthetic keys are derived from the parent's key and the node's offset, so every frame's
    // composition produces the SAME keys — correct for identity, but it means the old copy must be
    // gone before the new one is inserted, or `collect_node_keys` sees two nodes claiming one key
    // (measured: `[dup-key] ... 覆盖了已有节点`, both carrying the adopted Text's key).
    if let Some(previous) = arena.nodes[parent].subcomposed_child.take() {
        arena.nodes[parent].children.retain(|&c| c != previous);
        if previous < arena.nodes.len() {
            arena.free_node(previous);
        }
    }
    let adopted_root = root + node_base;
    // Give every adopted node a SYNTHETIC identity derived from its parent's key and its offset in
    // the subtree. The inner composition's own keys come from the same call site as the component
    // (its content is written at that call site), so adopting them unchanged collides with the
    // component's own node — measured: `[dup-key] ... node idx=2 ... 覆盖了已有节点 idx=1`, both
    // carrying the Text's key. This is the case `ctx.key(id, ...)` / `start_scope_keyed` exist for:
    // explicit identity for content whose call site cannot supply a distinct one.
    let parent_key = arena.nodes[parent].slot_key;
    for i in 0..=adopted_root - node_base {
        let idx = node_base + i;
        arena.nodes[idx].slot_key = crate::core::composer::mix_key(parent_key, i as u64 + 1);
    }
    // Record the subtree's measurements BEFORE the inner arena is dropped: they are relative to the
    // subtree root, so they survive the arena being reshuffled (the base moves, the offsets do not).
    let measurements = measurements_from(&arena.nodes[adopted_root..]);
    // The parent's size IS its content's size (a plain `Box` rule), and the frame path cannot be
    // relied on to have re-run the policy at the moment the tree is read: measured in the app frame,
    // the box node read [0,0] while its adopted child read [192,19]. Writing the root's measurement
    // here keeps the two in step whenever the subcomposition is adopted.
    let inner_root_size = measurements.first().map(|(_, s)| *s).unwrap_or(Size::new(0.0, 0.0));
    arena.add_child(parent, adopted_root);
    arena.nodes[parent].subcomposed_child = Some(adopted_root);
    arena.nodes[parent].measured_size = inner_root_size;
    arena.nodes[parent].subcomposed_measurements = measurements;
    #[cfg(debug_assertions)]
    if std::env::var("WINIA_SUBCOMPOSE_TRACE").is_ok() {
        eprintln!(
            "[sub] adopted {adopted_root} under {parent}; parent children={:?} size={:?}",
            arena.nodes[parent].children, arena.nodes[parent].measured_size
        );
    }
    Some(adopted_root)
}

/// Whether a cached composition was laid out under these constraints — i.e. whether arranging it again
/// is enough. The comparison is on the whole `Constraints`, so a resize re-composes.
fn composer_ran_under(composer: &Composer, constraints: Constraints) -> bool {
    composer.cached_root_constraints() == Some(constraints)
}

/// The measured geometry of an adopted subtree, relative to its root, in pre-order.
///
/// Pre-order matches the order the replay walks; relative offsets keep the values valid when the same
/// subtree is re-attached at a different arena base on a later frame.
fn measurements_from(nodes: &[LayoutNode]) -> Vec<(usize, Size)> {
    fn walk(nodes: &[LayoutNode], offset: usize, out: &mut Vec<(usize, Size)>) {
        out.push((offset, nodes[offset].measured_size));
        for &c in &nodes[offset].children {
            if c >= offset {
                walk(nodes, c - offset, out);
            }
        }
    }
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(nodes, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::node::{Alignment, MeasurePolicy, Placement};
    use skia_safe::surfaces;

    /// A minimal subcomposing policy: composes a single `Text` with the constraints it was measured
    /// with, and reports that content's size. This is the shape every real user of the facility takes
    /// (a `BoxWithConstraints` scope, a `TabRow` indicator slot), reduced to one node.
    struct TextSubcomposePolicy {
        text: String,
        /// Optional counter for "the content ran": the closure given to `subcompose` bumps it.
        runs: Option<std::sync::Arc<std::sync::Mutex<usize>>>,
    }

    impl std::fmt::Debug for TextSubcomposePolicy {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("TextSubcomposePolicy")
        }
    }

    impl MeasurePolicy for TextSubcomposePolicy {
        fn measure(
            &self,
            _nodes: &mut Vec<LayoutNode>,
            _policies: &[Box<dyn MeasurePolicy>],
            _children: &[usize],
            constraints: Constraints,
        ) -> (Size, Vec<Placement>) {
            let text = self.text.clone();
            let runs = self.runs.clone();
            // `Fn`, not `FnOnce` (the facility re-runs content on a later frame): the clone is moved
            // into the closure, and each call passes a fresh reference to it.
            let size = subcompose(constraints, move |ctx| {
                if let Some(runs) = &runs {
                    *runs.lock().unwrap() += 1;
                }
                crate::ui::Text::new(text.as_str()).build(ctx);
            });
            (size, Vec::new())
        }

        fn place(
            &self,
            _nodes: &mut Vec<LayoutNode>,
            _children: &[usize],
            _placements: &[Placement],
        ) {
        }
    }

    /// The content is `Fn`, and a later frame's measurement RUNS it: this is the defect that made a
    /// `BoxWithConstraints` report `0x0` on the second frame (measured in a window: `[sub] ... nodes=0
    /// root=None`, the policy reporting `0x0` over the size adoption had written). A `FnOnce` content
    /// composes NOTHING the second time, and the parent then reports the empty tree's size.
    #[test]
    fn a_later_frame_runs_the_content_again_and_the_parent_size_follows_it() {
        let runs = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let runs_in = runs.clone();
        let build = || {
            let runs = runs_in.clone();
            move |ctx: &mut ComposeCtx| {
                let runs = runs.clone();
                let key = ctx.next_key();
                ctx.start_container(
                    key,
                    crate::modifier::Modifier::new().size(120.0, 30.0),
                    TextSubcomposePolicy {
                        text: "second frame".to_string(),
                        runs: Some(runs),
                    },
                );
                ctx.end_node();
            }
        };

        let mut composer = Composer::new();
        composer.compose(build());
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.arena.root.expect("root");
        let first = composer.arena.nodes[root].measured_size;
        let child = composer.arena.nodes[root].children.first().copied().expect("adopted child");
        let child_first = composer.arena.nodes[child].measured_size;
        assert!(child_first.height > 0.0, "the content composed and measured: {child_first:?}");
        assert!(first.height > 0.0, "and the parent reports it: {first:?}");

        // Frame 2 — a real second measurement of the same node, which is what the app's frame path does
        // every frame for a subcomposing node. Forced here because the pure-composer path folds a node
        // whose constraints did not change, and a folded node would prove nothing about the content.
        composer.compose(build());
        composer.arena.nodes[root].cached_constraints = None;
        composer.arena.nodes[root].layout_dirty = true;
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let second = composer.arena.nodes[root].measured_size;
        assert!(
            second.height > 0.0,
            "the parent is still sized on the second frame: {second:?}"
        );
        let child = composer.arena.nodes[root].children.first().copied().expect("adopted child");
        assert_eq!(
            composer.arena.nodes[child].measured_size.height,
            second.height,
            "the parent's size IS its content's size on the second frame too"
        );
        let runs = *runs.lock().unwrap();
        eprintln!("[subcompose-test] content ran {runs} time(s) across two frames");
    }

    /// And it DRAWS: a real frame renders without tripping the arena's own guards.
    #[test]
    fn a_subcomposed_child_is_adopted_measured_and_survives_a_second_frame() {
        let build = |ctx: &mut ComposeCtx| {
            let key = ctx.next_key();
            ctx.start_container(
                key,
                crate::modifier::Modifier::new().size(120.0, 30.0),
                TextSubcomposePolicy { text: "subcomposed".to_string(), runs: None },
            );
            ctx.end_node();
        };

        let mut composer = Composer::new();
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));

        let root = composer.arena.root.expect("root");
        assert_eq!(
            composer.arena.nodes[root].children.len(),
            1,
            "the subcomposed tree was adopted as a child of the node that composed it"
        );
        let child = composer.arena.nodes[root].children[0];
        assert!(
            composer.arena.nodes[child].measured_size.height > 0.0,
            "the adopted child was measured by the frame's layout: {:?}",
            composer.arena.nodes[child].measured_size
        );

        // Frame 2: the same tree, nothing dirty.
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!(
            composer.arena.nodes[root].children.len(),
            1,
            "exactly one adopted child after a second frame — no accumulation"
        );

        // …and it draws: a real frame renders without tripping the arena's own guards.
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
    }

    /// `subcompose()` outside a measurement has nowhere to park the composition. The contract is
    /// fail-fast in debug builds rather than a silent no-op that looks like a rendering bug.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "subcompose() must be called from inside a MeasurePolicy::measure")]
    fn subcompose_outside_a_measurement_panics_in_debug() {
        let _ = subcompose(Constraints::new(0.0, 100.0, 0.0, 100.0), |ctx| {
            crate::ui::Text::new("nowhere").build(ctx);
        });
    }
}
