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

use crate::animation::{AnimationSpec, TweenSpec};
use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::layout::{Alignment, Arrangement};
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::alert_dialog::{AlertDialogDefaults, BasicAlertDialog};
use crate::ui::button::Button;
use crate::ui::divider::Divider;
use crate::ui::icon::Icon;
use crate::ui::icon_button::{IconButton, IconButtonSize};
use crate::layout::lazy_column::{LazyColumn, LazyListState, LazyRow};
use crate::layout::components::{Column, Row, Spacer, Stack};
use crate::overlay::ExposedDropdownMenuDefaults;
use crate::ui::scrollbar::LazyScrollbar;
use crate::ui::surface::{Surface, SurfaceBorder};
use crate::effect::LaunchedEffect;
use crate::ui::text::{ProvideTextStyle, Text};
use crate::ui::text_field::{TextField, TextFieldValue};
use crate::ui::text_transformation::{OffsetMapping, TransformedText, VisualTransformation};
use crate::theme::{ThemeColors, WiniaTheme};
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

/// "Today" for a machine whose clock reads `now_millis` and whose zone runs `offset_millis` ahead of
/// UTC — the clock and the zone kept apart so the arithmetic underneath is testable without a
/// calendar.
fn today_at(now_millis: i64, offset_millis: i64) -> i64 {
    canonical_millis(now_millis.saturating_add(offset_millis))
}

/// The system clock, in milliseconds since the epoch — the one place it is read.
///
/// Split out so [`CalendarModel::today_millis`] reads as the composition it is (an instant plus a zone)
/// rather than as arithmetic buried in a clock call, and so the tests can see what has to meet.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// How far the machine's local zone runs ahead of UTC, in milliseconds.///
/// Everything in this module is UTC arithmetic — `canonical_millis` stamps a day boundary and
/// `civil_from_days` reads one back — so the local zone shows up in exactly one place, and this is it.
/// The offset is a whole number of minutes by definition (every zone definition is), which is why
/// nothing here has to model a DST transition: an offset that shifts mid-day moves the whole instant,
/// and the day it lands in is still the local one.
fn local_utc_offset_millis() -> i64 {
    // `time`'s local-offset probe is fallible in principle (a system with no zone configured) and in
    // practice on a handful of exotic targets. UTC is the one answer that is always representable, and
    // it is what this returned before any of this, so a failure degrades to the old behaviour rather
    // than to nonsense.
    time::UtcOffset::current_local_offset()
        .map(|offset| offset.whole_seconds() as i64 * 1000)
        .unwrap_or(0)
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
    /// The twelve abbreviated month names, January first, for the docked picker's compact month
    /// button (the M3 specs docked figure notes "Aug", not "August").
    pub month_names_short: [String; 12],
    /// How the input mode's text field writes and reads a date.
    pub date_input_format: DateInputFormat,
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
        const MONTHS_SHORT: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        Self {
            // Sunday, the first day of the week in the en-US locale material3's samples run in.
            first_day_of_week: 7,
            weekday_names: WEEKDAYS.map(|(full, narrow)| (full.to_string(), narrow.to_string())),
            month_names: MONTHS.map(|name| name.to_string()),
            month_names_short: MONTHS_SHORT.map(|name| name.to_string()),
            date_input_format: DateInputFormat::default(),
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

    /// How the input field writes and reads a date (`getDateInputFormat`,
    /// `CalendarModel.kt:100`).
    pub fn date_input_format(&self) -> &DateInputFormat {
        &self.date_input_format
    }
}

/// The order the input field's three fields appear in, read off the pattern
/// (`DateInputFormat`'s pattern is always exactly one `yyyy`, one `MM` and one
/// `dd`, so the order is the only thing the locale actually decides).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateInputFieldOrder {
    /// `dd/MM/yyyy` — the pattern en-GB and most of Europe use.
    DayMonthYear,
    /// `MM/dd/yyyy` — the pattern en-US uses.
    MonthDayYear,
    /// `yyyy/MM/dd` — the pattern ja-JP and zh-CN use.
    YearMonthDay,
}

/// How the input field writes and reads a date: a pattern over `d`, `M` and `y` plus the one
/// character separating them (`DateInputFormat`, `CalendarModel.kt:277`).
///
/// material3 derives this from the platform locale's best date-time pattern for the `yMd`
/// skeleton. winia has no locale database, so like [`CalendarLocale`]'s names this is data —
/// [`CalendarLocale::default`] is en-US (`MM/dd/yyyy`), and a caller wanting another ordering
/// builds one with [`DateInputFormat::from_pattern`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateInputFormat {
    pattern_with_delimiters: String,
    delimiter: char,
}

impl Default for DateInputFormat {
    fn default() -> Self {
        // The en-US best pattern for the `yMd` skeleton: `MM/dd/yyyy`.
        Self::from_pattern("MM/dd/yyyy").expect("MM/dd/yyyy is a valid input pattern")
    }
}

impl DateInputFormat {
    /// Build a format from a locale pattern such as `M/d/yyyy`, `dd.MM.yyyy` or `yyyy/MM/dd`.
    ///
    /// Cleans the pattern the way material3 does (`datePatternAsInputFormat`,
    /// `CalendarModel.kt:296-315`): drop everything that is not a `d`, `M` or `y` field or a
    /// `/ - .` delimiter, widen each field to two digits for day and month and four for year, and
    /// take the delimiter from the first separator that survives.
    ///
    /// A run of the same letter is **one** field however wide the locale wrote it, so this takes a
    /// locale pattern (`M/d/yyyy`) and an already-normalized one (`MM/dd/yyyy`) alike — a
    /// deviation from Compose's regex, whose `d{1,2}` counts characters and would read the
    /// normalized form as two day fields. Returns `None` when what is left is not exactly one day,
    /// one month and one year, or when no delimiter is present: a pattern without a separator has
    /// nowhere to put one, and the visual transformation keys off the two delimiter offsets.
    ///
    /// Not ported: Compose's `.replace("My", "M/y")` for the Kako locale, whose pattern spells one
    /// combined year-month field. winia has no locale database, so no locale needs it yet.
    pub fn from_pattern(pattern: &str) -> Option<Self> {
        const DELIMITERS: [char; 3] = ['/', '-', '.'];
        let delimiter = pattern.chars().find(|c| DELIMITERS.contains(c))?;

        let mut pattern_with_delimiters = String::with_capacity(10);
        let mut fields: Vec<char> = Vec::with_capacity(3);
        let mut chars = pattern.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                'd' | 'M' | 'y' => {
                    while chars.peek() == Some(&c) {
                        chars.next();
                    }
                    fields.push(c);
                    pattern_with_delimiters.push_str(match c {
                        'd' => "dd",
                        'M' => "MM",
                        _ => "yyyy",
                    });
                }
                c if DELIMITERS.contains(&c) => pattern_with_delimiters.push(c),
                _ => {}
            }
        }
        let names_each_field_once = fields.len() == 3
            && fields.contains(&'d')
            && fields.contains(&'M')
            && fields.contains(&'y');
        if !names_each_field_once {
            return None;
        }
        Some(Self { pattern_with_delimiters, delimiter })
    }

    /// The pattern as the field displays it in its placeholder, `MM/dd/yyyy`.
    pub fn pattern_with_delimiters(&self) -> &str {
        &self.pattern_with_delimiters
    }

    /// The separator, `/` or `-` or `.`.
    pub fn delimiter(&self) -> char {
        self.delimiter
    }

    /// The same pattern with the separators dropped, `MMddyyyy`. This is what the field
    /// actually holds; the separators exist only in the transformed text
    /// (`DateInputFormat.patternWithoutDelimiters`, `CalendarModel.kt:279`).
    pub fn pattern_without_delimiters(&self) -> String {
        self.pattern_with_delimiters.replace(self.delimiter, "")
    }

    /// The digit count a full entry has, always 8.
    pub fn pattern_length(&self) -> usize {
        self.pattern_without_delimiters().len()
    }

    /// Which field comes first, taken from where each letter sits in the pattern.
    pub fn field_order(&self) -> DateInputFieldOrder {
        let pattern = self.pattern_without_delimiters();
        let day = pattern.find('d').unwrap_or(0);
        let month = pattern.find('M').unwrap_or(0);
        if day < month {
            DateInputFieldOrder::DayMonthYear
        } else if pattern.find('y').unwrap_or(0) < day {
            DateInputFieldOrder::YearMonthDay
        } else {
            DateInputFieldOrder::MonthDayYear
        }
    }

    /// The index of the first delimiter in [`DateInputFormat::pattern_with_delimiters`], which
    /// the visual transformation uses to decide where the first separator goes.
    pub fn first_delimiter_offset(&self) -> usize {
        self.pattern_with_delimiters
            .find(self.delimiter)
            .expect("a format is built from a pattern that had a delimiter")
    }

    /// The index of the last delimiter in [`DateInputFormat::pattern_with_delimiters`].
    pub fn last_delimiter_offset(&self) -> usize {
        self.pattern_with_delimiters
            .rfind(self.delimiter)
            .expect("a format is built from a pattern that had a delimiter")
    }
}

/// Writes the format's delimiters into a date entry as it is typed, so the field shows
/// `03/01/2024` while what it holds is the eight digits `03012024`
/// (`DateVisualTransformation`, `DateInput.kt:392-439`).
///
/// The delimiters are inserted as the user types and are never part of the stored text, which is
/// why the offsets have to be translated both ways: a caret sitting after the fifth displayed
/// character is after the fourth typed one.
#[derive(Clone, Debug)]
pub struct DateVisualTransformation {
    /// Where the first delimiter goes in the delimited pattern, `MM/dd/yyyy` → 2.
    first_delimiter_offset: usize,
    /// Where the second goes, `MM/dd/yyyy` → 5.
    second_delimiter_offset: usize,
    /// The digit count, always 8.
    date_format_length: usize,
    delimiter: char,
}

impl DateVisualTransformation {
    /// A transformation for `format`.
    pub fn new(format: &DateInputFormat) -> Self {
        Self {
            first_delimiter_offset: format.first_delimiter_offset(),
            second_delimiter_offset: format.last_delimiter_offset(),
            date_format_length: format.pattern_length(),
            delimiter: format.delimiter(),
        }
    }
}

impl VisualTransformation for DateVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        // A longer entry is cut at a full field's width, which is the same width the field's
        // `onValueChange` accepts; the two never disagree, so this is belt and braces.
        let trimmed = if text.len() > self.date_format_length {
            &text[..self.date_format_length]
        } else {
            text
        };
        let mut transformed = String::with_capacity(self.date_format_length + 2);
        for (index, c) in trimmed.chars().enumerate() {
            transformed.push(c);
            if index + 1 == self.first_delimiter_offset
                || index + 2 == self.second_delimiter_offset
            {
                transformed.push(self.delimiter);
            }
        }
        TransformedText {
            text: transformed,
            offset_mapping: Arc::new(DateOffsetMapping {
                first_delimiter_offset: self.first_delimiter_offset,
                second_delimiter_offset: self.second_delimiter_offset,
                date_format_length: self.date_format_length,
            }),
        }
    }
}

/// Moves a caret between the digits a date entry holds and the digits it shows
/// (`DateVisualTransformation`'s offset translator, `DateInput.kt:401-421`).
#[derive(Clone, Copy, Debug)]
struct DateOffsetMapping {
    first_delimiter_offset: usize,
    second_delimiter_offset: usize,
    date_format_length: usize,
}

impl OffsetMapping for DateOffsetMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        if offset < self.first_delimiter_offset {
            offset
        } else if offset < self.second_delimiter_offset {
            offset + 1
        } else if offset <= self.date_format_length {
            offset + 2
        } else {
            // Past a full entry there is nothing more to show; clamp rather than run off the end.
            self.date_format_length + 2
        }
    }

    /// Deliberate deviation from `DateInput.kt:413-420`, and the whole point of the round-trip
    /// test. Compose's branches are `<= firstDelimiterOffset - 1` and `<= secondDelimiterOffset - 1`,
    /// which is one too tight: for `MM/dd/yyyy` it sends displayed offset 5 — the caret just after
    /// `01` — back to 3, while its own forward map puts typed offset 4 at displayed 5. Typing the
    /// day therefore moves the caret one character back as soon as the caret crosses the second
    /// delimiter. Using `<= firstDelimiterOffset` and `<= secondDelimiterOffset` makes each side
    /// the inverse of the other for every position, and puts both sides of a delimiter — which is
    /// zero-width in the stored text — on the same offset, which is where a caret there belongs.
    ///
    /// Forward is unchanged; it already inverts correctly.
    fn transformed_to_original(&self, offset: usize) -> usize {
        if offset <= self.first_delimiter_offset {
            offset
        } else if offset <= self.second_delimiter_offset {
            offset - 1
        } else if offset <= self.date_format_length + 1 {
            offset - 2
        } else {
            self.date_format_length
        }
    }
}

/// Material Icons `edit` (24 dp) — the mode toggle while the calendar is showing
/// (`DisplayModeToggleButton`, `DatePicker.kt:1413`).
pub const EDIT_PATH: &str = "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a.996.996 0 0 0 0-1.41l-2.34-2.34a.996.996 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z";

/// Material Icons `date_range` (24 dp) — the mode toggle while the text field is showing
/// (`DatePicker.kt:1420`).
pub const DATE_RANGE_PATH: &str = "M9 11H7v2h2v-2zm4 0h-2v2h2v-2zm4 0h-2v2h2v-2zm2-7h-1V2h-2v2H8V2H6v2H5c-1.11 0-1.99.9-1.99 2L3 20a2 2 0 0 0 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V9h14v11z";

/// The input field's padding, 24 dp at each end (`InputTextFieldPadding`, `DateInput.kt:441`).
pub const INPUT_TEXT_FIELD_PADDING: f32 = 24.0;

/// How long the entry field waits before taking focus for itself, material3's
/// `MotionTokens.DurationMedium2` (`DateInput.kt:259-266`). Long enough for the picker's own
/// entrance motion to have finished, so the caret does not arrive mid-animation.
pub const MODAL_MODE_SWITCH_FOCUS_DELAY_MS: u64 = 300;

/// The test tag on the date entry field, so a UI test can click and type into it without having to
/// find the field by its geometry.
pub const INPUT_FIELD_TEST_TAG: &str = "date-picker-input-field";

/// The test tag on the mode toggle in the header.
pub const MODE_TOGGLE_TEST_TAG: &str = "date-picker-mode-toggle";

/// The bottom padding the field carries only while no error is showing, so an error appearing as
/// supporting text does not make the container jump (`InputTextNonErroneousBottomPadding`,
/// `DateInput.kt:445`).
pub const INPUT_TEXT_NON_ERROROUS_BOTTOM_PADDING: f32 = 16.0;

/// The modal date picker's text entry half: one outlined field that takes the date as digits and
/// offers the locale's pattern as its placeholder (`DateInputContent`, `DateInput.kt:59-113`).
///
/// The field holds the eight digits with no delimiters — [`DateVisualTransformation`] is what puts
/// them on screen — and [`DatePickerState::validate_date_input`] is what decides whether an entry
/// may become the selection. An entry that is not complete, or that fails a check, leaves the
/// selection empty rather than committing something the field itself calls wrong.
#[composable]
pub fn date_input_content(ctx: &mut ComposeCtx, state: &DatePickerState) {
    let model = state.calendar_model().clone();
    let format = model.date_input_format().clone();
    let pattern = format.pattern_with_delimiters().to_uppercase();
    let selected = state.selected_date_millis();

    let text: State<TextFieldValue> = ctx.remember(|| TextFieldValue::new(""));
    let error: State<String> = ctx.remember(|| String::new());

    // A selection made outside the field — the calendar, the initial value — rewrites the digits,
    // exactly as `LaunchedEffect(initialDateMillis)` does (`DateInput.kt:238-258`).
    let effect_selected = selected;
    let effect_model = model.clone();
    let effect_format = format.clone();
    let effect_text = text.clone();
    let effect_error = error.clone();
    LaunchedEffect::new(selected).build(ctx, move |_scope| {
        async move {
            let Some(millis) = effect_selected else { return };
            // `TextFieldValue::new` puts the caret at the end, which is the right place for a value
            // this field just filled in wholesale.
            effect_text.set(TextFieldValue::new(effect_model.format_with_pattern(millis, &effect_format)));
            effect_error.set(String::new());
        }
    });

    // The field asks for focus once it is showing, but not straight away: the picker has just
    // animated in, and a caret that arrives mid-motion is a caret the user never chose. Compose
    // waits `MotionTokens.DurationMedium2` for the same reason
    // (`LaunchedEffect(Unit) { delay(...); focusRequester?.requestFocus() }`, `DateInput.kt:259-266`).
    let focus = ctx.remember(|| crate::modifier::FocusRequester::new()).get();
    let effect_focus = focus.clone();
    LaunchedEffect::new(()).build(ctx, move |_scope| {
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(
                MODAL_MODE_SWITCH_FOCUS_DELAY_MS,
            ))
            .await;
            effect_focus.request_focus();
        }
    });

    // Anything that is not a digit, or that runs past a full entry, is refused outright: the field
    // keeps what it had (`DateInput.kt:166-169`). A shorter entry clears the error and empties the
    // selection without being judged; a full one is parsed and judged, and only commits if it
    // passes (`DateInput.kt:171-200`).
    let on_value_change_model = model.clone();
    let on_value_change_format = format.clone();
    let on_value_change_text = text.clone();
    let on_value_change_error = error.clone();
    let on_value_change_state = state.clone();

    let message = error.get();
    let is_error = !message.trim().is_empty();

    let transformation: Arc<dyn VisualTransformation> =
        Arc::new(DateVisualTransformation::new(&format));

    // The message is the field's error semantics as well as its supporting text, so a reader is
    // told what is wrong rather than left to find it on the screen (`DateInput.kt:211-214`).
    let field_semantics = if is_error {
        crate::semantics::SemanticsConfig::new()
            .state(crate::semantics::SemanticsState::new().error(message.clone()))
    } else {
        crate::semantics::SemanticsConfig::new()
    };

    let label_pattern = pattern.clone();
        let mut field = TextField::new(text.clone())
        .outlined()
        .single_line(true)
        .visual_transformation(transformation)
        .label(move |ctx| {
            Text::new(DATE_INPUT_LABEL)
                .modifier(Modifier::new().semantics(
                    crate::semantics::SemanticsConfig::new()
                        // The label names the field and the shape it wants, so a reader says what to
                        // type before the user types it (`DateInput.kt:93-98`).
                        .content_description(format!("{DATE_INPUT_LABEL}, {label_pattern}")),
                ))
                .build(ctx);
        })
        .placeholder(move |ctx| {
            Text::new(pattern.clone()).build(ctx);
        })
        .on_value_change(move |value| {
            let digits = value.text.trim().to_string();
            let width = on_value_change_format.pattern_length();
            if digits.len() > width || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return;
            }
            on_value_change_text.set(value);
            if digits.is_empty() || digits.len() < width {
                on_value_change_error.set(String::new());
                on_value_change_state.set_selected_date_millis(None);
                return;
            }
            let parsed = on_value_change_model.parse(&digits, &on_value_change_format);
            let message = on_value_change_state.validate_date_input(parsed);
            on_value_change_error.set(message.clone());
            // Commit only what the validator passed, so the calendar never shows a date the field
            // is currently calling wrong.
            let millis = if message.is_empty() { parsed.map(|d| d.start_of_day_millis()) } else { None };
            on_value_change_state.set_selected_date_millis(millis);
        })
        .is_error(is_error);
    if is_error {
        field = field.supporting_text(message);
    }
    // The error's own line of supporting text brings its own padding, so the bottom padding is only
    // there to keep the container the same height either way (`DateInput.kt:152-162`).
    let bottom = if is_error { 0.0 } else { INPUT_TEXT_NON_ERROROUS_BOTTOM_PADDING };
    field
        .modifier(
            Modifier::new()
                .test_tag(INPUT_FIELD_TEST_TAG)
                .semantics(field_semantics)
                .focus_requester(focus.clone())
                .padding_start(INPUT_TEXT_FIELD_PADDING)
                .padding_end(INPUT_TEXT_FIELD_PADDING)
                .padding_bottom(bottom),
        )
        .build(ctx);
}

