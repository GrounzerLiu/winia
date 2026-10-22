use super::*;

// ── Docked date picker ──

/// Material Icons `calendar_month` glyph for the docked picker's input affordance. Like the chevrons above,
/// this sandbox cannot byte-verify the artwork against Google's assets, so it is an inline path.
pub const CALENDAR_MONTH_PATH: &str = "M19 4h-1V2h-2v2H8V2H6v2H5c-1.11 0-1.99.9-1.99 2L3 20c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V10h14v10zM9 14H7v-2h2v2zm4 0h-2v-2h2v2zm4 0h-2v-2h2v2zm-8 4H7v-2h2v2zm4 0h-2v-2h2v2zm4 0h-2v-2h2v2z";

/// The docked container's corner radius, read off the M3 specs measurements diagram. Provisional: docked has
/// no token file (unlike the modal's 28 dp `ContainerShape`), so this stays a plain constant until the token
/// value is confirmed.
pub const DOCKED_CONTAINER_CORNER: f32 = 16.0;

/// The inline panel the docked picker shows in place of the weekday row and the month grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockedPanel {
    Calendar,
    Months,
    Years,
}

/// The docked date picker: the M3 specs variant that opens from an onscreen input.
///
/// Unlike [`DatePicker`] (the modal calendar with its "Select date" title and headline), the docked picker has
/// no header. Its container holds, top to bottom: a navigation row with a month group and a year group (each
/// arrows plus a button whose list opens inline, replacing the grid), the weekday row, the month grid, and a
/// Cancel/OK action row.
///
/// The input field is NOT part of this component: in material3 it belongs to the caller that opens the picker
/// (`DatePickerDialog`'s dock mode keeps its own field, and the M3 specs docked figure draws one above the
/// picker), so winia leaves it out and the caller pairs this with a `TextField` of its own.
///
/// Day taps write the selection into `state` immediately; Cancel/OK only notify. A caller that needs
/// discard-on-cancel snapshots `selected_date_millis` before opening and restores it in `on_dismiss`.
pub struct DockedDatePicker {
    pub(crate) state: DatePickerState,
    pub(crate) on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) modifier: Modifier,
    /// A tag for a readout node reporting the pager's derived anchor, or `None`.
    ///
    /// Diagnostic only, and behind a caller-supplied tag rather than always on: the anchor
    /// (`LazyListState::first_visible`) is written back inside the measure every frame and is not part
    /// of what gets drawn, so a probe outside the process cannot read it — the tree shows positions,
    /// which is the pixel offset the anchor is derived FROM. The month arrows compute their target as
    /// `anchor ± 1`, so anything that goes wrong asymmetrically between the two directions lives here,
    /// and this is the only way to see it.
    pub(crate) page_probe: Option<String>,
}

impl DockedDatePicker {
    /// A docked picker over `state`.
    pub fn new(state: DatePickerState) -> Self {
        Self {
            state,
            on_confirm: None,
            on_dismiss: None,
            modifier: Modifier::new(),
            page_probe: None,
        }
    }

    /// The affirming action, after the dismiss button in the row. The picker wires no event of its own into it.
    pub fn on_confirm(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_confirm = Some(Arc::new(cb));
        self
    }

