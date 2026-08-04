//! 布局节点 — LayoutNode 及相关的尺寸/位置/排列/对齐类型

use crate::modifier::{Modifier, ModifierElement, RichSpanStyle};
use crate::ui::text::FontSlant;
use skia_safe::FontStyle as SkFontStyle;
use skia_safe::textlayout::TextStyle as SkTextStyle;
use super::constraints::Constraints;
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局节点 ID 生成器
static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);

// ── LayoutDirection ──

/// 布局方向——控制 Row 等水平布局的 Start/End 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    /// 从左到右（拉丁语系）
    Ltr,
    /// 从右到左（阿拉伯语、希伯来语等）
    Rtl,
}

// ── Size ──

/// 2D 尺寸
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };

    pub fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

// ── Point ──

/// 2D 位置
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

// ── Placement ──

/// 子节点在父节点中的放置结果（测量阶段产出尺寸，布局阶段产出位置）
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// 分配给子节点的尺寸
    pub size: Size,
    /// 子节点在父节点中的位置
    pub position: Point,
}

// ── Arrangement ──

/// 主轴排列方式（类似 Compose 的 Arrangement）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrangement {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

// ── Alignment ──

/// 交叉轴对齐方式（类似 Compose 的 Alignment）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Start,
    End,
    Center,
    Stretch,
}

/// 检查 modifier 中是否包含 TextContent
pub(crate) fn modifier_has_text(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::TextContent { .. }))
}

/// 比较两个 modifier 的文本内容（TextContent/RichTextContent 的 content）——
/// 文本内容变化但 slot Clean（依赖注册在父容器）时，复用节点需重测
/// （否则常量折叠 + cached_paragraph 旧内容 → 渲染画旧文本，输入不显示）。
pub(crate) fn modifier_text_content_differs(a: &Modifier, b: &Modifier) -> bool {
    let text_of = |m: &Modifier| -> Option<String> {
        m.elements().iter().find_map(|el| match el {
            ModifierElement::TextContent { content, .. } => Some(content.clone()),
            ModifierElement::RichTextContent { .. } => Some("<richtext>".to_string()), // RichText 变化保守视为不同
            _ => None,
        })
    };
    text_of(a) != text_of(b)
}

/// 检查 modifier 中是否包含 RichTextContent
pub(crate) fn modifier_has_richtext(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::RichTextContent { .. }))
}

// ── LayoutNode ──

/// 布局树中的一个节点。
///
/// 每个 LayoutNode 对应 UI 树中的一个可测量/可布局的单元。
/// 包含 modifier 链、子节点。
pub struct LayoutNode {
    /// 唯一标识符（用于渲染阶段的精确查找）
    pub id: u64,
    pub modifier: Modifier,
    pub measured_size: Size,
    pub position: Point,
    /// 子节点索引（arena 树——节点存于 NodeArena.nodes，跨重组复用）
    pub children: Vec<usize>,
    /// 测量策略索引（NodeArena.policies 池——独立于节点，避免借用冲突）
    pub measure_policy: Option<usize>,
    /// 叶子节点是否包含 TextContent
    pub(crate) has_text_content: bool,
    /// 叶子节点是否包含 RichTextContent
    pub(crate) has_richtext_content: bool,
    /// 是否获得焦点
    pub focused: bool,
    /// 节点从布局树移除时调用（用于 Window 生命周期管理）
    pub(crate) on_remove: Option<Box<dyn FnOnce() + Send>>,
    /// 是否需要重新测量（clean slot 复用时为 false）
    pub(crate) dirty: bool,
    /// 上次测量时的约束（用于跳过常量布局的 re-measure）
    pub(crate) cached_constraints: Option<Constraints>,
    /// composable 调用对应的 slot key（用于 replay 时子节点查找）
    pub(crate) slot_key: u64,
    /// 测量阶段缓存的 Paragraph（避免渲染时重建）
    pub(crate) cached_paragraph: std::cell::RefCell<Option<crate::text::Paragraph>>,
    /// scroll 容器的 viewport 高度（由 measure_node 在布局阶段设值，供 apply_scroll_delta 使用）
    pub(crate) scroll_viewport_height: f32,
    /// 父节点 ID（键盘事件冒泡用，由 add_child 设置）
    pub(crate) parent_id: Option<u64>,
    /// CompositionLocal 作用域内的 SelectionRegistrar（Text 节点存引用）
    pub(crate) registrar: std::cell::RefCell<Option<crate::ui::selection_container::SelectionRegistrar>>,
    /// 文本光标 x 偏移（TextField 用，render 根据 focused 画竖线）
    pub(crate) cursor_x: std::cell::Cell<f32>,
    pub(crate) cursor_height: std::cell::Cell<f32>,
    /// 光标字符索引（TextField 设置，render 用 TextLayout 精确计算位置）
    pub(crate) cursor_index: std::cell::Cell<usize>,
    /// 光标是否可见（闪烁 toggle，TextField 设置）
    pub(crate) cursor_visible: std::cell::Cell<bool>,
    /// 光标位置回调（TextField 点击后更新 selection 用）
    pub(crate) cursor_callback: std::cell::RefCell<Option<Box<dyn Fn(usize) + Send>>>,
    /// IME 预输入回调（TextField 处理 Preedit 用）
    pub(crate) ime_callback: std::cell::RefCell<Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>>,
    /// IME 组合文本范围（供渲染画下划线）
    pub(crate) composing_range: std::cell::RefCell<Option<std::ops::Range<usize>>>,
    /// 选区范围（供渲染高亮选中文本）
    pub(crate) selection_range: std::cell::RefCell<Option<std::ops::Range<usize>>>,
}