/// The button that moves between the calendar and the text field
/// (`DisplayModeToggleButton`, `DatePicker.kt:1401-1425`).
///
/// Neither icon is auto-mirrored: material3 gives `Icon` no `autoMirrored`, and the pair means
/// "edit" and "calendar" rather than a direction.
fn display_mode_toggle(
    ctx: &mut ComposeCtx,
    display_mode: DisplayMode,
    on_toggle: impl Fn() + Send + Sync + 'static,
    color: Color,
) {
    let (path, description) = match display_mode {
        DisplayMode::Picker => (EDIT_PATH, SWITCH_TO_INPUT_MODE_DESCRIPTION),
        DisplayMode::Input => (DATE_RANGE_PATH, SWITCH_TO_CALENDAR_MODE_DESCRIPTION),
    };
    IconButton::new()
        .on_click(on_toggle)
        .modifier(Modifier::new().test_tag(MODE_TOGGLE_TEST_TAG))
        .build(ctx, |ctx| {
            Icon::svg_path(path)
                .tint(color)
                .modifier(Modifier::new().semantics(
                    crate::semantics::SemanticsConfig::new().content_description(description),
                ))
                .build(ctx);
        });
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

    /// The locale's input format (`getDateInputFormat`, `CalendarModel.kt:100`).
    pub fn date_input_format(&self) -> &DateInputFormat {
        self.locale.date_input_format()
    }

    /// Read `digits` — the delimiter-free pattern's text — as a date, or `None` when it is not
    /// one (`CalendarModel.parse`, `CalendarModel.kt:209`).
    ///
    /// `digits` holds no separators and, by the caller in
    /// [`crate::ui::date_picker`], exactly [`DateInputFormat::pattern_length`] of them; anything
    /// shorter or longer is not a complete entry and has no answer here. The fields are cut at
    /// the positions the pattern puts them in, so `dd/MM/yyyy` and `yyyy/MM/dd` read the same
    /// digits into different dates.
    pub fn parse(&self, digits: &str, format: &DateInputFormat) -> Option<CalendarDate> {
        if digits.len() != format.pattern_length() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let order = format.field_order();
        let (year_digits, month, day) = match order {
            DateInputFieldOrder::MonthDayYear => {
                (&digits[4..8], digits[0..2].parse().ok()?, digits[2..4].parse().ok()?)
            }
            DateInputFieldOrder::DayMonthYear => {
                (&digits[4..8], digits[2..4].parse().ok()?, digits[0..2].parse().ok()?)
            }
            DateInputFieldOrder::YearMonthDay => {
                (&digits[0..4], digits[4..6].parse().ok()?, digits[6..8].parse().ok()?)
            }
        };
        let year: i32 = year_digits.parse().ok()?;
        CalendarDate::new(year, month, day)
    }

    /// Write `millis` as the digits a full entry holds, in the format's field order
    /// (`CalendarModel.formatWithPattern`, `CalendarModel.kt:195`).
    ///
    /// The result is the delimiter-free text the field stores; the separators are added by the
    /// visual transformation on the way to the screen.
    pub fn format_with_pattern(&self, millis: i64, format: &DateInputFormat) -> String {
        let date = self.canonical_date(millis);
        let month = format!("{:02}", date.month);
        let day = format!("{:02}", date.day);
        let year = format!("{:04}", date.year);
        match format.field_order() {
            DateInputFieldOrder::MonthDayYear => format!("{month}{day}{year}"),
            DateInputFieldOrder::DayMonthYear => format!("{day}{month}{year}"),
            DateInputFieldOrder::YearMonthDay => format!("{year}{month}{day}"),
        }
    }

    /// Today at the start of its UTC day, from the system clock — material3's `CalendarModel.today`
    /// (`internal/CalendarModelImpl.android.kt:48-62`; the clock itself is read at `:50`).
    ///
    /// Compose reads `LocalDate.now()` and only THEN stamps it at UTC midnight
    /// (`.atTime(MIDNIGHT).atZone(utcTimeZoneId)`), so the DATE it rings is the user's local calendar
    /// date while the timestamp it stores is a UTC one. Taking the UTC instant at face value — which is
    /// what this used to do — rings the wrong cell for every user whose local date has already rolled
    /// over: at UTC+8 that is the eight hours after local midnight, when the picker would open on the
    /// previous day. So the local offset goes in first, and `canonical_millis` then does the UTC
    /// stamping exactly as it always did.
    pub fn today_millis(&self) -> i64 {
        today_at(now_millis(), local_utc_offset_millis())
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
    /// A `State`, because the policy is LIVE: Compose's `rememberDatePickerState` ends with
    /// `rememberSaveable(...).apply { this.selectableDates = selectableDates }`
    /// (`DatePicker.kt:384-389`, where the property is `by mutableStateOf`), so a caller whose
    /// policy changed between compositions has it taken up without recreating the state. The
    /// initial values are the ones taken once.
    selectable_dates: State<SelectableDatesHandle>,
}

/// A state's policy handle.
///
/// `State::set` dedups on `PartialEq`, and a caller hands the SAME `Arc` back every composition, so
/// comparing by pointer says "unchanged" without calling into the user's [`SelectableDates`] — and a
/// fresh allocation says "changed", which is what Compose's `mutableStateOf` does for a policy
/// object with no `equals` of its own.
#[derive(Clone)]
pub struct SelectableDatesHandle(Arc<dyn SelectableDates>);

impl PartialEq for SelectableDatesHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
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
            selectable_dates: State::new(SelectableDatesHandle(init.selectable_dates)),
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
    /// for it (`internal/CalendarModelImpl.android.kt:48-62`); a caller may pin it, and the tests do.
    pub fn today_millis(&self) -> i64 {
        self.today_millis
    }

    /// Shows `year`, keeping the displayed month (`MonthPicker`'s `onYearSelected`, `DatePicker.kt:1643-1652`):
    /// material3 scrolls its month list to `(year - first) * 12 + month - 1` and lets the list write the
    /// displayed month back, winia shows one month at a time and writes it here. A year outside the year range
    /// is ignored.
    pub fn set_displayed_year(&self, year: i32) {
        if !self.year_range.contains(&year) {
            return;
        }
        let model = self.calendar_model();
        let current = model.month_of_millis(self.displayed_month_millis());
        let month = model.month_of(year, current.month);
        self.set_displayed_month_millis(month.start_utc_time_millis);
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

    /// Which dates and years are selectable, as the live policy stands now.
    pub fn selectable_dates(&self) -> Arc<dyn SelectableDates> {
        self.selectable_dates.get().0
    }

    /// Replaces the policy — what `remember_date_picker_state` does on every composition, the way
    /// Compose's `apply { this.selectableDates = selectableDates }` does.
    pub fn set_selectable_dates(&self, policy: Arc<dyn SelectableDates>) {
        self.selectable_dates.set(SelectableDatesHandle(policy));
    }

    /// Run the input field's checks on what the user typed, returning the message to show under the
    /// field, or an empty string when the entry is good (`DateInputValidator.validate`,
    /// `DateInput.kt:313-357`).
    ///
    /// The three checks are in material3's order, and the order is load-bearing: a complete entry
    /// that names no date reports the pattern, a named date outside `year_range` reports the range
    /// rather than the policy, and only a date the range accepts is put to the policy. An empty
    /// return means the selection may be committed.
    ///
    /// `date` is [`CalendarModel::parse`]'s answer for the typed digits, so `None` covers both an
    /// incomplete entry and one that names no real date — material3 cannot tell those apart here
    /// either, because the caller only reaches this once the entry is a full field's width.
    pub fn validate_date_input(&self, date: Option<CalendarDate>) -> String {
        let Some(date) = date else {
            return format_string(
                DATE_INPUT_INVALID_FOR_PATTERN,
                &[&self
                    .model
                    .date_input_format()
                    .pattern_with_delimiters()
                    .to_uppercase()],
            );
        };
        if !self.year_range.contains(&date.year) {
            return format_string(
                DATE_INPUT_INVALID_YEAR_RANGE,
                &[&self.year_range.start().to_string(), &self.year_range.end().to_string()],
            );
        }
        let policy = self.selectable_dates.get().0;
        if !policy.is_selectable_year(date.year)
            || !policy.is_selectable_date(date.start_of_day_millis())
        {
            return format_string(
                DATE_INPUT_INVALID_NOT_ALLOWED,
                &[&self.model.format_date(date.start_of_day_millis(), false)],
            );
        }
        String::new()
    }
}

/// The state a picker composes with, remembered across recompositions
/// (`rememberDatePickerState`, `DatePicker.kt:368-390`).
pub fn remember_date_picker_state(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    locale: CalendarLocale,
    init: DatePickerStateInit,
) -> DatePickerState {
    // Compose's `rememberDatePickerState` takes the five parameters this reads out of `init`
    // (`DatePicker.kt:368-374`) — the locale is the one part of winia's signature with no Compose
    // counterpart, because Compose takes it from the platform.
    //
    // The `.apply` at the end of Compose's version is not decoration: the initial values are taken
    // once, but the SELECTABLE DATES are written back on every composition, so a caller whose policy
    // changed has it taken up without recreating the state. Hence the write after the `remember`.
    let state = ctx.remember(|| DatePickerState::with(locale, init.clone())).get();
    state.set_selectable_dates(init.selectable_dates);
    state
}

/// The rows a month grid always draws, whether or not the month needs them (`MaxCalendarRows`,
/// `DatePicker.kt:2303`).
pub const MAX_CALENDAR_ROWS: u32 = 6;

/// The word a today cell announces (`DatePickerTodayDescription`).
pub const TODAY_DESCRIPTION: &str = "Today";

/// The mode toggle's content description while the calendar is showing, which is the button that
/// moves to text entry (`m3c_date_picker_switch_to_input_mode`).
pub const SWITCH_TO_INPUT_MODE_DESCRIPTION: &str = "Switch to text input mode";

/// The mode toggle's content description while the text field is showing
/// (`m3c_date_picker_switch_to_calendar_mode`).
pub const SWITCH_TO_CALENDAR_MODE_DESCRIPTION: &str = "Switch to calendar input mode";

/// The input field's floating label (`m3c_date_input_label`).
pub const DATE_INPUT_LABEL: &str = "Date";

/// The header headline while the input field is showing and nothing is entered
/// (`m3c_date_input_headline`).
pub const DATE_INPUT_HEADLINE: &str = "Entered date";

/// What the header headline announces while the input field is showing, given what is entered
/// (`m3c_date_input_headline_description`).
pub const DATE_INPUT_HEADLINE_DESCRIPTION: &str = "Entered date: {1}";

/// What the header headline announces while the input field is showing and nothing is entered
/// (`m3c_date_input_no_input_description`).
pub const DATE_INPUT_NO_INPUT_DESCRIPTION: &str = "None";

/// The entry is complete but names no date (`m3c_date_input_invalid_for_pattern`).
pub const DATE_INPUT_INVALID_FOR_PATTERN: &str = "Date does not match expected pattern: {1}";

/// The entry's year falls outside the picker's year range (`m3c_date_input_invalid_year_range`).
pub const DATE_INPUT_INVALID_YEAR_RANGE: &str = "Date out of expected year range {1} - {2}";

/// The entry is a date the policy refuses (`m3c_date_input_invalid_not_allowed`).
pub const DATE_INPUT_INVALID_NOT_ALLOWED: &str = "Date not allowed: {1}";

/// Substitute `{1}`, `{2}`, … from `args` in order — what Kotlin's `String.format` does, and
/// therefore what material3's `formatString` calls do (`DateInput.kt:319`).
///
/// The placeholders are one-based and numbered because the templates come from a string resource,
/// where they are `%1$s` and it is the resource system, not the caller, that decides the order. A
/// bare `{}` spelling would not survive two arguments: `replace` rewrites every occurrence alike,
/// so `{} - {}` filled from one value comes out as `1900 - 1900`.
fn format_string(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        match after[..close].parse::<usize>() {
            Ok(index) => out.push_str(args.get(index.wrapping_sub(1)).copied().unwrap_or("")),
            // Not a placeholder: copy the brace pair through untouched. `close` indexes into
            // `after`, which starts one past the `{`, so the `}` sits one further along again.
            Err(_) => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

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
    /// (`DatePicker.kt:294-298`). Always `false` for an outside-month cell — those are context, not a way to
    /// reach another month (see [`MonthGrid`]).
    pub is_enabled: bool,
    /// Whether the day belongs to the neighbouring month rather than the displayed one.
    ///
    /// A divergence from material3 in the DOCKED picker, and only there. Compose composes a `Spacer` in
    /// these cells and has no colour role for them (`DatePicker.kt:1870-1890`), but the M3 specs page draws
    /// the two variants differently: the *Docked date picker* anatomy lists "Outside month date" among the
    /// grid's states and gives it its own tokens, while the *Modal date picker* anatomy has no such entry.
    /// So the docked picker draws these days and the modal one leaves the slots empty; the model computes
    /// them either way, because the dates are true regardless of who draws them.
    pub is_outside_month: bool,
}

/// A month laid out the way the picker draws it: [`MAX_CALENDAR_ROWS`] rows of [`DAYS_IN_WEEK`] cells.
///
/// Every cell carries a day. A month that does not start on the first day of the week, or does not end on
/// the last, is padded with the days of the month before or after it, so the grid is always six full rows.
///
/// material3 leaves those cells empty (`Month`, `DatePicker.kt:1856-1890`) and so does winia's modal
/// picker, which is what the M3 specs' modal anatomy shows. Only the docked picker draws them — see
/// [`DayCell::is_outside_month`] and `month_grid`'s `show_outside_month`. Outside-month days are drawn as
/// context and are never selectable: `is_enabled` is `false` for them whatever `SelectableDates` says, so
/// tapping one cannot move the selection into a month the grid is not showing.
#[derive(Clone, Debug)]
pub struct MonthGrid {
    month: CalendarMonth,
    cells: Vec<DayCell>,
}

impl MonthGrid {
    /// Lays `month` out for `selection` and `today_millis`, asking `selectable_dates` about every day.
    pub fn of(
        month: CalendarMonth,
        selection: Option<i64>,
        today_millis: i64,
        selectable_dates: &dyn SelectableDates,
    ) -> Self {
        let offset = month.days_from_start_of_week_to_first_of_month as i64;
        let end = offset + month.number_of_days as i64;
        let year_selectable = selectable_dates.is_selectable_year(month.year);
        let mut cells = Vec::with_capacity((MAX_CALENDAR_ROWS * DAYS_IN_WEEK) as usize);
        for index in 0..(MAX_CALENDAR_ROWS * DAYS_IN_WEEK) as i64 {
            // The month start is the 1st at 00:00 UTC, so a signed day offset walks back into the previous
            // month and forward into the next one on its own — `date_of_millis` resolves the day number.
            // material3 adds the same product for its in-month cells (`DatePicker.kt:1893-1894`).
            let utc_time_millis =
                month.start_utc_time_millis + (index - offset) * MILLIS_IN_24_HOURS;
            let date = date_of_millis(utc_time_millis);
            let is_outside_month = index < offset || index >= end;
            cells.push(DayCell {
                day: date.day,
                utc_time_millis,
                is_today: utc_time_millis == today_millis,
                is_selected: selection == Some(utc_time_millis),
                is_enabled: !is_outside_month
                    && year_selectable
                    && selectable_dates.is_selectable_date(utc_time_millis),
                is_outside_month,
            });
        }
        Self { month, cells }
    }

    /// The month this grid lays out.
    pub fn month(&self) -> CalendarMonth {
        self.month
    }

    /// Every cell, in reading order.
    pub fn cells(&self) -> &[DayCell] {
        &self.cells
    }

    /// The grid row by row, [`DAYS_IN_WEEK`] cells each.
    pub fn rows(&self) -> impl Iterator<Item = &[DayCell]> {
        self.cells.chunks(DAYS_IN_WEEK as usize)
    }

    /// The cell at `row` and `column`, counting from zero.
    pub fn cell(&self, row: u32, column: u32) -> Option<&DayCell> {
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

/// The `test_tag` on the menu button shared by `DatePicker` and the docked picker, so a UI test can tap
/// the control that opens the year/month panel. Docked composes two buttons: pass a distinct tag per group.
const YEAR_MENU_TAG: &str = "dp-year-menu";

/// The docked month menu button's tag (`dp-month-menu`), distinct from the year groups'.
const MONTH_MENU_TAG: &str = "dp-month-menu";

/// The prefix of a year cell's `test_tag` — `dp-year-2024` — so a UI test can find one year's box.
const YEAR_CELL_TAG_PREFIX: &str = "dp-year-";

/// The prefix of a month cell's `test_tag` in the docked month list — `dp-month-9` — matching
/// [`YEAR_CELL_TAG_PREFIX`]'s scheme so a UI test can find one month's pill.
const MONTH_CELL_TAG_PREFIX: &str = "dp-month-";

/// Material Icons `keyboard_arrow_left` (24 dp), the glyph material3 auto-mirrors for its month arrows
/// (`internal/Icons.kt:34`). Provenance and the measured guard: `docs/date-picker.md`.
pub const CHEVRON_LEFT_PATH: &str = "M15.41 16.09l-4.58-4.59 4.58-4.59L14 5.5l-6 6 6 6z";

/// Material Icons `keyboard_arrow_right` (24 dp, `internal/Icons.kt:60`).
pub const CHEVRON_RIGHT_PATH: &str = "M8.59 16.59L13.17 12 8.59 7.41 10 6l6 6-6 6z";

/// The modal date picker: a title and a headline over a month calendar
/// (`DatePicker`, `DatePicker.kt:168-237`).
///
/// ```ignore
/// let state = remember_date_picker_state(ctx, CalendarLocale::default(), DatePickerStateInit::default());
/// DatePicker::new(state).build(ctx);
/// ```
///
/// Deliberate deviations: the mode toggle arrives with the input mode. (The months ARE paged, in a
/// `LazyRow` with a snap fling, exactly as material3 does — `month_pages`.)
pub struct DatePicker {
    state: DatePickerState,
    title: Option<String>,
    /// Whether `title` is still material3's default. Compose hands the title a lambda that names the
    /// mode (`title = { if (displayMode == Input) DateInputTitle else DatePickerTitle }`,
    /// `DatePicker.kt:654`), so the default follows the mode while a caller's own title does not.
    title_is_default: bool,
    /// The colour roles, read from the theme when absent. [`DatePickerDialog`] forwards its own set here
    /// for its default content — a winia convenience, since material3's dialog uses its `colors` only for
    /// its own surface (`DatePickerDialog.android.kt:86`) and invokes the caller's slot with nothing
    /// (`:95`).
    colors: Option<DatePickerColors>,
    modifier: Modifier,
}

impl DatePicker {
    /// A picker over `state`, with material3's default title.
    pub fn new(state: DatePickerState) -> Self {
        Self {
            state,
            title: Some(DatePickerDefaults::TITLE.to_string()),
            title_is_default: true,
            colors: None,
            modifier: Modifier::new(),
        }
    }

    /// The picker's colour roles (`colors`), read from the theme by default.
    ///
    /// Material3 has no such parameter on `DatePicker` itself — the colours are whatever the composable
    /// that built it passed (`DatePicker.kt:172` is its own parameter default) — but winia needs the
    /// setter so [`DatePickerDialog`] can hand its set down. The gap it closes was visible: the dialog
    /// painted its surface from `colors.container` and then let the default content re-derive a fresh
    /// set from the theme, so an overridden dialog got a calendar in the theme's colours inside a
    /// container in the caller's.
    ///
    /// This is a winia convenience, not a port of Compose's wiring — see [`DatePickerDialog::build`].
    pub fn colors(mut self, colors: DatePickerColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// The title above the headline. `None` drops the title slot, and with it the header's minimum height and
    /// the divider (`DatePicker.kt:1680-1685`, `:1392-1394`).
    pub fn title(mut self, title: Option<impl Into<String>>) -> Self {
        self.title = title.map(Into::into);
        self.title_is_default = false;
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
        let colors = self.colors.unwrap_or_else(|| DatePickerColors::from_theme(&WiniaTheme::colors()));
        let model = self.state.calendar_model().clone();
        let month = model.month_of_millis(self.state.displayed_month_millis());
        let state = self.state.clone();
        let title = self.title.clone();
        let title_is_default = self.title_is_default;
        // material3 keeps the year panel's visibility in a `rememberSaveable` inside the picker
        // (`DatePicker.kt:1557`). winia keeps it in a remembered `State`, so a toggle recomposes the picker.
        let year_panel_open = ctx.remember(|| false);
        // The panel's row list, which the toggle scrolls to the row above the displayed year.
        let year_rows = ctx.remember(LazyListState::new).get();
        // The paged month list. Its position IS the displayed month (see `sync_month_pages`), so the
        // arrows animate it rather than writing the month — the same two-way sync material3 has
        // between `monthsListState` and `displayedMonthMillis` (`DatePicker.kt:1541-1592`). It is
        // seeded by that sync's `scroll_to_item`, which is the anchor-authoritative jump: a
        // constructor cannot do it, because the pixel offset it needs comes from the measure.
        let month_rows = ctx.remember(LazyListState::new).get();
        // The page the month arrows have already asked for but the list has not reached, so a press
        // during the animation continues from it instead of restating it (see `arrow_target`).
        let month_step_in_flight = ctx.remember(|| None::<usize>);
        // Page 0 is the January of the range's first year, so a page index converts to a month by
        // counting from here — the same reference `month_pages` composes against.
        let first_month = model.month_of(*state.year_range().start(), 1).start_utc_time_millis;
        // `SwitchableDateEntryContent` reads the mode here and hands the toggle to the header
        // (`DatePicker.kt:1432`, `:1467-1480`). Reading it registers this slot as a reader, so the
        // calendar and the text field replace each other rather than both being composed.
        let display_mode = state.display_mode();
        let on_toggle_display_mode = {
            let state = state.clone();
            move || {
                let next = match state.display_mode() {
                    DisplayMode::Picker => DisplayMode::Input,
                    DisplayMode::Input => DisplayMode::Picker,
                };
                state.set_display_mode(next);
            }
        };
        let on_toggle_year_panel = {
            let year_panel_open = year_panel_open.clone();
            let year_rows = year_rows.clone();
            let state = state.clone();
            let model = model.clone();
            move || {
                let open = !year_panel_open.get();
                if open {
                    year_rows.scroll_to_item(year_panel_first_row(&state, &model), 0.0);
                }
                year_panel_open.set(open);
            }
        };
        // material3's `onYearSelected` both shows the picked year and closes the panel (`DatePicker.kt:1643-1652`).
        let on_year_selected = {
            let year_panel_open = year_panel_open.clone();
            let state = state.clone();
            move |year: i32| {
                state.set_displayed_year(year);
                year_panel_open.set(false);
            }
        };
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
                // Compose's default title names the mode it is asking about (`DatePicker.kt:654`), so the
                // default follows the mode while a title the caller supplied stands on its own.
                let title = if title_is_default && display_mode == DisplayMode::Input {
                    Some(DatePickerDefaults::INPUT_TITLE.to_string())
                } else {
                    title.clone()
                };
                header(ctx, &state, title.as_deref(), &colors, display_mode, on_toggle_display_mode);
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding_horizontal(DatePickerDefaults::HORIZONTAL_PADDING),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        // `SwitchableDateEntryContent` picks between the two by the state's display
                        // mode (`DatePicker.kt:1432`). Before this, `set_display_mode(DisplayMode::Input)`
                        // stored a value nothing read and the modal picker stayed a calendar; the
                        // mode toggle in the header is what reaches it now.
                        if display_mode == DisplayMode::Input {
                            date_input_content(ctx, &state);
                            return;
                        }
                        let open = year_panel_open.get();
                        sync_month_pages(ctx, &state, &model, &month_rows, first_month, &month_step_in_flight);
                        months_navigation(
                            ctx,
                            &state,
                            &month,
                            open,
                            on_toggle_year_panel,
                            &colors,
                            &month_rows,
                            &month_step_in_flight,
                        );
                        if open {
                            year_panel(ctx, &state, &model, &colors, &year_rows, on_year_selected);
                        } else {
                            weekday_row(ctx, &model, &colors);
                            // The modal picker leaves the neighbouring month's slots empty, as material3
                            // does and as the M3 specs' modal anatomy has no state for.
                            month_pages(ctx, &state, &model, &colors, &month_rows, false);
                        }
                    });
            });
    }
}

/// The paged month list (material3's `HorizontalMonthsList`, `DatePicker.kt:1700-1761`): one page per
/// month across the whole year range — `(last - first + 1) * 12`, 2412 for the default range — so the
/// calendar can be swiped. material3's items are `Box(fillParentMaxWidth())`, which winia reaches with
/// [`crate::layout::LazyRow::fill_items`], and its `rememberSnapFlingBehavior` with
/// [`crate::layout::LazyRow::snap_paging`], so a fast swipe settles on a whole month.
///
/// `show_outside_month` is what separates the two variants: the docked picker draws the neighbouring
/// month's days in the padding cells, the modal one leaves them empty (see [`MonthGrid`]).
fn month_pages(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    colors: &DatePickerColors,
    list: &LazyListState,
    show_outside_month: bool,
) {
    let range = state.year_range();
    let first_month = model.month_of(*range.start(), 1).start_utc_time_millis;
    let page_count = CalendarModel::number_of_months_in_range(&range) as usize;
    let today = state.today_millis();
    let selected = state.selected_date_millis();
    let selectable = state.selectable_dates();
    let list_state = list.clone();
    let list_model = model.clone();
    let list_state_for_pages = state.clone();
    let list_colors = colors.clone();

    LazyRow::new()
        .state(list_state)
        .fill_items(true)
        .snap_paging(true)
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_HEIGHT),
        )
        .items(page_count, |index: usize| index as u64, move |ctx, index| {
            let month = list_model.plus_months(first_month, index as i64);
            let grid = MonthGrid::of(month, selected, today, selectable.as_ref());
            month_grid(ctx, &list_state_for_pages, &list_model, &grid, &list_colors, show_outside_month);
        })
        .build(ctx);
}

