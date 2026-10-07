//! UI-test fixture: the docked date picker, with a fixed selection, month and "today" so the panel
//! switch reads the same year list on any day the suite runs.
//!
//! Drives `date_picker_panel_switch_cross_fades_both_panels` in `ui_test.rs`. What only this can
//! measure is the docked picker's PANEL SWITCH: the year panel and the calendar panel are two
//! generations of the docked `Crossfade`, so during the switch both must be composed at once. The
//! serial fade this replaced could only ever show one at a time, so the assertion is what keeps it
//! from coming back — and it is also the only coverage the panel's 300 ms default has at all.

use winia::prelude::*;
use winia::components::date_picker::{
    CalendarDate, CalendarLocale, DatePickerState, DatePickerStateInit, DockedDatePicker,
};

/// The start of a fixed UTC day, so nothing in the fixture depends on the clock.
fn millis(year: i32, month: u32, day: u32) -> i64 {
    CalendarDate::new(year, month, day)
        .expect("a real date")
        .start_of_day_millis()
}

#[composable]
fn docked_date_picker_fixture(ctx: &mut ComposeCtx) {
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

    // No overlay: the panel switch lives inside the picker, so the fixture does not need the
    // anchor field and popup the demo wraps it in.
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(8.0))
        .build(ctx, |ctx| {
            DockedDatePicker::new(state.clone()).build(ctx);
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario
/// from `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(700.0, 760.0)
                .title("Docked Date Picker Fixture")
                .build(ctx, |ctx| docked_date_picker_fixture(ctx));
        });
    });
}
