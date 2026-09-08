//! 窗口自适应基础设施——WindowSizeClass（对齐 androidx WindowSizeClass 断点）。
//!
//! Winia 的窗口尺寸由每个 Composer 的 adaptive context 持有；Composer
//! compose/layout 期间将该 context 注入当前运行帧。组合期读
//! [`window_width_size_class`] 等即可做自适应布局。窗口 resize 由 app 层触发
//! 重组（SurfaceResized → request_recomposition），尺寸类随之刷新。

use std::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

use crate::core::state::{Backchannel, State};

/// Per-Composer adaptive window context. The public adaptive API resolves the
/// context active during compose/layout, so interleaved windows do not share a
/// thread-local singleton.
#[derive(Clone)]
pub(crate) struct AdaptiveContext {
    size: Arc<Mutex<(f32, f32)>>,
    size_state: Arc<Mutex<Option<State<(f32, f32)>>>>,
}

impl AdaptiveContext {
    pub(crate) fn new() -> Self {
        Self {
            size: Arc::new(Mutex::new(fallback_window_size())),
            // A Composer owns its responsive State. Do not inherit another
            // Composer's fallback State when a new context is created.
            size_state: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn set_size(&self, width: f32, height: f32) {
        *self.size.lock().unwrap() = (width, height);
    }

    pub(crate) fn set_size_state(&self, state: State<(f32, f32)>) {
        *self.size_state.lock().unwrap() = Some(state);
    }
}

thread_local! {
    /// Compatibility fallback for callers outside a Composer (for example
    /// pure adaptive unit tests). Production compose/layout uses AdaptiveContext.
    static FALLBACK_WINDOW_SIZE: Cell<(f32, f32)> = const { Cell::new((800.0, 600.0)) };
    static FALLBACK_WINDOW_SIZE_STATE: RefCell<Option<State<(f32, f32)>>> = const { RefCell::new(None) };
    static ACTIVE_CONTEXTS: RefCell<Vec<AdaptiveContext>> = const { RefCell::new(Vec::new()) };
}

fn fallback_window_size() -> (f32, f32) {
    FALLBACK_WINDOW_SIZE.with(|size| size.get())
}

fn fallback_window_size_state() -> Option<State<(f32, f32)>> {
    FALLBACK_WINDOW_SIZE_STATE.with(|state| state.borrow().clone())
}

fn active_context() -> Option<AdaptiveContext> {
    ACTIVE_CONTEXTS.with(|contexts| contexts.borrow().last().cloned())
}

pub(crate) struct AdaptiveContextGuard;

pub(crate) fn enter_context(context: AdaptiveContext) -> AdaptiveContextGuard {
    ACTIVE_CONTEXTS.with(|contexts| contexts.borrow_mut().push(context));
    AdaptiveContextGuard
}

impl Drop for AdaptiveContextGuard {
    fn drop(&mut self) {
        ACTIVE_CONTEXTS.with(|contexts| {
            contexts.borrow_mut().pop();
        });
    }
}

/// 注入当前窗口逻辑尺寸。Composer 外调用时更新兼容 fallback；Composer
/// compose/layout 内调用时更新当前 Composer 的上下文。
pub fn set_window_size(width: f32, height: f32) {
    if let Some(context) = active_context() {
        context.set_size(width, height);
    } else {
        FALLBACK_WINDOW_SIZE.with(|size| size.set((width, height)));
    }
}

/// 挂载窗口尺寸的响应式 State。Composer 上下文内只绑定当前 Composer，
/// Composer 外保留测试/旧调用方的 fallback 语义。
pub fn set_window_size_state(state: State<(f32, f32)>) {
    if let Some(context) = active_context() {
        context.set_size_state(state);
    } else {
        FALLBACK_WINDOW_SIZE_STATE.with(|current| *current.borrow_mut() = Some(state));
    }
}

/// Per-frame sync of the window-size value without notify (Backchannel write).
/// The `set()` on resize (app.rs SurfaceResized path) is the notifying write
/// that drives recomposition; this keeps the stored value current in between.
pub fn sync_window_size_state(state: &Backchannel<(f32, f32)>, size: (f32, f32)) {
    state.set(size);
}

/// 当前窗口逻辑尺寸。挂载了 State 时走 `get()` 注册当前 Composer 依赖。
pub fn window_size() -> (f32, f32) {
    let (size, state) = if let Some(context) = active_context() {
        (
            *context.size.lock().unwrap(),
            context.size_state.lock().unwrap().clone(),
        )
    } else {
        (fallback_window_size(), fallback_window_size_state())
    };
    state.map(|state| state.get()).unwrap_or(size)
}

/// 测试隔离：清除 fallback 尺寸 State。
#[cfg(test)]
pub(crate) fn reset_window_size_state() {
    FALLBACK_WINDOW_SIZE_STATE.with(|state| *state.borrow_mut() = None);
    if let Some(context) = active_context() {
        *context.size_state.lock().unwrap() = None;
    }
}

/// 宽度尺寸类（androidx WindowWidthSizeClass 断点：<600 / 600–840 / ≥840）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthSizeClass {
    Compact,
    Medium,
    Expanded,
}

/// 高度尺寸类（androidx WindowHeightSizeClass 断点：<480 / 480–900 / ≥900）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightSizeClass {
    Compact,
    Medium,
    Expanded,
}

pub fn window_width_size_class() -> WidthSizeClass {
    let (w, _) = window_size();
    if w >= 840.0 {
        WidthSizeClass::Expanded
    } else if w >= 600.0 {
        WidthSizeClass::Medium
    } else {
        WidthSizeClass::Compact
    }
}

pub fn window_height_size_class() -> HeightSizeClass {
    let (_, h) = window_size();
    if h >= 900.0 {
        HeightSizeClass::Expanded
    } else if h >= 480.0 {
        HeightSizeClass::Medium
    } else {
        HeightSizeClass::Compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_breakpoints_match_androidx() {
        reset_window_size_state();
        set_window_size(599.0, 600.0);
        assert_eq!(window_width_size_class(), WidthSizeClass::Compact);
        set_window_size(600.0, 600.0);
        assert_eq!(window_width_size_class(), WidthSizeClass::Medium);
        set_window_size(840.0, 600.0);
        assert_eq!(window_width_size_class(), WidthSizeClass::Expanded);
    }

    #[test]
    fn height_breakpoints_match_androidx() {
        reset_window_size_state();
        set_window_size(800.0, 479.0);
        assert_eq!(window_height_size_class(), HeightSizeClass::Compact);
        set_window_size(800.0, 480.0);
        assert_eq!(window_height_size_class(), HeightSizeClass::Medium);
        set_window_size(800.0, 900.0);
        assert_eq!(window_height_size_class(), HeightSizeClass::Expanded);
    }

    #[test]
    fn nested_composers_keep_adaptive_context_isolated() {
        reset_window_size_state();
        let mut outer = crate::core::composer::Composer::new();
        let mut inner = crate::core::composer::Composer::new();

        outer.compose(|_| {
            set_window_size(500.0, 700.0);
            assert_eq!(window_size(), (500.0, 700.0));

            inner.compose(|_| {
                set_window_size(1000.0, 400.0);
                assert_eq!(window_size(), (1000.0, 400.0));
            });

            assert_eq!(window_size(), (500.0, 700.0));
        });

        reset_window_size_state();
        assert_eq!(window_size(), (800.0, 600.0));
    }
}