/// How long [`sync_month_pages`] will wait on something before it stops waiting: for a jump it issued
/// to be picked up, or for a scroll in flight to finish first.
///
/// ~1 s at 60 Hz, longer than any settle or spring. It is a module-level constant rather than a local
/// so the test can name the same number the code uses — see
/// `a_stuck_scroll_flag_cannot_freeze_the_month_sync_forever`, which is the guard on this bound.
const MAX_WAIT_FRAMES: u8 = 60;

/// material3's two-way sync between the page list and `displayedMonthMillis`, in one place:
/// `LaunchedEffect(monthIndex)` scrolls the list when the month changes from OUTSIDE it
/// (`DatePicker.kt:1544-1554`), and `snapshotFlow { firstVisibleItemIndex }` writes the list's page
/// back into the month (`DatePicker.kt:1763-1779`).
///
/// There is no separate "current page" state: the list's position IS the displayed month, and the
/// arrows animate the list rather than writing the month themselves. That is what keeps a swipe and
/// an arrow press indistinguishable, and what lets arrow enablement come from the list
/// (`canScrollForward` / `canScrollBackward`) instead of being recomputed from the year range.
///
/// The one piece of memory is `published` — the month THIS function last wrote into the state. That is
/// what makes the two directions distinguishable, and it has to be that rather than "did the month
/// change since last frame": `displayed_month_millis` is read at the top of `build`
/// (`DatePicker::build`), so on the frame after a write it still carries the value from BEFORE it, one
/// frame behind the list. Comparing against a lagging value makes each direction react to the other's
/// past and the two chase each other — measured on a month arrow, the picker ping-ponged 1500 ⇄ 1520
/// forever, one `scroll_to_item` per frame, each cancelling the animation the last one started.
///
/// `pending` holds the page an outside change asked for, so the list side stays quiet until the list
/// actually arrives there instead of publishing the page it is still leaving.
fn sync_month_pages(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    list: &LazyListState,
    first_month: i64,
    step_in_flight: &State<Option<usize>>,
) {
    let range = state.year_range();
    let page_of = |month_millis: i64| {
        model
            .month_of_millis(month_millis)
            .index_in(&range)
            .max(0) as usize
    };
    let month_of = |page: usize| model.plus_months(first_month, page as i64).start_utc_time_millis;

    let published = ctx.remember(|| i64::MIN);
    let pending = ctx.remember(|| None::<usize>);
    // Consecutive frames spent WAITING — either for a jump this function issued to be picked up, or for
    // a scroll in flight to finish before the next one.
    //
    // Both waits are bounded, and that is not defensive padding. `is_scrolling()` is the same state the
    // drag, fling, programmatic-jump and cancellation paths all write
    // (`lazy_column.rs:997,1028` hand it to the list's `ScrollState`), and two of those can leave it set:
    // `cancel_animation_by_id` and `clear_animations_for_states` drop an animation with `retain`
    // (`animation.rs:580-583`, `:43-49`), never running its finish callback, and both `drag_scroll_up`
    // paths return before their reset when the drag's target node has gone (`app.rs:3021`,
    // `app.rs:3936`). Compose's guard is safe because a single owner maintains that flag with a
    // guaranteed reset; here it is shared, so an UNBOUNDED wait would turn a leaked flag into a calendar
    // frozen on a stale month for the life of the window — strictly worse than the gesture cancellation
    // the guard exists to prevent.
    // `remember` hands back a `State<T>`, so this slot holds the count itself. Writing it every frame is
    // free: `set_reactive` compares before notifying (`state.rs:390-393`), so re-setting the same count
    // dirties nothing and cannot loop the composition.
    let waited = ctx.remember(|| 0u8);

    let displayed = state.displayed_month_millis();
    let actual = list.first_visible();

    match pending.get() {
        // An outside change is still being carried out. Say nothing until the list arrives; the page
        // it is leaving is not news. Past the bound, give up on the arrival instead: falling through
        // lands in the branch below, which republishes wherever the list actually is, so a target the
        // list can never reach costs a momentary disagreement rather than a dead sync.
        Some(target) if actual != target => {
            if waited.get() < MAX_WAIT_FRAMES {
                waited.set(waited.get() + 1);
                return;
            }
            pending.set(None);
        }
        Some(_) => {
            pending.set(None);
            waited.set(0);
        }
        None => {}
    }

    if displayed != published.get() {
        // The month holds something this function did not put there, so it came from outside — the
        // year panel, or a caller writing the month. Take the list to it.
        let target = page_of(displayed);
        if actual == target {
            published.set(displayed);
            waited.set(0);
            // An outside change moved the list; whatever an arrow had asked for is superseded.
            step_in_flight.set(None);
        } else if list.is_scrolling() && waited.get() < MAX_WAIT_FRAMES {
            // A drag or a fling is still running, so this has to wait. Compose guards the same way and
            // for the same reason — "The DatePicker has other actions that can trigger a scroll and
            // update the displayedMonthMillis as they do so, hence we check here for isScrollInProgress
            // and only scroll to the monthIndex when there is none in progress"
            // (`DatePicker.kt:1545-1547`, guard at `:1548-1553`). Without it `scroll_to_item` calls
            // `cancel_animation`, so the jump would land by killing the gesture the user was in the
            // middle of. Leaving `published` alone is what makes this retry: `displayed != published`
            // still holds next frame.
            waited.set(waited.get() + 1);
        } else {
            list.scroll_to_item(target, 0.0);
            pending.set(Some(target));
            published.set(displayed);
            waited.set(0);
        }
        return;
    }
    waited.set(0);

    // Nothing came from outside, so any difference is the LIST having moved — a swipe, or an arrow's
    // animation partway through. Publish where it is so the nav row and the headline follow.
    let landed = month_of(actual);
    if landed != displayed {
        state.set_displayed_month_millis(landed);
        published.set(landed);
    }
    // The arrows' in-flight page is released once the list actually stands on it: from then on the
    // anchor names the same page, so the two agree and the memory has nothing left to add. It is also
    // released by any motion the arrows did not ask for — a swipe, or an outside jump — so a later
    // press steps from where the list really is rather than from a request the user has overridden.
    if step_in_flight.get() == Some(actual) {
        step_in_flight.set(None);
    }
}

/// The modal date picker's dialog (`DatePickerDialog`, `DatePickerDialog.kt:51-61`; its Android body is
/// `DatePickerDialog.android.kt:75-114`).
///
/// material3 wraps the picker in a `BasicAlertDialog` whose own surface is
/// `requiredWidth(ContainerWidth = 360)` and `heightIn(max = ContainerHeight = 568)`, shaped
/// `DatePickerDefaults.shape` (28 dp) and filled with `colors.containerColor` — the dialog contributes no
/// padding, the picker IS the surface. Under it comes the action row: `DialogButtonsPadding` (bottom 8, end 6)
/// holding a `FlowRow` at `DialogButtonsMainAxisSpacing` (8), the dismiss button first and the confirm button
/// second, in `DialogTokens.ActionLabelTextFont` (LabelLarge) with `DialogTokens.ActionLabelTextColor`
/// (`Primary`) as a default the buttons may override.
///
/// The content defaults to [`DatePicker`] over this dialog's state, which is what M3's modal date
/// picker draws; [`DatePickerDialog::content`] replaces it (the input mode will).
///
/// ```ignore
/// DatePickerDialog::new(state.clone(), open.get())
///     .on_dismiss_request(move || open.set(false))
///     .dismiss_button(|ctx| { Button::text().build(ctx, |ctx| { Text::new("Cancel").build(ctx); }); })
///     .confirm_button(|ctx| { Button::text().build(ctx, |ctx| { Text::new("OK").build(ctx); }); })
///     .build(ctx);
/// ```
pub struct DatePickerDialog {
    state: DatePickerState,
    visible: bool,
    on_dismiss_request: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    confirm_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    content: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    modifier: Modifier,
    shape: Option<Shape>,
    colors: Option<DatePickerColors>,
}

impl DatePickerDialog {
    /// A dialog over `state`, up while `visible` is true. material3 takes the confirm button as a required
    /// parameter and the dismiss button as an optional one; the same here, as builders.
    pub fn new(state: DatePickerState, visible: bool) -> Self {
        Self {
            state,
            visible,
            on_dismiss_request: None,
            dismiss_button: None,
            confirm_button: None,
            content: None,
            modifier: Modifier::new(),
            shape: None,
            colors: None,
        }
    }

    /// Compose's `onDismissRequest`: a click outside the dialog, not the dismiss button.
    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss_request = Some(Arc::new(cb));
        self
    }

    /// The affirming action. It sits after the dismiss button in the row, and the dialog wires no event of its
    /// own into it — not even its enablement.
    pub fn confirm_button(mut self, button: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.confirm_button = Some(Box::new(button));
        self
    }

    /// The dismissing action, before the confirm button.
    pub fn dismiss_button(mut self, button: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.dismiss_button = Some(Box::new(button));
        self
    }

    /// The dialog's content, in place of the default calendar.
    pub fn content(mut self, content: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.content = Some(Box::new(content));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// `DatePickerDefaults.shape` (28 dp) by default.
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// The picker's colour roles, read from the theme by default.
    pub fn colors(mut self, colors: DatePickerColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Composes the dialog into the overlay layer while `visible`.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| DatePickerColors::from_theme(&theme));
        let shape = self.shape.unwrap_or(Shape::RoundedRect {
            corner_radius: DatePickerDefaults::CONTAINER_CORNER,
        });
        let button_color = AlertDialogDefaults::button_color(&theme);
        let button_style = WiniaTheme::typography().label_large.clone();
        let state = self.state.clone();
        let confirm = self.confirm_button;
        let dismiss = self.dismiss_button;
        let content = self.content;
        // The same set the surface is painted from, handed to the default content too. This is a winia
        // convenience rather than a copy of Compose's wiring: material3's `DatePickerDialog` reads its
        // `colors` in exactly one place — `color = colors.containerColor` on its own `Surface`
        // (`DatePickerDialog.android.kt:86`) — and then calls the caller's slot with nothing
        // (`:95  Box(Modifier.weight(1f, fill = false)) { this@Column.content() }`). A caller nesting a
        // `DatePicker` is expected to pass `colors` down itself; winia has no caller for its default
        // content to be anyone but itself, so the dialog does it. Either way the visible result was wrong
        // before: a caller overriding the dialog got a theme-coloured calendar inside their own container.
        let content_colors = colors.clone();
        let size = Modifier::new()
            .width(DatePickerDefaults::CONTAINER_WIDTH)
            .max_height(DatePickerDefaults::MODAL_CONTAINER_HEIGHT);

        let mut dialog = BasicAlertDialog::new(self.visible)
            .shape(shape)
            .container_color(colors.container)
            // The picker is the container: material3's date picker dialog adds no padding, where winia's
            // `BasicAlertDialog` carries the alert dialog's 24 dp (`DatePickerDialog.android.kt:88-93`).
            .content_padding(0.0)
            .modifier(self.modifier.then(size))
            .content(move |ctx| {
                // `Column(verticalArrangement = SpaceBetween)`: the content in a `weight(1f, fill = false)` box,
                // the action row aligned to the end under it (`DatePickerDialog.android.kt:89-111`).
                //
                // The box is what lets the dialog be SHORT: its share is the box's MAXIMUM, so the
                // box reports the size the picker actually asked for and the Column ends up content
                // + buttons rather than the whole cap. material3 says so in the source — "Fill is
                // false to support collapsing the dialog's height when switching to input mode"
                // (`:93-94`) — while the calendar's own height is what fills the cap in picker mode.
                //
                // `Arrangement::SpaceBetween` as the source writes it. That only works because a
                // spreading arrangement no longer grows a content-sized container to the maximum its
                // parent offers (`layout/flex.rs`'s `measured_main`, aligned with Compose's
                // `mainAxisLayoutSize = max(fixedSpace + weightedSpace, mainAxisMin)`); before that
                // alignment this column had to say `Start` to escape the 568 dp cap. The leftover here
                // is zero either way, so the two place identically.
                Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .arrangement(Arrangement::SpaceBetween)
                    .alignment(Alignment::End)
                    .build(ctx, |ctx| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .fill_max_width()
                                    .layout_weight_fill(1.0, false),
                            )
                            .build(ctx, |ctx| {
                                match content.as_ref() {
                                    Some(content) => content(ctx),
                                    None => DatePicker::new(state.clone())
                                        .colors(content_colors.clone())
                                        .build(ctx),
                                }
                            });
                        Row::new()
                            .modifier(
                                Modifier::new()
                                    .padding_bottom(DatePickerDefaults::MODAL_BUTTONS_BOTTOM_PADDING)
                                    .padding_end(DatePickerDefaults::MODAL_BUTTONS_END_PADDING),
                            )
                            .spacing(DatePickerDefaults::MODAL_BUTTONS_SPACING)
                            .alignment(Alignment::Center)
                            .build(ctx, |ctx| {
                                ProvideTextStyle(button_style.clone(), ctx, |ctx| {
                                    WiniaTheme::with_content_color(button_color, ctx, |ctx| {
                                        if let Some(dismiss) = dismiss.as_ref() {
                                            dismiss(ctx);
                                        }
                                        if let Some(confirm) = confirm.as_ref() {
                                            confirm(ctx);
                                        }
                                    });
                                });
                            });
                    });
            });
        // The handler is already boxed here, so it goes in through the same slot `AlertDialog` uses.
        dialog.dismiss_handler(self.on_dismiss_request).build(ctx);
    }
}

