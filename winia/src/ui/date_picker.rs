//! Date pickers, starting with the calendar the picker is built on.
//!
//! material3's date pickers are a calendar model plus three surfaces over it: the docked date picker, the
//! modal date picker and the modal date input (see `docs/date-picker.md` for the spec tables and the source
//! anchors). This module starts at the bottom of that stack — the proleptic Gregorian date arithmetic and
//! the month descriptions the grid, the header and the range pickers all read.
//!
//! The rules come from `androidx/compose/material3/internal/CalendarModel.kt` and its Android
//! implementation:

//! everything is UTC (material3's millis values are "the start of the day in UTC"), a week starts on the
//! locale's `first_day_of_week`, and the offset of a month's first day inside that week is
//! `day_of_week - first_day_of_week`, wrapped into `0..7` (`CalendarModelImpl.android.kt:201-222`).
//!
//! winia has no locale database, so [`CalendarLocale`] carries the weekday and month names and the first day
//! of the week explicitly, and [`CalendarLocale::default`] is an English, Sunday-first locale. A caller that
//! needs another language supplies its own; the picker never reads a global.

use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::layout::{Alignment, Arrangement};
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::divider::Divider;
use crate::ui::icon::Icon;
use crate::ui::icon_button::IconButton;
use crate::ui::layout_components::{Column, Row, Stack};
use crate::ui::surface::{Surface, SurfaceBorder};
use crate::ui::text::{ProvideTextStyle, Text};
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::ops::RangeInclusive;
use std::sync::Arc;

/// Milliseconds in a day, the unit material3's pickers count in (`CalendarModel.kt:316`).
pub const MILLIS_IN_24_HOURS: i64 = 86_400_000;

/// Days in a week (`CalendarModel.kt:315`).
pub const DAYS_IN_WEEK: u32 = 7;

/// The default year range of the date picker dialogs, `DatePickerDefaults.YearRange`
/// (`DatePicker.kt:764`).
pub const DEFAULT_YEAR_RANGE: std::ops::RangeInclusive<i32> = 1900..=2100;

/// A day in the proleptic Gregorian calendar, with no time and no zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDate {
    /// The Gregorian year, which may be negative for dates before year 1.
    pub year: i32,
    /// The month, `1..=12`.
    pub month: u32,
    /// The day of the month, `1..=31`, always valid for `year` and `month` when built through
    /// [`CalendarDate::new`].
    pub day: u32,
}

impl CalendarDate {
    /// A date, or `None` when the month or the day is out of range for that month.
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        if month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// The number of days since 1970-01-01, negative before it.
    pub fn days_since_epoch(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// The start of this day in UTC milliseconds since the epoch — material3's canonical form
    /// (`DatePickerState.selectedDateMillis`).
    pub fn start_of_day_millis(self) -> i64 {
        self.days_since_epoch() * MILLIS_IN_24_HOURS
    }

    /// The day of the week, `1 = Monday .. 7 = Sunday`, the numbering `java.time.DayOfWeek` uses and the
    /// one `CalendarModel.firstDayOfWeek` is expressed in (`CalendarModelImpl.android.kt:64`).
    pub fn day_of_week(self) -> u32 {
        // 1970-01-01 was a Thursday, which is 4 in this numbering.
        ((self.days_since_epoch() + 3).rem_euclid(DAYS_IN_WEEK as i64)) as u32 + 1
    }

    /// The number of days in this date's month.
    pub fn number_of_days(self) -> u32 {
        days_in_month(self.year, self.month)
    }
}

/// Whether `year` is a leap year in the proleptic Gregorian calendar.
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The number of days in a month, `1..=28..=31`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// The date `millis` falls on, in UTC.
pub fn date_of_millis(millis: i64) -> CalendarDate {
    let (year, month, day) = civil_from_days(millis.div_euclid(MILLIS_IN_24_HOURS));
    CalendarDate { year, month, day }
}

/// The start of the UTC day `millis` falls in.
pub fn canonical_millis(millis: i64) -> i64 {
    millis.div_euclid(MILLIS_IN_24_HOURS) * MILLIS_IN_24_HOURS
}

/// Days from 1970-01-01 to `year-month-day`, proleptic Gregorian (Howard Hinnant's `days_from_civil`).
///
/// The month is shifted so the year starts in March, which puts the leap day at the end of the year and makes
/// the whole computation linear.
pub fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The inverse of [`days_from_civil`] (Hinnant's `civil_from_days`).
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

/// One month of the calendar, as the grid needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarMonth {
    /// The Gregorian year.
    pub year: i32,
    /// The month, `1..=12`.
    pub month: u32,
    /// The number of days in the month.
    pub number_of_days: u32,
    /// How many empty cells the grid leaves before the 1st: `day_of_week - first_day_of_week`, wrapped into
    /// `0..7` (`CalendarModelImpl.android.kt:202-208`).
    pub days_from_start_of_week_to_first_of_month: u32,
    /// The 1st of the month at 00:00 UTC — the value `DatePickerState.displayedMonthMillis` holds.
    pub start_utc_time_millis: i64,
}

impl CalendarMonth {
    /// The last millisecond of the month, `start + days · 24h − 1` (`CalendarModel.kt:258`). The range
    /// pickers compare dates against it.
    pub fn end_utc_time_millis(&self) -> i64 {
        self.start_utc_time_millis + self.number_of_days as i64 * MILLIS_IN_24_HOURS - 1
    }

    /// The index of this month in `year_range`, `0` for the first month of the first year. This is the list
    /// index material3 scrolls to (`DatePicker.kt:1540`).
    pub fn index_in(&self, year_range: &std::ops::RangeInclusive<i32>) -> i64 {
        (self.year - year_range.start()) as i64 * 12 + self.month as i64 - 1
    }
}

/// The month names and the week layout winia formats with.
///
/// material3 reads these from the platform locale (`java.util.Locale` plus `WeekFields`). winia has no locale
/// database, so they are data: [`CalendarLocale::default`] is English, Sunday-first, and a caller that wants
/// another language builds one.
#[derive(Clone, Debug)]
pub struct CalendarLocale {
    /// The first day of the week, `1 = Monday .. 7 = Sunday` (`WeekFields.firstDayOfWeek.value`).
    pub first_day_of_week: u32,
    /// The seven weekday names, **from Monday**, each `(full, narrow)` — the order and the pair shape
    /// `CalendarModel.weekdayNames` has (`CalendarModelImpl.android.kt:66-73`).
    pub weekday_names: [(String, String); 7],
    /// The twelve month names, January first, for the header's month and year text.
    pub month_names: [String; 12],
}

impl Default for CalendarLocale {
    fn default() -> Self {
        const WEEKDAYS: [(&str, &str); 7] = [
            ("Monday", "M"),
            ("Tuesday", "T"),
            ("Wednesday", "W"),
            ("Thursday", "T"),
            ("Friday", "F"),
            ("Saturday", "S"),
            ("Sunday", "S"),
        ];
        const MONTHS: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        Self {
            // Sunday, the first day of the week in the en-US locale material3's samples run in.
            first_day_of_week: 7,
            weekday_names: WEEKDAYS.map(|(full, narrow)| (full.to_string(), narrow.to_string())),
            month_names: MONTHS.map(|name| name.to_string()),
        }
    }
}

impl CalendarLocale {
    /// The weekday names in the order the header row draws them: starting at
    /// `first_day_of_week - 1` and wrapping (`DatePicker.kt:1784-1793`).
    pub fn weekdays_in_row_order(&self) -> Vec<(String, String)> {
        let start = self.first_day_of_week as usize - 1;
        let mut names = Vec::with_capacity(7);
        for index in 0..7 {
            names.push(self.weekday_names[(start + index) % 7].clone());
        }
        names
    }
}

/// The calendar model: date arithmetic plus the locale the picker formats and lays out with.
#[derive(Clone, Debug)]
pub struct CalendarModel {
    locale: CalendarLocale,
}

impl CalendarModel {
    /// A model over `locale`.
    pub fn new(locale: CalendarLocale) -> Self {
        Self { locale }
    }

