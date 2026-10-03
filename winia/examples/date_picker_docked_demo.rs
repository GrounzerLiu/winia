//! DockedDatePicker demo — the M3 specs docked variant, judged by eye.
//!
//! Run: `cargo run -p winia --example date_picker_docked_demo`
//!
//! What to look at, in order:
//!
//! 1. **The anchor field.** A form-like outlined `TextField` with a circular calendar button at its end.
//!    Clicking the button pops the picker open just below the field; clicking outside closes it.
//! 2. **The picker container.** A navigation row with a month group (`< September >`) and a year group
//!    (`< 2025 >`), the weekday row, the month grid, and Cancel/OK. The input field is the anchor above, not
//!    part of the picker.
//! 3. **Inline lists.** The month/year buttons swap the grid for a month/year list inside the picker;
//!    picking one keeps the other half and closes the list.
//! 4. **Cancel vs OK.** Tapping days edits live; OK writes the anchor field and closes, Cancel restores the
//!    selection from when the popup opened and closes.
use letclone::clone;
use winia::composable;
use winia::runtime::composer::{ComposeCtx, GroupStatus};
use winia::layout::BoxLayout;
use winia::prelude::*;
use winia::ui::date_picker::{
    remember_date_picker_state, CalendarLocale, DatePickerStateInit, DockedDatePicker, CALENDAR_MONTH_PATH,
};
use winia::ui::icon::Icon;
use winia::ui::overlay::{OverlayAnimSpec, Popup, PopupPosition};

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// The tag the pager's anchor readout should carry, or `None` for a normal run.
///
/// `first_visible_index` is written back inside the list's measure every frame and is not part of what
/// gets drawn, so a probe outside the process cannot read it — the tree reports positions, which is the
/// pixel offset the anchor is derived from rather than the anchor. The month arrows build their target
/// as `anchor ± 1`, so anything that goes wrong asymmetrically between the two directions lives in that
/// number, and this switch is the only way to see it. Set `WINIA_DP_PAGE_PROBE=dp-page-probe`.
fn page_probe_tag() -> Option<String> {
    std::env::var("WINIA_DP_PAGE_PROBE").ok().filter(|v| !v.is_empty())
}

