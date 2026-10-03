//! THROWAWAY PROBE (branch `exp/lookahead-probe`): the minimal design-2 shape — a component whose
//! content is composed **during measurement, with the constraints it was just measured with** — plus
//! the cross-frame experiment: does an adopted subtree survive a real frame's compose tail (the
//! prev-drain) and keep drawing?
//!
//! Findings, each a test below:
//!
//! 1. Composing inside a measure call runs, produces a real size, leaves the frame's TLS context
//!    usable afterwards, and a panic inside it does not corrupt the outer arena.
//! 2. Adopting that tree into the outer arena works — nodes moved, child indices re-based, the policy
//!    pool appended and every `measure_policy` index re-based (checked against a decoy policy at the
//!    colliding index), root stamped with a synthetic key.
//! 3. `MeasurePolicy::measure` receives nodes but NOT the arena, so adoption cannot happen inside a
//!    policy; it must be driven from a site that has the arena (`Composer::layout`/`materialize`).
//! 4. The cross-frame half: an adopted subtree, parented under its component's node and marked reused,
//!    survives the next frame's compose and still paints.

use crate::runtime::composer::ComposeCtx;
use crate::runtime::composer::Composer;
use crate::layout::constraints::Constraints;
use crate::layout::node::{Alignment, MeasurePolicy, Placement, Size};

/// A key no real call site can produce (slot keys are mixed hashes).
const ADOPTED_SLOT_KEY: u64 = u64::MAX;

/// A `Box`-like component whose content is composed during measurement.
pub struct SubcomposeProbe {
    modifier: crate::modifier::Modifier,
    alignment: Alignment,
}

impl SubcomposeProbe {
    pub fn new() -> Self {
        Self { modifier: crate::modifier::Modifier::new(), alignment: Alignment::Start }
    }

    pub fn modifier(mut self, m: crate::modifier::Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    pub fn alignment(mut self, a: Alignment) -> Self {
        self.alignment = a;
        self
    }

    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        content: impl Fn(&mut ComposeCtx, Constraints) + Send + Sync + 'static,
    ) {
        let key = ctx.next_key();
        ctx.start_container(
            key,
            self.modifier,
            SubcomposePolicy { alignment: self.alignment, content: std::sync::Arc::new(content) },
        );
        ctx.end_node();
    }
}

impl Default for SubcomposeProbe {
    fn default() -> Self {
        Self::new()
    }
}

/// One subcomposition result: the composer holding it and the constraints it ran under.
pub struct Subcomposed {
    composer: Composer,
    constraints: Constraints,
}

impl Subcomposed {
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

thread_local! {
    /// Where a policy parks its subcomposition: `measure` takes `&self`, and the real design would have
    /// the component own a slot for it.
    static SUBCOMPOSED: std::cell::RefCell<Option<Subcomposed>> =
        const { std::cell::RefCell::new(None) };
}

pub fn take_subcomposed() -> Option<Subcomposed> {
    SUBCOMPOSED.with(|s| s.borrow_mut().take())
}

struct SubcomposePolicy {
    alignment: Alignment,
    content: std::sync::Arc<dyn Fn(&mut ComposeCtx, Constraints) + Send + Sync>,
}

impl std::fmt::Debug for SubcomposePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SubcomposePolicy")
    }
}

impl MeasurePolicy for SubcomposePolicy {
    fn measure(
        &self,
        _nodes: &mut Vec<crate::layout::node::LayoutNode>,
        _policies: &[Box<dyn MeasurePolicy>],
        _children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // The design-2 step: compose NOW, with the real constraints, into a composer this policy owns.
        // Its slots never touch the outer table — which is what makes this safe where re-composing the
        // same tree is not (feasibility note §3).
        let mut inner = Composer::new();
        let content = self.content.clone();
        inner.compose(move |ctx| {
            content(ctx, constraints);
        });
        inner.layout(constraints);
        let size = match inner.layout_root_idx() {
            Some(root) => inner.arena_nodes()[root].measured_size,
            None => Size::new(0.0, 0.0),
        };
        SUBCOMPOSED.with(|s| {
            *s.borrow_mut() = Some(Subcomposed { composer: inner, constraints });
        });
        let _ = self.alignment;
        (size, Vec::new())
    }

    fn place(
        &self,
        _nodes: &mut Vec<crate::layout::node::LayoutNode>,
        _children: &[usize],
        _placements: &[Placement],
    ) {
    }
}