    /// The locale this model was built with (`CalendarModel.locale`).
    pub fn locale(&self) -> &CalendarLocale {
        &self.locale
    }

    /// The first day of the week, `1 = Monday .. 7 = Sunday`.
    pub fn first_day_of_week(&self) -> u32 {
        self.locale.first_day_of_week
    }

    /// The weekday names in header order.
    pub fn weekday_names(&self) -> Vec<(String, String)> {
        self.locale.weekdays_in_row_order()
    }

    /// The month `millis` falls in.
    pub fn month_of_millis(&self, millis: i64) -> CalendarMonth {
        let date = date_of_millis(millis);
        self.month_of(date.year, date.month)
    }

    /// The month `year-month` describes.
    pub fn month_of(&self, year: i32, month: u32) -> CalendarMonth {
        let first = CalendarDate::new(year, month, 1).expect("month 1..=12 names a real month");
        let difference = first.day_of_week() as i64 - self.locale.first_day_of_week as i64;
        let offset = if difference < 0 { difference + DAYS_IN_WEEK as i64 } else { difference };
        CalendarMonth {
            year,
            month,
            number_of_days: days_in_month(year, month),
            days_from_start_of_week_to_first_of_month: offset as u32,
            start_utc_time_millis: first.start_of_day_millis(),
        }
    }

    /// The month `count` months after the month starting at `month_start_millis`
    /// (`CalendarModel.plusMonths`).
    ///
    /// Deliberate deviation: material3's Android implementation returns its input unchanged for a
    /// non-positive `count` (`CalendarModelImpl.android.kt:118-120`), which its month list never reaches —
    /// it only ever adds `0..`. winia computes the real month in both directions.
    pub fn plus_months(&self, month_start_millis: i64, count: i64) -> CalendarMonth {
        let month = self.month_of_millis(month_start_millis);
        let index = month.year as i64 * 12 + month.month as i64 - 1 + count;
        self.month_of(index.div_euclid(12) as i32, index.rem_euclid(12) as u32 + 1)
    }

    /// The date `millis` names, at the start of its UTC day.
    pub fn canonical_date(&self, millis: i64) -> CalendarDate {
        date_of_millis(millis)
    }

    /// The start of the UTC day `millis` falls in.
    pub fn canonical_millis(&self, millis: i64) -> i64 {
        canonical_millis(millis)
    }

    /// Today at the start of its UTC day, from the system clock — material3's `CalendarModel.today`
    /// (`internal/CalendarModelImpl.android.kt:49-66`), which is the platform clock in UTC.
    pub fn today_millis(&self) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0);
        canonical_millis(now)
    }

    /// The month and year as text, `September 2024` — the shape `DatePickerFormatter.formatMonthYear`
    /// returns and the picker's year menu shows (`DatePicker.kt:1565-1568`).
    pub fn format_month_year(&self, month_start_millis: i64) -> String {
        let month = self.month_of_millis(month_start_millis);
        format!("{} {}", self.locale.month_names[month.month as usize - 1], month.year)
    }

    /// The date as text, `Sep 1, 2024`; with `for_content_description` the verbose form, which carries the
    /// full weekday because material3 formats descriptions with the `yMMMMEEEEd` skeleton
    /// (`DatePicker.kt:789`): `Sunday, September 1, 2024`.
    pub fn format_date(&self, date_millis: i64, for_content_description: bool) -> String {
        let date = date_of_millis(date_millis);
        let month = &self.locale.month_names[date.month as usize - 1];
        if for_content_description {
            let weekday = &self.locale.weekday_names[date.day_of_week() as usize - 1].0;
            format!("{weekday}, {month} {}, {}", date.day, date.year)
        } else {
            format!("{} {}, {}", &month[..3.min(month.len())], date.day, date.year)
        }
    }

    /// The number of months between the ends of `year_range`, twelve per year
    /// (`numberOfMonthsInRange`, `DatePicker.kt:1963`).
    pub fn number_of_months_in_range(year_range: &std::ops::RangeInclusive<i32>) -> i64 {
        (*year_range.end() as i64 - *year_range.start() as i64 + 1) * 12
    }
}

/// Which half of a date picker is showing (`DisplayMode`, `DatePicker.kt:330-348`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    /// The calendar.
    Picker,
    /// Manual entry in a text field.
    Input,
}

/// Decides which dates and years a picker allows (`SelectableDates`, `DatePicker.kt:286-299`).
pub trait SelectableDates: Send + Sync {
    /// Whether the day containing `utc_time_millis` may be selected.
    fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
        let _ = utc_time_millis;
        true
    }

    /// Whether `year` may be selected. A year that is not selectable disables all of its dates.
    fn is_selectable_year(&self, year: i32) -> bool {
        let _ = year;
        true
    }
}

/// The default policy: every date and year is selectable (`DatePickerDefaults.AllDates`, `DatePicker.kt:774`).
#[derive(Clone, Copy, Debug, Default)]
pub struct AllDates;

impl SelectableDates for AllDates {}

/// What a [`DatePickerState`] starts from — material3's `DatePickerState(…)` parameters
/// (`DatePicker.kt:423-438`), in a struct because Rust has no default arguments.
#[derive(Clone)]
pub struct DatePickerStateInit {
    /// The initial selection, `None` for no selection.
    pub initial_selected_date_millis: Option<i64>,
    /// The month to show first. `None` means material3's default: the month of the initial selection, or
    /// today when there is none.
    pub initial_displayed_month_millis: Option<i64>,
    /// The years the picker may show and select.
    pub year_range: RangeInclusive<i32>,
    /// The mode to start in.
    pub initial_display_mode: DisplayMode,
    /// Which dates and years are allowed.
    pub selectable_dates: Arc<dyn SelectableDates>,
    /// What "today" means. `None` reads the system clock; a test passes a fixed value.
    pub today_millis: Option<i64>,
}

impl Default for DatePickerStateInit {
    fn default() -> Self {
        Self {
            initial_selected_date_millis: None,
            initial_displayed_month_millis: None,
            year_range: DEFAULT_YEAR_RANGE,
            initial_display_mode: DisplayMode::Picker,
            selectable_dates: Arc::new(AllDates),
            today_millis: None,
        }
    }
}

