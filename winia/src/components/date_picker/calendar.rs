use super::*;

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
pub(super) fn today_at(now_millis: i64, offset_millis: i64) -> i64 {
    canonical_millis(now_millis.saturating_add(offset_millis))
}

/// The system clock, in milliseconds since the epoch — the one place it is read.
///
/// Split out so [`CalendarModel::today_millis`] reads as the composition it is (an instant plus a zone)
/// rather than as arithmetic buried in a clock call, and so the tests can see what has to meet.
pub(super) fn now_millis() -> i64 {
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
pub(super) fn local_utc_offset_millis() -> i64 {
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
    /// [`crate::components::date_picker`], exactly [`DateInputFormat::pattern_length`] of them; anything
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
pub(super) fn format_string(template: &str, args: &[&str]) -> String {
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