// ── CachedNode：LayoutNode 的可缓存子集，用于增量重组时恢复节点 ──

/// LayoutNode 的缓存快照。新增 LayoutNode 字段时，必须同步更新此结构
/// 及 to_cached() / restore_from() 方法。
#[derive(Debug, Clone)]
pub(crate) struct CachedNode {
    pub modifier: Modifier,
    pub measured_size: Size,
    pub position: Point,
    pub focused: bool,
    pub dirty: bool,
    pub cached_constraints: Option<Constraints>,
    pub slot_key: u64,
    pub registrar: std::cell::RefCell<Option<crate::ui::selection_container::SelectionRegistrar>>,
}

impl LayoutNode {
    /// 生成可缓存快照（编译器强制覆盖所有需缓存字段）
    pub(crate) fn to_cached(&self) -> CachedNode {
        CachedNode {
            modifier: self.modifier.clone(),
            measured_size: self.measured_size,
            position: self.position,
            focused: self.focused,
            dirty: self.dirty,
            cached_constraints: self.cached_constraints,
            slot_key: self.slot_key,
            registrar: self.registrar.clone(),
        }
    }

    /// 从缓存恢复节点状态
    pub(crate) fn restore_from(&mut self, cached: &CachedNode) {
        self.modifier = cached.modifier.clone();
        self.measured_size = cached.measured_size;
        self.position = cached.position;
        self.focused = cached.focused;
        self.dirty = cached.dirty;
        self.cached_constraints = cached.cached_constraints;
        self.slot_key = cached.slot_key;
        self.registrar = cached.registrar.clone();
        self.has_text_content = modifier_has_text(&self.modifier);
        self.has_richtext_content = modifier_has_richtext(&self.modifier);
    }

    /// 只恢复布局部分（measured_size/cached_constraints/position/focused）——
    /// 不覆盖 modifier（modifier 用本帧 build 的值；Enter 重建的节点若恢复旧
    /// modifier，会把本帧新值覆盖成上帧缓存，导致状态变化（如按钮 label）丢失）。
    /// 用于 start_node 的 clean leaf 恢复；Skip 的 stub 用完整 restore_from。
    pub(crate) fn restore_layout(&mut self, cached: &CachedNode) {
        self.measured_size = cached.measured_size;
        self.position = cached.position;
        self.focused = cached.focused;
        self.dirty = cached.dirty;
        self.cached_constraints = cached.cached_constraints;
        self.slot_key = cached.slot_key;
    }
}

impl Drop for LayoutNode {
    fn drop(&mut self) {
        if let Some(f) = self.on_remove.take() { f(); }
    }
}

impl LayoutNode {
    /// 创建节点。`measure_policy` 为 NodeArena.policies 池中的策略索引
    ///（由调用方先 `arena.alloc_policy(...)` 取得）；叶子节点传 `None`。
    pub fn new(modifier: Modifier, measure_policy: Option<usize>) -> Self {
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            has_text_content: modifier_has_text(&modifier),
            has_richtext_content: modifier_has_richtext(&modifier),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children: Vec::new(),
            measure_policy,
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            scroll_viewport_height: 0.0, parent_id: None,
            registrar: std::cell::RefCell::new(None),
            cursor_x: std::cell::Cell::new(0.0),
            cursor_height: std::cell::Cell::new(0.0),
            cursor_index: std::cell::Cell::new(0),
            cursor_visible: std::cell::Cell::new(false),
            cursor_callback: std::cell::RefCell::new(None),
            ime_callback: std::cell::RefCell::new(None),
            composing_range: std::cell::RefCell::new(None),
            selection_range: std::cell::RefCell::new(None),
        }
    }

    /// 添加子节点（arena 化——子节点分配索引后挂入 children）
    pub fn add_child(&mut self, child_idx: usize) {
        // parent_id 由 NodeArena 在挂入时设置（需要父 id）
        self.children.push(child_idx);
    }

    /// 创建叶子节点（无子节点）
    pub fn leaf(modifier: Modifier) -> Self {
        LayoutNode::new(modifier, None)
    }

    /// 创建容器节点（有子节点和布局策略）
    /// 是否叶子节点
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty() && self.measure_policy.is_none()
    }
}

impl Default for LayoutNode {
    fn default() -> Self {
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            modifier: Modifier::new(),
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children: Vec::new(),
            measure_policy: None,
            has_text_content: false,
            has_richtext_content: false,
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            scroll_viewport_height: 0.0, parent_id: None,
            registrar: std::cell::RefCell::new(None),
            cursor_x: std::cell::Cell::new(0.0),
            cursor_height: std::cell::Cell::new(0.0),
            cursor_index: std::cell::Cell::new(0),
            cursor_visible: std::cell::Cell::new(false),
            cursor_callback: std::cell::RefCell::new(None),
            ime_callback: std::cell::RefCell::new(None),
            composing_range: std::cell::RefCell::new(None),
            selection_range: std::cell::RefCell::new(None),
        }
    }
}

// ── NodeArena：布局树节点池（arena 索引树，节点跨重组复用）──

