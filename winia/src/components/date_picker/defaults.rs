use super::*;

// ─────────────────────────────────────────────────────────────────────────
// Defaults, colours and the docked picker
// ─────────────────────────────────────────────────────────────────────────

/// The material3 measurements the date pickers are built from, named after `DatePickerModalTokens` and the
/// constants beside `DatePicker` (`DatePicker.kt:2293-2304`). `docs/date-picker.md` carries every value and its
/// anchor.
pub struct DatePickerDefaults;

impl DatePickerDefaults {
    /// `DatePickerDefaults.YearRange` (`DatePicker.kt:764`).
    pub fn year_range() -> RangeInclusive<i32> {
        DEFAULT_YEAR_RANGE
    }

    /// The default title while the picker shows a calendar (`DatePicker.kt:654`). material3's wording comes from
    /// resources this checkout does not carry, so winia supplies its own English.
    pub const TITLE: &'static str = "Select date";

    /// The default title while the picker shows the entry field (`DatePicker.kt:654`, `DateInputTitle`).
    /// Compose names the mode from the title as well as the headline, so a field that asks for a
    /// date is not headed "Select date".
    pub const INPUT_TITLE: &'static str = "Enter date";

    /// The headline while nothing is selected (`DatePicker.kt:704`).
    pub const HEADLINE: &'static str = "No date selected";

    /// What the headline announces for a selection, given the verbose date
    /// (`m3c_date_picker_headline_description`).
    pub const HEADLINE_DESCRIPTION: &'static str = "Current selection: {1}";

    /// What the headline announces when nothing is selected, in either mode
    /// (`m3c_date_picker_no_selection_description`, `m3c_date_input_no_input_description`).
    pub const NO_SELECTION_DESCRIPTION: &'static str = "None";

    /// `DatePickerModalTokens.ContainerWidth`: the container's minimum width.
    pub const CONTAINER_WIDTH: f32 = 360.0;

    /// `DatePickerModalTokens.ContainerHeight`: the modal dialog's maximum height.
    pub const CONTAINER_HEIGHT: f32 = 568.0;

    /// `DatePickerModalTokens.ContainerShape`, `CornerExtraLarge`.
    pub const CONTAINER_CORNER: f32 = 28.0;

    /// `DatePickerModalTokens.HeaderContainerHeight`, applied only when a title is present
    /// (`DatePicker.kt:1680-1685`).
    pub const HEADER_MIN_HEIGHT: f32 = 120.0;

    /// `DatePickerModalTokens.DateContainerWidth` and `…Height`: the painted day.
    pub const DAY_CELL: f32 = 40.0;

    /// `RecommendedSizeForAccessibility` (`DatePicker.kt:2293`): a grid row, a grid column and a weekday
    /// label.
    pub const ACCESSIBLE_SIZE: f32 = 48.0;

    /// `MonthYearHeight` (`DatePicker.kt:2294`): the month navigation row.
    pub const MONTH_YEAR_HEIGHT: f32 = 56.0;

    /// `DatePickerHorizontalPadding` (`DatePicker.kt:2295`).
    pub const HORIZONTAL_PADDING: f32 = 12.0;

    /// `DatePickerTitlePadding`'s start (`DatePicker.kt:2298`).
    pub const TITLE_START_PADDING: f32 = 24.0;

    /// `DatePickerTitlePadding`'s end.
    pub const TITLE_END_PADDING: f32 = 12.0;

    /// `DatePickerTitlePadding`'s top.
    pub const TITLE_TOP_PADDING: f32 = 16.0;

    /// `DatePickerHeadlinePadding`'s bottom (`DatePicker.kt:2299`).
    pub const HEADLINE_BOTTOM_PADDING: f32 = 12.0;

    /// The header's height when there is no title: material3 applies no minimum then, so the header is the
    /// headline's row — one `HeadlineLarge` line plus `HEADLINE_BOTTOM_PADDING`.
    pub const HEADLINE_ROW_HEIGHT: f32 = 44.0;

    /// `DatePickerModeTogglePadding` (`DatePicker.kt:2296`).
    pub const MODE_TOGGLE_PADDING: f32 = 12.0;

    /// `DatePickerModalTokens.DateTodayContainerOutlineWidth`.
    pub const TODAY_OUTLINE_WIDTH: f32 = 1.0;

    /// `DisabledAlpha` (`ColorScheme.kt:1518`): every disabled colour role carries it.
    pub const DISABLED_ALPHA: f32 = 0.38;

    /// The height a month reserves, `RecommendedSizeForAccessibility * MaxCalendarRows`
    /// (`DatePicker.kt:1859`).
    pub const MONTH_HEIGHT: f32 = Self::ACCESSIBLE_SIZE * MAX_CALENDAR_ROWS as f32;

    /// `YearsInRow` (`DatePicker.kt:2304`): the year panel's columns.
    pub const YEARS_PER_ROW: usize = 3;

