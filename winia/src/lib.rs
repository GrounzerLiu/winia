//! Winia — 声明式跨平台 GUI 框架
//!
//! 架构对标 Jetpack Compose，基于 winit + skia-safe。
//!
//! # 核心模块
//! - [`core`]: 运行时核心（State, ComposeCtx, Composer）
//! - `modifier`: 链式 Modifier 系统（待实现）
//! - `layout`: 布局引擎（待实现）
//! - `ui`: UI 组件（待实现）

pub mod core;
pub mod modifier;
pub mod layout;
pub mod ui;
pub mod render;
pub mod app;
pub mod animation;
#[cfg(feature = "debug-server")]
pub mod debug;

#[cfg(not(feature = "debug-server"))]
pub mod debug {
    // no-op stubs
    pub fn start_server() {}
    pub fn has_pending() -> bool { false }
    pub fn set_wake_callback(_cb: impl Fn() + Send + Sync + 'static) {}
    pub fn set_event_loop_proxy(_proxy: winit::event_loop::EventLoopProxy) {}
    pub fn update_tree(_json: &str) {}
    pub fn screenshot_requested() -> bool { false }
    pub fn screenshot_done() {}
    pub fn wake() {}
    pub fn force_shutdown() {}
    pub fn is_shutdown() -> bool { false }
    pub fn update_pixels(_pixels: &[u8], _width: u32, _height: u32) {}
    pub fn take_queued_events() -> Vec<DebugEvent> { Vec::new() }
    pub fn queue_event(_event: DebugEvent) {}
    pub fn simulate_native_click(_x: f32, _y: f32) {}
    pub fn build_tree_json(_root: &crate::layout::node::LayoutNode) -> String { String::new() }
    pub fn set_event_result(_s: &str) {}
    pub fn get_event_result() -> String { String::new() }
    #[derive(Debug, Clone)]
    pub enum DebugEvent { Click { x: f32, y: f32 }, Key { key: String }, Text { value: String }, Scroll { dx: f32, dy: f32 }, Resize { w: f32, h: f32 }, FocusNext, RequestFocus { id: u64 } }
}
pub mod input;

// 公开核心类型
pub use core::composer::{ComposeCtx, Composer, Key};
pub use core::state::State;

/// Prelude: 使用 Winia 时通常需要的所有导入
pub mod prelude {
    pub use crate::core::composer::ComposeCtx;
    pub use crate::core::state::State;
    pub use crate::modifier::{Dimension, Modifier, Shape, Color, ScrollDirection, FocusRequester, ScrollState};
    pub use crate::ui::{Text, TextAlign, TextOverflow, Button, ButtonStyle, Column, Row, Stack, Window, WiniaTheme, ThemeColors};
    pub use crate::ui::theme::is_system_dark_theme;
    pub use crate::layout::{Arrangement, Alignment, Constraints, LayoutDirection};
    pub use crate::animation::{animate_as_state, animate_to, animate_to_cb, Easing};
    pub use crate::ui::theme::current_layout_direction;
    pub use std::time::Duration;
}