    /// The dismissing action, before the confirm button in the row.
    pub fn on_dismiss(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// A modifier for the container (`modifier`).
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Compose a readout of the pager's derived anchor under `tag`, for a probe outside the process.
    ///
    /// Diagnostic only — see [`DockedDatePicker::page_probe`]. The node carries `page:N`, where `N` is
    /// `LazyListState::first_visible` for the month list on the frame it was composed, and is marked
    /// `test_tag(tag)` so a debug-server probe can find it.
    pub fn page_probe(mut self, tag: impl Into<String>) -> Self {
        self.page_probe = Some(tag.into());
        self
    }

    /// Composes the picker.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let colors = DatePickerColors::from_theme(&WiniaTheme::colors());
        let model = self.state.calendar_model().clone();
        let month = model.month_of_millis(self.state.displayed_month_millis());
        let state = self.state.clone();
        // Which inline panel replaces the weekday row and the grid, if any.
        let panel = ctx.remember(|| DockedPanel::Calendar);
        // The year list's scroll state, parked here so reopening the panel keeps its position.
        let year_rows = ctx.remember(LazyListState::new).get();
        // The paged month list, its position being the displayed month (see `sync_month_pages`), which
        // seeds it with `scroll_to_item` — see the note in `DatePicker::build`.
        let month_rows = ctx.remember(LazyListState::new).get();
        // The page the arrows have asked for but not reached — see the note in `DatePicker::build`.
        let month_step_in_flight = ctx.remember(|| None::<usize>);
        // Page 0 is the January of the range's first year — see the note in `DatePicker::build`.
        let first_month = model.month_of(*state.year_range().start(), 1).start_utc_time_millis;
        let on_confirm = self.on_confirm.clone();
        let on_dismiss = self.on_dismiss.clone();

        let container = self
            .modifier
            .then(Modifier::new().width(DatePickerDefaults::CONTAINER_WIDTH))
            .background(colors.container, Shape::rounded(DOCKED_CONTAINER_CORNER))
            .clip(Shape::rounded(DOCKED_CONTAINER_CORNER));

        Column::new()
            .modifier(container)
            .arrangement(Arrangement::Start)
            .build(ctx, |ctx| {
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding_horizontal(DatePickerDefaults::HORIZONTAL_PADDING),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        let current = panel.get();
                        // Only while the calendar is the panel on show: an inline month or year list
                        // covers the pages, and the year panel is exactly what moves the month.
                        if current == DockedPanel::Calendar {
                            sync_month_pages(ctx, &state, &model, &month_rows, first_month, &month_step_in_flight);
                        }
                        // Diagnostic readout of the anchor the arrows compute their target from, plus
                        // the pixel offset it is derived from. It sits before the navigation row so its
                        // text is composed on every frame — `first_visible()` only moves when the
                        // measure writes it back, and a probe reading the tree needs the value of the
                        // frame it just asked about. The offset comes along because the anchor alone
                        // cannot tell "the spring has not arrived" from "it arrived and the anchor did
                        // not move".
                        if let Some(tag) = self.page_probe.as_deref() {
                            Text::new(format!(
                                "page:{} off:{:.0}",
                                month_rows.first_visible(),
                                month_rows.offset()
                            ))
                            .font_size(8.0)
                            .modifier(Modifier::new().test_tag(tag))
                            .build(ctx);
                        }
                        docked_navigation(ctx, &state, &month, current, &panel, &colors, &year_rows, &month_rows, &month_step_in_flight);
                        // The inline lists crossfade in and out of the calendar's place (material3
                        // swaps its year overlay with expand + fade; a full-bleed fade reads the same
                        // here and never moves the action row).
                        let cross_state = state.clone();
                        let cross_model = model.clone();
                        let cross_month = month;
                        let cross_colors = colors.clone();
                        let cross_rows = year_rows.clone();
                        let cross_month_rows = month_rows.clone();
                        let cross_panel = panel.clone();
                        crate::components::Crossfade::new(panel.clone())
                            // The default: Compose's `tween()` — one 300 ms cross-fade. This used to
                            // ask for 150 ms because `Crossfade` faded the old panel out and the new
                            // one in in sequence, so 300 ms played twice (600 ms of panel switch);
                            // both generations now overlap in that single tween.
                            .build(
                            ctx,
                            move |ctx, shown| match shown {
                                DockedPanel::Calendar => {
                                    // A Column, not bare siblings: `Crossfade` wraps its content in a
                                    // Box (stacked, overlapping children), so without this the grid
                                    // starts at the same y as the weekday row (measured overlap).
                                    Column::new()
                                        .modifier(Modifier::new().fill_max_width().test_tag(CALENDAR_PANEL_TAG))
                                        .arrangement(Arrangement::Start)
                                        .build(ctx, |ctx| {
                                            weekday_row(ctx, &cross_model, &cross_colors);
                                            // The docked picker draws the neighbouring month's days — the
                                            // M3 specs' docked anatomy lists them as a grid state, and
                                            // the modal one does not.
                                            month_pages(
                                                ctx,
                                                &cross_state,
                                                &cross_model,
                                                &cross_colors,
                                                &cross_month_rows,
                                                true,
                                            );
                                        });
                                }
                                DockedPanel::Months => {
                                    month_list(
                                        ctx,
                                        &cross_state,
                                        &cross_model,
                                        &cross_month,
                                        &cross_panel,
                                        &cross_colors,
                                    );
                                }
                                DockedPanel::Years => {
                                    let panel_for_close = cross_panel.clone();
                                    let state_for_close = cross_state.clone();
                                    year_panel(
                                        ctx,
                                        &cross_state,
                                        &cross_model,
                                        &cross_colors,
                                        &cross_rows,
                                        move |year| {
                                            state_for_close.set_displayed_year(year);
                                            panel_for_close.set(DockedPanel::Calendar);
                                        },
                                    );
                                }
                            },
                        );
                        docked_action_row(ctx, on_confirm, on_dismiss);
                    });
            });
    }
}

