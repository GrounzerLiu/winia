//! 布局节点 — LayoutNode 及相关的尺寸/位置/排列/对齐类型

use crate::modifier::{Modifier, ModifierElement};
use super::constraints::Constraints;

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

fn set_focus_index(root: &mut LayoutNode, list: &[&LayoutNode], index: usize) {
    if let Some(target) = list.get(index) {
        let target_ptr = *target as *const LayoutNode;
        set_focus_by_ptr(root, target_ptr);
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
