//! UI-test fixture: the docked date picker, with a fixed selection, month and "today" so that the geometry
//! guards read the same numbers on any day the suite runs.
//!
//! Drives `date_picker_*` in `ui_test.rs`. The unit tests pin the calendar model, the coercion rules and the
//! grid; what can only be measured here is the picker as the renderer lays it out — the container's 360 dp
//! minimum and the sum of its rows, the 40 dp day circle with its 1 dp ring around today, the month the arrows
//! step to, and the day a tap selects.

use winia::prelude::*;
use winia::ui::date_picker::{CalendarDate, CalendarLocale, DatePicker, DatePickerState, DatePickerStateInit};

/// The start of a fixed UTC day, so nothing in the fixture depends on the clock.
fn millis(year: i32, month: u32, day: u32) -> i64 {
    CalendarDate::new(year, month, day)
        .expect("a real date")
        .start_of_day_millis()
}

#[composable]
fn date_picker_fixture(ctx: &mut ComposeCtx) {
    // 2024-09-01 is a Sunday, so a Sunday-first locale puts the 5th (today) in the first row's fifth column and
    // the selection, the 10th, in the second row's third column.
    let state = ctx
        .remember(|| {
            DatePickerState::with(
                CalendarLocale::default(),
                DatePickerStateInit {
                    initial_selected_date_millis: Some(millis(2024, 9, 10)),
                    initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                    today_millis: Some(millis(2024, 9, 5)),
                    ..Default::default()
                },
            )
        })
        .get();
    let model = state.calendar_model().clone();

    // The picker comes first and the read-out texts after it, so the month grid sits high in the window.
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(8.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            DatePicker::new(state.clone())
                .modifier(Modifier::new().test_tag("dp-picker"))
                .build(ctx);
            Text::new(format!(
                "month: {}",
                model.format_month_year(state.displayed_month_millis())
            ))
            .build(ctx);
            Text::new(format!(
                "selected: {}",
                state
                    .selected_date_millis()
                    .map(|millis| model.format_date(millis, false))
                    .unwrap_or_else(|| "none".to_string())
            ))
            .build(ctx);
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 700.0)
                .title("Date Picker Fixture")
                .build(ctx, |ctx| date_picker_fixture(ctx));
        });
    });
}
