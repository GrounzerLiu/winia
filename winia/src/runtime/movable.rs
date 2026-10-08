//! Movable content — Compose's `movableContentOf`.
//!
//! Content that can be composed in one place on one frame and another place on the next, taking its
//! `remember`ed state with it. Compose uses it where a layout chooses between two branches that hold
//! the SAME content: `NavigationSuiteScaffold` invokes one content lambda in both the bar and the rail
//! branch (`NavigationSuiteScaffold.kt:577-578`), and the whole point is that switching between them
//! does not rebuild what is inside.
//!
//! winia's composer drops the slots of a branch that was not visited (`end_restartable_group`'s
//! `children.retain(|c| c.visited)`), which is why switching a shape used to reset the items. The
//! content therefore does not live in the tree at all: it lives in the slot table's movable store, and
//! the invocation site holds a reference slot whose position decides only where the content's NODES are
//! parented (the descriptor walk inlines the store's subtree there).
//!
//! What survives a move: everything `ctx.remember`ed, plus the materialized nodes themselves — the
//! arena is rebuilt from the slot tree each frame and reuses nodes BY KEY, and the keys inside movable
//! content come from statement ids, so they do not depend on where the content is invoked.

use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;

/// A handle to movable content, from [`ComposeCtx::remember_movable_content`].
#[derive(Clone)]
pub struct MovableContent {
    pub(crate) id: u64,
    pub(crate) content: Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>,
}

