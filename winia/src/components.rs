//! UI 组件 — composable 函数集合
//!
//! 每个组件都是一个 Builder 结构体 + `build(ctx)` 方法，
//! 注册到 Composer 的组合树中。

pub mod text;
pub mod button;
pub mod card;
pub mod rich_text;
pub mod selection_container;
pub mod text_field;
pub mod animated_visibility;
pub mod animated_size;
pub mod animated_content;
pub mod crossfade;
pub mod alert_dialog;
pub mod icon;
pub mod image;
pub mod icon_button;
pub mod icon_toggle_button;
pub mod checkbox;
pub mod switch;
pub mod chip;
pub mod tooltip;
pub mod shared_transition;
pub mod radio_button;
pub mod badge;
pub mod slider;
pub mod range_slider;
pub mod segmented_button;
pub mod scrollbar;
pub mod progress_indicator;
pub mod loading_indicator;
pub mod wavy_progress_indicator;
pub mod floating_action_button;
pub mod divider;
pub mod list_item;
pub mod top_app_bar;
pub mod scaffold;
pub mod snackbar;
pub mod bottom_sheet;
pub mod bottom_sheet_scaffold;
pub mod sheet_state;
pub mod surface;
pub mod navigation_bar;
pub mod navigation_rail;
pub mod navigation_drawer;
pub mod swipe_to_dismiss;
pub mod tab_row;
pub mod split_button;
pub mod date_picker;
pub mod navigation_suite;
pub mod short_navigation_bar;
pub mod search_bar;
pub mod draw_scope;

pub use draw_scope::{draw_behind, draw_with_content, Canvas, DrawScope, TextMetrics};

pub use animated_visibility::{
    AnimatedVisibility, ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
};
pub use animated_size::AnimatedSize;
pub use animated_content::AnimatedContent;
pub use crossfade::Crossfade;

