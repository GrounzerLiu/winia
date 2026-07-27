//! 布局节点 — LayoutNode 及相关的尺寸/位置/排列/对齐类型

use crate::modifier::{Modifier, ModifierElement};
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

// ── ContentMeasurer trait ──

/// 内容测量器 — 抽象叶子节点的内容测量逻辑
///
/// 不同内容类型（文本、图片、自定义绘制）实现此 trait，
/// 由 LayoutNode 持有，在 measure 时调用。
pub trait ContentMeasurer: Send + Sync {
    /// 测量内容在给定约束下的理想尺寸
    fn measure(&self, constraints: Constraints) -> Size;
}

/// 文本内容测量器
pub struct TextContentMeasurer {
    pub content: String,
    pub font_size: f32,
}

impl ContentMeasurer for TextContentMeasurer {
    fn measure(&self, constraints: Constraints) -> Size {
        let max_w = if constraints.has_fixed_width() {
            constraints.max_width
        } else {
            f32::MAX
        };
        let (size, _para) = measure_text_size(&self.content, self.font_size, max_w);
        // 注意：paragraph 无法通过 trait 返回（非 Send），由 measure_node 统一缓存
        size
    }
}

/// 从 Modifier 中提取 TextContent 创建 TextContentMeasurer
pub fn extract_content_measurer(modifier: &Modifier) -> Option<Box<dyn ContentMeasurer>> {
    for el in modifier.elements() {
        if let ModifierElement::TextContent { content, font_size, .. } = el {
            return Some(Box::new(TextContentMeasurer {
                content: content.clone(),
                font_size: *font_size,
            }));
        }
    }
    None
}

// ── LayoutNode ──

/// 布局树中的一个节点。
///
/// 每个 LayoutNode 对应 UI 树中的一个可测量/可布局的单元。
/// 包含 modifier 链、子节点和可选的内容测量器。
pub struct LayoutNode {
    /// 唯一标识符（用于渲染阶段的精确查找）
    pub id: u64,
    pub modifier: Modifier,
    pub measured_size: Size,
    pub position: Point,
    pub children: Vec<LayoutNode>,
    pub measure_policy: Option<Box<dyn MeasurePolicy>>,
    /// 叶子节点的内容测量器（如文本、图片）
    pub(crate) content_measurer: Option<Box<dyn ContentMeasurer>>,
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
    pub(crate) cached_paragraph: std::cell::RefCell<Option<skia_safe::textlayout::Paragraph>>,
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
    }
}

impl Drop for LayoutNode {
    fn drop(&mut self) {
        if let Some(f) = self.on_remove.take() { f(); }
    }
}

impl LayoutNode {
    pub fn new(modifier: Modifier, measure_policy: Option<Box<dyn MeasurePolicy>>) -> Self {
        let content_measurer = extract_content_measurer(&modifier);
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children: Vec::new(),
            measure_policy,
            content_measurer,
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
        }
    }

    /// 添加子节点
    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    /// 创建叶子节点（无子节点）
    pub fn leaf(modifier: Modifier) -> Self {
        LayoutNode::new(modifier, None)
    }

    /// 创建容器节点（有子节点和布局策略）
    pub fn container(
        modifier: Modifier,
        children: Vec<LayoutNode>,
        measure_policy: impl MeasurePolicy + 'static,
    ) -> Self {
        let content_measurer = extract_content_measurer(&modifier);
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children,
            measure_policy: Some(Box::new(measure_policy)),
            content_measurer,
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
        }
    }

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
            content_measurer: None,
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
        }
    }
}

// ── MeasurePolicy trait ──

/// 测量和布局策略。
///
/// 类似 Compose 的 MeasurePolicy。
/// 实现此 trait 的类型定义了一个容器的布局逻辑。
pub trait MeasurePolicy: std::fmt::Debug {
    /// 测量阶段：给定约束，返回自身尺寸和子节点的放置方案
    fn measure(
        &self,
        children: &mut [LayoutNode],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>);

    /// 布局阶段：给定已分配的尺寸，为子节点分配位置
    fn place(&self, children: &mut [LayoutNode], placements: &[Placement]);
}

// ── 命中测试 ──

/// 命中测试结果：从根到叶的节点引用链
pub fn hit_test(root: &LayoutNode, x: f32, y: f32) -> Vec<&LayoutNode> {
    let mut path = Vec::new();
    hit_test_recursive(root, x, y, 0.0, 0.0, &mut path);
    path
}

