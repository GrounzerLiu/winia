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
fn modifier_has_text(modifier: &Modifier) -> bool {
    modifier.elements().iter().any(|el| matches!(el, ModifierElement::TextContent { .. }))
}

/// 检查 modifier 中是否包含 RichTextContent
fn modifier_has_richtext(modifier: &Modifier) -> bool {
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
    pub children: Vec<LayoutNode>,
    pub measure_policy: Option<Box<dyn MeasurePolicy>>,
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
    pub(crate) cached_paragraph: std::cell::RefCell<Option<skia_safe::textlayout::Paragraph>>,
    /// 富文本内联元素（图片/SVG），测量阶段缓存供渲染使用
    pub(crate) inline_drawables: std::cell::RefCell<Vec<std::sync::Arc<dyn crate::text::InlineDrawable>>>,
    /// scroll 容器的 viewport 高度（由 measure_node 在布局阶段设值，供 apply_scroll_delta 使用）
    pub(crate) scroll_viewport_height: f32,
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
        self.has_text_content = modifier_has_text(&self.modifier);
        self.has_richtext_content = modifier_has_richtext(&self.modifier);
    }
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
            inline_drawables: std::cell::RefCell::new(Vec::new()),
            scroll_viewport_height: 0.0,
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
            has_text_content: modifier_has_text(&modifier),
            has_richtext_content: modifier_has_richtext(&modifier),
            modifier,
            measured_size: Size::ZERO,
            position: Point::ZERO,
            children,
            measure_policy: Some(Box::new(measure_policy)),
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            inline_drawables: std::cell::RefCell::new(Vec::new()),
            scroll_viewport_height: 0.0,
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
            has_text_content: false,
            has_richtext_content: false,
            focused: false,
            on_remove: None,
            dirty: true,
            cached_constraints: None,
            slot_key: 0,
            cached_paragraph: std::cell::RefCell::new(None),
            inline_drawables: std::cell::RefCell::new(Vec::new()),
            scroll_viewport_height: 0.0,
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

    // 3. 应用 FillMax 约束（在 scroll 修改 max 之前，保存 viewport 约束）
    let viewport_height = inner_constraints.max_height;
    if node.modifier.is_fill_max_width() {
        inner_constraints.min_width = inner_constraints.max_width;
    }
    if node.modifier.is_fill_max_height() {
        if inner_constraints.max_height < f32::MAX {
            inner_constraints.min_height = inner_constraints.max_height;
        }
    }

    // 4. 检查 scroll 修饰符——给子节点无限约束
    if node.modifier.vertical_scroll_state().is_some() {
        // scroll 容器自身填 viewport（fill_max_height 在无限 max 时跳过，这里补上）
        if node.modifier.is_fill_max_height() && inner_constraints.max_height >= f32::MAX {
            inner_constraints.min_height = viewport_height;
        }
        // 保存 viewport 高度供滚动 clamping 使用
        node.scroll_viewport_height = viewport_height;
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
        // apply per-child offset modifier
        for child in &mut node.children {
            if let Some((ox, oy)) = child.modifier.get_offset() {
                child.position.x += ox;
                child.position.y += oy;
            }
        }
        let outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        node.measured_size = outer_size;
        (outer_size, placements)
    } else {
        // 叶子节点：使用 ContentMeasurer 或默认逻辑
        let size = if node.has_text_content {
            // 合并的 measure + cache（避免重复创建 Paragraph）
            // 使用父约束的 max_width 作为排版宽度，确保文本在可用空间内自动换行。
            // 对于可滚动容器，inner_constraints.max_width 已被设为 f32::MAX。
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_text(node, layout_width);
            // 用约束 clamping 最终尺寸（fill_max_width 时约束收紧，文本应填满可用宽度）
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height),
            )
        } else if node.has_richtext_content {
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_richtext(node, layout_width);
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

        node.measured_size = size;
        (size, Vec::new())
    };

    // 标记测量完成，缓存约束供下帧复用
    node.dirty = false;
    node.cached_constraints = Some(constraints);
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
            let mut builder = skia_safe::textlayout::ParagraphBuilder::new(&para_style, &fc);
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
    use skia_safe::textlayout::{ParagraphStyle, PlaceholderStyle, PlaceholderAlignment, TextBaseline, TextStyle as SkTextStyle};
    use skia_safe::FontStyle as SkFontStyle;
    use crate::ui::text::FontSlant;
    let fc = crate::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::RichTextContent { content, drawables, drawable_ranges, spans } = el {
            let para_style = ParagraphStyle::new();
            let mut builder = skia_safe::textlayout::ParagraphBuilder::new(&para_style, &fc);

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
                        let (w, h) = drawables[di].size();
                        let ph = PlaceholderStyle::new(w, h, PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0);
                        builder.add_placeholder(&ph);
                        if let Some(s) = spans.iter().find(|s| s.start <= ci && s.end > ci) { builder.pop(); }
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
            *node.inline_drawables.borrow_mut() = drawables.clone();
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
                (space, space / 2.0) // (元素间间距, 首/尾边缘间距 = 一半)
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