/// The header: the title over the headline, with the divider below them
/// (`DateEntryContainer` and `DatePickerHeader`, `DatePicker.kt:1365-1396`, `:1671-1698`).
fn header(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    title: Option<&str>,
    colors: &DatePickerColors,
    display_mode: DisplayMode,
    on_toggle_display_mode: impl Fn() + Send + Sync + 'static,
) {
    let model = state.calendar_model();
    // material3's headline says what is selected in either mode, and names the mode's own wording
    // when nothing is (`DatePickerHeadline`, `DatePicker.kt:701-717`). The description carries the
    // verbose date for a reader, falling back to the mode's own "nothing" wording.
    let headline = state
        .selected_date_millis()
        .map(|millis| model.format_date(millis, false))
        .unwrap_or_else(|| match display_mode {
            DisplayMode::Picker => DatePickerDefaults::HEADLINE.to_string(),
            DisplayMode::Input => DATE_INPUT_HEADLINE.to_string(),
        });
    let headline_description = state
        .selected_date_millis()
        .map(|millis| model.format_date(millis, true))
        .unwrap_or_else(|| DatePickerDefaults::NO_SELECTION_DESCRIPTION.to_string());
    let headline_description = match display_mode {
        DisplayMode::Picker => format_string(
            DatePickerDefaults::HEADLINE_DESCRIPTION,
            &[&headline_description],
        ),
        DisplayMode::Input => format_string(DATE_INPUT_HEADLINE_DESCRIPTION, &[&headline_description]),
    };
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
                                    .padding_bottom(DatePickerDefaults::HEADLINE_BOTTOM_PADDING)
                                    // The headline announces both what it reads and which mode it is
                                    // in, politely rather than assertively, so a selection made in
                                    // the calendar or typed in the field is picked up either way
                                    // (`DatePicker.kt:722-725`).
                                    .semantics(
                                        crate::semantics::SemanticsConfig::new()
                                            .content_description(headline_description)
                                            .live_region(
                                                crate::semantics::LiveRegionMode::Polite,
                                            ),
                                    ),
                            )
                            .build(ctx);
                    });
                    display_mode_toggle(
                        ctx,
                        display_mode,
                        on_toggle_display_mode,
                        headline_color,
                    );
                });
            // material3 draws the divider when a title, a headline or a mode toggle is present
            // (`DatePicker.kt:1392-1394`); a headline is always composed here.
            Divider::horizontal().build(ctx);
        });
}

/// The month navigation row: the year menu button, then the two month arrows while the year panel is closed
/// (`MonthsNavigation`, `DatePicker.kt:2182-2239`). material3 drops the arrows and packs the row to its start
/// while the panel is open.
///
/// The arrows' enablement and effect both come from the page list, not from the year range:
/// `nextAvailable = monthsListState.canScrollForward` (`DatePicker.kt:1561-1562`) and a press runs
/// `animateScrollToItem(firstVisibleItemIndex ± 1)` (`DatePicker.kt:1569-1592`). An arrow that read the
/// year range instead could disagree with a swipe at the ends of the list.
fn months_navigation(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    month: &CalendarMonth,
    year_panel_open: bool,
    on_toggle_year_panel: impl Fn() + Send + Sync + 'static,
    colors: &DatePickerColors,
    list: &LazyListState,
    step_in_flight: &State<Option<usize>>,
) {
    let text = state.calendar_model().format_month_year(month.start_utc_time_millis);
    let navigation_color = colors.navigation_content;

    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_YEAR_HEIGHT),
        )
        .arrangement(if year_panel_open {
            Arrangement::Start
        } else {
            Arrangement::SpaceBetween
        })
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            year_menu_button(ctx, text, on_toggle_year_panel, colors, true, YEAR_MENU_TAG);
            if !year_panel_open {
                // The arrows are one unit at the row's far end: in a `SpaceBetween` row they need a row of
                // their own, or the arrangement would spread them across the whole width.
                Row::new()
                    .arrangement(Arrangement::Start)
                    .alignment(Alignment::Center)
                    .build(ctx, |ctx| {
                        // The last page the list can show, so a forward step cannot ask past the end
                        // and leave the arrow's own request stranded above the clamp.
                        let last_page = CalendarModel::number_of_months_in_range(&state.year_range())
                            .saturating_sub(1) as usize;
                        let back = list.clone();
                        let back_flight = step_in_flight.clone();
                        let back_anchor = list.first_visible();
                        step_arrow(ctx, move || {
                            let target = arrow_target(back_anchor, back_flight.get(), false, last_page);
                            back_flight.set(Some(target));
                            back.animate_scroll_to_item(target, 0.0);
                        }, list.can_scroll_backward(), CHEVRON_LEFT_PATH, navigation_color, IconButtonSize::Small, true);
                        let forward = list.clone();
                        let forward_flight = step_in_flight.clone();
                        let forward_anchor = list.first_visible();
                        step_arrow(ctx, move || {
                            let target = arrow_target(forward_anchor, forward_flight.get(), true, last_page);
                            forward_flight.set(Some(target));
                            forward.animate_scroll_to_item(target, 0.0);
                        }, list.can_scroll_forward(), CHEVRON_RIGHT_PATH, navigation_color, IconButtonSize::Small, true);
                    });
            }
        });
}

/// One navigation arrow: the month stepper's arrows (`monthsListState.canScrollBackward/Forward`,
/// `DatePicker.kt:1561-1562`; enabled here while the month has a neighbour inside the year range) and the docked
/// picker's year arrows.
///
/// `on_click` carries what the arrow steps, because that is the only thing that differs between them — the
/// month arrows read the displayed month at the moment of the click (`DatePickerState::step_displayed_month`),
/// the year ones step the displayed year by one. Sharing the control keeps the hiding rule in one place.
///
/// The glyph auto-mirrors, which is the second half of RTL and is easy to miss: `Row` already puts the
/// previous arrow at the start (the RIGHT edge in RTL), so without the mirror both arrows would point
/// INWARD — "previous" pointing right, towards the label, and "next" pointing left, towards it too.
/// material3 asks for exactly this by drawing the two arrows as `Icons.AutoMirrored.Filled.KeyboardArrowLeft`
/// and `…KeyboardArrowRight` (`DatePicker.kt:2225`, `:2232`), so the artwork flips and each arrow keeps
/// pointing outward from the month it moves.
fn step_arrow(
    ctx: &mut ComposeCtx,
    on_click: impl Fn() + Send + Sync + 'static,
    enabled: bool,
    path: &'static str,
    color: Color,
    size: IconButtonSize,
    visible: bool,
) {
    // Hidden arrows stay composed at alpha 0 (still disabled): removing them would shift the menu
    // button, and the alpha reads straight into a fade animation later.
    let mut modifier = Modifier::new();
    if !visible {
        modifier = modifier.alpha(0.0);
    }
    IconButton::new()
        .size(size)
        .modifier(modifier)
        .enabled(enabled && visible)
        .on_click(on_click)
        .build(ctx, |ctx| {
            Icon::svg_path(path)
                .tint(color)
                .auto_mirror(true)
                .build(ctx);
        });
}

/// Which page a month arrow should step to, given where the list is and where it is already going.
///
/// The arrows step **one page from the page already in flight**, not from the anchor. That distinction
/// is the whole of a reported asymmetry: the anchor (`first_visible`) is the page whose span still
/// contains the pixel offset, so while a forward spring runs it names the page being LEFT — `anchor + 1`
/// then keeps naming the page already being animated to, the request is deduplicated as "already going
/// there", and every press during the animation is swallowed. Scrolling backward flips the anchor as
/// soon as the offset leaves the old page's span, so `anchor - 1` names a genuinely new page and every
/// press lands. Measured on the docked demo, three presses 120 ms apart: `prev` advanced 2 months,
/// `next` advanced 0.
///
/// `in_flight` is the last page an arrow asked for, and is cleared once the list arrives (`sync_month_pages`
/// clears it), so a press during the motion continues from the target and a press after it starts from the
/// anchor again. Either way each press is worth exactly one page.
///
/// Clamped to `[0, last_page]`: an arrow is disabled at the ends, but the in-flight page can be past the
/// end for the frame between the request and the measure that clamps it.
///
/// **Deliberately beyond Compose, and the reason is recorded here rather than in the docs alone.** Compose
/// reads `firstVisibleItemIndex ± 1` at the press (`DatePicker.kt:1569-1592`) and therefore swallows a
/// press during a forward animation too — material3 wraps both handlers in
/// `catch (_: IllegalArgumentException)` for exactly that (`:1571-1587`). One press for one page is fine;
/// one DIRECTION swallowing and the other not is not, and that is what the asymmetry above amounts to.
fn arrow_target(anchor: usize, in_flight: Option<usize>, forward: bool, last_page: usize) -> usize {
    let from = in_flight.unwrap_or(anchor);
    if forward {
        from.saturating_add(1).min(last_page)
    } else {
        from.saturating_sub(1)
    }
}

/// The year menu button: the "September 2024" text and a dropdown arrow, the control that opens the year panel
/// (`YearPickerMenuButton`, `DatePicker.kt:2243-2269`). material3 builds it from a `TextButton` whose elevation
/// and border it explicitly clears; winia's buttons carry no such parameters to clear.
///
/// A disabled button degrades to plain dimmed text with its dropdown glyph faded to alpha 0 (still
/// composed, so the label does not shift) and no interaction — the M3 specs docked figure shows the idle
/// group exactly so ("2025" with neither pill nor arrow) while the other group's list is open.
fn year_menu_button(
    ctx: &mut ComposeCtx,
    text: String,
    on_click: impl Fn() + Send + Sync + 'static,
    colors: &DatePickerColors,
    enabled: bool,
    tag: &str,
) {
    let description = text.clone();
    let color = if enabled {
        colors.navigation_content
    } else {
        DatePickerColors::disabled(colors.navigation_content)
    };
    Surface::new()
        .shape(Shape::Pill)
        .color(Color::TRANSPARENT)
        .content_color(color)
        .enabled(enabled)
        .selectable(false, on_click)
        .modifier(
            Modifier::new()
                .height(DatePickerDefaults::YEAR_MENU_BUTTON_HEIGHT)
                .test_tag(tag.to_string())
                // material3 repeats the button's text as its content description and makes it a polite live
                // region, so a reader announces the month as the arrows move it (`DatePicker.kt:2205-2216`).
                .semantics(crate::semantics::SemanticsConfig::new().content_description(description)),
        )
        .build(ctx, |ctx| {
            // The wrapper fills the button's 40 dp height and centres the row in it; a wrap-content
            // row would hug the top while the arrows around it centre in theirs. Height only: filling
            // the width as well stretches the surface full-bleed and the hover state layer with it.
            Stack::new()
                .alignment(Alignment::Center)
                .modifier(Modifier::new().fill_max_height())
                .build(ctx, |ctx| {
                    // material3's `TextButtonWithIconContentPadding` (a `TextButton` holding the
                    // dropdown glyph): 12 dp at the start, 16 dp at the end (`Button.kt:515-522`).
                    Row::new()
                        .modifier(
                            Modifier::new()
                                .padding_start(12.0)
                                .padding_end(16.0),
                        )
                        .arrangement(Arrangement::Start)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            ProvideTextStyle(WiniaTheme::typography().label_large.clone(), ctx, |ctx| {
                                Text::new(text).color(color).max_lines(1).build(ctx);
                            });
                            Spacer::horizontal(DatePickerDefaults::YEAR_MENU_ICON_SPACING).build(ctx);
                            // Faded, not removed: dropping the glyph would shrink the button and shift the
                            // label, and the alpha reads straight into a fade animation later.
                            let mut glyph = Modifier::new();
                            if !enabled {
                                glyph = glyph.alpha(0.0);
                            }
                            Icon::svg_path(ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH)
                                .tint(color)
                                .modifier(glyph)
                                .build(ctx);
                        });
                });
        });
}

/// The row the year list starts on when the panel opens: material3's
/// `max(0, displayedYear - yearRange.first - YearsInRow)` as an item index of a three-column grid
/// (`DatePicker.kt:2073-2080`), which is one row above the displayed year.
fn year_panel_first_row(state: &DatePickerState, model: &CalendarModel) -> usize {
    let year_range = state.year_range();
    let displayed_year = model.month_of_millis(state.displayed_month_millis()).year;
    let offset = (displayed_year - *year_range.start()).max(0) as usize;
    (offset / DatePickerDefaults::YEARS_PER_ROW).saturating_sub(1)
}

/// The year panel: three columns of years in a lazy list, `YEAR_PANEL_HEIGHT` tall over the divider that closes
/// it (`YearPicker`, `DatePicker.kt:2061-2116`).
///
/// material3 overlays this on the month calendar and keeps the calendar composed underneath
/// (`DatePicker.kt:1612-1660`); winia swaps the calendar out instead, which looks the same because the panel is
/// exactly as tall as the weekday row and the grid it replaces and paints the picker's own container colour
/// behind it. What is missing is material3's expand and fade (`AnimatedVisibility`) — the modal path still
/// switches abruptly, while the docked path wraps this panel in a `Crossfade` of its own, so only the modal
/// picker carries that deviation; `docs/date-picker.md` lists it.
fn year_panel(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    colors: &DatePickerColors,
    rows: &LazyListState,
    on_year_selected: impl Fn(i32) + Clone + Send + Sync + 'static,
) {
    let year_range = state.year_range();
    let first = *year_range.start();
    let count = year_range.clone().count();
    let row_count = count.div_ceil(DatePickerDefaults::YEARS_PER_ROW);
    let current_year = model.month_of_millis(state.today_millis()).year;
    // material3 highlights the DISPLAYED year, not the selected date's year — `YearPicker` passes
    // `selected = selectedYear == displayedYear` (`DatePicker.kt:2091`). The displayed year is what
    // the panel replaces and what picking one changes, so it is the one the user sees filled.
    let displayed_year = model.month_of_millis(state.displayed_month_millis()).year;
    let list_state = rows.clone();
    let cell_state = state.clone();
    let cell_colors = colors.clone();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::YEAR_PANEL_HEIGHT + DatePickerDefaults::DIVIDER_THICKNESS),
        )
        .arrangement(Arrangement::Start)
        .build(ctx, |ctx| {
            // A divider closes the panel at both ends; the list gives back 1 dp so the 336 dp
            // total still matches the weekday row plus the grid it replaces.
            Divider::horizontal().build(ctx);
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(DatePickerDefaults::YEAR_PANEL_HEIGHT - DatePickerDefaults::DIVIDER_THICKNESS),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        LazyColumn::new()
                            .modifier(Modifier::new().layout_weight(1.0).fill_max_height())
                            .state(list_state.clone())
                            .spacing(DatePickerDefaults::YEARS_VERTICAL_PADDING)
                            .items(
                                row_count,
                                |row| row as u64,
                                move |ctx, row| {
                                    Row::new()
                                        .modifier(Modifier::new().fill_max_width())
                                        .arrangement(Arrangement::SpaceEvenly)
                                        .alignment(Alignment::Center)
                                        .build(ctx, |ctx| {
                                            for column in 0..DatePickerDefaults::YEARS_PER_ROW {
                                                let index =
                                                    row * DatePickerDefaults::YEARS_PER_ROW + column;
                                                if index >= count {
                                                    break;
                                                }
                                                let year = first + index as i32;
                                                year_cell(
                                                    ctx,
                                                    &cell_colors,
                                                    cell_state
                                                        .selectable_dates()
                                                        .is_selectable_year(year),
                                                    year,
                                                    displayed_year == year,
                                                    year == current_year,
                                                    on_year_selected.clone(),
                                                );
                                            }
                                        });
                                },
                            )
                            .build(ctx);
                        // Always visible: the list is 2412 months tall and nothing else tells the user
                        // it scrolls (the bottom row is cut off mid-year otherwise).
                        LazyScrollbar::new(list_state)
                            .always_show(true)
                            .build(ctx);
                    });
            });
            Divider::horizontal().build(ctx);
        });
}

/// One year in the panel (`Year`, `DatePicker.kt:2120-2180`): a 72 × 36 stadium that fills with `Primary` when
/// it is the displayed year, and carries the same 1 dp outline around the current year that today gets. winia
/// notes `Pill` where material3 notes `CornerFull`; on a 72 × 36 box they are the same stadium.
fn year_cell(
    ctx: &mut ComposeCtx,
    colors: &DatePickerColors,
    enabled: bool,
    year: i32,
    selected: bool,
    current_year: bool,
    on_year_selected: impl Fn(i32) + Send + Sync + 'static,
) {
    let label = year.to_string();
    // material3 merges the description into the surface and clears the inner text's semantics
    // (`DatePicker.kt:2141-2162`); the label's colour role rides on the surface's content colour here.
    let description = format!("Navigate to {label}");
    let mut surface = Surface::new()
        .shape(Shape::Pill)
        .color(colors.year_container(selected, enabled))
        .content_color(colors.year_label(current_year, selected, enabled))
        .enabled(enabled)
        .selectable(selected, move || on_year_selected(year))
        .modifier(
            Modifier::new()
                .size(
                    DatePickerDefaults::YEAR_CELL_WIDTH,
                    DatePickerDefaults::YEAR_CELL_HEIGHT,
                )
                .test_tag(format!("{YEAR_CELL_TAG_PREFIX}{year}"))
                .semantics(crate::semantics::SemanticsConfig::new().content_description(description)),
        );
    if current_year && !selected {
        surface = surface.border(SurfaceBorder::new(
            DatePickerDefaults::TODAY_OUTLINE_WIDTH,
            colors.today_border,
        ));
    }
    surface.build(ctx, |ctx| {
        // Same centering as `day_cell`: the surface lays content out top-start, so without this
        // the label hugs the pill's edge and the today outline clips it (measured on "2026").
        Stack::new()
            .alignment(Alignment::Center)
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new(label).build(ctx);
            });
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

/// The month grid: six rows of seven slots, each slot a day (`Month`, `DatePicker.kt:1856-1890`).
///
/// `show_outside_month` decides what the leading and trailing slots do with the days that belong to the
/// neighbouring month, which [`MonthGrid`] always computes — the dates are true whether or not they are
/// drawn. The docked picker asks for them and material3 does not:
///
/// - Docked: drawn, at the specs' dimmed label. The M3 specs' *Docked date picker* anatomy lists
///   "Outside month date" among the grid's states, and gives it two tokens of its own.
/// - Modal: a `Spacer`, exactly as `Month` composes them (`DatePicker.kt:1870-1890`). The *Modal date
///   picker* anatomy on the same specs page has no "Outside month date" entry at all.
///
/// The slot still measures 48 dp either way, so the grid's geometry does not depend on the flag.
fn month_grid(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    grid: &MonthGrid,
    colors: &DatePickerColors,
    show_outside_month: bool,
) {
    let rows = grid.rows().map(<[DayCell]>::to_vec).collect::<Vec<_>>();
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
                                // material3's empty cell, and it is NOT an empty node: `Month` composes a
                                // `Spacer` sized to the day's 48 dp so the row keeps its lattice
                                // (`DatePicker.kt:1876-1890`, whose comment says exactly that). Composing
                                // nothing collapses the row to zero and `SpaceEvenly` then shifts every
                                // row above it — measured on September 2024, whose trailing week is
                                // entirely outside the month: the rows moved 13.7 dp up, and a scan across
                                // the 10th's old centre measured a 31 dp chord instead of 39.
                                //
                                // An empty `Stack`, because winia's `Spacer` only spans one axis; both are
                                // childless `BoxLayout`s, so the slot measures the same.
                                if cell.is_outside_month && !show_outside_month {
                                    Stack::new()
                                        .modifier(Modifier::new().size(
                                            DatePickerDefaults::ACCESSIBLE_SIZE,
                                            DatePickerDefaults::ACCESSIBLE_SIZE,
                                        ))
                                        .build(ctx, |_| {});
                                    continue;
                                }
                                Stack::new()
                                    .alignment(Alignment::Center)
                                    .modifier(Modifier::new().size(
                                        DatePickerDefaults::ACCESSIBLE_SIZE,
                                        DatePickerDefaults::ACCESSIBLE_SIZE,
                                    ))
                                    .build(ctx, |ctx| {
                                        day_cell(ctx, state, model, cell, colors);
                                    });
                            }
                        });
                }
            });
        });
}

