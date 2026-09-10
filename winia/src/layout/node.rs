//! 布局节点 — LayoutNode 及相关的尺寸/位置/排列/对齐类型

use crate::modifier::{Modifier, ModifierElement, RichSpanStyle};
use crate::ui::shared_transition::{abs_rect_upward, find_idx_by_slot, TransitionRole};
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
/// 文本内容变化但 slot Clean（依赖注册在父容器）时，复用节点需重测。
/// 检查 modifier 文本内容差异（决定"折叠测量是否失效"）：
/// 内容、对齐、**颜色**、字号、字重、行数、字间距、行高、溢出——**所有
/// 影响测量/渲染结果的属性**（color 变化必须触发重测——否则淡入动画
/// （alpha 0→1）后 cached_paragraph 仍是透明色，渲染画不出文字）
pub(crate) fn modifier_text_content_differs(a: &Modifier, b: &Modifier) -> bool {
    let text_of = |m: &Modifier| -> Option<(String, crate::ui::TextAlign, crate::modifier::Color, f32, crate::ui::text::FontWeight, crate::ui::text::FontSlant, usize, bool, f32, Option<f32>, crate::ui::TextOverflow)> {
        m.elements().iter().find_map(|el| match el {
            ModifierElement::TextContent { content, align, color, font_size, font_weight, font_style, max_lines, soft_wrap, letter_spacing, line_height, overflow, .. } => {
                Some((content.clone(), *align, *color, *font_size, *font_weight, *font_style, *max_lines, *soft_wrap, *letter_spacing, *line_height, *overflow))
            }
            // RichText 变化保守视为不同
            ModifierElement::RichTextContent { .. } => Some(("<richtext>".to_string(), crate::ui::TextAlign::Left, crate::modifier::Color::TRANSPARENT, 0.0, crate::ui::text::FontWeight::NORMAL, crate::ui::text::FontSlant::Upright, 0, true, 0.0, None, crate::ui::TextOverflow::Clip)),
            _ => None,
        })
    };
    text_of(a) != text_of(b)
}

/// 检查 modifier 中是否包含 RichTextContent
pub(crate) fn modifier_has_richtext(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::RichTextContent { .. }))
}

/// 检查 modifier 中是否包含 ImageContent（Image 组件）
pub(crate) fn modifier_has_image(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::ImageContent { .. }))
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
    /// 叶子节点是否包含 ImageContent（Image 组件）
    pub(crate) has_image_content: bool,
    /// 是否获得焦点
    pub focused: bool,
    /// 节点从布局树移除时调用（用于 Window 生命周期管理）
    pub(crate) on_remove: Option<Box<dyn FnOnce() + Send>>,
    /// 是否需要重新测量（clean slot 复用时为 false）
    pub(crate) dirty: bool,
    /// 布局级失效（两段式依赖：布局动画值变化只重测不重组——measure 后清除）
    pub(crate) layout_dirty: bool,
    /// 上次测量时的约束（用于跳过常量布局的 re-measure）
    pub(crate) cached_constraints: Option<Constraints>,
    /// 布局方向（组合期快照：modifier 覆盖 > CompositionLocal 默认）——
    /// 测量/渲染期 padding start/end 镜像、文本对齐用
    pub(crate) layout_direction: LayoutDirection,
    /// composable 调用对应的 slot key（用于 replay 时子节点查找）
    pub(crate) slot_key: u64,
    /// 测量阶段缓存的 Paragraph（避免渲染时重建）
    pub(crate) cached_paragraph: std::cell::RefCell<Option<crate::text::Paragraph>>,
    /// scroll 容器的 viewport 高度（由 measure_node 在布局阶段设值，供 apply_scroll_delta 使用）
    pub(crate) scroll_viewport_height: f32,
    /// scroll 容器的 viewport 宽度（水平滚动用——同 scroll_viewport_height）
    pub(crate) scroll_viewport_width: f32,
    /// scroll 容器的内容总高（lazy 列表用——apply_scroll_delta 计算 max_offset；0 = 未设置）
    pub(crate) scroll_content_height: f32,
    /// lazy 列表反向布局（reverseLayout：render 滚动平移镜像为 content - vh - offset）
    pub(crate) scroll_reverse: bool,
    /// scroll 容器的内容总宽（水平滚动用——同 scroll_content_height）
    pub(crate) scroll_content_width: f32,
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
    /// 显示聚焦标记（text-field-v2 容器化：焦点在容器节点，输入子节点
    /// 渲染光标/选区用此标记——组合期写入，回退 node.focused）
    pub(crate) display_focused: std::cell::Cell<bool>,
    /// 光标位置回调（TextField 点击后更新 selection 用）
    pub(crate) cursor_callback: std::cell::RefCell<Option<Box<dyn Fn(usize) + Send>>>,
    /// IME 预输入回调（TextField 处理 Preedit 用）
    pub(crate) ime_callback: std::cell::RefCell<Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>>,
    /// IME 组合文本范围（供渲染画下划线）
    pub(crate) composing_range: std::cell::RefCell<Option<std::ops::Range<usize>>>,
    /// 焦点环颜色（组合期由组件从主题捕获写入——渲染期 CompositionLocal
    /// 已退出，不能读主题；未设置时回退默认蓝色）
    pub(crate) focus_color: std::cell::Cell<crate::modifier::Color>,
    /// IME 组合下划线颜色（组合期捕获主题 primary——渲染期不能读
    /// CompositionLocal（Phase 4.2）；未设置时回退默认色）
    pub(crate) composing_color: std::cell::Cell<crate::modifier::Color>,
    /// 共享元素转场视觉（Phase 2）：`Some` 时渲染期按起止矩形做 morph
    /// （位移/缩放/淡入淡出/圆角），命中测试跳过。逐帧由协调器重写；
    /// 转场结束即清 `None`。刻意不进 `CachedNode`——飞行态是瞬态，
    /// 缓存命中必须从干净状态重建（协调器按 slot 回填）。
    pub(crate) transition: Option<crate::ui::shared_transition::TransitionVisual>,
    /// 转场期间提升到 layer 的**非共享**子树（Compose
    /// `renderInSharedTransitionScopeOverlay`）：树内遍历跳过它，由协调器
    /// 在 layer 末尾按原样重画（无变换），从而压在飞行端点之上。每帧由
    /// `refresh_scope_overlay_roots` 重算——属于瞬态，不进缓存。
    pub(crate) in_scope_overlay: bool,
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
    /// 结构签名（P3-1）：上帧直接子节点数——Skip 恢复命中条件之一。
    /// 子树结构增删（if 分支/列表项）后同位置 slot_key 仍相同，签名不等则
    /// 放弃恢复（走 Enter 重建），防旧内容缓存张冠李戴。
    pub children_count: usize,
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
            children_count: self.children.len(),
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
            has_image_content: modifier_has_image(&modifier),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children: Vec::new(),
            measure_policy,
            focused: false,
            on_remove: None,
            dirty: true,
            layout_dirty: false,
            cached_constraints: None,
            layout_direction: LayoutDirection::Ltr,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            scroll_viewport_height: 0.0, scroll_viewport_width: 0.0, scroll_content_height: 0.0, scroll_content_width: 0.0, scroll_reverse: false, parent_id: None,
            registrar: std::cell::RefCell::new(None),
            cursor_x: std::cell::Cell::new(0.0),
            cursor_height: std::cell::Cell::new(0.0),
            cursor_index: std::cell::Cell::new(0),
            cursor_visible: std::cell::Cell::new(false),
            display_focused: std::cell::Cell::new(false),
            cursor_callback: std::cell::RefCell::new(None),
            ime_callback: std::cell::RefCell::new(None),
            composing_range: std::cell::RefCell::new(None),
            focus_color: std::cell::Cell::new(crate::modifier::Color::from_argb(204, 77, 153, 255)),
            composing_color: std::cell::Cell::new(crate::modifier::Color::TRANSPARENT),
            transition: None,
            in_scope_overlay: false,
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
            has_image_content: false,
            children: Vec::new(),
            measure_policy: None,
            has_text_content: false,
            has_richtext_content: false,
            focused: false,
            on_remove: None,
            dirty: true,
            layout_dirty: false,
            cached_constraints: None,
            layout_direction: LayoutDirection::Ltr,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            scroll_viewport_height: 0.0, scroll_viewport_width: 0.0, scroll_content_height: 0.0, scroll_content_width: 0.0, scroll_reverse: false, parent_id: None,
            registrar: std::cell::RefCell::new(None),
            cursor_x: std::cell::Cell::new(0.0),
            cursor_height: std::cell::Cell::new(0.0),
            cursor_index: std::cell::Cell::new(0),
            cursor_visible: std::cell::Cell::new(false),
            display_focused: std::cell::Cell::new(false),
            cursor_callback: std::cell::RefCell::new(None),
            ime_callback: std::cell::RefCell::new(None),
            composing_range: std::cell::RefCell::new(None),
            focus_color: std::cell::Cell::new(crate::modifier::Color::from_argb(204, 77, 153, 255)),
            composing_color: std::cell::Cell::new(crate::modifier::Color::TRANSPARENT),
            transition: None,
            in_scope_overlay: false,
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
}

