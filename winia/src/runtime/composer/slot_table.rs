use super::*;

// ── SlotTable ──

/// 节点描述（组合产物——组合树持有，布局阶段物化为 LayoutNode）
pub(super) struct NodeDesc {
    pub(super) key: u64,
    pub(super) modifier: Modifier,
    pub(super) policy: Option<Box<dyn MeasurePolicy>>,
    pub(super) on_remove: Option<Box<dyn FnOnce() + Send>>,
    /// 本帧是否需重测（start_slot 的 Dirty 状态——slot.dirty 在 start_slot
    /// 被消费清 false，物化时须从 desc 携带）
    pub(super) dirty: bool,
    /// 文本选择 registrar（组合期 set_current_node_registrar 写入——物化时应用）
    pub(super) registrar: Option<crate::text::selection::SelectionRegistrar>,
    pub(super) clear_registrar: bool,
    /// 焦点环颜色（组合期 set_current_node_focus_color 写入——物化时应用；
    /// 渲染期 CompositionLocal 已退出，必须组合期捕获）
    pub(super) focus_color: Option<crate::graphics::Color>,
    /// 选区高亮色（组合期 set_current_node_selection_color 写入——同 focus_color 通道）
    pub(super) selection_color: Option<crate::graphics::Color>,
    /// IME 组合下划线颜色（组合期 set_current_node_composing_color 写入——
    /// 物化时应用；渲染期不能读 CompositionLocal（Phase 4.2），组合期捕获主题 primary）
    pub(super) composing_color: Option<crate::graphics::Color>,
    /// 光标（TextField）——组合期写入，物化时应用（node_stack 已废弃——
    /// 组合期无 arena 节点，直接写节点会静默失效）
    pub(super) cursor_index: Option<usize>,
    pub(super) cursor_visible: Option<bool>,
    pub(super) cursor_callback: Option<Box<dyn Fn(usize) + Send>>,
    /// 显示聚焦标记（text-field-v2 容器化：焦点/交互在容器节点，输入
    /// 子节点渲染光标/选区需组合期标记——渲染端优先用此，回退 node.focused）
    pub(super) display_focused: Option<bool>,
    /// IME 预输入回调（TextField——app.rs 的 Ime::Preedit 直接调用）
    pub(super) ime_callback: Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>,
    /// IME 组合范围（渲染画下划线用）——外层 Option 区分"未设置"与"清空"
    pub(super) composing_range: Option<Option<std::ops::Range<usize>>>,
    /// 布局方向（组合期捕获——provides 作用域内读 CompositionLocal；
    /// 物化在组合回调后执行——届时主题的 provider 已退出作用域，
    /// 必须从 desc 携带，否则 RTL 下节点快照恒 Ltr → offset/padding 镜像失效）
    pub(super) direction: crate::layout::LayoutDirection,
}

/// 组合节点的一个槽位。每个 composable 调用对应一个 Slot。
pub(super) struct Slot {
    pub(super) key: u64,
    pub(super) remembered: HashMap<u64, Box<dyn Any>>,
    pub(super) children: Vec<Slot>,
    /// 重组时是否需要执行（State 变化标记）
    pub(super) dirty: bool,
    /// 当前帧中此 slot 的子树总 slot 数（含自身；用于 skip 时重放）
    pub(super) children_count: usize,
    /// 是否为组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    pub(super) is_scope: bool,
    /// 上帧参数（`ComposeCtx::changed` 写入，按序比较——对标 Compose `$composer.changed`）
    pub(super) params: Vec<Box<dyn ParamValue>>,
    /// 节点描述（组合产物）——is_scope 或纯组合 Slot 为 None（物化阶段消费）
    pub(super) desc: Option<NodeDesc>,
    /// 本帧是否被访问（start_slot 置 true；reset 每帧清）——物化只收集活跃
    /// slot：结构回退时（content 少建子节点）末尾残留的上帧 slot 不收集——
    /// 其 desc 不物化，对应 arena 节点由 prev_node_by_key 回收（free）
    pub(super) visited: bool,
    /// Skip 子树时保留的组合产物 modifier（content 未执行——desc 为 None；
    /// 但 build 的 modifier 参数可能变化（父层重跑传入的 offset/背景等视觉
    /// 属性）——物化 Skip 恢复时应用，避免视觉卡旧值（动画中间值不渲染）
    pub(super) skip_modifier: Option<Modifier>,
    /// 布局方向（组合期捕获——物化期读不到 CompositionLocal）
    pub(super) direction: crate::layout::LayoutDirection,
    /// Skip 时保存的容器 policy（外层传入——content 未执行但 policy 可用，
    /// 物化降级/恢复时避免 policy 缺失导致测量 0 尺寸）
    pub(super) skip_policy: Option<Box<dyn MeasurePolicy>>,
    /// This slot IS the storage root of movable content #id — Compose's `movableContentOf`.
    ///
    /// The content's slots live here, OUTSIDE the tree, so the branch that invokes the content can
    /// disappear without taking the content's `remember`ed state with it. A container that ends its
    /// group without visiting a child drops that child (`end_restartable_group`), which is exactly
    /// what loses the state today — putting the content in a store keeps it out of harm's way.
    pub(super) movable_store: Option<u64>,
    /// Movable content #id is composed HERE. This slot holds nothing of its own: the descriptor walk
    /// inlines the store's subtree in its place, so the content's nodes are parented by whoever
    /// invokes it, while its slots (and so its state) live in the store.
    pub(super) movable_ref: Option<u64>,
    /// Lifecycle callbacks owned by this slot, keyed like `remembered` so a recomposition
    /// REPLACES rather than stacks. Drained — and run — when the slot is dropped. This is what
    /// lets an effect clean up without materializing a layout node to carry the callback.
    pub(super) forget: HashMap<u64, Box<dyn FnOnce() + Send>>,
}

