use super::*;
use super::calendar::format_string;

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

