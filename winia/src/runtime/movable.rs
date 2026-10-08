//! Movable content with placement-independent remembered values and node identities.
//!
//! A retained handle moves between hosts in a composition without rebuilding its state. Its placement
//! owns a separate slot tree; a live reference attaches the nodes under the current host. State reads
//! invalidate both the stored reader and its current host. Locals come from the invocation site.
//!
//! A completed `Composer::compose` with no placement disposes the stored state and nodes, even if the
//! callable is retained. Reinsertion starts fresh; this is not a keep-alive cache.
//!
//! Deliberate minimal-API restriction: one placement per handle per composition, with no nested movable
//! invocation. Duplicate/nested placement is rejected rather than silently omitted. Compose supports
//! multiple independent placements of one callable (copies as needed); use distinct handles for
//! simultaneous copies until matching surviving placements to released instances is implemented here.
//! This restriction is winia's, not a Compose rule.
//!
//! The callable is initialized once. Each invocation conservatively enters its stored child scopes so
//! fresh captured payloads and placement locals cannot be hidden by clean wrappers; values and nodes
//! remain reused. Unchanged hosts may skip their invocation entirely.

use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;

/// A retained callable from [`ComposeCtx::remember_movable_content`].
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

    /// Place this callable at the current host. Duplicate or nested placement is rejected by the
    /// current single-placement API. A failed invocation restores the caller before resuming unwind.
    pub fn compose(&self, ctx: &mut ComposeCtx) {
        ctx.begin_movable(self.id);
        let mut reads = crate::runtime::state::checkpoint_dependency_reads();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (self.content)(ctx);
            ctx.end_movable();
        }));
        match result {
            Ok(()) => reads.commit(),
            Err(panic) => {
                ctx.abort_movable();
                std::panic::resume_unwind(panic);
            }
        }
    }

    /// Stable callable identity, for diagnostics.
    pub fn id(&self) -> u64 { self.id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{BoxLayout, Constraints};
    use crate::layout::components::Column;
    use crate::modifier::Modifier;
    use crate::runtime::composer::Composer;
    use crate::runtime::state::State;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn leaf(ctx: &mut ComposeCtx, width: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(width, 10.0));
        ctx.end_node();
    }
    fn host(ctx: &mut ComposeCtx) {
        ctx.start_container(0x8888, Modifier::new(), BoxLayout::new());
    }
    fn layout(composer: &mut Composer) {
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    }
    fn leaf_values(composer: &Composer) -> Vec<(u64, f32)> {
        fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, out: &mut Vec<(u64, f32)>) {
            if nodes[idx].children.is_empty() { out.push((nodes[idx].id, nodes[idx].measured_size.width)); }
            for &child in &nodes[idx].children { walk(nodes, child, out); }
        }
        let mut values = Vec::new();
        if let Some(root) = composer.layout_root_idx() { walk(composer.arena_nodes(), root, &mut values); }
        values
    }

    #[test]
    fn an_inlined_payload_is_a_child_of_the_invoking_component() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(|ctx| {
                ctx.key("payload", |ctx| leaf(ctx, 30.0));
            }));
            host(ctx);
            ctx.key("own", |ctx| leaf(ctx, 20.0));
            ctx.key("invoke", |ctx| payload.compose(ctx));
            ctx.end_node();
        });
        layout(&mut composer);
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(nodes[root].children.len(), 2);
        let widths: Vec<_> = nodes[root].children.iter().map(|&i| nodes[i].measured_size.width).collect();
        assert_eq!(widths, vec![20.0, 30.0], "the payload itself, not a wrapper, must materialize");
    }

    #[test]
    fn handles_remembered_in_a_loop_survive_being_invoked_deeper_down() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let handles: Vec<_> = (0..3).map(|index| ctx.key(index, |ctx| ctx.remember_movable_content(move |ctx| {
                ctx.key("payload", |ctx| leaf(ctx, 10.0 + index as f32));
            }))).collect();
            Column::new().modifier(Modifier::new().size(200.0, 60.0)).build(ctx, |ctx| {
                for (index, handle) in handles.into_iter().enumerate() {
                    ctx.key(index, |ctx| Column::new().modifier(Modifier::new().size(100.0, 20.0)).build(ctx, |ctx| {
                        handle.compose(ctx);
                    }));
                }
            });
        });
        layout(&mut composer);
        assert_eq!(leaf_values(&composer).into_iter().map(|(_, w)| w).collect::<Vec<_>>(), vec![10.0, 11.0, 12.0]);
    }

    #[test]
    fn removing_and_reinserting_a_child_preserves_only_surviving_state() {
        let count = Arc::new(AtomicUsize::new(3));
        let inits = Arc::new(AtomicUsize::new(0));
        let mut composer = Composer::new();
        let frame = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let count = count.clone();
                let inits = inits.clone();
                let content = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                    for index in 0..count.load(Ordering::SeqCst) {
                        ctx.key(index, |ctx| Column::new().build(ctx, |ctx| {
                            let marker = ctx.remember(|| inits.fetch_add(1, Ordering::SeqCst));
                            leaf(ctx, 10.0 + marker.peek() as f32);
                        }));
                    }
                }));
                host(ctx);
                ctx.key("invoke", |ctx| content.compose(ctx));
                ctx.end_node();
            });
            layout(composer);
        };
        frame(&mut composer);
        let original = leaf_values(&composer);
        assert_eq!(original.len(), 3);
        assert_eq!(inits.load(Ordering::SeqCst), 3);
        count.store(2, Ordering::SeqCst);
        frame(&mut composer);
        assert_eq!(leaf_values(&composer), original[..2]);
        assert_eq!(inits.load(Ordering::SeqCst), 3);
        count.store(3, Ordering::SeqCst);
        frame(&mut composer);
        let restored = leaf_values(&composer);
        assert_eq!(&restored[..2], &original[..2]);
        assert_eq!(inits.load(Ordering::SeqCst), 4, "the actually removed child initializes anew");
        assert_eq!(restored[2].1, 13.0);
    }

    #[test]
    fn a_keyed_remember_inside_movable_content_survives_a_resize_of_its_host() {
        let width = Arc::new(AtomicUsize::new(40));
        let inits = Arc::new(AtomicUsize::new(0));
        let mut composer = Composer::new();
        let frame = |composer: &mut Composer, host_width: f32| {
            composer.compose(|ctx| {
                let width = width.clone();
                let inits = inits.clone();
                let handle = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| ctx.key("payload", |ctx| {
                    let marker = ctx.remember(|| inits.fetch_add(1, Ordering::SeqCst));
                    assert_eq!(marker.peek(), 0);
                    leaf(ctx, width.load(Ordering::SeqCst) as f32);
                })));
                ctx.start_container(0x8888, Modifier::new().size(host_width, 40.0), BoxLayout::new());
                ctx.key("invoke", |ctx| handle.compose(ctx));
                ctx.end_node();
            });
            layout(composer);
        };
        frame(&mut composer, 100.0);
        let id = leaf_values(&composer)[0].0;
        width.store(24, Ordering::SeqCst);
        frame(&mut composer, 80.0);
        assert_eq!(composer.layout_root().unwrap().measured_size.width, 80.0);
        assert_eq!(leaf_values(&composer), vec![(id, 24.0)]);
        assert_eq!(inits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn state_survives_a_move_between_parents() {
        let inits = Arc::new(AtomicUsize::new(0));
        let mut composer = Composer::new();
        let frame = |composer: &mut Composer, nested: bool| {
            composer.compose(|ctx| {
                let source = inits.clone();
                let handle = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                    ctx.key("payload", |ctx| {
                        let marker: State<usize> = ctx.remember(|| source.fetch_add(1, Ordering::SeqCst));
                        assert_eq!(marker.peek(), 0);
                        leaf(ctx, 20.0);
                    });
                }));
                host(ctx);
                if nested {
                    ctx.key("nested", |ctx| Column::new().modifier(Modifier::new().size(40.0, 40.0)).build(ctx, |ctx| {
                        ctx.key("invoke-b", |ctx| handle.compose(ctx));
                    }));
                } else {
                    ctx.key("invoke-a", |ctx| handle.compose(ctx));
                }
                ctx.end_node();
            });
            layout(composer);
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().unwrap();
            let invoking_parent = if nested { nodes[root].children[0] } else { root };
            assert_eq!(nodes[invoking_parent].children.len(), 1);
            assert_eq!(nodes[nodes[invoking_parent].children[0]].measured_size.width, 20.0);
        };
        frame(&mut composer, false);
        let original = leaf_values(&composer);
        frame(&mut composer, true);
        assert_eq!(leaf_values(&composer), original);
        frame(&mut composer, false);
        assert_eq!(leaf_values(&composer), original);
        assert_eq!(inits.load(Ordering::SeqCst), 1);
    }
}
