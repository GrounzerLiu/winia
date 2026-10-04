//! Winia — a declarative cross-platform GUI framework.
//!
//! The architecture follows Jetpack Compose; the backend is winit + skia-safe. A module's layer is
//! the direction it is allowed to depend in, lowest first:
//!
//! - [`mod@unit`], [`mod@graphics`] — the values everything else is expressed in (`Dp`, `Color`, `Shape`);
//! - [`text`], [`animation`], [`input`] — text layout, animation, and the event vocabulary;
//! - [`layout`], [`runtime`] — the measure/place pass and the composition runtime (`State`,
//!   `ComposeCtx`, `Composer`, [`modifier`]);
//! - [`render`], [`transition`] — painting and the shared-element machinery;
//! - [`components`], [`theme`], [`overlay`], [`nav`], [`selection`], [`semantics`] — what a caller
//!   builds with;
//! - [`app`] — the winit application and the window that hosts a tree.
//!
//! [`debug`] is the dev-server channel and [`accessibility`] the Windows UI Automation bridge; both
//! are stubs unless their feature is on.
//!
//! # The one sanctioned inversion
//!
//! [`runtime`] names `crate::overlay::OverlayDesc` in three places: `ComposeCtx::open_overlay`
//! queues one, `Composer` holds the queue, and `Composer::take_overlays` hands it to the frame that
//! hosts it. The record is the overlay layer's — it carries `PopupPosition`, `OverlayAnimSpec`, the
//! dismissal flags — and the runtime does exactly one thing to it: it stamps
//! `local_snapshot` with the composition locals captured at the call site, because an overlay
//! composes in its own `Composer` after those providers have popped and cannot read the main tree's
//! theme or direction otherwise.
//!
//! Moving the record into `runtime` to satisfy the direction would take `PopupPosition`,
//! `OverlayAnimSpec` and the four presentation helpers they need with it, which places five things
//! worse than it fixes one. So the edge is deliberate, narrow — one type — and recorded here.

pub mod runtime;
pub mod unit;
pub mod anim_trace;
/// 调试日志宏：仅 `debug-server` feature 下打印（用户构建零噪音）。
/// 用法：`debug_log!("[tag] {}", x);`——编译期折叠（非 feature 构建零开销）。
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(feature = "debug-server")]
        { eprintln!($($arg)*); }
    };
}

pub mod graphics;
pub mod icon;
pub mod modifier;
pub mod interaction;
pub mod layout;
pub mod components;
pub mod text;
pub mod render;
pub mod animation;
pub mod nested_scroll;
pub mod nav;
pub mod semantics;
pub mod theme;
pub mod overlay;
pub mod transition;
pub mod selection;
/// The Windows UI Automation bridge (feature `accessibility`, Windows only): publishes the semantics
/// tree to the OS so screen readers can read and operate the UI. See `docs/semantics.md`.
#[cfg(all(windows, feature = "accessibility"))]
pub mod accessibility;
/// Outside Windows, or without the feature, the bridge is absent — call sites stay unconditional.
#[cfg(not(all(windows, feature = "accessibility")))]
pub mod accessibility {
    pub fn install(_window: &dyn winit::window::Window, _window_id: u64) -> bool {
        false
    }
    pub fn uninstall(_window: &dyn winit::window::Window, _window_id: u64) {}
    pub fn notify(_window_id: u64, _snapshot: &crate::semantics::WindowSemantics) {}
}
pub mod app;
pub mod effect;
pub mod input;
pub mod debug;

// 公开核心类型
pub use runtime::composer::{ComposeCtx, Composer};
pub use runtime::state::{
    Animating, Backchannel, DerivedFloat, DerivedValue, Reactive, State, StateId, Visual,
};
// Observable collections: `mutableStateListOf` / `mutableStateMapOf`.
pub use runtime::state_list::{ListSnapshot, MapSnapshot, StateList, StateMap};
pub use nested_scroll::{NestedScrollConnection, NestedScrollDispatcher, NestedScrollSource, ScrollDelta, ScrollVelocity};
pub use winia_macros::{app_root, compose, composable, composable_keyed, keyed_stmt, run_app};