fn hit_test_recursive<'a>(
    node: &'a LayoutNode,
    x: f32,
    y: f32,
    parent_x: f32,
    parent_y: f32,
    path: &mut Vec<&'a LayoutNode>,
) -> bool {
    let nx = parent_x + node.position.x;
    let ny = parent_y + node.position.y;
    let nw = node.measured_size.width;
    let nh = node.measured_size.height;

    // 检查是否在节点范围内
    if x < nx || x > nx + nw || y < ny || y > ny + nh {
        return false;
    }

    path.push(node);

    // 计算 scroll 偏移（渲染时 canvas.translate(-offset)）
    let (scroll_dx, scroll_dy) = scroll_offset_for_node(node);

    // 子节点坐标 = 父节点坐标 + scroll 偏移
    let child_px = nx - scroll_dx;
    let child_py = ny - scroll_dy;

    // 深度优先：先检查子节点（子节点在父节点上方）
    for child in &node.children {
        if hit_test_recursive(child, x, y, child_px, child_py, path) {
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
        let mut parent = LayoutNode::leaf(Modifier::new().size(100.0, 100.0));
        parent.measured_size = Size::new(100.0, 100.0);
        parent.position = Point::new(0.0, 0.0);

        let path = hit_test(&parent, 50.0, 50.0);
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn test_hit_test_miss() {
        let mut parent = LayoutNode::leaf(Modifier::new().size(100.0, 100.0));
        parent.measured_size = Size::new(100.0, 100.0);

        let path = hit_test(&parent, 150.0, 50.0);
        assert_eq!(path.len(), 0);
    }

    #[test]
    fn test_hit_test_child() {
        let mut child = LayoutNode::leaf(Modifier::new().size(50.0, 30.0));
        child.measured_size = Size::new(50.0, 30.0);
        child.position = Point::new(10.0, 60.0);

        let mut parent = LayoutNode::leaf(Modifier::new().size(100.0, 100.0));
        parent.measured_size = Size::new(100.0, 100.0);
        parent.children.push(child);

        // 点击子节点
        let path = hit_test(&parent, 30.0, 75.0);
        assert_eq!(path.len(), 2, "should hit parent and child");
    }

    #[test]
    fn test_hit_test_child_miss() {
        let mut child = LayoutNode::leaf(Modifier::new().size(50.0, 30.0));
        child.measured_size = Size::new(50.0, 30.0);
        child.position = Point::new(10.0, 60.0);

        let mut parent = LayoutNode::leaf(Modifier::new().size(100.0, 100.0));
        parent.measured_size = Size::new(100.0, 100.0);
        parent.children.push(child);

        // 点击父节点但不在子节点范围内
        let path = hit_test(&parent, 80.0, 75.0);
        assert_eq!(path.len(), 1, "should only hit parent");
    }
}

// ── 焦点遍历 ──

/// 收集树中所有可聚焦节点的 id
pub fn collect_focusable_ids(root: &LayoutNode, list: &mut Vec<u64>) {
    if has_focusable_modifier(root) {
        list.push(root.id);
    }
    for child in &root.children {
        collect_focusable_ids(child, list);
    }
}

/// 通过 node.id 查找节点不可变引用
fn find_node_by_id(root: &LayoutNode, id: u64) -> Option<&LayoutNode> {
    if root.id == id { return Some(root); }
    for child in &root.children {
        if let Some(n) = find_node_by_id(child, id) { return Some(n); }
    }
    None
}

fn has_focusable_modifier(node: &LayoutNode) -> bool {
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::Focusable))
}

/// 移动到下一个可聚焦节点，返回是否成功
pub fn focus_next(root: &mut LayoutNode) -> bool {
    let ids: Vec<u64> = {
        let mut ids = Vec::new();
        collect_focusable_ids(root, &mut ids);
        ids
    };
    if ids.is_empty() { return false; }
    let current = ids.iter().position(|id| {
        find_node_by_id(root, *id).map(|n| n.focused).unwrap_or(false)
    });
    let next = match current {
        Some(i) if i + 1 < ids.len() => i + 1,
        _ => 0,
    };
    clear_focus(root);
    set_focus_by_id(root, ids[next]);
    true
}

fn clear_focus(node: &mut LayoutNode) {
    node.focused = false;
    for child in &mut node.children {
        clear_focus(child);
    }
}

fn set_focus_by_id(node: &mut LayoutNode, target_id: u64) -> bool {
    if node.id == target_id {
        node.focused = true;
        return true;
    }
    for child in &mut node.children {
        if set_focus_by_id(child, target_id) {
            return true;
        }
    }
    false
}

/// 点击时聚焦指定节点
pub fn focus_node(root: &mut LayoutNode, target: &LayoutNode) {
    clear_focus(root);
    set_focus_by_id(root, target.id);
}

// ── FocusRequester 全局注册表 ──

/// 通过 FocusRequester ID 设置焦点
pub fn focus_by_id(root: &mut LayoutNode, focus_requester_id: u64) -> bool {
    let target_id = find_node_id_by_focus_requester(root, focus_requester_id);
    if let Some(id) = target_id {
        clear_focus(root);
        set_focus_by_id(root, id);
        true
    } else {
        false
    }
}

fn find_node_id_by_focus_requester(node: &LayoutNode, requester_id: u64) -> Option<u64> {
    if has_focus_id(node, requester_id) {
        return Some(node.id);
    }
    for child in &node.children {
        if let Some(id) = find_node_id_by_focus_requester(child, requester_id) {
            return Some(id);
        }
    }
    None
}

fn has_focus_id(node: &LayoutNode, id: u64) -> bool {
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::FocusRequesterId { id: fid } if *fid == id))
}