    /// `DatePickerModalTokens.SelectionYearContainerWidth`: a year cell's width.
    pub const YEAR_CELL_WIDTH: f32 = 72.0;

    /// A month pill's width in the DOCKED month list: a year cell (72 dp) widened for the longest English
    /// month name. NOT a material3 token — material3 has no docked month list to tokenize — so this is a
    /// winia measurement: three 104 dp pills plus [`Self::CELL_SPACING`] fill the 336 dp content width
    /// exactly, and at 72 dp "September" renders as "Septemb" (measured). It lives here only because the
    /// docked picker's sizing has no defaults object of its own yet.
    pub const MONTH_CELL_WIDTH: f32 = 104.0;

    /// `DatePickerModalTokens.SelectionYearContainerHeight`: a year cell's height.
    pub const YEAR_CELL_HEIGHT: f32 = 36.0;

    /// `YearsVerticalPadding` (`DatePicker.kt:2301`): between the year panel's rows.
    pub const YEARS_VERTICAL_PADDING: f32 = 16.0;

    /// The gap between the docked month list's cells, on both axes. NOT a material3 token, like
    /// [`Self::MONTH_CELL_WIDTH`] — it is the measurement that keeps three of those pills even across the
    /// 336 dp content width, and matching the two axes is what keeps the grid even (measured:
    /// `SpaceEvenly` + a vertical `spacing` gave 8 dp across and 16 dp down).
    pub const CELL_SPACING: f32 = 12.0;

    /// `ButtonSmallTokens.IconLabelSpace` (`ButtonDefaults.IconSpacing`): between the year menu button's text
    /// and its dropdown arrow.
    pub const YEAR_MENU_ICON_SPACING: f32 = 8.0;

    /// `ButtonSmallTokens.ContainerHeight` (`ButtonDefaults.MinHeight`): the year menu button's height.
    pub const YEAR_MENU_BUTTON_HEIGHT: f32 = 40.0;

    /// `DividerDefaults.Thickness`, which the year panel takes off its height (`DatePicker.kt:1639`).
    pub const DIVIDER_THICKNESS: f32 = 1.0;

    /// The year panel's height, `RecommendedSizeForAccessibility * (MaxCalendarRows + 1)` less the divider that
    /// closes it (`DatePicker.kt:1634-1641`). It equals the weekday row plus the month grid — the two things it
    /// stands in for — so opening the panel moves nothing above or below it.
    pub const YEAR_PANEL_HEIGHT: f32 =
        Self::ACCESSIBLE_SIZE * (MAX_CALENDAR_ROWS as f32 + 1.0) - Self::DIVIDER_THICKNESS;

    /// `DatePickerModalTokens.ContainerHeight` (568): the height the modal picker's dialog is capped
    /// at — a MAXIMUM, not the height it takes. material3 applies it as
    /// `.heightIn(max = DatePickerModalTokens.ContainerHeight)` (`DatePickerDialog.android.kt:84`).
    ///
    /// The dialog's content is 512: a 120 dp header over the 56 dp month navigation, 48 dp weekday row
    /// and 288 dp month. The action row adds 48 — the button's 40 dp in winia (`ButtonSize::Small`'s
    /// container height, which `ButtonDefaults::min_height` applies to this `Button::text()`; material3
    /// uses 48 for the same button) under [`Self::MODAL_BUTTONS_BOTTOM_PADDING`]. So the dialog lands on
    /// 560, 8 short of the cap — which is exactly why material3 wraps its content in a
    /// `weight(1f, fill = false)` box (`DatePickerDialog.android.kt:95`) rather than filling the cap.
    /// This comment used to say the "docked picker's 512 plus the action row's 56 reach it exactly":
    /// the row is 48, and the 568 a fixture reported was the cap holding because winia stretched the
    /// column. `docs/date-picker.md` has the measurement and the fix.
    pub const MODAL_CONTAINER_HEIGHT: f32 = 568.0;

    /// `DialogButtonsPadding`'s bottom (`DatePickerDialog.android.kt:113`).
    pub const MODAL_BUTTONS_BOTTOM_PADDING: f32 = 8.0;

    /// `DialogButtonsPadding`'s end.
    pub const MODAL_BUTTONS_END_PADDING: f32 = 6.0;

    /// `DialogButtonsMainAxisSpacing`: between the dismiss and the confirm button.
    pub const MODAL_BUTTONS_SPACING: f32 = 8.0;
}

