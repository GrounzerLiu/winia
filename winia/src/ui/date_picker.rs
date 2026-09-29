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
}