/// 布局树节点池：所有 LayoutNode 存于 `nodes`，树通过索引（`children: Vec<usize>`）
/// 组织。节点**跨重组持久**（对象级复用——组合 diff 只更新内容，不重建对象）。
pub struct NodeArena {
    pub(crate) nodes: Vec<LayoutNode>,
    pub(crate) policies: Vec<Box<dyn MeasurePolicy>>,
    pub(crate) free_policies: Vec<usize>,
    pub(crate) free: Vec<usize>,
    pub(crate) root: Option<usize>,
}

impl NodeArena {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), policies: Vec::new(), free_policies: Vec::new(), free: Vec::new(), root: None }
    }

    /// 分配测量策略到池（优先复用回收槽），返回索引（供 LayoutNode.measure_policy 引用）
    pub fn alloc_policy(&mut self, policy: Box<dyn MeasurePolicy>) -> usize {
        if let Some(idx) = self.free_policies.pop() {
            self.policies[idx] = policy;
            idx
        } else {
            self.policies.push(policy);
            self.policies.len() - 1
        }
    }

    /// 回收节点占用的 policy 槽（结构变化移除节点时——低频泄漏防护）
    fn recycle_policy(&mut self, idx: usize) {
        if let Some(p) = self.nodes[idx].measure_policy.take() {
            self.free_policies.push(p);
        }
    }

    /// 分配/复用槽位（free 优先），返回索引
    pub fn alloc(&mut self, node: LayoutNode) -> usize {
        if let Some(idx) = self.free.pop() {
            self.nodes[idx] = node;
            idx
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    /// 释放节点（含子树——递归释放 children，on_remove 触发）。
    /// `skip`：本帧已复用的节点集合——复用节点已挂入本帧树，free 它会导致
    /// 递归进本帧树形成环（无限递归栈溢出），必须跳过。
    /// `visited`：防环防御（树异常成环时终止递归）。
    pub fn free_node_skip(
        &mut self,
        idx: usize,
        skip: &std::collections::HashSet<usize>,
        visited: &mut std::collections::HashSet<usize>,
    ) {
        if !visited.insert(idx) { return; }
        if skip.contains(&idx) { return; }
        // 防御：被 free 的节点不应在本帧树中（复用节点在 skip；新建节点不在
        // prev_node_by_key——若破坏该不变量会静默误 free 本帧节点）
        debug_assert!(
            !self.nodes[idx].children.iter().any(|&c| c == idx),
            "树环（自引用）——free 应终止"
        );
        let children = std::mem::take(&mut self.nodes[idx].children);
        for c in children {
            self.free_node_skip(c, skip, visited);
        }
        self.recycle_policy(idx);
        if let Some(f) = self.nodes[idx].on_remove.take() { f(); }
        self.nodes[idx] = LayoutNode::default();
        self.free.push(idx);
    }

    /// 释放整棵根树（free_node 递归 + on_remove），root 置空
    pub fn free_root(&mut self) {
        if let Some(r) = self.root.take() {
            self.free_node(r);
        }
    }

    /// 释放节点（含子树——递归释放 children，on_remove 触发）
    pub fn free_node(&mut self, idx: usize) {
        let children = std::mem::take(&mut self.nodes[idx].children);
        for c in children {
            self.free_node(c);
        }
        self.recycle_policy(idx);
        if let Some(f) = self.nodes[idx].on_remove.take() { f(); }
        self.nodes[idx] = LayoutNode::default();
        self.free.push(idx);
    }

    /// 将子节点挂到父（子节点已在池中——start_node 时 alloc，这里只挂索引 + 设 parent_id）
    pub fn add_child(&mut self, parent: usize, child_idx: usize) {

        self.nodes[parent].children.push(child_idx);
        self.nodes[child_idx].parent_id = Some(self.nodes[parent].id);
    }

    pub fn get(&self, idx: usize) -> &LayoutNode { &self.nodes[idx] }
    pub fn get_mut(&mut self, idx: usize) -> &mut LayoutNode { &mut self.nodes[idx] }
    pub fn root(&self) -> Option<&LayoutNode> { self.root.map(|i| &self.nodes[i]) }
    pub fn root_mut(&mut self) -> Option<&mut LayoutNode> { self.root.map(|i| &mut self.nodes[i]) }
    pub fn root_idx(&self) -> Option<usize> { self.root }
    pub fn set_root(&mut self, idx: usize) { self.root = Some(idx); }

    /// 递归遍历（只读）——fn(arena, idx)
    pub fn for_each(&self, mut f: impl FnMut(&NodeArena, usize)) {
        if let Some(r) = self.root {
            self.for_each_rec(r, &mut f);
        }
    }
    fn for_each_rec(&self, idx: usize, f: &mut impl FnMut(&NodeArena, usize)) {
        f(self, idx);
        let children = self.nodes[idx].children.clone();
        for c in children {
            self.for_each_rec(c, f);
        }
    }
}

// ── MeasurePolicy trait ──

/// 测量和布局策略。
///
/// 类似 Compose 的 MeasurePolicy。
/// 实现此 trait 的类型定义了一个容器的布局逻辑。
pub trait MeasurePolicy: std::fmt::Debug {
    /// 测量阶段：给定约束，返回自身尺寸和子节点的放置方案。
    /// arena 版：children 是子节点索引；nodes/policies 为拆分借用（policy 在
    /// policies 池、节点在 nodes——字段级借用避免冲突）。实现内通过
    /// `measure_node(nodes, policies, c, cc)` 递归测量子节点。
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>);

    /// 布局阶段：给定已分配的尺寸，为子节点分配位置。
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]);

    /// 布局动画策略：返回 true 的节点**每帧强制重测**（连同所有祖先——下方兄弟
    /// 位置随之更新）。动画值在 measure 期直接读取（`State::peek`——不注册依赖、
    /// 不触发重组），每帧渲染的 layout 阶段用最新值重新测量。
    /// 典型实现：AnimatedVisibility 退出时的"高度收缩"（对标 Compose shrinkVertically）。
    fn force_remeasure(&self) -> bool {
        false
    }
}

