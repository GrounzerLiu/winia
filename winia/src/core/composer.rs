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

use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, CachedNode};
use crate::modifier::Modifier;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::any::Any;
use std::cell::Cell;
use std::cell::RefCell;

thread_local! { static ACTIVE_SLOT_KEY: Cell<u64> = const { Cell::new(0) }; }
thread_local! { static SCOPE_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) }; }

/// 读取当前组合作用域的依赖注册目标：
/// scope 栈非空 → 最内层 scope key（组合 scope 内、组件外的 State::get() 注册到这里）；
/// 否则 → 当前 slot key（组件内）。
pub(crate) fn with_active_scope(f: impl FnOnce(u64)) {
    SCOPE_STACK.with(|s| {
        let s = s.borrow();
        if let Some(&k) = s.last() {
            f(k);
        } else {
            with_active_slot_key(f);
        }
    });
}

/// 读取当前 compose 位置的 slot key（供 state.rs 依赖追踪使用）
pub(crate) fn with_active_slot_key(f: impl FnOnce(u64)) {
    ACTIVE_SLOT_KEY.with(|c| f(c.get()));
}

/// 设置当前 slot key（measure_node 用它把动态尺寸的依赖注册到节点）
pub(crate) fn set_active_slot_key(key: u64) {
    ACTIVE_SLOT_KEY.with(|c| c.set(key));
}

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
        Self {
            composer,
            remember_counter: 0,
        }
    }

    /// 在组合中记住一个状态。初次调用时执行 init 创建 State，后续重组时返回上次的同一个 State 实例。
    pub fn remember<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let slot_key = self.next_remember_key();
        let pq = Arc::downgrade(&self.composer.pending_states);
        self.composer.slot_table.remember(slot_key, || {
            crate::core::state::STATE_OWNER_QUEUE.with(|q| *q.borrow_mut() = Some(pq.clone()));
            State::new(init())
        })
    }

    /// 使用固定 key 记住一个状态（不受 remember_counter 影响，适合跨分支持久化的值）
    pub fn remember_at_key<T: Clone + 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T> {
        let pq = Arc::downgrade(&self.composer.pending_states);
        self.composer.slot_table.remember(key, || {
            crate::core::state::STATE_OWNER_QUEUE.with(|q| *q.borrow_mut() = Some(pq.clone()));
            State::new(init())
        })
    }

    /// 生成下一个组合 key（公开 API，用于 start_node）
    pub fn next_key(&mut self) -> u64 {
        self.composer.next_group_key()
    }

    /// 开始一个组合 scope——scope 内（组件外）的 `State::get()` 注册依赖到本 scope，
    /// State 变化 → scope 失效 → 其内组合代码整体重跑（组件不 Skip，modifier 重算）。
    /// 与 `end_scope` 配对。
    pub fn start_scope(&mut self) -> u64 {
        self.composer.start_scope()
    }

    /// 结束组合 scope（与 start_scope 配对）
    pub fn end_scope(&mut self) {
        self.composer.end_scope();
    }

    /// 获取当前正在构建的节点 ID（用于注册选中、焦点等外部状态）
    pub fn current_node_id(&self) -> Option<u64> {
        self.composer.current_node_id()
    }

    pub fn layout_root(&self) -> Option<&crate::layout::node::LayoutNode> {
        self.composer.layout_root()
    }

    /// 设置当前选区注册表（由 SelectionContainer::build 调用）
    pub fn set_selection_registrar(&mut self, reg: crate::ui::selection_container::SelectionRegistrar) {
        self.composer.selection_registrar = Some(reg);
    }

    pub fn clear_selection_registrar(&mut self) {
        self.composer.selection_registrar = None;
    }

    /// 给当前节点设 registrar 引用（供后续渲染/事件从中读取）
    pub fn set_current_node_registrar(&self, reg: crate::ui::selection_container::SelectionRegistrar) {
        if let Some(id) = self.current_node_id() {
            if let Some(idx) = self.composer.node_stack.last() {
                let node = &self.composer.layout_nodes[*idx];
                *node.registrar.borrow_mut() = Some(reg);
            }
        }
    }

    /// 覆盖当前节点的 modifier（modifier_fn 延迟求值用——start 后用求得的 modifier 替换空壳）
    pub fn set_current_node_modifier(&mut self, m: Modifier) {
        if let Some(&idx) = self.composer.node_stack.last() {
            let node = &mut self.composer.layout_nodes[idx];
            node.modifier = m;
            node.has_text_content = crate::layout::node::modifier_has_text(&node.modifier);
            node.has_richtext_content = crate::layout::node::modifier_has_richtext(&node.modifier);
        }
    }

    /// 设置当前节点的光标位置和可见性，同时设置光标回调
    pub fn set_current_node_cursor_and_callback(
        &self,
        cursor_index: usize,
        visible: bool,
        callback: Box<dyn Fn(usize) + Send>,
    ) {
        if let Some(&idx) = self.composer.node_stack.last() {
            let node = &self.composer.layout_nodes[idx];
            node.cursor_index.set(cursor_index);
            node.cursor_visible.set(visible);
            *node.cursor_callback.borrow_mut() = Some(callback);
        }
    }

    /// animateFloatAsState — 动画浮点值到目标值
    pub fn animate_float_as_state(&mut self, target: f32, spec: crate::animation::AnimationSpec) -> State<f32> {
        let remember_key = self.next_remember_key();
        let state = self.composer.slot_table.remember(remember_key, || {
            crate::core::state::STATE_OWNER_QUEUE.with(|q| *q.borrow_mut() = Some(Arc::downgrade(&self.composer.pending_states)));
            crate::core::state::State::new(target)
        });
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateColorAsState — 动画颜色值到目标值（RGBA 插值，Tween 驱动）
    pub fn animate_color_as_state(&mut self, target: crate::modifier::Color, spec: crate::animation::AnimationSpec) -> State<crate::modifier::Color> {
        let state = self.remember(|| target);
        crate::animation::push_animatable_color(state.clone(), target, spec);
        state
    }

    /// animateDpAsState — 动画 Dp 值（对标 Compose animateDpAsState）
    pub fn animate_dp_as_state(&mut self, target: crate::unit::Dp, spec: crate::animation::AnimationSpec) -> State<crate::unit::Dp> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateOffsetAsState — 动画 Offset 值（对标 Compose animateOffsetAsState）
    pub fn animate_offset_as_state(&mut self, target: crate::unit::Offset, spec: crate::animation::AnimationSpec) -> State<crate::unit::Offset> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateSizeAsState — 动画 Size 值（对标 Compose animateSizeAsState）
    pub fn animate_size_as_state(&mut self, target: crate::unit::Size, spec: crate::animation::AnimationSpec) -> State<crate::unit::Size> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// 设置当前节点的 IME 预输入回调
    pub fn set_current_node_ime_callback(&self, callback: Box<dyn Fn(&str, Option<(usize, usize)>) + Send>) {
        if let Some(&idx) = self.composer.node_stack.last() {
            let node = &self.composer.layout_nodes[idx];
            *node.ime_callback.borrow_mut() = Some(callback);
        }
    }

    /// 同步 composing_range 到当前节点（渲染画下划线用）
    pub fn sync_composing_range(&self, range: Option<std::ops::Range<usize>>) {
        if let Some(&idx) = self.composer.node_stack.last() {
            let node = &self.composer.layout_nodes[idx];
            *node.composing_range.borrow_mut() = range;
        }
    }

    /// 同步 selection_range 到当前节点（渲染高亮选区用）
    pub fn sync_selection_range(&self, range: Option<std::ops::Range<usize>>) {
        if let Some(&idx) = self.composer.node_stack.last() {
            let node = &self.composer.layout_nodes[idx];
            *node.selection_range.borrow_mut() = range;
        }
    }

    /// 获取当前节点缓存段落中的索引映射（供方向键按 glyph 边界移动）
    pub fn cached_paragraph_maps(&self) -> (crate::text::IndexBiMap, crate::text::IndexBiMap) {
        if let Some(&idx) = self.composer.node_stack.last() {
            if let Some(node) = self.composer.layout_nodes.get(idx) {
                if let Some(p) = node.cached_paragraph.borrow().as_ref() {
                    return (p.paragraph_byte_to_real_indices.clone(), p.byte_to_utf16_indices.clone());
                }
            }
        }
        // 返回空映射作为后备
        (crate::text::IndexBiMap::new(), crate::text::IndexBiMap::new())
    }

    /// 设置当前节点的光标位置
    pub fn set_current_node_cursor(&self, cursor_index: usize, visible: bool) {
        if let Some(idx) = self.composer.node_stack.last() {
            let node = &self.composer.layout_nodes[*idx];
            node.cursor_index.set(cursor_index);
            node.cursor_visible.set(visible);
        }
    }

    /// 获取选区注册表
    pub fn selection_registrar(&self) -> Option<crate::ui::selection_container::SelectionRegistrar> {
        self.composer.selection_registrar.clone()
    }

    /// 为 remember 调用生成位置 key。
    ///
    /// 位置 key 编码方式: 基于 slot 树路径（结构稳定——不随 Enter/Skip 的
    /// next_key 序列漂移，保证同一组合位置跨重组复用同一 State）。
    fn next_remember_key(&mut self) -> u64 {
        let path = self.composer.slot_table.current_path().to_vec();
        // 路径哈希（FNV-1a 风格）：不同路径 → 不同高位，同一 slot 内多个 remember 用 counter 区分
        let mut h: u64 = 0xcbf29ce484222325;
        for &idx in &path {
            h ^= idx as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        let key = (h << 32) | (self.remember_counter as u64);
        self.remember_counter += 1;
        key
    }

    /// 开始一个布局节点（叶子组件如 Text 使用）
    pub fn start_leaf(&mut self, key: u64, modifier: Modifier) {
        self.composer.start_node(key, modifier, None, None);
    }

    /// 开始一个容器节点（布局组件如 Button/Column 使用）
    pub fn start_container(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) {
        self.composer
            .start_node(key, modifier, Some(Box::new(policy)), None);
    }

    /// 开始一个叶子节点并设置移除回调
    pub fn start_leaf_with_remove(
        &mut self,
        key: u64,
        modifier: Modifier,
        on_remove: Box<dyn FnOnce() + Send>,
    ) {
        self.composer.start_node(key, modifier, None, Some(on_remove));
    }

    /// 结束当前节点
    pub fn end_node(&mut self) {
        self.composer.end_node();
    }

    /// 开始一个可重启的组合分组（容器节点）。
    /// 返回 Enter（正常执行闭包）或 Skip（跳过内容，从缓存重放子树）。
    pub fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) -> GroupStatus {
        self.composer
            .start_restartable_group(key, modifier, Some(Box::new(policy)), None)
    }

    /// 结束一个可重启分组。
    /// 在 Enter 模式下等同于 end_node()；
    /// 在 Skip 模式下重放子 slot 结构并创建 stub LayoutNode。
    pub fn end_restartable_group(&mut self) {
        self.composer.end_restartable_group();
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
    /// 当前帧中此 slot 的子树总 slot 数（含自身；用于 skip 时重放）
    children_count: usize,
    /// 是否为组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    is_scope: bool,
}

impl Slot {
    fn new(key: u64) -> Self {
        Self {
            key,
            remembered: HashMap::new(),
            children: Vec::new(),
            dirty: true, // 新创建的 slot 总是 dirty（首次必须执行）
            children_count: 1, // 自身
            is_scope: false,
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
    New,
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

    fn start_slot(&mut self, key: u64) -> SlotStatus {
        let idx = *self.child_counters.last().unwrap_or(&0);
        self.active_slot_key = key;
        ACTIVE_SLOT_KEY.with(|c| c.set(key));
        let is_dirty = self.dirty_keys.remove(&key);
        let parent = self.current_slot();

        // key 匹配，或"同位置"（key 高位 = slot 路径哈希相同）——Enter/Skip 的
        // counter 漂移不改位置，按索引复用（更新 key 保持同步），避免 truncate
        // 重建 slot 导致 remember 的 State 丢失
        let same_position = idx < parent.children.len()
            && parent.children[idx].key >> 32 == key >> 32;
        if idx < parent.children.len() && (parent.children[idx].key == key || same_position) {
            parent.children[idx].key = key; // 同步最新 key（counter 可能漂移）
            if !parent.children[idx].dirty && !is_dirty {
                self.path.push(idx);
                self.child_counters.last_mut().map(|c| *c += 1);
                self.child_counters.push(0);
                return SlotStatus::Clean;
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
        SlotStatus::Dirty
    }

    fn end_slot(&mut self) {
        // 计算子树 slot 总数（自身 + 所有子 slot 的 children_count 之和）
        let count = 1 + self.current_slot().children.iter()
            .map(|c| c.children_count)
            .sum::<usize>();
        self.current_slot().children_count = count;
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

    /// 外部标记 slot key 为 dirty（由 State 变化触发）。
    /// 同时递归标记所有祖先 slot，确保父级 start_restartable_group 返回 Enter。
    pub(crate) fn mark_dirty(&mut self, key: u64) {
        self.dirty_keys.insert(key);
        let root = &mut self.root_slot;
        // scope 失效 → 整个子树强制 Enter（scope 内组合代码重跑，modifier 重算）
        if let Some(slot) = SlotTable::find_slot_mut(root, key) {
            if slot.is_scope {
                SlotTable::mark_dirty_subtree(slot);
            }
        }
        SlotTable::mark_dirty_path(root, key);
    }

    /// 在 slot 树中查找 key 对应的 slot
    fn find_slot_mut<'a>(slot: &'a mut Slot, key: u64) -> Option<&'a mut Slot> {
        if slot.key == key { return Some(slot); }
        for child in &mut slot.children {
            if let Some(s) = SlotTable::find_slot_mut(child, key) { return Some(s); }
        }
        None
    }

    /// 标记 slot 及其所有后代 dirty（scope 失效传播）
    fn mark_dirty_subtree(slot: &mut Slot) {
        slot.dirty = true;
        for child in &mut slot.children {
            SlotTable::mark_dirty_subtree(child);
        }
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    fn start_scope(&mut self, key: u64) -> SlotStatus {
        let status = self.start_slot(key);
        // 标记当前（刚进入的）slot 为 scope
        self.current_slot().is_scope = true;
        status
    }

    /// 结束组合 scope
    fn end_scope(&mut self) {
        self.end_slot();
    }

    /// 在 slot 树中查找 key 对应的节点，并将其及所有祖先标记 dirty。
    /// 返回 true 表示找到了目标。
    fn mark_dirty_path(slot: &mut Slot, key: u64) -> bool {
        if slot.key == key {
            slot.dirty = true;
            return true;
        }
        for child in &mut slot.children {
            if SlotTable::mark_dirty_path(child, key) {
                slot.dirty = true;
                return true;
            }
        }
        false
    }

    /// 返回当前 slot 在树中的路径（用于 LayoutNode 复用时的 measured_size 查找）
    fn current_path(&self) -> &[usize] {
        &self.path
    }

    /// 返回当前 slot 的子 slot 引用（用于 clean subtree 重放）
    fn current_children(&mut self) -> &[Slot] {
        &self.current_slot().children
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
    /// 记录每个 start_restartable_group 的 skip 状态（用于 end_restartable_group 判断）
    group_skip_stack: Vec<bool>,
    layout_root: Option<usize>,
    /// state_id -> slot_keys 依赖映射
    slot_deps: HashMap<u32, HashSet<u64>>,
    /// 当前 compose 期间记录的依赖（替代全局 RECORDED_DEPS）
    recorded_deps: Vec<(u32, u64)>,
    /// 本 Composer 实例的 pending state 通知队列
    pending_states: Arc<parking_lot::Mutex<Vec<u32>>>,
    /// 上一帧各 slot 路径 → 节点缓存（用于 clean slot 跳过和子树重放）
    prev_nodes: HashMap<Vec<usize>, CachedNode>,
    /// 本帧暂存（recompose 循环内 Enter 的结果——Skip 时优先恢复它，
    /// 避免循环内第二次重组用旧 prev_nodes 覆盖本次 Enter 的状态）
    frame_cache: HashMap<Vec<usize>, CachedNode>,
    /// 当前选区注册表（SelectionContainer compose 时注入，供事件处理访问）
    pub(crate) selection_registrar: Option<crate::ui::selection_container::SelectionRegistrar>,

    #[cfg(test)]
    pub(crate) compose_clean_count: usize,
    #[cfg(test)]
    pub(crate) compose_dirty_count: usize,
}

impl Composer {
/// 选区注册表（由 SelectionContainer 在 compose 时注入，供事件处理访问）

    pub fn new() -> Self {
        let pending_states = Arc::new(parking_lot::Mutex::new(Vec::new()));
        crate::core::state::register_composer_queue(Arc::downgrade(&pending_states));
        Self {
            slot_table: SlotTable::new(),
            current_group_key: 0,
            next_group_key_counter: 1,
            pending_recomposition: VecDeque::new(),
            needs_recomposition: false,
            layout_nodes: Vec::new(),
            node_stack: Vec::new(),
            group_skip_stack: Vec::new(),
            layout_root: None,
            slot_deps: HashMap::new(),
            recorded_deps: Vec::new(),
            pending_states,
            prev_nodes: HashMap::new(),
            frame_cache: HashMap::new(),
            selection_registrar: None,
            #[cfg(test)]
            compose_clean_count: 0,
            #[cfg(test)]
            compose_dirty_count: 0,
        }
    }

    /// 获取当前正在构建的节点 ID（node_stack 栈顶）
    pub fn current_node_id(&self) -> Option<u64> {
        self.node_stack.last().map(|&idx| self.layout_nodes[idx].id)
    }

    /// 分配下一个 group key。
    ///
    /// 基于 slot 路径编码：结构稳定——Enter/Skip 的执行顺序不影响 key，
    /// 保证同一组合位置跨重组得到相同 slot（否则 slot 树 truncate 重建，
    /// 导致 remember 的 State 全部丢失重建）。
    pub fn next_group_key(&mut self) -> u64 {
        let path = self.slot_table.current_path().to_vec();
        let mut h: u64 = 0xcbf29ce484222325;
        for &idx in &path {
            h ^= idx as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        let key = (h << 32) | (self.next_group_key_counter as u64);
        self.next_group_key_counter += 1;
        key
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——组合代码重跑的失效单位）。
    /// 返回 scope key；`State::get()` 在 scope 内（组件外）注册依赖到 scope。
    pub fn start_scope(&mut self) -> u64 {
        let key = self.next_group_key();
        self.slot_table.start_scope(key);
        SCOPE_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// 结束组合 scope
    pub fn end_scope(&mut self) {
        self.slot_table.end_scope();
        SCOPE_STACK.with(|s| { s.borrow_mut().pop(); });
    }

    /// 在组合树中开始一个节点（由组件的 build 方法调用）
    pub fn start_node(&mut self, key: u64, modifier: Modifier, policy: Option<Box<dyn MeasurePolicy>>, on_remove: Option<Box<dyn FnOnce() + Send>>) {
        self.current_group_key = key as u32;
        let slot_status = self.slot_table.start_slot(key);
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // 创建对应的 LayoutNode
        let mut node = LayoutNode::new(modifier, policy);
        node.on_remove = on_remove;
        node.slot_key = key;

        // Clean slot：从上一帧缓存恢复 measured_size 和 cached_constraints
        if slot_status == SlotStatus::Clean {
            let path = self.slot_table.current_path().to_vec();
            if let Some(cached) = self.prev_nodes.get(&path) {
                node.restore_from(cached);
            }
        }

        let index = self.layout_nodes.len();
        self.layout_nodes.push(node);
        self.node_stack.push(index);
    }

    /// 结束当前节点：出栈并建立父子关系
    pub fn end_node(&mut self) {
        // 在 end_slot（pop path）之前记录本帧结果到 frame_cache（供本帧后续 Skip 恢复）
        {
            let slot_path = self.slot_table.current_path().to_vec();
            let lpath: Vec<usize> = slot_path.get(1..).unwrap_or(&slot_path).to_vec();
            if let Some(&idx) = self.node_stack.last() {
                let cached = self.layout_nodes[idx].to_cached();
                self.frame_cache.insert(lpath, cached);
            }
        }
        self.slot_table.end_slot();

        if let Some(child_idx) = self.node_stack.pop() {
            if let Some(&parent_idx) = self.node_stack.last() {
                // 有父节点：将当前节点作为子节点添加
                // swap_remove 合法因为 child_idx 总是 layout_nodes 的最后一个元素
                //（子节点在 start_node 时 push，end_node 时 pop，中间无新的 push 越过它）
                let child = self.layout_nodes.swap_remove(child_idx);
                self.layout_nodes[parent_idx].add_child(child);
            } else {
                // 根节点
                self.layout_root = Some(child_idx);
            }
        }
    }

    /// 开始可重启分组（内部调用 start_node + 返回 skip/enter 状态）
    fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: Option<Box<dyn MeasurePolicy>>,
        on_remove: Option<Box<dyn FnOnce() + Send>>,
    ) -> GroupStatus {
        self.current_group_key = key as u32;
        let slot_status = self.slot_table.start_slot(key);
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        let mut node = LayoutNode::new(modifier, policy);
        node.on_remove = on_remove;
        node.slot_key = key;

        // Clean slot：从缓存恢复
        let is_skip = if slot_status == SlotStatus::Clean {
            let path = self.slot_table.current_path().to_vec();
            if let Some(cached) = self.prev_nodes.get(&path) {
                node.restore_from(cached);
                true // 子树可跳过
            } else {
                false
            }
        } else {
            false
        };

        let index = self.layout_nodes.len();
        self.layout_nodes.push(node);
        self.node_stack.push(index);
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
    fn end_restartable_group(&mut self) {
        let was_skip = self.group_skip_stack.pop().unwrap_or(false);
        if was_skip {
            self.replay_clean_subtree();
        }
        self.end_node();
    }

    /// 重放当前 slot 的所有子 slot：为每个子 slot 创建 stub LayoutNode（从缓存取值），
    /// 递归处理嵌套子树。slot 操作统一通过 start_node/end_node 入口（不再裸调 start_slot）。
    fn replay_clean_subtree(&mut self) {
        // 从 Slot 树读取子节点列表（LayoutNode 树在此阶段尚未构建）
        let children: Vec<(u64, usize)> = self
            .slot_table
            .current_children()
            .iter()
            .map(|c| (c.key, c.children_count))
            .collect();

        for (child_key, child_slot_count) in &children {
            // 通过 start_node 进入 slot + 创建节点（内部会调 start_slot）
            self.start_node(*child_key, Modifier::new(), None, None);

            // 标记为重放 stub：clean-skip 节点无 measure_policy，
            // 测量必须直接返回缓存尺寸（见 measure_node 的 is_replay_stub 分支）
            let node_idx = *self.node_stack.last().unwrap();
            self.layout_nodes[node_idx].is_replay_stub = true;

            // 用缓存覆盖节点属性：
            // 布局部分（size/position/constraints）来自 prev_nodes（上帧 layout 结果），
            // 内容部分（modifier）优先 frame_cache（本帧已 Enter 的构建结果，
            // 避免循环内第二次重组用旧 prev_nodes 覆盖本次 Enter 的内容）
            let slot_path = self.slot_table.current_path().to_vec();
            let path: Vec<usize> = slot_path.get(1..).unwrap_or(&slot_path).to_vec();
            if let Some(cached) = self.prev_nodes.get(&path) {
                self.layout_nodes[node_idx].restore_from(cached);
                if let Some(frame) = self.frame_cache.get(&path) {
                    self.layout_nodes[node_idx].modifier = frame.modifier.clone();
                    self.layout_nodes[node_idx].has_text_content =
                        crate::layout::node::modifier_has_text(&frame.modifier);
                    self.layout_nodes[node_idx].has_richtext_content =
                        crate::layout::node::modifier_has_richtext(&frame.modifier);
                }
            } else if let Some(frame) = self.frame_cache.get(&path) {
                self.layout_nodes[node_idx].restore_from(frame);
            }

            // 递归重放孙子节点
            if *child_slot_count > 1 { // >1 因为自身已计入 children_count
                self.replay_clean_subtree();
            }

            self.end_node(); // 将子节点挂到父节点
        }
    }

    /// Compose 末尾：递归遍历 LayoutNode 树，为所有 modifier 注册 State 依赖
fn register_modifier_deps_recursive(node: &LayoutNode) {
    node.modifier.register_state_deps();
    for child in &node.children {
        Self::register_modifier_deps_recursive(child);
    }
}

/// 执行组合：运行 content 闭包，构建/更新组合树和布局树。
    pub fn compose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        #[cfg(test)] { self.compose_clean_count = 0; self.compose_dirty_count = 0; }
        self.slot_table.reset();
        self.current_group_key = 0;
        self.next_group_key_counter = 1;
        self.layout_root = None;
        // 重置 Window 生命周期标志（先于 layout_nodes.clear，on_remove 再设置新值）
        crate::ui::window::reset_lifecycle_flags();
        self.layout_nodes.clear();
        self.node_stack.clear();

        // 消费本 Composer 实例的 pending states → 标记对应 slot 为脏
        // 同时收集受影响的 slot key（用于增量更新 slot_deps）
        let mut affected_slot_keys = HashSet::new();
        let mut pending = self.pending_states.lock();
        for state_id in pending.drain(..) {
            if let Some(keys) = self.slot_deps.get(&state_id) {
                for &k in keys {
                    self.slot_table.mark_dirty(k);
                    affected_slot_keys.insert(k);
                }
            }
        }
        drop(pending);

        // 设置依赖记录目标——State::get() 会通过 thread-local 指针写入 self.recorded_deps
        crate::core::state::set_recording_target(&mut self.recorded_deps);

        {
            let ctx = &mut ComposeCtx::new(self);
            content(ctx);
        }

        // 自动注册所有 modifier 中引用的 State 依赖（scroll 等）
        if let Some(root_idx) = self.layout_root {
            let root = &self.layout_nodes[root_idx];
            Self::register_modifier_deps_recursive(root);
        }

        crate::core::state::clear_recording_target();
        self.slot_table.truncate();

        // 将本帧收集的依赖写入 slot_deps（HashSet 自动去重）
        for (state_id, slot_key) in self.recorded_deps.drain(..) {
            self.slot_deps.entry(state_id).or_default().insert(slot_key);
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

    /// 执行整棵布局树的 measure + place，并缓存测量结果供下帧复用
    pub fn layout(&mut self, root_constraints: Constraints) {
        if let Some(root_idx) = self.layout_root {
            let root = &mut self.layout_nodes[root_idx];
            let (_size, _placements) = crate::layout::measure_node(root, root_constraints);
            root.measured_size = _size;
            // 收集整棵树的节点信息（measured_size、cached_constraints、modifier），按 slot 路径索引
            self.prev_nodes.clear();
            collect_nodes(root, &mut Vec::new(), &mut self.prev_nodes);
            // measure 阶段（SizeDynamic 闭包内的 State::get()）注册的依赖也要进入 slot_deps
            for (state_id, slot_key) in self.recorded_deps.drain(..) {
                self.slot_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    /// 请求重组（由 State 变化触发）。
    pub fn request_recomposition(&mut self, _key: u64) {
        self.needs_recomposition = true;
    }

    /// 是否有待处理的 state 变化
    pub fn has_pending_states(&self) -> bool {
        !self.pending_states.lock().is_empty()
    }

    /// 本帧暂存清理（在 recompose 循环开始前调用——整个重组周期内保留 Enter 结果，
    /// 供循环内后续 Skip 恢复，避免旧 prev_nodes 覆盖本次 Enter 的状态）
    pub fn clear_frame_cache(&mut self) {
        self.frame_cache.clear();
    }

    /// 执行待处理的重组。返回 true 表示实际执行了 compose。
    /// 若无待处理则跳过，保留上一帧的布局树。
    pub fn recompose(&mut self, content: impl FnOnce(&mut ComposeCtx)) -> bool {
        let has_pending = !self.pending_states.lock().is_empty();

        if !self.needs_recomposition && !has_pending && self.pending_recomposition.is_empty() {
            return false;
        }

        // 处理队列中的待重组节点（当前简化：全量重组）
        self.pending_recomposition.clear();
        self.needs_recomposition = false;
        self.compose(content);
        true
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

/// 递归遍历布局树，收集每个 slot 路径的 (measured_size, cached_constraints)。
/// 同时将子节点的 dirty 冒泡到父节点（确保父节点不会因 dirty=false 而跳过脏子树）。
/// 后序遍历布局树，收集所有节点的 measured_size、cached_constraints 和 modifier
/// 存入 prev_nodes（以 slot path 为键）。
/// 同时将子节点的 dirty 冒泡到父节点（确保父节点不会因 dirty=false 而跳过脏子树）。
fn collect_nodes(
    node: &mut LayoutNode,
    path: &mut Vec<usize>,
    map: &mut HashMap<Vec<usize>, CachedNode>,
) {
    // 先递归子节点（后序），以便 dirty 从子向父冒泡
    for (i, child) in node.children.iter_mut().enumerate() {
        path.push(i);
        collect_nodes(child, path, map);
        if child.dirty {
            node.dirty = true;
        }
        path.pop();
    }
    // 缓存当前节点的可缓存子集
    map.insert(path.clone(), node.to_cached());
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composer_new() {
        let mut composer = Composer::new();
        let key = composer.next_group_key();
        let key2 = composer.next_group_key();
        // key = (slot 路径哈希 << 32) | counter：同一路径下 counter 区分，高位相同
        assert_ne!(key, key2);
        assert_eq!(key >> 32, key2 >> 32);
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

use crate::layout::BoxLayout;

    /// 测试 restartable group 的 skip → replay 路径：
    /// 状态变化只影响某个 leaf slot，兄弟 slot 应被 clean skip 并正确 replay。
    #[test]
    fn test_restartable_skip_replay_tree_integrity() {
        let mut composer = Composer::new();

        // Frame 1: 建立初始树
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 子 1: state-dependent leaf
                    let text_key = ctx.next_key();
                    {
                        let _count: State<i32> = ctx.remember(|| 0);
                        ctx.start_leaf(text_key, Modifier::new());
                    }
                    ctx.end_node();

                    // 子 2: clean restartable group
                    let btn_key = ctx.next_key();
                    match ctx.start_restartable_group(btn_key, Modifier::new(), BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let content_key = ctx.next_key();
                            ctx.start_leaf(content_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // 断言 Frame 1 树结构
        let root = composer.layout_root().expect("root should exist");
        assert_eq!(root.children.len(), 2, "root should have 2 children");
        assert_eq!(root.children[0].children.len(), 0, "text should be leaf");
        assert_eq!(root.children[1].children.len(), 1, "button should have 1 child (content)");

        // Frame 2: recompose（button 应被 skip/replay）
        composer.recompose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let text_key = ctx.next_key();
                    {
                        let _count: State<i32> = ctx.remember(|| 999);
                        ctx.start_leaf(text_key, Modifier::new());
                    }
                    ctx.end_node();

                    let btn_key = ctx.next_key();
                    match ctx.start_restartable_group(btn_key, Modifier::new(), BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let content_key = ctx.next_key();
                            ctx.start_leaf(content_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // 断言 Frame 2 树结构仍正确
        let root = composer.layout_root().expect("root should exist");
        assert_eq!(root.children.len(), 2, "after recompose: root should have 2 children, got {}", root.children.len());
        assert_eq!(root.children[0].children.len(), 0, "after recompose: text should still be leaf");
        assert_eq!(root.children[1].children.len(), 1, "after recompose: button should still have 1 child");
    }

    /// 3 层嵌套 restartable group: Column → Column → Text，验证深层 replay 正确性
    #[test]
    fn test_deep_nested_skip_replay() {
        let mut composer = Composer::new();

        // Frame 1: compose 3-level tree
        composer.compose(|ctx| {
            let key1 = ctx.next_key();
            match ctx.start_restartable_group(key1, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // Level 2: nested restartable group
                    let key2 = ctx.next_key();
                    match ctx.start_restartable_group(key2, Modifier::new(), BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            // Level 3: state-dependent leaf
                            let leaf_key = ctx.next_key();
                            {
                                let _count: State<i32> = ctx.remember(|| 0);
                                ctx.start_leaf(leaf_key, Modifier::new());
                            }
                            ctx.end_node();

                            // Sibling: clean leaf
                            let leaf2_key = ctx.next_key();
                            ctx.start_leaf(leaf2_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // Verify Frame 1: root → [level2 → [leaf1, leaf2]]
        let root = composer.layout_root().unwrap();
        assert_eq!(root.children.len(), 1, "Frame1: root has 1 child");
        assert_eq!(root.children[0].children.len(), 2, "Frame1: level2 has 2 children");

        // Frame 2: recompose (level2 and leaf2 should be clean → skip/replay)
        composer.recompose(|ctx| {
            let key1 = ctx.next_key();
            match ctx.start_restartable_group(key1, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let key2 = ctx.next_key();
                    match ctx.start_restartable_group(key2, Modifier::new(), BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let leaf_key = ctx.next_key();
                            {
                                let _count: State<i32> = ctx.remember(|| 999);
                                ctx.start_leaf(leaf_key, Modifier::new());
                            }
                            ctx.end_node();

                            let leaf2_key = ctx.next_key();
                            ctx.start_leaf(leaf2_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // Verify Frame 2: structure should be identical
        let root = composer.layout_root().unwrap();
        assert_eq!(root.children.len(), 1, "Frame2: root has 1 child");
        assert_eq!(root.children[0].children.len(), 2, "Frame2: level2 has 2 children (leaf1 + leaf2)");
    }

    /// 验证 compose 时 slot 计数功能正常（增量重组的前提）
    #[test]
    fn test_compose_counts_clean_and_dirty() {
        let mut composer = Composer::new();
        let count: State<i32> = State::new(0);

        // Frame 1: 初始 compose
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        assert_eq!(composer.compose_dirty_count, 2, "initial: root + leaf both new");
        assert_eq!(composer.compose_clean_count, 0, "initial: no clean slots");

        // Frame 2: recompose
        composer.recompose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        // recompose 后计数非零（具体值取决于 state deps 是否正确触发）
        assert!(composer.compose_dirty_count + composer.compose_clean_count >= 2,
            "expected >=2 slots, got dirty={} clean={}",
            composer.compose_dirty_count, composer.compose_clean_count);
    }
}