/// 找到树中第一个焦点节点的 FocusRequesterId（用于持久化）
pub fn get_focus_id(root: &LayoutNode) -> Option<u64> {
    if root.focused {
        if let Some(id) = modifier_focus_id(root) {
            return Some(id);
        }
    }
    for child in &root.children {
        if let Some(id) = get_focus_id(child) {
            return Some(id);
        }
    }
    None
}

fn modifier_focus_id(node: &LayoutNode) -> Option<u64> {
    node.modifier.focus_requester_id()
}

// ── 递归测量引擎 ──

/// 递归测量节点（处理 modifier 中的约束并调用子节点的 measure_policy）
pub(crate) fn measure_node(
    node: &mut LayoutNode,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // 常量折叠：若节点未变脏且约束相同，直接复用上次结果
    if !node.dirty && node.cached_constraints == Some(constraints) {
        return (node.measured_size, Vec::new());
    }

    // 应用 modifier 中的 Layout 约束（使用查询方法）
    let mut inner_constraints = constraints;

    // 1. 应用固定尺寸
    if let Some((width, height)) = node.modifier.fixed_size() {
        use crate::modifier::Dimension;
        if let Dimension::Fixed(w) = width {
            inner_constraints = inner_constraints.tighten_width(w);
        }
        if let Dimension::Fixed(h) = height {
            inner_constraints = inner_constraints.tighten_height(h);
        }
    }

    // 2. 应用 padding
    let (pad_left, pad_right) = node.modifier.get_padding_horizontal();
    let (pad_top, pad_bottom) = node.modifier.get_padding_vertical();
    let pad_x = pad_left + pad_right;
    let pad_y = pad_top + pad_bottom;
    if pad_x > 0.0 || pad_y > 0.0 {
        inner_constraints = inner_constraints.offset(pad_x, pad_y);
    }

    // 3. 应用 FillMax 约束
    if node.modifier.is_fill_max_width() {
        inner_constraints.min_width = inner_constraints.max_width;
    }
    if node.modifier.is_fill_max_height() {
        inner_constraints.min_height = inner_constraints.max_height;
    }

    // 4. 检查 scroll 修饰符——给子节点无限约束
    if node.modifier.vertical_scroll_state().is_some() {
        inner_constraints.max_height = f32::MAX;
    }
    if node.modifier.horizontal_scroll_state().is_some() {
        inner_constraints.max_width = f32::MAX;
    }

    // 实际测量
    let result = if let Some(ref policy) = node.measure_policy {
        let (size, placements) = {
            let children = &mut node.children;
            policy.measure(children, inner_constraints)
        };
        // apply positions
        policy.place(&mut node.children, &placements);
        // apply padding offset
        if pad_left > 0.0 || pad_top > 0.0 {
            for child in &mut node.children {
                child.position.x += pad_left;
                child.position.y += pad_top;
            }
        }
        let outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        node.measured_size = outer_size;
        (outer_size, placements)
    } else {
        // 叶子节点：使用 ContentMeasurer 或默认逻辑
        let size = if let Some(ref measurer) = node.content_measurer {
            // 有内容测量器（如文本）
            let s = measurer.measure(inner_constraints);
            // 为文本节点缓存 Paragraph（避免渲染时重建）
            cache_text_paragraph(node, &inner_constraints);
            s
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

        node.measured_size = size;
        (size, Vec::new())
    };

    // 标记测量完成，缓存约束供下帧复用
    node.dirty = false;
    node.cached_constraints = Some(constraints);
    result
}

/// 使用 Skia Paragraph 测量文本的尺寸（复用全局字体缓存）
fn measure_text_size(text: &str, font_size: f32, _max_width: f32) -> (Size, skia_safe::textlayout::Paragraph) {
    use skia_safe::textlayout::{ParagraphBuilder, ParagraphStyle, TextStyle};
    let para_style = ParagraphStyle::new();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    let fc = crate::font::get_font_collection();
    let mut builder = ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(text);
    let mut para = builder.build();
    para.layout(10000.0);
    (Size::new(para.max_intrinsic_width().ceil(), para.height().ceil()), para)
}

/// 为文本节点构建并缓存 Paragraph（供渲染复用，避免重复排版）
fn cache_text_paragraph(node: &LayoutNode, _constraints: &Constraints) {
    for el in node.modifier.elements() {
        if let ModifierElement::TextContent { content, font_size, .. } = el {
            let (_size, para) = measure_text_size(content, *font_size, f32::MAX);
            *node.cached_paragraph.borrow_mut() = Some(para);
            return;
        }
    }
}

// ── 主轴间距计算（Column/Row 共用）──

/// 计算主轴上的 spacing 和 leading space
pub(crate) fn compute_spacing(
    arrangement: Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        Arrangement::Start => (0.0, 0.0),
        Arrangement::End => (0.0, remaining),
        Arrangement::Center => (0.0, remaining / 2.0),
        Arrangement::SpaceBetween => {
            if gap_count > 0 {
                (remaining / gap_count as f32, 0.0)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        Arrangement::SpaceAround => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        Arrangement::SpaceEvenly => {
            let total_gaps = gap_count + 2; // 前后也有间距
            if total_gaps > 0 {
                let space = remaining / total_gaps as f32;
                (space, space)
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}