/// A date picker's state, hoisted so a caller can read and drive it (`DatePickerState`, `DatePicker.kt:244`).
///
/// Both millisecond values are the start of a UTC day, and both setters **coerce** rather than throw: a
/// selection whose year is outside `year_range` becomes `None`, and a displayed month outside it is dropped.
/// material3 documents `IllegalArgumentException` and does exactly this in code (`DatePicker.kt:1153-1159`,
/// `:1209-1218`); see `docs/date-picker.md` for the disagreement.
#[derive(Clone)]
pub struct DatePickerState {
    selected_date_millis: State<Option<i64>>,
    displayed_month_millis: State<i64>,
    display_mode: State<DisplayMode>,
    today_millis: i64,
    year_range: RangeInclusive<i32>,
    locale: CalendarLocale,
    model: CalendarModel,
    selectable_dates: Arc<dyn SelectableDates>,
}

impl DatePickerState {
    /// A state with material3's defaults: no selection, the calendar on today, the default year range, every
    /// date selectable.
    pub fn new(locale: CalendarLocale) -> Self {
        Self::with(locale, DatePickerStateInit::default())
    }

    /// A state over `init`.
    pub fn with(locale: CalendarLocale, init: DatePickerStateInit) -> Self {
        let model = CalendarModel::new(locale.clone());
        let today_millis = init
            .today_millis
            .map(canonical_millis)
            .unwrap_or_else(|| model.today_millis());
        let today = model.month_of_millis(today_millis);
        let selected = init
            .initial_selected_date_millis
            .map(|millis| model.canonical_millis(millis))
            .filter(|millis| init.year_range.contains(&model.canonical_date(*millis).year));
        // material3's default displayed month is the selection's month; a month whose year is out of range
        // falls back to today, like `BaseDatePickerStateImpl` (`DatePicker.kt:1135-1149`).
        let displayed = init
            .initial_displayed_month_millis
            .or(selected)
            .map(|millis| model.month_of_millis(millis))
            .filter(|month| init.year_range.contains(&month.year))
            .unwrap_or(today);
        Self {
            selected_date_millis: State::new(selected),
            displayed_month_millis: State::new(displayed.start_utc_time_millis),
            display_mode: State::new(init.initial_display_mode),
            today_millis,
            year_range: init.year_range,
            locale,
            model,
            selectable_dates: init.selectable_dates,
        }
    }

    /// Steps the displayed month by `delta`, clamped to the year range.
    ///
    /// The month arrows call this rather than computing from a month captured when they were composed: that
    /// value goes stale as soon as the picker recomposes without rebuilding the arrow, which made a step back
    /// followed by a step forward read the month it started from.
    pub fn step_displayed_month(&self, delta: i32) {
        let model = self.calendar_model();
        let current = model.month_of_millis(self.displayed_month_millis());
        let month = model.plus_months(current.start_utc_time_millis, delta as i64);
        if self.year_range.contains(&month.year) {
            self.set_displayed_month_millis(month.start_utc_time_millis);
        }
    }

    /// The day the picker outlines as today, at the start of its UTC day. material3 reads the platform clock
    /// for it (`internal/CalendarModelImpl.android.kt:49-66`); a caller may pin it, and the tests do.
    pub fn today_millis(&self) -> i64 {
        self.today_millis
    }

    /// The selection, or `None` — the start of the selected day in UTC.
    pub fn selected_date_millis(&self) -> Option<i64> {
        self.selected_date_millis.get()
    }

    /// Sets the selection. The timestamp is canonicalised to the start of its UTC day, and a date whose year
    /// is outside the year range clears the selection (`DatePicker.kt:1209-1218`).
    pub fn set_selected_date_millis(&self, millis: Option<i64>) {
        let canonical = millis
            .map(|millis| self.model.canonical_millis(millis))
            .filter(|millis| self.year_range.contains(&self.model.canonical_date(*millis).year));
        self.selected_date_millis.set(canonical);
    }

    /// The month the calendar shows, as the millis of its first day.
    pub fn displayed_month_millis(&self) -> i64 {
        self.displayed_month_millis.get()
    }

    /// Shows the month containing `millis`. A month whose year is outside the year range is ignored
    /// (`DatePicker.kt:1153-1159`).
    pub fn set_displayed_month_millis(&self, millis: i64) {
        let month = self.model.month_of_millis(millis);
        if self.year_range.contains(&month.year) {
            self.displayed_month_millis.set(month.start_utc_time_millis);
        }
    }

    /// The current mode.
    pub fn display_mode(&self) -> DisplayMode {
        self.display_mode.get()
    }

    /// Switches mode. As in material3 (`DatePicker.kt:1228-1233`), a selection pulls the calendar back to the
    /// month that selection is in.
    pub fn set_display_mode(&self, mode: DisplayMode) {
        if let Some(selected) = self.selected_date_millis() {
            self.set_displayed_month_millis(selected);
        }
        self.display_mode.set(mode);
    }

    /// The years this picker may show and select.
    pub fn year_range(&self) -> RangeInclusive<i32> {
        self.year_range.clone()
    }

    /// The locale the calendar formats and lays out with.
    pub fn locale(&self) -> &CalendarLocale {
        &self.locale
    }

    /// The calendar model this state was built with (`BaseDatePickerStateImpl.calendarModel`,
    /// `DatePicker.kt:1131`).
    pub fn calendar_model(&self) -> &CalendarModel {
        &self.model
    }

    /// Which dates and years are selectable.
    pub fn selectable_dates(&self) -> &dyn SelectableDates {
        self.selectable_dates.as_ref()
    }
}

/// The state a picker composes with, remembered across recompositions
/// (`rememberDatePickerState`, `DatePicker.kt:368-390`).
pub fn remember_date_picker_state(
    ctx: &mut crate::core::composer::ComposeCtx,
    locale: CalendarLocale,
) -> DatePickerState {
    ctx.remember(|| DatePickerState::new(locale)).get()
}

/// The rows a month grid always draws, whether or not the month needs them (`MaxCalendarRows`,
/// `DatePicker.kt:2303`).
pub const MAX_CALENDAR_ROWS: u32 = 6;

/// The word a today cell announces (`DatePickerTodayDescription`).
pub const TODAY_DESCRIPTION: &str = "Today";

/// One day in a month grid, with the flags the cell paints from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayCell {
    /// The day of the month, `1..=31`.
    pub day: u32,
    /// The start of that day in UTC millis — material3 adds `dayNumber · 24 h` to the month start
    /// (`DatePicker.kt:1893-1894`).
    pub utc_time_millis: i64,
    /// Whether this is the day the caller calls today.
    pub is_today: bool,
    /// Whether this is the selected day.
    pub is_selected: bool,
    /// Whether it may be chosen: `SelectableDates::is_selectable_date`, and its year must be selectable too
    /// (`DatePicker.kt:294-298`).
    pub is_enabled: bool,
}

