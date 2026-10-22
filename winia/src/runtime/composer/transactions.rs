use super::*;



pub(super) struct ComposeRuntimeSnapshot {
    pub(super) slot_table: SlotTableRuntimeSnapshot,
    pub(super) current_group_key: u32,
    pub(super) path_counters: crate::layout::node::SlotKeyMap<u32>,
    pub(super) remember_path_counters: crate::layout::node::SlotKeyMap<u32>,
    pub(super) scope_source_stack: Vec<Option<u64>>,
    pub(super) key_override_stack: Vec<u64>,
    pub(super) pending_recomposition: VecDeque<u64>,
    pub(super) needs_recomposition: bool,
    pub(super) node_stack: Vec<usize>,
    pub(super) group_skip_stack: Vec<bool>,
    pub(super) overlay_active: HashMap<u64, bool>,
    pub(super) entered_compose_keys: crate::layout::node::SlotKeySet,
    pub(super) reused_nodes: crate::layout::node::NodeMarks,
}

/// Restores the small, non-owning compose runtime context on panic. The full
/// SlotTable/arena transaction remains deliberately separate because it owns
/// `Box<dyn Any>` and user callbacks.
pub(super) struct ComposeRuntimeTransaction {
    pub(super) composer: *mut Composer,
    pub(super) committed: bool,
}

impl ComposeRuntimeTransaction {
    pub(super) fn new(composer: &mut Composer) -> Self {
        composer.compose_transaction = Some(composer.capture_compose_runtime());
        Self { composer, committed: false }
    }

    pub(super) fn commit(&mut self) {
        self.committed = true;
        unsafe { (*self.composer).compose_transaction = None; }
    }
}

impl Drop for ComposeRuntimeTransaction {
    fn drop(&mut self) {
        if !self.committed {
            unsafe { (*self.composer).rollback_compose_runtime(); }
        }
    }
}

/// State the claim walk needs while the slot tree is walked (`SlotTable::collect_desc_tree`).

/// Snapshot of mutable Composer state touched by layout. A measurement policy
/// can execute user code, so panic must preserve the last committed graph and
/// the invalidation batch for a retry.
pub(super) struct LayoutTransactionSnapshot {
    pub(super) pending: Vec<StateId>,
    pub(super) layout_dirty_keys: HashSet<u64>,
    pub(super) prev_node_by_key: crate::layout::node::SlotKeyMap<usize>,
    pub(super) layout_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    pub(super) layout_deps: HashMap<StateId, HashSet<u64>>,
    pub(super) layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    pub(super) removed_slot_keys: HashSet<u64>,
    pub(super) root: Option<usize>,
    pub(super) nodes_len: usize,
    pub(super) free_nodes: Vec<usize>,
    pub(super) free_policies: Vec<usize>,
    pub(super) node_state: Vec<LayoutNodeTransactionState>,
    pub(super) scroll_limits: Vec<(crate::runtime::state::Backchannel<f32>, f32)>,
}

/// The state a layout frame can change on a node that already existed.
///
/// Deliberately NOT the whole node. `LayoutTransaction` exists so a panic mid-layout leaves the
/// composer recoverable, and what it keeps is the set of fields layout actually writes: measure writes
/// the size, the flags and the scroll metrics, and placement writes the position. What is left out is
/// out because layout cannot touch it — the modifier chain, the child list, the measure policy and the
/// content flags are all composition products, written by `materialize::materialize` while COMPOSING,
/// and the transaction's whole lifetime sits inside `layout()`.
///
/// Cloning them cost a `Vec` per node plus — through a `TextContent`'s `String` — a string per text
/// node, every frame. Measured at 4001 nodes, that clone was the single largest item in layout, larger
/// than measurement itself; removing it cut an idle 800-row frame by 40% (`docs/benchmarks.md`).
///
/// Rollback already forces a complete retry (`dirty = true`, cached constraints and paragraph cleared),
/// so nothing here has to preserve a partially rebuilt measurement either.
#[derive(Clone)]
pub(super) struct LayoutNodeTransactionState {
    pub(super) idx: usize,
    pub(super) focused: bool,
    pub(super) measured_size: crate::unit::Size,
    pub(super) position: crate::unit::Offset,
    pub(super) scroll_viewport_height: f32,
    pub(super) scroll_viewport_width: f32,
    pub(super) scroll_content_height: f32,
    pub(super) scroll_content_width: f32,
    pub(super) scroll_reverse: bool,
}