/// The selected day's background circle, drawn by its own node so that only it animates: it fades
/// 0 → full over [`DAY_POP_MILLIS`] when the selection lands (and back out when it leaves), radius
/// always full, and the label above it never moves.
/// The progress is peeked at paint time (ticks never recompose); per the `DrawNode` contract it stays
/// out of `node_key` — only the static color fingerprints the node.
///
/// How long the selected circle takes to fade in and out. material3 has no selection animation of its own
/// (the container colour switches outright, `DatePicker.kt:973-993`), so this is winia's own: a short,
/// symmetric ease-out that keeps the change legible without making the grid feel slow.
const DAY_POP_MILLIS: u64 = 220;
#[derive(Debug)]
struct DayCircleNode {
    progress: State<f32>,
    color: Color,
}

impl crate::modifier::DrawNode for DayCircleNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        let t = self.progress.peek().clamp(0.0, 1.0);
        if t <= 0.0 {
            return;
        }
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::PaintStyle::Fill);
        paint.set_color(crate::render::skia_color(Color {
            a: (t * 255.0).round() as u8,
            ..self.color
        }));
        canvas.draw_circle(
            skia_safe::Point::new(rect.center_x(), rect.center_y()),
            rect.width().min(rect.height()) / 2.0,
            &paint,
        );
    }
    fn node_key(&self) -> String {
        format!("day-circle:{:?}", self.color)
    }
}

/// One day of the grid: a 40 dp circle, outlined when it is today and not selected, filled when it is selected
/// (`Day`, `DatePicker.kt:1993-2058`).
///
/// An outside-month cell (`DayCell::is_outside_month`) is context: it takes the specs' dimmed label, gets no
/// today ring and no selection fill, and is not clickable — which `is_enabled` already carries, since
/// [`MonthGrid`] never enables one.
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
    let outside = cell.is_outside_month;
    // Selection progress 0 → 1: every cell remembers one, so landing the selection here plays the
    // circle in, and moving it away plays the same value 1 → 0 (the node keeps drawing while the
    // progress is above zero, so the circle shrinks out instead of vanishing).
    //
    // An outside cell is pinned to 0: a date selected in another month can appear here, and a context cell
    // must not claim it with a fill the user cannot tap to keep.
    let pop = ctx.animate_float_as_state(
        if cell.is_selected && !outside { 1.0 } else { 0.0 },
        AnimationSpec::Tween(TweenSpec::new(
            std::time::Duration::from_millis(DAY_POP_MILLIS),
            crate::animation::interpolator::EaseOutCubic::new(),
        )),
    );
    let mut surface = Surface::new()
        .shape(Shape::Circle)
        .color(Color::TRANSPARENT)
        .content_color(if outside {
            colors.outside_month_label()
        } else {
            colors.day_label(cell.is_selected, cell.is_enabled, cell.is_today)
        })
        .enabled(cell.is_enabled)
        .selectable(cell.is_selected, move || {
            state_for_click.set_selected_date_millis(Some(millis));
        })
        .modifier(
            Modifier::new()
                .size(DatePickerDefaults::DAY_CELL, DatePickerDefaults::DAY_CELL)
                .semantics(
                    crate::semantics::SemanticsConfig::new().content_description(description),
                )
                .draw_node(DayCircleNode {
                    // The circle's own selected-container color, NOT `day_container(is_selected)`:
                    // on deselect the cell rebuilds unselected while the progress is still fading,
                    // and `TRANSPARENT` carries zeroed rgb — fading that paints a black disc instead
                    // of the primary one (measured). The node only draws while progress > 0, so the
                    // base stays valid across the whole out-play.
                    progress: pop,
                    color: colors.day_container(true, cell.is_enabled),
                }),
        );
    // No today ring on a context cell either: the specs draw "Today's date" and "Outside month date" as
    // separate states, and a ring would claim a cell that cannot be chosen.
    if cell.is_today && !cell.is_selected && !outside {
        surface = surface.border(SurfaceBorder::new(
            DatePickerDefaults::TODAY_OUTLINE_WIDTH,
            colors.today_border,
        ));
    }
    surface.build(ctx, |ctx| {
        // material3's `Day` centers its label in the 40 dp circle (`DatePicker.kt:1993-2058`); the surface
        // itself lays content out top-start, so the centering is explicit here.
        Stack::new()
            .alignment(Alignment::Center)
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new(cell.day.to_string()).build(ctx);
            });
    });
}

// ── Docked date picker ──

/// Material Icons `calendar_month` glyph for the docked picker's input affordance. Like the chevrons above,
/// this sandbox cannot byte-verify the artwork against Google's assets, so it is an inline path.
pub const CALENDAR_MONTH_PATH: &str = "M19 4h-1V2h-2v2H8V2H6v2H5c-1.11 0-1.99.9-1.99 2L3 20c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V10h14v10zM9 14H7v-2h2v2zm4 0h-2v-2h2v2zm4 0h-2v-2h2v2zm-8 4H7v-2h2v2zm4 0h-2v-2h2v2zm4 0h-2v-2h2v2z";

/// The docked container's corner radius, read off the M3 specs measurements diagram. Provisional: docked has
/// no token file (unlike the modal's 28 dp `ContainerShape`), so this stays a plain constant until the token
/// value is confirmed.
pub const DOCKED_CONTAINER_CORNER: f32 = 16.0;

/// The inline panel the docked picker shows in place of the weekday row and the month grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockedPanel {
    Calendar,
    Months,
    Years,
}

/// The docked date picker: the M3 specs variant that opens from an onscreen input.
///
/// Unlike [`DatePicker`] (the modal calendar with its "Select date" title and headline), the docked picker has
/// no header. Its container holds, top to bottom: a navigation row with a month group and a year group (each
/// arrows plus a button whose list opens inline, replacing the grid), the weekday row, the month grid, and a
/// Cancel/OK action row.
///
/// The input field is NOT part of this component: in material3 it belongs to the caller that opens the picker
/// (`DatePickerDialog`'s dock mode keeps its own field, and the M3 specs docked figure draws one above the
/// picker), so winia leaves it out and the caller pairs this with a `TextField` of its own.
///
/// Day taps write the selection into `state` immediately; Cancel/OK only notify. A caller that needs
/// discard-on-cancel snapshots `selected_date_millis` before opening and restores it in `on_dismiss`.
pub struct DockedDatePicker {
    state: DatePickerState,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    modifier: Modifier,
    /// A tag for a readout node reporting the pager's derived anchor, or `None`.
    ///
    /// Diagnostic only, and behind a caller-supplied tag rather than always on: the anchor
    /// (`LazyListState::first_visible`) is written back inside the measure every frame and is not part
    /// of what gets drawn, so a probe outside the process cannot read it — the tree shows positions,
    /// which is the pixel offset the anchor is derived FROM. The month arrows compute their target as
    /// `anchor ± 1`, so anything that goes wrong asymmetrically between the two directions lives here,
    /// and this is the only way to see it.
    page_probe: Option<String>,
}

impl DockedDatePicker {
    /// A docked picker over `state`.
    pub fn new(state: DatePickerState) -> Self {
        Self {
            state,
            on_confirm: None,
            on_dismiss: None,
            modifier: Modifier::new(),
            page_probe: None,
        }
    }

    /// The affirming action, after the dismiss button in the row. The picker wires no event of its own into it.
    pub fn on_confirm(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_confirm = Some(Arc::new(cb));
        self
    }

    /// The dismissing action, before the confirm button in the row.
    pub fn on_dismiss(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// A modifier for the container (`modifier`).
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Compose a readout of the pager's derived anchor under `tag`, for a probe outside the process.
    ///
    /// Diagnostic only — see [`DockedDatePicker::page_probe`]. The node carries `page:N`, where `N` is
    /// `LazyListState::first_visible` for the month list on the frame it was composed, and is marked
    /// `test_tag(tag)` so a debug-server probe can find it.
    pub fn page_probe(mut self, tag: impl Into<String>) -> Self {
        self.page_probe = Some(tag.into());
        self
    }

    /// Composes the picker.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let colors = DatePickerColors::from_theme(&WiniaTheme::colors());
        let model = self.state.calendar_model().clone();
        let month = model.month_of_millis(self.state.displayed_month_millis());
        let state = self.state.clone();
        // Which inline panel replaces the weekday row and the grid, if any.
        let panel = ctx.remember(|| DockedPanel::Calendar);
        // The year list's scroll state, parked here so reopening the panel keeps its position.
        let year_rows = ctx.remember(LazyListState::new).get();
        // The paged month list, its position being the displayed month (see `sync_month_pages`), which
        // seeds it with `scroll_to_item` — see the note in `DatePicker::build`.
        let month_rows = ctx.remember(LazyListState::new).get();
        // The page the arrows have asked for but not reached — see the note in `DatePicker::build`.
        let month_step_in_flight = ctx.remember(|| None::<usize>);
        // Page 0 is the January of the range's first year — see the note in `DatePicker::build`.
        let first_month = model.month_of(*state.year_range().start(), 1).start_utc_time_millis;
        let on_confirm = self.on_confirm.clone();
        let on_dismiss = self.on_dismiss.clone();

        let container = self
            .modifier
            .then(Modifier::new().width(DatePickerDefaults::CONTAINER_WIDTH))
            .background(colors.container, Shape::rounded(DOCKED_CONTAINER_CORNER))
            .clip(Shape::rounded(DOCKED_CONTAINER_CORNER));

        Column::new()
            .modifier(container)
            .arrangement(Arrangement::Start)
            .build(ctx, |ctx| {
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding_horizontal(DatePickerDefaults::HORIZONTAL_PADDING),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        let current = panel.get();
                        // Only while the calendar is the panel on show: an inline month or year list
                        // covers the pages, and the year panel is exactly what moves the month.
                        if current == DockedPanel::Calendar {
                            sync_month_pages(ctx, &state, &model, &month_rows, first_month, &month_step_in_flight);
                        }
                        // Diagnostic readout of the anchor the arrows compute their target from, plus
                        // the pixel offset it is derived from. It sits before the navigation row so its
                        // text is composed on every frame — `first_visible()` only moves when the
                        // measure writes it back, and a probe reading the tree needs the value of the
                        // frame it just asked about. The offset comes along because the anchor alone
                        // cannot tell "the spring has not arrived" from "it arrived and the anchor did
                        // not move".
                        if let Some(tag) = self.page_probe.as_deref() {
                            Text::new(format!(
                                "page:{} off:{:.0}",
                                month_rows.first_visible(),
                                month_rows.offset()
                            ))
                            .font_size(8.0)
                            .modifier(Modifier::new().test_tag(tag))
                            .build(ctx);
                        }
                        docked_navigation(ctx, &state, &month, current, &panel, &colors, &year_rows, &month_rows, &month_step_in_flight);
                        // The inline lists crossfade in and out of the calendar's place (material3
                        // swaps its year overlay with expand + fade; a full-bleed fade reads the same
                        // here and never moves the action row).
                        let cross_state = state.clone();
                        let cross_model = model.clone();
                        let cross_month = month;
                        let cross_colors = colors.clone();
                        let cross_rows = year_rows.clone();
                        let cross_month_rows = month_rows.clone();
                        let cross_panel = panel.clone();
                        crate::ui::Crossfade::new(panel.clone())
                            // The default 300 ms linear tween plays twice per switch (out, then
                            // in) — 150 ms eased each way lands about as fast as the panel feels.
                            .animation(TweenSpec::new(
                                std::time::Duration::from_millis(150),
                                crate::animation::interpolator::EaseOutCubic::new(),
                            ))
                            .build(
                            ctx,
                            move |ctx, shown| match shown {
                                DockedPanel::Calendar => {
                                    // A Column, not bare siblings: `Crossfade` wraps its content in a
                                    // Box (stacked, overlapping children), so without this the grid
                                    // starts at the same y as the weekday row (measured overlap).
                                    Column::new()
                                        .modifier(Modifier::new().fill_max_width())
                                        .arrangement(Arrangement::Start)
                                        .build(ctx, |ctx| {
                                            weekday_row(ctx, &cross_model, &cross_colors);
                                            // The docked picker draws the neighbouring month's days — the
                                            // M3 specs' docked anatomy lists them as a grid state, and
                                            // the modal one does not.
                                            month_pages(
                                                ctx,
                                                &cross_state,
                                                &cross_model,
                                                &cross_colors,
                                                &cross_month_rows,
                                                true,
                                            );
                                        });
                                }
                                DockedPanel::Months => {
                                    month_list(
                                        ctx,
                                        &cross_state,
                                        &cross_model,
                                        &cross_month,
                                        &cross_panel,
                                        &cross_colors,
                                    );
                                }
                                DockedPanel::Years => {
                                    let panel_for_close = cross_panel.clone();
                                    let state_for_close = cross_state.clone();
                                    year_panel(
                                        ctx,
                                        &cross_state,
                                        &cross_model,
                                        &cross_colors,
                                        &cross_rows,
                                        move |year| {
                                            state_for_close.set_displayed_year(year);
                                            panel_for_close.set(DockedPanel::Calendar);
                                        },
                                    );
                                }
                            },
                        );
                        docked_action_row(ctx, on_confirm, on_dismiss);
                    });
            });
    }
}

/// The docked navigation row: a month group and a year group, each arrows around a button whose list opens
/// inline. While a group's panel is open its arrows are hidden, leaving the button to close it.
fn docked_navigation(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    month: &CalendarMonth,
    current: DockedPanel,
    panel: &State<DockedPanel>,
    colors: &DatePickerColors,
    year_rows: &LazyListState,
    month_rows: &LazyListState,
    month_step_in_flight: &State<Option<usize>>,
) {
    let navigation_color = colors.navigation_content;
    let months_open = current == DockedPanel::Months;
    let years_open = current == DockedPanel::Years;
    // While either list is open both groups' step arrows fade to alpha 0 (still composed and disabled),
    // so neither button moves; the idle group degrades to plain dimmed text (the M3 specs docked figure
    // shows the year side exactly so: "2025" with neither pill nor dropdown arrow, both chevron pairs gone).
    let panel_open = months_open || years_open;

    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_YEAR_HEIGHT),
        )
        .arrangement(Arrangement::SpaceBetween)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            // Month group: step arrows around the month name; the button swaps in the month list. The
            // arrows page the list, like the modal picker's — they never write the month themselves.
            Row::new()
                .arrangement(Arrangement::Start)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    // The last page the list can show, so a forward step cannot ask past the end.
                    let last_page = CalendarModel::number_of_months_in_range(&state.year_range())
                        .saturating_sub(1) as usize;
                    let back = month_rows.clone();
                    let back_flight = month_step_in_flight.clone();
                    let back_anchor = month_rows.first_visible();
                    step_arrow(ctx, move || {
                        let target = arrow_target(back_anchor, back_flight.get(), false, last_page);
                        back_flight.set(Some(target));
                        back.animate_scroll_to_item(target, 0.0);
                    }, month_rows.can_scroll_backward(), CHEVRON_LEFT_PATH, navigation_color, IconButtonSize::XSmall, !panel_open);
                    let panel_toggle = panel.clone();
                    let is_open = months_open;
                    // Abbreviated month ("Sep", not "September") — the M3 specs docked figure.
                    let label = state.calendar_model().locale().month_names_short[month.month as usize - 1].clone();
                    year_menu_button(
                        ctx,
                        label,
                        move || {
                            panel_toggle.set(if is_open {
                                DockedPanel::Calendar
                            } else {
                                DockedPanel::Months
                            });
                        },
                        colors,
                        !years_open,
                        MONTH_MENU_TAG,
                    );
                    let forward = month_rows.clone();
                    let forward_flight = month_step_in_flight.clone();
                    let forward_anchor = month_rows.first_visible();
                    step_arrow(ctx, move || {
                        let target = arrow_target(forward_anchor, forward_flight.get(), true, last_page);
                        forward_flight.set(Some(target));
                        forward.animate_scroll_to_item(target, 0.0);
                    }, month_rows.can_scroll_forward(), CHEVRON_RIGHT_PATH, navigation_color, IconButtonSize::XSmall, !panel_open);
                });
            // Year group: step arrows around the year; the button swaps in the year list.
            Row::new()
                .arrangement(Arrangement::Start)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    let displayed_year = month.year;
                    let year_range = state.year_range();
                    let state_for_step = state.clone();
                    step_arrow(
                        ctx,
                        move || state_for_step.set_displayed_year(displayed_year - 1),
                        year_range.contains(&(displayed_year - 1)),
                        CHEVRON_LEFT_PATH,
                        navigation_color,
                        IconButtonSize::XSmall,
                        !panel_open,
                    );
                    let panel_toggle = panel.clone();
                    let is_open = years_open;
                    let rows_for_scroll = year_rows.clone();
                    let state_for_scroll = state.clone();
                    year_menu_button(
                        ctx,
                        displayed_year.to_string(),
                        move || {
                            if is_open {
                                panel_toggle.set(DockedPanel::Calendar);
                            } else {
                                // Park the list one row above the displayed year before it opens
                                // (mirrors `DatePicker`'s toggle).
                                rows_for_scroll.scroll_to_item(
                                    year_panel_first_row(
                                        &state_for_scroll,
                                        state_for_scroll.calendar_model(),
                                    ),
                                    0.0,
                                );
                                panel_toggle.set(DockedPanel::Years);
                            }
                        },
                        colors,
                        !months_open,
                        YEAR_MENU_TAG,
                    );
                    let state_for_step = state.clone();
                    step_arrow(
                        ctx,
                        move || state_for_step.set_displayed_year(displayed_year + 1),
                        year_range.contains(&(displayed_year + 1)),
                        CHEVRON_RIGHT_PATH,
                        navigation_color,
                        IconButtonSize::XSmall,
                        !panel_open,
                    );
                });
        });
}

/// The checkmark that flags the displayed month in the month list.
pub const CHECK_PATH: &str = "M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";

