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

use crate::core::state::State;
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
        let today = model.month_of_millis(init.today_millis.unwrap_or_else(|| model.today_millis()));
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
            year_range: init.year_range,
            locale,
            model,
            selectable_dates: init.selectable_dates,
        }
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
}