// ── 命中测试 ──

/// 命中测试：返回从根到叶的节点索引链（arena 版）
///
/// 坐标空间 = 布局空间（场景坐标）。只处理 scroll 偏移（布局层位移）；
/// **不处理 graphics_layer 变换**——图形层只影响绘制、不影响命中
/// （对标 Compose graphicsLayer：点击区域始终是布局 bounds，变换后
/// 视觉超出/缩进的部分不改变命中范围）。波纹按压点本地坐标换算
/// （scene_to_node_local）与此保持一致。
pub fn hit_test(nodes: &[LayoutNode], root: usize, x: f32, y: f32) -> Vec<usize> {
    let mut path = Vec::new();
    hit_test_recursive(nodes, root, x, y, 0.0, 0.0, &mut path);
    path
}

/// Flight-aware hit test (Phase 3; extended by the overlay pass in Phase 6).
/// The transition layer paints above the main tree, so it is tested first,
/// topmost entry first:
///
/// - **Source** (detached ghost): clicking the ghost == clicking the live
///   target — fraction-mapped into the target's natural rect with a
///   root-anchored path for bubbling fidelity.
/// - **Target/Morph, elevated** (Compose `renderInOverlayDuringTransition`):
///   the endpoint is *still in the tree*, so it routes through its own lerped
///   visual rect with the flight transform inverted, exactly like an in-tree
///   transitioning node — but WITHOUT the ancestor bounds/viewport rejection.
///   That rejection is what makes an element flying outside its container
///   untouchable, and escaping it is the whole point of the overlay pass.
///
/// Everything else falls through to [`hit_test`]. Empty roots (idle) cost one
/// length check — the hot path is untouched.
pub fn hit_test_with_flights(
    nodes: &[LayoutNode],
    root: usize,
    transition_roots: &[usize],
    x: f32,
    y: f32,
) -> Vec<usize> {
    if !transition_roots.is_empty() {
        // Index for parent-chain walks (mid-flight clicks only).
        let mut id_to_idx = std::collections::HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            id_to_idx.insert(n.id, i);
        }
        // Painted back-to-front, so the LAST entry is on top — test in reverse.
        for &tidx in transition_roots.iter().rev() {
            let Some(node) = nodes.get(tidx) else { continue };
            let Some(t) = node.transition.as_ref() else { continue };
            let hit = match t.role {
                TransitionRole::Source => hit_through_ghost(nodes, root, &id_to_idx, tidx, x, y),
                TransitionRole::Target | TransitionRole::Morph => {
                    if !t.elevated {
                        continue;
                    }
                    hit_through_elevated(nodes, root, &id_to_idx, tidx, x, y)
                }
            };
            if let Some(path) = hit {
                return path;
            }
        }
    }
    hit_test(nodes, root, x, y)
}

/// Root→`idx` ancestor chain including `root` (bubbling fidelity), or `None`
/// when the chain is broken (stale arena). `idx` itself is NOT included.
fn ancestor_prefix(
    nodes: &[LayoutNode],
    id_to_idx: &std::collections::HashMap<u64, usize>,
    root: usize,
    idx: usize,
) -> Option<Vec<usize>> {
    let mut prefix: Vec<usize> = Vec::new();
    let mut cur = idx;
    let mut ok = idx == root;
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        if pidx == root {
            prefix.push(root);
            ok = true;
            break;
        }
        prefix.push(pidx);
        cur = pidx;
    }
    if !ok {
        return None;
    }
    prefix.reverse();
    Some(prefix)
}

/// Detached source ghost → live target routing (Phase 3).
fn hit_through_ghost(
    nodes: &[LayoutNode],
    root: usize,
    id_to_idx: &std::collections::HashMap<u64, usize>,
    tidx: usize,
    x: f32,
    y: f32,
) -> Option<Vec<usize>> {
    let t = nodes.get(tidx)?.transition.as_ref()?;
    let l = t.lerped();
    if l.width <= 0.0 || l.height <= 0.0 {
        return None;
    }
    if x < l.x || x > l.x + l.width || y < l.y || y > l.y + l.height {
        return None;
    }
    let target_slot = t.link_slot?;
    // NOTE (Tier1 limitation): the target may live in a PEER composer's arena
    // (overlay) — invisible to this single-arena search, so cross-composer
    // ghosts paint but ignore taps. The live target itself stays directly
    // hittable in its own composer.
    let target_idx = find_idx_by_slot(nodes, root, target_slot)?;
    // Fraction-map ghost → target natural rect, then descend the live target
    // subtree directly: the target node itself is hit by construction
    // (fractions clamped into its natural rect), and re-entering
    // hit_test_recursive on it would invert the flight transform twice
    // (remap is single-application per level).
    let fx = ((x - l.x) / l.width).clamp(0.0, 1.0);
    let fy = ((y - l.y) / l.height).clamp(0.0, 1.0);
    let tb = abs_rect_upward(nodes, id_to_idx, target_idx);
    // Scrolled-container targets: mapped point outside the visible viewport
    // misses (mirrors the viewport clamp in recursion).
    {
        let tn = &nodes[target_idx];
        let vw = if tn.scroll_viewport_width > 0.0 {
            tn.scroll_viewport_width
        } else {
            tn.measured_size.width
        };
        let vh = if tn.scroll_viewport_height > 0.0 {
            tn.scroll_viewport_height
        } else {
            tn.measured_size.height
        };
        let tx0 = tb.x + fx * tb.width;
        let ty0 = tb.y + fy * tb.height;
        if tx0 < tb.x || tx0 > tb.x + vw || ty0 < tb.y || ty0 > tb.y + vh {
            return None;
        }
    }
    let tx = tb.x + fx * tb.width;
    let ty = tb.y + fy * tb.height;
    // Ancestor prefix root→target, target excluded (pushed below). Broken
    // chains fall through to the main hit test.
    let mut path = ancestor_prefix(nodes, id_to_idx, root, target_idx)?;
    path.push(target_idx);
    // Child basis in layout space (scroll-corrected, like recursion).
    let (sdx, sdy) = scroll_offset_for_node(&nodes[target_idx]);
    let (cpx, cpy) = (tb.x - sdx, tb.y - sdy);
    for &c in nodes[target_idx].children.iter().rev() {
        if hit_test_recursive(nodes, c, tx, ty, cpx, cpy, &mut path) {
            break;
        }
    }
    Some(path)
}

