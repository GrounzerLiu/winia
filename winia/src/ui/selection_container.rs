//! SelectionContainer — 类似 Jetpack Compose 的文本选择容器
//!
//! 通过 `CompositionLocal` 向下层文本组件提供 `SelectionRegistrar`，
//! 用于注册可选中文本段、管理选区状态。

use std::ops::Range;

use crate::core::composer::ComposeCtx;
use crate::core::composition_local::CompositionLocal;
use crate::modifier::Modifier;

// ── Rect ──

/// 简单的 2D 矩形（像素坐标）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

// ── RegisteredSegment ──

/// 已注册的可选中文本段。
///
/// 文本组件在 layout 阶段调用 `register()` 将其文本范围注册到
/// `SelectionRegistrar` 中，供选区命中测试和渲染使用。
#[derive(Debug, Clone)]
pub struct RegisteredSegment {
    /// 所属 LayoutNode 的 ID
    pub node_id: u64,
    /// 该段文本在整个容器内容中的起始字符偏移
    pub text_start: usize,
    /// 该段文本在整个容器内容中的结束字符偏移（不含）
    pub text_end: usize,
    /// 该段在屏幕上的边界矩形（用于命中测试）
    pub bounds: Rect,
}

// ── SelectionRegistrar ──

/// 选区注册表，管理一个 `SelectionContainer` 内的所有可选文本段和当前选区。
///
/// # 生命周期
/// 每个 `SelectionContainer` 在 `build` 时创建一个实例并通过
/// `CompositionLocal` 提供给子组件。
#[derive(Debug, Clone)]
pub struct SelectionRegistrar {
    /// 选区起始字符偏移（在整个容器文本内容中）
    pub selection_start: Option<usize>,
    /// 选区结束字符偏移（不含）
    pub selection_end: Option<usize>,
    /// 当前选中文本所在的 LayoutNode ID
    pub selection_node_id: Option<u64>,
    /// 已注册的文本段列表
    pub(crate) segments: Vec<RegisteredSegment>,
}

impl SelectionRegistrar {
    /// 创建一个空的选区注册表
    pub fn new() -> Self {
        Self {
            selection_start: None,
            selection_end: None,
            selection_node_id: None,
            segments: Vec::new(),
        }
    }

    /// 注册一个可选中文本段。
    ///
    /// # 参数
    /// - `node_id`: 文本所属 LayoutNode 的 ID
    /// - `start`: 该段在整个容器内容中的起始字符偏移
    /// - `end`: 该段在整个容器内容中的结束字符偏移（不含）
    /// - `bounds`: 该段在屏幕上的边界矩形
    pub fn register(&mut self, node_id: u64, start: usize, end: usize, bounds: Rect) {
        self.segments.push(RegisteredSegment {
            node_id,
            text_start: start,
            text_end: end,
            bounds,
        });
    }

    /// 设置选区。
    ///
    /// # 参数
    /// - `node_id`: 触发选区的 LayoutNode ID
    /// - `start`: 选区起始字符偏移
    /// - `end`: 选区结束字符偏移
    pub fn set_selection(&mut self, node_id: u64, start: usize, end: usize) {
        self.selection_node_id = Some(node_id);
        self.selection_start = Some(start.min(end));
        self.selection_end = Some(start.max(end));
    }

    /// 清除当前选区
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.selection_node_id = None;
    }

    /// 查询指定 `node_id` 对应的选区字符范围。
    ///
    /// 返回该节点文本中与全局选区重叠部分的起始/结束偏移。
    /// 如果该节点没有选区或选区不在此节点，返回 `None`。
    pub fn selected_range(&self, node_id: u64) -> Option<Range<usize>> {
        let (sel_start, sel_end) = (self.selection_start?, self.selection_end?);
        for seg in &self.segments {
            if seg.node_id == node_id {
                // 计算全局选区与当前段的重叠
                let overlap_start = sel_start.max(seg.text_start);
                let overlap_end = sel_end.min(seg.text_end);
                if overlap_start < overlap_end {
                    // 转换为该段本地的字符偏移
                    let local_start = overlap_start - seg.text_start;
                    let local_end = overlap_end - seg.text_start;
                    return Some(local_start..local_end);
                }
            }
        }
        None
    }
}

impl Default for SelectionRegistrar {
    fn default() -> Self {
        Self::new()
    }
}

// ── LOCAL_SELECTION_REGISTRAR ──

/// 用于向下层传递 `SelectionRegistrar` 的 CompositionLocal。
///
/// 文本组件通过此 local 获取当前选区信息，绘制高亮。
/// 默认值为一个空的选区注册表。
pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));

// ── SelectionContainer ──

