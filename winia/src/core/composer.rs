//! 组合引擎 — ComposeCtx 和 Composer
//!
//! ComposeCtx 是 composable 函数的入口点，提供:
//! - remember(): 在组合中持久化状态（重组时返回同一个 State 实例）
//! - next_key(): 生成组合 key，用于 start_node/end_node
//!
//! Composer 管理组合树的生命周期:
//! - SlotTable: 存储组合节点和 remembered 状态
//! - 重组调度: 批处理状态变化，在下一帧重组
//! - Key 管理: 全局唯一 key 计数器

use crate::core::state::{State, clear_current_composer, set_current_composer};
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy};
use crate::modifier::Modifier;
use std::collections::{HashMap, VecDeque};
use std::any::Any;
use std::cell::Cell;

thread_local! { static ACTIVE_SLOT_KEY: Cell<u64> = const { Cell::new(0) }; }

// ── Key ──

/// 组合节点的唯一标识符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(u64);

// ── ComposeCtx ──

/// 组合上下文。
///
/// 传递给每个 composable 函数，用于:
/// - `remember()`: 在组合中持久化状态
/// - `next_key()`: 生成唯一的组合 key
///
/// 注意: ComposeCtx 不实现 Clone —— 组合树是严格树形遍历。
pub struct ComposeCtx<'a> {
    composer: &'a mut Composer,
    /// 当前节点内 remember 调用的序号（用于生成位置 key）
    remember_counter: u32,
}

impl<'a> ComposeCtx<'a> {
    pub(crate) fn new(composer: &'a mut Composer) -> Self {
        // 设置 thread-local 指针，使 State::get() 能追踪依赖
        let ptr = composer as *const Composer as *const ();
        set_current_composer(ptr);

        Self {
            composer,
            remember_counter: 0,
        }
    }

    /// 在组合中记住一个状态。初次调用时执行 init 创建 State，后续重组时返回上次的同一个 State 实例。
    ///
    /// 类似 Compose 的 `remember { mutableStateOf(...) }`。
    ///
    /// # 参数
    /// - `init`: 仅在首次组合时调用，创建初始值
    ///
    /// # 返回
    /// - 始终返回同一个 `State<T>` 实例（重组时通过 Arc clone 返回）
    pub fn remember<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember(slot_key, || State::new(init()))
    }

    /// 生成下一个组合 key（公开 API，用于 start_node）
    pub fn next_key(&mut self) -> u64 {
        self.composer.next_group_key()
    }

    /// 为 remember 调用生成位置 key。
    ///
    /// 位置 key 编码方式: (current_group_key << 32) | remember_counter。
    /// 这保证了同一 composable 函数中的同一 remember 调用在重组时得到相同的 key。
    fn next_remember_key(&mut self) -> u64 {
        let key = ((self.composer.current_group_key as u64) << 32) | (self.remember_counter as u64);
        self.remember_counter += 1;
        key
    }

    /// 访问内部 Composer（pub(crate)，供 ui/layout 模块使用）
    pub(crate) fn composer(&mut self) -> &mut Composer {
        self.composer
    }

    /// 开始一个布局节点（叶子组件如 Text 使用）
    pub fn start_leaf(&mut self, key: u64, modifier: Modifier) {
        self.composer.start_node(key, modifier, None);
    }

    /// 开始一个容器节点（布局组件如 Button/Column 使用）
    pub fn start_container(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) {
        self.composer
            .start_node(key, modifier, Some(Box::new(policy)));
    }

    /// 结束当前节点
    pub fn end_node(&mut self) {
        self.composer.end_node();
    }
}

impl Drop for ComposeCtx<'_> {
    fn drop(&mut self) {
        clear_current_composer();
    }
}

// ── SlotTable ──

/// 组合节点的一个槽位。每个 composable 调用对应一个 Slot。
#[derive(Debug)]
struct Slot {
    key: u64,
    remembered: HashMap<u64, Box<dyn Any>>,
    children: Vec<Slot>,
    /// 重组时是否需要执行（State 变化标记）
    dirty: bool,
}

impl Slot {
    fn new(key: u64) -> Self {
        Self {
            key,
            remembered: HashMap::new(),
            children: Vec::new(),
            dirty: true, // 新创建的 slot 总是 dirty(首次必须执行)
        }
    }

    fn remember<T: Clone + 'static>(
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
}

