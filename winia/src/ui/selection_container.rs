//! SelectionContainer — 文本选中容器（对齐 Jetpack Compose）

use crate::core::composition_local::CompositionLocal;
use crate::core::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::core::composer::GroupStatus;
use std::ops::Range;
use std::rc::Rc;
use std::cell::RefCell;

// ═══════════════════════════════════════════════════════════
// 辅助类型
// ═══════════════════════════════════════════════════════════

/// 简单的 2D 矩形（对齐 Compose Rect）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }
}

/// 注册的文本段信息
#[derive(Debug, Clone)]
pub(crate) struct RegisteredSegment {
    pub node_id: u64,
    pub text_start: usize,
    pub text_end: usize,
    pub bounds: Rect,
}

// ═══════════════════════════════════════════════════════════
// SelectionRegistrar
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct RegistrarInner {
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    selection_node_id: Option<u64>,
    segments: Vec<RegisteredSegment>,
}

/// 选区注册表，通过 `Rc<RefCell<>>` 实现内部可变性，
/// CompositionLocal 中 clone 后仍共享同一状态。
#[derive(Debug, Clone)]
pub struct SelectionRegistrar {
    inner: Rc<RefCell<RegistrarInner>>,
}

impl SelectionRegistrar {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(RegistrarInner {
                selection_start: None,
                selection_end: None,
                selection_node_id: None,
                segments: Vec::new(),
            })),
        }
    }

    /// 注册一个可选中文本段。
    pub fn register(&self, node_id: u64, start: usize, end: usize, bounds: Option<Rect>) {
        let mut inner = self.inner.borrow_mut();
        inner.segments.push(RegisteredSegment {
            node_id,
            text_start: start,
            text_end: end,
            bounds: bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
        });
    }

    /// 设置选区。
    pub fn set_selection(&self, node_id: u64, start: usize, end: usize) {
        let mut inner = self.inner.borrow_mut();
        inner.selection_node_id = Some(node_id);
        inner.selection_start = Some(start.min(end));
        inner.selection_end = Some(start.max(end));
    }

    /// 清除选区。
    pub fn clear_selection(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.selection_start = None;
        inner.selection_end = None;
        inner.selection_node_id = None;
    }

    /// 获取指定 node 在本地位移范围内的选中区域。
    pub fn selected_range(&self, node_id: u64) -> Option<Range<usize>> {
        let inner = self.inner.borrow();
        let (global_start, global_end) = (inner.selection_start?, inner.selection_end?);
        let seg = inner.segments.iter().find(|s| s.node_id == node_id)?;
        let local_start = global_start.saturating_sub(seg.text_start);
        let local_end = global_end.saturating_sub(seg.text_start);
        if local_start >= seg.text_len() || local_end == 0 { return None; }
        let len = seg.text_len();
        Some(local_start.min(len)..local_end.min(len))
    }
}

impl Default for SelectionRegistrar {
    fn default() -> Self { Self::new() }
}

impl RegisteredSegment {
    fn text_len(&self) -> usize {
        self.text_end - self.text_start
    }
}

// ═══════════════════════════════════════════════════════════
// LOCAL_SELECTION_REGISTRAR
// ═══════════════════════════════════════════════════════════


pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));

// ═══════════════════════════════════════════════════════════
// SelectionContainer
// ═══════════════════════════════════════════════════════════

/// 文本选中容器（对齐 Compose SelectionContainer）。
///
/// 通过 `LOCAL_SELECTION_REGISTRAR` CompositionLocal 向子树注入一个
/// `SelectionRegistrar`，子树中的 `Text` 组件通过它注册文本段并读取选区状态。
pub struct SelectionContainer {
    modifier: Modifier,
}

impl SelectionContainer {
    pub fn new() -> Self {
        SelectionContainer { modifier: Modifier::new() }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 构建选区容器，注入新的 `SelectionRegistrar`。
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let registrar = SelectionRegistrar::new();
        match ctx.start_restartable_group(key, self.modifier, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                LOCAL_SELECTION_REGISTRAR.provides(registrar, || {
                    content(ctx);
                });
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for SelectionContainer {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: f32, t: f32, r: f32, b: f32) -> Option<Rect> {
        Some(Rect::new(l, t, r, b))
    }

    #[test]
    fn test_new_registrar_no_selection() {
        let reg = SelectionRegistrar::new();
        assert!(reg.selected_range(1).is_none());
    }

    #[test]
    fn test_register_and_set_selection_single_node() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 0, 20, rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(42, 5, 15);
        assert_eq!(reg.selected_range(42), Some(5..15));
    }

    #[test]
    fn test_selection_outside_any_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 0, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg.set_selection(42, 20, 30);
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clear_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 0, 20, rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(42, 5, 15);
        reg.clear_selection();
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_rc_sharing_via_clone() {
        let reg1 = SelectionRegistrar::new();
        let reg2 = reg1.clone();
        reg1.register(42, 0, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg1.set_selection(42, 2, 8);
        // reg2 读到的是同一个内部状态
        assert_eq!(reg2.selected_range(42), Some(2..8));
    }
}