/// A month laid out the way the picker draws it: [`MAX_CALENDAR_ROWS`] rows of [`DAYS_IN_WEEK`] cells, with the
/// cells before the 1st and after the last day empty (`Month`, `DatePicker.kt:1856-1890`).
#[derive(Clone, Debug)]
pub struct MonthGrid {
    month: CalendarMonth,
    cells: Vec<Option<DayCell>>,
}

impl MonthGrid {
    /// Lays `month` out for `selection` and `today_millis`, asking `selectable_dates` about every day.
    pub fn of(
        month: CalendarMonth,
        selection: Option<i64>,
        today_millis: i64,
        selectable_dates: &dyn SelectableDates,
    ) -> Self {
        let offset = month.days_from_start_of_week_to_first_of_month as usize;
        let end = offset + month.number_of_days as usize;
        let year_selectable = selectable_dates.is_selectable_year(month.year);
        let mut cells = Vec::with_capacity((MAX_CALENDAR_ROWS * DAYS_IN_WEEK) as usize);
        for index in 0..(MAX_CALENDAR_ROWS * DAYS_IN_WEEK) as usize {
            if index < offset || index >= end {
                cells.push(None);
                continue;
            }
            let day = (index - offset) as u32 + 1;
            let utc_time_millis =
                month.start_utc_time_millis + (index - offset) as i64 * MILLIS_IN_24_HOURS;
            cells.push(Some(DayCell {
                day,
                utc_time_millis,
                is_today: utc_time_millis == today_millis,
                is_selected: selection == Some(utc_time_millis),
                is_enabled: year_selectable
                    && selectable_dates.is_selectable_date(utc_time_millis),
            }));
        }
        Self { month, cells }
    }

    /// The month this grid lays out.
    pub fn month(&self) -> CalendarMonth {
        self.month
    }

    /// Every cell, row by row.
    pub fn cells(&self) -> &[Option<DayCell>] {
        &self.cells
    }

    /// The grid row by row, [`DAYS_IN_WEEK`] cells each.
    pub fn rows(&self) -> impl Iterator<Item = &[Option<DayCell>]> {
        self.cells.chunks(DAYS_IN_WEEK as usize)
    }

    /// The cell at `row` and `column`, counting from zero.
    pub fn cell(&self, row: u32, column: u32) -> Option<&Option<DayCell>> {
        self.cells.get((row * DAYS_IN_WEEK + column) as usize)
    }
}

/// What a day cell announces: the today word, then the date itself
/// (`DatePicker.kt:1946-1951`, `:1967-1990`).
///
/// The range words material3 adds for the range pickers (`DateRangePickerStartHeadline`,
/// `DateRangePickerEndHeadline`, `DateRangePickerDayInRange`) arrive with those pickers. material3's wording
/// comes from resources this checkout does not carry, so winia supplies its own — see
/// `docs/date-picker.md`.
pub fn day_content_description(model: &CalendarModel, cell: &DayCell) -> String {
    let formatted = model.format_date(cell.utc_time_millis, true);
    if cell.is_today {
        format!("{TODAY_DESCRIPTION}, {formatted}")
    } else {
        formatted
    }
}

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

    /// The default title (`DatePicker.kt:654`). material3's wording comes from resources this checkout does not
    /// carry, so winia supplies its own English.
    pub const TITLE: &'static str = "Select date";

    /// The headline while nothing is selected (`DatePicker.kt:704`).
    pub const HEADLINE: &'static str = "No date selected";

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
            selected_content: theme.on_primary,
            selected_container: theme.primary,
            today_content: theme.primary,
            today_border: theme.primary,
            divider: theme.outline_variant,
        }
    }

    /// A role at `DisabledAlpha` (`ColorScheme.kt:1518`). Compose's `copy(alpha = 0.38f)` *replaces* the alpha
    /// rather than scaling it, so this replaces the channel too.
    fn disabled(role: Color) -> Color {
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
}

/// Material Icons `keyboard_arrow_left` (24 dp), the glyph material3 auto-mirrors for its month arrows
/// (`internal/Icons.kt:34`). Provenance and the measured guard: `docs/date-picker.md`.
pub const CHEVRON_LEFT_PATH: &str = "M15.41 16.09l-4.58-4.59 4.58-4.59L14 5.5l-6 6 6 6z";

/// Material Icons `keyboard_arrow_right` (24 dp, `internal/Icons.kt:60`).
pub const CHEVRON_RIGHT_PATH: &str = "M8.59 16.59L13.17 12 8.59 7.41 10 6l6 6-6 6z";

/// The docked date picker: a title and a headline over a month calendar
/// (`DatePicker`, `DatePicker.kt:168-237`; the M3 spec's docked date picker).
///
/// ```ignore
/// let state = remember_date_picker_state(ctx, CalendarLocale::default());
/// DatePicker::new(state).build(ctx);
/// ```
///
/// Deliberate deviations: material3 pages months in a `LazyRow` with a snap fling, while winia has no lazy row,
/// so the picker composes the displayed month and its arrows step it one month at a time; and the mode toggle
/// arrives with the input mode.
pub struct DatePicker {
    state: DatePickerState,
    title: Option<String>,
    modifier: Modifier,
}

impl DatePicker {
    /// A picker over `state`, with material3's default title.
    pub fn new(state: DatePickerState) -> Self {
        Self {
            state,
            title: Some(DatePickerDefaults::TITLE.to_string()),
            modifier: Modifier::new(),
        }
    }

    /// The title above the headline. `None` drops the title slot, and with it the header's minimum height and
    /// the divider (`DatePicker.kt:1680-1685`, `:1392-1394`).
    pub fn title(mut self, title: Option<impl Into<String>>) -> Self {
        self.title = title.map(Into::into);
        self
    }

    /// A modifier for the container (`modifier`).
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Composes the picker.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let colors = DatePickerColors::from_theme(&WiniaTheme::colors());
        let model = self.state.calendar_model().clone();
        let month = model.month_of_millis(self.state.displayed_month_millis());
        let grid = MonthGrid::of(
            month,
            self.state.selected_date_millis(),
            self.state.today_millis(),
            self.state.selectable_dates(),
        );
        let state = self.state.clone();
        let title = self.title.clone();
        // material3 pins the picker's width with `sizeIn(minWidth = 360)` on a column that wraps its content,
        // and those numbers are exact: seven slots of the 48 dp accessibility size (336) plus the 12 dp of
        // horizontal padding on both sides. winia stretches an auto-width child to the width its parent
        // offers, which would spread the weekday row and the grid past that lattice, so the width is pinned.
        let container = self
            .modifier
            .then(Modifier::new().width(DatePickerDefaults::CONTAINER_WIDTH))
            .background(colors.container, Shape::Rectangle);

        Column::new()
            .modifier(container)
            .arrangement(Arrangement::Start)
            .build(ctx, |ctx| {
                header(ctx, &state, title.as_deref(), &colors);
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding_horizontal(DatePickerDefaults::HORIZONTAL_PADDING),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        months_navigation(ctx, &state, &month, &colors);
                        weekday_row(ctx, &model, &colors);
                        month_grid(ctx, &state, &model, &grid, &colors);
                    });
            });
    }
}