/// 判断节点自身或任一后代是否带 `force_remeasure` 布局动画策略——
/// 有则本节点必须跳过测量缓存（后代高度变化 → 本节点尺寸/子位置随之变化）
fn has_force_remeasure(nodes: &[LayoutNode], policies: &[Box<dyn MeasurePolicy>], idx: usize) -> bool {
    let own = nodes[idx].measure_policy
        .map(|p| policies[p].force_remeasure())
        .unwrap_or(false);
    if own {
        return true;
    }
    nodes[idx].children.iter().any(|&c| has_force_remeasure(nodes, policies, c))
}

// ── 命中测试 ──

/// 命中测试：返回从根到叶的节点索引链（arena 版）
pub fn hit_test(nodes: &[LayoutNode], root: usize, x: f32, y: f32) -> Vec<usize> {
    let mut path = Vec::new();
    hit_test_recursive(nodes, root, x, y, 0.0, 0.0, &mut path);
    path
}

fn hit_test_recursive(
    nodes: &[LayoutNode],
    idx: usize,
    x: f32,
    y: f32,
    parent_x: f32,
    parent_y: f32,
    path: &mut Vec<usize>,
) -> bool {
    let node = &nodes[idx];
    let nx = parent_x + node.position.x;
    let ny = parent_y + node.position.y;
    let nw = node.measured_size.width;
    let nh = node.measured_size.height;

    // 检查是否在节点范围内
    if x < nx || x > nx + nw || y < ny || y > ny + nh {
        return false;
    }

    path.push(idx);

    // 计算 scroll 偏移（渲染时 canvas.translate(-offset)）
    let (scroll_dx, scroll_dy) = scroll_offset_for_node(node);

    // 子节点坐标 = 父节点坐标 + scroll 偏移
    let child_px = nx - scroll_dx;
    let child_py = ny - scroll_dy;

    // 深度优先：先检查子节点（子节点在父节点上方）
    for &c in &node.children {
        if hit_test_recursive(nodes, c, x, y, child_px, child_py, path) {
            return true;
        }
    }

    // 没有命中子节点，停在当前节点
    true
}

fn scroll_offset_for_node(node: &LayoutNode) -> (f32, f32) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    if let Some(state) = node.modifier.vertical_scroll_state() {
        dy += state.get();
    }
    if let Some(state) = node.modifier.horizontal_scroll_state() {
        dx += state.get();
    }
    (dx, dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::Modifier;

    #[test]
    fn test_hit_test_basic() {
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 100.0))];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[0].position = Point::new(0.0, 0.0);

        let path = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn test_hit_test_miss() {
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 100.0))];
        nodes[0].measured_size = Size::new(100.0, 100.0);

        let path = hit_test(&nodes, 0, 150.0, 50.0);
        assert_eq!(path.len(), 0);
    }

    #[test]
    fn test_hit_test_child() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 30.0)),
        ];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].measured_size = Size::new(50.0, 30.0);
        nodes[1].position = Point::new(10.0, 60.0);
        nodes[0].children.push(1);

        // 点击子节点
        let path = hit_test(&nodes, 0, 30.0, 75.0);
        assert_eq!(path, vec![0, 1], "should hit parent and child");
    }

    #[test]
    fn test_hit_test_child_miss() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 30.0)),
        ];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].measured_size = Size::new(50.0, 30.0);
        nodes[1].position = Point::new(10.0, 60.0);
        nodes[0].children.push(1);

        // 点击父节点但不在子节点范围内
        let path = hit_test(&nodes, 0, 80.0, 75.0);
        assert_eq!(path, vec![0], "should only hit parent");
    }
}

// ── 焦点遍历 ──

/// 收集树中所有可聚焦节点的 id（arena 版）
pub fn collect_focusable_ids(nodes: &[LayoutNode], root: usize, list: &mut Vec<u64>) {
    if has_focusable_modifier(&nodes[root]) {
        list.push(nodes[root].id);
    }
    let children = nodes[root].children.clone();
    for c in children {
        collect_focusable_ids(nodes, c, list);
    }
}

/// 通过 node.id 查找节点，返回 arena 索引（不可变）
pub fn find_node_by_id(nodes: &[LayoutNode], root: usize, id: u64) -> Option<usize> {
    if nodes[root].id == id { return Some(root); }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(n) = find_node_by_id(nodes, c, id) { return Some(n); }
    }
    None
}

pub fn has_focusable_modifier(node: &LayoutNode) -> bool {
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::Focusable))
}

