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

pub mod assets;
pub mod runtime;
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

/// The shape type a caller passes to `LoadingIndicator::shapes` — the very `RoundedPolygon` winia builds
/// its Material shapes from, re-exported so a caller does not need to depend on `material-shapes` itself.
pub use material_shapes::RoundedPolygon;

pub mod animation;
/// Clipboard access: reads through `arboard`, writes through winit. See the module header for why.
mod clipboard;
pub mod components;
pub mod focus;
pub mod graphics;
pub mod icon;
pub mod layout;
pub mod modifier;
pub mod render;
/// The scroll family: fling strategies, the nested-scroll protocol and overscroll.
///
/// The modules keep their historical crate-root names through the re-export below
/// (`winia::fling`, `winia::nested_scroll`, `winia::overscroll`).
pub mod scroll;
pub mod text;
pub use scroll::{fling, nested as nested_scroll, overscroll};
/// The Windows UI Automation bridge (feature `accessibility`, Windows only): publishes the semantics
/// tree to the OS so screen readers can read and operate the UI. See `docs/semantics.md`.
#[cfg(all(windows, feature = "accessibility"))]
pub mod accessibility;
pub mod nav;
pub mod overlay;
pub mod selection;
pub mod semantics;
pub mod theme;
pub mod transition;
pub mod window;
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
pub mod debug;
pub mod effect;
pub mod input;
pub mod permissions;
#[cfg(target_os = "android")]
pub mod platform;

// 公开核心类型
#[cfg(target_os = "android")]
pub use jni;
#[cfg(target_os = "android")]
pub use platform::android::AndroidJni;
#[cfg(target_os = "android")]
pub use runtime::composer::AndroidUi;
pub use runtime::composer::{ComposeCtx, Composer};
pub use runtime::movable::MovableContent;
pub use runtime::state::{
    Animating, Backchannel, DerivedFloat, DerivedValue, Reactive, State, StateId, Visual,
};
// Observable collections: `mutableStateListOf` / `mutableStateMapOf`.
pub use nested_scroll::{
    NestedScrollConnection, NestedScrollDispatcher, NestedScrollSource, ScrollDelta, ScrollVelocity,
};
pub use runtime::state_list::{ListSnapshot, MapSnapshot, StateList, StateMap};
pub use winia_macros::{app_root, composable, composable_keyed, compose, keyed_stmt, run_app};