pub(super) struct LayoutTransaction {
    pub(super) composer: *mut Composer,
    pub(super) snapshot: Option<LayoutTransactionSnapshot>,
    pub(super) committed: bool,
}

impl LayoutTransaction {
    /// Takes the whole-tree reuse index layout is about to rebuild, and keeps it for rollback.
    ///
    /// `prev_node_by_key` is cleared and rebuilt from the arena by `layout()`
    /// (`collect_layout_index`), so the snapshot needs its old content only because a panic has to
    /// leave the composer exactly as it was — and that content is genuinely needed. It does not follow
    /// that it must be COPIED: the rebuild begins by clearing, so while the new content is being built
    /// nothing reads what the composer had. Moving the map into the snapshot is therefore equivalent,
    /// and it removed a deep clone of every node's cached entry — a `Modifier` (`Vec` + any strings)
    /// and a `RefCell` per node. Measured: 149 µs of an idle 800-row layout (4001 nodes), on a frame
    /// whose actual measurement folded in 0.04 µs (`docs/benchmarks.md`).
    ///
    /// What remains is the drop of the snapshot's map at commit (which the clone paid too) plus one
    /// allocation for the map being rebuilt — which is why `collect_layout_index` reserves up front
    /// rather than letting a `HashMap` grow into 4000 entries. Rollback is unchanged: it restores the
    /// map it took.
    pub(super) fn new(composer: &mut Composer) -> Self {
        let node_state = composer
            .arena
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| LayoutNodeTransactionState {
                idx,
                focused: node.focused,
                measured_size: node.measured_size,
                position: node.position,
                scroll_viewport_height: node.scroll_viewport_height,
                scroll_viewport_width: node.scroll_viewport_width,
                scroll_content_height: node.scroll_content_height,
                scroll_content_width: node.scroll_content_width,
                scroll_reverse: node.scroll_reverse,
            })
            .collect();
        let mut scroll_limits = Vec::new();
        for node in &composer.arena.nodes {
            if let Some(scroll) = node.modifier.vertical_scroll_state() {
                scroll_limits.push((scroll.fling_limit.clone(), scroll.fling_limit.peek()));
            }
            if let Some(scroll) = node.modifier.horizontal_scroll_state() {
                scroll_limits.push((scroll.fling_limit.clone(), scroll.fling_limit.peek()));
            }
        }