/// 移动到下一个可聚焦节点，返回是否成功
pub fn focus_next(nodes: &mut Vec<LayoutNode>, root: usize) -> bool {
    let ids: Vec<u64> = {
        let mut ids = Vec::new();
        collect_focusable_ids(nodes, root, &mut ids);
        ids
    };
    if ids.is_empty() { return false; }
    let current = ids.iter().position(|id| {
        find_node_by_id(nodes, root, *id).map(|n| nodes[n].focused).unwrap_or(false)
    });
    let next = match current {
        Some(i) if i + 1 < ids.len() => i + 1,
        _ => 0,
    };
    clear_focus(nodes, root);
    set_focus_by_id(nodes, root, ids[next]);
    true
}

/// 焦点移到上一个 focusable 节点（Shift+Tab）
pub fn focus_prev(nodes: &mut Vec<LayoutNode>, root: usize) -> bool {
    let ids: Vec<u64> = {
        let mut ids = Vec::new();
        collect_focusable_ids(nodes, root, &mut ids);
        ids
    };
    if ids.is_empty() { return false; }
    let current = ids.iter().position(|id| {
        find_node_by_id(nodes, root, *id).map(|n| nodes[n].focused).unwrap_or(false)
    });
    let prev = match current {
        Some(0) | None => ids.len() - 1,
        Some(i) => i - 1,
    };
    clear_focus(nodes, root);
    set_focus_by_id(nodes, root, ids[prev]);
    true
}

pub fn clear_focus(nodes: &mut Vec<LayoutNode>, root: usize) {
    nodes[root].focused = false;
    let children = nodes[root].children.clone();
    for c in children {
        clear_focus(nodes, c);
    }
}

pub fn set_focus_by_id(nodes: &mut Vec<LayoutNode>, root: usize, target_id: u64) -> bool {
    if nodes[root].id == target_id {
        nodes[root].focused = true;
        return true;
    }
    let children = nodes[root].children.clone();
    for c in children {
        if set_focus_by_id(nodes, c, target_id) {
            return true;
        }
    }
    false
}

/// 点击时聚焦指定节点（target 为 arena 索引）
// ── FocusRequester 全局注册表 ──

/// 通过 FocusRequester ID 设置焦点
pub fn focus_by_id(nodes: &mut Vec<LayoutNode>, root: usize, focus_requester_id: u64) -> bool {
    let target_id = find_node_id_by_focus_requester(nodes, root, focus_requester_id);
    if let Some(id) = target_id {
        clear_focus(nodes, root);
        set_focus_by_id(nodes, root, id);
        true
    } else {
        false
    }
}

/// 内部辅助：按 FocusRequesterId 查找节点 id（arena 版）
pub fn find_node_id_by_focus_requester(nodes: &[LayoutNode], root: usize, requester_id: u64) -> Option<u64> {
    if has_focus_id(&nodes[root], requester_id) {
        return Some(nodes[root].id);
    }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(id) = find_node_id_by_focus_requester(nodes, c, requester_id) {
            return Some(id);
        }
    }
    None
}

/// 按 slot_key 查找节点 ID（slot_key 跨重组稳定）
pub fn find_node_id_by_slot_key(nodes: &[LayoutNode], root: usize, slot_key: u64) -> Option<u64> {
    if nodes[root].slot_key == slot_key { return Some(nodes[root].id); }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(id) = find_node_id_by_slot_key(nodes, c, slot_key) {
            return Some(id);
        }
    }
    None
}

fn has_focus_id(node: &LayoutNode, id: u64) -> bool {
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::FocusRequesterId { id: fid } if *fid == id))
}

/// 找到树中第一个焦点节点的 FocusRequesterId（用于持久化）
pub fn get_focus_id(nodes: &[LayoutNode], root: usize) -> Option<u64> {
    if nodes[root].focused {
        return Some(nodes[root].id);
    }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(id) = get_focus_id(nodes, c) {
            return Some(id);
        }
    }
    None
}

fn modifier_focus_id(node: &LayoutNode) -> Option<u64> {
    node.modifier.focus_requester_id()
}

// ── 递归测量引擎 ──