/// Elevated live endpoint (Target/Morph painted by the transition layer):
/// same remap contract as [`hit_test_recursive`], minus ancestor rejection.
fn hit_through_elevated(
    nodes: &[LayoutNode],
    root: usize,
    id_to_idx: &std::collections::HashMap<u64, usize>,
    idx: usize,
    x: f32,
    y: f32,
) -> Option<Vec<usize>> {
    let node = &nodes[idx];
    let t = node.transition.as_ref()?;
    let l = t.lerped();
    if l.width <= 0.0 || l.height <= 0.0 {
        return None;
    }
    // Absolute position in the same frame the layer paints in.
    let tb = abs_rect_upward(nodes, id_to_idx, idx);
    let (w, h) = (node.measured_size.width, node.measured_size.height);
    let (lx, ly) = t.remap_hit(x, y, tb.x, tb.y, w, h)?;
    // The endpoint's OWN viewport clamp still applies — only ancestors are
    // escaped (a scrollable hero must not take input outside its viewport).
    let mut nw = w;
    let mut nh = h;
    if node.scroll_viewport_width > 0.0 {
        nw = node.scroll_viewport_width;
    }
    if node.scroll_viewport_height > 0.0 {
        nh = node.scroll_viewport_height;
    }
    if lx < tb.x || lx > tb.x + nw || ly < tb.y || ly > tb.y + nh {
        return None;
    }
    let mut path = ancestor_prefix(nodes, id_to_idx, root, idx)?;
    path.push(idx);
    // Child basis: same convention as hit_test_recursive (parent minus its own
    // scroll translate), so descendants resolve identically in-tree and in the
    // layer.
    let (sdx, sdy) = scroll_offset_for_node(node);
    let (cpx, cpy) = (tb.x - sdx, tb.y - sdy);
    for &c in node.children.iter().rev() {
        if hit_test_recursive(nodes, c, lx, ly, cpx, cpy, &mut path) {
            break;
        }
    }
    Some(path)
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
    let mut nw = node.measured_size.width;
    let mut nh = node.measured_size.height;
    // scroll 容器：命中范围按可视 viewport 计——measured_size 是内容全高，
    // 否则滚动内容会在视口外拦截本应命中后续兄弟的点击/滚轮
    if node.scroll_viewport_height > 0.0 {
        nh = node.scroll_viewport_height;
    }
    if node.scroll_viewport_width > 0.0 {
        nw = node.scroll_viewport_width;
    }

    // Flight remap (Phase 3): transitioning endpoints test their lerped
    // visual rect; hits descend in layout space (children keep layout
    // positions — the flight transform is inverted here). Visual miss passes
    // through (v1 skip behavior).
    let (x, y) = match node.transition.as_ref() {
        Some(t) => match t.remap_hit(
            x,
            y,
            nx,
            ny,
            node.measured_size.width,
            node.measured_size.height,
        ) {
            Some(p) => p,
            None => return false,
        },
        None => (x, y),
    };

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

    // 深度优先：先检查子节点（子节点在父节点上方）；
    // ⚠ 兄弟节点**倒序**遍历——绘制按 children 正序（后画在上层），
    // 命中必须后画的优先（z-order 语义）；此前正序导致上层兄弟
    // （如全屏图片上的矩形）永远命中底层兄弟（Image 铺满遮挡）
    for &c in node.children.iter().rev() {
        // Shared-element flight (Phase 3): live endpoints (Target/Morph)
        // descend via remap_hit above, so their subtrees stay hittable
        // mid-flight. Only Source-role visuals are skipped — detached
        // retained sources are unreachable from the root walk anyway, and a
        // Source visual must never take input (its ghost routes to the live
        // target through the transition-roots prefix instead).
        if nodes[c].transition.as_ref().is_some_and(|t| t.role == TransitionRole::Source) {
            continue;
        }
        if hit_test_recursive(nodes, c, x, y, child_px, child_py, path) {
            return true;
        }
    }

    // 没有命中子节点，停在当前节点
    true
}

/// 场景坐标 → 目标节点的本地坐标（相对节点左上角）。
///
/// 与 `hit_test_recursive` 走同一条路径：从根累加 position，并减去
/// 祖先 scroll 偏移（渲染时 scroll 容器 translate(-offset)）。用于
/// 波纹按压点存储——对标 Compose `PressInteraction.Press.pressPosition`
/// 的本地坐标语义（绘制时加回布局原点，滚动/图形层变换后波纹跟随节点）。
/// graphics_layer 变换暂不计——与命中测试行为一致（命中本身未反变换 GL）。
///
/// ⚠ `node_abs_position`（app.rs）与 `hit_test` 也走同一坐标空间——修 scroll
/// 时须同步（多行 TextField 点击定位依赖一致的"滚动画布坐标"）。
/// scroll 容器的滚动偏移量（场景坐标 → 内容坐标转换用）。
///
/// ⚠ RTL（scroll_reverse）：render 平移是 `off = content_w - viewport_w - offset`
///（render.rs:746-748 镜像），hit_test 的坐标转换必须用同一个 off——否则
/// 点击检测位置错位（用户实测：RTL 下滚动 tab 点击触发位置不对）。
pub(crate) fn scroll_offset_for_node(node: &LayoutNode) -> (f32, f32) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    if let Some(state) = node.modifier.vertical_scroll_state() {
        dy += state.offset.get();
    }
    if let Some(state) = node.modifier.horizontal_scroll_state() {
        dx += if node.scroll_reverse {
            // RTL：offset 语义被镜像——内容平移量 = 总宽 - 视口 - offset
            //（content_w 首帧可能未回写为 0，此时退化为 -offset→max(0)=0，
            // 与 render 首帧行为一致）
            let content_w = if node.scroll_content_width > 0.0 { node.scroll_content_width } else { 0.0 };
            (content_w - node.scroll_viewport_width - state.offset.get()).max(0.0)
        } else {
            state.offset.get()
        };
    }
    (dx, dy)
}