        Self {
            composer: composer as *mut Composer,
            snapshot: Some(LayoutTransactionSnapshot {
                pending: composer.pending_states.pending_ids(),
                layout_dirty_keys: composer.layout_dirty_keys.clone(),
                prev_node_by_key: std::mem::take(&mut composer.prev_node_by_key),
                layout_slot_reads: composer.layout_slot_reads.clone(),
                layout_deps: composer.layout_deps.clone(),
                layout_signal_handles: composer.layout_signal_handles.clone(),
                removed_slot_keys: composer.removed_slot_keys.clone(),
                root: composer.arena.root,
                nodes_len: composer.arena.nodes.len(),
                free_nodes: composer.arena.free.clone(),
                free_policies: composer.arena.free_policies.clone(),
                node_state,
                scroll_limits,
            }),
            committed: false,
        }
    }

    pub(super) fn commit(&mut self) {
        self.committed = true;
        self.snapshot = None;
    }

    unsafe fn rollback(&mut self) {
        let Some(snapshot) = self.snapshot.take() else { return };
        let composer = unsafe { &mut *self.composer };

        composer.pending_states.restore_pending(&snapshot.pending);
        composer.layout_dirty_keys = snapshot.layout_dirty_keys;
        composer.prev_node_by_key = snapshot.prev_node_by_key;
        composer.layout_slot_reads = snapshot.layout_slot_reads;
        composer.layout_deps = snapshot.layout_deps;
        composer.layout_signal_handles = snapshot.layout_signal_handles;
        composer.removed_slot_keys = snapshot.removed_slot_keys;
        composer.arena.root = snapshot.root;
        composer.arena.nodes.truncate(snapshot.nodes_len);
        composer.arena.free = snapshot.free_nodes;
        composer.arena.free_policies = snapshot.free_policies;

        for state in snapshot.node_state {
            if let Some(node) = composer.arena.nodes.get_mut(state.idx) {
                node.focused = state.focused;
                node.measured_size = state.measured_size;
                node.position = state.position;
                node.scroll_viewport_height = state.scroll_viewport_height;
                node.scroll_viewport_width = state.scroll_viewport_width;
                node.scroll_content_height = state.scroll_content_height;
                node.scroll_content_width = state.scroll_content_width;
                node.scroll_reverse = state.scroll_reverse;
                // Force a complete retry. This is safer than restoring a
                // partially rebuilt paragraph or measurement cache.
                node.dirty = true;
                node.layout_dirty = false;
                node.cached_constraints = None;
                if let Ok(mut paragraph) = node.cached_paragraph.try_borrow_mut() {
                    *paragraph = None;
                }
            }
        }
        for (state, value) in snapshot.scroll_limits {
            state.set(value);
        }
    }
}

impl Drop for LayoutTransaction {
    fn drop(&mut self) {
        if !self.committed {
            // Rollback must not panic while an application panic is unwinding.
            unsafe { self.rollback(); }
        }
    }
}

/// Snapshot of the dependency graph committed by the last successful compose.
/// This is intentionally narrower than a SlotTable/arena transaction: it restores
/// only graph state and subscriptions after a late compose panic.
pub(super) struct ComposeDependencySnapshot {
    pub(super) compose_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    pub(super) slot_deps: HashMap<StateId, HashSet<u64>>,
    pub(super) signal_handles: HashMap<StateId, Arc<StateSignal>>,
    pub(super) layout_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    pub(super) layout_deps: HashMap<StateId, HashSet<u64>>,
    pub(super) layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    pub(super) pending: Vec<StateId>,
    pub(super) layout_dirty_keys: HashSet<u64>,
    pub(super) removed_slot_keys: HashSet<u64>,
}

/// Restores dependency maps and live signal subscriptions if compose panics
/// after reconciliation but before its cleanup completes.
pub(super) struct ComposeDependencyTransaction {
    pub(super) composer: *mut Composer,
    pub(super) snapshot: Option<ComposeDependencySnapshot>,
    pub(super) committed: bool,
}

impl ComposeDependencyTransaction {
    pub(super) fn new(composer: &Composer) -> Self {
        Self {
            composer: composer as *const Composer as *mut Composer,
            snapshot: Some(ComposeDependencySnapshot {
                compose_slot_reads: composer.compose_slot_reads.clone(),
                slot_deps: composer.slot_deps.clone(),
                signal_handles: composer.signal_handles.clone(),
                layout_slot_reads: composer.layout_slot_reads.clone(),
                layout_deps: composer.layout_deps.clone(),
                layout_signal_handles: composer.layout_signal_handles.clone(),
                pending: composer.pending_states.pending_ids(),
                layout_dirty_keys: composer.layout_dirty_keys.clone(),
                removed_slot_keys: composer.removed_slot_keys.clone(),
            }),
            committed: false,
        }
    }

