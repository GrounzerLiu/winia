use super::*;
use super::calendar::format_string;
use super::input::{date_input_content, display_mode_toggle};

/// The `test_tag` on the menu button shared by `DatePicker` and the docked picker, so a UI test can tap
/// the control that opens the year/month panel. Docked composes two buttons: pass a distinct tag per group.
const YEAR_MENU_TAG: &str = "dp-year-menu";

/// The docked month menu button's tag (`dp-month-menu`), distinct from the year groups'.
const MONTH_MENU_TAG: &str = "dp-month-menu";

/// The prefix of a year cell's `test_tag` — `dp-year-2024` — so a UI test can find one year's box.
const YEAR_CELL_TAG_PREFIX: &str = "dp-year-";

/// The docked picker's calendar panel (`dp-calendar-panel`). The year and month panels are findable
/// through their cells; this is the calendar side's handle, which is what lets a UI test assert that
/// BOTH panels are composed while the docked crossfade runs — the one thing a serial fade can never
/// do, and the assertion that would catch it coming back.
const CALENDAR_PANEL_TAG: &str = "dp-calendar-panel";

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

mod dialog;
mod docked;
mod picker;

pub(crate) use dialog::*;
pub(crate) use picker::*;

pub use dialog::DatePickerDialog;
pub use docked::{
    CALENDAR_MONTH_PATH, CHECK_PATH, DOCKED_CONTAINER_CORNER, DockedDatePicker,
};
pub use picker::DatePicker;