impl Drop for Slot {
    /// The removal path with no collector: a slot can be dropped wholesale — the whole `Composer`
    /// going away, a `movable_store` entry removed, a `retain` in `end_scope` — and none of those
    /// go through `truncate_children`. `LayoutNode` has the same `Drop` for the same reason, and
    /// losing it here was a real leak: a `remember_infinite_transition`'s ids stayed in the global
    /// animation table after its composer was dropped (measured 1 -> 0 before the migration, 1 -> 1
    /// after it).
    ///
    /// The explicit sites drain `forget` before dropping, so this fires only for the paths they do
    /// not cover, and never twice.
    fn drop(&mut self) {
        for (_, f) in self.forget.drain() {
            f();
        }
    }
}

impl Slot {
    pub(super) fn new(key: u64) -> Self {
        Self {
            key,
            remembered: HashMap::new(),
            children: Vec::new(),
            dirty: true, // 新创建的 slot 总是 dirty（首次必须执行）
            children_count: 1, // 自身
            is_scope: false,
            params: Vec::new(),
            desc: None,
            visited: true, // 新建即本帧活跃
            skip_modifier: None,
            skip_policy: None,
            movable_store: None,
            movable_ref: None,
            forget: HashMap::new(),
            direction: crate::layout::LayoutDirection::Ltr,
        }
    }

    pub(super) fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        if let Some(existing) = self.remembered.get(&slot_key) {
            if let Some(state) = existing.downcast_ref::<State<T>>() {
                return state.clone();
            }
        }
        let state = init();
        self.remembered.insert(slot_key, Box::new(state.clone()));
        state
    }

    pub(super) fn remember_handle<H: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> H,
    ) -> H {
        if let Some(existing) = self.remembered.get(&slot_key) {
            if let Some(handle) = existing.downcast_ref::<H>() {
                return handle.clone();
            }
        }
        let handle = init();
        self.remembered.insert(slot_key, Box::new(handle.clone()));
        handle
    }

}

/// 可重启分组状态 — start_restartable_group() 返回
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GroupStatus {
    /// 子树完全 clean → 跳过内容闭包，重放 slot 结构
    Skip,
    /// 子树有变化 → 正常执行闭包
    Enter,
}


/// 槽位状态 — start_slot() 返回
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum SlotStatus {
    /// slot 与上一帧相同，且未被标记为脏 → 可跳过子树重建
    Clean,
    /// slot 与上一帧相同，但被标记为脏 → 需要重新测量
    Dirty,
}

/// 槽位表 — 组合树的内部数据结构（树形嵌套）

pub(crate) struct SlotTable {
    pub(super) path: Vec<usize>,
    pub(super) root_slot: Slot,
    pub(super) child_counters: Vec<usize>,
    /// The slot subtrees of movable content, by id — Compose's movable content store. They are NOT
    /// in the tree: the invocation site holds a `movable_ref` slot and the descriptor walk inlines
    /// the store's subtree there, so the content survives its branch going away.
    movable_store: std::collections::HashMap<u64, Slot>,
    /// Which store is being composed right now, if any. While this is set, `current_slot` addresses
    /// the store instead of `root_slot`, so `remember`/`changed`/`start_slot` need no special case
    /// inside movable content.
    pub(super) movable_composing: Option<u64>,
    /// The `path` while a store is being composed — saved on entry because the content's indices are
    /// relative to its own root, not to the invocation site.
    movable_saved_path: Vec<usize>,
    /// The `child_counters` while a store is being composed — the content's positions are its own.
    movable_saved_counters: Vec<usize>,
    /// Frame of the last invocation, used to reject unsupported duplicate placement explicitly.
    /// Compose supports multiple independent placements; this minimal API requires distinct handles.
    movable_frame: crate::layout::node::SlotKeyMap<u64>,
    /// The frame number `movable_frame` is compared against; `Composer::compose` sets it.
    pub(super) frame_no: u64,
    /// 当前 compose 期间活跃的 slot key（用于 State→Slot 的脏标记）
    pub(super) active_slot_key: u64,
    /// 被 State 变化标记为 dirty 的 slot key 集合
    dirty_keys: crate::layout::node::SlotKeySet,
}

pub(super) struct SlotTableRuntimeSnapshot {
    pub(super) path: Vec<usize>,
    pub(super) child_counters: Vec<usize>,
    pub(super) active_slot_key: u64,
    dirty_keys: crate::layout::node::SlotKeySet,
    pub(super) movable_composing: Option<u64>,
    movable_saved_path: Vec<usize>,
    movable_saved_counters: Vec<usize>,
}

pub(super) struct ClaimCtx<'a> {
    arena: &'a crate::layout::node::NodeArena,
    prev: &'a mut crate::layout::node::SlotKeyMap<usize>,
    reused: &'a mut crate::layout::node::NodeMarks,
    /// Indices marked by the claim currently being verified. Merged into `reused` only once the whole
    /// subtree verified, so a mismatch leaves nothing behind.
    scratch: crate::layout::node::NodeMarks,
    /// Every index this frame's claims have taken, across all of them. The index walk drops these
    /// from `prev_node_by_key` ONCE per frame at the end — doing it per claim would make a list of N
    /// small skipped rows cost N scans of an N-entry map (measured: the one-row update went from
    /// 2023 µs to 3064 µs before this was hoisted out).
    claimed_frame: crate::layout::node::NodeMarks,
    /// Subtrees claimed in place during this walk, and subtrees whose claim was REJECTED because the
    /// arena did not match (test-visible through `Composer::skip_claims` / `skip_claim_bails`).
    claims: usize,
    bails: usize,
}

impl SlotTable {
    pub(super) fn runtime_snapshot(&self) -> SlotTableRuntimeSnapshot {
        SlotTableRuntimeSnapshot {
            path: self.path.clone(),
            child_counters: self.child_counters.clone(),
            active_slot_key: self.active_slot_key,
            dirty_keys: self.dirty_keys.clone(),
            movable_composing: self.movable_composing,
            movable_saved_path: self.movable_saved_path.clone(),
            movable_saved_counters: self.movable_saved_counters.clone(),
        }
    }

    pub(super) fn restore_runtime(&mut self, snapshot: SlotTableRuntimeSnapshot) {
        self.path = snapshot.path;
        self.child_counters = snapshot.child_counters;
        self.active_slot_key = snapshot.active_slot_key;
        self.dirty_keys = snapshot.dirty_keys;
        self.movable_composing = snapshot.movable_composing;
        self.movable_saved_path = snapshot.movable_saved_path;
        self.movable_saved_counters = snapshot.movable_saved_counters;
    }