    pub(super) fn commit(&mut self) {
        self.snapshot = None;
        self.committed = true;
    }

    unsafe fn rollback(&mut self) {
        let Some(snapshot) = self.snapshot.take() else { return };
        let composer = unsafe { &mut *self.composer };
        composer.compose_slot_reads = snapshot.compose_slot_reads;
        composer.slot_deps = snapshot.slot_deps;
        composer.signal_handles = snapshot.signal_handles;
        composer.layout_slot_reads = snapshot.layout_slot_reads;
        composer.layout_deps = snapshot.layout_deps;
        composer.layout_signal_handles = snapshot.layout_signal_handles;
        composer.layout_dirty_keys = snapshot.layout_dirty_keys;
        composer.removed_slot_keys = snapshot.removed_slot_keys;

        // Remove every live subscription created or retained by the failed
        // frame before restoring the committed signal set. Otherwise a signal
        // that only appeared in the failed graph can still enqueue the queue.
        composer.pending_states.retain_signals(&HashSet::new());

        // Restore the exact pending batch from before this compose began.
        // Notifications produced by the failed frame must not leak into the
        // restored dependency graph.
        composer.pending_states.drain();
        composer.pending_states.restore_pending(&snapshot.pending);

        // Replace the queue's signal set with the last committed handles.
        let mut restored = HashSet::new();
        for signal in composer
            .signal_handles
            .values()
            .chain(composer.layout_signal_handles.values())
        {
            if restored.insert(signal.id()) {
                composer.pending_states.subscribe_signal(signal);
            }
        }

        composer.debug_assert_dependency_graphs();
    }
}

impl Drop for ComposeDependencyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            // Rollback must not panic while another application panic unwinds.
            unsafe { self.rollback(); }
        }
    }
}

/// Keeps compose-triggering invalidations queued until the compose transaction
/// reaches its final cleanup. A panic restores only the consumed batch; other
/// Composer mutations remain governed by the existing self-healing path.
///
/// # 不变量（形式化，2026-08-26）
///
/// 1. **帧内新通知只进入下一批**：`drain_matching`（state.rs:159）在队列锁内
///    原子消费匹配的 IDs，未匹配的 IDs 留在队列。帧内 `State::set` → `enqueue`
///    （state.rs:126）在锁内追加新 ID → 新通知一定在队列中等待下帧，不会在当前帧
///    被消费。请勿在 `drain_matching` 锁外修改 pending 队列。
///
/// 2. **panic 恢复不重复**：`Drop` 未 commit 时调用 `restore_pending`
///    （state.rs:147），在锁内**去重**追加——帧内已到来的新通知不会被重复添加。
///
/// 3. **commit 后不再回滚**：commit 后 `ids` 清空、`committed=true`，`Drop` 不
///    做任何操作——已消费的批被视为已提交。
///
/// 4. **布局 pending 不被 compose 误消费**：`drain_matching` 只取出 `slot_deps`
///    中存在的 IDs（composeIds），layout-only 的 IDs 留在队列，由 `layout()`
///    通过 `drain_non_compose_collect_layout`（state.rs:175）原子消费。
pub(super) struct PendingBatchGuard {
    pub(super) queue: Arc<ComposerSubscription>,
    pub(super) ids: Vec<StateId>,
    pub(super) committed: bool,
}

impl PendingBatchGuard {
    pub(super) fn new(queue: Arc<ComposerSubscription>, ids: Vec<StateId>) -> Self {
        Self {
            queue,
            ids,
            committed: false,
        }
    }

    pub(super) fn ids(&self) -> &[StateId] {
        &self.ids
    }

    pub(super) fn commit(&mut self) {
        self.ids.clear();
        self.committed = true;
    }
}

impl Drop for PendingBatchGuard {
    fn drop(&mut self) {
        if !self.committed {
            self.queue.restore_pending(&self.ids);
        }
    }
}
