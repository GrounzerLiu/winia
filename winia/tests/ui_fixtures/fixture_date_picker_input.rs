//! UI-test fixture: the modal date picker opened on its text-entry mode.
//!
//! Drives `date_picker_input_*` in `ui_test.rs`. What can only be measured here is the field as the
//! user meets it: the header toggle switching the picker between a calendar and a field, digits
//! typed into that field reaching the selection, and an entry the validator refuses saying so under
//! the field rather than becoming the selection.

use letclone::clone;
use winia::prelude::*;
use winia::ui::date_picker::{
    CalendarDate, CalendarLocale, DatePickerDialog, DatePickerState, DatePickerStateInit, DisplayMode,
};

/// The start of a fixed UTC day, so nothing in the fixture depends on the clock.
fn millis(year: i32, month: u32, day: u32) -> i64 {
    CalendarDate::new(year, month, day)
        .expect("a real date")
        .start_of_day_millis()
}

#[composable]
fn date_picker_input_fixture(ctx: &mut ComposeCtx) {
    // The same fixed calendar the other date-picker fixtures use, opened on the text field rather
    // than on the calendar.
    let state = ctx
        .remember(|| {
            DatePickerState::with(
                CalendarLocale::default(),
                DatePickerStateInit {
                    initial_selected_date_millis: Some(millis(2024, 9, 10)),
                    initial_displayed_month_millis: Some(millis(2024, 9, 1)),
                    initial_display_mode: DisplayMode::Input,
                    today_millis: Some(millis(2024, 9, 5)),
                    ..Default::default()
                },
            )
        })
        .get();
    let open = ctx.remember(|| true);
    let model = state.calendar_model().clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new(format!(
                "mode: {}",
                match state.display_mode() {
                    DisplayMode::Picker => "picker",
                    DisplayMode::Input => "input",
                }
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

            DatePickerDialog::new(state.clone(), open.get())
                .on_dismiss_request({
                    clone!(open);
                    move || open.set(false)
                })
                .modifier(Modifier::new().test_tag("dpi-dialog"))
                .confirm_button(|ctx: &mut ComposeCtx| {
                    Button::text()
                        .modifier(Modifier::new().test_tag("dpi-ok"))
                        .build(ctx, |ctx| {
                            Text::new("OK").build(ctx);
                        });
                })
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
                .title("Date Picker Input Fixture")
                .build(ctx, |ctx| date_picker_input_fixture(ctx));
        });
    });
}