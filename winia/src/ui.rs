//! UI 组件 — composable 函数集合
//!
//! 每个组件都是一个 Builder 结构体 + `build(ctx)` 方法，
//! 注册到 Composer 的组合树中。

pub mod text;
pub mod button;
pub mod card;
pub mod layout_components;
pub mod window;
pub mod theme;
pub mod rich_text;
pub mod selection_container;
pub mod text_field;
pub mod text_transformation;
pub mod animated_visibility;
pub mod animated_size;
pub mod animated_content;
pub mod crossfade;
pub mod overlay;
pub mod interaction;
pub mod icon;
pub mod image;
pub mod icon_button;
pub mod icon_toggle_button;
pub mod checkbox;
pub mod switch;
pub mod chip;
pub mod tooltip;
pub mod radio_button;
pub mod badge;
pub mod slider;
pub mod scrollbar;
pub mod progress_indicator;
pub mod loading_indicator;
pub mod wavy_progress_indicator;
pub mod floating_action_button;
pub mod divider;
pub mod lazy_column;
pub mod list_item;
pub mod top_app_bar;
pub mod scaffold;
pub mod snackbar;
pub mod bottom_sheet;
pub mod bottom_sheet_scaffold;
pub mod anchored_draggable;
pub mod sheet_state;
pub mod surface;
pub mod navigation_bar;
pub mod navigation_rail;
pub mod tab_row;
pub mod navigation_suite;
pub mod short_navigation_bar;

pub use animated_visibility::{
    AnimatedVisibility, ExpandFrom, ExpandFromH, SlideDirection, SlideOffset, VisibilityTransition,
};
pub use animated_size::AnimatedSize;
pub use animated_content::AnimatedContent;
pub use crossfade::Crossfade;
pub use overlay::{Popup, Dialog, DropdownMenu, DropdownMenuItem, PopupPosition, OverlayAnimSpec};

pub use text::Text;
pub use text::TextAlign;
pub use text::TextOverflow;
pub use text::TextStyle;
pub use text::ProvideTextStyle;
pub use text::FontWeight;
pub use text::FontSlant;
pub use button::Button;
pub use button::ButtonBorder;
pub use button::ButtonColors;
pub use button::ButtonDefaults;
pub use button::ButtonElevation;
pub use button::ButtonSize;
pub use button::ButtonStyle;
pub use card::{Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle};
pub use surface::{Surface, SurfaceBorder};
pub use layout_components::{Column, Row, Stack, Spacer, FlowRow, FlowColumn};
pub use window::Window;
pub use theme::WiniaTheme;
pub use theme::ThemeColors;
pub use theme::Typography;
pub use theme::is_system_dark_theme;
pub use rich_text::RichText;
pub use rich_text::RichTextScope;
pub use selection_container::SelectionContainer;
pub use selection_container::SelectionRegistrar;
pub use selection_container::LOCAL_SELECTION_REGISTRAR;
pub use text_field::{TextField, TextFieldValue, TextFieldVariant, TextFieldColors, TextChange};
pub use text_transformation::{VisualTransformation, OffsetMapping, TransformedText, IdentityTransformation, PasswordTransformation};
pub use interaction::{MutableInteractionSource, ComponentState};
pub use icon::{AxisValue, Icon, IconSource, PathFillType, Tint};
pub use image::{ContentScale, Image, ImageAlignment};
pub use icon_button::{IconButton, IconButtonColors, IconButtonDefaults, IconButtonSize, IconButtonStyle};
pub use icon_toggle_button::{IconToggleButton, IconToggleButtonColors, IconToggleButtonDefaults};
pub use checkbox::{Checkbox, CheckboxColors, CheckboxDefaults, TriStateCheckbox, ToggleableState};
pub use switch::{Switch, SwitchColors, SwitchDefaults};
pub use radio_button::{RadioButton, RadioButtonColors, RadioButtonDefaults};
pub use badge::{Badge, BadgedBox};
pub use slider::{Slider, SliderColors, SliderDefaults};
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
pub use lazy_column::{ItemHeightCache, LazyColumn, LazyListState, LazyRow};
pub use list_item::{ListItem, ListItemColors, ListItemDefaults, LIST_ITEM_ONE_LINE_HEIGHT, LIST_ITEM_TWO_LINE_HEIGHT, LIST_ITEM_THREE_LINE_HEIGHT, LIST_ITEM_HORIZONTAL_PADDING, LIST_ITEM_VERTICAL_PADDING, LIST_ITEM_SLOT_GAP, LIST_ITEM_CONTENT_GAP};
pub use top_app_bar::{TopAppBar, TopAppBarColors, TopAppBarScrollBehavior, TopAppBarState, TopAppBarNestedConnection, TopAppBarScrollMode, TopAppBarVariant, TOP_APP_BAR_HEIGHT, TOP_APP_BAR_MEDIUM_HEIGHT, TOP_APP_BAR_LARGE_HEIGHT, TOP_APP_BAR_HORIZONTAL_PADDING};
pub use scaffold::{Scaffold, ScaffoldContentPadding, ScaffoldFabPosition, SCAFFOLD_FAB_MARGIN};
pub use snackbar::{Snackbar, SnackbarData, SnackbarDuration, SnackbarHost, SnackbarHostState};
pub use bottom_sheet::ModalBottomSheet;
pub use bottom_sheet_scaffold::{BottomSheetScaffold, SCAFFOLD_SHEET_PEEK_HEIGHT};
pub use anchored_draggable::{AnchoredDraggableState, DraggableAnchors};
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
pub mod adaptive;
pub use adaptive::{set_window_size, window_size, WidthSizeClass, HeightSizeClass};
pub use navigation_rail::{
    NavigationRail, NavigationRailItem, NavigationRailItemColors, WideNavigationRail,
    WideNavigationRailItem, WideNavigationRailState, WindowInsets,
    NAVIGATION_RAIL_INDICATOR_HEIGHT, NAVIGATION_RAIL_INDICATOR_WIDTH, NAVIGATION_RAIL_ITEM_HEIGHT,
    NAVIGATION_RAIL_ICON_SIZE, NAVIGATION_RAIL_WIDTH, WIDE_RAIL_COLLAPSED_WIDTH,
    WIDE_RAIL_EXPANDED_MIN_WIDTH,
};
pub use tooltip::Tooltip;
