//! Modal date picker demo — the M3 specs modal variant, judged by eye.
//!
//! Run: `cargo run -p winia --example date_picker_modal_demo`
//!
//! The page carries a button and a read-out; the dialog is up only while you have opened it. There is no
//! bare calendar on the page: `DatePicker` on its own is just the body, and this demo is about the
//! dialog that wraps it.
//!
//! What to look at:
//!
//! 1. **The container.** 360 dp wide, capped at `DatePickerModalTokens.ContainerHeight` (568), shaped
//!    28 dp and filled with the container colour. The picker is the surface: no padding of its own.
//! 2. **The header.** The "Select date" title over the headline, with the divider below them. The
//!    headline follows the selection and reads "No date selected" until there is one.
//! 3. **The body.** The month navigation row, the weekday row, the month grid. Tapping a day writes the
//!    selection immediately; the outside-month slots at either end of the grid stay empty, as material3
//!    composes them and as the M3 specs' modal anatomy shows.
//! 4. **The actions.** Cancel and OK under the content. Cancel restores the selection from when the dialog
//!    opened; OK keeps it and closes. Clicking the scrim or pressing Escape dismisses either way.
//! 5. **RTL.** The settings sheet switches the whole window: the header, the weekday row, the month
//!    arrows (whose artwork flips with them), the year menu button and the action row all mirror.

use letclone::clone;
use winia::composable;
use winia::runtime::composer::ComposeCtx;
use winia::prelude::*;
use winia::ui::date_picker::{remember_date_picker_state, CalendarLocale, DatePickerStateInit, DatePickerDialog};

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

#[composable]
fn modal_demo(ctx: &mut ComposeCtx) {
    // Closed until the page's button opens it.
    let dialog_open = ctx.remember(|| false);
    // The selection as it stood when the dialog opened, so Cancel can put it back.
    let baseline = ctx.remember(|| None::<i64>);
    let state = remember_date_picker_state(ctx, CalendarLocale::default(), DatePickerStateInit::default());

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0))
        .spacing(16.0)
        .build(ctx, {
            clone!(dialog_open, baseline, state);
            move |ctx| {
                Text::new(
                    "The dialog is the modal date picker. Dismiss it and re-open it here; the read-out \
                     below follows the selection the dialog holds.",
                )
                .font_size(12.0)
                .color(winia::modifier::Color::from_argb(255, 150, 150, 150))
                .build(ctx);

                Button::text()
                    .on_click({
                        clone!(dialog_open, baseline, state);
                        move || {
                            baseline.set(state.selected_date_millis());
                            dialog_open.set(true);
                        }
                    })
                    .modifier(Modifier::new().test_tag("dpm-open"))
                    .build(ctx, |ctx| {
                        Text::new("Open the modal date picker").build(ctx);
                    });

                let selected = state
                    .selected_date_millis()
                    .map(|millis| {
                        let model = state.calendar_model();
                        let date = model.canonical_date(millis);
                        format!(
                            "selected: {} · {:02}/{:02}/{:04}",
                            model.format_date(millis, false),
                            date.month,
                            date.day,
                            date.year
                        )
                    })
                    .unwrap_or_else(|| "selected: No date selected".to_string());
                Text::new(selected)
                    .font_size(12.0)
                    .color(winia::modifier::Color::from_argb(255, 150, 150, 150))
                    .build(ctx);

                // Composed UNCONDITIONALLY, with `visible` carrying the state — as
                // `fixture_date_picker_dialog` does. Guarding this with `if dialog_open.get()` looks
                // equivalent and is not: an overlay is released by composing its owner once with
                // `visible == false`, and the guard skips that composition entirely. Measured: with the
                // guard the dialog opens, Cancel's handler runs and flips the state to false, and the
                // overlay stays on screen — nothing ever tells it to go.
                DatePickerDialog::new(state.clone(), dialog_open.get())
                    .on_dismiss_request({
                        clone!(dialog_open);
                        move || dialog_open.set(false)
                    })
                    .modifier(Modifier::new().test_tag("dpm-dialog"))
                    .dismiss_button({
                        clone!(dialog_open, baseline, state);
                        move |ctx: &mut ComposeCtx| {
                            Button::text()
                                .on_click({
                                    clone!(dialog_open, baseline, state);
                                    move || {
                                        state.set_selected_date_millis(baseline.get());
                                        dialog_open.set(false);
                                    }
                                })
                                .modifier(Modifier::new().test_tag("dpm-cancel"))
                                .build(ctx, |ctx| {
                                    Text::new("Cancel").build(ctx);
                                });
                        }
                    })
                    .confirm_button({
                        clone!(dialog_open);
                        move |ctx: &mut ComposeCtx| {
                            Button::text()
                                .on_click({
                                    clone!(dialog_open);
                                    move || dialog_open.set(false)
                                })
                                .modifier(Modifier::new().test_tag("dpm-ok"))
                                .build(ctx, |ctx| {
                                    Text::new("OK").build(ctx);
                                });
                        }
                    })
                    .build(ctx);
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 760.0)
                .title("ModalDatePicker Demo")
                .build(ctx, |ctx| {
                    settings::shell("ModalDatePicker Demo", ctx, modal_demo);
                });
        });
    });
}