/// 场景坐标 → 目标节点的本地坐标（相对节点左上角）。
pub(crate) fn scene_to_node_local(
    nodes: &[LayoutNode],
    path: &[usize],
    target: usize,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let mut ox = 0.0;
    let mut oy = 0.0;
    for &i in path {
        let node = &nodes[i];
        ox += node.position.x;
        oy += node.position.y;
        if i == target {
            break;
        }
        let (dx, dy) = scroll_offset_for_node(node);
        ox -= dx;
        oy -= dy;
    }
    (x - ox, y - oy)
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
    fn test_hit_test_z_order_topmost_first() {
        // 兄弟节点：绘制按 children 正序（后画在上层）——命中必须倒序优先
        // 底层铺满（leaf1）+ 上层局部矩形（leaf2）重叠点 → 应命中 leaf2
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(40.0, 40.0)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[2].measured_size = Size::new(40.0, 40.0);
        nodes[1].position = Point::new(0.0, 0.0);
        nodes[2].position = Point::new(30.0, 30.0);
        nodes[0].children = vec![1, 2];

        // (40,40) 同时落在 leaf1 与 leaf2 内——上层（后画 leaf2）优先
        let path = hit_test(&nodes, 0, 40.0, 40.0);
        assert_eq!(path, vec![0, 2], "上层兄弟优先命中（z-order）");
        // (20,20) 只落 leaf1（leaf2 范围外）
        let path2 = hit_test(&nodes, 0, 20.0, 20.0);
        assert_eq!(path2, vec![0, 1]);
    }

    #[test]
    fn test_hit_test_miss() {
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 100.0))];
        nodes[0].measured_size = Size::new(100.0, 100.0);

        let path = hit_test(&nodes, 0, 150.0, 50.0);
        assert_eq!(path.len(), 0);
    }

    #[test]
    fn test_scroll_offset_rtl_reverse_mirrors() {
        // RTL（scroll_reverse）回归：hit_test 坐标转换必须用 render 的镜像
        // 偏移量 off = content_w - viewport_w - offset，而非裸 offset——
        // 否则 RTL 滚动 tab 的点击位置错位（用户实测 bug）
        let state = crate::modifier::ScrollState::new();
        let m = Modifier::new().horizontal_scroll(state.clone());
        let mut node = LayoutNode::leaf(m);
        node.scroll_reverse = true;
        node.scroll_content_width = 1000.0;
        node.scroll_viewport_width = 360.0;

        // offset=0 → 镜像偏移 = 1000-360-0 = 640（render 显示内容末端）
        state.offset.set(0.0);
        let (dx, dy) = scroll_offset_for_node(&node);
        assert!((dx - 640.0).abs() < 0.01, "RTL offset=0 应镜像为 640，实际 {dx}");
        assert_eq!(dy, 0.0);

        // offset=100 → 640-100 = 540
        state.offset.set(100.0);
        let (dx, _) = scroll_offset_for_node(&node);
        assert!((dx - 540.0).abs() < 0.01, "RTL offset=100 应镜像为 540，实际 {dx}");

        // LTR（非 reverse）对照：偏移 = 裸 offset
        node.scroll_reverse = false;
        state.offset.set(100.0);
        let (dx, _) = scroll_offset_for_node(&node);
        assert!((dx - 100.0).abs() < 0.01, "LTR 应裸 offset=100，实际 {dx}");
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

    /// 回归：滚动容器内 scene 坐标（hit_test 输入）与 node_abs_position
    /// （app.rs 点击/拖拽定位用）必须同一空间——都减祖先 scroll 偏移。
    /// 此前 node_abs_position 纯累加 layout position，滚动后局部 y 被
    /// 滚动量污染 → 多行 TextField 点击/拖拽只能定位到第一行。
    #[test]
    fn test_scroll_scene_coords_consistent() {
        // 滚动容器（y=0，滚动 50px）内含子节点（y=100，内容高 60）
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(50.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).size(100.0, 200.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 60.0)),
        ];
        nodes[0].measured_size = Size::new(100.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 60.0);
        nodes[1].position = Point::new(0.0, 100.0);
        nodes[0].children.push(1);

        // 渲染时画布 translate(0, -50) → 子节点视觉顶边在场景 y=50
        let path = hit_test(&nodes, 0, 50.0, 60.0);
        assert_eq!(path, vec![0, 1], "滚动后子节点视觉范围 (50,50)-(50,110) 应命中");
        // 子节点本地坐标 = 场景 - 视觉顶边 = (50-0, 60-50) = (50, 10)
        let (x, y) = scene_to_node_local(&nodes, &path, 1, 50.0, 60.0);
        assert_eq!((x, y), (50.0, 10.0), "本地坐标须扣除滚动偏移");

        // 未滚动点（y=140 → 场景 140-50=90，仍命中子节点本地 y=40）
        let path2 = hit_test(&nodes, 0, 50.0, 90.0);
        assert_eq!(path2, vec![0, 1]);
        let (_, y2) = scene_to_node_local(&nodes, &path2, 1, 50.0, 90.0);
        assert_eq!(y2, 40.0);

        // 滚动偏移变化 → 同一场景点对应不同本地坐标（偏移已消耗）。
        // offset=50：视觉顶边 y=50，场景 (50,50) → 本地 y=0；
        // offset=100：视觉顶边 y=0，场景 (50,50) → 本地 y=50（仍在可视范围）
        scroll.offset.set(100.0);
        let path3 = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path3, vec![0, 1]);
        let (_, y3) = scene_to_node_local(&nodes, &path3, 1, 50.0, 50.0);
        assert_eq!(y3, 50.0, "本地坐标随滚动偏移同步变化");
    }

    // ── scene_to_node_local（波纹按压点本地坐标）──

    #[test]
    fn scene_to_node_local_plain() {
        // root(10,20) → child(30,40)：视觉原点 (40,60)
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0)),
        ];
        nodes[0].position = Point::new(10.0, 20.0);
        nodes[1].position = Point::new(30.0, 40.0);
        nodes[0].children.push(1);

        let (lx, ly) = scene_to_node_local(&nodes, &[0, 1], 1, 45.0, 62.0);
        assert_eq!((lx, ly), (5.0, 2.0), "无滚动：本地 = 场景 - 布局原点");
    }

    #[test]
    fn scene_to_node_local_under_scroll() {
        // root(0,0) → scroll 容器(50,100, offset=50) → child(100,0)：
        // 视觉原点 = (50+100-0, 100+0-50) = (150, 50)
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(50.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(300.0, 300.0)),
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0).vertical_scroll(scroll)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0)),
        ];
        nodes[0].position = Point::new(0.0, 0.0);
        nodes[1].position = Point::new(50.0, 100.0);
        nodes[2].position = Point::new(100.0, 0.0);
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let (lx, ly) = scene_to_node_local(&nodes, &[0, 1, 2], 2, 160.0, 60.0);
        assert_eq!((lx, ly), (10.0, 10.0), "滚动偏移从视觉原点扣除（与 hit_test/渲染一致）");
    }

    #[test]
    fn collect_focus_candidates_uses_visual_centers() {
        // root(0,0) → scroll 容器(50,100, offset=50) → focusable(100,0, 20x20)
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(50.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(300.0, 300.0)),
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0).vertical_scroll(scroll)),
            LayoutNode::leaf(Modifier::new().size(20.0, 20.0).focusable()),
        ];
        nodes[0].position = Point::new(0.0, 0.0);
        nodes[1].position = Point::new(50.0, 100.0);
        nodes[2].position = Point::new(100.0, 0.0);
        // 测试不执行 measure——显式设置测量尺寸（与 scene_to_node_local 系列一致）
        nodes[0].measured_size = Size::new(300.0, 300.0);
        nodes[1].measured_size = Size::new(200.0, 200.0);
        nodes[2].measured_size = Size::new(20.0, 20.0);
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let cands = collect_focus_candidates(&nodes, 0);
        assert_eq!(cands.len(), 1, "仅 focusable 子节点进入候选");
        // 视觉位置 = (150, 50)，中心 = (160, 60)（scroll offset 已扣除）
        assert_eq!(cands[0].0, nodes[2].id);
        assert_eq!((cands[0].1, cands[0].2), (160.0, 60.0));
    }

    // ── aspectRatio / requiredSize ──

    #[test]
    fn required_size_overrides_incoming() {
        // 父约束 100x100，requiredSize(200, 50) → 溢出（enforceIncoming=false）
        let m = Modifier::new().required_size(200.0, 50.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (200.0, 50.0), "required 忽略 incoming 收缩");
    }

    #[test]
    fn required_width_only() {
        let m = Modifier::new().required_width(300.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 300.0, "requiredWidth 溢出");
        assert!(size.height <= 100.0, "高度仍受 incoming 约束");
    }

    #[test]
    fn aspect_ratio_derives_from_max_width() {
        // 无 size 约束：外层 max 300x300，ratio 2 → 宽 300 高 150
        let m = Modifier::new().aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (300.0, 150.0), "以 max_w 为基准推导");
    }

    #[test]
    fn aspect_ratio_respects_inner_fixed_size() {
        // 链内 size(50,20) 是 tight（min=max）——aspect 推导 40x20 被
        // clamp 回 min——Compose 同（tight size 下 aspect 无法改变尺寸）
        let m = Modifier::new().size(50.0, 20.0).aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (50.0, 20.0), "tight size 约束 aspect 不可改变（Compose 一致）");
    }

    #[test]
    fn aspect_ratio_clamps_to_max_height() {
        // 无 size：外层 max 300x100，ratio 2 → h = 300/2 = 150 > 100 → clamp：(200, 100)
        let m = Modifier::new().aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (200.0, 100.0), "高度超界时反推宽度");
    }

    #[test]
    fn aspect_ratio_match_height_first() {
        // match_height_first：以 max_h 为基准——无 size 时外层 100x300，ratio 2
        // → h = 300，w = 600 > 100 → clamp：(100, 50)
        let m = Modifier::new().aspect_ratio(2.0, true);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (100.0, 50.0), "match_height_first 反推");
    }

    // ── minWidth / minHeight（对标 Compose widthIn/heightIn）──

    #[test]
    fn min_width_height_raises_min_constraints() {
        // min_width(58).min_height(40) + 空内容 → 撑到 58x40（Button 默认最小尺寸）
        let m = Modifier::new().min_width(58.0).min_height(40.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (58.0, 40.0), "min 约束提升 incoming min");
    }

    #[test]
    fn min_width_yields_to_tight_size() {
        // tight size(30,20) 之后 min(58) 被 max=30 夹住（Compose constraints 合并语义）
        let m = Modifier::new().size(30.0, 20.0).min_width(58.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (30.0, 20.0), "min 不得越过 tight max");
    }

    #[test]
    fn min_width_dynamic_state() {
        // 动态 min（动画）：measure 期 get() 注册布局依赖——值变化重测生效
        use crate::core::state::State;
        let s = State::new(58.0);
        let m = Modifier::new().min_width(&s);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 58.0);
        s.set(70.0);
        // 模拟动画值 notify 后的布局失效（真实链路：composer pending → layout_dirty）
        nodes[0].layout_dirty = true;
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 70.0, "动态 min 值变化后重测取新值");
    }

    // ── test_tag ──

    #[test]
    fn test_tag_stored_and_queried() {
        let m = Modifier::new().test_tag("btn-submit");
        assert_eq!(m.get_test_tag(), Some("btn-submit"));
        let m2 = Modifier::new();
        assert_eq!(m2.get_test_tag(), None);
    }

    #[test]
    fn test_tag_in_param_eq() {
        let a = Modifier::new().test_tag("a");
        let b = Modifier::new().test_tag("b");
        assert!(!a.param_eq(&b), "tag 不同必须不等");
        let c = Modifier::new().test_tag("a");
        assert!(a.param_eq(&c), "tag 相同必须相等");
    }

    #[test]
    fn test_leaf_padding_included_in_measured_size() {
        // 叶子节点（无 policy）尺寸回加 padding（此前丢失：测量不含内边距，
        // Text+padding 高度塌陷、内容贴边）
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 50.0).padding(10.0))];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        // size(100,50) 经 padding(10) 内缩 → 内容 80x30，回加后 100x50
        assert_eq!(size, Size::new(100.0, 50.0));
        assert_eq!(nodes[0].measured_size, Size::new(100.0, 50.0));
    }

    #[test]
    fn test_leaf_asymmetric_padding_included() {
        // 非对称 padding：start(10)+bottom(5) + 固定 size(60,40)
        // → 内容区 50x35 + padding 回加 = 总尺寸 60x40（修改前返回 50x35 丢失 padding）
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new().size(60.0, 40.0).padding_start(10.0).padding_bottom(5.0),
        )];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(60.0, 40.0));
        // 无固定尺寸的纯 padding 叶子：内容 0 + padding 回加
        let mut nodes2 = vec![LayoutNode::leaf(Modifier::new().padding_start(10.0).padding_bottom(5.0))];
        let (size2, _) = measure_node(&mut nodes2, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size2, Size::new(10.0, 5.0));
    }

    // ── Image 叶子测量（固有尺寸布局——对齐 Compose：未指定维度以固有尺寸为基准）──

    fn image_modifier() -> Modifier {
        use crate::ui::image::{ContentScale, ImageAlignment};
        Modifier::new().image_content(
            crate::ui::icon::IconSource::svg("<svg viewBox=\"0 0 48 24\"/>"),
            ContentScale::Fit,
            ImageAlignment::Center,
            1.0,
            None,
            crate::modifier::FilterQuality::Low,
        )
    }

    #[test]
    fn test_leaf_image_intrinsic_size() {
        // 固有尺寸来自 SVG viewBox（纯内存，无需文件）
        let mut nodes = vec![LayoutNode::leaf(image_modifier())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(48.0, 24.0), "UNBOUNDED 下按固有尺寸");
        assert_eq!(nodes[0].measured_size, Size::new(48.0, 24.0));
    }

    #[test]
    fn test_leaf_image_tight_constraints_clamped() {
        // tight 100x50：无 size modifier 时被约束钳制
        let mut nodes = vec![LayoutNode::leaf(image_modifier())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(100.0, 100.0, 50.0, 50.0));
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    #[test]
    fn test_leaf_image_size_modifier_overrides() {
        // modifier size 覆盖固有尺寸（Compose 语义：size 指定即以此为准）
        let m = Modifier::new().size(64.0, 32.0).image_content(
            crate::ui::icon::IconSource::svg("<svg viewBox=\"0 0 48 24\"/>"),
            crate::ui::image::ContentScale::Fit,
            crate::ui::image::ImageAlignment::Center,
            1.0,
            None,
            crate::modifier::FilterQuality::Low,
        );
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(64.0, 32.0));
    }

    #[test]
    fn test_leaf_image_svg_without_viewbox_fallback() {
        // 无 viewBox/width 的 SVG：测量回退 24×24（与解码回退一致——
        // 否则 Image 测量 0 尺寸空白而 Icon 正常显示的不一致）
        let m = Modifier::new().image_content(
            crate::ui::icon::IconSource::svg(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h24v24H0z\"/></svg>",
            ),
            crate::ui::image::ContentScale::Fit,
            crate::ui::image::ImageAlignment::Center,
            1.0,
            None,
            crate::modifier::FilterQuality::Low,
        );
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(24.0, 24.0));
    }

    #[test]
    fn test_measure_node_rtl_padding() {
        // RTL + padding_start(10)：start 在右——子内容靠右 10（左侧空隙 0）
        use crate::layout::row::RowLayout;
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new()
                    .size(100.0, 50.0)
                    .layout_direction(LayoutDirection::Rtl)
                    .padding_start(10.0),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl; // 模拟物化快照（modifier 覆盖）
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        // 内容区宽 = 100-10 = 90；RTL 第一个子（30 宽）在右端 → x = 60
        // padding 偏移（Rtl 用 end=0）→ 不变
        assert_eq!(nodes[1].position.x, 60.0, "RTL 下 start 在右：子内容靠右，右侧空隙 10");
        // 右侧空隙 = 100 - (60+30) = 10 ✓（padding_start 生效在右）
    }

    #[test]
    fn test_measure_node_rtl_padding_end_left() {
        // RTL + padding_end(20)：end 在左——子内容从右侧排，左侧空隙 20
        use crate::layout::row::RowLayout;
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new()
                    .size(100.0, 50.0)
                    .layout_direction(LayoutDirection::Rtl)
                    .padding_end(20.0),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl; // 模拟物化快照
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        // 内容区宽 = 100-20 = 80；flex RTL 第一个子 x = 80-30 = 50
        // padding 偏移（Rtl 用 end=20）→ 50+20 = 70
        assert_eq!(nodes[1].position.x, 70.0, "RTL 下 end 在左：子靠右排，左侧空隙 20");
    }

    #[test]
    fn test_measure_node_ltr_padding_unaffected() {
        // LTR 对照组：padding_start(10) → 子靠左 10
        use crate::layout::row::RowLayout;
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new().size(100.0, 50.0).padding_start(10.0),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 10.0, "LTR 下 start 在左：子靠左 10");
    }

    #[test]
    fn test_measure_node_offset_rtl_mirror() {
        // 普通 offset 在 RTL 下 x 镜像（对标 Compose offset）；absolute_offset 豁免
        use crate::layout::row::RowLayout;
        // LTR：offset(10, 5) → 子 x 加 10
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).offset(10.0, 5.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 10.0, "LTR offset(10) 向右");
        assert_eq!(nodes[1].position.y, 5.0, "offset y 不受方向影响");

        // RTL：offset(10) 镜像 → x 减 10
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).offset(10.0, 5.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl;
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 60.0, "RTL offset(10) x 镜像（flex 70 - 10）");

        // RTL + absolute_offset：不镜像 → x 加 10
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).absolute_offset(10.0, 0.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl;
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 80.0, "RTL absolute_offset(10) 不镜像（flex 70 + 10）");
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

