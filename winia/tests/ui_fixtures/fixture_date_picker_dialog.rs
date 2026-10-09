//! UI-test fixture: the modal date picker — a `DatePickerDialog` over a fixed calendar.
//!
//! Drives `date_picker_dialog_*` in `ui_test.rs`. What can only be measured here is the dialog as the renderer
//! lays it out: the 360 dp container capped at `DatePickerModalTokens.ContainerHeight` (568), a day tap moving
//! the state that the page read-out shows, and the two actions closing and re-opening it.

use letclone::clone;
use winia::prelude::*;
use winia::components::date_picker::{
    CalendarDate, CalendarLocale, DatePickerDialog, DatePickerState, DatePickerStateInit,
};

/// The start of a fixed UTC day, so nothing in the fixture depends on the clock.
fn millis(year: i32, month: u32, day: u32) -> i64 {
    CalendarDate::new(year, month, day)
        .expect("a real date")
        .start_of_day_millis()
}

#[composable]
fn date_picker_dialog_fixture(ctx: &mut ComposeCtx) {
    // The same fixed calendar the docked fixture uses: 2024-09-01 is a Sunday, so the 5th (today) is in the
    // first row's fifth column and the 10th (the selection) in the second row's third.
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
    let open = ctx.remember(|| true);
    let model = state.calendar_model().clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Button::text()
                .on_click({
                    clone!(open);
                    move || open.set(true)
                })
                .modifier(Modifier::new().test_tag("dpd-open"))
                .build(ctx, |ctx| {
                    Text::new("Open the modal picker").build(ctx);
                });
            Text::new(format!(
                "open: {}",
                if open.get() { "yes" } else { "no" }
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
                .modifier(Modifier::new().test_tag("dpd-dialog"))
                .dismiss_button({
                    clone!(open);
                    move |ctx: &mut ComposeCtx| {
                        Button::text()
                            .on_click({
                                clone!(open);
                                move || open.set(false)
                            })
                            .modifier(Modifier::new().test_tag("dpd-cancel"))
                            .build(ctx, |ctx| {
                                Text::new("Cancel").build(ctx);
                            });
                    }
                })
                .confirm_button(|ctx: &mut ComposeCtx| {
                    Button::text()
                        .modifier(Modifier::new().test_tag("dpd-ok"))
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
                .title("Date Picker Dialog Fixture")
                .build(ctx, |ctx| date_picker_dialog_fixture(ctx));
        });
    });
}