pub use text::Text;
pub use text::ProvideTextStyle;
pub use button::Button;
pub use button::ButtonBorder;
pub use button::ButtonColors;
pub use button::ButtonDefaults;
pub use button::ButtonElevation;
pub use button::ButtonSize;
pub use button::ButtonStyle;
pub use split_button::{
    LeadingButton, SplitButtonDefaults, SplitButtonLayout, SplitButtonShapes, TrailingButton,
};
pub use card::{Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle};
pub use surface::{Surface, SurfaceBorder};
pub use rich_text::RichText;
pub use rich_text::RichTextScope;
pub use selection_container::SelectionContainer;
pub use text_field::{TextField, TextFieldValue, TextFieldVariant, TextFieldColors, TextChange};
pub use icon::{Icon, Tint};
pub use image::Image;
pub use icon_button::{IconButton, IconButtonColors, IconButtonDefaults, IconButtonSize, IconButtonStyle};
pub use icon_toggle_button::{IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults};
pub use checkbox::{Checkbox, CheckboxColors, CheckboxDefaults, TriStateCheckbox};
pub use switch::{Switch, SwitchColors, SwitchDefaults};
pub use radio_button::{RadioButton, RadioButtonColors, RadioButtonDefaults};
pub use badge::{Badge, BadgedBox};
pub use slider::{Slider, SliderColors, SliderDefaults};
pub use range_slider::{RangeSlider, RangeThumb, RangeValue};
pub use segmented_button::{
    CHECK_ICON_PATH, MultiChoiceSegmentedButtonRow, SegmentedButton, SegmentedButtonColors,
    SegmentedButtonDefaults, SingleChoiceSegmentedButtonRow,
};
pub use scrollbar::{
    VerticalScrollbar, HorizontalScrollbar, LazyScrollbar, HorizontalLazyScrollbar,
    SCROLLBAR_THICKNESS, SCROLLBAR_THUMB_MIN_LENGTH, SCROLLBAR_THUMB_MAX_FRACTION,
};
pub use progress_indicator::{
    LinearProgressIndicator, CircularProgressIndicator,
    ProgressIndicatorDefaults, ProgressIndicatorStrokeCap,
    LINEAR_INDICATOR_WIDTH, LINEAR_INDICATOR_HEIGHT, LINEAR_STOP_SIZE,
    STOP_INDICATOR_TRAILING_SPACE, CIRCULAR_INDICATOR_DIAMETER,
    CIRCULAR_STROKE_WIDTH, TRACK_ACTIVE_SPACE,
};
pub use loading_indicator::{
    LoadingIndicator,
    LOADING_INDICATOR_SIZE, LOADING_INDICATOR_ACTIVE_SIZE,
    LOADING_INDICATOR_CONTAINER_SHAPE,
};
pub use wavy_progress_indicator::{
    LinearWavyProgressIndicator, CircularWavyProgressIndicator,
    WavyProgressIndicatorDefaults,
    WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT, WAVY_CIRCULAR_SIZE,
    WAVY_STROKE_WIDTH, WAVY_TRACK_STROKE_WIDTH, WAVY_GAP_SIZE,
    WAVY_LINEAR_STOP_SIZE, WAVY_LINEAR_DETERMINATE_WAVELENGTH,
    WAVY_LINEAR_INDETERMINATE_WAVELENGTH, WAVY_CIRCULAR_WAVELENGTH,
    WAVY_ANIMATION_DURATION_MS,
};
pub use floating_action_button::{
    ExtendedFloatingActionButton, EXTENDED_FAB_COLLAPSED_WIDTH, EXTENDED_FAB_HEIGHT, EXTENDED_FAB_MIN_EXPANDED_WIDTH,
    FloatingActionButton, FloatingActionButtonColors, FloatingActionButtonDefaults,
    FloatingActionButtonElevation, FloatingActionButtonSize,
    FAB_SMALL_SIZE, FAB_REGULAR_SIZE, FAB_MEDIUM_SIZE, FAB_LARGE_SIZE,
    FAB_ICON_SIZE, FAB_MEDIUM_ICON_SIZE, FAB_LARGE_ICON_SIZE,
};
pub use chip::{Chip, ChipVariant, ChipColors, SelectableChipColors, ChipDefaults};
pub use divider::{Divider, DividerDefaults, DIVIDER_THICKNESS, DIVIDER_HAIRLINE};
pub use list_item::{ListItem, ListItemColors, ListItemDefaults, LIST_ITEM_ONE_LINE_HEIGHT, LIST_ITEM_TWO_LINE_HEIGHT, LIST_ITEM_THREE_LINE_HEIGHT, LIST_ITEM_HORIZONTAL_PADDING, LIST_ITEM_VERTICAL_PADDING, LIST_ITEM_SLOT_GAP, LIST_ITEM_CONTENT_GAP};
pub use top_app_bar::{TopAppBar, TopAppBarColors, TopAppBarScrollBehavior, TopAppBarState, TopAppBarNestedConnection, TopAppBarScrollMode, TopAppBarVariant, TOP_APP_BAR_HEIGHT, TOP_APP_BAR_MEDIUM_HEIGHT, TOP_APP_BAR_LARGE_HEIGHT, TOP_APP_BAR_HORIZONTAL_PADDING};
pub use scaffold::{Scaffold, ScaffoldContentPadding, ScaffoldFabPosition, SCAFFOLD_FAB_MARGIN};
pub use snackbar::{Snackbar, SnackbarData, SnackbarDuration, SnackbarHost, SnackbarHostState};
pub use bottom_sheet::ModalBottomSheet;
pub use bottom_sheet_scaffold::{BottomSheetScaffold, SCAFFOLD_SHEET_PEEK_HEIGHT};
pub use sheet_state::{SheetState, SheetValue};
pub use navigation_bar::{
    NavigationBar, NavigationBarItem, NavigationBarColors, NavigationBarItemColors,
    NavigationItemIconPosition, NavigationBarDefaults, NAVIGATION_BAR_HEIGHT,
    NAVIGATION_BAR_ITEM_SPACING, NAVIGATION_BAR_H_INDICATOR_HEIGHT,
    NAVIGATION_BAR_INDICATOR_WIDTH, NAVIGATION_BAR_INDICATOR_HEIGHT, NAVIGATION_BAR_ICON_SIZE,
};
pub use short_navigation_bar::{
    ShortNavigationBar, ShortNavigationBarItem, ShortNavigationBarArrangement,
};
pub use navigation_suite::{
    NavigationSuiteScaffold, NavigationSuiteType, NavigationSuiteItems,
};
pub use tab_row::{
    Tab, TabRow, TabRowDefaults, TabPosition,
    ScrollableTabRow, ScrollableTabRowDefaults,
    TAB_ROW_HEIGHT, ACTIVE_INDICATOR_HEIGHT, HORIZONTAL_TEXT_PADDING,
    MIN_INDICATOR_WIDTH, LARGE_TAB_HEIGHT, SMALL_TAB_HEIGHT,
    SCROLLABLE_TAB_ROW_MIN_TAB_WIDTH, SCROLLABLE_TAB_ROW_EDGE_START_PADDING,
};
pub use navigation_rail::{
    NavigationRail, NavigationRailItem, NavigationRailItemColors, WideNavigationRail,
    WideNavigationRailItem, WideNavigationRailState, WindowInsets,
    NAVIGATION_RAIL_INDICATOR_HEIGHT, NAVIGATION_RAIL_INDICATOR_WIDTH, NAVIGATION_RAIL_ITEM_HEIGHT,
    NAVIGATION_RAIL_ICON_SIZE, NAVIGATION_RAIL_WIDTH, WIDE_RAIL_COLLAPSED_WIDTH,
    WIDE_RAIL_EXPANDED_MIN_WIDTH,
};
pub use tooltip::Tooltip;
pub use alert_dialog::{
    AlertDialog, AlertDialogDefaults, BasicAlertDialog,
    DIALOG_BUTTON_SPACING, DIALOG_CONTAINER_PADDING, DIALOG_CORNER_RADIUS, DIALOG_ICON_PADDING_BOTTOM,
    DIALOG_ICON_SIZE, DIALOG_MAX_WIDTH, DIALOG_MIN_WIDTH, DIALOG_TEXT_PADDING_BOTTOM,
    DIALOG_TITLE_PADDING_BOTTOM,
};
pub use navigation_drawer::{
    DrawerDefaults, DrawerState, DrawerValue, ModalDrawerSheet, ModalNavigationDrawer,
    NavigationDrawerItem, NavigationDrawerItemColors,
    DRAWER_CORNER_RADIUS, DRAWER_ITEM_END_PADDING, DRAWER_ITEM_HEIGHT, DRAWER_ITEM_ICON_SIZE,
    DRAWER_ITEM_SLOT_GAP, DRAWER_ITEM_START_PADDING, DRAWER_MAX_WIDTH, DRAWER_MIN_WIDTH,
    DRAWER_SHEET_HORIZONTAL_PADDING,
};
pub use swipe_to_dismiss::{
    SwipeToDismissBox, SwipeToDismissBoxState, SwipeToDismissBoxValue,
    SWIPE_DISMISS_POSITIONAL_THRESHOLD, SWIPE_DISMISS_VELOCITY_THRESHOLD,
};
pub use shared_transition::{SharedContentState, SharedTransitionDefaults, SharedTransitionLayout, SharedTransitionScope};
pub use crate::transition::{BoundsTransform, OverlayClip, PathMotion, PlaceHolderSize, ResizeMode, SharedBounds, SharedKind};
pub use search_bar::{SearchBar, SearchBarColors, SearchBarDefaults, SearchBarState, DockedSearchBar, SEARCH_ICON_PATH, BACK_ICON_PATH, SEARCH_BAR_HEIGHT};