/// The month list: the twelve months in four rows of three pill cells, exactly as tall as the calendar it
/// replaces. Three columns do fit: the month pills are 104 dp wide (the year cells' 72 dp plus room for the
/// longest English name), not 72 dp.
fn month_list(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    month: &CalendarMonth,
    panel: &State<DockedPanel>,
    colors: &DatePickerColors,
) {
    let names = model.locale().month_names.clone();
    let displayed_year = month.year;
    let displayed_month = month.month;
    let year_enabled = state.selectable_dates().is_selectable_year(displayed_year);
    // The item closure outlives this call, so it only captures owned clones (mirrors `year_panel`).
    let cell_state = state.clone();
    let cell_model = model.clone();
    let cell_panel = panel.clone();
    let cell_colors = colors.clone();
    let cell_names = names.clone();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::YEAR_PANEL_HEIGHT + DatePickerDefaults::DIVIDER_THICKNESS),
        )
        .arrangement(Arrangement::Start)
        .build(ctx, |ctx| {
            // Same chrome as the year panel: a divider closes the list at both ends, and the grid
            // gives back 1 dp so the 336 dp total still matches the calendar it replaces.
            Divider::horizontal().build(ctx);
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                // The panel keeps the calendar's height, so the four rows are spread over the whole of
                // it (`SpaceEvenly` on the column) instead of huddling in the middle — `Center` leaves
                // 155 dp of blank above and below. Inside a row, `Start` plus an explicit
                // `CELL_SPACING` is what measures 12 dp across and 12 dp down: `SpaceEvenly` adds its
                // own extra ON TOP of `spacing` (see `flex.rs`), so three 104 dp pills and two 12 dp
                // gaps fill the 336 dp content width exactly.
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(DatePickerDefaults::YEAR_PANEL_HEIGHT - DatePickerDefaults::DIVIDER_THICKNESS),
                    )
                    .arrangement(Arrangement::SpaceEvenly)
                    .build(ctx, |ctx| {
                        for row in 0..4 {
                            Row::new()
                                .modifier(Modifier::new().fill_max_width())
                                .arrangement(Arrangement::Start)
                                .spacing(DatePickerDefaults::CELL_SPACING)
                                .alignment(Alignment::Center)
                                .build(ctx, |ctx| {
                                    for column in 0..3 {
                                        let month_no = (row * 3 + column + 1) as u32;
                                        let label = cell_names[month_no as usize - 1].clone();
                                        let selected = month_no == displayed_month;
                                        let state_for_pick = cell_state.clone();
                                        let model_for_pick = cell_model.clone();
                                        let panel_for_close = cell_panel.clone();
                                        let surface = Surface::new()
                                            .shape(Shape::Pill)
                                            .color(cell_colors.year_container(selected, year_enabled))
                                            .content_color(
                                                cell_colors.year_label(false, selected, year_enabled),
                                            )
                                            .enabled(year_enabled)
                                            .selectable(selected, move || {
                                                let target =
                                                    model_for_pick.month_of(displayed_year, month_no);
                                                state_for_pick.set_displayed_month_millis(
                                                    target.start_utc_time_millis,
                                                );
                                                panel_for_close.set(DockedPanel::Calendar);
                                            })
                                            .modifier(
                                                Modifier::new()
                                                    .size(
                                                        DatePickerDefaults::MONTH_CELL_WIDTH,
                                                        DatePickerDefaults::YEAR_CELL_HEIGHT,
                                                    )
                                                    .test_tag(format!(
                                                        "{MONTH_CELL_TAG_PREFIX}{month_no}"
                                                    )),
                                            );
                                        surface.build(ctx, |ctx| {
                                            Stack::new()
                                                .alignment(Alignment::Center)
                                                .modifier(Modifier::new().fill_max_size())
                                                .build(ctx, |ctx| {
                                                    Text::new(label).build(ctx);
                                                });
                                        });
                                    }
                                });
                        }
                    });
            });
            Divider::horizontal().build(ctx);
        });
}