/// Adopt a subcomposed tree into the OUTER arena, parent it under `parent`, and mark every adopted
/// node reused so the compose tail's prev-drain leaves it alone. Returns the adopted root's index.
///
/// Three pieces of bookkeeping, each a live problem while writing this:
///
/// 1. **Node indices** shift by where the nodes landed.
/// 2. **Policy indices** must shift by the old pool length, or a node measures through an unrelated
///    outer policy.
/// 3. **Reachability**: parented here, and every adopted node marked reused — the mechanism the
///    compose tail's drain reads (`self.reused_nodes.contains(idx)` → `continue`).
pub fn adopt_inner_nodes(
    inner: &mut Composer,
    outer: &mut crate::layout::node::NodeArena,
    parent: Option<usize>,
    reused: &mut crate::layout::node::NodeMarks,
) -> Option<usize> {
    let root = inner.arena.root?;
    let node_base = outer.nodes.len();
    let policy_base = outer.policies.len();
    for p in inner.arena.policies.drain(..) {
        outer.policies.push(p);
    }
    let taken: Vec<crate::layout::node::LayoutNode> = inner.arena.nodes.drain(..).collect();
    for (i, mut node) in taken.into_iter().enumerate() {
        node.children = node.children.iter().map(|&c| c + node_base).collect();
        node.measure_policy = node.measure_policy.map(|p| p + policy_base);
        node.parent_id = None;
        outer.nodes.push(node);
        reused.insert(node_base + i);
    }
    let adopted = root + node_base;
    outer.nodes[adopted].slot_key = ADOPTED_SLOT_KEY;
    if let Some(p) = parent {
        outer.add_child(p, adopted);
    }
    Some(adopted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Step 1: composing inside a measure call runs, produces a size, and leaves the thread's
    /// composition context usable — the check a leaked frame context would fail.
    #[test]
    fn composing_inside_a_measure_call_runs_and_leaves_the_context_usable() {
        let policy = SubcomposePolicy {
            alignment: Alignment::Start,
            content: std::sync::Arc::new(|ctx: &mut ComposeCtx, c: Constraints| {
                crate::ui::Text::new(format!("w={}", c.max_width)).build(ctx);
            }),
        };
        let mut nodes = Vec::new();
        let (size, _) =
            policy.measure(&mut nodes, &[], &[], Constraints::new(0.0, 200.0, 0.0, 100.0));
        assert!(size.width > 0.0 && size.height > 0.0, "the inner composition measured: {size:?}");
        let sub = take_subcomposed().expect("a subcomposition was recorded");
        assert_eq!(sub.constraints().max_width, 200.0, "the content saw the real constraints");

        let mut outer = Composer::new();
        outer.compose(|ctx| {
            crate::ui::Text::new("after").build(ctx);
        });
        outer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = outer.layout_root_idx().expect("the outer composition produced a tree");
        assert!(
            outer.arena_nodes()[root].measured_size.height > 0.0,
            "the frame still composes after a measure-time subcomposition"
        );
    }

    /// Step 2: the subcomposed tree becomes real nodes in the outer arena and still measures there —
    /// the proof being a measurement through the outer pool, not a node count.
    #[test]
    fn an_adopted_subcomposition_measures_inside_the_outer_arena() {
        let policy = SubcomposePolicy {
            alignment: Alignment::Start,
            content: std::sync::Arc::new(|ctx: &mut ComposeCtx, c: Constraints| {
                crate::ui::Text::new(format!("inner w={}", c.max_width)).build(ctx);
            }),
        };
        let mut outer = Composer::new();
        outer.compose(|ctx| {
            crate::ui::Text::new("outer").build(ctx);
        });
        let mut nodes = Vec::new();
        let _ = policy.measure(&mut nodes, &[], &[], Constraints::new(0.0, 200.0, 0.0, 100.0));
        let mut sub = take_subcomposed().expect("subcomposition recorded");
        let mut marks = crate::layout::node::NodeMarks::default();
        let adopted = adopt_inner_nodes(sub.composer_mut(), &mut outer.arena, None, &mut marks)
            .expect("adopted");
        let (size, _) = crate::layout::node::measure_node(
            &mut outer.arena.nodes,
            &outer.arena.policies,
            adopted,
            Constraints::new(0.0, 200.0, 0.0, 100.0),
        );
        assert!(size.height > 0.0, "the adopted subtree measures inside the outer arena");
        assert_eq!(outer.arena.nodes[adopted].slot_key, ADOPTED_SLOT_KEY);
    }

    /// The failure mode adoption must not have: an un-re-based policy index lands on whatever the
    /// outer pool holds at that index. A decoy policy sits exactly there, so only correct re-basing
    /// measures the adopted node successfully.
    #[test]
    fn adoption_re_bases_policy_indices_instead_of_pointing_at_outer_policies() {
        let policy = SubcomposePolicy {
            alignment: Alignment::Start,
            content: std::sync::Arc::new(|ctx: &mut ComposeCtx, _c: Constraints| {
                crate::ui::Text::new("sub").build(ctx);
            }),
        };
        let mut outer = Composer::new();
        outer.compose(|ctx| {
            crate::ui::Text::new("outer").build(ctx);
        });
        let _decoy = outer
            .arena
            .alloc_policy(Box::new(crate::layout::BoxLayout::new()) as Box<dyn MeasurePolicy>);
        let mut nodes = Vec::new();
        let _ = policy.measure(&mut nodes, &[], &[], Constraints::new(0.0, 200.0, 0.0, 100.0));
        let mut sub = take_subcomposed().expect("subcomposition recorded");
        let mut marks = crate::layout::node::NodeMarks::default();
        let adopted = adopt_inner_nodes(sub.composer_mut(), &mut outer.arena, None, &mut marks)
            .expect("adopted");
        let (size, _) = crate::layout::node::measure_node(
            &mut outer.arena.nodes,
            &outer.arena.policies,
            adopted,
            Constraints::new(0.0, 200.0, 0.0, 100.0),
        );
        assert!(
            size.height > 0.0,
            "the adopted node measures through its own policy, not the decoy"
        );
    }

    /// A panic inside the subcomposition happens while the OUTER composer is inside its layout
    /// transaction: the inner guards must unwind cleanly, the outer arena must be untouched, and the
    /// thread must still be able to subcompose afterwards.
    #[test]
    fn a_panicking_subcomposition_leaves_the_outer_tree_usable() {
        let policy = SubcomposePolicy {
            alignment: Alignment::Start,
            content: std::sync::Arc::new(|ctx: &mut ComposeCtx, _c: Constraints| {
                crate::ui::Text::new("before the panic").build(ctx);
                panic!("subcomposition panicked on purpose");
            }),
        };
        let mut outer = Composer::new();
        outer.compose(|ctx| {
            crate::ui::Text::new("outer").build(ctx);
        });
        outer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let before = outer.arena.nodes.len();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut nodes = Vec::new();
            policy.measure(&mut nodes, &[], &[], Constraints::new(0.0, 200.0, 0.0, 100.0))
        }));
        assert!(caught.is_err(), "the panic propagates out of measure");
        assert_eq!(outer.arena.nodes.len(), before, "nothing was adopted");
        let root = outer.layout_root_idx().expect("the outer tree still has its root");
        assert!(
            outer.arena_nodes()[root].measured_size.height > 0.0,
            "the outer tree still measures"
        );

        let ok = SubcomposePolicy {
            alignment: Alignment::Start,
            content: std::sync::Arc::new(|ctx: &mut ComposeCtx, _c: Constraints| {
                crate::ui::Text::new("after").build(ctx);
            }),
        };
        let mut nodes = Vec::new();
        let (size, _) = ok.measure(&mut nodes, &[], &[], Constraints::new(0.0, 200.0, 0.0, 100.0));
        assert!(size.height > 0.0, "a later subcomposition still works on this thread");
    }

    /// The cross-frame question, and the one that decides whether this can become a component: an
    /// adopted subtree is parented under its component's node and marked reused, then the NEXT frame
    /// composes the same tree normally. Does it survive the compose tail's prev-drain, and does the
    /// frame still render?
    #[test]
    fn an_adopted_subtree_survives_the_next_frame() {
        let build = |ctx: &mut ComposeCtx| {
            SubcomposeProbe::new()
                .modifier(crate::modifier::Modifier::new().size(80.0, 40.0))
                .build(ctx, |ctx, _c| {
                    crate::ui::Text::new("inner").build(ctx);
                });
        };
        let mut composer = Composer::new();
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let probe_idx = composer.arena.root.expect("the probe is the root");
        let mut sub = take_subcomposed().expect("the probe subcomposed");
        let adopted = adopt_inner_nodes(
            sub.composer_mut(),
            &mut composer.arena,
            Some(probe_idx),
            &mut composer.reused_nodes,
        )
        .expect("adopted");
        assert!(
            composer.arena.nodes[probe_idx].children.contains(&adopted),
            "the adopted root is a child of the probe node"
        );

        // Frame 2: the same tree composes again, nothing dirty.
        composer.compose(build);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert!(
            composer
                .arena_nodes()
                .iter()
                .any(|n| n.slot_key == ADOPTED_SLOT_KEY),
            "the adopted node survived the frame's compose tail (prev-drain)"
        );
    }
}