/// The header: the title over the headline, with the divider below them
/// (`DateEntryContainer` and `DatePickerHeader`, `DatePicker.kt:1365-1396`, `:1671-1698`).
fn header(ctx: &mut ComposeCtx, state: &DatePickerState, title: Option<&str>, colors: &DatePickerColors) {
    let model = state.calendar_model();
    let headline = state
        .selected_date_millis()
        .map(|millis| model.format_date(millis, false))
        .unwrap_or_else(|| DatePickerDefaults::HEADLINE.to_string());
    // material3 gives the header a *minimum* height of 120 dp and lets its content grow past it; winia
    // stretches an auto-height child to fill the space its parent offers, which with `SpaceBetween` would push
    // the title and the headline apart, so the header's height is exact here.
    let height = if title.is_some() {
        DatePickerDefaults::HEADER_MIN_HEIGHT
    } else {
        DatePickerDefaults::HEADLINE_ROW_HEIGHT
    };
    let title = title.map(str::to_string);
    let title_color = colors.title_content;
    let headline_color = colors.headline_content;

    Column::new()
        .modifier(Modifier::new().fill_max_width().height(height))
        .arrangement(Arrangement::SpaceBetween)
        .build(ctx, |ctx| {
            if let Some(text) = title.as_deref() {
                ProvideTextStyle(WiniaTheme::typography().label_large.clone(), ctx, |ctx| {
                    Text::new(text.to_string())
                        .color(title_color)
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .padding_start(DatePickerDefaults::TITLE_START_PADDING)
                                .padding_end(DatePickerDefaults::TITLE_END_PADDING)
                                .padding_top(DatePickerDefaults::TITLE_TOP_PADDING),
                        )
                        .build(ctx);
                });
            }
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .arrangement(Arrangement::SpaceBetween)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    ProvideTextStyle(WiniaTheme::typography().headline_large.clone(), ctx, |ctx| {
                        Text::new(headline)
                            .color(headline_color)
                            .max_lines(1)
                            .modifier(
                                Modifier::new()
                                    .padding_start(DatePickerDefaults::TITLE_START_PADDING)
                                    .padding_end(DatePickerDefaults::TITLE_END_PADDING)
                                    .padding_bottom(DatePickerDefaults::HEADLINE_BOTTOM_PADDING),
                            )
                            .build(ctx);
                    });
                });
            // material3 draws the divider when a title, a headline or a mode toggle is present
            // (`DatePicker.kt:1392-1394`); a headline is always composed here.
            Divider::horizontal().build(ctx);
        });
}

/// The month navigation row: the month and year text between the two arrows (`MonthsNavigation`,
/// `DatePicker.kt:2182-2239`). The year menu button arrives with the year picker.
fn months_navigation(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    month: &CalendarMonth,
    colors: &DatePickerColors,
) {
    let year_range = state.year_range();
    let index = month.index_in(&year_range);
    let last = CalendarModel::number_of_months_in_range(&year_range) - 1;
    let text = state.calendar_model().format_month_year(month.start_utc_time_millis);
    let navigation_color = colors.navigation_content;

    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_YEAR_HEIGHT),
        )
        .arrangement(Arrangement::SpaceBetween)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            month_arrow(ctx, state, -1, index > 0, CHEVRON_LEFT_PATH, navigation_color);
            ProvideTextStyle(WiniaTheme::typography().label_large.clone(), ctx, |ctx| {
                Text::new(text)
                    .color(navigation_color)
                    .max_lines(1)
                    .build(ctx);
            });
            month_arrow(ctx, state, 1, index < last, CHEVRON_RIGHT_PATH, navigation_color);
        });
}

/// One month arrow. material3 enables them from the month list's scroll state
/// (`monthsListState.canScrollBackward/Forward`, `DatePicker.kt:1561-1562`); with one month composed at a time
/// they are enabled while the month has a neighbour inside the year range. The step reads the displayed month
/// at the moment of the click (`DatePickerState::step_displayed_month`).
fn month_arrow(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    step: i32,
    enabled: bool,
    path: &'static str,
    color: Color,
) {
    let state = state.clone();
    IconButton::new()
        .enabled(enabled)
        .on_click(move || state.step_displayed_month(step))
        .build(ctx, |ctx| {
            Icon::svg_path(path).tint(color).build(ctx);
        });
}

/// The weekday header: the locale's names in row order, one cell per column (`WeekDays`,
/// `DatePicker.kt:1783-1830`).
fn weekday_row(ctx: &mut ComposeCtx, model: &CalendarModel, colors: &DatePickerColors) {
    let names = model.weekday_names();
    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::ACCESSIBLE_SIZE),
        )
        .arrangement(Arrangement::SpaceEvenly)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                for (full, narrow) in names {
                    Stack::new()
                        .alignment(Alignment::Center)
                        .modifier(Modifier::new().size(
                            DatePickerDefaults::ACCESSIBLE_SIZE,
                            DatePickerDefaults::ACCESSIBLE_SIZE,
                        ))
                        .build(ctx, |ctx| {
                            Text::new(narrow)
                                .color(colors.weekday_content)
                                .modifier(Modifier::new().semantics(
                                    crate::semantics::SemanticsConfig::new()
                                        .content_description(full),
                                ))
                                .build(ctx);
                        });
                }
            });
        });
}

/// The month grid: six rows of seven slots, each slot a day or empty (`Month`,
/// `DatePicker.kt:1856-1890`).
fn month_grid(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    grid: &MonthGrid,
    colors: &DatePickerColors,
) {
    let rows = grid.rows().map(<[Option<DayCell>]>::to_vec).collect::<Vec<_>>();
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_HEIGHT),
        )
        .arrangement(Arrangement::SpaceEvenly)
        .build(ctx, |ctx| {
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                for row in &rows {
                    Row::new()
                        .modifier(Modifier::new().fill_max_width())
                        .arrangement(Arrangement::SpaceEvenly)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            for cell in row {
                                Stack::new()
                                    .alignment(Alignment::Center)
                                    .modifier(Modifier::new().size(
                                        DatePickerDefaults::ACCESSIBLE_SIZE,
                                        DatePickerDefaults::ACCESSIBLE_SIZE,
                                    ))
                                    .build(ctx, |ctx| {
                                        if let Some(cell) = cell {
                                            day_cell(ctx, state, model, cell, colors);
                                        }
                                    });
                            }
                        });
                }
            });
        });
}

