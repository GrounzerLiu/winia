//! SelectionContainer — 文本选中容器（对齐 Jetpack Compose）

use crate::core::composition_local::CompositionLocal;
use crate::core::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::core::composer::GroupStatus;
use std::ops::Range;
use std::sync::Arc;
use std::sync::Mutex;


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
    pub slot_key: u64,
    /// 全局起始偏移（在 SelectionContainer 中的累计字符位置）
    pub global_offset: usize,
    pub text_len: usize,
    pub bounds: Rect,
}

// ═══════════════════════════════════════════════════════════
// SelectionRegistrar
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
struct RegistrarInner {
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    /// 下一个注册段的全局起始偏移（跨 Text 累积）
    next_global_offset: usize,
    segments: Vec<RegisteredSegment>,
}

/// 选区注册表，通过 `Arc<Mutex<>>` 实现线程安全内部可变性，
/// CompositionLocal 中 clone 后仍共享同一状态。
#[derive(Debug, Clone)]
pub struct SelectionRegistrar {
    inner: Arc<Mutex<RegistrarInner>>,
}

impl SelectionRegistrar {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RegistrarInner {
                selection_start: None,
                selection_end: None,
                next_global_offset: 0,
                segments: Vec::new(),
            })),
        }
    }

    /// 注册一个可选中文本段。返回全局偏移（供后续 Text 计算）。
    pub fn register(&self, slot_key: u64, text_len: usize, bounds: Option<Rect>) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let offset = inner.next_global_offset;
        inner.segments.push(RegisteredSegment {
            slot_key,
            global_offset: offset,
            text_len,
            bounds: bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
        });
        inner.next_global_offset += text_len;
        offset
    }

    /// 设置选区（全局索引）
    pub fn set_selection(&self, start: usize, end: usize) {
        let mut inner = self.inner.lock().unwrap();
        inner.selection_start = Some(start.min(end));
        inner.selection_end = Some(start.max(end));
    }

    /// 获取注册段信息（全局偏移 + 长度）
    pub fn segment_info(&self, slot_key: u64) -> Option<(usize, usize)> {
        let inner = self.inner.lock().unwrap();
        inner.segments.iter()
            .find(|s| s.slot_key == slot_key)
            .map(|s| (s.global_offset, s.text_len))
    }
    pub fn clear_selection(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.selection_start = None;
        inner.selection_end = None;
    }

    /// 获取指定 slot_key 对应段内的选中区域（本地索引）。
    /// 跨 Text 合并选择：全局 selection → local = global - segment.global_offset
    pub fn selected_range(&self, slot_key: u64) -> Option<Range<usize>> {
        let inner = self.inner.lock().unwrap();
        let (global_start, global_end) = (inner.selection_start?, inner.selection_end?);
        let seg = inner.segments.iter().find(|s| s.slot_key == slot_key)?;
        let local_start = global_start.saturating_sub(seg.global_offset);
        let local_end = global_end.saturating_sub(seg.global_offset);
        if local_start >= seg.text_len || local_end == 0 { return None; }
        let len = seg.text_len;
        Some(local_start.min(len)..local_end.min(len))
    }
}

impl Default for SelectionRegistrar {
    fn default() -> Self { Self::new() }
}

impl RegisteredSegment {
    fn text_len(&self) -> usize {
        self.text_len
    }
}

// ═══════════════════════════════════════════════════════════
// LOCAL_SELECTION_REGISTRAR
// ═══════════════════════════════════════════════════════════


pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));

/// 活跃的 SelectionRegistrar（渲染和事件处理时取用，避免 CompositionLocal 上下文丢失）
static ACTIVE_REGISTRAR: std::sync::LazyLock<Mutex<Option<SelectionRegistrar>>> = std::sync::LazyLock::new(|| Mutex::new(None));

pub(crate) fn active_registrar() -> SelectionRegistrar {
    ACTIVE_REGISTRAR.lock().unwrap().clone().unwrap_or_else(|| SelectionRegistrar::new())
}

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
                ctx.set_selection_registrar(registrar.clone());
                *ACTIVE_REGISTRAR.lock().unwrap() = Some(registrar.clone());
                LOCAL_SELECTION_REGISTRAR.provides(registrar, || {
                    content(ctx);
                });
            }
        }
        ctx.end_restartable_group();
        // 恢复（清除注入的 registrar，避免子树外 Text 误注册）
        ctx.clear_selection_registrar();
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
        reg.register(42, 20, rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(5, 15);
        assert_eq!(reg.selected_range(42), Some(5..15));
    }

    #[test]
    fn test_cross_text_merged_selection() {
        let reg = SelectionRegistrar::new();
        // Text A: slot=1, len=10 → global offset=0
        reg.register(1, 10, rect(0.0, 0.0, 100.0, 20.0));
        // Text B: slot=2, len=15 → global offset=10
        reg.register(2, 15, rect(0.0, 0.0, 150.0, 20.0));
        // Select global 5..20 (spans both texts)
        reg.set_selection(5, 20);
        // Text A local: 5..10 (from global 5..10)
        assert_eq!(reg.selected_range(1), Some(5..10));
        // Text B local: 0..10 (from global 10..20)
        assert_eq!(reg.selected_range(2), Some(0..10));
        // Text outside: none
        assert!(reg.selected_range(99).is_none());
    }

    #[test]
    fn test_selection_outside_any_segment() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg.set_selection(20, 30);
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clear_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 20, rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(5, 15);
        reg.clear_selection();
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_rc_sharing_via_clone() {
        let reg1 = SelectionRegistrar::new();
        let reg2 = reg1.clone();
        reg1.register(42, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg1.set_selection(2, 8);
        // reg2 读到的是同一个内部状态
        assert_eq!(reg2.selected_range(42), Some(2..8));
    }
}