/// 递归测量节点（处理 modifier 中的约束并调用子节点的 measure_policy）。
///
/// arena 版：`nodes` 为节点池、`policies` 为策略池、`idx` 为当前节点索引。
/// 子节点通过 `nodes[idx].children`（索引列表）递归测量。
pub(crate) fn measure_node(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // 重放 stub：clean-skip 节点无 measure_policy，绝不能重新测量
    //（无 policy 走叶子分支会返回 0 并污染 prev_nodes 缓存，导致塌缩不可逆）。
    // stub 只在 slot 真正 clean（无状态变化）时出现；约束若变化，下帧该 slot dirty → Enter 正常重建。
    // 常量折叠：若节点未变脏且约束相同，直接复用上次结果。
    // 布局动画（force_remeasure）节点及其所有祖先跳过折叠——每帧用最新动画值重测
    if !nodes[idx].dirty
        && !has_force_remeasure(nodes, policies, idx)
        && nodes[idx].cached_constraints == Some(constraints)
    {
        return (nodes[idx].measured_size, Vec::new());
    }

    // 设置 ACTIVE_SLOT_KEY = 本节点 slot——使 SizeDynamic 闭包内的 State::get()
    // 把依赖注册到本节点（动画值变化 → 本节点 dirty → 重组重测）
    crate::core::composer::set_active_slot_key(nodes[idx].slot_key);

    // 应用 modifier 中的 Layout 约束（使用查询方法）
    let mut inner_constraints = constraints;

    // 应用 Size 元素（静态/动态单轴独立解析——布局属性动画用 State/闭包，
    // 测量时求值并注册依赖到本节点）
    if let Some((sw, sh)) = nodes[idx].modifier.resolved_size() {
        if let Some(w) = sw { inner_constraints = inner_constraints.tighten_width(w); }
        if let Some(h) = sh { inner_constraints = inner_constraints.tighten_height(h); }
    }

    // 1. 固定尺寸（仅 Static+Static 的 Size——由 resolved_size 已处理，此分支保留兼容其他查询）
    if let Some((width, height)) = nodes[idx].modifier.fixed_size() {
        use crate::modifier::Dimension;
        if let Dimension::Fixed(w) | Dimension::Dp(crate::unit::Dp(w)) = width {
            inner_constraints = inner_constraints.tighten_width(w);
        }
        if let Dimension::Fixed(h) | Dimension::Dp(crate::unit::Dp(h)) = height {
            inner_constraints = inner_constraints.tighten_height(h);
        }
        // Px 需 Density 转换
        if let Dimension::Px(p) = width {
            inner_constraints = inner_constraints.tighten_width(p.to_logical(crate::unit::current_density()));
        }
        if let Dimension::Px(p) = height {
            inner_constraints = inner_constraints.tighten_height(p.to_logical(crate::unit::current_density()));
        }
    }

    // 2. 应用 padding
    let (pad_left, pad_right) = nodes[idx].modifier.get_padding_horizontal();
    let (pad_top, pad_bottom) = nodes[idx].modifier.get_padding_vertical();
    let pad_x = pad_left + pad_right;
    let pad_y = pad_top + pad_bottom;
    if pad_x > 0.0 || pad_y > 0.0 {
        inner_constraints = inner_constraints.offset(pad_x, pad_y);
    }

    // 3. 应用 FillMax 约束（在 scroll 修改 max 之前，保存 viewport 约束）
    let viewport_height = inner_constraints.max_height;
    if nodes[idx].modifier.is_fill_max_width() {
        inner_constraints.min_width = inner_constraints.max_width;
    }
    if nodes[idx].modifier.is_fill_max_height() {
        if inner_constraints.max_height < f32::MAX {
            inner_constraints.min_height = inner_constraints.max_height;
        }
    }

    // 4. 检查 scroll 修饰符——给子节点无限约束
    if nodes[idx].modifier.vertical_scroll_state().is_some() {
        // scroll 容器自身填 viewport（fill_max_height 在无限 max 时跳过，这里补上）
        if nodes[idx].modifier.is_fill_max_height() && inner_constraints.max_height >= f32::MAX {
            inner_constraints.min_height = viewport_height;
        }
        // 保存 viewport 高度供滚动 clamping 使用
        nodes[idx].scroll_viewport_height = viewport_height;
        inner_constraints.max_height = f32::MAX;
    }
    if nodes[idx].modifier.horizontal_scroll_state().is_some() {
        inner_constraints.max_width = f32::MAX;
    }

    // 实际测量
    let result = if let Some(pidx) = nodes[idx].measure_policy {
        // 先拷贝子节点索引（policy.measure 会可变借用整个 nodes，不能持有 nodes[idx] 借用）
        let children = nodes[idx].children.clone();
        let (size, placements) = policies[pidx].measure(nodes, policies, &children, inner_constraints);
        // apply positions
        policies[pidx].place(nodes, &children, &placements);
        // apply padding offset
        if pad_left > 0.0 || pad_top > 0.0 {
            for &c in &children {
                nodes[c].position.x += pad_left;
                nodes[c].position.y += pad_top;
            }
        }
        // apply per-child offset modifier
        for &c in &children {
            if let Some((ox, oy)) = nodes[c].modifier.get_offset() {
                nodes[c].position.x += ox;
                nodes[c].position.y += oy;
            }
        }
        let outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        nodes[idx].measured_size = outer_size;
        (outer_size, placements)
    } else {
        // 叶子节点：使用 ContentMeasurer 或默认逻辑
        let size = if nodes[idx].has_text_content {
            // 合并的 measure + cache（避免重复创建 Paragraph）
            // 使用父约束的 max_width 作为排版宽度，确保文本在可用空间内自动换行。
            // 对于可滚动容器，inner_constraints.max_width 已被设为 f32::MAX。
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_text(&nodes[idx], layout_width);
            // 用约束 clamping 最终尺寸（fill_max_width 时约束收紧，文本应填满可用宽度）
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height),
            )
        } else if nodes[idx].has_richtext_content {
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_richtext(&nodes[idx], layout_width);
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height),
            )
        } else {
            // 普通叶子节点
            let w = inner_constraints.constrain_width(
                if inner_constraints.has_fixed_width() {
                    inner_constraints.min_width
                } else {
                    0.0
                },
            );
            let h = inner_constraints.constrain_height(
                if inner_constraints.has_fixed_height() {
                    inner_constraints.min_height
                } else {
                    0.0
                },
            );
            Size::new(w, h)
        };

        nodes[idx].measured_size = size;
        (size, Vec::new())
    };

    // 标记测量完成，缓存约束供下帧复用
    nodes[idx].dirty = false;
    nodes[idx].cached_constraints = Some(constraints);
    result
}