/// The docked navigation row: a month group and a year group, each arrows around a button whose list opens
/// inline. While a group's panel is open its arrows are hidden, leaving the button to close it.
pub(crate) fn docked_navigation(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    month: &CalendarMonth,
    current: DockedPanel,
    panel: &State<DockedPanel>,
    colors: &DatePickerColors,
    year_rows: &LazyListState,
    month_rows: &LazyListState,
    month_step_in_flight: &State<Option<usize>>,
) {
    let navigation_color = colors.navigation_content;
    let months_open = current == DockedPanel::Months;
    let years_open = current == DockedPanel::Years;
    // While either list is open both groups' step arrows fade to alpha 0 (still composed and disabled),
    // so neither button moves; the idle group degrades to plain dimmed text (the M3 specs docked figure
    // shows the year side exactly so: "2025" with neither pill nor dropdown arrow, both chevron pairs gone).
    let panel_open = months_open || years_open;

    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_YEAR_HEIGHT),
        )
        .arrangement(Arrangement::SpaceBetween)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            // Month group: step arrows around the month name; the button swaps in the month list. The
            // arrows page the list, like the modal picker's — they never write the month themselves.
            Row::new()
                .arrangement(Arrangement::Start)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    // Derived flags, so the arrows rebuild when the list reaches an end and not on
                    // every scroll pixel.
                    let back_enabled = month_rows.remember_can_scroll_backward(ctx);
                    let forward_enabled = month_rows.remember_can_scroll_forward(ctx);
                    // The last page the list can show, so a forward step cannot ask past the end.
                    let last_page = CalendarModel::number_of_months_in_range(&state.year_range())
                        .saturating_sub(1) as usize;
                    let back = month_rows.clone();
                    let back_flight = month_step_in_flight.clone();
                    let back_anchor = month_rows.first_visible();
                    step_arrow(ctx, move || {
                        let target = arrow_target(back_anchor, back_flight.get(), false, last_page);
                        back_flight.set(Some(target));
                        back.animate_scroll_to_item(target, 0.0);
                    }, back_enabled.get(), CHEVRON_LEFT_PATH, navigation_color, IconButtonSize::XSmall, !panel_open);
                    let panel_toggle = panel.clone();
                    let is_open = months_open;
                    // Abbreviated month ("Sep", not "September") — the M3 specs docked figure.
                    let label = state.calendar_model().locale().month_names_short[month.month as usize - 1].clone();
                    year_menu_button(
                        ctx,
                        label,
                        move || {
                            panel_toggle.set(if is_open {
                                DockedPanel::Calendar
                            } else {
                                DockedPanel::Months
                            });
                        },
                        colors,
                        !years_open,
                        MONTH_MENU_TAG,
                    );
                    let forward = month_rows.clone();
                    let forward_flight = month_step_in_flight.clone();
                    let forward_anchor = month_rows.first_visible();
                    step_arrow(ctx, move || {
                        let target = arrow_target(forward_anchor, forward_flight.get(), true, last_page);
                        forward_flight.set(Some(target));
                        forward.animate_scroll_to_item(target, 0.0);
                    }, forward_enabled.get(), CHEVRON_RIGHT_PATH, navigation_color, IconButtonSize::XSmall, !panel_open);
                });
            // Year group: step arrows around the year; the button swaps in the year list.
            Row::new()
                .arrangement(Arrangement::Start)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    let displayed_year = month.year;
                    let year_range = state.year_range();
                    let state_for_step = state.clone();
                    step_arrow(
                        ctx,
                        move || state_for_step.set_displayed_year(displayed_year - 1),
                        year_range.contains(&(displayed_year - 1)),
                        CHEVRON_LEFT_PATH,
                        navigation_color,
                        IconButtonSize::XSmall,
                        !panel_open,
                    );
                    let panel_toggle = panel.clone();
                    let is_open = years_open;
                    let rows_for_scroll = year_rows.clone();
                    let state_for_scroll = state.clone();
                    year_menu_button(
                        ctx,
                        displayed_year.to_string(),
                        move || {
                            if is_open {
                                panel_toggle.set(DockedPanel::Calendar);
                            } else {
                                // Park the list one row above the displayed year before it opens
                                // (mirrors `DatePicker`'s toggle).
                                rows_for_scroll.scroll_to_item(
                                    year_panel_first_row(
                                        &state_for_scroll,
                                        state_for_scroll.calendar_model(),
                                    ),
                                    0.0,
                                );
                                panel_toggle.set(DockedPanel::Years);
                            }
                        },
                        colors,
                        !months_open,
                        YEAR_MENU_TAG,
                    );
                    let state_for_step = state.clone();
                    step_arrow(
                        ctx,
                        move || state_for_step.set_displayed_year(displayed_year + 1),
                        year_range.contains(&(displayed_year + 1)),
                        CHEVRON_RIGHT_PATH,
                        navigation_color,
                        IconButtonSize::XSmall,
                        !panel_open,
                    );
                });
        });
}