/// 收集所有可聚焦节点及其**视觉中心**（绝对坐标，含祖先 scroll 偏移——
/// 与 hit_test 同路径）——方向键焦点导航用。
pub fn collect_focus_candidates(nodes: &[LayoutNode], root: usize) -> Vec<(u64, f32, f32)> {
    let mut out = Vec::new();
    collect_focus_candidates_rec(nodes, root, 0.0, 0.0, &mut out);
    out
}

fn collect_focus_candidates_rec(
    nodes: &[LayoutNode],
    idx: usize,
    parent_x: f32,
    parent_y: f32,
    out: &mut Vec<(u64, f32, f32)>,
) {
    let node = &nodes[idx];
    let nx = parent_x + node.position.x;
    let ny = parent_y + node.position.y;
    if has_focusable_modifier(node) {
        out.push((
            node.id,
            nx + node.measured_size.width / 2.0,
            ny + node.measured_size.height / 2.0,
        ));
    }
    let (sdx, sdy) = scroll_offset_for_node(node);
    let children = node.children.clone();
    for c in children {
        collect_focus_candidates_rec(nodes, c, nx - sdx, ny - sdy, out);
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
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::Focusable { .. }))
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
/// 应用布局失效：DFS 树，命中 layout_dirty_keys 的节点标 layout_dirty=true 并沿祖先链传播。
/// 保守超集：祖先全链标脏（布局动画场景父必然依赖子尺寸；Compose 精确传播留待优化）。
pub(crate) fn apply_layout_dirty(nodes: &mut [LayoutNode], root_idx: usize, dirty_keys: &std::collections::HashSet<u64>) {
    fn walk(nodes: &mut [LayoutNode], idx: usize, dirty_keys: &std::collections::HashSet<u64>, ancestor_dirty: bool) -> bool {
        let hit = dirty_keys.contains(&nodes[idx].slot_key);
        if hit || ancestor_dirty {
            nodes[idx].layout_dirty = true;
        }
        // 索引读避免 clone（layout 是热路径）；每次索引读是临时借用，不阻塞递归写
        let mut child_hit = false;
        let n = nodes[idx].children.len();
        for i in 0..n {
            let child = nodes[idx].children[i];
            if walk(nodes, child, dirty_keys, hit || ancestor_dirty) {
                child_hit = true;
            }
        }
        if child_hit {
            nodes[idx].layout_dirty = true;
        }
        hit || child_hit
    }
    walk(nodes, root_idx, dirty_keys, false);
}