/// 合并的文本测量 + Paragraph 缓存。
///
/// 从 TextContent modifier 中提取所有参数（font_size、max_lines、overflow、align），
/// 在 ParagraphStyle 上正确设置后一次创建 Paragraph，测量尺寸并缓存供渲染复用。
/// 消除旧代码中 `measurer.measure()` + `cache_text_paragraph()` 重复创建的开销。
fn measure_and_cache_text(node: &LayoutNode, max_width: f32) -> Size {
    use skia_safe::textlayout::ParagraphStyle;
    let fc = crate::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::TextContent {
            content, font_size, color, font_weight, font_style, max_lines, align, overflow, soft_wrap,
        } = el {
            let mut para_style = ParagraphStyle::new();

            // max_lines：限制行数
            if *max_lines < usize::MAX {
                para_style.set_max_lines(*max_lines);
            }

            // ellipsis overflow：超出时显示省略号
            if *overflow == crate::ui::TextOverflow::Ellipsis {
                para_style.set_ellipsis("\u{2026}");
            }

            // justify alignment
            if *align == crate::ui::TextAlign::Justify {
                para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify);
            }

            // soft_wrap=false: 无限宽度排版，不换行
            let layout_width = if *soft_wrap { max_width } else { f32::MAX };

            let mut text_style = skia_safe::textlayout::TextStyle::new();
            text_style.set_font_size(*font_size);
            // IMPORTANT: 设置文字颜色（Skia TextStyle 默认白色，不设的话画在白色背景上不可见）
            text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
            // 设置字重和倾斜
            if *font_weight != crate::ui::text::FontWeight::NORMAL || *font_style != crate::ui::text::FontSlant::Upright {
                use skia_safe::FontStyle;
                use crate::ui::text::FontSlant;
                let slant = match font_style {
                    FontSlant::Upright => skia_safe::font_style::Slant::Upright,
                    FontSlant::Italic => skia_safe::font_style::Slant::Italic,
                    FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
                };
                text_style.set_font_style(FontStyle::new(font_weight.value().into(), 5.into(), slant));
            }
            let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);
            builder.push_style(&text_style);
            builder.add_text(content.as_str());
            let mut para = builder.build();
            para.layout(layout_width);

            let size = Size::new(
                para.max_intrinsic_width().ceil().min(max_width),
                para.height().ceil(),
            );

            *node.cached_paragraph.borrow_mut() = Some(para);
            return size;
        }
    }
    Size::ZERO
}

/// 富文本测量 + 缓存（含内联 drawable）。
///
/// 从 RichTextContent modifier 中提取内容文本、内联元素列表和每段样式，
/// 使用 Skia ParagraphBuilder 构建带 U+FFFC 占位符的段落，
/// 对每个片段应用对应的样式后缓存 Paragraph 和 drawables 供渲染复用。
fn measure_and_cache_richtext(node: &LayoutNode, max_width: f32) -> Size {
    use skia_safe::textlayout::{ParagraphStyle, PlaceholderStyle, PlaceholderAlignment, TextBaseline};
    
    
    let fc = crate::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::RichTextContent { content, drawables, drawable_ranges, spans } = el {
            let para_style = ParagraphStyle::new();
            let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);

            // 按 span 迭代 + drawable 范围
            let chars: Vec<char> = content.chars().collect();
            let mut di = 0usize;
            let mut span_idx = 0usize;
            let total = chars.len();
            let mut ci = 0usize;
            while ci < total {
                // 是否在 drawable 范围内
                if di < drawable_ranges.len() && ci >= drawable_ranges[di].start && ci < drawable_ranges[di].end {
                    // 占位符（只对范围起点做一次 add_placeholder，跳过剩余字符）
                    if ci == drawable_ranges[di].start {
                        if let Some(s) = spans.iter().find(|s| s.start <= ci && s.end > ci) {
                            builder.push_style(&to_sktextstyle(s));
                        }
                        let d = drawables[di].clone();
                        builder.add_placeholder(" ", d, &Some((PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0)));
                        if let Some(_s) = spans.iter().find(|s| s.start <= ci && s.end > ci) { builder.pop(); }
                    }
                    ci += 1;
                    if ci >= drawable_ranges[di].end { di += 1; }
                    continue;
                }

                // 文本：先查 span
                while span_idx < spans.len() && spans[span_idx].end <= ci { span_idx += 1; }
                if span_idx < spans.len() && spans[span_idx].start <= ci && ci < spans[span_idx].end {
                    let run_end = spans[span_idx].end.min(total);
                    let text: String = chars[ci..run_end].iter().collect();
                    if !text.is_empty() {
                        builder.push_style(&to_sktextstyle(&spans[span_idx]));
                        builder.add_text(&text);
                        builder.pop();
                    }
                    ci = run_end;
                    continue;
                }

                // 无样式文本
                builder.add_text(&chars[ci].to_string());
                ci += 1;
            }

            let mut para = builder.build();
            para.layout(max_width);
            let size = Size::new(
                para.max_intrinsic_width().ceil().min(max_width),
                para.height().ceil(),
            );
            *node.cached_paragraph.borrow_mut() = Some(para);
            return size;
        }
    }
    Size::ZERO
}