/// The Cancel/OK action row at the picker's end: the dismiss button first, the confirm button second.
///
/// A row with only one of the two has no gap to draw, so the spacer is composed only when both buttons are.
fn docked_action_row(
    ctx: &mut ComposeCtx,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    let both_present = on_confirm.is_some() && on_dismiss.is_some();
    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding_end(DatePickerDefaults::MODAL_BUTTONS_END_PADDING)
                .padding_bottom(DatePickerDefaults::MODAL_BUTTONS_BOTTOM_PADDING),
        )
        .arrangement(Arrangement::End)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            if let Some(on_dismiss) = on_dismiss {
                Button::text()
                    .on_click(move || on_dismiss())
                    .build(ctx, |ctx| {
                        Text::new("Cancel").build(ctx);
                    });
            }
            if both_present {
                Spacer::horizontal(DatePickerDefaults::MODAL_BUTTONS_SPACING).build(ctx);
            }
            if let Some(on_confirm) = on_confirm {
                Button::text()
                    .on_click(move || on_confirm())
                    .build(ctx, |ctx| {
                        Text::new("OK").build(ctx);
                    });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compose's `rememberDatePickerState` ends with `apply { this.selectableDates =
    /// selectableDates }` (`DatePicker.kt:384-389`, the property being `by mutableStateOf`), so the
    /// policy is LIVE while the initial values are the ones taken once.
    ///
    /// Both halves are asserted on purpose: a test that only looked at the second composition would
    /// pass if the state were rebuilt from scratch, and one that only looked at the first would pass
    /// if the policy were frozen at construction.
    #[test]
    fn remember_date_picker_state_takes_a_new_policy_but_keeps_its_initial_values() {
        struct RefusesEpoch;
        impl SelectableDates for RefusesEpoch {
            fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
                utc_time_millis != 0
            }
        }

        let mut composer = crate::runtime::composer::Composer::new();
        let mut seen: Option<DatePickerState> = None;
        let mut frame = |composer: &mut crate::runtime::composer::Composer,
                         policy: Arc<dyn SelectableDates>,
                         seen: &mut Option<DatePickerState>| {
            composer.compose(|ctx| {
                let state = remember_date_picker_state(
                    ctx,
                    CalendarLocale::default(),
                    DatePickerStateInit {
                        initial_selected_date_millis: Some(0),
                        selectable_dates: policy.clone(),
                        ..DatePickerStateInit::default()
                    },
                );
                *seen = Some(state);
            });
        };

        frame(&mut composer, Arc::new(AllDates), &mut seen);
        let first = seen.clone().expect("a state");
        assert!(
            first.selectable_dates().is_selectable_date(0),
            "the first composition's policy is the one in force"
        );
        assert_eq!(first.selected_date_millis(), Some(0), "and its initial selection");

        // A NEW Arc, which is what a caller building its policy inline hands over.
        frame(&mut composer, Arc::new(RefusesEpoch), &mut seen);
        let second = seen.clone().expect("a state");
        assert!(
            !second.selectable_dates().is_selectable_date(0),
            "the second composition's policy is taken up, not the one remembered with"
        );
        assert_eq!(
            second.selected_date_millis(),
            Some(0),
            "while the initial values stay the ones taken at construction"
        );
    }

    /// A locale whose input format orders the fields a particular way.
    fn locale_with_input_format(pattern: &str) -> CalendarLocale {
        CalendarLocale {
            date_input_format: DateInputFormat::from_pattern(pattern)
                .unwrap_or_else(|| panic!("{pattern} is a valid input pattern")),
            ..CalendarLocale::default()
        }
    }

    /// A locale format pattern is widened to two-digit day and month and a four-digit year, and
    /// everything that is not `d`, `M`, `y` or a separator is dropped — the cleanup
    /// `datePatternAsInputFormat` does (`CalendarModel.kt:296-315`).
    #[test]
    fn a_locale_pattern_is_normalized_into_an_input_format() {
        let cases = [
            ("M/d/yyyy", "MM/dd/yyyy", '/'),
            ("dd.MM.yyyy", "dd.MM.yyyy", '.'),
            ("y/M/d", "yyyy/MM/dd", '/'),
            ("yyyy-MM-dd", "yyyy-MM-dd", '-'),
            ("d/M/yy", "dd/MM/yyyy", '/'),
        ];
        for (locale_pattern, expected, delimiter) in cases {
            let format = DateInputFormat::from_pattern(locale_pattern).expect(locale_pattern);
            assert_eq!(format.pattern_with_delimiters(), expected, "{locale_pattern}");
            assert_eq!(format.delimiter(), delimiter, "{locale_pattern}");
            assert_eq!(format.pattern_length(), 8, "{locale_pattern}");
        }
    }

    /// A pattern that does not name each field exactly once, or that carries no separator, has
    /// no input format: there would be nowhere to put a separator and no way to cut the digits.
    #[test]
    fn a_pattern_that_cannot_name_three_fields_yields_no_format() {
        for pattern in ["MM/dd", "MM/dd/yyyy/yyyy", "dd/MM", "", "HH:mm", "dMy"] {
            assert!(
                DateInputFormat::from_pattern(pattern).is_none(),
                "{pattern} should not become an input format"
            );
        }
    }

    /// The field order is what the locale decides, and the delimiters sit where the pattern puts
    /// them — both of which the visual transformation and the parser key off.
    #[test]
    fn the_field_order_and_delimiter_offsets_come_off_the_pattern() {
        let cases = [
            ("MM/dd/yyyy", DateInputFieldOrder::MonthDayYear, 2, 5),
            ("dd/MM/yyyy", DateInputFieldOrder::DayMonthYear, 2, 5),
            ("yyyy/MM/dd", DateInputFieldOrder::YearMonthDay, 4, 7),
            ("dd.MM.yyyy", DateInputFieldOrder::DayMonthYear, 2, 5),
        ];
        for (pattern, order, first, last) in cases {
            let format = DateInputFormat::from_pattern(pattern).expect(pattern);
            assert_eq!(format.field_order(), order, "{pattern}");
            assert_eq!(format.first_delimiter_offset(), first, "{pattern}");
            assert_eq!(format.last_delimiter_offset(), last, "{pattern}");
            assert_eq!(format.pattern_without_delimiters().len(), 8, "{pattern}");
        }
    }

    /// The same digits read through three orderings are three different dates, which is the whole
    /// point of the pattern carrying the order.
    #[test]
    fn the_same_digits_read_into_three_different_dates() {
        let model = CalendarModel::new(CalendarLocale::default());
        let march_first = CalendarDate::new(2024, 3, 1).expect("2024-03-01 is a date");

        let us = model.date_input_format().clone();
        assert_eq!(us.field_order(), DateInputFieldOrder::MonthDayYear);
        assert_eq!(model.parse("03012024", &us), Some(march_first));
        assert_eq!(model.format_with_pattern(march_first.start_of_day_millis(), &us), "03012024");

        let gb = DateInputFormat::from_pattern("dd/MM/yyyy").expect("dd/MM/yyyy");
        assert_eq!(gb.field_order(), DateInputFieldOrder::DayMonthYear);
        assert_eq!(model.parse("01032024", &gb), Some(march_first));
        assert_eq!(model.format_with_pattern(march_first.start_of_day_millis(), &gb), "01032024");

        let jp = DateInputFormat::from_pattern("yyyy/MM/dd").expect("yyyy/MM/dd");
        assert_eq!(jp.field_order(), DateInputFieldOrder::YearMonthDay);
        assert_eq!(model.parse("20240301", &jp), Some(march_first));
        assert_eq!(model.format_with_pattern(march_first.start_of_day_millis(), &jp), "20240301");
    }

    /// Every day of a leap February round-trips through every field order: this is the property
    /// the field falls back on when the user edits the selection, not just the placeholder.
    #[test]
    fn a_leap_day_round_trips_through_every_field_order() {
        let model = CalendarModel::new(CalendarLocale::default());
        let leap_day = CalendarDate::new(2024, 2, 29).expect("2024 is a leap year");
        for pattern in ["MM/dd/yyyy", "dd/MM/yyyy", "yyyy/MM/dd"] {
            let format = DateInputFormat::from_pattern(pattern).expect(pattern);
            let digits = model.format_with_pattern(leap_day.start_of_day_millis(), &format);
            assert_eq!(digits.len(), 8, "{pattern}");
            assert_eq!(model.parse(&digits, &format), Some(leap_day), "{pattern}");
        }
    }

    /// An incomplete or non-numeric entry has no answer, and so does a complete one that names a
    /// day its month does not have. This is the first of the validator's three checks.
    #[test]
    fn parsing_refuses_partial_non_numeric_and_impossible_dates() {
        let model = CalendarModel::new(CalendarLocale::default());
        let us = model.date_input_format();
        for digits in ["", "0", "03", "030", "03012", "0301202", "030120242"] {
            assert_eq!(model.parse(digits, us), None, "{digits:?} is incomplete");
        }
        for digits in [" 3012024", "03-012024", "0301202a", "０３０１"] {
            assert_eq!(model.parse(digits, us), None, "{digits:?} is not digits");
        }
        // 2024-02-30, 2023-02-29 (not a leap year), month 13, day 0.
        for digits in ["02302024", "02292023", "13312024", "00012024"] {
            assert_eq!(model.parse(digits, us), None, "{digits:?} is not a date");
        }
        // The same day the month does have, one field-order over, is not a rescue.
        let gb = DateInputFormat::from_pattern("dd/MM/yyyy").expect("dd/MM/yyyy");
        assert_eq!(model.parse("30022024", &gb), None);
    }

    /// Dates before the epoch keep their sign-free four-digit year, because the year is written
    /// and read as exactly four digits.
    #[test]
    fn a_date_before_the_epoch_keeps_a_four_digit_year() {
        let model = CalendarModel::new(CalendarLocale::default());
        let us = model.date_input_format().clone();
        let digits = model.format_with_pattern(0, &us);
        assert_eq!(digits, "01011970");
        assert_eq!(model.parse(&digits, &us), CalendarDate::new(1970, 1, 1));
    }

    /// A picker with everything at its defaults: 1900..=2100, every date selectable.
    fn picker() -> DatePickerState {
        DatePickerState::new(CalendarLocale::default())
    }

    /// The three checks run in material3's order and each says what it has to say. The order is
    /// the point of the first two cases: an unparseable entry never reaches the range check, and a
    /// date outside the range is reported as the range rather than as the policy refusing it.
    #[test]
    fn the_input_field_reports_pattern_range_and_policy_failures_apart() {
        let state = picker();

        assert_eq!(
            state.validate_date_input(None),
            "Date does not match expected pattern: MM/DD/YYYY"
        );

        let outside = CalendarDate::new(1800, 5, 4).expect("1800-05-04 is a date");
        assert_eq!(
            state.validate_date_input(Some(outside)),
            "Date out of expected year range 1900 - 2100"
        );

        assert_eq!(state.validate_date_input(CalendarDate::new(2024, 3, 1)), "");
    }

    /// The year range's two ends land in their own slots. A formatter that filled both with one
    /// value would still look plausible on a single-year range, so this uses a wide one.
    #[test]
    fn the_year_range_message_names_both_ends() {
        let message = picker().validate_date_input(CalendarDate::new(1800, 5, 4));
        assert_eq!(message, "Date out of expected year range 1900 - 2100");
        assert!(message.contains("1900"), "{message}");
        assert!(message.contains("2100"), "{message}");
    }

    /// A policy that refuses a day is told so by name and date, not by a generic failure.
    #[test]
    fn a_refused_day_is_named_rather_than_reported_as_a_range_problem() {
        struct OnlyWeekdays;
        impl SelectableDates for OnlyWeekdays {
            fn is_selectable_date(&self, utc_time_millis: i64) -> bool {
                // 2024-03-01 is a Friday; 2024-03-02 a Saturday.
                date_of_millis(utc_time_millis).day != 2
            }
        }
        let state = DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                selectable_dates: Arc::new(OnlyWeekdays),
                ..DatePickerStateInit::default()
            },
        );
        assert_eq!(state.validate_date_input(CalendarDate::new(2024, 3, 1)), "");
        let refused = state.validate_date_input(CalendarDate::new(2024, 3, 2));
        assert_eq!(refused, "Date not allowed: Mar 2, 2024");
    }

    /// A year the policy refuses stops every date in it, even a day the policy would otherwise
    /// allow — material3 checks `isSelectableYear` in the same condition (`DateInput.kt:330-333`).
    #[test]
    fn a_year_the_policy_refuses_stops_every_date_in_it() {
        struct NotTwentyTwentyFour;
        impl SelectableDates for NotTwentyTwentyFour {
            fn is_selectable_year(&self, year: i32) -> bool {
                year != 2024
            }
        }
        let state = DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                selectable_dates: Arc::new(NotTwentyTwentyFour),
                ..DatePickerStateInit::default()
            },
        );
        // Every March date would otherwise pass, so only the year check can reject this.
        let refused = state.validate_date_input(CalendarDate::new(2024, 3, 1));
        assert_eq!(refused, "Date not allowed: Mar 1, 2024");
        assert_eq!(state.validate_date_input(CalendarDate::new(2025, 3, 1)), "");
    }

    /// The pattern message carries the locale's own pattern, uppercased, so a reader knows what
    /// shape was expected — and a non-en-US locale says its own.
    #[test]
    fn the_pattern_message_names_the_locales_own_pattern() {
        let state = DatePickerState::new(locale_with_input_format("yyyy/MM/dd"));
        assert_eq!(
            state.validate_date_input(None),
            "Date does not match expected pattern: YYYY/MM/DD"
        );
    }

    /// The substitution Compose relies on: two arguments fill their own slots, a missing one
    /// leaves an empty hole instead of panicking, and text that only looks like a placeholder
    /// passes through.
    #[test]
    fn format_string_fills_numbered_placeholders_in_order() {
        assert_eq!(format_string("a {1} b {2} c", &["1", "2"]), "a 1 b 2 c");
        assert_eq!(format_string("{2} then {1}", &["first", "second"]), "second then first");
        assert_eq!(format_string("{1} only", &[]), " only");
        assert_eq!(format_string("no placeholders", &["x"]), "no placeholders");
        assert_eq!(format_string("{name} stays", &["x"]), "{name} stays");
        // One-based, as `%1$s` is: `{0}` is out of range and fills nothing.
        assert_eq!(format_string("{1} {0} {2}", &["a", "b"]), "a  b");
        assert_eq!(format_string("unclosed {1", &["a"]), "unclosed {1");
    }

    /// The delimiters appear as the digits are typed, one after the field they close, and a full
    /// entry shows the pattern exactly.
    #[test]
    fn the_visual_transformation_writes_the_delimiters_as_the_digits_arrive() {
        let format = DateInputFormat::from_pattern("MM/dd/yyyy").expect("MM/dd/yyyy");
        let transformation = DateVisualTransformation::new(&format);
        let shown = |digits: &str| transformation.filter(digits).text;
        assert_eq!(shown(""), "");
        assert_eq!(shown("0"), "0");
        assert_eq!(shown("03"), "03/");
        assert_eq!(shown("030"), "03/0");
        assert_eq!(shown("0301"), "03/01/");
        assert_eq!(shown("030120"), "03/01/20");
        assert_eq!(shown("03012024"), "03/01/2024");
        // A longer entry is cut at a full field's width rather than pushing the end out.
        assert_eq!(shown("03012024999"), "03/01/2024");
    }

    /// A non-default field order puts its delimiters where its own pattern does: `yyyy/MM/dd`
    /// closes a four-digit year first, so the first separator lands after the fourth digit and
    /// the display is two characters longer before it appears.
    #[test]
    fn the_visual_transformation_follows_the_locale_field_order() {
        let cases = [
            ("MM/dd/yyyy", "03012024", "03/01/2024", [(2, "03/"), (4, "03/01/")]),
            ("dd/MM/yyyy", "01032024", "01/03/2024", [(2, "01/"), (4, "01/03/")]),
            ("yyyy/MM/dd", "20240301", "2024/03/01", [(4, "2024/"), (6, "2024/03/")]),
            ("dd.MM.yyyy", "01032024", "01.03.2024", [(2, "01."), (4, "01.03.")]),
        ];
        for (pattern, digits, full, prefixes) in cases {
            let format = DateInputFormat::from_pattern(pattern).expect(pattern);
            let transformation = DateVisualTransformation::new(&format);
            assert_eq!(transformation.filter(digits).text, full, "{pattern}");
            for (width, prefix) in prefixes {
                assert_eq!(
                    transformation.filter(&digits[..width]).text,
                    prefix,
                    "{pattern} at {width} digits"
                );
            }
        }
    }

    /// Every caret position maps to the position of the same digit both ways, and past the end
    /// both clamp. This is the whole contract of the two methods: get one wrong and the caret
    /// jumps a character as soon as it crosses a delimiter.
    #[test]
    fn every_caret_position_maps_to_the_same_digit_both_ways() {
        let format = DateInputFormat::from_pattern("MM/dd/yyyy").expect("MM/dd/yyyy");
        let transformation = DateVisualTransformation::new(&format);
        let mapping = transformation.filter("03012024").offset_mapping;

        // Caret before each digit: typed offset -> shown offset -> back.
        for original in 0..=8 {
            let shown = mapping.original_to_transformed(original);
            assert!(shown <= 10, "{original} -> {shown} ran past the end");
            assert_eq!(mapping.transformed_to_original(shown), original, "offset {original}");
        }
        // The exact pairs, so a change in one direction cannot hide behind the round trip.
        let pairs = [(0, 0), (1, 1), (2, 3), (3, 4), (4, 5), (5, 7), (6, 8), (7, 9), (8, 10)];
        for (original, shown) in pairs {
            assert_eq!(mapping.original_to_transformed(original), shown, "forward {original}");
            assert_eq!(mapping.transformed_to_original(shown), original, "back {shown}");
        }
        // Past the entry, both directions clamp to its end rather than walking off.
        assert_eq!(mapping.original_to_transformed(9), 10);
        assert_eq!(mapping.original_to_transformed(200), 10);
        assert_eq!(mapping.transformed_to_original(11), 8);
        assert_eq!(mapping.transformed_to_original(200), 8);
    }

    /// The offsets are read off the pattern, so the year-first order's mapping is not the
    /// month-first one's shifted — it is a different function.
    #[test]
    fn the_year_first_order_maps_its_own_offsets() {
        let format = DateInputFormat::from_pattern("yyyy/MM/dd").expect("yyyy/MM/dd");
        let mapping = DateVisualTransformation::new(&format).filter("20240301").offset_mapping;
        let pairs = [(0, 0), (3, 3), (4, 5), (6, 7), (7, 9), (8, 10)];
        for (original, shown) in pairs {
            assert_eq!(mapping.original_to_transformed(original), shown, "forward {original}");
            assert_eq!(mapping.transformed_to_original(shown), original, "back {shown}");
        }
        for original in 0..=8 {
            let shown = mapping.original_to_transformed(original);
            assert_eq!(mapping.transformed_to_original(shown), original, "offset {original}");
        }
    }

    /// The month arrows step one page per press, in BOTH directions, however fast the presses come.
    ///
    /// The asymmetry this pins was reported from the demo: three presses 120 ms apart on `prev`
    /// advanced two months, the same three on `next` advanced none. The cause is that `first_visible`
    /// names the page whose span still contains the offset, so during a forward animation it is the page
    /// being LEFT — `anchor + 1` then names the page already in flight, the request deduplicates against
    /// it, and the press is lost. Backward flips the anchor the moment the offset leaves the old page,
    /// so `anchor - 1` names something new.
    ///
    /// Stepping from the page already REQUESTED removes the direction from the outcome. Each case below
    /// asserts the whole trajectory of targets, not just the last one: a fix that happened to land on
    /// the right page while skipping one would pass a final-state check.
    #[test]
    fn a_second_press_during_the_animation_steps_again_in_both_directions() {
        const LAST: usize = 2411;

        // Three presses during one animation, starting from page 100. Forward: 101, 102, 103.
        let mut in_flight = None;
        let mut targets = Vec::new();
        for _ in 0..3 {
            let t = arrow_target(100, in_flight, true, LAST);
            in_flight = Some(t);
            targets.push(t);
        }
        assert_eq!(
            targets,
            vec![101, 102, 103],
            "three forward presses during one animation must be three pages, not one repeated target"
        );

        // And backward, from the same anchor: 99, 98, 97.
        let mut in_flight = None;
        let mut targets = Vec::new();
        for _ in 0..3 {
            let t = arrow_target(100, in_flight, false, LAST);
            in_flight = Some(t);
            targets.push(t);
        }
        assert_eq!(targets, vec![99, 98, 97], "and the two directions must agree in shape");

        // A press after the motion is honoured falls back to the anchor, which by then agrees with the
        // in-flight page — the sync clears the memory when the list arrives, so this is the same answer
        // either way rather than a stale one.
        assert_eq!(
            arrow_target(103, None, true, LAST),
            104,
            "with nothing in flight a press steps from where the list actually is"
        );
    }

    /// The ends. An arrow is disabled there, but a step must still be well-defined rather than wrapping.
    #[test]
    fn the_arrow_targets_clamp_at_both_ends_of_the_range() {
        assert_eq!(arrow_target(0, None, false, 10), 0, "no page before the first");
        assert_eq!(arrow_target(0, Some(0), false, 10), 0, "nor from an in-flight request at the start");
        assert_eq!(arrow_target(10, None, true, 10), 10, "no page past the last");
        assert_eq!(
            arrow_target(10, Some(10), true, 10),
            10,
            "nor from an in-flight request at the end — the request cannot climb past the clamp"
        );
        assert_eq!(arrow_target(usize::MAX, None, false, 10), usize::MAX - 1, "a step down never wraps");
    }

    /// The bound on [`sync_month_pages`]'s two waits, and the reason it exists.
    ///
    /// `is_scrolling()` is NOT a flag with one owner — it is the same `State` the drag, fling,
    /// programmatic-jump and cancellation paths all write, and at least two of those can leave it set:
    /// `cancel_animation_by_id` / `clear_animations_for_states` drop an animation with `retain` and never
    /// run its finish callback (`animation.rs:580-583`, `:43-49`), and both `drag_scroll_up` paths return
    /// before their reset when the drag's target node has gone (`app.rs:3021`, `app.rs:3936`). Compose's
    /// guard is safe because a single owner maintains that flag with a guaranteed reset; here it is
    /// shared, so an unbounded wait would turn ONE leaked flag into a month label frozen for the life of
    /// the window — strictly worse than the gesture cancellation the guard exists to prevent.
    ///
    /// The observable is `jump_request`: the jump is issued through `scroll_to_item`, which parks the
    /// request for the measure to consume, and a bare `compose` never measures — so the request is still
    /// sitting there to be read. Two things are asserted, and both matter: that the deferral happens at
    /// all (a sync that barged in immediately would cancel the gesture it is guarding), and that it ENDS
    /// (which is the anti-freeze half).
    #[test]
    fn a_stuck_scroll_flag_cannot_freeze_the_month_sync_forever() {
        let list = LazyListState::new();
        let model = CalendarModel::new(CalendarLocale::default());
        let state = DatePickerState::with(
            CalendarLocale::default(),
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                today_millis: Some(millis(2024, 9, 5)),
                ..Default::default()
            },
        );
        let first_month = model.month_of(*state.year_range().start(), 1).start_utc_time_millis;
        // A gesture that never finishes. `first_visible` stays 0, so the sync is asked to take the list
        // to September 2024's page (1496) and can never see it arrive.
        list.is_scrolling.set(true);
        assert_ne!(
            state.displayed_month_millis(),
            model.plus_months(first_month, list.first_visible() as i64).start_utc_time_millis,
            "the test is vacuous unless the state's month differs from the page the list is on"
        );

        let mut composer = crate::runtime::composer::Composer::new();
        let mut deferred = 0usize;
        let mut issued_on = None;
        let in_flight = State::new(None::<usize>);
        for frame in 1..=(MAX_WAIT_FRAMES as usize + 4) {
            composer.compose(|ctx| {
                sync_month_pages(ctx, &state, &model, &list, first_month, &in_flight);
            });
            if list.jump_request.peek().is_some() {
                issued_on = Some(frame);
                break;
            }
            deferred += 1;
        }

        let issued_on = issued_on.expect(
            "the sync never gave up waiting: a leaked `is_scrolling` would freeze the month forever",
        );
        assert!(
            issued_on > 1,
            "the sync must defer at least a frame before giving up — it waited {deferred} frame(s) \
             and then jumped on frame {issued_on}, which is the gesture cancellation the guard prevents"
        );
        assert_eq!(
            issued_on,
            MAX_WAIT_FRAMES as usize + 1,
            "the deferral ends exactly at the bound, not early and not late"
        );

        // And the flag is still the input, not the output: a list that reports it is idle jumps on the
        // very first frame.
        let idle = LazyListState::new();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            sync_month_pages(ctx, &state, &model, &idle, first_month, &State::new(None::<usize>));
        });
        assert!(
            idle.jump_request.peek().is_some(),
            "an idle list is taken to the month immediately — the wait is only for a scroll in flight"
        );
    }

    #[test]
    fn the_epoch_is_a_thursday() {
        // 1970-01-01 is the reference point of the whole module, and Thursday is 4 when Monday is 1.
        assert_eq!(CalendarDate::new(1970, 1, 1).unwrap().day_of_week(), 4);
        assert_eq!(CalendarDate::new(1970, 1, 1).unwrap().days_since_epoch(), 0);
    }

    /// "Today" is the LOCAL calendar date, stamped at UTC midnight — Compose's `LocalDate.now()` then
    /// `.atTime(MIDNIGHT).atZone(utcTimeZoneId)` (`CalendarModelImpl.android.kt:48-62`). Reading the UTC
    /// instant directly rings the previous day for every user whose local date has already rolled over.
    #[test]
    fn today_is_the_local_date_stamped_at_utc_midnight() {
        const HOUR: i64 = 3_600_000;
        /// 2024-09-01T00:00Z, the UTC midnight the local dates below are stamped at.
        const SEP_1: i64 = 1_725_148_800_000;
        // UTC+8 at 01:00 local on the 2nd: the UTC instant is still the 1st, the local date is not.
        let after_local_midnight = SEP_1 + 17 * HOUR; // 2024-09-01T17:00Z
        assert_eq!(date_of_millis(after_local_midnight), CalendarDate { year: 2024, month: 9, day: 1 });
        assert_eq!(
            date_of_millis(today_at(after_local_midnight, 8 * HOUR)),
            CalendarDate { year: 2024, month: 9, day: 2 },
            "at UTC+8 the local date is already the 2nd"
        );
        // The same instant in UTC is still the 1st — which is the bug this offset exists to avoid.
        assert_eq!(
            date_of_millis(today_at(after_local_midnight, 0)),
            CalendarDate { year: 2024, month: 9, day: 1 }
        );
        // West of UTC the correction runs the other way: UTC-5 at 20:00 local on the 1st is already the
        // 2nd in UTC, and the local date is the one that has to win.
        assert_eq!(
            date_of_millis(today_at(SEP_1 + 25 * HOUR, -5 * HOUR)),
            CalendarDate { year: 2024, month: 9, day: 1 },
            "at UTC-5 the local date is still the 1st while UTC has moved on"
        );
        // The stamped value is still a UTC day boundary, which is what every other reader here assumes,
        // and a real zone offset can only move the answer by the day it actually straddles.
        let instant = SEP_1 + 9 * HOUR; // 2024-09-01T09:00Z
        for offset in [-12 * HOUR, -5 * HOUR, 0, 5 * HOUR + 30 * 60_000, 14 * HOUR] {
            let today = today_at(instant, offset);
            assert_eq!(today % MILLIS_IN_24_HOURS, 0, "today {today} is not on a UTC day boundary");
            assert!(
                (today - canonical_millis(instant)).abs() <= MILLIS_IN_24_HOURS,
                "an offset of {offset} moved today {today} more than a day from the instant"
            );
        }
    }

    /// The live probe behind it: whatever zone this machine is in, the offset has to be one a real zone
    /// could carry. A garbage answer here is the one failure mode `today_at` cannot catch.
    ///
    /// This is a SMOKE CHECK, not a regression guard. A probe hard-wired to return 0 satisfies both
    /// assertions on every machine, and 0 is the correct answer on a UTC machine anyway — so this says
    /// "the zone lookup returned something legal", nothing more. The arithmetic is pinned separately by
    /// `today_is_the_local_date_stamped_at_utc_midnight` with synthetic offsets, and the wiring is
    /// pinned by `today_millis_reads_the_local_day_not_the_utc_one`.
    #[test]
    fn the_local_offset_is_a_whole_number_of_minutes_in_range() {
        let offset = local_utc_offset_millis();
        assert_eq!(offset % 60_000, 0, "offset {offset} is not a whole number of minutes");
        assert!(
            offset.abs() <= 14 * 3_600_000,
            "offset {offset} is outside the real range of UTC-12..UTC+14"
        );
    }

    /// The production call, not the helper: `CalendarModel::today_millis` must be the LOCAL day.
    ///
    /// ⚠ This can only bite while the machine is inside the window where its local date and its UTC date
    /// disagree — eight hours a day at UTC+8, zero on a UTC machine. That is a real limitation, not a
    /// formality: the clock is read inside the function and there is no seam to move it, so the one
    /// assertion that distinguishes "applies the offset" from "reads the UTC instant" is only available
    /// during that window. It is written to be skipped loudly rather than to pass quietly, so a suite run
    /// that never exercises it says so. The arithmetic is deterministic and covered by the test above;
    /// this covers the composition.
    #[test]
    fn today_millis_reads_the_local_day_not_the_utc_one() {
        let offset = local_utc_offset_millis();
        let model = CalendarModel::new(CalendarLocale::default());
        let answered = model.today_millis();

        if offset == 0 {
            eprintln!("today_millis_reads_the_local_day_not_the_utc_one: skipped — this machine is UTC, \
                       where the UTC day IS the local day and the two cannot be told apart");
            return;
        }
        // The two days, read the same way the function does, a moment either side of the call. A
        // microsecond of clock movement between the three reads is the only slack allowed.
        let utc_day = canonical_millis(now_millis());
        let local_day = today_at(now_millis(), offset);
        if utc_day == local_day {
            eprintln!(
                "today_millis_reads_the_local_day_not_the_utc_one: skipped — outside the window where \
                 this machine's local date and UTC date disagree; rerun within {} hours of local midnight",
                offset.abs() / 3_600_000
            );
            return;
        }
        assert_eq!(
            answered, local_day,
            "today_millis answered the UTC day ({answered}) while the local date is a different one"
        );
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
    fn the_cells_outside_a_month_hold_the_neighbouring_month() {
        let model = CalendarModel::new(CalendarLocale::default());
        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);

        // Sunday-first, 2024-09-01 is a Sunday: no leading cells, and four cells after the 30th carry
        // October's first days — 2024-09-30 is a Monday, so the month ends six cells in.
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &AllDates);
        assert_eq!(grid.cells()[0].day, 1, "no leading cells");
        assert_eq!(grid.cells()[29].day, 30);
        assert!(!grid.cells()[..30].iter().any(|cell| cell.is_outside_month));
        let tail: Vec<u32> = grid.cells()[30..].iter().map(|cell| cell.day).collect();
        assert_eq!(tail, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert!(grid.cells()[30..].iter().all(|cell| cell.is_outside_month));
        // October's cells carry October's dates, not September's day numbers shifted along.
        assert_eq!(date_of_millis(grid.cells()[30].utc_time_millis).month, 10);

        // Monday-first, the same month starts six cells in, and August's last days fill the gap.
        let grid = MonthGrid::of(monday_first.month_of(2024, 9), None, 0, &AllDates);
        assert!(grid.cells()[..6].iter().all(|cell| cell.is_outside_month));
        let head: Vec<u32> = grid.cells()[..6].iter().map(|cell| cell.day).collect();
        assert_eq!(head, vec![26, 27, 28, 29, 30, 31], "August's last six days");
        assert_eq!(date_of_millis(grid.cells()[0].utc_time_millis).month, 8);
        assert_eq!(grid.cells()[6].day, 1);
        assert_eq!(grid.cells()[35].day, 30);
    }

    #[test]
    fn an_outside_month_cell_is_never_selectable() {
        // `SelectableDates` allows everything, so a disabled outside cell is the grid's own rule and not
        // the caller's: tapping a context day must not move the selection into a month on the other side
        // of the displayed one.
        let model = CalendarModel::new(CalendarLocale::default());
        let grid = MonthGrid::of(model.month_of(2024, 9), None, 0, &AllDates);
        assert!(
            grid.cells()
                .iter()
                .filter(|cell| cell.is_outside_month)
                .all(|cell| !cell.is_enabled)
        );
        assert_eq!(
            grid.cells().iter().filter(|cell| cell.is_enabled).count(),
            30,
            "exactly the displayed month stays enabled"
        );
    }

    #[test]
    fn an_outside_month_cell_still_knows_its_real_date() {
        // The flags describe the date, not the cell's role in the grid: an outside cell can be today or the
        // selected date, and `day_cell` is what decides to draw it as context.
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let october_first = month.start_utc_time_millis + 30 * MILLIS_IN_24_HOURS;
        let grid = MonthGrid::of(month, Some(october_first), october_first, &AllDates);
        let cell = grid.cells()[30];
        assert_eq!(cell.day, 1);
        assert!(cell.is_outside_month);
        assert!(cell.is_today, "today is reported wherever it lands");
        assert!(cell.is_selected);
        assert!(!cell.is_enabled);
    }

    #[test]
    fn an_outside_month_label_is_the_plain_role_at_disabled_alpha() {
        // The M3 specs tokens: "Date unselected outside month label text color" #1D1B20, "…text opacity"
        // 0.38. In the baseline light scheme #1D1B20 is onSurface, which is the plain day role.
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.outside_month_label().a, 97, "0.38 of 255 rounds to 97");
        assert_eq!(
            Color { a: 255, ..colors.outside_month_label() },
            Color { a: 255, ..colors.day_content },
            "same rgb as the plain day role, only the alpha differs"
        );
        assert_eq!(
            colors.outside_month_label(),
            colors.day_label(false, false, false),
            "the same expression as a disabled unselected day, which is what the two tokens add up to"
        );
        assert_eq!(
            colors.outside_month_label().a,
            (DatePickerDefaults::DISABLED_ALPHA * 255.0).round() as u8,
            "and that alpha is the theme's DisabledAlpha (0.38)"
        );
        assert_ne!(
            colors.outside_month_label(),
            colors.day_label(false, true, false),
            "a real day in the month is not dimmed"
        );
    }

    #[test]
    fn a_cell_carries_its_day_and_the_start_of_that_day() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let grid = MonthGrid::of(month, None, 0, &AllDates);
        let fifteenth = &grid.cells()[14];
        assert_eq!(fifteenth.day, 15);
        assert_eq!(
            fifteenth.utc_time_millis,
            month.start_utc_time_millis + 14 * MILLIS_IN_24_HOURS
        );
        assert_eq!(date_of_millis(fifteenth.utc_time_millis).day, 15);
        assert_eq!(grid.cell(2, 0).unwrap().day, 15, "row two, column zero");
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
            .filter(|cell| cell.is_enabled)
            .map(|cell| cell.day)
            .collect::<Vec<u32>>();
        assert_eq!(enabled, (1..=7).collect::<Vec<u32>>());

        // material3: a year that cannot be selected makes every date in it unselectable
        // (`DatePicker.kt:296-297`).
        let grid = MonthGrid::of(model.month_of(2025, 3), None, 0, &No2025);
        assert!(grid.cells().iter().all(|cell| !cell.is_enabled));
    }

    #[test]
    fn a_day_description_names_today_and_the_date() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2024, 9);
        let grid = MonthGrid::of(month, None, month.start_utc_time_millis, &AllDates);

        assert_eq!(
            day_content_description(&model, &grid.cells()[0]),
            "Today, Sunday, September 1, 2024"
        );
        assert_eq!(
            day_content_description(&model, &grid.cells()[1]),
            "Monday, September 2, 2024"
        );
        // An outside cell announces its own real date, which is what makes it context rather than noise.
        // Monday-first, September 2024 starts a week in (its 1st is a Sunday), so cell zero is six days
        // earlier — 2024-08-26, a Monday.
        let mut monday_first_locale = CalendarLocale::default();
        monday_first_locale.first_day_of_week = 1;
        let monday_first = CalendarModel::new(monday_first_locale);
        let grid = MonthGrid::of(monday_first.month_of(2024, 9), None, 0, &AllDates);
        assert_eq!(
            day_content_description(&model, &grid.cells()[0]),
            "Monday, August 26, 2024"
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
    fn a_year_label_follows_material3s_precedence() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        // The signature is `year_label(current_year, selected, enabled)`, the order material3's
        // `yearContentColor(currentYear, selected, enabled)` takes (`DatePicker.kt:1005-1027`).
        assert_eq!(
            colors.year_label(true, false, true),
            colors.today_content,
            "the current year reads as today's label"
        );
        assert_eq!(colors.year_label(false, false, true), colors.year_content);
        assert_eq!(colors.year_label(false, true, true), colors.selected_content);
        assert_eq!(
            colors.year_label(false, true, false),
            Color {
                a: 97,
                ..colors.selected_content
            },
            "a disabled selected year is OnPrimary at DisabledAlpha"
        );
        assert_eq!(
            colors.year_label(true, false, false),
            Color {
                a: 97,
                ..colors.year_content
            },
            "material3 falls through to the disabled branch, so the current year's colour does not survive the \
             disable"
        );
    }

    #[test]
    fn a_year_container_is_primary_only_when_the_year_is_displayed() {
        let colors = DatePickerColors::from_theme(&ThemeColors::default_light());
        assert_eq!(colors.year_container(true, true), colors.selected_container);
        assert_eq!(
            colors.year_container(true, false),
            Color {
                a: 97,
                ..colors.selected_container
            }
        );
        assert_eq!(colors.year_container(false, true), Color::TRANSPARENT);
        assert_eq!(colors.year_container(false, false), Color::TRANSPARENT);
    }

    #[test]
    fn the_year_panel_is_as_tall_as_the_calendar_it_stands_in_for() {
        // material3 takes the panel off `RecommendedSizeForAccessibility * (MaxCalendarRows + 1)` and draws the
        // divider below it inside that height (`DatePicker.kt:1634-1641`), so the panel plus its divider is the
        // weekday row (48) plus the month grid (288) exactly — opening it moves nothing above or below.
        assert_eq!(
            DatePickerDefaults::YEAR_PANEL_HEIGHT + DatePickerDefaults::DIVIDER_THICKNESS,
            DatePickerDefaults::ACCESSIBLE_SIZE + DatePickerDefaults::MONTH_HEIGHT
        );
        assert_eq!(
            DatePickerDefaults::DIVIDER_THICKNESS,
            crate::ui::divider::DividerDefaults::thickness()
        );
    }

    #[test]
    fn a_year_keeps_the_displayed_month() {
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
        // material3 scrolls its month list to `(year - yearRange.first) * 12 + displayedMonth.month - 1`, which
        // keeps the month of year (`DatePicker.kt:1643-1652`).
        state.set_displayed_year(2025);
        assert_eq!(text(&state), "September 2025");
        state.set_displayed_year(2023);
        assert_eq!(text(&state), "September 2023");
    }

    #[test]
    fn a_year_outside_the_range_is_ignored() {
        let state = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                year_range: 2000..=2100,
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let text = |state: &DatePickerState| {
            state
                .calendar_model()
                .format_month_year(state.displayed_month_millis())
        };
        state.set_displayed_year(1999);
        assert_eq!(text(&state), "September 2024");
        state.set_displayed_year(2101);
        assert_eq!(text(&state), "September 2024");
        state.set_displayed_year(2100);
        assert_eq!(text(&state), "September 2100");
    }

    #[test]
    fn the_year_panel_starts_one_row_above_the_displayed_year() {
        let displayed = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        let model = displayed.calendar_model().clone();
        // 2024 is item 124 of the range that starts at 1900 — row 41 of three columns — and material3's
        // `max(0, displayedYear - yearRange.first - YearsInRow)` item index (`DatePicker.kt:2073-2080`) is that
        // row less one.
        assert_eq!(year_panel_first_row(&displayed, &model), 40);
        // A displayed year at the range's start clamps to the first row rather than underflowing.
        let at_range_start = state(
            DatePickerStateInit {
                initial_displayed_month_millis: Some(millis(1900, 1, 1)),
                ..Default::default()
            },
            (2024, 9, 5),
        );
        assert_eq!(year_panel_first_row(&at_range_start, &model), 0);
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

    /// The M3 specs page draws the two variants differently and this pins it: the DOCKED anatomy lists
    /// "Outside month date" among the grid's states, the MODAL anatomy has no such entry, and material3
    /// agrees with the modal one (it composes a `Spacer`, `DatePicker.kt:1870-1890`). So a docked page
    /// fills all 42 slots and a modal page draws only its own month.
    ///
    /// Counted by what is INERT rather than by how many labels there are. The grid is a paged `LazyRow`
    /// now (material3's `HorizontalMonthsList`), so a frame composes a couple of dozen pages of mixed
    /// lengths and "42 a page against 30" is not a number the tree can be asked for. What it can be
    /// asked is the difference that does not depend on the page count: an outside-month cell is never
    /// enabled, so its `Surface` carries no `click`, while every real day of the month does.
    #[test]
    fn the_docked_grid_draws_the_neighbouring_months_days_and_the_modal_one_does_not() {
        let model = CalendarModel::new(CalendarLocale::default());
        let month = model.month_of(2026, 9);

        /// `(choosable day cells, inert day cells)`. A day label is one numeric text in 1..=31; it is a
        /// leaf, so its `Surface` is two levels up — past the centring `Stack` — and that `Surface` is
        /// clickable exactly when the day can be chosen.
        fn day_cells(composer: &crate::runtime::composer::Composer) -> (usize, usize) {
            let nodes = composer.arena_nodes();
            // `LayoutNode` stores children but not a parent, so invert it to walk up from a leaf.
            let mut parent = vec![usize::MAX; nodes.len()];
            for (idx, node) in nodes.iter().enumerate() {
                for &child in &node.children {
                    if child < parent.len() {
                        parent[child] = idx;
                    }
                }
            }
            let clickable = |idx: usize| {
                nodes[idx].modifier.elements().iter().any(|e| {
                    matches!(e, crate::modifier::ModifierElement::Clickable { .. })
                })
            };
            let day_label = |idx: usize| {
                nodes[idx].modifier.elements().iter().any(|e| match e {
                    crate::modifier::ModifierElement::TextContent { content, .. } => {
                        content.parse::<u32>().is_ok_and(|v| (1..=31).contains(&v))
                    }
                    _ => false,
                })
            };
            let (mut choosable, mut inert) = (0, 0);
            for idx in 0..nodes.len() {
                if !day_label(idx) {
                    continue;
                }
                let stack = parent[idx];
                let surface = if stack == usize::MAX { usize::MAX } else { parent[stack] };
                if surface == usize::MAX {
                    continue;
                }
                if clickable(surface) {
                    choosable += 1;
                } else {
                    inert += 1;
                }
            }
            (choosable, inert)
        }

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let mut state = DatePickerState::new(CalendarLocale::default());
            state.set_displayed_month_millis(month.start_utc_time_millis);
            DockedDatePicker::new(state).build(ctx);
        });
        let (docked_choosable, docked_inert) = day_cells(&composer);

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let mut state = DatePickerState::new(CalendarLocale::default());
            state.set_displayed_month_millis(month.start_utc_time_millis);
            DatePicker::new(state).build(ctx);
        });
        let (modal_choosable, modal_inert) = day_cells(&composer);

        assert!(
            docked_inert > 0,
            "the docked grid composes inert outside-month cells, found none"
        );
        assert_eq!(
            modal_inert, 0,
            "the modal grid composes an inert day cell, so it is drawing an outside-month cell"
        );
        assert_eq!(
            docked_choosable, modal_choosable,
            "both compose the same pages, so both draw the same number of choosable days"
        );
    }

    /// The 24x24 ink mask of a glyph drawn through the real `Icon` pipeline (node, then render, then pixels),
    /// the measurement `the_published_arrow_data_draws_the_same_arrow` makes for the dropdown arrow
    /// (`winia/src/ui/overlay.rs:1970`).
    fn render_glyph(data: &str) -> Vec<bool> {
        render_glyph_in(data, crate::layout::LayoutDirection::Ltr)
    }

    /// [`render_glyph`] under an ambient `direction`, so the RTL run exercises the real path — the theme's
    /// direction reaching the node's `layout_direction` and `draw_icon`'s mirror test — rather than a
    /// modifier bolted on for the test. `auto_mirror` is on, which is what the arrow tests are about.
    fn render_glyph_in(data: &str, direction: crate::layout::LayoutDirection) -> Vec<bool> {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), direction, ctx, |ctx| {
                Icon::svg_path(data)
                    .tint(Color::BLACK)
                    .size(24.0)
                    .auto_mirror(true)
                    .build(ctx);
            });
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

    #[test]
    fn the_navigation_arrows_flip_their_artwork_under_rtl() {
        // `Row` already carries the previous arrow to the RIGHT edge in RTL, so the artwork has to flip
        // with it or both arrows point inward — the failure `step_arrow` guards with `auto_mirror`.
        // Inked pixels are compared against the OTHER glyph rather than through an ink centroid: the
        // chevron's centroid sits at ~11.5 either way round, so a centroid cannot see the flip at all
        // (measured: 11.516 under LTR, 11.484 under RTL — a genuine mirror that a centroid test misses).
        let previous_ltr = render_glyph(CHEVRON_LEFT_PATH);
        let previous_rtl = render_glyph_in(CHEVRON_LEFT_PATH, crate::layout::LayoutDirection::Rtl);
        let next_ltr = render_glyph(CHEVRON_RIGHT_PATH);
        let ink_diff = |a: &[bool], b: &[bool]| a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();

        // Under RTL the previous arrow lands on the next chevron's pixels — 17 of 576 measured, the
        // same figure `the_month_arrows_draw_mirrored_chevrons` records for this pair of paths.
        assert!(
            ink_diff(&previous_rtl, &next_ltr) <= 24,
            "under RTL the previous chevron draws as the next one ({} pixels differ)",
            ink_diff(&previous_rtl, &next_ltr)
        );

        // And under LTR it does not, so the assertion above is the mirror doing work rather than the
        // two constants happening to rasterize alike (41 pixels measured).
        assert!(
            ink_diff(&previous_ltr, &next_ltr) > 24,
            "under LTR the previous chevron stays its own glyph (only {} pixels differ)",
            ink_diff(&previous_ltr, &next_ltr)
        );
    }

    /// The real guard on `step_arrow`: composing the picker has to mark its chevrons auto-mirrored. The
    /// pixel test above proves the pipeline flips an icon that ASKS to be flipped; this one proves the
    /// picker asks — drop the `auto_mirror(true)` from `step_arrow` and it goes red.
    #[test]
    fn the_picker_composes_its_chevrons_as_auto_mirrored() {
        use crate::modifier::ModifierElement;
        use crate::ui::icon::IconSource;

        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let state = DatePickerState::new(CalendarLocale::default());
            DockedDatePicker::new(state).build(ctx);
        });

        let chevrons: Vec<(String, bool)> = composer
            .arena_nodes()
            .iter()
            .flat_map(|node| node.modifier.elements().to_vec())
            .filter_map(|element| match element {
                ModifierElement::DrawIcon { spec } => match &spec.source {
                    IconSource::SvgPath { data, .. }
                        if data.as_ref() == CHEVRON_LEFT_PATH || data.as_ref() == CHEVRON_RIGHT_PATH =>
                    {
                        Some((data.to_string(), spec.auto_mirror))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();

        assert!(
            !chevrons.is_empty(),
            "the docked picker composes no chevron icons, so this test would pass vacuously"
        );
        for (data, auto_mirror) in &chevrons {
            assert!(
                *auto_mirror,
                "the chevron `{}` is not marked auto_mirror, so RTL leaves it pointing inward",
                &data[..data.len().min(24)]
            );
        }
    }

    /// Compose the modal picker with a runtime entered, which `LaunchedEffect` needs to spawn its
    /// task into. The task is never driven here — nothing in these assertions depends on the effect
    /// having run, and driving it would only race the assertions.
    fn compose_picker(state: &DatePickerState) -> crate::runtime::composer::Composer {
        compose_picker_with(|ctx| DatePicker::new(state.clone()).build(ctx))
    }

    /// Compose `content` inside a runtime, the way the app's own frame loop does.
    ///
    /// The entry field spawns a `LaunchedEffect` to ask for focus once it is showing, and spawning
    /// needs an entered runtime; a test that composes the picker directly has to provide one. The
    /// runtime is entered but never driven: what these helpers assert is what got composed, and
    /// letting the spawned task run would race that against the assertions.
    fn compose_picker_with(content: impl FnOnce(&mut ComposeCtx)) -> crate::runtime::composer::Composer {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(content);
        composer
    }

    /// The composed text the modal picker puts on screen, in the order it composed it.
    fn composed_texts(state: &DatePickerState) -> Vec<String> {
        composed_texts_with_title(state, None)
    }

    /// The texts a picker composes when its title is set explicitly, the way a caller's own title
    /// reaches the header. `None` leaves the default in place.
    fn composed_texts_with_title(state: &DatePickerState, title: Option<&str>) -> Vec<String> {
        let state = state.clone();
        let title = title.map(str::to_string);
        let composer = compose_picker_with(|ctx| {
            let mut picker = DatePicker::new(state);
            if let Some(title) = title {
                picker = picker.title(Some(title));
            }
            picker.build(ctx);
        });
        let mut out = Vec::new();
        for node in composer.arena_nodes() {
            for element in node.modifier.elements() {
                if let crate::modifier::ModifierElement::TextContent { content, .. } = element {
                    out.push(content.clone());
                }
            }
        }
        out
    }

    /// The icon paths the modal picker composes.
    fn composed_icon_paths(state: &DatePickerState) -> Vec<String> {
        let composer = compose_picker(state);
        let mut out = Vec::new();
        for node in composer.arena_nodes() {
            for element in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec, .. } = element {
                    if let crate::ui::icon::IconSource::SvgPath { data, .. } = &spec.source {
                        out.push(data.to_string());
                    }
                }
            }
        }
        out
    }

    /// `DisplayMode::Input` has to change what the picker composes. This is the guard on the
    /// original defect: `set_display_mode(DisplayMode::Input)` stored a value, and the whole crate
    /// had nothing that read it, so the picker stayed a calendar and the mode was a silent no-op.
    /// Both directions are asserted because either half alone would pass a picker that simply
    /// ignored the state and always drew the calendar.
    #[test]
    fn the_display_mode_decides_whether_the_picker_composes_a_calendar_or_a_field() {
        let state = picker();
        assert_eq!(state.display_mode(), DisplayMode::Picker);

        let calendar = composed_texts(&state);
        assert!(
            !calendar.is_empty(),
            "the picker composes no text at all, so this test would pass vacuously"
        );
        assert!(
            composed_icon_paths(&state).iter().any(|path| path == EDIT_PATH),
            "the calendar half composes no edit icon for the mode toggle, so the toggle is unreachable"
        );

        state.set_display_mode(DisplayMode::Input);
        let input = composed_texts(&state);
        assert!(
            input.iter().any(|text| text == DATE_INPUT_HEADLINE),
            "the input half has no {DATE_INPUT_HEADLINE:?} headline, got {input:?}"
        );
        assert!(
            input.iter().any(|text| text == DATE_INPUT_LABEL),
            "the input half has no {DATE_INPUT_LABEL:?} field label, got {input:?}"
        );
        assert!(
            composed_icon_paths(&state).iter().any(|path| path == DATE_RANGE_PATH),
            "the input half still offers the edit icon, so the toggle cannot go back"
        );

        state.set_display_mode(DisplayMode::Picker);
        assert_eq!(composed_texts(&state), calendar, "the mode did not go back to the calendar");
    }

    /// The headline names the mode's own wording when nothing is selected, and says the same thing
    /// either way once something is — material3 changes the word, not the fact
    /// (`DatePickerHeadline`, `DatePicker.kt:701-717`).
    #[test]
    fn the_headline_names_the_mode_when_nothing_is_selected() {
        let state = picker();
        assert!(composed_texts(&state).contains(&DatePickerDefaults::HEADLINE.to_string()));

        state.set_display_mode(DisplayMode::Input);
        assert!(composed_texts(&state).contains(&DATE_INPUT_HEADLINE.to_string()));

        let selected = CalendarDate::new(2024, 3, 1).expect("2024-03-01 is a date");
        state.set_selected_date_millis(Some(selected.start_of_day_millis()));
        let headline = CalendarModel::new(CalendarLocale::default())
            .format_date(selected.start_of_day_millis(), false);
        let texts = composed_texts(&state);
        assert!(
            texts.contains(&headline),
            "a selection should name the date in either mode, got {texts:?}"
        );
        assert!(
            !texts.contains(&DATE_INPUT_HEADLINE.to_string()),
            "the mode's own wording should be replaced once something is entered"
        );
    }

    /// The title names the mode as well as the headline does: a field that asks for a date is not
    /// headed "Select date" (`DatePicker.kt:654`). A title the caller supplied is left alone —
    /// only the default follows the mode.
    #[test]
    fn the_default_title_names_the_mode_and_a_supplied_one_does_not() {
        let state = picker();
        assert!(composed_texts(&state).contains(&DatePickerDefaults::TITLE.to_string()));
        assert!(!composed_texts(&state).contains(&DatePickerDefaults::INPUT_TITLE.to_string()));

        state.set_display_mode(DisplayMode::Input);
        let texts = composed_texts(&state);
        assert!(
            texts.contains(&DatePickerDefaults::INPUT_TITLE.to_string()),
            "the entry field should be headed by the input title, got {texts:?}"
        );
        assert!(
            !texts.contains(&DatePickerDefaults::TITLE.to_string()),
            "the calendar title should give way to the input title"
        );

        state.set_display_mode(DisplayMode::Picker);
        let texts = composed_texts(&state);
        assert!(texts.contains(&DatePickerDefaults::TITLE.to_string()));
        assert!(!texts.contains(&DatePickerDefaults::INPUT_TITLE.to_string()));

        // A caller's own title stands in both modes.
        let custom = "Pick a day off";
        let mut owned = state.clone();
        owned.set_display_mode(DisplayMode::Input);
        let texts = composed_texts_with_title(&owned, Some(custom));
        assert!(
            texts.contains(&custom.to_string()),
            "a supplied title should survive the mode switch, got {texts:?}"
        );
        assert!(!texts.contains(&DatePickerDefaults::INPUT_TITLE.to_string()));
    }

    /// The content descriptions the modal picker composes.
    fn composed_content_descriptions(state: &DatePickerState) -> Vec<String> {
        let composer = compose_picker(state);
        let mut out = Vec::new();
        for node in composer.arena_nodes() {
            for element in node.modifier.elements() {
                if let crate::modifier::ModifierElement::Semantics(config) = element {
                    if let Some(description) = config.content_description_value() {
                        out.push(description.to_string());
                    }
                }
            }
        }
        out
    }

    /// The field names its shape to whoever is not looking at it: the label carries the locale's
    /// pattern in its description, so a reader hears what to type before anything is typed
    /// (`DateInput.kt:93-98`).
    ///
    /// This reads the label's description rather than the placeholder because the placeholder is
    /// focus-gated — an unfocused field that has a label keeps the label sitting in the input slot
    /// instead (`text_field.rs:1094-1101`) — and a compose-only test has no focus to give it.
    #[test]
    fn the_input_field_names_the_locales_pattern_on_its_label() {
        let state = DatePickerState::new(locale_with_input_format("dd.MM.yyyy"));
        state.set_display_mode(DisplayMode::Input);
        let descriptions = composed_content_descriptions(&state);
        assert!(
            descriptions.iter().any(|d| d == "Date, DD.MM.YYYY"),
            "the field label does not name the locale's pattern, got {descriptions:?}"
        );
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