/// 测量计数（测试用：验证常量折叠/布局失效路径确实跳过或执行 measure）。
/// thread_local 隔离——cargo test 并行线程互不串扰（全局 Atomic 会跨测试计数破坏断言）。
#[cfg(test)]
thread_local! {
    pub(crate) static MEASURE_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub(crate) fn measure_node(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // 重放 stub：clean-skip 节点无 measure_policy，绝不能重新测量
    //（无 policy 走叶子分支会返回 0 并污染 prev_nodes 缓存，导致塌缩不可逆）。
    // stub 只在 slot 真正 clean（无状态变化）时出现；约束若变化，下帧该 slot dirty → Enter 正常重建。
    // 常量折叠：若节点未变脏、无布局失效且约束相同，直接复用上次结果
    //（layout_dirty：两段式依赖——布局动画值变化只重测不重组）
    if !nodes[idx].dirty && !nodes[idx].layout_dirty && nodes[idx].cached_constraints == Some(constraints) {
        return (nodes[idx].measured_size, Vec::new());
    }

    // 真正执行 measure 才计数（常量折叠命中不计——测试验证折叠路径）
    #[cfg(test)]
    MEASURE_COUNT.with(|c| c.set(c.get() + 1));

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

    // 开放布局节点 A 型（exp/modifier-node）：resolved_size 之后串行变换约束。
    // 位置语义 = 链中 size 之后、min/required/padding 之前（后续 min/required
    // 仍可覆盖 node 的变换，链序直觉保持）。动态值在 transform 内求值
    // （measure 期 State::get 注册布局依赖——与 SizeValue::Dynamic 同机制）。
    // Arc 克隆出链表避免借用冲突（nodes[idx] 不可变借用与后续可变写冲突）。
    // P2-2：无 node 时早退（全树每节点每次 measure 省一次 collect 分配）。
    let layout_transforms: Vec<std::sync::Arc<dyn crate::modifier::LayoutNode>> =
        if nodes[idx].modifier.has_layout_nodes() {
            nodes[idx].modifier.layout_nodes().cloned().collect()
        } else {
            Vec::new()
        };
    for t in &layout_transforms {
        inner_constraints = t.transform(inner_constraints);
    }

    // 最小尺寸（MinWidth/MinHeight——对标 Compose widthIn/heightIn）：
    // 提升 incoming min，受 max 夹住（min 不得越过 max——tight size 下
    // 最小约束让位于固定尺寸，与 Compose constraints 合并语义一致）。
    let (min_w, min_h) = nodes[idx].modifier.min_size_constraint();
    if let Some(w) = min_w {
        inner_constraints.min_width = inner_constraints.min_width.max(w).min(inner_constraints.max_width);
    }
    if let Some(h) = min_h {
        inner_constraints.min_height = inner_constraints.min_height.max(h).min(inner_constraints.max_height);
    }

    // 强制尺寸（requiredSize——忽略 incoming 收缩，允许溢出：
    // min/max 直接覆盖 incoming，Compose enforceIncoming=false 语义）。
    // ⚠ 必须在 resolved_size **之后**执行：否则 size() 的 tighten 会反超
    // required（链序反转——Compose requiredSize 固定大小最终胜出）。
    if let Some((rw, rh)) = nodes[idx].modifier.required_size_constraint() {
        if let Some(w) = rw {
            inner_constraints.min_width = w;
            inner_constraints.max_width = w;
        }
        if let Some(h) = rh {
            inner_constraints.min_height = h;
            inner_constraints.max_height = h;
        }
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
    let (pad_start, pad_end) = nodes[idx].modifier.get_padding_horizontal();
    let (pad_top, pad_bottom) = nodes[idx].modifier.get_padding_vertical();
    let pad_x = pad_start + pad_end;
    let pad_y = pad_top + pad_bottom;
    if pad_x > 0.0 || pad_y > 0.0 {
        inner_constraints = inner_constraints.offset(pad_x, pad_y);
    }

    // 3. 应用 FillMax 约束（在 scroll 修改 max 之前，保存 viewport 约束）
    let viewport_height = inner_constraints.max_height;
    let viewport_width = inner_constraints.max_width;
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
        // lazy 列表：内容总高 State → 节点字段（apply_scroll_delta 用；在测量前读，
        // 值是上一帧测量回写的——首帧 0 退化到节点自身高度）
        if let Some(ch) = nodes[idx].modifier.lazy_scroll_content_height() {
            nodes[idx].scroll_content_height = ch.get();
            nodes[idx].scroll_reverse = nodes[idx].modifier.is_lazy_scroll_reverse();
        }
        // ⚠ lazy 容器**不**改写成无界：policy 自己显式控制子约束（child_constraints
        // 高度 ∞），保留有限 max_height 让 policy 拿到真实视口高——内部 clamp 的
        // max_offset = content_h - vh 才正确（实测：回退 vh=600 > 真实视口 ~500，
        // 跳末尾滚过头，Item 999 被推出视口底部）。普通 scroll 容器仍改写（子内容
        // wrap）。
        if nodes[idx].modifier.lazy_scroll_content_height().is_none() {
            inner_constraints.max_height = f32::MAX;
        }
    }
    if nodes[idx].modifier.horizontal_scroll_state().is_some() {
        // scroll 容器自身填 viewport（fill_max_width 在无限 max 时跳过，这里补上）
        if nodes[idx].modifier.is_fill_max_width() && inner_constraints.max_width >= f32::MAX {
            inner_constraints.min_width = viewport_width;
        }
        // 保存 viewport 宽度供滚动 clamping 使用
        nodes[idx].scroll_viewport_width = viewport_width;
        // 水平反向滚动（RTL：render 平移镜像——offset 0 显示内容末端，对齐垂直 reverseLayout；
        // LazyRow 的 reverse_layout 打 lazy_scroll_reverse 标记（与垂直同构），
        // 此处同样认领——否则水平反向有 placement 镜像、无 render 镜像，
        // offset 两端语义不一致。非懒水平容器无 LazyScroll 元素，|| 安全。
        nodes[idx].scroll_reverse = nodes[idx].modifier.is_horizontal_scroll_reverse()
            || nodes[idx].modifier.is_lazy_scroll_reverse();
        // lazy 横向列表：内容总宽 State → 节点字段（与垂直同语义）
        if let Some(cw) = nodes[idx].modifier.lazy_scroll_content_height() {
            nodes[idx].scroll_content_width = cw.get();
        }
        // ⚠ lazy 容器不改写成无界：保留有限 max_width 让 policy 拿真实视口宽
        if nodes[idx].modifier.lazy_scroll_content_height().is_none() {
            inner_constraints.max_width = f32::MAX;
        }
    }

    // 实际测量
    let mut result = if let Some(pidx) = nodes[idx].measure_policy {
        // 先拷贝子节点索引（policy.measure 会可变借用整个 nodes，不能持有 nodes[idx] 借用）
        let children = nodes[idx].children.clone();
        let (size, placements) = policies[pidx].measure(nodes, policies, &children, inner_constraints);
        // 测量后同步 lazy 内容总高/宽到节点字段（render 的 reverse translate 与
        // apply_scroll_delta 依赖；measure_node 顶部读的是上一帧值——首帧为 0
        // 会让 reverseLayout 首帧平移错误，内容整体被推出视口）
        let content_h_state = nodes[idx].modifier.lazy_scroll_content_height().map(|s| s.get());
        if let Some(v) = content_h_state {
            nodes[idx].scroll_content_height = v;
            nodes[idx].scroll_content_width = v;
        }
        // apply positions
        policies[pidx].place(nodes, &children, &placements);
        // apply padding offset（RTL：start 在右——子靠右偏移）
        let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
        let left_offset = if rtl { pad_end } else { pad_start };
        if left_offset > 0.0 || pad_top > 0.0 {
            for &c in &children {
                nodes[c].position.x += left_offset;
                nodes[c].position.y += pad_top;
            }
        }
        // apply per-child offset modifier（普通 offset 在 RTL 下 x 镜像——
        // 对标 Compose；absolute_offset 豁免镜像）
        let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
        for &c in &children {
            if let Some((ox, oy)) = nodes[c].modifier.get_offset() {
                nodes[c].position.x += if rtl { -ox } else { ox };
                nodes[c].position.y += oy;
            }
            if let Some((ax, ay)) = nodes[c].modifier.get_absolute_offset() {
                nodes[c].position.x += ax;
                nodes[c].position.y += ay;
            }
        }
        // 非 lazy 垂直滚动容器：内容总高 = 子节点底部最大值（含底部 padding）——
        // 供 apply_scroll_delta 的 max_offset 与 fling 极限。修复 fill 容器场景：
        // 自身高度 = 视口 → 原 measured_size 高度算 max_offset 恒 0（无法滚动）
        if nodes[idx].modifier.vertical_scroll_state().is_some()
            && nodes[idx].modifier.lazy_scroll_content_height().is_none()
        {
            let mut content_h = 0.0f32;
            for &c in &children {
                let b = nodes[c].position.y + nodes[c].measured_size.height;
                if b > content_h { content_h = b; }
            }
            if content_h > 0.0 {
                content_h += pad_bottom;
                nodes[idx].scroll_content_height = content_h;
                let max_off = (content_h - nodes[idx].scroll_viewport_height).max(0.0);
                if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                    ss.fling_limit.set(max_off);
                }
            }
        }
        // 非 lazy 水平滚动容器：内容总宽 = 子节点右侧最大值（含尾部 padding）
        if nodes[idx].modifier.horizontal_scroll_state().is_some()
            && nodes[idx].modifier.lazy_scroll_content_height().is_none()
        {
            let mut content_w = 0.0f32;
            for &c in &children {
                let right = nodes[c].position.x + nodes[c].measured_size.width;
                if right > content_w { content_w = right; }
            }
            if content_w > 0.0 {
                content_w += pad_end;
                nodes[idx].scroll_content_width = content_w;
                let max_off = (content_w - nodes[idx].scroll_viewport_width).max(0.0);
                if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                    ss.fling_limit.set(max_off);
                }
            }
        }
        let mut outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        // scroll 容器自身尺寸 clamp 回视口——第 4 步为子节点把约束改成了无界
        // （max=f32::MAX），若不做 clamp，自身高度/宽度会被内容撑开（如
        // `.height(180.0)` 的滚动列表实测 991 高）。viewport_* 保存于第 3 步
        // （padding 已扣、size 约束已收窄），故完整视口高 = viewport + pad。
        // lazy 容器不改写约束（见第 4 步注释），其 policy 已按有限视口测量，
        // 此处 clamp 对它们是无操作（内容本来就 ≤ 视口）。若父约束无限
        // （viewport=f32::MAX），min 无效果，仍按内容撑开（语义：无固定高度
        // 的滚动容器随内容增长）。
        if nodes[idx].modifier.vertical_scroll_state().is_some() && viewport_height < f32::MAX {
            outer_size.height = outer_size.height.min(viewport_height + pad_y);
        }
        if nodes[idx].modifier.horizontal_scroll_state().is_some() && viewport_width < f32::MAX {
            outer_size.width = outer_size.width.min(viewport_width + pad_x);
        }
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
            // 支持文本（TextField supporting——渲染画在容器底部外 4dp，
            // 高度 +20 预留，防与下方元素重叠）
            let supporting_h = if nodes[idx].modifier.elements().iter().any(|el| {
                matches!(el, crate::modifier::ModifierElement::TextFieldVisual { supporting: Some(_), .. })
            }) { 4.0 + 16.0 } else { 0.0 };
            // 用约束 clamping 最终尺寸（fill_max_width 时约束收紧，文本应填满可用宽度）。
            // ⚠ 高度 = paragraph 实际高度（含自动折行）+ supporting——不得按
            // 显式换行数近似（折行文本高度会裁剪）；min_height 提升兜底占位
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height + supporting_h),
            )
        } else if nodes[idx].has_richtext_content {
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_richtext(&nodes[idx], layout_width);
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height),
            )
        } else if nodes[idx].has_image_content {
            // Image 叶子：固有尺寸（位图像素 / SVG viewBox），约束钳制
            let (iw, ih) = nodes[idx].modifier.image_intrinsic_size().unwrap_or((0.0, 0.0));
            Size::new(
                inner_constraints.constrain_width(iw),
                inner_constraints.constrain_height(ih),
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
        // 叶子尺寸回加 padding（容器路径 1279 同语义——叶子此前丢失：
        // 测量尺寸不含内边距 → Text+padding 高度塌陷、内容贴边）
        let outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        nodes[idx].measured_size = outer_size;
        (outer_size, Vec::new())
    };

    // aspectRatio：测量后按 inner_constraints（含 size/required 链内收紧）
    // 推导节点尺寸（对标 Compose——aspect 的 incoming = 链中 aspect 位置的
    // 约束；固定一轴推另一轴 + clamp；内容按 inner_constraints 已排版，
    // 节点尺寸可能大于内容——留白正常）
    if let Some((ratio, match_height_first)) = nodes[idx].modifier.aspect_ratio_constraint() {
        let (min_w, max_w) = (inner_constraints.min_width, inner_constraints.max_width);
        let (min_h, max_h) = (inner_constraints.min_height, inner_constraints.max_height);
        let (w, h) = if match_height_first {
            if max_h < f32::MAX {
                let w = max_h * ratio;
                if w <= max_w { (w, max_h) } else { (max_w, max_w / ratio) }
            } else if max_w < f32::MAX {
                (max_w, max_w / ratio)
            } else {
                (result.0.width, result.0.height)
            }
        } else if max_w < f32::MAX {
            let h = max_w / ratio;
            if h <= max_h { (max_w, h) } else { (max_h * ratio, max_h) }
        } else if max_h < f32::MAX {
            (max_h * ratio, max_h)
        } else {
            (result.0.width, result.0.height)
        };
        let w = w.clamp(min_w, max_w);
        let h = h.clamp(min_h, max_h);
        result.0 = Size::new(w, h);
        nodes[idx].measured_size = result.0;
    }

    // 标记测量完成，缓存约束供下帧复用
    nodes[idx].dirty = false;
    nodes[idx].layout_dirty = false;
    nodes[idx].cached_constraints = Some(constraints);
    // 尺寸上报（对标 Compose onSizeChanged）——最终尺寸定型后回调
    // （常量折叠早退路径不经过这里——尺寸未变无需上报；元素内再去重）
    let (rw, rh) = (nodes[idx].measured_size.width, nodes[idx].measured_size.height);
    nodes[idx].modifier.report_measured_size(rw, rh);
    result
}