/// Prelude: 使用 Winia 时通常需要的所有导入
pub mod prelude {
    pub use crate::runtime::composer::ComposeCtx;
    pub use crate::runtime::state_list::{ListSnapshot, MapSnapshot, StateList, StateMap};
    pub use crate::runtime::state::{
        Animating, Backchannel, DerivedFloat, DerivedValue, Reactive, State, StateId, Visual,
    };
    pub use crate::graphics::{Brush, BrushTile};
    pub use crate::graphics::{
        AxisValue, ContentScale, IconSource, ImageAlignment, PathFillType, SymbolAxes,
    };
    pub use crate::modifier::{Modifier, FocusRequester, ScrollState};
    pub use crate::layout::{Dimension};
    pub use crate::text::{DecoStyle, DecoMode, FontEdge, FontHint};
    pub use crate::input::{KbEvent, KbEventType, PointerEvent, PointerEventType, PointerButton, PointerKind, PenKind};
    pub use crate::graphics::{Shape, Color, BlendMode, ColorFilter, FilterQuality};
    pub use crate::components::{
        Text, ProvideTextStyle, Button, ButtonBorder, ButtonColors, ButtonDefaults, ButtonElevation, ButtonSize,
        ButtonStyle, Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle, Surface,
        SurfaceBorder, Icon, Tint, Image, IconButton, IconButtonColors, IconButtonDefaults, IconButtonSize,
        IconButtonStyle, IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults, Checkbox,
        CheckboxColors, CheckboxDefaults, TriStateCheckbox, Switch, SwitchColors, SwitchDefaults, RadioButton,
        RadioButtonColors, RadioButtonDefaults, Badge, BadgedBox, Slider, SliderColors, SliderDefaults,
        RangeSlider, RangeThumb, RangeValue, SegmentedButton, SegmentedButtonColors, SegmentedButtonDefaults,
        SingleChoiceSegmentedButtonRow, MultiChoiceSegmentedButtonRow, VerticalScrollbar, HorizontalScrollbar,
        LazyScrollbar, HorizontalLazyScrollbar, SCROLLBAR_THICKNESS, SCROLLBAR_THUMB_MIN_LENGTH,
        SCROLLBAR_THUMB_MAX_FRACTION, LinearProgressIndicator, CircularProgressIndicator,
        ProgressIndicatorDefaults, ProgressIndicatorStrokeCap, LoadingIndicator, LOADING_INDICATOR_SIZE,
        LOADING_INDICATOR_ACTIVE_SIZE, LOADING_INDICATOR_CONTAINER_SHAPE, LinearWavyProgressIndicator,
        CircularWavyProgressIndicator, WavyProgressIndicatorDefaults, WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT,
        WAVY_CIRCULAR_SIZE, WAVY_STROKE_WIDTH, WAVY_TRACK_STROKE_WIDTH, WAVY_GAP_SIZE, WAVY_LINEAR_STOP_SIZE,
        WAVY_LINEAR_DETERMINATE_WAVELENGTH, WAVY_LINEAR_INDETERMINATE_WAVELENGTH, WAVY_CIRCULAR_WAVELENGTH,
        WAVY_ANIMATION_DURATION_MS, FloatingActionButton, FloatingActionButtonColors,
        ExtendedFloatingActionButton, EXTENDED_FAB_COLLAPSED_WIDTH, EXTENDED_FAB_HEIGHT,
        EXTENDED_FAB_MIN_EXPANDED_WIDTH, FloatingActionButtonDefaults, FloatingActionButtonElevation,
        FloatingActionButtonSize, FAB_SMALL_SIZE, FAB_REGULAR_SIZE, FAB_MEDIUM_SIZE, FAB_LARGE_SIZE,
        FAB_ICON_SIZE, FAB_MEDIUM_ICON_SIZE, FAB_LARGE_ICON_SIZE, Divider, DividerDefaults, DIVIDER_THICKNESS,
        DIVIDER_HAIRLINE, ListItem, ListItemColors, ListItemDefaults, LIST_ITEM_ONE_LINE_HEIGHT,
        LIST_ITEM_TWO_LINE_HEIGHT, LIST_ITEM_THREE_LINE_HEIGHT, LIST_ITEM_HORIZONTAL_PADDING,
        LIST_ITEM_VERTICAL_PADDING, LIST_ITEM_SLOT_GAP, LIST_ITEM_CONTENT_GAP, TopAppBar, TopAppBarColors,
        TopAppBarScrollBehavior, TopAppBarState, TopAppBarNestedConnection, TopAppBarScrollMode,
        TopAppBarVariant, TOP_APP_BAR_HEIGHT, TOP_APP_BAR_MEDIUM_HEIGHT, TOP_APP_BAR_LARGE_HEIGHT,
        TOP_APP_BAR_HORIZONTAL_PADDING, Scaffold, ScaffoldContentPadding, ScaffoldFabPosition,
        SCAFFOLD_FAB_MARGIN, NavigationBar, NavigationBarItem, NavigationBarColors, NavigationBarItemColors,
        ShortNavigationBar, ShortNavigationBarItem, ShortNavigationBarArrangement, NavigationSuiteScaffold,
        NavigationSuiteType, NavigationRail, NavigationRailItem, NavigationRailItemColors, WideNavigationRail,
        WideNavigationRailItem, WideNavigationRailState, WindowInsets, NavigationBarDefaults,
        NAVIGATION_BAR_HEIGHT, NAVIGATION_BAR_ITEM_SPACING, NAVIGATION_BAR_H_INDICATOR_HEIGHT,
        NavigationItemIconPosition, NAVIGATION_BAR_INDICATOR_WIDTH, NAVIGATION_BAR_INDICATOR_HEIGHT,
        NAVIGATION_BAR_ICON_SIZE, NAVIGATION_RAIL_WIDTH, NAVIGATION_RAIL_ITEM_HEIGHT,
        NAVIGATION_RAIL_INDICATOR_WIDTH, NAVIGATION_RAIL_INDICATOR_HEIGHT, NAVIGATION_RAIL_ICON_SIZE,
        WIDE_RAIL_COLLAPSED_WIDTH, WIDE_RAIL_EXPANDED_MIN_WIDTH, ModalNavigationDrawer, ModalDrawerSheet,
        DrawerState, DrawerValue, DrawerDefaults, NavigationDrawerItem, NavigationDrawerItemColors,
        DRAWER_MAX_WIDTH, DRAWER_MIN_WIDTH, DRAWER_CORNER_RADIUS, DRAWER_ITEM_HEIGHT, DRAWER_ITEM_ICON_SIZE,
        DRAWER_SHEET_HORIZONTAL_PADDING, DRAWER_ITEM_START_PADDING, DRAWER_ITEM_END_PADDING,
        DRAWER_ITEM_SLOT_GAP, AlertDialog, AlertDialogDefaults, BasicAlertDialog, DIALOG_MIN_WIDTH,
        DIALOG_MAX_WIDTH, DIALOG_CORNER_RADIUS, DIALOG_CONTAINER_PADDING, DIALOG_ICON_PADDING_BOTTOM,
        DIALOG_TITLE_PADDING_BOTTOM, DIALOG_TEXT_PADDING_BOTTOM, DIALOG_BUTTON_SPACING, DIALOG_ICON_SIZE,
        SwipeToDismissBox, SwipeToDismissBoxState, SwipeToDismissBoxValue, SWIPE_DISMISS_POSITIONAL_THRESHOLD,
        SWIPE_DISMISS_VELOCITY_THRESHOLD, SelectionContainer, TextField, TextFieldValue, Tab, TabRow,
        TabRowDefaults, TabPosition, ScrollableTabRow, ScrollableTabRowDefaults, TAB_ROW_HEIGHT,
        ACTIVE_INDICATOR_HEIGHT, HORIZONTAL_TEXT_PADDING, MIN_INDICATOR_WIDTH, LARGE_TAB_HEIGHT,
        SMALL_TAB_HEIGHT, SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH, SCROLLABLE_TAB_ROW_EDGE_START_PADDING,
    };
    pub use crate::text::{TextAlign, TextOverflow, TextStyle, FontWeight, FontSlant};
    pub use crate::interaction::{MutableInteractionSource, ComponentState};
    // The general drawing surface (Compose `Canvas` / `DrawScope`) and the constraints-aware box
    // (Compose `BoxWithConstraints`) — the two entry points a caller reaches for when no existing
    // component expresses what they need.
    pub use crate::layout::{BoxWithConstraints, BoxWithConstraintsScope};
    pub use crate::components::{draw_behind, draw_with_content, Canvas, DrawScope, TextMetrics};
    // Accessibility declarations belong in the same scope as `Modifier` — a caller states a role or a
    // name while building the chain.
    pub use crate::selection::ToggleableState;
    pub use crate::semantics::{SemanticsConfig, SemanticsRole, SemanticsState};
    pub use crate::components::snackbar::{Snackbar, SnackbarData, SnackbarDuration, SnackbarHost, SnackbarHostState};
    pub use crate::components::bottom_sheet::ModalBottomSheet;
    pub use crate::components::split_button::{
        LeadingButton, SplitButtonDefaults, SplitButtonLayout, SplitButtonShapes, TrailingButton,
    };
    pub use crate::components::bottom_sheet_scaffold::{BottomSheetScaffold, SCAFFOLD_SHEET_PEEK_HEIGHT};
    pub use crate::components::sheet_state::{SheetState, SheetValue};
    pub use crate::components::animated_visibility::{
        AnimatedVisibility,
    };
    pub use crate::animation::{
        ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
    };
    pub use crate::components::search_bar::{
        SearchBar, SearchBarColors, SearchBarDefaults, SearchBarState, DockedSearchBar,
        SEARCH_ICON_PATH, BACK_ICON_PATH, SEARCH_BAR_HEIGHT,
    };
    pub use crate::components::animated_size::AnimatedSize;
    pub use crate::components::animated_content::AnimatedContent;
    pub use crate::components::crossfade::Crossfade;
    pub use crate::components::shared_transition::{
        SharedContentState, SharedTransitionDefaults, SharedTransitionLayout, SharedTransitionScope,
        current_shared_scope,
    };
    pub use crate::transition::{
        BoundsTransform, OverlayClip, PathMotion, PlaceHolderSize, ResizeMode, SharedBounds, SharedKind,
    };
    pub use crate::{app_root, compose, composable, composable_keyed, keyed_stmt, run_app};
    pub use crate::components::rich_text::RichText;
    // The overlay runtime: a popup and a dialog.
    pub use crate::overlay::{Dialog, OverlayAnimSpec, Popup, PopupPosition};
    // …and the menu built on it, which is a component.
    pub use crate::components::dropdown_menu::{
        DropdownMenu, DropdownMenuItem, ExposedDropdownMenuAnchorType, ExposedDropdownMenuBox,
        ExposedDropdownMenuDefaults, MenuDefaults, MenuItemColors,
    };
    pub use crate::overlay::anchored_draggable::{AnchoredDraggableState, DraggableAnchors};
    pub use crate::app::window::Window;
    pub use crate::theme::{
        is_system_dark_theme, set_system_dark_mode, ThemeColors, ThemeSpec, Typography,
        WiniaTheme,
    };
    pub use crate::text::{
        IdentityTransformation, OffsetMapping, PasswordTransformation, VisualTransformation,
    };
    pub use crate::text::{InlineDrawable, ImageDrawable, SvgDrawable};
    pub use crate::layout::{Column, FlowColumn, FlowRow, Row, Spacer, Stack};
    pub use crate::layout::{ItemHeightCache, LazyColumn, LazyListState, LazyRow};
    pub use crate::layout::{set_window_size, window_size, HeightSizeClass, WidthSizeClass};
    pub use crate::layout::{Arrangement, Alignment, Constraints, LayoutDirection};
    pub use crate::unit::{Dp, Sp, Offset, Size, Density, Px, DpExt, SpExt, PxExt};
    pub use crate::effect::{LaunchedEffect, DisposableEffect, CoroutineScope, remember_coroutine_scope, observe_watch, with_frame_nanos};
    pub use crate::animation::{
        animate_int_as_state, animate_value_as_state, cancel_animation, DecaySpec,
        exponential_decay, push_decay,
    };
    pub use std::time::Duration;
}
