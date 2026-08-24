//! 窗口自适应基础设施——WindowSizeClass（对齐 androidx WindowSizeClass 断点）。
//!
//! winia 的窗口尺寸经 thread_local 每帧注入（app.rs recompose_layout_render，
//! 与 Density 同一注入点）——组合期读 [`window_width_size_class`] 等即可做
//! 自适应布局。窗口 resize 由 app 层触发重组（SurfaceResized →
//! request_recomposition），尺寸类随之刷新。

use std::cell::{Cell, RefCell};

use crate::core::state::State;

thread_local! {
    /// 当前组合所属窗口的逻辑尺寸（物理 / scale_factor）——兜底值
    static WINDOW_SIZE: Cell<(f32, f32)> = const { Cell::new((800.0, 600.0)) };
    /// 窗口尺寸的响应式 State（app 层挂载）——window_size() 读它注册依赖，
    /// resize 时 set 通知 → 依赖该尺寸的组件（NavigationSuiteScaffold 等）
    /// 所在 slot dirty → 形态切换。None = 退化为 thread_local 兜底（无响应式）
    static WINDOW_SIZE_STATE: RefCell<Option<State<(f32, f32)>>> = const { RefCell::new(None) };
}

/// 注入当前窗口逻辑尺寸（app 层每帧组合前调用——非响应式兜底路径）
pub fn set_window_size(width: f32, height: f32) {
    WINDOW_SIZE.with(|s| s.set((width, height)));
}

/// 挂载窗口尺寸的响应式 State（app 层组合时调用——remember 产物，
/// owner queue 指向当前 Composer，resize set() 才能路由到 pending 队列）
pub fn set_window_size_state(state: State<(f32, f32)>) {
    WINDOW_SIZE_STATE.with(|c| *c.borrow_mut() = Some(state));
}

/// 当前窗口逻辑尺寸。挂载了 State 时走 `get()`——**注册依赖**：
/// 读取方（套件脚手架等）随 resize 自动重组
pub fn window_size() -> (f32, f32) {
    let attached = WINDOW_SIZE_STATE.with(|c| c.borrow().clone());
    if let Some(s) = attached {
        return s.get();
    }
    WINDOW_SIZE.with(|s| s.get())
}

/// 测试隔离：清除挂载的尺寸 State（退回 thread_local 兜底）
#[cfg(test)]
pub(crate) fn reset_window_size_state() {
    WINDOW_SIZE_STATE.with(|c| *c.borrow_mut() = None);
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
}
