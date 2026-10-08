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
