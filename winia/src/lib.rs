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
pub mod unit;
/// 调试日志宏：仅 `debug-server` feature 下打印（用户构建零噪音）。
/// 用法：`debug_log!("[tag] {}", x);`——编译期折叠（非 feature 构建零开销）。
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-server")]
        { eprintln!($($arg)*); }
    };
}

pub mod font;
pub mod icon;
pub mod modifier;
pub mod layout;
pub mod ui;
pub mod text;
pub mod render;
pub mod animation;
pub mod app;
pub mod effect;
pub(crate) mod input;
#[cfg(feature = "debug-server")]
pub mod debug;

#[cfg(not(feature = "debug-server"))]
pub mod debug {
    // no-op stubs
    pub fn start_stdin_channel() {}
    pub fn start_ws_server() {}
    pub fn has_pending() -> bool { false }
    pub fn set_wake_callback(_cb: impl Fn() + Send + Sync + 'static) {}
    pub fn set_event_loop_proxy(_proxy: winit::event_loop::EventLoopProxy) {}
    pub fn update_tree(_window_id: u64, _json: &str) {}
    pub fn remove_tree(_window_id: u64) {}
    pub fn screenshot_requested() -> bool { false }
    pub fn screenshot_done() {}
    pub fn wake() {}
    pub fn force_shutdown() {}
    pub fn is_shutdown() -> bool { false }
    pub fn update_pixels(_pixels: &[u8], _width: u32, _height: u32) {}
    pub fn take_queued_events() -> Vec<DebugEvent> { Vec::new() }
    pub fn queue_event(_event: DebugEvent) {}
    pub fn simulate_native_click(_x: f32, _y: f32) {}
    pub fn build_tree_json(_nodes: &[crate::layout::node::LayoutNode], _root_idx: usize) -> String { String::new() }
    pub fn set_event_result(_s: &str) {}
    pub fn get_event_result() -> String { String::new() }
    #[derive(Debug, Clone)]
    pub enum DebugEvent { Click { x: f32, y: f32 }, Key { key: String }, Text { value: String }, Scroll { dx: f32, dy: f32 }, Resize { w: f32, h: f32 }, FocusNext, RequestFocus { id: u64 }, PointerDown { x: f32, y: f32 }, PointerMove { x: f32, y: f32 }, PointerUp { x: f32, y: f32 } }
}

// 公开核心类型
pub use core::composer::{ComposeCtx, Composer};
pub use core::state::{DerivedFloat, DerivedValue, State};
pub use winia_macros::{app_root, compose, composable, composable_keyed, keyed_stmt, run_app};

/// Prelude: 使用 Winia 时通常需要的所有导入
pub mod prelude {
    pub use crate::core::composer::ComposeCtx;
    pub use crate::core::state::{DerivedFloat, DerivedValue, State};
    pub use crate::modifier::{Dimension, Modifier, Shape, Color, FocusRequester, ScrollState, DecoStyle, DecoMode, FontEdge, FontHint, KbEvent, KbEventType, PointerEvent, PointerEventType, PointerButton, PointerKind, PenKind, BlendMode, ColorFilter, FilterQuality};
    pub use crate::ui::{Text, TextAlign, TextOverflow, TextStyle, ProvideTextStyle, FontWeight, FontSlant, Button, ButtonBorder, ButtonColors, ButtonDefaults, ButtonElevation, ButtonSize, ButtonStyle, Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle, Icon, IconSource, Tint, PathFillType, AxisValue, Image, ContentScale, ImageAlignment, IconButton, IconButtonColors, IconButtonDefaults, IconButtonSize, IconButtonStyle, IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults, Checkbox, CheckboxColors, CheckboxDefaults, TriStateCheckbox, ToggleableState, Switch, SwitchColors, SwitchDefaults, RadioButton, RadioButtonColors, RadioButtonDefaults, Badge, BadgedBox, Slider, SliderColors, SliderDefaults, Column, Row, Stack, Window, WiniaTheme, ThemeColors, SelectionContainer, TextField, TextFieldValue, VisualTransformation, OffsetMapping, IdentityTransformation, PasswordTransformation};
    pub use crate::ui::interaction::{MutableInteractionSource, ComponentState};
    pub use crate::ui::animated_visibility::{AnimatedVisibility, VisibilityTransition, SlideDirection};
    pub use crate::ui::animated_size::AnimatedSize;
    pub use crate::ui::animated_content::AnimatedContent;
    pub use crate::ui::crossfade::Crossfade;
    pub use crate::ui::theme::is_system_dark_theme;
    pub use crate::{app_root, compose, composable, composable_keyed, keyed_stmt, run_app};
    pub use crate::ui::rich_text::RichText;
    pub use crate::text::{InlineDrawable, ImageDrawable, SvgDrawable};
    pub use crate::layout::{Arrangement, Alignment, Constraints, LayoutDirection};
    pub use crate::unit::{Dp, Sp, Offset, Size, Density, Px, DpExt, SpExt, PxExt};
    pub use crate::effect::{LaunchedEffect, DisposableEffect, CoroutineScope, remember_coroutine_scope, observe_watch, with_frame_nanos};
    pub use crate::animation::{
        animate_int_as_state, animate_value_as_state, cancel_animation, DecaySpec,
        exponential_decay, push_decay,
    };
    pub use std::time::Duration;
}
