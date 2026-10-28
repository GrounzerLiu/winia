use super::*;
use super::transactions::*;

// ── Composer ──

/// 组合引擎。
///
/// 每个窗口/组合根拥有一个 Composer 实例。
/// 负责:
/// - 管理 SlotTable（组合节点的持久化存储）
/// - 调度重组（状态变化 → 标记 dirty → 下一帧批量重组）
/// - 持有全局 key 计数器
static NEXT_COMPOSER_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct Composer {
    pub(crate) slot_table: SlotTable,
    /// Identity of this composer, unique for the process lifetime. Anything that caches per composer
    /// must key on this rather than on an arena index: node indices are local to a composer, so two
    /// windows (or a page and an overlay) both have a node 0.
    pub(crate) uid: u64,
    /// Where this window's saveable values come from and go to. Provided to every compose pass, so
    /// `remember_saveable` sees it on every frame rather than only the first.
    pub(crate) saveable: Option<std::sync::Arc<crate::runtime::saveable::SaveableStateRegistry>>,
    pub(crate) current_group_key: u32,
    pub(super) compose_transaction: Option<ComposeRuntimeSnapshot>,
    /// 每路径独立 counter（next_group_key 用）——同组合位置跨帧 counter 恒定，
    /// key 不随 Skip/Enter 的 next_key 调用序变化（全局 counter 会因 Skip 的
    /// content 不执行而平移 → key 漂移 → 节点复用错位 + 常量折叠冻结）
    pub(super) path_counters: crate::layout::node::SlotKeyMap<u32>,
    /// 每路径独立 counter（next_remember_key 用——同上，防 remember key 漂移）
    pub(super) remember_path_counters: crate::layout::node::SlotKeyMap<u32>,

    /// 组合 scope 的源码哈希栈（宏传——函数级 key 基）
    pub(super) scope_source_stack: Vec<Option<u64>>,
    /// Invocation context restored when a movable placement finishes, including explicit keys and
    /// dependency targets. Neither identities nor readers inside a placement belong to its caller.
    pub(super) movable_saved_scopes: Vec<Option<u64>>,
    pub(super) movable_saved_stmts: Vec<(u32, u32)>,
    pub(super) movable_saved_keys: Vec<u64>,
    pub(super) movable_saved_groups: Vec<u64>,
    pub(super) movable_saved_active: u64,
    pub(super) movable_saved_skip_depth: usize,
    pub(super) movable_saved_group_key: u32,
    pub(super) movable_saved_path_counters: crate::layout::node::SlotKeyMap<u32>,
    pub(super) movable_saved_remember_counters: crate::layout::node::SlotKeyMap<u32>,
    pub(super) movable_saved_params: Vec<Box<dyn ParamValue>>,
    pub(super) movable_saved_pending_next: usize,
    /// ctx.key() 显式 key 栈（最高优先级）
    pub(super) key_override_stack: Vec<u64>,
    pub(super) pending_recomposition: VecDeque<u64>,
    pub(super) needs_recomposition: bool,
    pub(crate) arena: crate::layout::node::NodeArena,
    pub(super) node_stack: Vec<usize>,
    /// 记录每个 start_restartable_group 的 skip 状态（用于 end_restartable_group 判断）
    pub(super) group_skip_stack: Vec<bool>,
    /// 顶层弹出层（Popup/Dialog/DropdownMenu——组合期注册，compose 后取走；
    /// 内容为独立组合单元——独立 Composer 物化/布局/渲染，不参与主树布局）
    pub(crate) overlays: Vec<crate::overlay::OverlayDesc>,
    /// Callbacks queued by `ComposeCtx::side_effect`, run once at the END of a successful
    /// `compose()` — after the slot tree is materialized and after the dependency frame closed.
    /// Compose's `SideEffect`: publishing to the world outside the composition, with no cleanup
    /// and no rollback. A pass that panics before the drain drops them unrun.
    pub(super) post_compose: Vec<Box<dyn FnOnce() + Send>>,
    /// 本帧组合期各 overlay 的 active 状态（Popup/Dialog build 总执行时记录——
    /// Skip 帧不记录 → sync 保留上帧；主动关闭 visible=false → 记录 false →
    /// sync 删除）。区分"注册方 Skip"（保留）与"主动关闭"（删除）——
    /// slot 层不可区分，需此组合期显式记录。
    pub(crate) overlay_active: HashMap<u64, bool>,
    /// state_id -> slot_keys 依赖映射
    pub(super) slot_deps: HashMap<StateId, HashSet<u64>>,
    /// compose 期每个 slot 的完整读取集合；Enter 时替换，Skip 时保留。
    ///
    /// `SlotKeyMap` for the same reason as `entered_compose_keys` beside it: keyed by `slot_key`, and
    /// the convergence pass looks a key up here up to three times per ENTERING group — 800 groups in a
    /// list loop is 2400 lookups a frame, and they were paying the std hasher on keys the composer had
    /// already mixed. Measured on the row loop (800 rows, the container re-running, every row
    /// skipping): 889 → 840 µs, best of four interleaved runs each way, with the unchanged idle arm as
    /// the control — the cost is proportional to the ENTERED set, which is what the idle frame does not
    /// have (`docs/benchmarks.md`).
    pub(super) compose_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    /// compose 依赖对应的 signal handle，用于移除 stale Composer 订阅。
    pub(super) signal_handles: HashMap<StateId, Arc<StateSignal>>,
    /// layout 依赖对应的 signal handle；compose 与 layout 共享一个队列但独立收敛。
    pub(super) layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    /// 帧内实际执行过的 compose slot；用于按 Enter/Skip 语义收敛读取集合。
    ///
    /// `SlotKeySet` rather than a plain `HashSet`: this is keyed by `slot_key` and takes one insert per
    /// ENTERING group — 4000 of them on a cold frame, with the std hasher on keys the composer had
    /// already mixed (`docs/benchmarks.md`).
    pub(super) entered_compose_keys: crate::layout::node::SlotKeySet,
    /// 布局期每个 slot 的读取集合；测量命中时替换，常量折叠时保留。
    /// 与 `compose_slot_reads` 同一理由取 `SlotKeyMap`（见上）。
    pub(super) layout_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    /// 布局依赖反向表（state_id → slot_key；由 layout_slot_reads 重建）
    pub(super) layout_deps: HashMap<StateId, HashSet<u64>>,
    /// 本帧 pending 消费收集的布局失效 key（layout() 应用后清空）。
    /// 协调器（shared_transition 的 flight writer）每帧往这里播种飞行端点的
    /// slot_key：`layout()` 每帧先清全树 layout_dirty 再按此集合重标祖先，
    /// 不播种则折叠的父级永不下探（逐帧重测就无从发生）。
    pub(crate) layout_dirty_keys: HashSet<u64>,
    /// Set by the shared-element writers when a flight attaches its layout override for the
    /// FIRST time (the node had none). The override can only be attached after layout —
    /// resolving a flight needs the target's measured rect — so the pass that just ran told
    /// the entering end's parent the target's NATURAL size; without this the frame renders
    /// with the parent stale and the rows below the hero dip and snap back. The app loop
    /// consumes it once per frame and re-lays-out, so at most one extra pass per composer per
    /// frame happens, and only on frames where a flight starts.
    pub(crate) layout_override_fresh: bool,
    /// 本帧确认移除的 slot_key（compose 末尾回收未复用节点时收集——layout_deps 死 key 清理用）
    pub(super) removed_slot_keys: HashSet<u64>,
    /// 本 Composer 实例的 pending state 通知队列
    pub(super) pending_states: Arc<crate::runtime::state::ComposerSubscription>,
    /// `ComposeCtx::changed` 暂存的参数（start_slot 时写入新 slot 的 params）
    ///
    /// Doubles as a RECYCLING buffer: `commit_pending_params` swaps it with the slot's previous vector,
    /// so the boxes (and the `Vec`'s capacity) come back here to be overwritten next frame instead of
    /// being dropped and re-allocated. Only the first `pending_next` entries are this frame's.
    pub(super) pending_params: Vec<Box<dyn ParamValue>>,
    /// Write cursor into `pending_params` — the position the next `changed` writes, and the count of
    /// declarations made for the group being built. A cursor rather than `pending_params.len()`, because
    /// that vector now arrives carrying the previous frame's (longer or shorter) parameter list.
    pub(super) pending_next: usize,
    /// 上帧布局树：slot_key → arena 节点索引（阶段D 节点复用——start_node 按 key 复用槽位）
    pub(crate) prev_node_by_key: crate::layout::node::SlotKeyMap<usize>,
    /// 本帧已复用的节点索引（free 时跳过——避免递归进本帧树形成环）
    pub(crate) reused_nodes: crate::layout::node::NodeMarks,
    /// Subcompositions parked during the current layout pass, waiting for adoption
    /// (`layout::subcompose`). Cleared at the start of every `layout()`; emptied by the adoption pass.
    pub(crate) subcompositions: Vec<(usize, crate::layout::subcompose::Subcomposition)>,
    /// A subcomposition this composer can re-arrange instead of re-composing (`layout::subcompose`).
    /// Taken by the next `subcompose()` call in the same frame and put back by `layout()`'s adoption
    /// pass, so a second layout pass in one frame reuses the first pass's composition.
    pub(super) subcomposition_cache: Option<Box<Composer>>,
    /// How many skipped subtrees the last `materialize` claimed in place, instead of re-encoding them
    /// into descriptors (`SlotTable::try_claim_skipped_subtree`). Test-visible so the tests can tell
    /// "the fast path ran" from "it bailed" — the two are deliberately indistinguishable in the tree.
    #[cfg(test)]
    pub(crate) skip_claims: usize,
    /// Subtrees whose in-place claim was rejected this frame (see `skip_claims`).
    #[cfg(test)]
    pub(crate) skip_claim_bails: usize,
    /// Slots the last compose found still in the tree but not composed (and not inside a skipped
    /// subtree) — the slots `collect_live_keys` exists to exclude. Test-visible because the number
    /// only exists at that moment: materialize consumes the frame's markers (`desc`, `skip_modifier`),
    /// so counting the same tree after a frame reports slots that were live all along.
    #[cfg(test)]
    pub(crate) live_residue: usize,
    /// 当前选区注册表（SelectionContainer compose 时注入，供事件处理访问）
    pub(crate) selection_registrar: Option<crate::text::selection::SelectionRegistrar>,
    /// Window lifecycle flags are scoped to this Composer, not the thread.
    pub(crate) lifecycle: crate::runtime::lifecycle::LifecycleState,
    /// The winit window this composer belongs to; 0 when unwired (a test, an overlay composer).
    /// `ComposeCtx::frame_clock` hands out this window's clock.
    pub(crate) window_id: u64,
    /// Adaptive window size context owned by this Composer.
    pub(crate) adaptive: crate::layout::adaptive::AdaptiveContext,
    /// State IDs used by this Composer's animation registrations.
    pub(crate) animation_state_ids: HashSet<StateId>,

    #[cfg(test)]
    pub(crate) compose_clean_count: usize,
    #[cfg(test)]
    pub(crate) compose_dirty_count: usize,
    /// Test-only: how many RESTARTABLE GROUPS actually re-entered this pass (dirty OR parameter
    /// change). Unlike `compose_dirty_count`, this counts a parameter-driven Enter of a Clean slot -
    /// the probe a "no recomposition" test needs.
    #[cfg(test)]
    pub(crate) compose_entered_count: usize,
    /// 重组总次数（vsync 研究——单次渲染内多次 compose 的观测）
    pub(crate) compose_count: u64,
    /// Shared-element flights (Phase 2): active flights by id (Tier-0
    /// coordinator lives in components::shared_transition as `impl Composer`).
    pub(crate) shared_flights: HashMap<FlightId, ActiveFlight>,
    pub(crate) next_flight_id: u64,
    /// Last frame's (scope, key) → slot map for switch detection.
    pub(crate) prev_shared_endpoints: HashMap<(u64, String), u64>,
    /// Detached retained source roots (absolute coords, rendered after main tree).
    pub(crate) transition_layer: Vec<usize>,
    /// Per-frame render order for the transition layer: detached sources plus
    /// every elevated (in-tree) flight endpoint, z-sorted back-to-front.
    /// Rebuilt by `rebuild_layer_order` after each flight poll — render and
    /// hit testing both read it, so paint order and hit order can never drift.
    pub(crate) layer_order: Vec<usize>,
    /// In-tree endpoints that fly in this composer's layer (elevated visuals),
    /// recorded by the visual writers themselves. Rebuilt every poll: the
    /// writers are the only place that knows both the arena index and whether
    /// that end is elevated, and a Tier1 peer's flight lives in the MAIN
    /// composer's map, so the peer could not discover it by scanning its own.
    pub(crate) elevated_roots: Vec<usize>,
    /// Non-shared subtrees elevated for the duration of a flight (Compose
    /// `renderInSharedTransitionScopeOverlay`). Recomputed each poll by
    /// `refresh_scope_overlay_roots`; also drives their `LayoutNode` flag so
    /// the tree walk skips them and the layer re-draws them untransformed.
    pub(crate) scope_overlay_roots: Vec<usize>,
    /// True while any node carries a paint disposition other than `InTree` (a placeholder, or chrome in
    /// the layer). `refresh_paint_dispositions` can then skip its whole-arena walk on the frames where
    /// nothing is shared and the previous frame left nothing to clear — and it MUST run once more after
    /// the last non-`InTree` assignment, which is what this flag guarantees.
    pub(crate) paint_dirty: bool,
    /// Last-frame absolute bounds per live marked endpoint (Phase 3
    /// same-screen size-morph detection). Keyed by endpoint identity
    /// (scope, key) — never by slot: slots are positional identities a new
    /// key can reuse, which would seed a spurious morph from stale rects.
    pub(crate) shared_last_bounds: HashMap<(u64, String), SharedBounds>,
    /// Stable per-Composer identity for cross-composer flight matching
    /// (Phase 4 Tier1: main tree ↔ overlays share (scope, key) but never slots).
    pub(crate) composer_id: u64,
    /// Retired-detached sources awaiting cross-composer counterparts (Phase 4
    /// Tier1). Drained by cross-poll same frame — freed when unmatched.
    pub(crate) pending_cross: Vec<PendingSource>,
    /// Fresh (scope, key) → slot appearances THIS frame (Phase 4 Tier1 cross
    /// matching reads these). Overwritten every retain — peer prev maps absorb
    /// appearances before cross-poll runs, so this is the only freshness source.
    pub(crate) fresh_shared: Vec<((u64, String), u64)>,
    /// This composer's canvas-frame origin in window-logical units (Phase 4):
    /// main tree renders untranslated (0,0); overlays render translated by
    /// their screen_pos. Flight bounds are canonicalized to window coords.
    pub(crate) screen_origin: (f32, f32),
}