/// The colour roles a date picker paints with (`DatePickerColors`, `DatePicker.kt:835-1103`).
///
/// The defaults come from the theme the way `DatePickerDefaults.defaultDatePickerColors` derives them
/// (`DatePicker.kt:545-608`), including the one role material3 hardcodes instead of reading a token
/// (`navigation_content`, `DatePicker.kt:559`).
#[derive(Clone, Debug)]
pub struct DatePickerColors {
    /// The container behind every part.
    pub container: Color,
    /// The title's text.
    pub title_content: Color,
    /// The headline's text, and the mode toggle's icon.
    pub headline_content: Color,
    /// The weekday letters.
    pub weekday_content: Color,
    /// The month navigation: its arrows and its year text.
    pub navigation_content: Color,
    /// A day's label when it is neither selected nor today.
    pub day_content: Color,
    /// A year's label when it is neither the current year nor selected
    /// (`SelectionYearUnselectedLabelTextColor`).
    pub year_content: Color,
    /// The label of a selected day or year.
    pub selected_content: Color,
    /// The container of a selected day or year.
    pub selected_container: Color,
    /// Today's label while today is not selected.
    pub today_content: Color,
    /// The 1 dp outline around today.
    pub today_border: Color,
    /// The divider under the header.
    pub divider: Color,
}

impl DatePickerColors {
    /// The defaults for `theme`.
    pub fn from_theme(theme: &ThemeColors) -> Self {
        Self {
            container: theme.surface_container_high,
            title_content: theme.on_surface_variant,
            headline_content: theme.on_surface_variant,
            weekday_content: theme.on_surface,
            navigation_content: theme.on_surface_variant,
            day_content: theme.on_surface,
            year_content: theme.on_surface_variant,
            selected_content: theme.on_primary,
            selected_container: theme.primary,
            today_content: theme.primary,
            today_border: theme.primary,
            divider: theme.outline_variant,
        }
    }

    /// A role at `DisabledAlpha` (`ColorScheme.kt:1518`). Compose's `copy(alpha = 0.38f)` *replaces* the alpha
    /// rather than scaling it, so this replaces the channel too.
    pub(super) fn disabled(role: Color) -> Color {
        Color {
            a: (DatePickerDefaults::DISABLED_ALPHA * 255.0).round() as u8,
            ..role
        }
    }

    /// The container behind one day cell (`dayContainerColor`, `DatePicker.kt:973-993`): `Primary` when the day
    /// is selected, transparent when it is not.
    pub fn day_container(&self, selected: bool, enabled: bool) -> Color {
        if !selected {
            return Color::TRANSPARENT;
        }
        if enabled {
            self.selected_container
        } else {
            Self::disabled(self.selected_container)
        }
    }

    /// One day cell's label (`dayContentColor`, `DatePicker.kt:936-963`): selected takes `OnPrimary`, today
    /// takes `Primary`, anything else takes the plain day role — and a disabled cell takes the plain role at
    /// `DisabledAlpha` even when it is today.
    pub fn day_label(&self, selected: bool, enabled: bool, is_today: bool) -> Color {
        match (selected, enabled) {
            (true, true) => self.selected_content,
            (true, false) => Self::disabled(self.selected_content),
            (false, true) if is_today => self.today_content,
            (false, true) => self.day_content,
            (false, false) => Self::disabled(self.day_content),
        }
    }

    /// A day that belongs to the neighbouring month, in the docked picker.
    ///
    /// The M3 specs page gives this its own two tokens — "Date unselected outside month label text color"
    /// `#1D1B20` and "Date unselected outside month label text opacity" `0.38`. `#1D1B20` is the baseline
    /// `onSurface`, the same colour the specs give "Date unselected label text color", and `0.38` is
    /// `DisabledAlpha` (`ColorScheme.kt:1518`) — so the pair lands on the plain day role dimmed to 38%, which
    /// is what [`Self::day_label`] returns for a disabled, unselected cell.
    ///
    /// Spelled out as its own role rather than left to fall out of [`Self::day_label`] because outside days
    /// are NOT disabled — they are context, and [`DayCell::is_enabled`] is false for them for a different
    /// reason. material3 has no colour for them at all (it composes an empty `Spacer`,
    /// `DatePicker.kt:1870-1890`), so the match with the disabled expression is the only thing joining them,
    /// and a reader should not have to rediscover that. Only the docked picker reaches this; the modal one
    /// leaves those slots empty.
    pub fn outside_month_label(&self) -> Color {
        Self::disabled(self.day_content)
    }

    /// The container behind one year cell (`yearContainerColor`, `DatePicker.kt:1029-1046`): `Primary` when the
    /// year is selected, transparent when it is not.
    pub fn year_container(&self, selected: bool, enabled: bool) -> Color {
        if !selected {
            return Color::TRANSPARENT;
        }
        if enabled {
            self.selected_container
        } else {
            Self::disabled(self.selected_container)
        }
    }

    /// One year cell's label (`yearContentColor`, `DatePicker.kt:1005-1027`): a selected year takes
    /// `OnPrimary`, the current year takes `Primary`, anything else takes the plain year role — and a disabled
    /// cell takes the plain role at `DisabledAlpha` even when it is the current year.
    pub fn year_label(&self, current_year: bool, selected: bool, enabled: bool) -> Color {
        match (selected, enabled) {
            (true, true) => self.selected_content,
            (true, false) => Self::disabled(self.selected_content),
            (false, true) if current_year => self.today_content,
            (false, true) => self.year_content,
            (false, false) => Self::disabled(self.year_content),
        }
    }
}