/// 构建普通文本段落（测量与绘制共用——单一事实来源）。
///
/// 从 TextStyle 参数构造 skia Paragraph（含 max_lines/ellipsis/justify/字重/倾斜），
/// 并按 soft_wrap 决定布局宽度。测量期（node.rs）与绘制兜底（render.rs）都调此函数，
/// 避免两处独立构造导致样式不一致。
pub(crate) fn build_plain_paragraph(
    content: &str,
    font_size: f32,
    color: &crate::modifier::Color,
    font_weight: crate::ui::text::FontWeight,
    font_style: crate::ui::text::FontSlant,
    max_lines: usize,
    align: crate::ui::TextAlign,
    overflow: crate::ui::TextOverflow,
    soft_wrap: bool,
    letter_spacing: f32,
    line_height: Option<f32>,
    max_width: f32,
) -> crate::text::Paragraph {
    use skia_safe::textlayout::ParagraphStyle;

    let mut para_style = ParagraphStyle::new();

    // max_lines：限制行数
    if max_lines < usize::MAX {
        para_style.set_max_lines(max_lines);
    }

    // ellipsis overflow：超出时显示省略号
    if overflow == crate::ui::TextOverflow::Ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    // justify alignment
    if align == crate::ui::TextAlign::Justify {
        para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify);
    }

    // soft_wrap=false: 无限宽度排版，不换行
    let layout_width = if soft_wrap { max_width } else { f32::MAX };

    let mut text_style = skia_safe::textlayout::TextStyle::new();
    text_style.set_font_size(font_size);
    // IMPORTANT: 设置文字颜色（Skia TextStyle 默认白色，不设的话画在白色背景上不可见）
    text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
    // 字间距（对标 Compose TextStyle.letterSpacing——逻辑像素）
    if letter_spacing != 0.0 {
        text_style.set_letter_spacing(letter_spacing);
    }
    // 行高（对标 Compose TextStyle.lineHeight——固定 px；skia 是倍数语义，
    // set_height(multiplier) + set_height_override(true)——override 才强制
    // 行高生效（与 RichText 路径 node.rs:1284 一致，否则被字体默认行高覆盖））
    if let Some(lh) = line_height {
        if lh > 0.0 && font_size > 0.0 {
            text_style.set_height(lh / font_size);
            text_style.set_height_override(true);
        }
    }
    // 设置字重和倾斜
    if font_weight != crate::ui::text::FontWeight::NORMAL || font_style != crate::ui::text::FontSlant::Upright {
        use skia_safe::FontStyle;
        use crate::ui::text::FontSlant;
        let slant = match font_style {
            FontSlant::Upright => skia_safe::font_style::Slant::Upright,
            FontSlant::Italic => skia_safe::font_style::Slant::Italic,
            FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
        };
        text_style.set_font_style(FontStyle::new(font_weight.value().into(), 5.into(), slant));
    }
    let fc = crate::font::get_font_collection();
    let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(content);
    let mut para = builder.build();
    para.layout(layout_width);
    para
}

/// 合并的文本测量 + Paragraph 缓存。
///
/// 从 TextContent modifier 中提取所有参数（font_size、max_lines、overflow、align），
/// 在 ParagraphStyle 上正确设置后一次创建 Paragraph，测量尺寸并缓存供渲染复用。
/// 消除旧代码中 `measurer.measure()` + `cache_text_paragraph()` 重复创建的开销。
fn measure_and_cache_text(node: &LayoutNode, max_width: f32) -> Size {
    for el in node.modifier.elements() {
        if let ModifierElement::TextContent {
            content, font_size, color, font_weight, font_style, max_lines, align, overflow, soft_wrap,
            letter_spacing, line_height,
        } = el {
            let para = build_plain_paragraph(
                content.as_str(),
                *font_size,
                color,
                *font_weight,
                *font_style,
                *max_lines,
                *align,
                *overflow,
                *soft_wrap,
                *letter_spacing,
                *line_height,
                max_width,
            );

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

    // 基线偏移（shift = font_size * multiplier）
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