impl Composer {
/// 选区注册表（由 SelectionContainer 在 compose 时注入，供事件处理访问）

    /// This composer's process-unique identity; see the `uid` field.
    pub(crate) fn uid(&self) -> u64 {
        self.uid
    }

    pub(crate) fn take_layout_override_fresh(&mut self) -> bool {
        std::mem::take(&mut self.layout_override_fresh)
    }

    pub fn new() -> Self {
        let pending_states = crate::runtime::state::ComposerSubscription::new();
        Self {
            uid: NEXT_COMPOSER_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            saveable: None,
            slot_table: SlotTable::new(),
            current_group_key: 0,
            compose_transaction: None,
            layout_override_fresh: false,
            path_counters: crate::layout::node::SlotKeyMap::default(),
            remember_path_counters: crate::layout::node::SlotKeyMap::default(),
            scope_source_stack: Vec::new(),
            key_override_stack: Vec::new(),
            movable_saved_scopes: Vec::new(),
            movable_saved_stmts: Vec::new(),
            movable_saved_keys: Vec::new(),
            movable_saved_groups: Vec::new(),
            movable_saved_active: 0,
            movable_saved_skip_depth: 0,
            movable_saved_group_key: 0,
            movable_saved_path_counters: crate::layout::node::SlotKeyMap::default(),
            movable_saved_remember_counters: crate::layout::node::SlotKeyMap::default(),
            movable_saved_params: Vec::new(),
            movable_saved_pending_next: 0,
            pending_recomposition: VecDeque::new(),
            needs_recomposition: true,
            arena: crate::layout::node::NodeArena::new(),
            node_stack: Vec::new(),
            group_skip_stack: Vec::new(),
            overlays: Vec::new(),
            post_compose: Vec::new(),
            overlay_active: HashMap::new(),
            slot_deps: HashMap::new(),
            compose_slot_reads: crate::layout::node::SlotKeyMap::default(),
            signal_handles: HashMap::new(),
            layout_signal_handles: HashMap::new(),
            entered_compose_keys: crate::layout::node::SlotKeySet::default(),
            layout_slot_reads: crate::layout::node::SlotKeyMap::default(),
            layout_deps: HashMap::new(),
            layout_dirty_keys: HashSet::new(),
            removed_slot_keys: HashSet::new(),
            pending_states,
            pending_params: Vec::new(),
            pending_next: 0,
            prev_node_by_key: crate::layout::node::SlotKeyMap::default(),
            reused_nodes: crate::layout::node::NodeMarks::default(),
            subcompositions: Vec::new(),
            subcomposition_cache: None,
            #[cfg(test)]
            skip_claims: 0,
            #[cfg(test)]
            skip_claim_bails: 0,
            #[cfg(test)]
            live_residue: 0,
            selection_registrar: None,
            lifecycle: crate::runtime::lifecycle::LifecycleState::default(),
            window_id: 0,
            adaptive: crate::layout::adaptive::AdaptiveContext::new(),
            animation_state_ids: HashSet::new(),
            shared_flights: HashMap::new(),
            next_flight_id: 1,
            prev_shared_endpoints: HashMap::new(),
            transition_layer: Vec::new(),
            layer_order: Vec::new(),
            elevated_roots: Vec::new(),
            scope_overlay_roots: Vec::new(),
            paint_dirty: false,
            shared_last_bounds: HashMap::new(),
            composer_id: NEXT_COMPOSER_ID.fetch_add(1, Ordering::Relaxed),
            pending_cross: Vec::new(),
            fresh_shared: Vec::new(),
            screen_origin: (0.0, 0.0),
            #[cfg(test)]
            compose_clean_count: 0,
            #[cfg(test)]
            compose_dirty_count: 0,
            #[cfg(test)]
            compose_entered_count: 0,
            compose_count: 0,
        }
    }

    /// 获取当前正在构建的节点 ID（node_stack 栈顶）
    pub fn current_node_id(&self) -> Option<u64> {
        self.node_stack.last().map(|&idx| self.arena.nodes[idx].id)
    }

    /// Bind this composer to the window whose frames should drive `ComposeCtx::frame_clock`.
    pub(crate) fn set_window_id(&mut self, window: u64) {
        self.window_id = window;
    }

    pub fn window_id(&self) -> u64 {
        self.window_id
    }

    pub(crate) fn pending_window_close_id(&self) -> Option<u64> {
        self.lifecycle.pending_close_id()
    }

    pub(crate) fn reset_pending_window_remove(&self) {
        self.lifecycle.reset_pending_remove();
    }

    pub(crate) fn set_adaptive_window_size(&self, width: f32, height: f32) {
        self.adaptive.set_size(width, height);
    }

    pub(crate) fn focus_window(&self, window_id: u64) -> crate::modifier::FocusWindowGuard {
        crate::modifier::focus_window(window_id)
    }

    pub(crate) fn take_focus_requests(&self) -> Vec<u64> {
        crate::modifier::take_focus_requests()
    }

