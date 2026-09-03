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
pub mod nested_scroll;
pub mod nav;
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
    pub fn begin_session() {}
    pub fn end_session() {}
    pub fn has_pending() -> bool { false }
    pub fn set_wake_callback(_cb: impl Fn() + Send + Sync + 'static) {}
    pub fn set_event_loop_proxy(_proxy: winit::event_loop::EventLoopProxy) {}
    pub fn update_tree(_window_id: u64, _json: &str) {}
    pub fn set_overlay_trees(_window_id: u64, _trees: Vec<(u64, String)>) {}
    pub fn remove_tree(_window_id: u64) {}
    pub fn screenshot_requested(_window_id: u64) -> bool { false }
    pub fn screenshot_done(_window_id: u64) {}
    pub fn wake() {}
    pub fn force_shutdown() {}
    pub fn is_shutdown() -> bool { false }
    pub fn update_pixels(_window_id: u64, _pixels: &[u8], _width: u32, _height: u32) {}
    pub fn set_legacy_target(_window_id: u64) {}
    pub fn take_queued_events(_window_id: u64) -> Vec<DebugEvent> { Vec::new() }
    pub fn queued_event_targets() -> std::collections::HashSet<u64> { std::collections::HashSet::new() }
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
pub use core::state::{DerivedFloat, DerivedValue, State, StateId};
pub use nested_scroll::{NestedScrollConnection, NestedScrollDispatcher, NestedScrollSource, ScrollDelta, ScrollVelocity};
pub use winia_macros::{app_root, compose, composable, composable_keyed, keyed_stmt, run_app};

/// Prelude: 使用 Winia 时通常需要的所有导入
pub mod prelude {
    pub use crate::core::composer::ComposeCtx;
    pub use crate::core::state::{DerivedFloat, DerivedValue, State, StateId};
    pub use crate::modifier::{Dimension, Modifier, Shape, Color, FocusRequester, ScrollState, DecoStyle, DecoMode, FontEdge, FontHint, KbEvent, KbEventType, PointerEvent, PointerEventType, PointerButton, PointerKind, PenKind, BlendMode, ColorFilter, FilterQuality};
    pub use crate::ui::{Text, TextAlign, TextOverflow, TextStyle, ProvideTextStyle, FontWeight, FontSlant, Button, ButtonBorder, ButtonColors, ButtonDefaults, ButtonElevation, ButtonSize, ButtonStyle, Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle, Surface, SurfaceBorder, Icon, IconSource, Tint, PathFillType, AxisValue, Image, ContentScale, ImageAlignment, IconButton, IconButtonColors, IconButtonDefaults, IconButtonSize, IconButtonStyle, IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults, Checkbox, CheckboxColors, CheckboxDefaults, TriStateCheckbox, ToggleableState, Switch, SwitchColors, SwitchDefaults, RadioButton, RadioButtonColors, RadioButtonDefaults, Badge, BadgedBox, Slider, SliderColors, SliderDefaults, LinearProgressIndicator, CircularProgressIndicator, ProgressIndicatorDefaults, ProgressIndicatorStrokeCap, LoadingIndicator, LOADING_INDICATOR_SIZE, LOADING_INDICATOR_ACTIVE_SIZE, LOADING_INDICATOR_CONTAINER_SHAPE, LinearWavyProgressIndicator, CircularWavyProgressIndicator, WavyProgressIndicatorDefaults, WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT, WAVY_CIRCULAR_SIZE, WAVY_STROKE_WIDTH, WAVY_TRACK_STROKE_WIDTH, WAVY_GAP_SIZE, WAVY_LINEAR_STOP_SIZE, WAVY_LINEAR_DETERMINATE_WAVELENGTH, WAVY_LINEAR_INDETERMINATE_WAVELENGTH, WAVY_CIRCULAR_WAVELENGTH, WAVY_ANIMATION_DURATION_MS, FloatingActionButton, FloatingActionButtonColors, ExtendedFloatingActionButton, EXTENDED_FAB_COLLAPSED_WIDTH, EXTENDED_FAB_HEIGHT, EXTENDED_FAB_MIN_EXPANDED_WIDTH, FloatingActionButtonDefaults, FloatingActionButtonElevation, FloatingActionButtonSize, FAB_SMALL_SIZE, FAB_REGULAR_SIZE, FAB_MEDIUM_SIZE, FAB_LARGE_SIZE, FAB_ICON_SIZE, FAB_MEDIUM_ICON_SIZE, FAB_LARGE_ICON_SIZE, Divider, DividerDefaults, DIVIDER_THICKNESS, DIVIDER_HAIRLINE, LazyColumn, LazyListState, LazyRow, ItemHeightCache, ListItem, ListItemColors, ListItemDefaults, LIST_ITEM_ONE_LINE_HEIGHT, LIST_ITEM_TWO_LINE_HEIGHT, LIST_ITEM_THREE_LINE_HEIGHT, LIST_ITEM_HORIZONTAL_PADDING, LIST_ITEM_VERTICAL_PADDING, LIST_ITEM_SLOT_GAP, LIST_ITEM_CONTENT_GAP, TopAppBar, TopAppBarColors, TopAppBarScrollBehavior, TopAppBarState, TopAppBarNestedConnection, TopAppBarScrollMode, TopAppBarVariant, TOP_APP_BAR_HEIGHT, TOP_APP_BAR_MEDIUM_HEIGHT, TOP_APP_BAR_LARGE_HEIGHT, TOP_APP_BAR_HORIZONTAL_PADDING, Scaffold, ScaffoldContentPadding, ScaffoldFabPosition, SCAFFOLD_FAB_MARGIN, NavigationBar, NavigationBarItem, NavigationBarColors, NavigationBarItemColors, ShortNavigationBar, ShortNavigationBarItem, ShortNavigationBarArrangement, NavigationSuiteScaffold, NavigationSuiteType, window_size, NavigationRail, NavigationRailItem, NavigationRailItemColors, WideNavigationRail, WideNavigationRailItem, WideNavigationRailState, WindowInsets, NavigationBarDefaults, NAVIGATION_BAR_HEIGHT, NAVIGATION_BAR_ITEM_SPACING, NAVIGATION_BAR_H_INDICATOR_HEIGHT, NavigationItemIconPosition, NAVIGATION_BAR_INDICATOR_WIDTH, NAVIGATION_BAR_INDICATOR_HEIGHT, NAVIGATION_BAR_ICON_SIZE, NAVIGATION_RAIL_WIDTH, NAVIGATION_RAIL_ITEM_HEIGHT, NAVIGATION_RAIL_INDICATOR_WIDTH, NAVIGATION_RAIL_INDICATOR_HEIGHT, NAVIGATION_RAIL_ICON_SIZE, WIDE_RAIL_COLLAPSED_WIDTH, WIDE_RAIL_EXPANDED_MIN_WIDTH, Column, Row, Stack, Spacer, Window, WiniaTheme, ThemeColors, Typography, SelectionContainer, TextField, TextFieldValue, VisualTransformation, OffsetMapping, IdentityTransformation, PasswordTransformation, Tab, TabRow, TabRowDefaults, TabPosition, TAB_ROW_HEIGHT, ACTIVE_INDICATOR_HEIGHT, HORIZONTAL_TEXT_PADDING, MIN_INDICATOR_WIDTH, LARGE_TAB_HEIGHT, SMALL_TAB_HEIGHT};
    pub use crate::ui::interaction::{MutableInteractionSource, ComponentState};
    pub use crate::ui::snackbar::{Snackbar, SnackbarData, SnackbarDuration, SnackbarHost, SnackbarHostState};
    pub use crate::ui::bottom_sheet::ModalBottomSheet;
    pub use crate::ui::bottom_sheet_scaffold::{BottomSheetScaffold, SCAFFOLD_SHEET_PEEK_HEIGHT};
    pub use crate::ui::sheet_state::{SheetState, SheetValue};
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
