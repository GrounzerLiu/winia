//! UI 组件 — composable 函数集合
//!
//! 每个组件都是一个 Builder 结构体 + `build(ctx)` 方法，
//! 注册到 Composer 的组合树中。

pub mod text;
pub mod button;
pub mod button_group;
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
pub mod dropdown_menu;
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
pub mod pager;

pub use pager::{HorizontalPager, Pager, PagerState, VerticalPager};

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
pub use button_group::{ButtonGroup, ButtonGroupDefaults, ButtonGroupItem, ButtonGroupMenuState, ButtonGroupShapes, BUTTON_GROUP_HEIGHT, BUTTON_GROUP_COMPRESSION_LIMIT, BUTTON_GROUP_BETWEEN_SPACE, CONNECTED_BUTTON_GROUP_BETWEEN_SPACE, BUTTON_GROUP_EXPANDED_RATIO, CONNECTED_INNER_CORNER_SIZE, CONNECTED_PRESSED_INNER_CORNER_SIZE};
pub use split_button::{
    LeadingButton, SplitButtonDefaults, SplitButtonLayout, SplitButtonShapes, TrailingButton,
};
pub use card::{Card, CardBorder, CardColors, CardDefaults, CardElevation, CardStyle};
pub use surface::{Surface, SurfaceBorder};
pub use rich_text::RichText;
pub use rich_text::RichTextScope;
pub use selection_container::SelectionContainer;
pub use text_field::{TextField, TextFieldValue, TextChange};
pub use crate::text::field::{TextFieldColors, TextFieldSlotRole, TextFieldVariant};
pub use icon::{Icon, Tint};
pub use image::Image;
pub use icon_button::{IconButton, IconButtonColors, IconButtonDefaults, IconButtonShape, IconButtonSize, IconButtonStyle, IconButtonWidthOption};
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
    ExtendedFloatingActionButton, ExtendedFabSize, EXTENDED_FAB_COLLAPSED_WIDTH, EXTENDED_FAB_HEIGHT, EXTENDED_FAB_MIN_EXPANDED_WIDTH,
    FloatingActionButton, FloatingActionButtonColors, FloatingActionButtonDefaults,
    FloatingActionButtonElevation, FloatingActionButtonSize,
    FAB_SMALL_SIZE, FAB_REGULAR_SIZE, FAB_MEDIUM_SIZE, FAB_LARGE_SIZE,
    FAB_ICON_SIZE, FAB_MEDIUM_ICON_SIZE, FAB_LARGE_ICON_SIZE,
};
pub use chip::{Chip, ChipColors, ChipDefaults, ChipElevation, ChipVariant, SelectableChipColors};
pub use divider::{Divider, DividerDefaults, DIVIDER_THICKNESS, DIVIDER_HAIRLINE};
pub use list_item::{ListItem, ListItemColors, ListItemDefaults, LIST_ITEM_ONE_LINE_HEIGHT, LIST_ITEM_TWO_LINE_HEIGHT, LIST_ITEM_THREE_LINE_HEIGHT, LIST_ITEM_HORIZONTAL_PADDING, LIST_ITEM_VERTICAL_PADDING, LIST_ITEM_SLOT_GAP, LIST_ITEM_CONTENT_GAP};
pub use top_app_bar::{TopAppBar, TopAppBarColors, TopAppBarScrollBehavior, TopAppBarState, TopAppBarNestedConnection, TopAppBarScrollMode, TopAppBarVariant, expanded_height as top_app_bar_expanded_height, TOP_APP_BAR_HEIGHT, TOP_APP_BAR_MEDIUM_HEIGHT, TOP_APP_BAR_LARGE_HEIGHT, TOP_APP_BAR_HORIZONTAL_PADDING, TOP_APP_BAR_MEDIUM_FLEXIBLE_HEIGHT, TOP_APP_BAR_MEDIUM_FLEXIBLE_SUBTITLE_HEIGHT, TOP_APP_BAR_LARGE_FLEXIBLE_HEIGHT, TOP_APP_BAR_LARGE_FLEXIBLE_SUBTITLE_HEIGHT};
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
    NavigationSuiteColors, NavigationSuiteDefaults, NavigationSuiteItemColors,
    NavigationSuite, NavigationSuiteScaffold, NavigationSuiteScaffoldLayout, NavigationSuiteScaffoldState,
    NavigationSuiteScaffoldValue,
    NavigationSuiteType, NavigationSuiteItems,
    remember_navigation_suite_scaffold_state,
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
    ModalWideNavigationRail, modal_rail_expanded_shape,
    WideNavigationRailItem, WideNavigationRailState, WindowInsets,
    NAVIGATION_RAIL_INDICATOR_HEIGHT, NAVIGATION_RAIL_INDICATOR_WIDTH, NAVIGATION_RAIL_ITEM_HEIGHT,
    NAVIGATION_RAIL_ICON_SIZE, NAVIGATION_RAIL_WIDTH, WIDE_RAIL_COLLAPSED_WIDTH,
    WIDE_RAIL_EXPANDED_MIN_WIDTH, WIDE_RAIL_EXPANDED_MAX_WIDTH,
};
pub use tooltip::Tooltip;
pub use alert_dialog::{
    AlertDialog, AlertDialogDefaults, BasicAlertDialog,
    DIALOG_BUTTON_SPACING, DIALOG_CONTAINER_PADDING, DIALOG_CORNER_RADIUS, DIALOG_ICON_PADDING_BOTTOM,
    DIALOG_ICON_SIZE, DIALOG_MAX_WIDTH, DIALOG_MIN_WIDTH, DIALOG_TEXT_PADDING_BOTTOM,
    DIALOG_TITLE_PADDING_BOTTOM,
};
pub use navigation_drawer::{
    DismissibleDrawerSheet, DismissibleNavigationDrawer, DrawerDefaults, DrawerSheet,
    DrawerSheetKind, DrawerState, DrawerValue, ModalDrawerSheet, ModalNavigationDrawer,
    NavigationDrawerItem, NavigationDrawerItemColors, NavigationDrawerItemDefaults,
    PermanentDrawerSheet, PermanentNavigationDrawer,
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
#[cfg(test)]
mod conventions {
    //! Source-level checks for the rules in `docs/component-authoring.md`.
    //!
    //! Not every rule can be checked by reading text, but the three below are the ones that fail
    //! silently and are mechanical enough to enforce instead of hoping a reviewer remembers:
    //!
    //! 1. a container that compares parameters declares them (`ctx.changed`), 2. a `remember` does not
    //!    nest a `State` inside a `State`, 3. a component does not spawn its own task.

    use std::path::PathBuf;

    /// Containers that compose a restartable group WITHOUT parameter comparison, because their
    /// content is driven by something else (an animation tick, an inner composition that reads the
    /// state itself). Adding a file here is a deliberate decision - say why in the same diff.
    const CONTENT_DRIVEN_WITHOUT_CHANGED: &[&str] = &[
        "animated_size.rs",        // re-runs every animation tick
        "animated_visibility.rs",  // same
        "chip.rs",                 // look is baked into the modifier; state is read inside the group
        "dropdown_menu.rs",        // anchor content reads the menu state itself (see its comment)
        "selection_container.rs",  // selection registry reads
        "tooltip.rs",              // anchored content, no parameter of its own to compare
    ];

    fn component_sources() -> Vec<(String, String)> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join("components");
        let mut out = Vec::new();
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            out.push((name, text));
        }
        assert!(out.len() > 40, "the component directory moved: {} files", out.len());
        out
    }

    #[test]
    fn every_container_declares_its_parameters() {
        let mut offenders = Vec::new();
        for (name, text) in component_sources() {
            if !text.contains("start_restartable_group") {
                continue;
            }
            if text.contains("ctx.changed(") || CONTENT_DRIVEN_WITHOUT_CHANGED.contains(&name.as_str()) {
                continue;
            }
            offenders.push(name);
        }
        assert!(
            offenders.is_empty(),
            "these containers compose a restartable group without declaring a single parameter via ctx.changed: {offenders:?}. A parameter the group does not compare is a change the Clean slot skips past forever (docs/component-authoring.md section 3.2). Either declare the parameters, or add the file to CONTENT_DRIVEN_WITHOUT_CHANGED with the reason."
        );
    }

    #[test]
    fn no_component_nests_a_state_in_remember() {
        let mut offenders = Vec::new();
        for (name, text) in component_sources() {
            for (line_no, line) in text.lines().enumerate() {
                if !line.contains("remember(") {
                    continue;
                }
                // The bare type name matters: `SheetState::new()` is a handle, not a nested `State`.
                let nested = line.contains("|| State::new(")
                    || line.contains("||State::new(")
                    || line.contains("|| Animating::new(")
                    || line.contains("||Animating::new(");
                if nested {
                    offenders.push(format!("{name}:{}", line_no + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these lines nest a handle inside a remember: {offenders:?}. `ctx.remember(|| value)` already wraps the value - see docs/state-handles.md section 7.4."
        );
    }

    #[test]
    fn no_component_spawns_its_own_task() {
        let mut offenders = Vec::new();
        for (name, text) in component_sources() {
            for (line_no, line) in text.lines().enumerate() {
                // `scope.spawn` is the sanctioned form (it is cancelled with the composition point);
                // a bare runtime spawn is not.
                let runtime_spawn = line.contains("rt.spawn(") || line.contains("tokio::spawn(");
                if runtime_spawn && !line.trim_start().starts_with("//") {
                    offenders.push(format!("{name}:{}", line_no + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these lines spawn a task: {offenders:?}. Spawn through an effect (LaunchedEffect / produce_state / a scope), which cancels the task with the composition point - docs/component-authoring.md section 4.4."
        );
    }
}