    /// 分配下一个 group key。
    ///
    /// 基于 slot 路径编码：结构稳定——Enter/Skip 的执行顺序不影响 key，
    /// 保证同一组合位置跨重组得到相同 slot（否则 slot 树 truncate 重建，
    /// 导致 remember 的 State 全部丢失重建）。
    /// 能否获得稳定 key 的 base（新 key 系统核心判定）：
    /// - 显式 ctx.key(id, f) 作用域内 → 稳定（用户保证唯一）
    /// - STMT_STACK 有宏注入的语句（`#[composable]`/keyed_stmt! 展开）→ 稳定
    ///   （base = fnv(scope_src, 语句id, 迭代seq)——编译期固定）
    /// - 都不是 → None（调用方 panic 兜底——fail-fast，不静默降级）
    /// Namespace compiler-derived and explicit identities by their movable placement.
    pub(super) fn namespace_key_base(&self, base: u64) -> u64 {
        match self.slot_table.movable_composing {
            Some(id) => mix_key(base, mix_key(0x6d6f_7661_626c_6500, id)),
            None => base,
        }
    }

    pub(crate) fn try_stable_base(&self) -> Option<u64> {
        if let Some(&k) = self.key_override_stack.last() {
            // ctx.key(id) 语义：显式 id 替代位置——但**同一 id 跨调用点**（多
            // 字段同 role：16 个 field 的 label 都是 TextFieldSlotRole::Label）
            // 会碰撞——混合调用链隔离实例（同 id 不同调用点 → 不同 base）；
            // 链空（非宏顶层）时退化纯 id（兼容旧行为）。
            // ⚠ 只混合 scope_src+sid（不含迭代 seq）：ctx.key 是列表/重排的
            // 显式兜底（Compose 语义）——若混入 seq，重排后同 id 位置变 →
            // base 变 → key 漂移 → 兜底失效（P1-1，探针证实）。多实例区分
            // 由 per-base counter（path_counters/remember_path_counters）完成。
            let chain = self.chain_hash_no_seq().unwrap_or(0);
            let mut h: u64 = 0xcbf29ce484222325;
            h ^= k; h = h.wrapping_mul(0x100000001b3);
            h ^= chain; h = h.wrapping_mul(0x100000001b3);
            return Some(self.namespace_key_base(h));
        }
        self.chain_hash().map(|base| self.namespace_key_base(base)).or_else(|| {
            // Raw payload closures have no compiler statement on entry. The placement itself is a
            // stable key source; repeated children are distinguished by its local counters.
            self.slot_table.movable_composing.map(|_| self.namespace_key_base(0))
        })
    }

    /// 调用链哈希（不含迭代 seq）：fnv(scope_src, STMT栈顶语句id)——编译期
    /// 固定的组合位置身份。ctx.key(id) 混合用——同 id 跨位置（列表重排）base
    /// 不变（兜底稳定），不同调用点（16 字段）靠 scope_src/sid 隔离。
    fn chain_hash_no_seq(&self) -> Option<u64> {
        STMT_STACK.with(|s| {
            let s = s.borrow();
            s.last().map(|&(sid, _seq)| {
                let scope_src = self.scope_source_stack.last().and_then(|s| *s).unwrap_or(0);
                let mut h: u64 = 0xcbf29ce484222325;
                h ^= scope_src; h = h.wrapping_mul(0x100000001b3);
                h ^= sid as u64; h = h.wrapping_mul(0x100000001b3);
                h
            })
        })
    }

    /// 调用链哈希：fnv(scope_src, STMT栈顶语句id, 迭代seq)——编译期固定
    /// 的组合位置（`#[composable]`/keyed_stmt! 注入的语句）
    fn chain_hash(&self) -> Option<u64> {
        STMT_STACK.with(|s| {
            let s = s.borrow();
            s.last().map(|&(sid, seq)| {
                let scope_src = self.scope_source_stack.last().and_then(|s| *s).unwrap_or(0);
                let mut h: u64 = 0xcbf29ce484222325;
                h ^= scope_src; h = h.wrapping_mul(0x100000001b3);
                h ^= sid as u64; h = h.wrapping_mul(0x100000001b3);
                h ^= (seq as u64).wrapping_mul(0x9E3779B97F4A7C15);
                h
            })
        })
    }

    /// 没有稳定 key 源时的 panic（fail-fast——不静默降级为路径哈希）
    pub(crate) fn panic_no_stable_key(&self, api: &str) -> ! {
        panic!(
            "无法获得稳定 key（{}）：调用点不在 `#[composable]`/keyed_stmt! 注入内，\
             也无 ctx.key() 包裹——key 会在结构变化时漂移。修复：①将调用点放入 \
             `#[composable]` 函数内 ②用 ctx.key() 包裹 ③content 闭包参数名与 \
             #[composable(x)] 指定的标识符一致",
            api
        )
    }