/// SelectionContainer 组件 — 提供文本选择能力的作用域。
///
/// 在 `build` 中创建一个 `SelectionRegistrar` 并通过 `CompositionLocal`
/// 提供给子组件，使子文本段能注册自身并响应选区操作。
///
/// # 示例
/// ```ignore
/// SelectionContainer::new()
///     .modifier(Modifier::new().size(300.0, 200.0))
///     .build(ctx, |ctx| {
///         Text::new("可选中的文本").build(ctx);
///     });
/// ```
#[derive(Clone)]
pub struct SelectionContainer {
    modifier: Modifier,
}

impl SelectionContainer {
    /// 创建新的 SelectionContainer
    pub fn new() -> Self {
        Self {
            modifier: Modifier::new(),
        }
    }

    /// 设置修饰符链
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 注册到组合树并执行子内容。
    ///
    /// 在组合阶段创建一个 `SelectionRegistrar` 实例，通过
    /// `LOCAL_SELECTION_REGISTRAR.provides(...)` 注入到子组合树中。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let registrar = SelectionRegistrar::new();
        match ctx.start_restartable_group(key, self.modifier, crate::layout::BoxLayout::new()) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                LOCAL_SELECTION_REGISTRAR.provides(registrar, || {
                    content(ctx);
                });
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for SelectionContainer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Getters（测试用）──
impl SelectionContainer {
    /// 获取当前 modifier（测试用）
    pub fn get_modifier(&self) -> &Modifier {
        &self.modifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_registrar_new() {
        let reg = SelectionRegistrar::new();
        assert!(reg.selection_start.is_none());
        assert!(reg.selection_end.is_none());
        assert!(reg.selection_node_id.is_none());
        assert!(reg.selected_range(0).is_none());
    }

    #[test]
    fn test_register_segment() {
        let mut reg = SelectionRegistrar::new();
        reg.register(42, 0, 10, Rect::new(0.0, 0.0, 100.0, 20.0));
        reg.register(42, 10, 20, Rect::new(0.0, 20.0, 100.0, 20.0));
        // 内部 segments 应该有 2 个
        assert_eq!(reg.segments.len(), 2);
    }

    #[test]
    fn test_set_selection_and_selected_range() {
        let mut reg = SelectionRegistrar::new();
        reg.register(42, 0, 20, Rect::new(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(42, 5, 15);
        assert_eq!(reg.selection_start, Some(5));
        assert_eq!(reg.selection_end, Some(15));
        let range = reg.selected_range(42);
        assert_eq!(range, Some(5..15));
    }

    #[test]
    fn test_selected_range_no_overlap() {
        let mut reg = SelectionRegistrar::new();
        reg.register(42, 0, 10, Rect::new(0.0, 0.0, 100.0, 20.0));
        reg.set_selection(42, 20, 30);
        // 选区完全不在注册段内
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_selected_range_partial_overlap() {
        let mut reg = SelectionRegistrar::new();
        reg.register(42, 10, 30, Rect::new(0.0, 0.0, 100.0, 40.0));
        reg.set_selection(42, 5, 20);
        // 覆盖段 [10..30] ∩ [5..20] = [10..20)，本地偏移为 [0..10)
        assert_eq!(reg.selected_range(42), Some(0..10));
    }

    #[test]
    fn test_clear_selection() {
        let mut reg = SelectionRegistrar::new();
        reg.set_selection(1, 0, 10);
        reg.clear_selection();
        assert!(reg.selection_start.is_none());
        assert!(reg.selection_end.is_none());
        assert!(reg.selection_node_id.is_none());
    }

    #[test]
    fn test_selected_range_wrong_node() {
        let mut reg = SelectionRegistrar::new();
        reg.register(1, 0, 10, Rect::new(0.0, 0.0, 100.0, 20.0));
        reg.set_selection(1, 2, 8);
        // 查询不存在的 node_id
        assert!(reg.selected_range(99).is_none());
    }

    #[test]
    fn test_selection_container_default() {
        let container = SelectionContainer::new();
        // 默认 modifier 应该为空
        assert_eq!(container.get_modifier().elements().len(), 0);
    }

    #[test]
    fn test_selection_container_modifier() {
        let container = SelectionContainer::new()
            .modifier(Modifier::new().size(300.0, 200.0));
        assert_eq!(container.get_modifier().elements().len(), 1);
    }

    #[test]
    fn test_local_selection_registrar_default() {
        // 在不 provides 的情况下读取，应返回默认的空实例
        let reg = LOCAL_SELECTION_REGISTRAR.current();
        assert!(reg.selection_start.is_none());
    }

    #[test]
    fn test_local_selection_registrar_provide() {
        let result = LOCAL_SELECTION_REGISTRAR.provides({
            let mut reg = SelectionRegistrar::new();
            reg.set_selection(1, 0, 10);
            reg
        }, || {
            let current = LOCAL_SELECTION_REGISTRAR.current();
            current.selection_start.unwrap()
        });
        assert_eq!(result, 0);
    }
}