/// 槽位表 — 组合树的内部数据结构（树形嵌套）
#[derive(Debug)]
pub(crate) struct SlotTable {
    path: Vec<usize>,
    root_slot: Slot,
    child_counters: Vec<usize>,
    /// 当前 compose 期间活跃的 slot key（用于 State→Slot 的脏标记）
    active_slot_key: u64,
    /// 被 State 变化标记为 dirty 的 slot key 集合
    dirty_keys: std::collections::HashSet<u64>,
}

impl SlotTable {
    fn new() -> Self {
        Self {
            path: Vec::new(),
            root_slot: Slot::new(0),
            child_counters: vec![0],
            active_slot_key: 0,
            dirty_keys: std::collections::HashSet::new(),
        }
    }

    fn current_slot(&mut self) -> &mut Slot {
        let mut slot = &mut self.root_slot;
        for &idx in &self.path {
            slot = &mut slot.children[idx];
        }
        slot
    }

    fn start_slot(&mut self, key: u64) {
        let idx = *self.child_counters.last().unwrap_or(&0);
        self.active_slot_key = key;
        ACTIVE_SLOT_KEY.with(|c| c.set(key));
        let is_dirty = self.dirty_keys.remove(&key); // 先取走 dirty 状态
        let parent = self.current_slot();

        if idx < parent.children.len() && parent.children[idx].key == key {
            if !parent.children[idx].dirty && !is_dirty {
                // clean slot，跳过
                self.path.push(idx);
                self.child_counters.last_mut().map(|c| *c += 1);
                self.child_counters.push(0);
                return;
            }
            parent.children[idx].dirty = false;
            self.path.push(idx);
        } else {
            parent.children.truncate(idx);
            parent.children.push(Slot::new(key));
            self.path.push(idx);
        }
        if let Some(last) = self.child_counters.last_mut() { *last += 1; }
        self.child_counters.push(0);
    }

    fn end_slot(&mut self) {
        self.path.pop();
        self.child_counters.pop();
    }

    fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        self.current_slot().remember(slot_key, init)
    }

    fn reset(&mut self) {
        self.path.clear();
        self.child_counters = vec![0];
        self.active_slot_key = 0;
    }

    fn truncate(&mut self) {
        if let Some(&idx) = self.child_counters.first() {
            self.root_slot.children.truncate(idx);
        }
    }

    /// 外部标记 slot key 为 dirty（由 State 变化触发）
    pub(crate) fn mark_dirty(&mut self, key: u64) {
        self.dirty_keys.insert(key);
    }
}

// ── Composer ──

/// 组合引擎。
///
/// 每个窗口/组合根拥有一个 Composer 实例。
/// 负责:
/// - 管理 SlotTable（组合节点的持久化存储）
/// - 调度重组（状态变化 → 标记 dirty → 下一帧批量重组）
/// - 持有全局 key 计数器
pub struct Composer {
    pub(crate) slot_table: SlotTable,
    pub(crate) current_group_key: u32,
    next_group_key_counter: u32,
    pending_recomposition: VecDeque<u64>,
    needs_recomposition: bool,
    layout_nodes: Vec<LayoutNode>,
    node_stack: Vec<usize>,
    layout_root: Option<usize>,
    /// state_id -> slot_keys 依赖映射
    slot_deps: HashMap<u32, Vec<u64>>,
}

impl Composer {
    pub fn new() -> Self {
        Self {
            slot_table: SlotTable::new(),
            current_group_key: 0,
            next_group_key_counter: 1,
            pending_recomposition: VecDeque::new(),
            needs_recomposition: false,
            layout_nodes: Vec::new(),
            node_stack: Vec::new(),
            layout_root: None,
            slot_deps: HashMap::new(),
        }
    }

    /// 分配下一个全局唯一的 group key
    pub fn next_group_key(&mut self) -> u64 {
        let key = self.next_group_key_counter as u64;
        self.next_group_key_counter += 1;
        key
    }

    /// 在组合树中开始一个节点（由组件的 build 方法调用）
    pub fn start_node(&mut self, key: u64, modifier: Modifier, policy: Option<Box<dyn MeasurePolicy>>) {
        self.current_group_key = key as u32;
        self.slot_table.start_slot(key);

        // 创建对应的 LayoutNode
        let node = LayoutNode::new(modifier, policy);
        let index = self.layout_nodes.len();
        self.layout_nodes.push(node);
        self.node_stack.push(index);
    }