/// One day of the grid: a 40 dp circle, outlined when it is today and not selected, filled when it is selected
/// (`Day`, `DatePicker.kt:1993-2058`).
fn day_cell(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    cell: &DayCell,
    colors: &DatePickerColors,
) {
    let state_for_click = state.clone();
    let millis = cell.utc_time_millis;
    let description = day_content_description(model, cell);
    let mut surface = Surface::new()
        .shape(Shape::Circle)
        .color(colors.day_container(cell.is_selected, cell.is_enabled))
        .content_color(colors.day_label(cell.is_selected, cell.is_enabled, cell.is_today))
        .enabled(cell.is_enabled)
        .selectable(cell.is_selected, move || {
            state_for_click.set_selected_date_millis(Some(millis));
        })
        .modifier(
            Modifier::new()
                .size(DatePickerDefaults::DAY_CELL, DatePickerDefaults::DAY_CELL)
                .semantics(
                    crate::semantics::SemanticsConfig::new().content_description(description),
                ),
        );
    if cell.is_today && !cell.is_selected {
        surface = surface.border(SurfaceBorder::new(
            DatePickerDefaults::TODAY_OUTLINE_WIDTH,
            colors.today_border,
        ));
    }
    surface.build(ctx, |ctx| {
        Text::new(cell.day.to_string()).build(ctx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_a_thursday() {
        // 1970-01-01 is the reference point of the whole module, and Thursday is 4 when Monday is 1.
        assert_eq!(CalendarDate::new(1970, 1, 1).unwrap().day_of_week(), 4);
        assert_eq!(CalendarDate::new(1970, 1, 1).unwrap().days_since_epoch(), 0);
    }

    #[test]
    fn civil_days_round_trip_over_three_centuries() {
        for year in 1800..=2200 {
            for month in 1..=12u32 {
                for day in 1..=days_in_month(year, month) {
                    let days = days_from_civil(year, month, day);
                    assert_eq!(
                        civil_from_days(days),
                        (year, month, day),
                        "round trip {year}-{month}-{day}"
                    );
                    assert_eq!(date_of_millis(days * MILLIS_IN_24_HOURS).year, year);
                }
            }
        }
    }

    #[test]
    fn february_has_29_days_only_in_leap_years() {
        assert_eq!(days_in_month(1900, 2), 28, "1900 is not a leap year");
        assert_eq!(days_in_month(2000, 2), 29, "2000 is a leap year");
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
    }

    #[test]
    fn a_day_is_exactly_one_day_of_millis() {
        assert_eq!(
            CalendarDate::new(2024, 9, 2).unwrap().start_of_day_millis()
                - CalendarDate::new(2024, 9, 1).unwrap().start_of_day_millis(),
            MILLIS_IN_24_HOURS
        );
    }

    #[test]
    fn milliseconds_before_the_epoch_land_on_the_right_day() {
        // Truncating division would put -1ms on 1970-01-01; the day before the epoch is 1969-12-31.
        assert_eq!(date_of_millis(-1), CalendarDate::new(1969, 12, 31).unwrap());
        assert_eq!(canonical_millis(-1), -MILLIS_IN_24_HOURS);
    }

    #[test]
    fn the_month_offset_counts_from_the_locales_first_day() {
        // 2024-09-01 is a Sunday, so a Sunday-first week has no empty cells and a Monday-first week has six.
        let sunday_first = CalendarModel::new(CalendarLocale::default());
        assert_eq!(sunday_first.first_day_of_week(), 7);
        assert_eq!(
            sunday_first.month_of(2024, 9).days_from_start_of_week_to_first_of_month,
            0
        );

        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);
        assert_eq!(
            monday_first.month_of(2024, 9).days_from_start_of_week_to_first_of_month,
            6
        );

        // 2021-01-01 is a Friday: five cells after a Sunday, four after a Monday.
        assert_eq!(
            sunday_first.month_of(2021, 1).days_from_start_of_week_to_first_of_month,
            5
        );
        assert_eq!(
            monday_first.month_of(2021, 1).days_from_start_of_week_to_first_of_month,
            4
        );
    }

    #[test]
    fn the_weekday_row_starts_at_the_locales_first_day() {
        let sunday_first = CalendarModel::new(CalendarLocale::default());
        assert_eq!(sunday_first.weekday_names()[0].0, "Sunday");
        assert_eq!(sunday_first.weekday_names()[6].0, "Saturday");

        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);
        assert_eq!(monday_first.weekday_names()[0].0, "Monday");
        assert_eq!(monday_first.weekday_names()[6].0, "Sunday");
    }

    #[test]
    fn a_month_reports_its_own_length_and_start() {
        let model = CalendarModel::new(CalendarLocale::default());
        let february = model.month_of(2024, 2);
        assert_eq!(february.number_of_days, 29);
        assert_eq!(february.start_utc_time_millis % MILLIS_IN_24_HOURS, 0);
        assert_eq!(model.canonical_date(february.start_utc_time_millis).day, 1);
    }

    #[test]
    fn plus_months_crosses_year_boundaries_both_ways() {
        let model = CalendarModel::new(CalendarLocale::default());
        let november = model.month_of(2024, 11).start_utc_time_millis;
        let february = model.plus_months(november, 3);
        assert_eq!((february.year, february.month), (2025, 2));
        let august = model.plus_months(november, -3);
        assert_eq!((august.year, august.month), (2024, 8));
    }

    #[test]
    fn a_months_index_is_its_position_in_the_year_range() {
        let model = CalendarModel::new(CalendarLocale::default());
        assert_eq!(model.month_of(1900, 1).index_in(&DEFAULT_YEAR_RANGE), 0);
        assert_eq!(model.month_of(1900, 2).index_in(&DEFAULT_YEAR_RANGE), 1);
        assert_eq!(model.month_of(1901, 1).index_in(&DEFAULT_YEAR_RANGE), 12);
        assert_eq!(
            CalendarModel::number_of_months_in_range(&DEFAULT_YEAR_RANGE),
            (2100 - 1900 + 1) * 12
        );
    }

    #[test]
    fn the_year_range_holds_the_material3_default() {
        assert_eq!(*DEFAULT_YEAR_RANGE.start(), 1900);
        assert_eq!(*DEFAULT_YEAR_RANGE.end(), 2100);
    }

    #[test]
    fn dates_format_as_the_header_and_the_field_show_them() {
        let model = CalendarModel::new(CalendarLocale::default());
        let september = model.month_of(2024, 9).start_utc_time_millis;
        assert_eq!(model.format_month_year(september), "September 2024");
        assert_eq!(model.format_date(september, false), "Sep 1, 2024");
        assert_eq!(model.format_date(september, true), "Sunday, September 1, 2024");
    }

    #[test]
    fn an_invalid_date_has_no_calendar_date() {
        assert!(CalendarDate::new(2023, 2, 29).is_none());
        assert!(CalendarDate::new(2024, 2, 29).is_some());
        assert!(CalendarDate::new(2024, 13, 1).is_none());
        assert!(CalendarDate::new(2024, 0, 1).is_none());
        assert!(CalendarDate::new(2024, 4, 31).is_none());
    }

    /// The start of the UTC day `year-month-day`.
    fn millis(year: i32, month: u32, day: u32) -> i64 {
        CalendarDate::new(year, month, day).unwrap().start_of_day_millis()
    }

    /// A state initialised with the given values and a fixed "today", so the tests never read the clock.
    fn state(
        init: DatePickerStateInit,
        today: (i32, u32, u32),
    ) -> DatePickerState {
        DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                today_millis: Some(millis(today.0, today.1, today.2)),
                ..init
            },
        )
    }

    #[test]
    fn the_arrows_step_from_the_month_the_state_is_on_now() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let text = |state: &DatePickerState| {
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis())
        };
        state.step_displayed_month(-1);
        assert_eq!(text(&state), "August 2024");
        // The second step reads what the first one left rather than the month the picker started on: a step
        // back and then forward returns to September, never to October.
        state.step_displayed_month(1);
        assert_eq!(text(&state), "September 2024");
        state.step_displayed_month(1);
        assert_eq!(text(&state), "October 2024");
    }

    #[test]
    fn the_arrows_stop_at_the_year_range() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 1, 1)),
                year_range: 2024..=2025,
                ..Default::default()
            },
            (2024, 1, 5),
        );
        state.step_displayed_month(-1);
        assert_eq!(
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis()),
            "January 2024",
            "a step below the range is dropped"
        );
        state.step_displayed_month(24);
        assert_eq!(
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis()),
            "January 2024",
            "and so is a step above it"
        );
    }

    #[test]
    fn a_selection_outside_the_year_range_is_dropped() {
        let state = state(
            DatePickerStateInit {
                initial_selected_date_millis: Some(millis(1899, 12, 31)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert_eq!(
            state.selected_date_millis(),
            None,
            "an initial selection before the year range is no selection"
        );

        state.set_selected_date_millis(Some(millis(2101, 1, 1)));
        assert_eq!(
            state.selected_date_millis(),
            None,
            "and a write outside the range clears the selection instead of throwing"
        );

        state.set_selected_date_millis(Some(millis(2100, 12, 31)));
        assert_eq!(
            state.selected_date_millis(),
            Some(millis(2100, 12, 31)),
            "the last day of the range is selectable"
        );
    }

    #[test]
    fn a_selection_is_canonicalised_to_the_start_of_its_utc_day() {
        let state = state(DatePickerStateInit::default(), (2024, 9, 1));
        state.set_selected_date_millis(Some(millis(2024, 9, 3) + 12 * 3_600_000 + 345));
        assert_eq!(state.selected_date_millis(), Some(millis(2024, 9, 3)));
    }

    #[test]
    fn the_displayed_month_follows_the_selection_and_otherwise_today() {
        let selected = state(
            DatePickerStateInit {
                initial_selected_date_millis: Some(millis(2024, 3, 15)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert_eq!(
            selected.displayed_month_millis(),
            millis(2024, 3, 1),
            "without a displayed month, the calendar opens on the selection's month"
        );

        let unselected = state(DatePickerStateInit::default(), (2024, 9, 1));
        assert_eq!(unselected.displayed_month_millis(), millis(2024, 9, 1));
    }

    #[test]
    fn an_initial_month_outside_the_year_range_falls_back_to_today() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2200, 1, 1)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert_eq!(state.displayed_month_millis(), millis(2024, 9, 1));
    }

    #[test]
    fn showing_a_month_outside_the_year_range_is_ignored() {
        let state = state(DatePickerStateInit::default(), (2024, 9, 1));
        state.set_displayed_month_millis(millis(2101, 5, 20));
        assert_eq!(
            state.displayed_month_millis(),
            millis(2024, 9, 1),
            "a month past the year range does not move the calendar"
        );

        state.set_displayed_month_millis(millis(2001, 5, 20));
        assert_eq!(
            state.displayed_month_millis(),
            millis(2001, 5, 1),
            "an in-range month snaps to its first day"
        );
    }

    #[test]
    fn switching_mode_pulls_the_calendar_to_the_selected_month() {
        let state = state(
            DatePickerStateInit {
                initial_selected_date_millis: Some(millis(2024, 3, 15)),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        state.set_displayed_month_millis(millis(2024, 12, 1));
        assert_eq!(state.displayed_month_millis(), millis(2024, 12, 1));

        state.set_display_mode(DisplayMode::Input);
        assert_eq!(state.display_mode(), DisplayMode::Input);
        assert_eq!(
            state.displayed_month_millis(),
            millis(2024, 3, 1),
            "the switch snaps the calendar back to the month the selection is in"
        );
    }

    #[test]
    fn a_state_starts_in_picker_mode_over_the_full_default_range() {
        let state = DatePickerState::new(CalendarLocale::default());
        assert_eq!(state.display_mode(), DisplayMode::Picker);
        assert_eq!(state.year_range(), DEFAULT_YEAR_RANGE);
        assert_eq!(state.selected_date_millis(), None);
        assert!(state.selectable_dates().is_selectable_date(0));
        assert!(state.selectable_dates().is_selectable_year(2000));
        assert_eq!(state.locale().first_day_of_week, 7);
    }

    #[test]
    fn the_state_keeps_the_selectable_dates_it_was_given() {
        struct NotSelectable;

        impl SelectableDates for NotSelectable {
            fn is_selectable_date(&self, _utc_time_millis: i64) -> bool {
                false
            }
        }

        let state = state(
            DatePickerStateInit {
                selectable_dates: Arc::new(NotSelectable),
                ..Default::default()
            },
            (2024, 9, 1),
        );
        assert!(!state.selectable_dates().is_selectable_date(millis(2024, 9, 1)));
    }

    #[test]
    fn a_month_grid_is_always_six_rows_of_seven() {
        let model = CalendarModel::new(CalendarLocale::default());
        // February 2021 fits in four rows and August 2020 needs six, but the grid is the same shape either
        // way, because the picker reserves the height (`DatePicker.kt:1859`).
        for (year, month) in [(2021, 2), (2020, 8), (2024, 9)] {
            let grid = MonthGrid::of(model.month_of(year, month), None, 0, &AllDates);
            assert_eq!(grid.cells().len(), 42, "{year}-{month} is six rows of seven");
            assert_eq!(grid.rows().count(), 6);
            assert!(grid.rows().all(|row| row.len() == 7));
        }
    }

    #[test]
    fn the_cells_outside_a_month_are_empty() {
        let model = CalendarModel::new(CalendarLocale::default());
        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);

        // Sunday-first, 2024-09-01 is a Sunday: no leading cells, and four empty cells after the 30th.
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &AllDates);
        assert_eq!(grid.cells()[0].unwrap().day, 1, "no leading cells");
        assert_eq!(grid.cells()[29].unwrap().day, 30);
        assert!(grid.cells()[30].is_none(), "the cells after the 30th are empty");
        assert!(grid.cells()[41].is_none(), "the last cell is empty too");

        // Monday-first, the same month starts six cells in.
        let grid = MonthGrid::of(monday_first.month_of(2024, 9), None, 0, &AllDates);
        assert!(grid.cells()[..6].iter().all(|cell| cell.is_none()));
        assert_eq!(grid.cells()[6].unwrap().day, 1);
        assert_eq!(grid.cells()[35].unwrap().day, 30);
    }

    #[test]
    fn a_cell_carries_its_day_and_the_start_of_that_day() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let grid = MonthGrid::of(month, None, 0, &AllDates);
        let fifteenth = grid.cells()[14].unwrap();
        assert_eq!(fifteenth.day, 15);
        assert_eq!(
            fifteenth.utc_time_millis,
            month.start_utc_time_millis + 14 * MILLIS_IN_24_HOURS
        );
        assert_eq!(date_of_millis(fifteenth.utc_time_millis).day, 15);
        assert_eq!(grid.cell(2, 0).unwrap().unwrap().day, 15, "row two, column zero");
    }

    #[test]
    fn today_and_the_selection_are_flagged_on_their_own_cells() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let today = month.start_utc_time_millis + 9 * MILLIS_IN_24_HOURS;
        let selected = month.start_utc_time_millis + 24 * MILLIS_IN_24_HOURS;
        let grid = MonthGrid::of(month, Some(selected), today, &AllDates);

        let flagged = |pick: fn(&DayCell) -> bool| {
            grid.cells()
                .iter()
                .flatten()
                .filter(|day| pick(day))
                .map(|day| day.day)
                .collect::<Vec<u32>>()
        };
        assert_eq!(flagged(|cell| cell.is_today), vec![10]);
        assert_eq!(flagged(|cell| cell.is_selected), vec![25]);
        assert_eq!(flagged(|cell| cell.is_enabled).len(), 30);
    }

    #[test]
    fn an_unselectable_day_or_year_disables_cells() {
        struct FirstWeekOnly;

        impl SelectableDates for FirstWeekOnly {
            fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
                date_of_millis(utc_time_millis).day <= 7
            }
        }

        struct No2025;

        impl SelectableDates for No2025 {
            fn is_selectable_year(&self, year: i32) -> bool {
                year != 2025
            }
        }

        let model = CalendarModel::new(CalendarLocale::default());
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &FirstWeekOnly);
        let enabled = grid
            .cells()
            .iter()
            .flatten()
            .filter(|cell| cell.is_enabled)
            .map(|cell| cell.day)
            .collect::<Vec<u32>>();
        assert_eq!(enabled, (1..=7).collect::<Vec<u32>>());

        // material3: a year that cannot be selected makes every date in it unselectable
        // (`DatePicker.kt:296-297`).
        let grid = MonthGrid::of(model.month_of(2025, 3), None, 0, &No2025);
        assert!(grid.cells().iter().flatten().all(|cell| !cell.is_enabled));
    }

    #[test]
    fn a_day_description_names_today_and_the_date() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let grid = MonthGrid::of(month, None, month.start_utc_time_millis, &AllDates);

        assert_eq!(
            day_content_description(&model, &grid.cells()[0].unwrap()),
            "Today, Sunday, September 1, 2024"
        );
        assert_eq!(
            day_content_description(&model, &grid.cells()[1].unwrap()),
            "Monday, September 2, 2024"
        );
    }

    #[test]
    fn a_day_container_is_primary_only_when_the_day_is_selected() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.day_container(true, true), colors.selected_container);
        assert_eq!(
            colors.day_container(true, false),
            Color {
                a: 97,
                ..colors.selected_container
            },
            "a disabled selected day is Primary at DisabledAlpha"
        );
        assert_eq!(colors.day_container(false, true), Color::TRANSPARENT);
        assert_eq!(colors.day_container(false, false), Color::TRANSPARENT);
    }

    #[test]
    fn a_day_label_follows_material3s_precedence() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.day_label(true, true, false), colors.selected_content);
        assert_eq!(colors.day_label(false, true, true), colors.today_content);
        assert_eq!(colors.day_label(false, true, false), colors.day_content);
        assert_eq!(
            colors.day_label(true, false, true),
            Color {
                a: 97,
                ..colors.selected_content
            }
        );
        // material3 falls through to the disabled branch for a day that is both disabled and today
        // (`DatePicker.kt:936-963`), so today's colour does not survive the disable.
        assert_eq!(
            colors.day_label(false, false, true),
            Color {
                a: 97,
                ..colors.day_content
            }
        );
    }

    #[test]
    fn the_month_arrows_draw_mirrored_chevrons() {
        let left = render_glyph(CHEVRON_LEFT_PATH);
        let right = render_glyph(CHEVRON_RIGHT_PATH);
        let ink = |mask: &[bool]| mask.iter().filter(|on| **on).count();
        let left_ink = ink(&left);
        let right_ink = ink(&right);
        assert!(
            (20..=140).contains(&left_ink),
            "the chevron covers {left_ink} of the 576 pixels in its 24 dp box"
        );
        assert!(
            (left_ink as i32 - right_ink as i32).abs() <= 8,
            "the two chevrons are the same shape ({left_ink} and {right_ink} inked pixels)"
        );

        // The two glyphs are the same shape pointing opposite ways, so the left arrow's ink centre sits left of
        // the right arrow's.
        let centroid_x = |mask: &[bool]| {
            let (sum, count) = mask
                .iter()
                .enumerate()
                .filter(|(_, on)| **on)
                .fold((0usize, 0usize), |(sum, count), (index, _)| {
                    (sum + index % 24, count + 1)
                });
            sum as f32 / count.max(1) as f32
        };
        assert!(
            centroid_x(&left) < centroid_x(&right),
            "the left chevron leans left of the right one ({} vs {})",
            centroid_x(&left),
            centroid_x(&right)
        );

        // And the two are near mirror images of each other (measured: 17 of the 576 pixels differ, so the pair
        // is the same chevron drawn the other way rather than two unrelated glyphs).
        let mirrored_diff = (0..576)
            .filter(|index| {
                let (x, y) = (index % 24, index / 24);
                left[y * 24 + x] != right[y * 24 + 23 - x]
            })
            .count();
        assert!(
            mirrored_diff <= 24,
            "the two chevrons are mirrors ({mirrored_diff} pixels differ)"
        );
        let (_, left_top, _, left_bottom) = glyph_bounds(&left);
        let (_, right_top, _, right_bottom) = glyph_bounds(&right);
        assert!(
            left_top.abs_diff(right_top) <= 1 && left_bottom.abs_diff(right_bottom) <= 1,
            "both chevrons span the same rows ({left_top}..{left_bottom} vs {right_top}..{right_bottom})"
        );
    }

    /// The 24x24 ink mask of a glyph drawn through the real `Icon` pipeline (node, then render, then pixels),
    /// the measurement `the_published_arrow_data_draws_the_same_arrow` makes for the dropdown arrow
    /// (`winia/src/ui/overlay.rs:1970`).
    fn render_glyph(data: &str) -> Vec<bool> {
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            Icon::svg_path(data)
                .tint(Color::BLACK)
                .size(24.0)
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 24.0, 0.0, 24.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((24, 24)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        px.iter().map(|p| p[0] < 128).collect()
    }

    /// The half-open pixel box an ink mask covers, `(left, top, right, bottom)`, or `(0, 0, 0, 0)` when nothing
    /// was drawn.
    fn glyph_bounds(mask: &[bool]) -> (usize, usize, usize, usize) {
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (index, on) in mask.iter().enumerate() {
            if !on {
                continue;
            }
            let (x, y) = (index % 24, index / 24);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
        if left == usize::MAX {
            return (0, 0, 0, 0);
        }
        (left, top, right, bottom)
    }
}