/// 将 RichSpanStyle 转为 Skia TextStyle
fn to_sktextstyle(s: &RichSpanStyle) -> SkTextStyle {
    let mut ts = SkTextStyle::new();
    ts.set_font_size(s.font_size);
    ts.set_color(skia_safe::Color::from_argb(s.color.a, s.color.r, s.color.g, s.color.b));

    // 字重/字型/字宽
    let slant = match s.font_style {
        FontSlant::Upright => skia_safe::font_style::Slant::Upright,
        FontSlant::Italic => skia_safe::font_style::Slant::Italic,
        FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
    };
    ts.set_font_style(SkFontStyle::new(s.font_weight.value().into(), s.font_width.into(), slant));

    // 装饰线
    let mut deco: skia_safe::textlayout::TextDecoration = skia_safe::textlayout::TextDecoration::default();
    if s.underline { deco |= skia_safe::textlayout::TextDecoration::UNDERLINE; }
    if s.overline { deco |= skia_safe::textlayout::TextDecoration::OVERLINE; }
    if s.strikethrough { deco |= skia_safe::textlayout::TextDecoration::LINE_THROUGH; }
    ts.set_decoration_type(deco);
    if let Some(c) = &s.decoration_color {
        ts.set_decoration_color(skia_safe::Color::from_argb(c.a, c.r, c.g, c.b));
    }
    if let Some(st) = s.decoration_style {
        use crate::modifier::DecoStyle;
        let sk = match st {
            DecoStyle::Solid => skia_safe::textlayout::TextDecorationStyle::Solid,
            DecoStyle::Double => skia_safe::textlayout::TextDecorationStyle::Double,
            DecoStyle::Dotted => skia_safe::textlayout::TextDecorationStyle::Dotted,
            DecoStyle::Dashed => skia_safe::textlayout::TextDecorationStyle::Dashed,
            DecoStyle::Wavy => skia_safe::textlayout::TextDecorationStyle::Wavy,
        };
        ts.set_decoration_style(sk);
    }
    if let Some(m) = s.decoration_mode {
        use crate::modifier::DecoMode;
        let sk = match m {
            DecoMode::Gaps => skia_safe::textlayout::TextDecorationMode::Gaps,
            DecoMode::Through => skia_safe::textlayout::TextDecorationMode::Through,
        };
        ts.set_decoration_mode(sk);
    }

    // 基线偏移（D:\winia: shift = font_size * multiplier）
    if s.baseline_shift != 0.0 {
        ts.set_baseline_shift(s.baseline_shift * s.font_size);
    }

    // 间距
    if s.letter_spacing != 0.0 { ts.set_letter_spacing(s.letter_spacing); }
    if s.word_spacing != 0.0 { ts.set_word_spacing(s.word_spacing); }
    if s.height_multiple != 0.0 { ts.set_height(s.height_multiple); ts.set_height_override(true); }
    if s.half_leading { ts.set_half_leading(true); }

    // 字体族
    if !s.font_families.is_empty() {
        ts.set_font_families(&s.font_families.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }

    // 渲染精度
    if let Some(e) = s.font_edging {
        use crate::modifier::FontEdge;
        let sk = match e {
            FontEdge::Alias => skia_safe::font::Edging::Alias,
            FontEdge::AntiAlias => skia_safe::font::Edging::AntiAlias,
            FontEdge::SubpixelAntiAlias => skia_safe::font::Edging::SubpixelAntiAlias,
        };
        ts.set_font_edging(sk);
    }
    if let Some(h) = s.font_hinting {
        use crate::modifier::FontHint;
        let sk = match h {
            FontHint::None => skia_safe::FontHinting::None,
            FontHint::Slight => skia_safe::FontHinting::Slight,
            FontHint::Normal => skia_safe::FontHinting::Normal,
            FontHint::Full => skia_safe::FontHinting::Full,
        };
        ts.set_font_hinting(sk);
    }
    if s.subpixel { ts.set_subpixel(true); }

    // 前景/背景
    if let Some(fg) = &s.foreground_color {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(fg.a, fg.r, fg.g, fg.b));
        paint.set_style(skia_safe::paint::Style::Fill);
        ts.set_foreground_paint(&paint);
    }
    if let Some(bg) = &s.background {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
        paint.set_style(skia_safe::paint::Style::Fill);
        ts.set_background_paint(&paint);
    }

    // locale
    if let Some(loc) = &s.locale {
        ts.set_locale(loc);
    }

    ts
}

// ── 主轴间距计算（Column/Row 共用）──

/// 计算 Arrangement 的间距分配：返回 (元素间额外间距, 首/尾 leading offset)。
/// Column 垂直主轴 / Row 水平主轴——由调用方（measure_flex）确定主轴分量。
pub fn compute_spacing(
    arrangement: crate::layout::Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        crate::layout::Arrangement::Start => (0.0, 0.0),
        crate::layout::Arrangement::End => (0.0, remaining),
        crate::layout::Arrangement::Center => (0.0, remaining / 2.0),
        crate::layout::Arrangement::SpaceBetween => {
            if gap_count > 0 {
                (remaining / gap_count as f32, 0.0)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        crate::layout::Arrangement::SpaceAround => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space / 2.0) // (元素间间距, 首/尾边缘间距 = 一半)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        crate::layout::Arrangement::SpaceEvenly => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space) // 元素间与边缘间距相等
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}