    /// 结束当前节点：出栈并建立父子关系
    pub fn end_node(&mut self) {
        self.slot_table.end_slot();

        if let Some(child_idx) = self.node_stack.pop() {
            if let Some(&parent_idx) = self.node_stack.last() {
                // 有父节点：将当前节点作为子节点添加
                // SAFETY: parent_idx 和 child_idx 都有效
                let child = self.layout_nodes.remove(child_idx);
                self.layout_nodes[parent_idx].add_child(child);
                // 调整后续索引
                for idx in self.node_stack.iter_mut() {
                    if *idx > child_idx {
                        *idx -= 1;
                    }
                }
            } else {
                // 根节点
                self.layout_root = Some(child_idx);
            }
        }
    }

    /// 执行组合：运行 content 闭包，构建/更新组合树和布局树。
    pub fn compose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        self.slot_table.reset();
        self.current_group_key = 0;
        self.next_group_key_counter = 1;
        self.layout_nodes.clear();
        self.node_stack.clear();
        self.layout_root = None;

        // 消费 global dirty → 标记对应 slot 为脏
        let pending = crate::core::state::take_pending_states();
        for state_id in &pending {
            if let Some(keys) = self.slot_deps.get(state_id) {
                for &k in keys {
                    self.slot_table.mark_dirty(k);
                }
            }
        }
        self.slot_deps.clear(); // 清空旧依赖，下面会重新收集

        crate::core::state::set_dependency_registrar(move |state_id, _| {
            let key = ACTIVE_SLOT_KEY.with(|c| c.get());
            crate::core::state::record_dep(state_id, key);
        });

        {
            let ctx = &mut ComposeCtx::new(self);
            content(ctx);
        }

        self.slot_table.truncate();

        // 将本帧收集的依赖写入 slot_deps
        for (state_id, slot_key) in crate::core::state::take_recorded_deps() {
            self.slot_deps.entry(state_id).or_default().push(slot_key);
        }
    }

    /// 返回 LayoutNode 树的根节点引用
    pub fn layout_root(&self) -> Option<&LayoutNode> {
        self.layout_root.map(|idx| &self.layout_nodes[idx])
    }

    /// 返回 LayoutNode 树的根节点可变引用
    pub fn layout_root_mut(&mut self) -> Option<&mut LayoutNode> {
        self.layout_root.map(|idx| &mut self.layout_nodes[idx])
    }

    /// 执行整棵布局树的 measure + place
    pub fn layout(&mut self, root_constraints: Constraints) {
        if let Some(root_idx) = self.layout_root {
            let root = &mut self.layout_nodes[root_idx];
            let (_size, _placements) = crate::layout::measure_node(root, root_constraints);
            root.measured_size = _size;
        }
    }

    /// 请求重组（由 State 变化触发）。
    pub fn request_recomposition(&mut self, _key: u64) {
        self.needs_recomposition = true;
    }

    /// 执行待处理的重组。若无待处理则跳过。
    pub fn recompose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        if !self.needs_recomposition && self.pending_recomposition.is_empty() {
            return;
        }

        // 处理队列中的待重组节点（当前简化：全量重组）
        self.pending_recomposition.clear();
        self.needs_recomposition = false;
        self.compose(content);
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composer_new() {
        let mut composer = Composer::new();
        let key = composer.next_group_key();
        assert_eq!(key, 1);
        let key2 = composer.next_group_key();
        assert_eq!(key2, 2);
    }

    #[test]
    fn test_remember_persistence() {
        let mut composer = Composer::new();

        // 首次组合
        composer.compose(|ctx| {
            let state: State<i32> = ctx.remember(|| 42);
            assert_eq!(state.get(), 42);
            state.set(100);
        });

        // 重组：同一个 remember 应返回同一个 State，值保持 100
        composer.recompose(|ctx| {
            let state: State<i32> = ctx.remember(|| 42);
            assert_eq!(state.get(), 100);
        });
    }

    #[test]
    fn test_remember_multiple() {
        let mut composer = Composer::new();

        composer.compose(|ctx| {
            let a: State<i32> = ctx.remember(|| 1);
            let b: State<String> = ctx.remember(|| "hello".to_string());
            a.set(10);
            b.set("world".to_string());
        });

        composer.recompose(|ctx| {
            let a: State<i32> = ctx.remember(|| 99);
            let b: State<String> = ctx.remember(|| "none".to_string());
            assert_eq!(a.get(), 10);
            assert_eq!(b.get(), "world");
        });
    }

    #[test]
    fn test_conditional_remember() {
        let mut composer = Composer::new();
        let show_extra = State::new(false);

        composer.compose(|ctx| {
            let always: State<i32> = ctx.remember(|| 1);
            if show_extra.get() {
                let extra: State<i32> = ctx.remember(|| 2);
                assert_eq!(extra.get(), 2);
            }
            assert_eq!(always.get(), 1);
        });
    }
}