#[composable]
fn docked_demo(ctx: &mut ComposeCtx) {
    let open = ctx.remember(|| false);
    let confirmed = ctx.remember(|| None::<i64>);
    let baseline = ctx.remember(|| None::<i64>);
    let field_value = ctx.remember(|| TextFieldValue::new(""));
    let picker_state = remember_date_picker_state(ctx, CalendarLocale::default(), DatePickerStateInit::default());

    Column::new()
        .spacing(16.0)
        .modifier(Modifier::new().padding(24.0))
        .build(ctx, {
            clone!(open, confirmed, baseline, field_value, picker_state);
            move |ctx| {
                let state = picker_state.clone();
                let model = state.calendar_model().clone();

                Text::new("Docked date picker (click the calendar button)")
                    .font_size(12.0)
                    .color(winia::modifier::Color::from_argb(255, 150, 150, 150))
                    .build(ctx);

                // ── Anchor: a form field with a calendar button, wrapped in an explicit group ──
                // The popup anchors to the GROUP's container node (`anchor_key`): anchoring to the
                // field's own slot would land on whatever the field composed last (here its 24x24
                // trailing icon). Same pattern as `ExposedDropdownMenuBox`'s anchor container.
                let anchor_key = ctx.next_key();
                match ctx.start_restartable_group(anchor_key, Modifier::new(), BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        TextField::new(field_value.clone())
                            .outlined()
                            .read_only(true)
                            .modifier(Modifier::new().width(280.0))
                            .label(|ctx| {
                                Text::new("Date").build(ctx);
                            })
                            .supporting_text("MM/DD/YYYY")
                            .trailing_icon({
                                clone!(open, baseline, state);
                                move |ctx| {
                                    let open_toggle = open.clone();
                                    let baseline_save = baseline.clone();
                                    let state_save = state.clone();
                                    // The trailing slot measures its child at fixed 24x24 (`TextFieldLayout`),
                                    // so this stays a plain 24 dp Icon, not an IconButton (min container
                                    // 32): material3's date-field trailing icon has no resting container —
                                    // the circle in the specs figure is the focused state layer.
                                    Icon::svg_path(CALENDAR_MONTH_PATH)
                                        .tint(WiniaTheme::colors().on_surface_variant)
                                        .modifier(
                                            Modifier::new()
                                                .clickable(move || {
                                                    baseline_save.set(state_save.selected_date_millis());
                                                    let v = open_toggle.get();
                                                    open_toggle.set(!v);
                                                })
                                                .test_tag("dp-docked-open"),
                                        )
                                        .build(ctx);
                                }
                            })
                            .build(ctx);
                    }
                }
                ctx.end_restartable_group();

                // ── The picker, just below the field ──
                {
                    clone!(open, confirmed, baseline, field_value, state);
                    Popup::new(open.clone().get())
                        .anchor_slot(Some(anchor_key))
                        .position(PopupPosition::BottomLeft)
                        .offset(0.0, 8.0)
                        // Enter: docked-dropdown slide + fade (the `slide_down` preset mirrors
                        // `slideIn(-height/2) + fadeIn`); exit is the same path reversed (slide back
                        // up + fade out, mirrored easing — the `default_exit` convention).
                        .enter_animation(Some(OverlayAnimSpec::slide_down(
                            std::time::Duration::from_millis(200),
                        )))
                        .exit_animation(Some(
                            OverlayAnimSpec::slide_down(std::time::Duration::from_millis(200))
                                .with_interpolator(
                                    winia::animation::interpolator::EaseInCubic::new(),
                                ),
                        ))
                        .on_dismiss_request({
                            // The scrim and Escape both land here, and they have to DISCARD just like the
                            // Cancel button does. Leaving the selection as the user left it meant the next
                            // open started from an unconfirmed pick, and a later Ok would then confirm a
                            // date the user never agreed to in this session.
                            //
                            // Measured: with this reduced to `open.set(false)`, picking a day and pressing
                            // Escape still leaves the date in the state, so reopening captures it as the
                            // new baseline and Ok writes it into the field
                            // (`tmp/probe_docked_discard.py`, one assertion red).
                            clone!(open, baseline, state);
                            move || {
                                state.set_selected_date_millis(baseline.get());
                                open.set(false);
                            }
                        })
                        .build(ctx, {
                            clone!(open, confirmed, baseline, field_value, state, model);
                            move |ctx| {
                                let mut picker = DockedDatePicker::new(state.clone());
                                // A probe switch for the pager's derived anchor — see
                                // `page_probe_tag`. Off unless the env var is set, so the demo the
                                // user looks at is unchanged.
                                if let Some(tag) = page_probe_tag() {
                                    picker = picker.page_probe(tag);
                                }
                                picker
                                    .on_confirm({
                                        clone!(open, confirmed, field_value, state, model);
                                        move || {
                                            let selected = state.selected_date_millis();
                                            confirmed.set(selected);
                                            // The specs anchor field shows MM/DD/YYYY, not the long date.
                                            let text = selected
                                                .map(|millis| {
                                                    let d = model.canonical_date(millis);
                                                    format!(
                                                        "{:02}/{:02}/{:04}",
                                                        d.month, d.day, d.year
                                                    )
                                                })
                                                .unwrap_or_default();
                                            field_value.set(TextFieldValue::new(text));
                                            open.set(false);
                                        }
                                    })
                                    .on_dismiss({
                                        clone!(open, baseline, state);
                                        move || {
                                            state.set_selected_date_millis(baseline.get());
                                            open.set(false);
                                        }
                                    })
                                    .build(ctx);
                            }
                        });
                }

                let status = confirmed
                    .get()
                    .map(|millis| model.format_date(millis, false))
                    .unwrap_or_else(|| String::from("(none)"));
                Text::new(format!("Confirmed: {status}"))
                    .font_size(12.0)
                    .color(winia::modifier::Color::from_argb(255, 150, 150, 150))
                    .build(ctx);
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 800.0)
                .title("DockedDatePicker Demo")
                .build(ctx, |ctx| {
                    settings::shell("DockedDatePicker Demo", ctx, docked_demo);
                });
        });
    });
}
