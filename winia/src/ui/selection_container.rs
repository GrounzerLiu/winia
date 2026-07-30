//! SelectionContainer — 文本选中容器（对齐 Jetpack Compose）

use crate::core::composition_local::CompositionLocal;
use crate::core::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::layout::BoxLayout;
use crate::core::composer::GroupStatus;
use std::collections::HashMap;
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
    pub global_offset: usize,
    pub text_len: usize,
    pub bounds: Rect,
}

// ═══════════════════════════════════════════════════════════
// SelectionRegistrar
// ═══════════════════════════════════════════════════════════

type OnChangeFn = Arc<dyn Fn(usize, usize) + Send + Sync>;

#[derive(Clone)]
struct RegistrarInner {
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    next_global_offset: usize,
    segments: HashMap<u64, RegisteredSegment>,
    on_change: Option<OnChangeFn>,
}

impl std::fmt::Debug for RegistrarInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrarInner").field("selection_start", &self.selection_start).field("selection_end", &self.selection_end).field("has_cb", &self.on_change.is_some()).finish()
    }
}

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
                segments: HashMap::new(),
                on_change: None,
            })),
        }
    }

    pub fn register(&self, slot_key: u64, text_len: usize, bounds: Option<Rect>) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let offset = inner.next_global_offset;
        // 用 HashMap 自动去重——每个 slot_key 只保留最新注册
        inner.segments.insert(slot_key, RegisteredSegment { slot_key, global_offset: offset, text_len, bounds: bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)) });
        inner.next_global_offset += text_len;
        offset
    }

    pub fn set_selection(&self, start: usize, end: usize) {
        let mut inner = self.inner.lock().unwrap();
        inner.selection_start = Some(start.min(end));
        inner.selection_end = Some(start.max(end));
    }

    pub fn clear_selection(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.selection_start = None;
        inner.selection_end = None;
    }

    pub fn segment_info(&self, slot_key: u64) -> Option<(usize, usize)> {
        let inner = self.inner.lock().unwrap();
        inner.segments.get(&slot_key).map(|s| (s.global_offset, s.text_len))
    }

    pub(crate) fn reset_offsets(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.next_global_offset = 0;
        inner.segments.clear();
    }

    pub fn total_text_len(&self) -> usize {
        self.inner.lock().unwrap().next_global_offset
    }

    pub fn selected_range(&self, slot_key: u64) -> Option<Range<usize>> {
        let inner = self.inner.lock().unwrap();
        let (global_start, global_end) = (inner.selection_start?, inner.selection_end?);
        let seg = inner.segments.get(&slot_key)?;
        let local_start = global_start.saturating_sub(seg.global_offset);
        let local_end = global_end.saturating_sub(seg.global_offset);
        if local_start >= seg.text_len || local_end == 0 { return None; }
        let len = seg.text_len;
        Some(local_start.min(len)..local_end.min(len))
    }

    /// 设置选区变化回调
    pub fn set_on_change(&self, f: impl Fn(usize, usize) + Send + Sync + 'static) {
        self.inner.lock().unwrap().on_change = Some(Arc::new(f));
    }

    pub(crate) fn fire_on_change(&self) {
        let inner = self.inner.lock().unwrap();
        if let (Some(s), Some(e)) = (inner.selection_start, inner.selection_end) {
            if let Some(ref cb) = inner.on_change {
                cb(s, e);
            }
        }
    }
}

impl Default for SelectionRegistrar {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// LOCAL_SELECTION_REGISTRAR + ACTIVE_REGISTRAR
// ═══════════════════════════════════════════════════════════

pub static LOCAL_SELECTION_REGISTRAR: std::sync::LazyLock<CompositionLocal<SelectionRegistrar>> =
    std::sync::LazyLock::new(|| CompositionLocal::new(|| SelectionRegistrar::new()));

static ACTIVE_REGISTRAR: std::sync::LazyLock<Mutex<Option<SelectionRegistrar>>> = std::sync::LazyLock::new(|| Mutex::new(None));

pub(crate) fn active_registrar() -> SelectionRegistrar {
    ACTIVE_REGISTRAR.lock().unwrap().clone().unwrap_or_else(|| SelectionRegistrar::new())
}

pub(crate) fn notify_selection_change() {
    if let Some(reg) = ACTIVE_REGISTRAR.lock().unwrap().as_ref() {
        reg.fire_on_change();
    }
}

// ═══════════════════════════════════════════════════════════
// SelectionContainer
// ═══════════════════════════════════════════════════════════

pub struct SelectionContainer {
    modifier: Modifier,
    on_change: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
}

impl SelectionContainer {
    pub fn new() -> Self {
        SelectionContainer { modifier: Modifier::new(), on_change: None }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    pub fn on_selection_change(mut self, f: impl Fn(usize, usize) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        // 持久化同一个 Registrar（重组时不新建，segments 跨重组保留）
        let registrar = ctx.remember_at_key(key, || SelectionRegistrar::new()).get();
        if let Some(cb) = self.on_change {
            registrar.set_on_change(cb);
        }
        let r = registrar.clone();
        ctx.set_selection_registrar(r.clone());
        *ACTIVE_REGISTRAR.lock().unwrap() = Some(r.clone());
        LOCAL_SELECTION_REGISTRAR.provides(r, || {
            match ctx.start_restartable_group(key, self.modifier, BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    registrar.reset_offsets();
                    content(ctx);
                }
            }
            ctx.end_restartable_group();
        });
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
    fn test_register_and_set_selection() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 20, rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(5, 15);
        assert_eq!(reg.selected_range(42), Some(5..15));
    }

    #[test]
    fn test_cross_text_merged() {
        let reg = SelectionRegistrar::new();
        reg.register(1, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg.register(2, 15, rect(0.0, 0.0, 150.0, 20.0));
        reg.set_selection(5, 20);
        assert_eq!(reg.selected_range(1), Some(5..10));
        assert_eq!(reg.selected_range(2), Some(0..10));
        assert!(reg.selected_range(99).is_none());
    }

    #[test]
    fn test_selection_outside() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg.set_selection(20, 30);
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clear() {
        let reg = SelectionRegistrar::new();
        reg.register(42, 20, rect(0.0, 0.0, 200.0, 40.0));
        reg.set_selection(5, 15);
        reg.clear_selection();
        assert!(reg.selected_range(42).is_none());
    }

    #[test]
    fn test_clone_shared() {
        let reg1 = SelectionRegistrar::new();
        let reg2 = reg1.clone();
        reg1.register(42, 10, rect(0.0, 0.0, 100.0, 20.0));
        reg1.set_selection(2, 8);
        assert_eq!(reg2.selected_range(42), Some(2..8));
    }
}
