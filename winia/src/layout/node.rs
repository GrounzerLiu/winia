//! 布局节点 — LayoutNode 及相关的尺寸/位置/排列/对齐类型

use crate::modifier::{Modifier, ModifierElement};
use super::constraints::Constraints;
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局节点 ID 生成器
static NEXT_NODE_ID: AtomicU64 = AtomicU64::new(1);

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

// ── LayoutNode ──

/// 布局树中的一个节点。
///
/// 每个 LayoutNode 对应 UI 树中的一个可测量/可布局的单元。
/// 包含 modifier 链和子节点。
pub struct LayoutNode {
    /// 唯一标识符（用于渲染阶段的精确查找）
    pub id: u64,
    pub modifier: Modifier,
    pub measured_size: Size,
    pub position: Point,
    pub children: Vec<LayoutNode>,
    pub measure_policy: Option<Box<dyn MeasurePolicy>>,
    /// 是否获得焦点
    pub focused: bool,
    /// 节点从布局树移除时调用（用于 Window 生命周期管理）
    pub(crate) on_remove: Option<Box<dyn FnOnce() + Send>>,
}

impl Drop for LayoutNode {
    fn drop(&mut self) {
        if let Some(f) = self.on_remove.take() { f(); }
    }
}

impl LayoutNode {
    pub fn new(modifier: Modifier, measure_policy: Option<Box<dyn MeasurePolicy>>) -> Self {
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children: Vec::new(),
            measure_policy,
            focused: false,
            on_remove: None,
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
        LayoutNode {
            id: NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children,
            measure_policy: Some(Box::new(measure_policy)),
            focused: false,
            on_remove: None,
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
            focused: false,
            on_remove: None,
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
    for el in node.modifier.elements() {
        match el {
            ModifierElement::VerticalScroll { state } => dy += state.get(),
            ModifierElement::HorizontalScroll { state } => dx += state.get(),
            _ => {}
        }
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

/// 收集树中所有可聚焦节点（深度优先，对应 Tab 键顺序）
pub fn collect_focusable<'a>(root: &'a LayoutNode, list: &mut Vec<&'a LayoutNode>) {
    if has_focusable_modifier(root) {
        list.push(root);
    }
    for child in &root.children {
        collect_focusable(child, list);
    }
}

fn has_focusable_modifier(node: &LayoutNode) -> bool {
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::Focusable))
}

/// 移动到下一个可聚焦节点，返回是否成功
pub fn focus_next(root: &mut LayoutNode) -> bool {
    // 先收集可聚焦节点（不可变借用）
    let list = {
        let mut list = Vec::new();
        collect_focusable(root, &mut list);
        list.into_iter().map(|n| n as *const LayoutNode).collect::<Vec<_>>()
    };
    if list.is_empty() {
        return false;
    }
    let current = list.iter().position(|p| unsafe { (**p).focused });
    let next = match current {
        Some(i) if i + 1 < list.len() => i + 1,
        _ => 0,
    };
    // 修改（可变借用）
    clear_focus(root);
    set_focus_by_ptr(root, list[next]);
    true
}

fn clear_focus(node: &mut LayoutNode) {
    node.focused = false;
    for child in &mut node.children {
        clear_focus(child);
    }
}

fn set_focus_by_ptr(node: &mut LayoutNode, target: *const LayoutNode) -> bool {
    if std::ptr::eq(node as *const _, target) {
        node.focused = true;
        return true;
    }
    for child in &mut node.children {
        if set_focus_by_ptr(child, target) {
            return true;
        }
    }
    false
}

/// 点击时聚焦指定节点
pub fn focus_node(root: &mut LayoutNode, target: &LayoutNode) {
    let target_ptr = target as *const LayoutNode;
    clear_focus(root);
    set_focus_by_ptr(root, target_ptr);
}

// ── FocusRequester 全局注册表 ──

/// 通过 FocusRequester ID 设置焦点（遍历树查找匹配的 FocusRequesterId modifier）
pub fn focus_by_id(root: &mut LayoutNode, id: u64) -> bool {
    let ptr = find_by_focus_id_immut(root, id);
    if let Some(ptr) = ptr {
        clear_focus(root);
        set_focus_by_ptr(root, ptr);
        true
    } else {
        false
    }
}

fn find_by_focus_id_immut(node: &LayoutNode, id: u64) -> Option<*const LayoutNode> {
    if has_focus_id(node, id) {
        return Some(node as *const _);
    }
    for child in &node.children {
        if let Some(p) = find_by_focus_id_immut(child, id) {
            return Some(p);
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
    node.modifier.elements().iter().find_map(|el| match el {
        crate::modifier::ModifierElement::FocusRequesterId { id } => Some(*id),
        _ => None,
    })
}

// ── 递归测量引擎 ──

/// 递归测量节点（处理 modifier 中的约束并调用子节点的 measure_policy）
pub(crate) fn measure_node(
    node: &mut LayoutNode,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // 应用 modifier 中的 Layout 约束
    let mut inner_constraints = constraints;
    let mut pad_x = 0.0;
    let mut pad_y = 0.0;

    for el in node.modifier.elements() {
        match el {
            ModifierElement::Size { width, height } => {
                use crate::modifier::Dimension;
                if let Dimension::Fixed(w) = width {
                    inner_constraints = inner_constraints.tighten_width(*w);
                }
                if let Dimension::Fixed(h) = height {
                    inner_constraints = inner_constraints.tighten_height(*h);
                }
            }
            ModifierElement::Padding { all } => {
                let p = *all;
                pad_x += p; pad_y += p;
                inner_constraints = inner_constraints.offset(p * 2.0, p * 2.0);
            }
            ModifierElement::PaddingHorizontal { value } => {
                pad_x += value;
                inner_constraints = inner_constraints.offset(value * 2.0, 0.0);
            }
            ModifierElement::PaddingVertical { value } => {
                pad_y += value;
                inner_constraints = inner_constraints.offset(0.0, value * 2.0);
            }
            ModifierElement::FillMaxWidth => {
                inner_constraints.min_width = inner_constraints.max_width;
            }
            ModifierElement::FillMaxHeight => {
                inner_constraints.min_height = inner_constraints.max_height;
            }
            ModifierElement::FillMaxSize => {
                inner_constraints.min_width = inner_constraints.max_width;
                inner_constraints.min_height = inner_constraints.max_height;
            }
            _ => {}
        }
    }

    // 检查是否包含 scroll 修饰符——给子节点无限约束
    let node_is_scroll_v = node.modifier.elements().iter().any(|el| matches!(el, ModifierElement::VerticalScroll { .. }));
    let node_is_scroll_h = node.modifier.elements().iter().any(|el| matches!(el, ModifierElement::HorizontalScroll { .. }));
    if node_is_scroll_v {
        inner_constraints.max_height = f32::MAX;
    }
    if node_is_scroll_h {
        inner_constraints.max_width = f32::MAX;
    }

    // 实际测量
    if let Some(ref policy) = node.measure_policy {
        let (size, placements) = {
            let children = &mut node.children;
            policy.measure(children, inner_constraints)
        };
        // apply positions
        policy.place(&mut node.children, &placements);
        // apply padding offset
        if pad_x != 0.0 || pad_y != 0.0 {
            for child in &mut node.children {
                child.position.x += pad_x;
                child.position.y += pad_y;
            }
        }
        node.measured_size = size;
        (size, placements)
    } else {
        // 叶子节点
        // 检查是否有 TextContent（文字节点需要根据字体测量尺寸）
        let mut text_content: Option<(&str, f32)> = None;
        for el in node.modifier.elements() {
            if let ModifierElement::TextContent { content, font_size, .. } = el {
                text_content = Some((content.as_str(), *font_size));
                break;
            }
        }

        let (width, height) = if let Some((content, font_size)) = text_content {
            // 用 Skia Paragraph 测量文字尺寸
            let max_w = if inner_constraints.has_fixed_width() {
                inner_constraints.max_width
            } else {
                f32::MAX
            };
            measure_text_size(content, font_size, max_w)
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
            (w, h)
        };

        node.measured_size = Size::new(width, height);
        (node.measured_size, Vec::new())
    }
}

/// 使用 Skia Paragraph 测量文本的尺寸
fn measure_text_size(text: &str, font_size: f32, _max_width: f32) -> (f32, f32) {
    use skia_safe::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle};
    let para_style = ParagraphStyle::new();
    let mut text_style = TextStyle::new();
    text_style.set_font_size(font_size);
    let mut fc = FontCollection::new();
    fc.set_default_font_manager(skia_safe::FontMgr::default(), None);
    let mut builder = ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(text);
    let mut para = builder.build();
    // 先 layout 到很大宽度（避免换行），再用 intrinsic width 确定实际宽度
    para.layout(10000.0);
    (para.max_intrinsic_width().ceil(), para.height().ceil())
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