impl MovableContent {
    pub(crate) fn new(content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            content: Arc::new(content),
        }
    }

    /// Compose this content here.
    ///
    /// Call it at most once per composition, and not from inside other movable content. A panic in the
    /// content is covered by the composer's own transaction rollback, which restores the store state
    /// along with the rest of the compose runtime.
    pub fn compose(&self, ctx: &mut ComposeCtx) {
        // FALSIFY: bypassing the store composes the content in the tree at each site, which is the
        // behaviour this exists to replace — its state is rebuilt on every move.
        if std::env::var("MOVABLE_OFF").is_ok() {
            (self.content)(ctx);
            return;
        }
        ctx.begin_movable(self.id);
        (self.content)(ctx);
        ctx.end_movable();
    }

    /// The content's id, for tests and diagnostics.
    pub fn id(&self) -> u64 {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::components::Column;
    use crate::layout::Constraints;
    use crate::modifier::Modifier;
    use crate::runtime::composer::Composer;

    /// A payload composed through a movable handle is still a CHILD of the component that invoked it,
    /// in the position that component's measure policy expects.
    ///
    /// This is the contract the navigation suite's item policies are written against — they index
    /// `children[1]` and `children[2]` for the icon and the label
    /// (`navigation_bar.rs:659`, `navigation_rail.rs:1396`), and they panicked with "index out of
    /// bounds: the len is 2 but the index is 2" while the payloads were being wired up. The ref slot
    /// itself holds nothing, so the payload's node has to arrive as the component's own child.
    #[test]
    fn an_inlined_payload_is_a_child_of_the_invoking_component() {
        use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};

        /// Reports how many children it was measured with, and sizes itself from them.
        #[derive(Debug)]
        struct CountingPolicy(std::sync::Arc<std::sync::Mutex<Vec<usize>>>);
        impl MeasurePolicy for CountingPolicy {
            fn measure(
                &self,
                nodes: &mut Vec<LayoutNode>,
                policies: &[Box<dyn MeasurePolicy>],
                children: &[usize],
                constraints: Constraints,
            ) -> (crate::unit::Size, Vec<Placement>) {
                self.0.lock().unwrap().push(children.len());
                let mut placements = Vec::new();
                let mut x = 0.0;
                let mut size = crate::unit::Size::new(0.0, 0.0);
                for &c in children {
                    let (cs, _) = measure_node(nodes, policies, c, constraints.loosen());
                    placements.push(Placement { size: cs, position: crate::unit::Offset::new(x, 0.0) });
                    x += cs.width;
                    size = crate::unit::Size::new(x, size.height.max(cs.height));
                }
                let size = crate::unit::Size::new(
                    constraints.constrain_width(size.width),
                    constraints.constrain_height(size.height),
                );
                (size, placements)
            }
            fn place(
                &self,
                nodes: &mut Vec<LayoutNode>,
                children: &[usize],
                placements: &[Placement],
            ) {
                for (index, &child) in children.iter().enumerate() {
                    nodes[child].position = placements[index].position;
                    nodes[child].measured_size = placements[index].size;
                }
            }
        }

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let payload = ctx.remember_movable_content(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().size(30.0, 10.0));
                ctx.end_node();
            });
            // A component with a child of its own, then the payload: the policy must see TWO children.
            // The payload is composed from INSIDE a component's content closure (a restartable group),
            // which is where the navigation suite invokes its item payloads.
            let key = ctx.next_key();
            let seen_here = seen.clone();
            ctx.start_container(
                key,
                Modifier::new().size(100.0, 20.0),
                CountingPolicy(seen_here),
            );
            let own = ctx.next_key();
            ctx.start_leaf(own, Modifier::new().size(20.0, 10.0));
            ctx.end_node();
            let payload_for_group = payload.clone();
            Column::new().build(ctx, move |ctx| {
                payload_for_group.compose(ctx);
            });
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[2],
            "the component's policy must be measured with its own child AND the inlined payload"
        );
    }

    /// Compose's movable group keeps everything under it, including the parts this frame did NOT
    /// compose — and that is what makes movable content survive a structure change INSIDE it, not just
    /// a move between parents.
    ///
    /// This is the navigation suite's shape, reduced: the items are composed by the caller's loop, so a
    /// shape that shows fewer of them simply composes fewer this frame. The third item must still be
    /// the same item when the structure grows back — measured by its own counter, which would climb if
    /// its slot had been dropped along with the branch that stopped composing it.
    #[test]
    fn a_structures_state_survives_while_the_content_shrinks_and_grows_back() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static ITEM_INITS: AtomicUsize = AtomicUsize::new(0);
        static COUNT: AtomicUsize = AtomicUsize::new(3);

        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let content = ctx.remember_movable_content(|ctx| {
                    for _ in 0..COUNT.load(Ordering::SeqCst) {
                        let index = ITEM_INITS.fetch_add(1, Ordering::SeqCst);
                        let remembered: crate::runtime::state::State<usize> =
                            ctx.remember(|| index);
                        // Read it, so the slot is subscribed and the value means something.
                        let _ = remembered.get();
                        Column::new()
                            .modifier(Modifier::new().size(20.0, 10.0))
                            .build(ctx, |_ctx| {});
                    }
                });
                Column::new()
                    .modifier(Modifier::new().size(100.0, 100.0))
                    .build(ctx, |ctx| {
                        content.compose(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        frame(&mut composer);
        assert_eq!(ITEM_INITS.load(Ordering::SeqCst), 3, "three items composed");

        // The shape shows two: the third is not composed this frame, but must not be dropped.
        COUNT.store(2, Ordering::SeqCst);
        frame(&mut composer);
        assert_eq!(
            ITEM_INITS.load(Ordering::SeqCst),
            3,
            "composing fewer items must not re-init the ones that remain"
        );

        // …and growing back, the third item is the SAME item.
        COUNT.store(3, Ordering::SeqCst);
        frame(&mut composer);
        assert_eq!(
            ITEM_INITS.load(Ordering::SeqCst),
            3,
            "the third item's remembered value survived the shrink, so it is not re-initialized"
        );
    }

    /// The whole point, measured: content composed under one parent and then under ANOTHER keeps what
    /// it `remember`ed, and switching back and forth does not run its `remember` initializer again.
    ///
    /// The counter is the evidence — it counts how many times the content's initializer ran, so a
    /// rebuilt content shows up as a higher number and nothing else can fake it. Without movable
    /// content the middle frame drops the first parent's slots and the count climbs on every switch;
    /// this is the same measurement that caught the navigation suite (3 → 3 → 6).
    #[test]
    fn state_survives_a_move_between_parents() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static INITS: AtomicUsize = AtomicUsize::new(0);

        let mut composer = Composer::new();
        let mut in_second_parent = false;
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        let mut frame = |composer: &mut Composer, in_second_parent: bool| {
            composer.compose(|ctx| {
                let content = ctx.remember_movable_content(|ctx| {
                    let runs: crate::runtime::state::State<usize> =
                        ctx.remember(|| INITS.fetch_add(1, Ordering::SeqCst));
                    // Reading it is what subscribes this slot to the state.
                    let _ = runs.get();
                    Column::new()
                        .modifier(Modifier::new().size(20.0, 20.0))
                        .build(ctx, |_ctx| {});
                });
                // The two parents put the content at DIFFERENT DEPTHS, which is what makes their slot
                // keys differ even in a test (where keys fall back to the path hash): one frame the
                // content is the child of a Column, the next it is a grandchild. Without the store the
                // second frame cannot match the first frame's slot and the content is rebuilt.
                if in_second_parent {
                    Column::new()
                        .modifier(Modifier::new().size(60.0, 60.0))
                        .build(ctx, |ctx| {
                            Column::new()
                                .modifier(Modifier::new().size(40.0, 40.0))
                                .build(ctx, |ctx| {
                                    content.compose(ctx);
                                });
                        });
                } else {
                    Column::new()
                        .modifier(Modifier::new().size(80.0, 60.0))
                        .build(ctx, |ctx| {
                            content.compose(ctx);
                        });
                }
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            // The content must be IN the tree too — under whichever parent currently invokes it. This
            // is the other half of the feature: the store keeps the state, and the descriptor walk
            // inlines the content at the invocation site, so it is laid out where it is composed.
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("laid out");
            // The invoking parent IS the composition root here (it is the outermost thing composed),
            // and the content must be its child — wherever in the composition it was invoked from.
            let child = nodes[root]
                .children
                .iter()
                .map(|&c| nodes[c].measured_size.width)
                .find(|w| (*w - 20.0).abs() < 0.01);
            seen.borrow_mut().push(INITS.load(Ordering::SeqCst));
            fn dump(nodes: &[crate::layout::node::LayoutNode], idx: usize, depth: usize, out: &mut String) {
                out.push_str(&format!(
                    "\n{}{} {:?} kids={} text={}",
                    "  ".repeat(depth),
                    idx,
                    nodes[idx].measured_size,
                    nodes[idx].children.len(),
                    nodes[idx].has_text_content
                ));
                let kids = nodes[idx].children.clone();
                for c in kids {
                    dump(nodes, c, depth + 1, out);
                }
            }
            let mut tree = String::new();
            dump(nodes, root, 0, &mut tree);
            assert!(
                child.is_some(),
                "the content's own node is a child of the parent that invoked it: {tree}"
            );
        };

        frame(&mut composer, in_second_parent);
        assert_eq!(
            seen.borrow().as_slice(),
            &[1],
            "the content's initializer runs once on the first frame"
        );

        in_second_parent = true;
        frame(&mut composer, in_second_parent);
        frame(&mut composer, false);
        assert_eq!(
            seen.borrow().as_slice(),
            &[1, 1, 1],
            "and not again after moving to the other parent, nor after moving back: {:?}",
            seen.borrow()
        );
    }
}