/// Prelude: 使用 Winia 时通常需要的所有导入
pub mod prelude {
    pub use crate::components::button::{ButtonStyling, ButtonStylingScope};
    pub use crate::components::checkbox::{CheckboxStyling, CheckboxStylingScope};
    pub use crate::components::component_styles::{
        ComponentStyles, Styling, with_component_styles,
    };
    pub use crate::components::{
        ACTIVE_INDICATOR_HEIGHT, AlertDialog, AlertDialogDefaults, BUTTON_GROUP_BETWEEN_SPACE,
        BUTTON_GROUP_COMPRESSION_LIMIT, BUTTON_GROUP_EXPANDED_RATIO, BUTTON_GROUP_HEIGHT, Badge,
        BadgedBox, BasicAlertDialog, Button, ButtonBorder, ButtonColors, ButtonDefaults,
        ButtonElevation, ButtonGroup, ButtonGroupDefaults, ButtonGroupItem, ButtonGroupMenuState,
        ButtonGroupShapes, ButtonSize, ButtonStyle, CONNECTED_BUTTON_GROUP_BETWEEN_SPACE,
        CONNECTED_INNER_CORNER_SIZE, CONNECTED_PRESSED_INNER_CORNER_SIZE, Card, CardBorder,
        CardColors, CardDefaults, CardElevation, CardStyle, Checkbox, CheckboxColors,
        CheckboxDefaults, CircularProgressIndicator, CircularWavyProgressIndicator,
        DIALOG_BUTTON_SPACING, DIALOG_CONTAINER_PADDING, DIALOG_CORNER_RADIUS,
        DIALOG_ICON_PADDING_BOTTOM, DIALOG_ICON_SIZE, DIALOG_MAX_WIDTH, DIALOG_MIN_WIDTH,
        DIALOG_TEXT_PADDING_BOTTOM, DIALOG_TITLE_PADDING_BOTTOM, DIVIDER_HAIRLINE,
        DIVIDER_THICKNESS, DRAWER_CORNER_RADIUS, DRAWER_ITEM_END_PADDING, DRAWER_ITEM_HEIGHT,
        DRAWER_ITEM_ICON_SIZE, DRAWER_ITEM_SLOT_GAP, DRAWER_ITEM_START_PADDING, DRAWER_MAX_WIDTH,
        DRAWER_MIN_WIDTH, DRAWER_SHEET_HORIZONTAL_PADDING, DismissibleDrawerSheet,
        DismissibleNavigationDrawer, Divider, DividerDefaults, DrawerDefaults, DrawerSheet,
        DrawerSheetKind, DrawerState, DrawerValue, EXTENDED_FAB_COLLAPSED_WIDTH,
        EXTENDED_FAB_HEIGHT, EXTENDED_FAB_MIN_EXPANDED_WIDTH, ExtendedFabSize,
        ExtendedFloatingActionButton, FAB_ICON_SIZE, FAB_LARGE_ICON_SIZE, FAB_LARGE_SIZE,
        FAB_MEDIUM_ICON_SIZE, FAB_MEDIUM_SIZE, FAB_REGULAR_SIZE, FAB_SMALL_SIZE,
        FloatingActionButton, FloatingActionButtonColors, FloatingActionButtonDefaults,
        FloatingActionButtonElevation, FloatingActionButtonSize, HORIZONTAL_TEXT_PADDING,
        HorizontalLazyScrollbar, HorizontalPager, HorizontalScrollbar, Icon, IconButton,
        IconButtonColors, IconButtonDefaults, IconButtonShape, IconButtonSize, IconButtonStyle,
        IconButtonWidthOption, IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults,
        Image, LARGE_TAB_HEIGHT, LIST_ITEM_CONTENT_GAP, LIST_ITEM_HORIZONTAL_PADDING,
        LIST_ITEM_ONE_LINE_HEIGHT, LIST_ITEM_SLOT_GAP, LIST_ITEM_THREE_LINE_HEIGHT,
        LIST_ITEM_TWO_LINE_HEIGHT, LIST_ITEM_VERTICAL_PADDING, LOADING_INDICATOR_ACTIVE_SIZE,
        LOADING_INDICATOR_CONTAINER_SHAPE, LOADING_INDICATOR_SIZE, LazyScrollbar,
        LinearProgressIndicator, LinearWavyProgressIndicator, ListItem, ListItemColors,
        ListItemDefaults, LoadingIndicator, MIN_INDICATOR_WIDTH, ModalDrawerSheet,
        ModalNavigationDrawer, ModalWideNavigationRail, MultiChoiceSegmentedButtonRow,
        NAVIGATION_BAR_H_INDICATOR_HEIGHT, NAVIGATION_BAR_HEIGHT, NAVIGATION_BAR_ICON_SIZE,
        NAVIGATION_BAR_INDICATOR_HEIGHT, NAVIGATION_BAR_INDICATOR_WIDTH,
        NAVIGATION_BAR_ITEM_SPACING, NAVIGATION_RAIL_ICON_SIZE, NAVIGATION_RAIL_INDICATOR_HEIGHT,
        NAVIGATION_RAIL_INDICATOR_WIDTH, NAVIGATION_RAIL_ITEM_HEIGHT, NAVIGATION_RAIL_WIDTH,
        NavigationBar, NavigationBarColors, NavigationBarDefaults, NavigationBarItem,
        NavigationBarItemColors, NavigationDrawerItem, NavigationDrawerItemColors,
        NavigationDrawerItemDefaults, NavigationItemIconPosition, NavigationRail,
        NavigationRailItem, NavigationRailItemColors, NavigationSuite, NavigationSuiteColors,
        NavigationSuiteDefaults, NavigationSuiteItemColors, NavigationSuiteScaffold,
        NavigationSuiteScaffoldLayout, NavigationSuiteScaffoldState, NavigationSuiteScaffoldValue,
        NavigationSuiteType, PageSize, Pager, PagerLayoutInfo, PagerState, PermanentDrawerSheet,
        PermanentNavigationDrawer, ProgressIndicatorDefaults, ProgressIndicatorStrokeCap,
        ProvideTextStyle, RadioButton, RadioButtonColors, RadioButtonDefaults, RangeSlider,
        RangeThumb, RangeValue, SCAFFOLD_FAB_MARGIN, SCROLLABLE_TAB_ROW_EDGE_START_PADDING,
        SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH, SCROLLBAR_THICKNESS, SCROLLBAR_THUMB_MAX_FRACTION,
        SCROLLBAR_THUMB_MIN_LENGTH, SMALL_TAB_HEIGHT, SWIPE_DISMISS_POSITIONAL_THRESHOLD,
        SWIPE_DISMISS_VELOCITY_THRESHOLD, Scaffold, ScaffoldContentPadding, ScaffoldDefaults,
        ScaffoldFabPosition, ScrollableTabRow, ScrollableTabRowDefaults, ScrollbarDefaults,
        SegmentedButton, SegmentedButtonColors, SegmentedButtonDefaults, SelectionContainer,
        ShortNavigationBar, ShortNavigationBarArrangement, ShortNavigationBarItem,
        SingleChoiceSegmentedButtonRow, Slider, SliderColors, SliderDefaults, SnapPosition,
        Surface, SurfaceBorder, SwipeToDismissBox, SwipeToDismissBoxState, SwipeToDismissBoxValue,
        Switch, SwitchColors, SwitchDefaults, TAB_ROW_HEIGHT, TOP_APP_BAR_HEIGHT,
        TOP_APP_BAR_HORIZONTAL_PADDING, TOP_APP_BAR_LARGE_FLEXIBLE_HEIGHT,
        TOP_APP_BAR_LARGE_FLEXIBLE_SUBTITLE_HEIGHT, TOP_APP_BAR_LARGE_HEIGHT,
        TOP_APP_BAR_MEDIUM_FLEXIBLE_HEIGHT, TOP_APP_BAR_MEDIUM_FLEXIBLE_SUBTITLE_HEIGHT,
        TOP_APP_BAR_MEDIUM_HEIGHT, Tab, TabPosition, TabRow, TabRowDefaults, Text, TextField,
        TextFieldValue, Tint, TopAppBar, TopAppBarColors, TopAppBarNestedConnection,
        TopAppBarScrollBehavior, TopAppBarScrollMode, TopAppBarState, TopAppBarVariant,
        TriStateCheckbox, VerticalPager, VerticalScrollbar, WAVY_ANIMATION_DURATION_MS,
        WAVY_CIRCULAR_SIZE, WAVY_CIRCULAR_WAVELENGTH, WAVY_GAP_SIZE,
        WAVY_LINEAR_DETERMINATE_WAVELENGTH, WAVY_LINEAR_HEIGHT,
        WAVY_LINEAR_INDETERMINATE_WAVELENGTH, WAVY_LINEAR_STOP_SIZE, WAVY_LINEAR_WIDTH,
        WAVY_STROKE_WIDTH, WAVY_TRACK_STROKE_WIDTH, WIDE_RAIL_COLLAPSED_WIDTH,
        WIDE_RAIL_EXPANDED_MAX_WIDTH, WIDE_RAIL_EXPANDED_MIN_WIDTH, WavyProgressIndicatorDefaults,
        WideNavigationRail, WideNavigationRailItem, WideNavigationRailState, WindowInsets,
        modal_rail_expanded_shape, remember_navigation_suite_scaffold_state,
        top_app_bar_expanded_height,
    };
    pub use crate::graphics::{
        AxisValue, ContentScale, Dyn, IconSource, ImageAlignment, PathFillType, SymbolAxes,
    };
    pub use crate::graphics::{BlendMode, Color, ColorFilter, FilterQuality, Shape};
    pub use crate::graphics::{Brush, BrushTile};
    pub use crate::input::interaction::{ComponentState, MutableInteractionSource};
    pub use crate::input::{
        KbEvent, KbEventType, PenKind, PointerButton, PointerEvent, PointerEventType, PointerKind,
    };
    pub use crate::layout::{Dimension, SizeValue};
    pub use crate::modifier::{FocusRequester, Modifier, ScrollState};
    pub use crate::overscroll::{
        OverscrollEffect, OverscrollFactory, disable_overscroll, remember_overscroll_effect,
        with_overscroll_factory,
    };
    pub use crate::permissions::{
        Capability, PermissionAction, PermissionError, PermissionRequest, PermissionStatus,
        ServiceKind, ServicePermission,
    };
    #[cfg(target_os = "android")]
    pub use crate::platform::android::AndroidJni;
    #[cfg(target_os = "android")]
    pub use crate::runtime::composer::AndroidUi;
    pub use crate::runtime::composer::ComposeCtx;
    pub use crate::runtime::saveable::{
        Saveable, SaveableStateRegistry, SavedValue, remember_saveable, with_saveable_registry,
    };
    pub use crate::runtime::state::{
        Animating, Backchannel, DerivedFloat, DerivedValue, Reactive, State, StateChanges, StateId,
        Visual, write_batch,
    };
    pub use crate::runtime::state_list::{ListSnapshot, MapSnapshot, StateList, StateMap};
    pub use crate::text::{DecoMode, DecoStyle, FontEdge, FontHint};
    pub use crate::text::{
        FontSlant, FontWeight, TextAlign, TextDecoration, TextLayoutResult, TextOverflow, TextStyle,
    };
    // The general drawing surface (Compose `Canvas` / `DrawScope`) and the constraints-aware box
    // (Compose `BoxWithConstraints`) — the two entry points a caller reaches for when no existing
    // component expresses what they need.
    pub use crate::components::{Canvas, DrawScope, TextMetrics, draw_behind, draw_with_content};
    pub use crate::layout::{BoxWithConstraints, BoxWithConstraintsScope};
    // Accessibility declarations belong in the same scope as `Modifier` — a caller states a role or a
    // name while building the chain.
    pub use crate::animation::DeferredTargetAnimation;
    pub use crate::animation::{
        ChannelValues, ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
    };
    pub use crate::components::Tooltip;
    pub use crate::components::animated_content::AnimatedContent;
    pub use crate::components::animated_size::AnimatedSize;
    pub use crate::components::animated_visibility::{AnimatedVisibility, VisibilityScope};
    pub use crate::components::bottom_sheet::ModalBottomSheet;
    pub use crate::components::bottom_sheet_scaffold::{
        BottomSheetScaffold, SCAFFOLD_SHEET_PEEK_HEIGHT,
    };
    pub use crate::components::crossfade::Crossfade;
    pub use crate::components::rich_text::RichText;
    pub use crate::components::search_bar::{
        BACK_ICON_PATH, DockedSearchBar, SEARCH_BAR_HEIGHT, SEARCH_ICON_PATH, SearchBar,
        SearchBarColors, SearchBarDefaults, SearchBarState,
    };
    pub use crate::components::shared_transition::{
        SharedContentState, SharedTransitionDefaults, SharedTransitionLayout,
        SharedTransitionScope, current_shared_scope,
    };
    pub use crate::components::sheet_state::{SheetState, SheetValue};
    pub use crate::components::snackbar::{
        Snackbar, SnackbarData, SnackbarDuration, SnackbarHost, SnackbarHostState,
    };
    pub use crate::components::split_button::{
        LeadingButton, SplitButtonDefaults, SplitButtonLayout, SplitButtonShapes, TrailingButton,
    };
    pub use crate::components::{PullToRefreshBox, PullToRefreshState};
    pub use crate::selection::ToggleableState;
    pub use crate::semantics::{SemanticsConfig, SemanticsRole, SemanticsState};
    pub use crate::transition::{
        BoundsTransform, OverlayClip, PathMotion, PlaceHolderSize, ResizeMode, SharedBounds,
        SharedKind,
    };
    pub use crate::{app_root, composable, composable_keyed, compose, keyed_stmt, run_app};
    // The overlay runtime: a popup and a dialog.
    pub use crate::overlay::{Dialog, OverlayAnimSpec, Popup, PopupPosition};
    // …and the menu built on it, which is a component.
    pub use crate::animation::{
        Animatable, DecaySpec, animate_int_as_state, animate_value_as_state, cancel_animation,
        exponential_decay, push_decay, push_decay_vector,
    };
    pub use crate::app::window::Window;
    pub use crate::components::dropdown_menu::{
        DropdownMenu, DropdownMenuItem, ExposedDropdownMenuAnchorType, ExposedDropdownMenuBox,
        ExposedDropdownMenuDefaults, MenuDefaults, MenuItemColors,
    };
    pub use crate::effect::{
        CoroutineScope, DisposableEffect, FrameClock, LaunchedEffect, observe_watch, on_forget,
        produce_state, remember_coroutine_scope, remember_updated_state, side_effect,
        with_frame_nanos,
    };
    pub use crate::fling::{FlingBehavior, FlingContext, FlingPlan, SnapFlingBehavior, SnapInfo};
    pub use crate::layout::{
        AbsoluteArrangement, Alignment, Arrangement, Constraints, ContentAlignment, LayoutDirection,
    };
    pub use crate::layout::{
        Column, FlowColumn, FlowOverflow, FlowRow, Layout, Row, Spacer, Stack,
    };
    pub use crate::layout::{
        FlexAlignContent, FlexAlignItems, FlexAlignSelf, FlexBasis, FlexBox, FlexBoxConfig,
        FlexDirection, FlexItemConfig, FlexJustifyContent, FlexWrap,
    };
    pub use crate::layout::{Grid, GridArea, GridFlow, GridTrackSpec};
    pub use crate::layout::{
        GridCells, ItemHeightCache, LazyColumn, LazyHorizontalGrid, LazyHorizontalStaggeredGrid,
        LazyListState, LazyRow, LazyVerticalGrid, LazyVerticalStaggeredGrid,
    };
    pub use crate::layout::{HeightSizeClass, WidthSizeClass, set_window_size, window_size};
    pub use crate::overlay::anchored_draggable::{AnchoredDraggableState, DraggableAnchors};
    pub use crate::runtime::flow::{SnapshotFlow, debounce, remember_derived_state, snapshot_flow};
    pub use crate::text::{
        IdentityTransformation, OffsetMapping, PasswordTransformation, VisualTransformation,
    };
    pub use crate::text::{ImageDrawable, InlineDrawable, SvgDrawable};
    pub use crate::theme::{
        MotionScheme, ThemeColors, ThemeSpec, Typography, WiniaTheme, is_system_dark_theme,
        set_system_dark_mode,
    };
    pub use crate::unit::{
        Density, Dp, DpExt, DpOffset, IntOffset, IntSize, Offset, Px, PxExt, Rect, Size, Sp, SpExt,
    };
    pub use crate::window::insets::{Insets, InsetsKind, InsetsVisibility, WindowInsetsController};
    pub use std::time::Duration;
}