    pub(super) fn new() -> Self {
        Self {
            path: Vec::new(),
            root_slot: Slot::new(0),
            child_counters: vec![0],
            movable_store: std::collections::HashMap::new(),
            movable_composing: None,
            movable_saved_path: Vec::new(),
            movable_saved_counters: Vec::new(),
            movable_frame: crate::layout::node::SlotKeyMap::default(),
            frame_no: 0,
            active_slot_key: 0,
            dirty_keys: crate::layout::node::SlotKeySet::default(),
        }
    }

    pub(super) fn current_slot(&mut self) -> &mut Slot {
        // Inside movable content the tree is the STORE, not the root: the content's slots live there
        // so that the branch invoking it can vanish without taking its state along.
        if let Some(id) = self.movable_composing {
            let store = self
                .movable_store
                .get_mut(&id)
                .expect("a store exists for the content being composed");
            let mut slot = store;
            for &idx in &self.path {
                slot = &mut slot.children[idx];
            }
            return slot;
        }
        let mut slot = &mut self.root_slot;
        for &idx in &self.path {
            slot = &mut slot.children[idx];
        }
        slot
    }

    /// 位置哈希（enter_stmt 的 seq 分量）：**完整 child_counters 链的 fnv**——
    /// 行容器语句（循环内）链 = [...,父层, 迭代位置]；content 闭包内语句链 =
    /// [...,行index, 行内位置]——不同行实例的链不同（父层 index 不同）→ seq
    /// 区分。start_slot 每帧无条件执行（Skip 帧也执行）→ 链跨帧稳定（位置
    /// 而非执行次数——替代旧 STMT_SEQ，滚动时 Skip/Enter 交替不再漂移）。
    pub(crate) fn sibling_position(&self) -> u32 {
        let mut start = 0;
        if let Some(id) = self.movable_composing {
            let mut slot = &self.movable_store[&id];
            for (depth, &index) in self.path.iter().enumerate() {
                slot = &slot.children[index];
                if slot.is_scope { start = depth + 1; }
            }
        }
        // Compiler scopes inside a placement already own stable identities. Their statement positions
        // are local to that scope, not to the ordinal at which the scope was called in the store.
        let mut h: u64 = 0xcbf29ce484222325;
        for &c in &self.child_counters[start..] {
            h ^= c as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h as u32
    }


    /// 当前活跃 slot 的 key（overlay 锚点用）
    pub(super) fn active_slot_key(&self) -> u64 {
        self.active_slot_key
    }

    /// 当前作用域内"前一个已组合兄弟"的 slot key（Popup 锚点用：对标 Compose
    /// Popup 在父布局中的位置）。按本帧 child_counters 索引——不能取
    /// children.last()：重组帧中上一帧的后缀兄弟尚未 truncate，会锚错节点。
    /// 无前一个兄弟时返回 None（调用方回退窗口对齐）。
    pub(super) fn prev_sibling_slot_key(&self) -> Option<u64> {
        let idx = *self.child_counters.last().unwrap_or(&0);
        if idx == 0 {
            return None;
        }
        let mut slot = match self.movable_composing {
            Some(id) => self.movable_store.get(&id)?,
            None => &self.root_slot,
        };
        for &i in &self.path {
            slot = &slot.children[i];
        }
        slot.children.get(idx - 1).map(|c| c.key)
    }

    /// Hands `incoming` to the current slot and returns what the slot held before.
    ///
    /// The exchange (rather than an assignment) is what recycles: the returned vector goes back to
    /// `Composer::pending_params`, where the next `changed` calls overwrite its boxes in place
    /// (`ParamValue::set_from_any`) instead of allocating new ones.
    pub(super) fn replace_current_params(&mut self, mut incoming: Vec<Box<dyn ParamValue>>) -> Vec<Box<dyn ParamValue>> {
        std::mem::swap(&mut self.current_slot().params, &mut incoming);
        incoming
    }

    /// 设置当前 slot 的节点描述（组合产物——物化阶段消费）
    pub(super) fn set_current_desc(&mut self, desc: Option<NodeDesc>) {
        self.current_slot().desc = desc;
    }

    /// Skip 路径保留组合产物 modifier（content 未执行——desc=None；物化
    /// Skip 恢复节点时应用新 modifier——视觉属性（offset/背景）随父层重跑更新）
    pub(super) fn set_skip_modifier(&mut self, modifier: Modifier) {
        self.current_slot().skip_modifier = Some(modifier);
    }

    /// Skip 分支组合期捕获布局方向（物化期读不到 CompositionLocal）
    pub(super) fn set_skip_direction(&mut self, direction: crate::layout::LayoutDirection) {
        self.current_slot().direction = direction;
    }
    pub(super) fn set_skip_policy(&mut self, policy: Option<Box<dyn MeasurePolicy>>) {
        self.current_slot().skip_policy = policy;
    }

    /// 收集物化描述树：Slot 树 → 纯节点树（scope 跳过——children 提升；
    /// Skip 子树 slot 记 skip 标记——物化时从 prev_node_by_key 恢复）。
    /// 消费 desc（take——policy/on_remove 移出）——物化阶段调用。
    /// visited 语义：本帧活跃（start_slot 置 true；reset 每帧清）——结构回退的
    /// 残留（visited false 且不在 Skip 子树内）不收集；Skip 子树（visited false
    /// 但属于 Skip group）整体收集（skip 标记——物化恢复）
    ///
    /// A skipped subtree whose slots are all INERT (no desc left, no skip payload left) is claimed
    /// IN PLACE instead of being re-encoded: the walk verifies node-for-node that the arena already
    /// holds exactly this subtree, marks the nodes as reused, drops their keys from
    /// `prev_node_by_key` (so the compose tail does not recycle them), and emits ONE descriptor
    /// carrying the claimed index. See `try_claim_skipped_subtree` for why that is equivalent, and
    /// `docs/benchmarks.md` for what it is worth: at 800 rows an idle frame spent ~390 µs building
    /// and then walking a descriptor tree for a subtree where nothing had changed.
    pub(crate) fn collect_desc_tree(
        &mut self,
        out: &mut Vec<crate::runtime::materialize::DescNode>,
        arena: &crate::layout::node::NodeArena,
        prev: &mut crate::layout::node::SlotKeyMap<usize>,
        reused: &mut crate::layout::node::NodeMarks,
    ) -> (usize, usize) {
        fn rec(
            slot: &mut Slot,
            out: &mut Vec<crate::runtime::materialize::DescNode>,
            in_skip: bool,
            depth: usize,
            ctx: &mut ClaimCtx,
            store: &mut std::collections::HashMap<u64, Slot>,
        ) {
            if !slot.visited && !in_skip { return; }
            // A live reference inlines its placement. When the invoking host skipped, replay the
            // stored tree as a skipped subtree; never revive an unvisited reference from an entered
            // host or an uncomposed tail from a placement whose body ran.
            if let Some(id) = slot.movable_ref {
                // Held out of the map across the walk so the recursion can keep its own `&mut` to the
                // store (movable content inside movable content is not supported, so nothing else can
                // reach for this same entry).
                let Some(mut content) = store.remove(&id) else { return };
                let replay = !content.visited;
                for child in &mut content.children {
                    rec(child, out, replay, depth + 1, ctx, store);
                }
                store.insert(id, content);
                return;
            }
            if let Some(desc) = slot.desc.take() {
                let mut node = crate::runtime::materialize::DescNode {
                    key: desc.key,
                    skip: false,
                    modifier: desc.modifier,
                    preserve_modifier: false,
                    policy: desc.policy,
                    on_remove: desc.on_remove,
                    dirty: desc.dirty, // start_slot 的 Dirty 状态（slot.dirty 已消费）
                    registrar: desc.registrar,
                    clear_registrar: desc.clear_registrar,
                    focus_color: desc.focus_color,
                    selection_color: desc.selection_color,
                    composing_color: desc.composing_color,
                    cursor_index: desc.cursor_index,
                    cursor_visible: desc.cursor_visible,
                    cursor_callback: desc.cursor_callback,
                    display_focused: desc.display_focused,
                    ime_callback: desc.ime_callback,
                    composing_range: desc.composing_range,
                    direction: desc.direction,
                    claimed: None,
                    children: Vec::new(),
                };
                for child in &mut slot.children {
                    rec(child, &mut node.children, false, depth + 1, ctx, store);
                }
                out.push(node);
            } else if !slot.is_scope {
                // Skip 子树 slot（content 未执行——desc 空但非 scope）：
                // 整棵子树按 key 结构恢复（物化时从 prev_node_by_key 恢复——
                // 不物化上帧 desc——子树整体保留，children 重新挂接）
                let claimed = SlotTable::try_claim_skipped_subtree(slot, ctx);
                let sm = slot.skip_modifier.take();
                let sp = slot.skip_policy.take();
                let mut node = crate::runtime::materialize::DescNode {
                    key: slot.key,
                    skip: true,
                    claimed,
                    modifier: sm.clone().unwrap_or_default(),
                    // 容器自身 build 被调（set_skip_modifier 写入）→ 应用新 modifier；
                    // 后代（Skip 子树内未执行）→ 保留缓存节点 modifier（不清空视觉）
                    preserve_modifier: sm.is_none(),
                    policy: sp,
                    on_remove: None,
                    dirty: false,
                    registrar: None,
                    clear_registrar: false,
                    focus_color: None,
                    selection_color: None,
                    composing_color: None,
                    cursor_index: None,
                    cursor_visible: None,
                    cursor_callback: None,
                    display_focused: None,
                    ime_callback: None,
                    composing_range: None,
                    direction: slot.direction,
                    children: Vec::new(),
                };
                if claimed.is_none() {
                    for child in &mut slot.children {
                        rec(child, &mut node.children, true, depth + 1, ctx, store); // Skip 子树内：子也按同一规则（收集）
                    }
                }
                #[cfg(debug_assertions)]
                if std::env::var("WINIA_MAT_PROBE").is_ok() {
                    eprintln!(
                        "[collect] skip key={:x} kids={} claimed={:?}",
                        slot.key, node.children.len(), claimed
                    );
                }
                out.push(node);
            } else {
                // scope：不物化——children 提升到最近物化父（保持 in_skip 状态）
                for child in &mut slot.children {
                    rec(child, out, in_skip, depth + 1, ctx, store);
                }
            }
        }
        let mut ctx = ClaimCtx {
            arena,
            prev,
            reused,
            scratch: crate::layout::node::NodeMarks::default(),
            claimed_frame: crate::layout::node::NodeMarks::default(),
            claims: 0,
            bails: 0,
        };
        for child in &mut self.root_slot.children {
            rec(child, out, false, 0, &mut ctx, &mut self.movable_store);
        }
        if ctx.claims > 0 {
            let claimed = std::mem::take(&mut ctx.claimed_frame);
            ctx.prev.retain(|_, idx| !claimed.contains(*idx));
        }
        (ctx.claims, ctx.bails)
    }

    /// Tries to keep a skipped subtree exactly where it already is.
    ///
    /// A skipped subtree is, by definition, one whose content did not run — so its slots still say
    /// what they said the last time it was built, and the arena already holds the result of building
    /// exactly that. Re-encoding it into descriptors and walking them back into the arena (the
    /// fallback this replaces) reproduces the tree node for node; measured at 800 rows, that cost
    /// ~390 µs of an idle frame (`docs/benchmarks.md`).
    ///
    /// The claim is only taken when it is provably a no-op beyond bookkeeping. That means every slot
    /// in the subtree must be INERT:
    ///
    /// * no `desc` (a live desc is this frame's content waiting to be applied, and the descriptor path
    ///   consumes it — `take()` — so skipping that would leave the slot in a different state),
    /// * no `skip_modifier` / `skip_policy` for anything but the root (the same `take()`
    ///   choreography: a descendant holding a payload would have it applied today and cleared).
    ///
    /// The root is exempt because its payload is what the caller carries on the descriptor anyway.
    /// With every slot inert, the fallback's per-node work reduces to: reuse the node, clear its
    /// children and re-add the same ones in the same order, keep the same modifier, keep the same
    /// dirty flag — all of which the arena already satisfies. What is left is the bookkeeping this
    /// does: mark the nodes (so the compose tail does not recycle them) and drop their keys from
    /// `prev_node_by_key`.
    ///
    /// Verification is node for node, not a count: at every level the arena node's children must be
    /// exactly the nodes the slot children claim, in order. That subsumes the shape check the
    /// fallback performs at skip boundaries (which exists so a structure change cannot resurrect a
    /// stale measurement — `docs/benchmarks.md` tells that story), and it is what makes "the arena
    /// already holds this subtree" a checked fact rather than an assumption. A mismatch claims
    /// nothing (the marks go to a scratch set that is merged only on success), so the caller's
    /// fallback starts from an untouched arena.
    ///
    /// The verification is not a formality — something DOES mutate the arena between the `layout` that
    /// fills `prev_node_by_key` and this walk: `app.rs` runs `poll_shared_flights()` after `layout()`,
    /// and a flight detachment unlinks nodes (`detach_source`'s `children.retain`). That is the window
    /// the child-order comparison above exists for, and `test_stale_arena_shape_makes_the_claim_bail`
    /// exercises it directly by swapping a container's children.
    ///
    /// **The walk has two jobs and both are load-bearing.** It verifies, and it MARKS every node of the
    /// subtree as reused — the compose tail then frees everything in `prev_node_by_key` that is not
    /// marked, and `free_node_skip` reads the same set. Skipping the walk (rather than just the
    /// comparisons) was tried as a measurement and corrupts the tree on the first idle frame: the
    /// unmarked descendants get recycled under the live subtree, come back as `LayoutNode::default()`
    /// with key 0, and `collect_node_keys` panics with `[dup-key]` on two nodes claiming that key. So
    /// "claim without walking" is not a cheaper claim; it is a broken one.
    pub(super) fn try_claim_skipped_subtree(slot: &Slot, ctx: &mut ClaimCtx) -> Option<usize> {
        let root = *ctx.prev.get(&slot.key)?;
        ctx.scratch.clear();
        if !SlotTable::verify_claimed_node(slot, root, ctx) {
            ctx.bails += 1;
            return None;
        }
        // Committed: the subtree is the frame's tree, so the drain must not reclaim it (the index is
        // swept once, at the end of the walk) and every node in it counts as reused (the
        // shared-element detach reads this set).
        ctx.reused.merge(&ctx.scratch);
        ctx.claimed_frame.merge(&ctx.scratch);
        ctx.claims += 1;
        Some(root)
    }

    /// Marks `node_idx` as claimed and verifies it is the node `slot` materializes to, children
    /// included.
    pub(super) fn verify_claimed_node(slot: &Slot, node_idx: usize, ctx: &mut ClaimCtx) -> bool {
        if !ctx.scratch.insert(node_idx) {
            return false; // already claimed: a cycle, or two keys naming one node
        }
        if ctx.prev.get(&slot.key) != Some(&node_idx) {
            return false;
        }
        let mut pos = 0usize;
        if !SlotTable::verify_claimed_children(
            &slot.children,
            &ctx.arena.nodes[node_idx].children,
            &mut pos,
            ctx,
        ) {
            return false;
        }
        pos == ctx.arena.nodes[node_idx].children.len()
    }

    /// Walks `slots` against `expected` — the arena children list of the node whose children they
    /// are — advancing `pos` once per materialized slot. Scopes materialize nothing and pass their
    /// children through, exactly as the descriptor walk hoists them.
    pub(super) fn verify_claimed_children(
        slots: &[Slot],
        expected: &[usize],
        pos: &mut usize,
        ctx: &mut ClaimCtx,
    ) -> bool {
        for slot in slots {
            if slot.desc.is_some() {
                // A live desc: the fallback applies it and consumes it. Not inert — bail.
                return false;
            }
            if slot.is_scope {
                if !SlotTable::verify_claimed_children(&slot.children, expected, pos, ctx) {
                    return false;
                }
                continue;
            }
            // A skipped group or an inert leaf. Only the ROOT may still hold a payload; anything
            // reached here is a descendant, so a payload means the fallback would apply it.
            if slot.skip_modifier.is_some() || slot.skip_policy.is_some() {
                return false;
            }
            let Some(&node_idx) = expected.get(*pos) else {
                return false; // the parent lists fewer children than the slots materialize
            };
            *pos += 1;
            if !SlotTable::verify_claimed_node(slot, node_idx, ctx) {
                return false;
            }
        }
        true
    }

    pub(super) fn start_slot(&mut self, key: u64) -> SlotStatus {
        let idx = *self.child_counters.last().unwrap_or(&0);
        self.active_slot_key = key;
        ACTIVE_SLOT_KEY.with(|c| c.set(key));
        let is_dirty = self.dirty_keys.remove(&key);
        // Lifecycle callbacks from slots this call drops; run at the END, once the tree and the
        // cursors are consistent again.
        let mut orphaned: Vec<Box<dyn FnOnce() + Send>> = Vec::new();
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                eprintln!("[slot] key={} path={:?} dirty={}", key >> 32, self.path.clone(), is_dirty);
            }
        }
        let parent = self.current_slot();
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                // Report the slot's OWN dirty flag as well as the key-set one: the two are different
                // sources (path marking vs a state write) and only their combination decides Skip.
                let slot_state = match parent.children.get(idx) {
                    Some(child) if child.key == key => format!("slot_dirty={}", child.dirty),
                    Some(child) => format!("slot_key_mismatch={:x}", child.key),
                    None => "no_slot".to_string(),
                };
                eprintln!(
                    "[slot] key={} idx={} parent_kids={} key_dirty={} {}",
                    key >> 32,
                    idx,
                    parent.children.len(),
                    is_dirty,
                    slot_state
                );
            }
        }

        // key 匹配，或"同位置"（key 高位 = slot 路径哈希相同）——Enter/Skip 的
        // counter 漂移不改位置，按索引复用（更新 key 保持同步），避免 truncate
        // 重建 slot 导致 remember 的 State 丢失
        let same_position = idx < parent.children.len()
            && parent.children[idx].key >> 32 == key >> 32;
        // A key that sits LATER among the siblings is a reorder — an explicit `ctx.key` list that
        // moved an item, since statement order cannot change on its own. Rotate that slot into the
        // current position instead of truncating from here: truncating would drop every sibling
        // after `idx` and rebuild them, losing the `remember`ed state the key exists to preserve.
        // Measured before this: a two-entry keyed swap recreated both slots.
        //
        // The search only fires when the index does not already match (below), and only among this
        // parent's direct children, so the unkeyed path is untouched: two unkeyed siblings have
        // different statement keys and neither is ever found elsewhere.
        if !(idx < parent.children.len()
            && (parent.children[idx].key == key || same_position))
        {
            if let Some(found) = parent.children.iter().position(|c| c.key == key) {
                if found > idx {
                    let moved = parent.children.remove(found);
                    parent.children.insert(idx, moved);
                }
            }
        }
        if idx < parent.children.len() && (parent.children[idx].key == key || same_position) {
            parent.children[idx].key = key; // 同步最新 key（counter 可能漂移）
            parent.children[idx].visited = true; // 本帧活跃（物化收集依据）
            // An ordinary scope/node can reuse a former reference position. Only begin_movable may
            // mark this visit as a reference; stale roles would replay an already removed payload.
            parent.children[idx].movable_ref = None;
            if !parent.children[idx].dirty && !is_dirty {
                self.path.push(idx);
                self.child_counters.last_mut().map(|c| *c += 1);
                self.child_counters.push(0);
                return SlotStatus::Clean;
            }
            parent.children[idx].dirty = false;
            self.path.push(idx);
        } else {
            #[cfg(debug_assertions)]
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                let plen = parent.children.len();
                let pkey = parent.key;
                eprintln!("[slot-trunc] key={:x} parent={:x} idx={} len={}", key, pkey, idx, plen);
            }
            // Dropped slots take their lifecycle callbacks with them; run them after the
            // structural change so a callback that composes nothing sees a consistent tree.
            orphaned = SlotTable::truncate_children(&mut parent.children, idx);
            parent.children.push(Slot::new(key));
            // 新建 slot：本帧返回 Dirty 即已执行；立即消费 dirty 标记，
            // 否则残留 true 会让下一帧本应 clean 的 slot 误判为 Dirty
            parent.children.last_mut().unwrap().dirty = false;
            self.path.push(idx);
        }
        if let Some(last) = self.child_counters.last_mut() { *last += 1; }
        self.child_counters.push(0);
        // Last, not first: `parent.children` and `self.path` are already in their new shape, so a
        // panicking cleanup unwinds from a consistent composer instead of one missing the slot it
        // was about to create.
        for f in orphaned { f(); }
        SlotStatus::Dirty
    }

    pub(super) fn end_slot(&mut self) {
        // 计算子树 slot 总数（自身 + 所有子 slot 的 children_count 之和）
        let count = 1 + self.current_slot().children.iter()
            .map(|c| c.children_count)
            .sum::<usize>();
        self.current_slot().children_count = count;
        self.path.pop();
        self.child_counters.pop();
    }

    /// Register a lifecycle callback on the CURRENT slot, replacing any earlier one at the same
    /// key. It runs when the slot is dropped, never on a plain recomposition.
    pub(super) fn remember_on_forget(&mut self, slot_key: u64, f: Box<dyn FnOnce() + Send>) {
        self.current_slot().forget.insert(slot_key, f);
    }

    /// Drain a whole removed subtree's callbacks into `out`. Moves out of the slot rather than
    /// borrowing, because the caller drops the slot straight after.
    pub(super) fn collect_forgets(mut slot: Slot, out: &mut Vec<Box<dyn FnOnce() + Send>>) {
        out.extend(std::mem::take(&mut slot.forget).into_values());
        for child in std::mem::take(&mut slot.children) {
            SlotTable::collect_forgets(child, out);
        }
    }

    /// Truncate `children` to `len`, handing back the callbacks owned by whatever was dropped.
    /// Every place that drops slots goes through here, so a `remember_on_forget` callback can
    /// never leak just because a branch took a different shape.
    pub(super) fn truncate_children(
        children: &mut Vec<Slot>,
        len: usize,
    ) -> Vec<Box<dyn FnOnce() + Send>> {
        let mut out = Vec::new();
        for dropped in children.drain(len.min(children.len())..) {
            SlotTable::collect_forgets(dropped, &mut out);
        }
        out
    }

    pub(super) fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        self.current_slot().remember(slot_key, init)
    }

    pub(super) fn remember_handle<H: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> H,
    ) -> H {
        self.current_slot().remember_handle(slot_key, init)
    }

    pub(super) fn reset(&mut self) {
        self.path.clear();
        self.child_counters = vec![0];
        self.active_slot_key = 0;
        // 每帧清 visited——物化只收集本帧活跃 slot（结构回退的残留不收集）
        // 同时清空未消费的 desc（panic 残留帧产物），避免下一帧 Clean 复用
        // slot 时 collect_desc_tree 误收集旧 desc 物化错误节点。
        // 正常成功帧的 desc 已被 collect_desc_tree take（=None），此操作无副作用。
        fn clear_visited(slot: &mut Slot) {
            slot.visited = false;
            let _ = slot.desc.take(); // 丢弃 panic 残留的帧产物 desc
            for child in &mut slot.children {
                clear_visited(child);
            }
        }
        clear_visited(&mut self.root_slot);
        // The store is not part of the tree, so it needs the same per-frame clearing by hand: an
        // unvisited/leftover desc there must not be materialized on the strength of a stale frame.
        for store in self.movable_store.values_mut() {
            clear_visited(store);
        }
    }

    pub(super) fn truncate(&mut self) {
        if let Some(&idx) = self.child_counters.first() {
            let orphaned = SlotTable::truncate_children(&mut self.root_slot.children, idx);
            for f in orphaned { f(); }
        }
    }

    pub(super) fn dispose_unplaced_movable(&mut self) {
        fn clear_references(slot: &mut Slot) {
            slot.movable_ref = None;
            for child in &mut slot.children { clear_references(child); }
        }
        fn references(slot: &mut Slot, in_skip: bool, live: &mut crate::layout::node::SlotKeySet) {
            if !slot.visited && !in_skip {
                // Scope residue can survive in the owning tree. Once omitted from an entered branch,
                // its references must never revive when that branch skips in a later composition.
                clear_references(slot);
                return;
            }
            if let Some(id) = slot.movable_ref {
                assert!(live.insert(id), "multiple live placements of one movable handle are not supported; use distinct handles");
                return;
            }
            let child_skip = in_skip || (slot.desc.is_none() && !slot.is_scope && slot.skip_modifier.is_some());
            for child in &mut slot.children { references(child, child_skip, live); }
        }
        let mut live = crate::layout::node::SlotKeySet::default();
        for child in &mut self.root_slot.children { references(child, false, &mut live); }
        // A movable store that is going away can carry lifecycle callbacks (an effect placed inside
        // movable content). Its slots are dropped here, not truncated out of a parent, so collect
        // them explicitly — otherwise that cleanup would silently never run.
        let mut orphaned = Vec::new();
        for (id, store) in self.movable_store.iter_mut() {
            if !live.contains(id) {
                SlotTable::collect_forgets(std::mem::replace(store, Slot::new(0)), &mut orphaned);
            }
        }
        for f in orphaned { f(); }
        self.movable_store.retain(|id, _| live.contains(id));
        self.movable_frame.retain(|id, _| live.contains(id));
    }

    /// 外部标记 slot key 为 dirty（由 State 变化触发）。
    /// 同时递归标记所有祖先 slot，确保父级 start_restartable_group 返回 Enter。
    pub(crate) fn mark_dirty(&mut self, key: u64) {
        self.dirty_keys.insert(key);
        SlotTable::mark_dirty_path_scope(&mut self.root_slot, key);
        // Stored readers are not descendants of their host in the ownership tree. Route their
        // invalidation through each live reference so an otherwise clean host enters the placement.
        let mut invalidated = Vec::new();
        for (&id, store) in &mut self.movable_store {
            if SlotTable::mark_dirty_path_scope(store, key) {
                invalidated.push(id);
            }
        }
        for id in invalidated {
            SlotTable::mark_reference_path(&mut self.root_slot, id);
        }
    }

    pub(super) fn mark_reference_path(slot: &mut Slot, id: u64) -> bool {
        let mut found = slot.movable_ref == Some(id);
        for child in &mut slot.children {
            found |= SlotTable::mark_reference_path(child, id);
        }
        if found { slot.dirty = true; }
        found
    }

    /// Find the slot holding `key`: mark it and all its ancestors dirty. Ancestors are marked so the
    /// walk can REACH the reader (a clean group's body does not run, and its children are never
    /// visited) — Compose instead schedules the invalidated scope directly, without re-running the
    /// groups above it.
    ///
    /// A read that lands on a SCOPE (the common `let v = state.get(); Child { … v … }` shape inside a
    /// `#[composable]` body) marks that scope and its DIRECT children. The body itself always runs, so
    /// its children are what decide Skip/Enter — and winia cannot compare a captured value as a
    /// parameter the way Compose compares a lambda, so without this the child would keep rendering the
    /// value it captured last time. Deeper descendants still skip unless their own dependencies fired:
    /// this is a one-level approximation of Compose's "invalidate the reading scope", not the
    /// previously unbounded subtree re-entry, and a component that needs the whole subtree says so
    /// explicitly with `ctx.mark_subtree_dirty()`.
    pub(super) fn mark_dirty_path_scope(slot: &mut Slot, key: u64) -> bool {
        if slot.key == key {
            slot.dirty = true;
            if slot.is_scope {
                for child in &mut slot.children {
                    child.dirty = true;
                }
            }
            return true;
        }
        for child in &mut slot.children {
            if SlotTable::mark_dirty_path_scope(child, key) {
                slot.dirty = true;
                return true;
            }
        }
        false
    }

    /// 标记 slot 及其所有后代 dirty（scope 失效传播）
    pub(super) fn mark_dirty_subtree(slot: &mut Slot) {
        slot.dirty = true;
        for child in &mut slot.children {
            SlotTable::mark_dirty_subtree(child);
        }
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    pub(super) fn start_scope(&mut self, key: u64) -> SlotStatus {
        let status = self.start_slot(key);
        // 标记当前（刚进入的）slot 为 scope
        self.current_slot().is_scope = true;
        status
    }

    /// Start composing movable content #id at the current position.
    ///
    /// A `movable_ref` slot goes into the tree here. It holds nothing of its own — the descriptor walk
    /// inlines the store's subtree in its place — and composition then switches INTO the store, so the
    /// content's `remember`ed values live somewhere no branch can drop them.
    ///
    /// Pair with [`SlotTable::end_movable`].
    pub(super) fn validate_movable(&self, id: u64) {
        assert!(self.movable_composing.is_none(), "nested movable placement is not supported");
        assert!(self.movable_frame.get(&id) != Some(&self.frame_no),
            "multiple placements of one movable handle are not supported; use distinct handles");
    }

    pub(super) fn begin_movable(&mut self, key: u64, id: u64) {
        self.movable_frame.insert(id, self.frame_no);
        // The reference itself is an ordinary slot in the tree: the position it sits at is where the
        // content's nodes will be parented.
        let status = self.start_slot(key);
        self.current_slot().is_scope = true;
        self.current_slot().movable_ref = Some(id);
        let _ = status;

        // The reference slot is COMPLETE: it holds nothing of its own (the walk inlines the store in
        // its place), so close it before switching into the store. Leaving its path frame open put the
        // rest of the invoking component's composition INSIDE the ref slot — measured: the navigation
        // suite's item lost its label child, because everything composed after the payload landed
        // under a slot the walk ignores (`navigation_bar.rs:659`, `navigation_rail.rs:1396`,
        // "index out of bounds: the len is 2 but the index is 2").
        self.end_slot();

        if let Some(store) = self.movable_store.get_mut(&id) {
            // Calling the retained lambda can supply fresh captured values even without a move.
            // Until lambda/parameter revisions are modeled explicitly, re-enter its readers rather
            // than skipping clean wrappers around the latest payload. Remembered values stay intact.
            SlotTable::mark_dirty_subtree(store);
        }
        let store = self.movable_store.entry(id).or_insert_with(|| {
            let mut store = Slot::new(mix_key(0x6d6f_7661_626c_6501, id));
            store.is_scope = true;
            store.movable_store = Some(id);
            store
        });
        store.visited = true;
        // Save the invoking site's position and hand composition over to the store. The order matters:
        // the save must happen AFTER the ref slot is closed, or the saved path carries the ref's own
        // frame and every later invocation lands one level too deep.
        self.movable_saved_path = std::mem::take(&mut self.path);
        self.movable_saved_counters = std::mem::take(&mut self.child_counters);
        self.child_counters.push(0);
        self.movable_composing = Some(id);
    }

    /// Stop composing movable content, back at the position that invoked it.
    pub(super) fn end_movable(&mut self) {
        assert!(self.path.is_empty(), "unbalanced nodes inside movable content");
        let count = self.child_counters.first().copied().unwrap_or(0);
        // This root is not closed by end_restartable_group. Trim its uncomposed tail here, otherwise
        // descriptor replay resurrects removed conditional/list children.
        let orphaned = {
            let slot = self.current_slot();
            SlotTable::truncate_children(&mut slot.children, count)
        };
        for f in orphaned { f(); }
        self.movable_composing = None;
        self.path = std::mem::take(&mut self.movable_saved_path);
        self.child_counters = std::mem::take(&mut self.movable_saved_counters);
    }

    pub(super) fn abort_movable(&mut self) {
        let id = self.movable_composing.take().expect("a movable placement is active");
        self.movable_store.remove(&id);
        self.movable_frame.remove(&id);
        fn clear_reference(slot: &mut Slot, id: u64) {
            if slot.movable_ref == Some(id) { slot.movable_ref = None; }
            for child in &mut slot.children { clear_reference(child, id); }
        }
        clear_reference(&mut self.root_slot, id);
        self.path = std::mem::take(&mut self.movable_saved_path);
        self.child_counters = std::mem::take(&mut self.movable_saved_counters);
    }

    /// 结束组合 scope
    pub(super) fn end_scope(&mut self) {
        if self.movable_composing.is_some() {
            // Compiler-only scopes have no restartable container to prune their removed children.
            // Their bodies ran, so unvisited descendants are absent, not cached movable state.
            self.current_slot().children.retain(|child| child.visited);
        }
        self.end_slot();
    }

    /// 设置当前（栈顶）slot 是否为 scope（start_node 复用 scope slot 时重置为普通节点）
    pub(super) fn set_current_scope(&mut self, is_scope: bool) {
        self.current_slot().is_scope = is_scope;
    }

    /// 在 slot 树中查找 key 对应的节点，并将其及所有祖先标记 dirty。
    /// 返回 true 表示找到了目标。
    /// 返回当前 slot 在树中的路径（用于 LayoutNode 复用时的 measured_size 查找）
    pub(super) fn current_path(&self) -> &[usize] {
        &self.path
    }

    /// An upper bound on how many layout nodes this composition can materialize: every slot in the
    /// tree, plus the root. Used by `materialize` to size the arena ONCE instead of letting it grow,
    /// and by `collect_live_keys` for the live-key set.
    ///
    /// Computed by SUMMING the root's children's subtree counts, not by reading the root's own
    /// `children_count`: that field is written by `end_slot`, and the root slot is never ended, so it
    /// holds its constructor value (1) forever. A first version of this read it directly and every
    /// "reserve" built on it asked for 2 nodes — which is how a reserve can look like a win (the
    /// hasher it shipped with was doing the work) and still be a no-op (`docs/benchmarks.md`).
    ///
    /// The bound over-counts (scope slots materialize nothing, and a claimed subtree allocates
    /// nothing), which is the safe direction: capacity that goes unused costs address space, not work,
    /// and a reserve against capacity already sufficient is a comparison.
    pub(crate) fn slot_count_bound(&self) -> usize {
        1 + self
            .root_slot
            .children
            .iter()
            .map(|child| child.children_count)
            .sum::<usize>()
    }

    /// Collect slot keys that remain part of the current composition. A skipped
    /// subtree is retained structurally even though its descendants were not visited.
    ///
    /// The set is RESERVED from the slot tree's own node count (`children_count`, maintained by
    /// `end_slot`) rather than growing: a `HashSet` filling up to 4000 entries rehashes everything
    /// several times on the way, and this runs every frame. `SlotKeySet` carries the cheap hasher for
    /// the same reason — both together were worth 2x on this call (`docs/benchmarks.md`).
    ///
    /// Returns how many slots it SKIPPED: slots that are still in the tree but were not composed this
    /// frame and are not inside a skipped subtree. The count is free here (the walk is already
    /// descending past exactly those slots) and it is the evidence for a claim that matters when
    /// somebody wants to replace this set with "every slot in the tree" — see the test that reads it.
    /// Note it only means anything at THIS moment: materialize later consumes the frame's markers
    /// (`desc` and `skip_modifier` are `take()`n), so the same tree read after a frame looks like it
    /// has far more residue than it did during composition.
    pub(super) fn collect_live_keys(&self, out: &mut crate::layout::node::SlotKeySet) -> usize {
        fn visit(
            slot: &Slot,
            out: &mut crate::layout::node::SlotKeySet,
            in_skip: bool,
            skipped: &mut usize,
            stores: &HashMap<u64, Slot>,
        ) {
            if !slot.visited && !in_skip {
                *skipped += 1;
                return;
            }
            out.insert(slot.key);
            if let Some(id) = slot.movable_ref {
                if let Some(store) = stores.get(&id) {
                    let replay = !store.visited;
                    out.insert(store.key);
                    for child in &store.children { visit(child, out, replay, skipped, stores); }
                }
                return;
            }
            let child_in_skip = in_skip
                || (slot.desc.is_none() && !slot.is_scope && slot.skip_modifier.is_some());
            for child in &slot.children {
                visit(child, out, child_in_skip, skipped, stores);
            }
        }

        // An upper bound: every slot in the tree, whether or not it turns out to be live.
        out.reserve(self.slot_count_bound());

        let mut skipped = 0usize;
        for child in &self.root_slot.children {
            visit(child, out, false, &mut skipped, &self.movable_store);
        }
        skipped
    }
}