/// The checkmark that flags the displayed month in the month list.
pub const CHECK_PATH: &str = "M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";

/// The month list: the twelve months in four rows of three pill cells, exactly as tall as the calendar it
/// replaces. Three columns do fit: the month pills are 104 dp wide (the year cells' 72 dp plus room for the
/// longest English name), not 72 dp.
pub(crate) fn month_list(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    month: &CalendarMonth,
    panel: &State<DockedPanel>,
    colors: &DatePickerColors,
) {
    let names = model.locale().month_names.clone();
    let displayed_year = month.year;
    let displayed_month = month.month;
    let year_enabled = state.selectable_dates().is_selectable_year(displayed_year);
    // The item closure outlives this call, so it only captures owned clones (mirrors `year_panel`).
    let cell_state = state.clone();
    let cell_model = model.clone();
    let cell_panel = panel.clone();
    let cell_colors = colors.clone();
    let cell_names = names.clone();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::YEAR_PANEL_HEIGHT + DatePickerDefaults::DIVIDER_THICKNESS),
        )
        .arrangement(Arrangement::Start)
        .build(ctx, |ctx| {
            // Same chrome as the year panel: a divider closes the list at both ends, and the grid
            // gives back 1 dp so the 336 dp total still matches the calendar it replaces.
            Divider::horizontal().build(ctx);
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                // The panel keeps the calendar's height, so the four rows are spread over the whole of
                // it (`SpaceEvenly` on the column) instead of huddling in the middle — `Center` leaves
                // 155 dp of blank above and below. Inside a row, `Start` plus an explicit
                // `CELL_SPACING` is what measures 12 dp across and 12 dp down: `SpaceEvenly` adds its
                // own extra ON TOP of `spacing` (see `flex.rs`), so three 104 dp pills and two 12 dp
                // gaps fill the 336 dp content width exactly.
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(DatePickerDefaults::YEAR_PANEL_HEIGHT - DatePickerDefaults::DIVIDER_THICKNESS),
                    )
                    .arrangement(Arrangement::SpaceEvenly)
                    .build(ctx, |ctx| {
                        for row in 0..4 {
                            Row::new()
                                .modifier(Modifier::new().fill_max_width())
                                .arrangement(Arrangement::Start)
                                .spacing(DatePickerDefaults::CELL_SPACING)
                                .alignment(Alignment::Center)
                                .build(ctx, |ctx| {
                                    for column in 0..3 {
                                        let month_no = (row * 3 + column + 1) as u32;
                                        let label = cell_names[month_no as usize - 1].clone();
                                        let selected = month_no == displayed_month;
                                        let state_for_pick = cell_state.clone();
                                        let model_for_pick = cell_model.clone();
                                        let panel_for_close = cell_panel.clone();
                                        let surface = Surface::new()
                                            .shape(Shape::Pill)
                                            .color(cell_colors.year_container(selected, year_enabled))
                                            .content_color(
                                                cell_colors.year_label(false, selected, year_enabled),
                                            )
                                            .enabled(year_enabled)
                                            .selectable(selected, move || {
                                                let target =
                                                    model_for_pick.month_of(displayed_year, month_no);
                                                state_for_pick.set_displayed_month_millis(
                                                    target.start_utc_time_millis,
                                                );
                                                panel_for_close.set(DockedPanel::Calendar);
                                            })
                                            .modifier(
                                                Modifier::new()
                                                    .size(
                                                        DatePickerDefaults::MONTH_CELL_WIDTH,
                                                        DatePickerDefaults::YEAR_CELL_HEIGHT,
                                                    )
                                                    .test_tag(format!(
                                                        "{MONTH_CELL_TAG_PREFIX}{month_no}"
                                                    )),
                                            );
                                        surface.build(ctx, |ctx| {
                                            Stack::new()
                                                .alignment(Alignment::Center)
                                                .modifier(Modifier::new().fill_max_size())
                                                .build(ctx, |ctx| {
                                                    Text::new(label).build(ctx);
                                                });
                                        });
                                    }
                                });
                        }
                    });
            });
            Divider::horizontal().build(ctx);
        });
}

/// The Cancel/OK action row at the picker's end: the dismiss button first, the confirm button second.
///
/// A row with only one of the two has no gap to draw, so the spacer is composed only when both buttons are.
pub(crate) fn docked_action_row(
    ctx: &mut ComposeCtx,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    let both_present = on_confirm.is_some() && on_dismiss.is_some();
    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding_end(DatePickerDefaults::MODAL_BUTTONS_END_PADDING)
                .padding_bottom(DatePickerDefaults::MODAL_BUTTONS_BOTTOM_PADDING),
        )
        .arrangement(Arrangement::End)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            if let Some(on_dismiss) = on_dismiss {
                Button::text()
                    .on_click(move || on_dismiss())
                    .build(ctx, |ctx| {
                        Text::new("Cancel").build(ctx);
                    });
            }
            if both_present {
                Spacer::horizontal(DatePickerDefaults::MODAL_BUTTONS_SPACING).build(ctx);
            }
            if let Some(on_confirm) = on_confirm {
                Button::text()
                    .on_click(move || on_confirm())
                    .build(ctx, |ctx| {
                        Text::new("OK").build(ctx);
                    });
            }
        });
}