    pub fn next_group_key(&mut self) -> u64 {
        // key 基优先级：显式 ctx.key() > `#[composable]` 语句 id（源码位置）。
        // 语句 id 由宏注入（编译期按源码结构固定编号）——结构变化（前面插入/移除兄弟
        // 节点）不影响语句 id → key 不漂移 → remember/复用稳定（对标 Compose 编译器
        // 的调用点 key）。无稳定源 → panic（fail-fast）。
        let base = match self.try_stable_base() {
            Some(b) => b,
            None if cfg!(test) => {
                // 测试路径：无语句级 key 时退化为路径哈希（测试自控结构——漂移由
                // 测试自己负责；生产代码禁止——见下方 panic）
                let path = self.slot_table.current_path().to_vec();
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in &path {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            None => self.panic_no_stable_key("next_key"),
        };
        // 每路径独立 counter：同 key 基第 N 次调用跨帧恒定（Skip 的 content 不执行
        // 不平移——节点复用错位 + 常量折叠冻结的防护）
        let counter = self.path_counters.entry(base).or_insert(1);
        let c = *counter;
        *counter += 1;
        // key = fnv(base, 序号)——全 64 位混合，不丢身份熵（拼接方案把身份
        // 截到 32 位，2^32 碰撞空间——dup-key 根因）
        crate::runtime::composer::mix_key(base, c as u64)
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——组合代码重跑的失效单位）。
    /// 返回 scope key；`State::get()` 在 scope 内（组件外）注册依赖到 scope。
    ///
    /// ⚠ 手动调用（无源码哈希）：scope_source_stack push None，与 start_scope_keyed
    /// 的 Some 区分——end_scope 严格配对，不破坏外层宏注入的 source。
    /// ⚠ release 下宏外调用会触发稳定 key panic（next_group_key 快速失败）——
    /// 生产代码应使用 `#[composable]`/`app_root!` 注入的 start_scope_keyed。
    pub fn start_scope(&mut self) -> u64 {
        self.scope_source_stack.push(None);
        let key = self.next_group_key();
        self.slot_table.start_scope(key);
        self.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// 结束组合 scope
    pub fn end_scope(&mut self) {
        self.slot_table.end_scope();
        // 防御性配对：非空才 pop（start_scope/end_scope 不配对时避免 panic/污染其他 scope）
        GROUP_STACK.with(|s| {
            let mut s = s.borrow_mut();
            if !s.is_empty() { s.pop(); }
        });
        if !self.scope_source_stack.is_empty() { self.scope_source_stack.pop(); }
    }

    /// 物化：组合树（Slot desc）→ 布局树（arena LayoutNode）——完整分离的核心。
    /// 由 compose 末尾调用（layout 只测量）。实现拆到 core/materialize.rs（SRP）。
    pub fn materialize(&mut self) {
        crate::runtime::materialize::materialize(self);
    }

    /// Hands this frame's declared parameters to the slot being started, and takes the slot's previous
    /// parameters back as the buffer for the next declarations.
    ///
    /// A no-op when nothing was declared (`pending_next == 0`): the slot keeps its parameters, which is
    /// what stops a replayed stub from clearing them — a Skipped parent's children must not be forced to
    /// Enter just because their params were not re-declared.
    ///
    /// The exchange is the whole point. Before it, `changed` pushed a fresh `Box` per parameter per
    /// frame and `set_current_params` dropped the slot's previous vector — so a 800-row list allocated
    /// 800 boxes a frame (~503 µs of a cold frame's content closure, `docs/benchmarks.md`). Now the
    /// previous vector comes back and the next `changed` overwrites its boxes in place, which in a list
    /// means the same few allocations circulate down the rows.
    fn commit_pending_params(&mut self) {
        if self.pending_next == 0 {
            return;
        }
        // Hand over exactly this frame's declarations; any tail is last frame's leftovers (a group that
        // declared more parameters than this one does now). Dropping them here is what keeps the count
        // honest for a group whose parameter list shrank.
        self.pending_params.truncate(self.pending_next);
        let incoming = std::mem::take(&mut self.pending_params);
        self.pending_params = self.slot_table.replace_current_params(incoming);
        self.pending_next = 0;
    }

    /// 在组合树中开始一个节点（由组件的 build 方法调用）
    pub fn start_node(&mut self, key: u64, modifier: Modifier, policy: Option<Box<dyn MeasurePolicy>>, on_remove: Option<Box<dyn FnOnce() + Send>>) {
        self.current_group_key = key as u32;
        self.entered_compose_keys.insert(key);
        let slot_status = self.slot_table.start_slot(key);
        // 普通节点：复用 scope slot 时重置为普通（同路径类型切换场景）
        self.slot_table.set_current_scope(false);
        // 写入 `ComposeCtx::changed` 暂存的参数（供下帧比较）——
        // 仅当本帧有 changed 声明；否则保留上帧 params：
        // replay 的 stub start_node（pending 空）不清空子 slot params，
        // 避免父 Skip 后子组件参数未变也被强制 Enter
        self.commit_pending_params();
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // 组合产物写入 Slot（物化阶段消费——完整分离：arena 建节点移出组合阶段）
        // ⚠ direction 必须在此捕获（组合期 provides 作用域内）——物化期
        // 主题的 provider 已退出作用域读不到（RTL 全局切换失效根因）
        let direction = modifier.get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        // A Clean slot still has to re-measure when this node's own modifier changed: the modifier is
        // where a container's or leaf's layout inputs live (`max_width`, `size`, `padding`), and a value
        // computed from a state read in an ANCESTOR's scope changes them without dirtying this slot.
        // See `node_modifier_changed`.
        let dirty = slot_status != SlotStatus::Clean || self.node_modifier_changed(key, &modifier);
        self.slot_table.set_current_desc(Some(NodeDesc {
            key,
            modifier,
            policy,
            on_remove,
            dirty, // 重测标记（slot.dirty 已消费）
            registrar: None,
            focus_color: None,
            composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
            direction,
        }));
        // 统一依赖栈：节点 push（组件 build 期间 State 读取注册到最内层 Group——
        // 组件内读取失效目标 = 本节点（对标 Compose 最内层 Group 语义））
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
    }

    /// 结束当前节点：组合侧出栈（物化建树由 materialize 统一处理）
    pub fn end_node(&mut self) {
        self.slot_table.end_slot();
        GROUP_STACK.with(|s| { s.borrow_mut().pop(); }); // 与 start_node 的 push 配对
    }

    /// 开始可重启分组（内部调用 start_node + 返回 skip/enter 状态）
    pub(super) fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: Option<Box<dyn MeasurePolicy>>,
        on_remove: Option<Box<dyn FnOnce() + Send>>,
    ) -> GroupStatus {
        self.current_group_key = key as u32;
        // 容器 build 期间维护（scope 栈由 start_restartable_group 管理）
        let slot_status = self.slot_table.start_slot(key);
        self.slot_table.set_current_scope(false);
        // 容器组件 = scope（对标 Compose RestartGroup）：push 组件 scope——
        // 组件内/ content 闭包内读取注册到本组件（最内层 scope）
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // Clean slot：从缓存恢复（阶段5：加参数相等条件——slot clean 且参数
        // 全相等才 Skip；参数变化（changed 比较）时即使 slot clean 也 Enter）
        let is_skip = if slot_status == SlotStatus::Clean {
            // 与上帧 slot.params 比较（pending_params 前 pending_next 项 = 本帧 changed 暂存；
            // slot.params 此刻仍是上帧的——本帧写入在其后。只比前 pending_next 项：那个 vector 现在
            // 承载着上一帧的（可能更长/更短的）参数表，是复用缓冲而不是本帧声明）
            let params_unchanged = params_equal(
                &self.pending_params[..self.pending_next],
                &self.slot_table.current_slot().params,
            ) && self.container_modifier_unchanged(key, &modifier);
            #[cfg(debug_assertions)] {
                if std::env::var("WINIA_SKIP_TRACE").is_ok() {
                    eprintln!("[skip] key={} clean={} params_u={} prev={} pending_len={}",
                        key >> 32, slot_status == SlotStatus::Clean, params_unchanged,
                        self.prev_node_by_key.contains_key(&key), self.pending_next);
                }
            }
            // Nothing else to ask: `container_modifier_unchanged` already failed for a key that is not
            // in the reuse index, and a key with no node is one materialize could not restore — the
            // same condition the old cache map was consulted for here.
            params_unchanged
        } else {
            false
        };
        // Enter 重新执行 content；Skip 保留上帧子树的读取集合。
        if !is_skip {
            self.entered_compose_keys.insert(key);
            #[cfg(test)] { self.compose_entered_count += 1; }
        }
        // 写入本帧参数（在 is_skip 比较之后——比较用上帧 slot.params）
        // 仅当 pending 非空（有 changed 声明）；空则保留上帧 params（replay stub 场景）
        self.commit_pending_params();

        // 组合产物写入 Slot：Enter 写完整描述（物化消费）；Skip 写 None——
        // content 不执行（无新描述），物化时按 key 恢复缓存节点（skip 标记）
        if is_skip {
            // 方向先算（modifier 随后 move 进 set_skip_modifier）
            let direction = modifier.get_layout_direction()
                .unwrap_or(crate::layout::direction::current());
            self.slot_table.set_current_desc(None);
            // 保留本帧组合产物 modifier（父层重跑传入的新 offset/背景——物化应用）
            self.slot_table.set_skip_modifier(modifier);
            // 保留 policy（外层传入——Skip 时 content 未执行但 policy 可用；
            // 物化恢复失败降级时避免 policy 缺失测量 0 尺寸）
            self.slot_table.set_skip_policy(policy);
            // Skip 容器方向也须组合期捕获（同 Enter——物化期读不到 theme）
            self.slot_table.set_skip_direction(direction);
        } else {
            // Enter：组合期捕获方向（provides 作用域内）——先算再 move
            let direction = modifier.get_layout_direction()
                .unwrap_or(crate::layout::direction::current());
        self.slot_table.set_current_desc(Some(NodeDesc {

                key,
                modifier,
                policy,
                on_remove,
                dirty: true, // Enter 即重测（content 重跑——参数/内容可能变；Skip 恢复不受影响）
                registrar: None,
                focus_color: None,
                composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
                composing_range: None,
                direction,
            }));
        }
        self.group_skip_stack.push(is_skip);

        if is_skip {
            GroupStatus::Skip
        } else {
            GroupStatus::Enter
        }
    }

    /// 结束可重启分组：
    /// - Skip 模式：重放子 slot 结构并创建 stub LayoutNode
    /// - Enter 模式：等同于 end_node()
    pub(super) fn end_restartable_group(&mut self) {
        let was_skip = self.group_skip_stack.pop().unwrap_or(false);
        // Enter（content 重跑）：清理未访问子槽——内容结构变化后残留的旧槽
        // （如 AnimatedContent B(3 文本) → A(2 文本) 的第 3 槽）若保留，下帧
        // Skip 收集时结构签名（desc children vs 缓存 children）不等 → 物化
        // 降级 0x0 → 子树塌缩。Skip 槽保留（content 未执行——结构需保留供恢复）。
        //
        // The same pruning applies inside a movable placement. Moving a surviving subtree preserves
        // its state; omitting a conditional/list child removes it, and reinsertion initializes anew.
        if !was_skip {
            // Pruning is a DROP, so it has to collect lifecycle callbacks the same way the other
            // removal sites do. `retain` alone silently discarded them: a `LaunchedEffect` inside a
            // branch that stopped composing kept its task running (caught by
            // `animation::test_infinite_transition_auto_dispose`).
            let slot = self.slot_table.current_slot();
            let mut orphaned = Vec::new();
            let mut kept = Vec::with_capacity(slot.children.len());
            for child in slot.children.drain(..) {
                if child.visited {
                    kept.push(child);
                } else {
                    SlotTable::collect_forgets(child, &mut orphaned);
                }
            }
            slot.children = kept;
            for f in orphaned { f(); }
        }
        // Skip：content 未执行——slot 树保留（上帧 children 结构）——物化时
        // 整棵子树按 key 从 prev_node_by_key 恢复（stub 机制已由物化替代）
        // GROUP_STACK pop 由 end_node 统一处理（与 start_restartable_group 的
        // push 配对——此前此处额外 pop 导致容器组件两次 pop 一次 push →
        // 栈错乱 → 后续依赖注册到错误 Group → State 变化不标记容器 dirty）
        self.end_node();
    }

    /// Whether this container's modifier matches the one the frame is running against — the second half
    /// of the Skip decision (`params_equal` is the first).
    ///
    /// The comparison target is the ARENA NODE's modifier, which during compose still holds what
    /// materialize applied last frame. That is the same value the slot used to keep a copy of
    /// (`Slot::prev_modifier`), and reading it instead removes a `Modifier` clone per Entering container
    /// per frame — the largest single item the content-closure probe found (~297 µs on a cold frame,
    /// ~93 ns per container, and a `Modifier` owns a `Vec` plus any strings its elements carry;
    /// `docs/benchmarks.md`).
    ///
    /// It is the same value, not an approximation, which is why this is preferred over storing a digest:
    /// a digest can only ever answer "unchanged" wrongly, and "unchanged" here means the container Skips
    /// with a stale modifier — the one failure direction this code must not have.
    ///
    /// A missing node means "no basis for comparison" and the container Enters, which is what a missing
    /// `prev_modifier` used to do. `compose` refills the index when it is empty (a second compose in the
    /// same frame drains it), so the node is found in every case where a Skip was possible before.
    fn container_modifier_unchanged(&self, key: u64, modifier: &Modifier) -> bool {
        match self.prev_node_by_key.get(&key) {
            Some(&idx) => modifier.param_eq(&self.arena.nodes[idx].modifier),
            None => false,
        }
    }

    /// Whether this frame's `modifier` differs from the one the node at `key` was last materialized with
    /// — i.e. the node's own LAYOUT INPUTS changed, so its measurement cannot be reused.
    ///
    /// A container or leaf created by `start_node` has no parameter list of its own; its modifier IS its
    /// parameter list. A value computed from a state read in an ANCESTOR's scope (`max_width(cap.get())`
    /// inside a parent's content closure) dirties that ancestor's slot and leaves this node's slot Clean,
    /// so `slot_status != SlotStatus::Clean` alone answers "nothing changed here" while this node's own
    /// constraints did change — and a folded measurement then keeps the old size and, for a subcomposing
    /// node, the content it composed with the old constraints (measured: the `bwc` UI fixture's click
    /// stopped reaching its `BoxWithConstraints` content). Compose compares the modifier chain for the
    /// same reason.
    ///
    /// `false` when there is no node to compare against: that is not "changed", it is "no basis" —
    /// materialize rebuilds a node it cannot find and starts it dirty on its own.
    fn node_modifier_changed(&self, key: u64, modifier: &Modifier) -> bool {
        match self.prev_node_by_key.get(&key) {
            Some(&idx) => !modifier.param_eq(&self.arena.nodes[idx].modifier),
            None => false,
        }
    }

    /// Rebuild reverse compose dependencies from the forward per-slot read graph.
    /// Keeping one canonical source prevents stale reverse edges after slot removal.
    pub(super) fn rebuild_compose_reverse_deps(&mut self) {
        self.slot_deps.clear();
        for (&slot_key, state_ids) in &self.compose_slot_reads {
            for &state_id in state_ids {
                self.slot_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    /// Rebuild reverse layout dependencies from the forward per-slot read graph.
    pub(super) fn rebuild_layout_reverse_deps(&mut self) {
        self.layout_deps.clear();
        for (&slot_key, state_ids) in &self.layout_slot_reads {
            for &state_id in state_ids {
                self.layout_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_dependency_graphs(&self) {
        let mut expected_compose = HashMap::<StateId, HashSet<u64>>::new();
        for (&slot_key, state_ids) in &self.compose_slot_reads {
            for &state_id in state_ids {
                expected_compose.entry(state_id).or_default().insert(slot_key);
            }
        }
        debug_assert_eq!(self.slot_deps, expected_compose, "compose dependency reverse graph drifted");

        let mut expected_layout = HashMap::<StateId, HashSet<u64>>::new();
        for (&slot_key, state_ids) in &self.layout_slot_reads {
            for &state_id in state_ids {
                expected_layout.entry(state_id).or_default().insert(slot_key);
            }
        }
        debug_assert_eq!(self.layout_deps, expected_layout, "layout dependency reverse graph drifted");
    }

    #[cfg(not(debug_assertions))]
    #[inline]
    pub(super) fn debug_assert_dependency_graphs(&self) {}

    fn reconcile_compose_deps(
        &mut self,
        recorded: Vec<(Arc<StateSignal>, u64)>,
        live_keys: &crate::layout::node::SlotKeySet,
    ) {
        let mut reads_by_slot: crate::layout::node::SlotKeyMap<HashSet<StateId>> = crate::layout::node::SlotKeyMap::default();
        let mut current_signals: HashMap<StateId, Arc<StateSignal>> = HashMap::new();
        // Reads recorded against a slot that is not live: dropped from the graph, but the signal was
        // already subscribed to (the read happened), so the subscription is dirty even when the graph
        // is not — see `changed` below.
        let mut dropped_reads = 0usize;
        for (signal, slot_key) in recorded {
            current_signals.entry(signal.id()).or_insert_with(|| signal.clone());
            if live_keys.contains(&slot_key) {
                reads_by_slot.entry(slot_key).or_default().insert(signal.id());
            } else {
                dropped_reads += 1;
            }
        }

        // A skipped subtree keeps its previous read set; an Entered slot gets
        // the exact set observed in this frame, including an empty set.
        //
        // `changed` records whether that actually moved anything, because the reverse graph below is a
        // pure function of the forward one and rebuilding it is the expensive half: a whole-tree walk
        // that clears and re-creates a set per state. It ran every frame — including the frames where
        // every read set is exactly what it already was, which is the common case (measured at 800
        // rows: 110 µs of an idle frame's compose, and the same figure on the frame where one row's
        // state moved, since the container re-records the same 800 reads).
        let mut changed = false;
        let before = self.compose_slot_reads.len();
        self.compose_slot_reads.retain(|key, _| live_keys.contains(key));
        changed |= self.compose_slot_reads.len() != before;
        // TAKEN rather than cloned: this is the last reader of the set in the frame (the next compose
        // clears it), and on a cold frame it holds every entering key — 4000 of them. A panic after
        // this point restores the set from the frame's runtime snapshot (`capture_compose_runtime`),
        // so the take is not observable to a rollback either.
        let entered = std::mem::take(&mut self.entered_compose_keys);
        for key in entered {
            let new_set = match reads_by_slot.remove(&key) {
                Some(reads) if !reads.is_empty() => Some(reads),
                _ => None,
            };
            let same = match (self.compose_slot_reads.get(&key), new_set.as_ref()) {
                (None, None) => true,
                (Some(old), Some(new)) => old == new,
                _ => false,
            };
            changed |= !same;
            match new_set {
                Some(reads) => {
                    self.compose_slot_reads.insert(key, reads);
                }
                None => {
                    self.compose_slot_reads.remove(&key);
                }
            }
        }
        // Be tolerant of a read recorded by a node whose entry marker was not
        // reached (for example, a modifier callback during materialization).
        if !reads_by_slot.is_empty() {
            changed = true;
            for (key, reads) in reads_by_slot {
                self.compose_slot_reads.entry(key).or_default().extend(reads);
            }
        }
        // A read against a dead slot means the subscription may hold a signal the graph does not, so
        // the cleanup has to run even though the graph is unchanged.
        changed |= dropped_reads > 0;

        if changed {
            self.rebuild_compose_reverse_deps();
            self.cleanup_signal_subscriptions();
        }
        self.signal_handles.extend(current_signals);
        self.debug_assert_dependency_graphs();
    }

    fn cleanup_signal_subscriptions(&mut self) {
        let mut live_ids = HashSet::new();
        for state_ids in self.compose_slot_reads.values() {
            live_ids.extend(state_ids.iter().copied());
        }
        live_ids.extend(self.layout_deps.keys().copied());

        let mut removed = HashMap::<StateId, Arc<StateSignal>>::new();
        let mut compose_handles = std::mem::take(&mut self.signal_handles);
        compose_handles.retain(|id, signal| {
            if live_ids.contains(id) {
                true
            } else {
                removed.insert(*id, signal.clone());
                false
            }
        });
        self.signal_handles = compose_handles;

        let mut layout_handles = std::mem::take(&mut self.layout_signal_handles);
        layout_handles.retain(|id, signal| {
            if live_ids.contains(id) {
                true
            } else {
                removed.entry(*id).or_insert_with(|| signal.clone());
                false
            }
        });
        self.layout_signal_handles = layout_handles;

        for signal in removed.values() {
            signal.unsubscribe(self.pending_states.id());
        }
        self.pending_states.retain_signals(&live_ids);
    }

    pub(super) fn capture_compose_runtime(&self) -> ComposeRuntimeSnapshot {
        ComposeRuntimeSnapshot {
            slot_table: self.slot_table.runtime_snapshot(),
            current_group_key: self.current_group_key,
            path_counters: self.path_counters.clone(),
            remember_path_counters: self.remember_path_counters.clone(),
            scope_source_stack: self.scope_source_stack.clone(),
            key_override_stack: self.key_override_stack.clone(),
            pending_recomposition: self.pending_recomposition.clone(),
            needs_recomposition: self.needs_recomposition,
            node_stack: self.node_stack.clone(),
            group_skip_stack: self.group_skip_stack.clone(),
            overlay_active: self.overlay_active.clone(),
            entered_compose_keys: self.entered_compose_keys.clone(),
            reused_nodes: self.reused_nodes.clone(),
        }
    }

    pub(super) fn rollback_compose_runtime(&mut self) {
        let Some(snapshot) = self.compose_transaction.take() else { return };
        self.slot_table.restore_runtime(snapshot.slot_table);
        self.current_group_key = snapshot.current_group_key;
        self.path_counters = snapshot.path_counters;
        self.remember_path_counters = snapshot.remember_path_counters;
        self.scope_source_stack = snapshot.scope_source_stack;
        self.key_override_stack = snapshot.key_override_stack;
        self.pending_recomposition = snapshot.pending_recomposition;
        self.needs_recomposition = snapshot.needs_recomposition;
        self.node_stack = snapshot.node_stack;
        self.group_skip_stack = snapshot.group_skip_stack;
        self.overlay_active = snapshot.overlay_active;
        self.entered_compose_keys = snapshot.entered_compose_keys;
        self.reused_nodes = snapshot.reused_nodes;
    }

    /// 执行组合：运行 content 闭包，构建/更新组合树和布局树。
    pub fn compose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        // The window's saveable registry is provided for EVERY pass, not just the first: a slot that
        // is re-entered has to be able to register again.
        match self.saveable.clone() {
            Some(registry) => {
                crate::runtime::saveable::with_registry_of(&registry, || self.compose_inner(content))
            }
            None => self.compose_inner(content),
        }
    }

    /// The registry this composer's window saves into, if it has one.
    pub(crate) fn saveable_registry(
        &self,
    ) -> Option<std::sync::Arc<crate::runtime::saveable::SaveableStateRegistry>> {
        self.saveable.clone()
    }

    /// Attach the registry this composer's window saves into and restores from.
    pub(crate) fn set_saveable_registry(
        &mut self,
        registry: std::sync::Arc<crate::runtime::saveable::SaveableStateRegistry>,
    ) {
        self.saveable = Some(registry);
    }

    fn compose_inner(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        // Tag this pass for the torn-read instrument: a read that observes another thread's write
        // from after this point is a pass split across two worlds.
        let _pass = crate::runtime::state::begin_pass();
        // Isolate shared TLS so a nested Composer can compose without replacing
        // the caller's active slot/group/statement context.
        let _runtime_frame = begin_runtime_frame();
        let mut compose_runtime_transaction = ComposeRuntimeTransaction::new(self);
        let _adaptive_context = crate::layout::adaptive::enter_context(self.adaptive.clone());
        let mut dependency_transaction = ComposeDependencyTransaction::new(self);
        #[cfg(test)] { self.compose_clean_count = 0; self.compose_dirty_count = 0; self.compose_entered_count = 0; }
        self.compose_count += 1;
        // Movable content is composed once per composition, so the table needs to know which
        // composition this is.
        self.slot_table.frame_no = self.compose_count;
        self.slot_table.reset();
        self.overlay_active.clear(); // 每帧组合期重记录（Skip 帧不记录）
        // A pass that panicked before its drain leaves its callbacks behind; they belong to a
        // composition that never committed, so they must not run in this one.
        self.post_compose.clear();
        self.current_group_key = 0;
        self.lifecycle.reset_for_compose();
        self.path_counters.clear();
        self.remember_path_counters.clear();
        STMT_STACK.with(|s| s.borrow_mut().clear());
        // The runtime frame starts empty; these clears also self-heal any state
        // left by an older caller that entered before frame isolation was added.
        GROUP_STACK.with(|s| s.borrow_mut().clear());
        ACTIVE_SLOT_KEY.with(|c| c.set(0));
        self.scope_source_stack.clear();
        self.key_override_stack.clear();
        // 注意：不在 compose 开头清 arena.root——materialize 管理 root
        // （开头清 + 末尾设）。此处若清，同帧第二次 compose 的 materialize
        // 守卫（prev 空 + root 有）失效 → 空 prev 重建 → 树塌缩。
        // Reset only this Composer's Window lifecycle state before old node
        // cleanup; its on_remove callbacks may set a pending close request.
        // 阶段D：保留上帧树（prev_node_by_key 由上帧 layout 构建）——
        // start_node 按 slot_key 复用节点槽位；本帧未复用的旧节点在
        // compose 末尾统一 free（见下方 drain）
        self.node_stack.clear();
        self.group_skip_stack.clear();
        // The parameter buffer is NOT cleared: it arrives holding a slot's previous parameters, which is
        // exactly the storage the next `changed` wants to overwrite in place. Only the write cursor
        // resets (`commit_pending_params` is what hands the buffer over, and it counts from the cursor).
        self.pending_next = 0;
        self.entered_compose_keys.clear();
        // Slot key 0 is the fallback target for top-level State::get().
        self.entered_compose_keys.insert(0);

        // Consume only invalidations that currently have compose dependencies.
        // Layout-only IDs stay queued for layout() to consume.
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        let mut pending_batch = PendingBatchGuard::new(
            self.pending_states.clone(),
            self.pending_states.drain_matching(|id| compose_ids.contains(&id)),
        );
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_RECOMPOSE_TRACE").is_ok() {
            // 触发本次重组的 State id 列表（对应 State::set/update 调用）
            eprintln!("[recompose] 触发 State: {:?}", pending_batch.ids());
        }
        for state_id in pending_batch.ids() {
            if let Some(keys) = self.slot_deps.get(state_id) {
                for &k in keys {
                    self.slot_table.mark_dirty(k);
                }
            }
            // 两段式依赖：布局期注册的依赖 → 只标布局失效（重测不重组）
            if let Some(keys) = self.layout_deps.get(state_id) {
                for &k in keys {
                    self.layout_dirty_keys.insert(k);
                }
            }
        }
        // The `slot_key` → arena-index map is valid for the whole content closure: it is filled by
        // `layout` and consumed by materialize at the END of compose, so during content it still names
        // the nodes the previous frame built — which is where a container's Skip decision reads the
        // modifier it is comparing against (`container_modifier_unchanged`).
        //
        // It can be EMPTY here: a second compose in the same frame follows a materialize that drained
        // it. Refilling costs one tree walk, and only on those frames — materialize's own repair
        // (`materialize`) does exactly this walk and would do it anyway, so this moves the same work
        // earlier rather than adding it.
        if self.prev_node_by_key.is_empty() {
            if let Some(root_idx) = self.arena.root {
                crate::runtime::materialize::collect_node_keys(&self.arena, root_idx, &mut self.prev_node_by_key);
            }
        }

        // Begin an isolated dependency frame. State::get() writes to its active
        // buffer, while nested Composer calls temporarily own their own frame.
        // The frame remains open through materialization and modifier reads.
        let mut dependency_frame = crate::runtime::state::begin_compose_deps_with_queue(
            std::sync::Arc::downgrade(&self.pending_states),
        );

        {
            let ctx = &mut ComposeCtx::new(self);
            content(ctx);
        }

        // modifier 依赖注册（scroll 等）移到 layout 的 materialize 之后——
        // 完整分离：compose 阶段不建 arena（此处 arena 为空）

        // 注意：recording target 不在此处清除——layout()（measure 阶段）的
        // SizeDynamic 闭包内 State::get() 也要记录依赖（kf/dp 尺寸动画），
        // 由 layout() 末尾统一 clear + drain（见 layout()）。
        self.slot_table.truncate();
        // Skipped hosts still place their referenced content. Sweep only after composition is done,
        // allowing a subtree removed from an earlier host to be claimed by a later host in this pass.
        self.slot_table.dispose_unplaced_movable();
        let mut live_compose_keys = crate::layout::node::SlotKeySet::default();
        let skipped_slots = self.slot_table.collect_live_keys(&mut live_compose_keys);
        #[cfg(test)]
        {
            self.live_residue = skipped_slots;
        }
        let _ = skipped_slots;
        live_compose_keys.insert(0);
        // 防御：scope 配对完整性（漏配 end_scope 会导致 SCOPE_STACK 残留跨帧，
        // 使下帧组件外读取注册到失效 scope → 失效静默丢失）
        debug_assert_eq!(GROUP_STACK.with(|s| s.borrow().len()), 0,
            "compose 结束时 GROUP_STACK 应清空（scope/节点配对不完整）");
        GROUP_STACK.with(|s| s.borrow_mut().clear());

        // 完整分离：组合完成后物化布局树（测试/调用方可直接 layout_root_idx）
        self.materialize();
        // A subcomposing node the composition touched has to be re-measured, and the LAYOUT path is
        // what does that: `layout()` clears every node's `dirty`/`layout_dirty` and re-derives them
        // from `layout_dirty_keys`, then uses that set to decide which parents descend at all. A node
        // whose slot status was Clean (its parent entered, so its body re-ran without its slot being
        // marked) arrives here with `dirty=false`, so the only place left to say "measure me" is this
        // set. Asked of the POLICY as well as of the node's `subcomposed` flag, because the flag is set
        // by a parking measurement and is therefore false on a node that was just rebuilt.
        //
        // The `dirty` guard is load-bearing and the flag has to be precise, or this seeding is what
        // makes a subcomposing tree re-measure everything: `subcomposed` used to be inferred from a
        // "how many subcompositions were parked while this node measured" counter, which is true for
        // every ANCESTOR of a subcomposing node, so the root of an 800-row tree was seeded on every
        // frame that changed anything and the whole tree re-measured — 800 subcompositions for one
        // row's update (74915 µs against ≈1600 µs; `docs/benchmarks.md`).
        for idx in 0..self.arena.nodes.len() {
            let declares = self.arena.nodes[idx].dirty
                && (self.arena.nodes[idx]
                    .measure_policy
                    .and_then(|p| self.arena.policies.get(p))
                    .map(|p| p.subcomposes())
                    .unwrap_or(false)
                    || self.arena.nodes[idx].subcomposed);
            #[cfg(debug_assertions)]
            if declares && std::env::var("WINIA_SUBCOMPOSE_TRACE").is_ok() {
                eprintln!(
                    "[compose-end] idx={idx} key={} sub={} dirty={} declares={}",
                    self.arena.nodes[idx].slot_key,
                    self.arena.nodes[idx].subcomposed,
                    self.arena.nodes[idx].dirty,
                    declares
                );
            }
            if declares {
                self.layout_dirty_keys.insert(self.arena.nodes[idx].slot_key);
            }
        }
        // Shared-element flights (Phase 2): detect switches + retain/detach
        // sources BEFORE the prev drain below frees them.
        self.retain_shared_sources();
        // Repair the arena listing invariant for EVERY compose — this must not live inside
        // `retain_shared_sources`, which returns early when the composer has no shared content:
        // that took the repair off the default path, and a stale listing then walked straight
        // into the `[dup-key]` guard during layout (measured: a composer with no shared content
        // kept `mid.children = [2, 2]` and panicked, while the same state with one shared
        // endpoint present was pruned to `[2]`). Order is load-bearing: after the retention
        // above (a flight ghost is a legitimate root and must keep its subtree) and before the
        // prev drain below (a stale listing's node still has to be reclaimed by it).
        crate::runtime::materialize::prune_stale_child_links(self);
        // 物化后：注册 modifier 中引用的 State 依赖（scroll 等——组合期 arena 空）。
        // 必须在 take_deps() 之前执行——其中 State::get() 依赖 DEP_MODE=Compose
        //（begin_compose_deps 后未复位）；先复位则 scroll 依赖被静默丢弃（滚动不刷新）
        if let Some(root_idx) = self.arena.root {
            register_modifier_deps_recursive(&self.arena, root_idx);
        }
        // 依赖注册（组合期 + modifier 期收集的 State 依赖 → 按 slot 收敛）。
        let recorded = crate::runtime::state::take_deps();
        self.reconcile_compose_deps(recorded, &live_compose_keys);
        // Commit only after the read graph is reconciled. If content/materialize
        // panics first, the guard restores the outer dependency frame and rolls
        // back subscriptions learned by this failed compose.
        dependency_frame.commit();
        // 回收本帧未复用的上帧节点（结构变化移除的子树——on_remove 触发）；
        // 跳过已复用节点（已挂入本帧树，free 会递归进本帧树形成环）
        let mut visited = crate::layout::node::NodeMarks::default();
        for (key, idx) in self.prev_node_by_key.drain() {
            // A node marked reused is part of THIS frame's tree — either claimed in place
            // (`try_claim_skipped_subtree`) or reached through a refilled index. It was not removed,
            // so neither its layout dependencies nor its `on_remove` are due: reporting it as removed
            // pruned a live slot's layout dependency and froze its animation (measured:
            // `test_layout_dep_survives_const_fold` fired exactly that way).
            if self.reused_nodes.contains(idx) {
                continue;
            }
            // 收集移除的 slot_key（layout_deps 死 key 清理）
            self.removed_slot_keys.insert(key);
            self.arena.free_node_skip(idx, &self.reused_nodes, &mut visited);
        }
        self.prev_node_by_key.clear();
        self.reused_nodes.clear();
        // A notification that arrived after the batch was drained may belong to
        // a read removed by this frame. Drop only IDs with no live channel.
        self.pending_states
            .drain_matching(|id| !self.slot_deps.contains_key(&id) && !self.layout_deps.contains_key(&id));
        pending_batch.commit();
        dependency_transaction.commit();
        compose_runtime_transaction.commit();
        // The composition has committed: the slot tree is materialized, the read graph is reconciled
        // and every transaction is closed, so user callbacks that publish to the outside world can
        // run. Deliberately LAST — a `State` written here notifies with the dependency frame closed,
        // so it becomes the NEXT PASS's input rather than an edge this one has to account for.
        //
        // Recording is forced OFF for the duration: when this compose() was started from a layout
        // pass (a subcomposition), committing the inner frame restores the outer LAYOUT frame, and a
        // read in a side effect would be filed as that composer's layout dependency.
        let _no_deps = crate::runtime::state::begin_no_deps_frame();
        for effect in std::mem::take(&mut self.post_compose) {
            effect();
        }
    }

    /// 返回 LayoutNode 树的根节点引用
    /// How many times this composer has composed. A subcomposing policy compares it to tell "the same
    /// frame measured me twice" (report the first result) from "a later frame measured me again"
    /// (compose the content again, because its parameters may have changed).
    pub fn compose_generation(&self) -> u64 {
        self.compose_count
    }

    /// The constraints this composer's last `layout()` ran under, if any.
    pub(crate) fn cached_root_constraints(&self) -> Option<Constraints> {
        self.arena
            .root
            .and_then(|root| self.arena.nodes.get(root))
            .and_then(|node| node.cached_constraints)
    }

    /// Re-run this composer's layout under new constraints — used for a CACHED subcomposition, whose
    /// content is already composed and only its arrangement can change.
    pub(crate) fn relayout_subcomposition(&mut self, constraints: Constraints) {
        self.layout(constraints);
    }

    /// Make a KEPT subcomposition ready for the next frame's composition.
    ///
    /// Adoption moves a subcomposition's arena into the outer one, which leaves this composer with an
    /// empty arena and the per-node bookkeeping `layout()` built from the OLD one. `prev_node_by_key` is
    /// the reuse index materialize consults before building nodes (`Some(idx)` into an arena that no
    /// longer has that index — measured as `index out of bounds: the len is 0 but the index is 0`); the
    /// per-frame reuse marks belong to the same dead frame. The SLOT TABLE is deliberately untouched:
    /// that is the half this reuse exists for.
    pub(crate) fn prepare_subcomposition_for_recompose(&mut self) {
        self.prev_node_by_key.clear();
        self.reused_nodes.clear();
        // The arena index the drained tree used. It is read by `layout()` to decide where to start the
        // reuse walk, and it names a node that is no longer in this arena — the second crash this
        // preparation had to clear (`index out of bounds` in `insert_reuse_key`, reached from
        // `collect_layout_index`).
        self.arena.root = None;
    }

    /// Take the cached subcomposition (see the field), if this frame's next `subcompose()` can    /// re-arrange it instead of composing again.
    pub(crate) fn take_cached_subcomposition(&mut self) -> Option<Box<Composer>> {
        self.subcomposition_cache.take()
    }

    /// Park a subcomposition against the node that composed it, for the adoption pass at the end of
    /// `layout` (`layout::subcompose`). The registry is cleared at the start of every layout pass.
    pub(crate) fn park_subcomposition(
        &mut self,
        node: usize,
        entry: crate::layout::subcompose::Subcomposition,
    ) {
        // The node's OWN policy composed content — recorded here, at the moment it happens, because this
        // is the only place that knows WHICH node subcomposed. `Composer::compose`'s compose-end seeding
        // reads the flag to decide whether the node has to be re-measured; inferring it from a
        // "subcompositions parked so far" counter (which is what the flag used to be) made it true for
        // every ancestor of a subcomposing node as well — see `LayoutNode::subcomposed`.
        if node < self.arena.nodes.len() {
            self.arena.nodes[node].subcomposed = true;
        }
        self.subcompositions.push((node, entry));
    }

    pub fn layout_root(&self) -> Option<&LayoutNode> {
        self.arena.root()
    }

    /// 返回 LayoutNode 树的根节点可变引用
    pub fn layout_root_mut(&mut self) -> Option<&mut LayoutNode> {
        self.arena.root_mut()
    }

    /// arena 节点池只读访问（arena 化遍历用）
    pub fn arena_nodes(&self) -> &[LayoutNode] {
        &self.arena.nodes
    }

    /// arena 节点池可变访问（arena 化遍历用）
    pub fn arena_nodes_mut(&mut self) -> &mut Vec<LayoutNode> {
        &mut self.arena.nodes
    }

    /// 根节点 arena 索引
    pub fn layout_root_idx(&self) -> Option<usize> {
        self.arena.root
    }

    /// 测量策略池只读访问（measure_node 用）
    pub fn arena_policies(&self) -> &[Box<dyn MeasurePolicy>] {
        &self.arena.policies
    }

    /// 执行整棵布局树的 measure + place，并缓存测量结果供下帧复用
    pub fn layout(&mut self, root_constraints: Constraints) {
        // Measure callbacks can read State and invoke another Composer. Keep
        // their active slot/group/statement context isolated as well.
        let _runtime_frame = begin_runtime_frame();
        let _adaptive_context = crate::layout::adaptive::enter_context(self.adaptive.clone());
        let mut layout_transaction = LayoutTransaction::new(self);
        // Layout can be called without a preceding compose; consume layout-only
        // invalidations here so measure sees the dirty path directly.
        self.consume_layout_pending();
        // 应用布局失效：清全树旧标记 → 按 layout_dirty_keys 标节点 + 祖先传播
        // （保守超集：祖先全链标脏——布局动画场景父必然依赖子尺寸，Compose 精确传播留待优化）
        if let Some(root_idx) = self.arena.root {
            let nodes = &mut self.arena.nodes;
            // 清全树旧标记（每帧重新标记）
            for n in nodes.iter_mut() { n.layout_dirty = false; }
            if !self.layout_dirty_keys.is_empty() {
                crate::layout::node::apply_layout_dirty(nodes, root_idx, &self.layout_dirty_keys);
            }
            self.layout_dirty_keys.clear();
        } else {
            self.layout_dirty_keys.clear();
        }
        // 物化只在 compose 末尾（完整分离：组合完成即建树）——layout 只测量。
        // 单独调 layout（无 compose）时树为空——measure 无操作（无害）
        // 开始布局期依赖记录（measure 中 State::get → 两段式分流）
        let mut dependency_frame = crate::runtime::state::begin_layout_deps_with_queue(
            Arc::downgrade(&self.pending_states),
        );
        begin_layout_measure_tracking();
        // Subcomposing policies park their compositions while measuring; adopt them right after the
        // tree is measured and BEFORE the reuse index is rebuilt — an adopted subtree has to be in
        // that index, or the next frame's reuse path never sees it.
        self.subcompositions.clear();
        let _layout_host = crate::layout::subcompose::LayoutHostGuard::arm(self as *mut Composer);
        if let Some(root_idx) = self.arena.root {
            let (_size, _placements) = crate::layout::measure_node(
                &mut self.arena.nodes, &self.arena.policies, root_idx, root_constraints);
            self.arena.nodes[root_idx].measured_size = _size;
            let parked = std::mem::take(&mut self.subcompositions);
            // `adopt_parked` hands back the last composition it moved, which becomes the cache a
            // second layout pass in this same frame can re-arrange instead of re-composing.
            self.subcomposition_cache = crate::layout::subcompose::adopt_parked(
                &mut self.arena,
                &mut self.reused_nodes,
                parked,
                self.uid,
            );
            // 阶段D：重建 slot_key → 节点索引映射（供下帧 start_node 复用）+ dirty 冒泡
            self.prev_node_by_key.clear();
            crate::runtime::materialize::collect_layout_index(
                &mut self.arena,
                root_idx,
                &mut self.prev_node_by_key,
            );
        } else {
            self.subcompositions.clear();
            // No root means every old layout dependency is stale.
            self.prev_node_by_key.clear();
        }

        let recorded = crate::runtime::state::take_deps();
        let measured_keys = take_layout_measure_keys();
        // A Composer with no root has no live layout readers. Clear the forward
        // graph before rebuilding the reverse index so direct layout() calls
        // cannot retain subscriptions after the tree disappeared.
        if self.arena.root.is_none() {
            self.layout_slot_reads.clear();
            self.layout_deps.clear();
        }
        let mut reads_by_slot: crate::layout::node::SlotKeyMap<HashSet<StateId>> = crate::layout::node::SlotKeyMap::default();
        for (signal, slot_key) in &recorded {
            reads_by_slot.entry(*slot_key).or_default().insert(signal.id());
            self.layout_signal_handles
                .entry(signal.id())
                .or_insert_with(|| signal.clone());
        }

        // Replace only slots that actually ran measure. A cached slot is absent
        // from measured_keys and therefore keeps its last successful reads.
        //
        // As in `reconcile_compose_deps`, the reverse index below is a pure function of this forward
        // one, so it is only rebuilt when something here actually moved: a layout in which every node
        // folded (nothing measured, nothing read) then costs a few map lookups instead of a graph
        // rebuild. `cleanup_signal_subscriptions` still runs every frame — it is the one place that
        // prunes both graphs' subscriptions, and it runs with their union.
        let mut changed = false;
        for slot_key in measured_keys {
            let new_set = match reads_by_slot.remove(&slot_key) {
                Some(reads) if !reads.is_empty() => Some(reads),
                _ => None,
            };
            let same = match (self.layout_slot_reads.get(&slot_key), new_set.as_ref()) {
                (None, None) => true,
                (Some(old), Some(new)) => old == new,
                _ => false,
            };
            changed |= !same;
            match new_set {
                Some(reads) => {
                    self.layout_slot_reads.insert(slot_key, reads);
                }
                None => {
                    self.layout_slot_reads.remove(&slot_key);
                }
            }
        }
        // Compose confirmed removals before rebuilding the reverse index so a
        // dead slot cannot reappear from an old forward edge.
        for &slot_key in &self.removed_slot_keys {
            changed |= self.layout_slot_reads.remove(&slot_key).is_some();
            self.layout_dirty_keys.remove(&slot_key);
        }
        self.removed_slot_keys.clear();

        if changed {
            self.rebuild_layout_reverse_deps();
        }

        // Selection registration lives on the nodes, not in composition: a text that Skips never
        // re-registers, so the table can only be ordered/pruned from the tree. Do it here, after the
        // walk, where every live node is known and document order is the arena's child order.
        self.reindex_selection_registrars();

        self.cleanup_signal_subscriptions();
        self.debug_assert_dependency_graphs();
        self.pending_states
            .drain_matching(|id| !self.slot_deps.contains_key(&id) && !self.layout_deps.contains_key(&id));
        // Normal layout completion restores the outer dependency frame. A panic
        // before this point drops the guard and rolls back partial subscriptions.
        dependency_frame.commit();
        layout_transaction.commit();
        self.dispatch_layout_callbacks();
    }

    /// Rebuild every selection registrar's offset table from the layout tree, in document order.
    ///
    /// A text leaf carries the registrar it registered into (`LayoutNode::registrar`), so the walk
    /// sees exactly the live text nodes of each container — skipped ones included, removed ones
    /// absent. Each registrar gets its own slot list, in the order the children are visited.
    fn reindex_selection_registrars(&mut self) {
        let Some(root) = self.arena.root else { return };
        // Cheap guard: most trees have no selection at all, and the walk allocates per registrar.
        if !self.arena.nodes.iter().any(|n| n.registrar.borrow().is_some()) {
            return;
        }
        let mut groups: Vec<(crate::text::selection::SelectionRegistrar, Vec<u64>)> = Vec::new();
        let mut stack = vec![root];
        // Pre-order, children in document order: push them reversed so the pop order is forward.
        while let Some(idx) = stack.pop() {
            let node = &self.arena.nodes[idx];
            if let Some(reg) = node.registrar.borrow().as_ref() {
                match groups.iter_mut().find(|(r, _)| r.is_same(reg)) {
                    Some((_, slots)) => slots.push(node.slot_key),
                    None => groups.push((reg.clone(), vec![node.slot_key])),
                }
            }
            if let Some(sub) = node.subcomposed_child { stack.push(sub); }
            for &child in node.children.iter().rev() { stack.push(child); }
        }
        for (reg, slots) in groups {
            reg.reindex(&slots);
        }
    }

    /// Fire the position callbacks whose box changed this pass, then serve bring-into-view requests.
    ///
    /// Runs AFTER the dependency frame and the layout transaction commit, so a callback that writes a
    /// State schedules the next frame instead of being recorded as a layout dependency.
    fn dispatch_layout_callbacks(&mut self) {
        let Some(root) = self.arena.root else { return };
        // Cheap guard: most trees use neither feature, and the guarded work is a full tree walk.
        let has_positions = self
            .arena
            .nodes
            .iter()
            .any(|node| node.modifier.has_position_callbacks());
        if has_positions {
            // The collector starts at this composer's origin, so its change gate stores WINDOW-space
            // boxes: an overlay layer whose origin moves re-fires the callback instead of reporting
            // local coordinates until its own box changes.
            let (placed, global) = crate::layout::node::collect_position_callbacks(
                &mut self.arena.nodes,
                root,
                self.screen_origin,
            );
            for (callback, position) in placed {
                callback(position);
            }
            for (callback, position) in global {
                callback(position);
            }
        }
        let has_requests = self
            .arena
            .nodes
            .iter()
            .any(|node| node.modifier.has_bring_into_view_requester());
        if has_requests {
            crate::layout::relocation::process_bring_into_view(&mut self.arena.nodes, root);
        }
    }

    /// The index of the node carrying `Modifier::layout_id(id)`, if there is one.
    pub fn find_by_layout_id(&self, id: &str) -> Option<usize> {
        self.arena
            .nodes
            .iter()
            .position(|node| node.modifier.layout_id_value() == Some(id))
    }

    /// 请求重组（由 State 变化触发）。
    pub fn request_recomposition(&mut self, _key: u64) {
        self.needs_recomposition = true;
    }

    /// Re-run the WHOLE content on the next pass: the root scope and every group under it are marked
    /// dirty, so nested groups enter regardless of their (unchanged) parameters.
    ///
    /// This is what a caller recomposing means for a subcomposition it owns — an overlay's composer is
    /// a separate one, so nothing inside it can see that the caller produced a NEW content closure, and
    /// a group re-enters only for state it read or parameters it declared. Without this, overlay content
    /// built from a value the caller captured (a filtered list, a formatted string) keeps the first
    /// closure's value — measured on `search_bar_demo`, whose docked dropdown and fullscreen panel both
    /// stayed on their first rows while the input field updated. Marking only the root is not enough:
    /// the caller's lambda usually sits inside wrapper groups of its own, which declare nothing.
    ///
    /// Cost: that pass re-runs the content subtree (remembered state survives, `LazyColumn` items are
    /// recomposed by key), so a caller should re-register an overlay when its inputs changed rather
    /// than unconditionally every frame.
    pub fn mark_content_dirty(&mut self) {
        self.needs_recomposition = true;
        SlotTable::mark_dirty_subtree(&mut self.slot_table.root_slot);
    }

    /// Mark every slot INSIDE the current composition position dirty, leaving the slot we are in alone.
    ///
    /// This is the CompositionLocal companion. A local is a stack, and a reader only picks a new value
    /// up when its own group runs again — so a provider whose value changed has to dirty the readers it
    /// is about to wrap, or a group that declared nothing about itself keeps what it composed with.
    /// Marking the position's children (not the position itself) keeps the enclosing group's own Skip
    /// decision out of it: it re-enters next frame only if something else asks it to.
    pub(crate) fn mark_inner_slots_dirty(&mut self) {
        let slot = self.slot_table.current_slot();
        for child in &mut slot.children {
            SlotTable::mark_dirty_subtree(child);
        }
    }

    fn has_pending_compose_states(&self) -> bool {
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        self.pending_states
            .pending_ids()
            .into_iter()
            .any(|id| compose_ids.contains(&id))
    }

    /// Move layout-only invalidations into layout_dirty_keys without consuming
    /// an ID that also requires composition. This makes direct layout() calls
    /// correct while preserving a mixed compose/layout notification for compose().
    fn consume_layout_pending(&mut self) {
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        let layout_ids: HashSet<StateId> = self.layout_deps.keys().copied().collect();
        let layout_pending = self
            .pending_states
            .drain_non_compose_collect_layout(&compose_ids, &layout_ids);
        for state_id in layout_pending {
            if let Some(keys) = self.layout_deps.get(&state_id) {
                self.layout_dirty_keys.extend(keys.iter().copied());
            }
        }
    }

    /// 是否有待处理的 state 变化
    pub fn has_pending_states(&self) -> bool {
        !self.pending_states.is_empty()
    }

    /// 取走本帧注册的顶层弹出层（compose 后调用——清空收集）
    pub fn take_overlays(&mut self) -> Vec<crate::overlay::OverlayDesc> {
        std::mem::take(&mut self.overlays)
    }

    /// 当前组合节点的 slot_key（overlay 锚点用）
    pub fn active_slot_key(&self) -> u64 {
        self.slot_table.active_slot_key()
    }

    /// 重组次数（vsync 研究——单次渲染内的 compose 次数）
    pub fn compose_count(&self) -> u64 {
        self.compose_count
    }

    /// 待消费 State 数（vsync 研究——渲染时刻的 pending 积压）
    pub fn pending_state_count(&self) -> usize {
        self.pending_states.len()
    }

    /// 执行待处理的重组。返回 true 表示实际执行了 compose。
    /// 若无待处理则跳过，保留上一帧的布局树。
    pub fn recompose(&mut self, content: impl FnOnce(&mut ComposeCtx)) -> bool {
        // Resolve layout-only notifications before deciding whether composition
        // is needed. This prevents the app frame loop from spinning on a pending
        // ID that belongs only to layout_deps.
        self.consume_layout_pending();
        let has_pending = self.has_pending_compose_states();
        let will_run = self.needs_recomposition || has_pending || !self.pending_recomposition.is_empty();
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_RECOMPOSE_TRACE").is_ok() {
            // 重组触发诊断：何时执行/跳过 + 触发原因（pending=State 变化数）
            eprintln!(
                "[recompose] pending={} needs={} q={} → {}",
                has_pending, self.needs_recomposition, self.pending_recomposition.len(),
                if will_run { "COMPOSE" } else { "SKIP（无变化）" }
            );
        }

        if !will_run {
            return false;
        }

        // 处理队列中的待重组节点（当前简化：全量重组）
        self.pending_recomposition.clear();
        self.needs_recomposition = false;
        self.compose(content);
        true
    }
}

/// Compose 末尾：递归遍历 LayoutNode 树（arena），为所有 modifier 注册 State 依赖。
/// 模块级函数（impl 外）——impl 内直接调用。
fn register_modifier_deps_recursive(arena: &crate::layout::node::NodeArena, idx: usize) {
    // Modifier reads happen after GROUP_STACK is cleared; restore the node target
    // explicitly so scroll State dependencies do not all attach to the last node.
    set_active_slot_key(arena.nodes[idx].slot_key);
    arena.nodes[idx].modifier.register_state_deps();
    let children = arena.nodes[idx].children.clone();
    for c in children {
        register_modifier_deps_recursive(arena, c);
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Composer {
    fn drop(&mut self) {
        if self.compose_transaction.is_some() {
            self.rollback_compose_runtime();
        }
        crate::animation::clear_animations_for_states(
            &self.animation_state_ids.iter().copied().collect::<Vec<_>>(),
        );
        self.pending_states.unsubscribe_all();
    }
}

