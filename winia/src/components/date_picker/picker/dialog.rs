use super::*;

/// The modal date picker's dialog (`DatePickerDialog`, `DatePickerDialog.kt:51-61`; its Android body is
/// `DatePickerDialog.android.kt:75-114`).
///
/// material3 wraps the picker in a `BasicAlertDialog` whose own surface is
/// `requiredWidth(ContainerWidth = 360)` and `heightIn(max = ContainerHeight = 568)`, shaped
/// `DatePickerDefaults.shape` (28 dp) and filled with `colors.containerColor` — the dialog contributes no
/// padding, the picker IS the surface. Under it comes the action row: `DialogButtonsPadding` (bottom 8, end 6)
/// holding a `FlowRow` at `DialogButtonsMainAxisSpacing` (8), the dismiss button first and the confirm button
/// second, in `DialogTokens.ActionLabelTextFont` (LabelLarge) with `DialogTokens.ActionLabelTextColor`
/// (`Primary`) as a default the buttons may override.
///
/// The content defaults to [`DatePicker`] over this dialog's state, which is what M3's modal date
/// picker draws; [`DatePickerDialog::content`] replaces it (the input mode will).
///
/// ```ignore
/// DatePickerDialog::new(state.clone(), open.get())
///     .on_dismiss_request(move || open.set(false))
///     .dismiss_button(|ctx| { Button::text().build(ctx, |ctx| { Text::new("Cancel").build(ctx); }); })
///     .confirm_button(|ctx| { Button::text().build(ctx, |ctx| { Text::new("OK").build(ctx); }); })
///     .build(ctx);
/// ```
pub struct DatePickerDialog {
    pub(crate) state: DatePickerState,
    pub(crate) visible: bool,
    pub(crate) on_dismiss_request: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) dismiss_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    pub(crate) confirm_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    pub(crate) content: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    pub(crate) modifier: Modifier,
    pub(crate) shape: Option<Shape>,
    pub(crate) colors: Option<DatePickerColors>,
}

impl DatePickerDialog {
    /// A dialog over `state`, up while `visible` is true. material3 takes the confirm button as a required
    /// parameter and the dismiss button as an optional one; the same here, as builders.
    pub fn new(state: DatePickerState, visible: bool) -> Self {
        Self {
            state,
            visible,
            on_dismiss_request: None,
            dismiss_button: None,
            confirm_button: None,
            content: None,
            modifier: Modifier::new(),
            shape: None,
            colors: None,
        }
    }

    /// Compose's `onDismissRequest`: a click outside the dialog, not the dismiss button.
    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss_request = Some(Arc::new(cb));
        self
    }

    /// The affirming action. It sits after the dismiss button in the row, and the dialog wires no event of its
    /// own into it — not even its enablement.
    pub fn confirm_button(mut self, button: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.confirm_button = Some(Box::new(button));
        self
    }

    /// The dismissing action, before the confirm button.
    pub fn dismiss_button(mut self, button: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.dismiss_button = Some(Box::new(button));
        self
    }

    /// The dialog's content, in place of the default calendar.
    pub fn content(mut self, content: impl Fn(&mut ComposeCtx) + 'static) -> Self {
        self.content = Some(Box::new(content));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// `DatePickerDefaults.shape` (28 dp) by default.
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// The picker's colour roles, read from the theme by default.
    pub fn colors(mut self, colors: DatePickerColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Composes the dialog into the overlay layer while `visible`.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| DatePickerColors::from_theme(&theme));
        let shape = self.shape.unwrap_or(Shape::RoundedRect {
            corner_radius: DatePickerDefaults::CONTAINER_CORNER,
        });
        let button_color = AlertDialogDefaults::button_color(&theme);
        let button_style = WiniaTheme::typography().label_large.clone();
        let state = self.state.clone();
        let confirm = self.confirm_button;
        let dismiss = self.dismiss_button;
        let content = self.content;
        // The same set the surface is painted from, handed to the default content too. This is a winia
        // convenience rather than a copy of Compose's wiring: material3's `DatePickerDialog` reads its
        // `colors` in exactly one place — `color = colors.containerColor` on its own `Surface`
        // (`DatePickerDialog.android.kt:86`) — and then calls the caller's slot with nothing
        // (`:95  Box(Modifier.weight(1f, fill = false)) { this@Column.content() }`). A caller nesting a
        // `DatePicker` is expected to pass `colors` down itself; winia has no caller for its default
        // content to be anyone but itself, so the dialog does it. Either way the visible result was wrong
        // before: a caller overriding the dialog got a theme-coloured calendar inside their own container.
        let content_colors = colors.clone();
        let size = Modifier::new()
            .width(DatePickerDefaults::CONTAINER_WIDTH)
            .max_height(DatePickerDefaults::MODAL_CONTAINER_HEIGHT);

        let dialog = BasicAlertDialog::new(self.visible)
            .shape(shape)
            .container_color(colors.container)
            // The picker is the container: material3's date picker dialog adds no padding, where winia's
            // `BasicAlertDialog` carries the alert dialog's 24 dp (`DatePickerDialog.android.kt:88-93`).
            .content_padding(0.0)
            // `DialogProperties(usePlatformDefaultWidth = false)` — material3's date picker dialog passes
            // it (`DatePickerDialog.kt:59`) so the 280..560 range does not constrain `requiredWidth(360)`:
            // the picker's own width is the dialog's width, and the `size` modifier below is the only
            // authority on it.
            .platform_default_width(false)
            .modifier(self.modifier.then(size))
            .content(move |ctx| {
                // `Column(verticalArrangement = SpaceBetween)`: the content in a `weight(1f, fill = false)` box,
                // the action row aligned to the end under it (`DatePickerDialog.android.kt:89-111`).
                //
                // The box is what lets the dialog be SHORT: its share is the box's MAXIMUM, so the
                // box reports the size the picker actually asked for and the Column ends up content
                // + buttons rather than the whole cap. material3 says so in the source — "Fill is
                // false to support collapsing the dialog's height when switching to input mode"
                // (`:93-94`) — while the calendar's own height is what fills the cap in picker mode.
                //
                // `Arrangement::SpaceBetween` as the source writes it. That only works because a
                // spreading arrangement no longer grows a content-sized container to the maximum its
                // parent offers (`layout/flex.rs`'s `measured_main`, aligned with Compose's
                // `mainAxisLayoutSize = max(fixedSpace + weightedSpace, mainAxisMin)`); before that
                // alignment this column had to say `Start` to escape the 568 dp cap. The leftover here
                // is zero either way, so the two place identically.
                Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .arrangement(Arrangement::SpaceBetween)
                    .alignment(Alignment::End)
                    .build(ctx, |ctx| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .fill_max_width()
                                    .layout_weight_fill(1.0, false),
                            )
                            .build(ctx, |ctx| {
                                match content.as_ref() {
                                    Some(content) => content(ctx),
                                    None => DatePicker::new(state.clone())
                                        .colors(content_colors.clone())
                                        .build(ctx),
                                }
                            });
                        Row::new()
                            .modifier(
                                Modifier::new()
                                    .padding_bottom(DatePickerDefaults::MODAL_BUTTONS_BOTTOM_PADDING)
                                    .padding_end(DatePickerDefaults::MODAL_BUTTONS_END_PADDING),
                            )
                            .spacing(DatePickerDefaults::MODAL_BUTTONS_SPACING)
                            .alignment(Alignment::Center)
                            .build(ctx, |ctx| {
                                ProvideTextStyle(button_style.clone(), ctx, |ctx| {
                                    WiniaTheme::with_content_color(button_color, ctx, |ctx| {
                                        if let Some(dismiss) = dismiss.as_ref() {
                                            dismiss(ctx);
                                        }
                                        if let Some(confirm) = confirm.as_ref() {
                                            confirm(ctx);
                                        }
                                    });
                                });
                            });
                    });
            });
        // The handler is already boxed here, so it goes in through the same slot `AlertDialog` uses.
        dialog.dismiss_handler(self.on_dismiss_request).build(ctx);
    }
}

/// The header: the title over the headline, with the divider below them
/// (`DateEntryContainer` and `DatePickerHeader`, `DatePicker.kt:1365-1396`, `:1671-1698`).
pub(crate) fn header(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    title: Option<&str>,
    colors: &DatePickerColors,
    display_mode: DisplayMode,
    on_toggle_display_mode: impl Fn() + Send + Sync + 'static,
) {
    let model = state.calendar_model();
    // material3's headline says what is selected in either mode, and names the mode's own wording
    // when nothing is (`DatePickerHeadline`, `DatePicker.kt:701-717`). The description carries the
    // verbose date for a reader, falling back to the mode's own "nothing" wording.
    let headline = state
        .selected_date_millis()
        .map(|millis| model.format_date(millis, false))
        .unwrap_or_else(|| match display_mode {
            DisplayMode::Picker => DatePickerDefaults::HEADLINE.to_string(),
            DisplayMode::Input => DATE_INPUT_HEADLINE.to_string(),
        });
    let headline_description = state
        .selected_date_millis()
        .map(|millis| model.format_date(millis, true))
        .unwrap_or_else(|| DatePickerDefaults::NO_SELECTION_DESCRIPTION.to_string());
    let headline_description = match display_mode {
        DisplayMode::Picker => format_string(
            DatePickerDefaults::HEADLINE_DESCRIPTION,
            &[&headline_description],
        ),
        DisplayMode::Input => format_string(DATE_INPUT_HEADLINE_DESCRIPTION, &[&headline_description]),
    };
    // material3 gives the header a *minimum* height of 120 dp and lets its content grow past it; winia
    // stretches an auto-height child to fill the space its parent offers, which with `SpaceBetween` would push
    // the title and the headline apart, so the header's height is exact here.
    let height = if title.is_some() {
        DatePickerDefaults::HEADER_MIN_HEIGHT
    } else {
        DatePickerDefaults::HEADLINE_ROW_HEIGHT
    };
    let title = title.map(str::to_string);
    let title_color = colors.title_content;
    let headline_color = colors.headline_content;

    Column::new()
        .modifier(Modifier::new().fill_max_width().height(height))
        .arrangement(Arrangement::SpaceBetween)
        .build(ctx, |ctx| {
            if let Some(text) = title.as_deref() {
                ProvideTextStyle(WiniaTheme::typography().label_large.clone(), ctx, |ctx| {
                    Text::new(text.to_string())
                        .color(title_color)
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .padding_start(DatePickerDefaults::TITLE_START_PADDING)
                                .padding_end(DatePickerDefaults::TITLE_END_PADDING)
                                .padding_top(DatePickerDefaults::TITLE_TOP_PADDING),
                        )
                        .build(ctx);
                });
            }
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .arrangement(Arrangement::SpaceBetween)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    ProvideTextStyle(WiniaTheme::typography().headline_large.clone(), ctx, |ctx| {
                        Text::new(headline)
                            .color(headline_color)
                            .max_lines(1)
                            .modifier(
                                Modifier::new()
                                    .padding_start(DatePickerDefaults::TITLE_START_PADDING)
                                    .padding_end(DatePickerDefaults::TITLE_END_PADDING)
                                    .padding_bottom(DatePickerDefaults::HEADLINE_BOTTOM_PADDING)
                                    // The headline announces both what it reads and which mode it is
                                    // in, politely rather than assertively, so a selection made in
                                    // the calendar or typed in the field is picked up either way
                                    // (`DatePicker.kt:722-725`).
                                    .semantics(
                                        crate::semantics::SemanticsConfig::new()
                                            .content_description(headline_description)
                                            .live_region(
                                                crate::semantics::LiveRegionMode::Polite,
                                            ),
                                    ),
                            )
                            .build(ctx);
                    });
                    display_mode_toggle(
                        ctx,
                        display_mode,
                        on_toggle_display_mode,
                        headline_color,
                    );
                });
            // material3 draws the divider when a title, a headline or a mode toggle is present
            // (`DatePicker.kt:1392-1394`); a headline is always composed here.
            Divider::horizontal().build(ctx);
        });
}

/// The month navigation row: the year menu button, then the two month arrows while the year panel is closed
/// (`MonthsNavigation`, `DatePicker.kt:2182-2239`). material3 drops the arrows and packs the row to its start
/// while the panel is open.
///
/// The arrows' enablement and effect both come from the page list, not from the year range:
/// `nextAvailable = monthsListState.canScrollForward` (`DatePicker.kt:1561-1562`) and a press runs
/// `animateScrollToItem(firstVisibleItemIndex ± 1)` (`DatePicker.kt:1569-1592`). An arrow that read the
/// year range instead could disagree with a swipe at the ends of the list.
pub(crate) fn months_navigation(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    month: &CalendarMonth,
    year_panel_open: bool,
    on_toggle_year_panel: impl Fn() + Send + Sync + 'static,
    colors: &DatePickerColors,
    list: &LazyListState,
    step_in_flight: &State<Option<usize>>,
) {
    let text = state.calendar_model().format_month_year(month.start_utc_time_millis);
    let navigation_color = colors.navigation_content;

    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_YEAR_HEIGHT),
        )
        .arrangement(if year_panel_open {
            Arrangement::Start
        } else {
            Arrangement::SpaceBetween
        })
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            year_menu_button(ctx, text, on_toggle_year_panel, colors, true, YEAR_MENU_TAG);
            if !year_panel_open {
                // The arrows are one unit at the row's far end: in a `SpaceBetween` row they need a row of
                // their own, or the arrangement would spread them across the whole width.
                Row::new()
                    .arrangement(Arrangement::Start)
                    .alignment(Alignment::Center)
                    .build(ctx, |ctx| {
                        // The arrows only care whether the list is at an end, so they listen to the
                        // derived flags instead of the scroll offset: scrolling inside one month does
                        // not rebuild them.
                        let back_enabled = list.remember_can_scroll_backward(ctx);
                        let forward_enabled = list.remember_can_scroll_forward(ctx);
                        // The last page the list can show, so a forward step cannot ask past the end
                        // and leave the arrow's own request stranded above the clamp.
                        let last_page = CalendarModel::number_of_months_in_range(&state.year_range())
                            .saturating_sub(1) as usize;
                        let back = list.clone();
                        let back_flight = step_in_flight.clone();
                        let back_anchor = list.first_visible();
                        step_arrow(ctx, move || {
                            let target = arrow_target(back_anchor, back_flight.get(), false, last_page);
                            back_flight.set(Some(target));
                            back.animate_scroll_to_item(target, 0.0);
                        }, back_enabled.get(), CHEVRON_LEFT_PATH, navigation_color, IconButtonSize::Small, true);
                        let forward = list.clone();
                        let forward_flight = step_in_flight.clone();
                        let forward_anchor = list.first_visible();
                        step_arrow(ctx, move || {
                            let target = arrow_target(forward_anchor, forward_flight.get(), true, last_page);
                            forward_flight.set(Some(target));
                            forward.animate_scroll_to_item(target, 0.0);
                        }, forward_enabled.get(), CHEVRON_RIGHT_PATH, navigation_color, IconButtonSize::Small, true);
                    });
            }
        });
}

/// One navigation arrow: the month stepper's arrows (`monthsListState.canScrollBackward/Forward`,
/// `DatePicker.kt:1561-1562`; enabled here while the month has a neighbour inside the year range) and the docked
/// picker's year arrows.
///
/// `on_click` carries what the arrow steps, because that is the only thing that differs between them — the
/// month arrows read the displayed month at the moment of the click (`DatePickerState::step_displayed_month`),
/// the year ones step the displayed year by one. Sharing the control keeps the hiding rule in one place.
///
/// The glyph auto-mirrors, which is the second half of RTL and is easy to miss: `Row` already puts the
/// previous arrow at the start (the RIGHT edge in RTL), so without the mirror both arrows would point
/// INWARD — "previous" pointing right, towards the label, and "next" pointing left, towards it too.
/// material3 asks for exactly this by drawing the two arrows as `Icons.AutoMirrored.Filled.KeyboardArrowLeft`
/// and `…KeyboardArrowRight` (`DatePicker.kt:2225`, `:2232`), so the artwork flips and each arrow keeps
/// pointing outward from the month it moves.
pub(crate) fn step_arrow(
    ctx: &mut ComposeCtx,
    on_click: impl Fn() + Send + Sync + 'static,
    enabled: bool,
    path: &'static str,
    color: Color,
    size: IconButtonSize,
    visible: bool,
) {
    // Hidden arrows stay composed at alpha 0 (still disabled): removing them would shift the menu
    // button, and the alpha reads straight into a fade animation later.
    let mut modifier = Modifier::new();
    if !visible {
        modifier = modifier.alpha(0.0);
    }
    IconButton::new()
        .size(size)
        .modifier(modifier)
        .enabled(enabled && visible)
        .on_click(on_click)
        .build(ctx, |ctx| {
            Icon::svg_path(path)
                .tint(color)
                .auto_mirror(true)
                .build(ctx);
        });
}

/// Which page a month arrow should step to, given where the list is and where it is already going.
///
/// The arrows step **one page from the page already in flight**, not from the anchor. That distinction
/// is the whole of a reported asymmetry: the anchor (`first_visible`) is the page whose span still
/// contains the pixel offset, so while a forward spring runs it names the page being LEFT — `anchor + 1`
/// then keeps naming the page already being animated to, the request is deduplicated as "already going
/// there", and every press during the animation is swallowed. Scrolling backward flips the anchor as
/// soon as the offset leaves the old page's span, so `anchor - 1` names a genuinely new page and every
/// press lands. Measured on the docked demo, three presses 120 ms apart: `prev` advanced 2 months,
/// `next` advanced 0.
///
/// `in_flight` is the last page an arrow asked for, and is cleared once the list arrives (`sync_month_pages`
/// clears it), so a press during the motion continues from the target and a press after it starts from the
/// anchor again. Either way each press is worth exactly one page.
///
/// Clamped to `[0, last_page]`: an arrow is disabled at the ends, but the in-flight page can be past the
/// end for the frame between the request and the measure that clamps it.
///
/// **Deliberately beyond Compose, and the reason is recorded here rather than in the docs alone.** Compose
/// reads `firstVisibleItemIndex ± 1` at the press (`DatePicker.kt:1569-1592`) and therefore swallows a
/// press during a forward animation too — material3 wraps both handlers in
/// `catch (_: IllegalArgumentException)` for exactly that (`:1571-1587`). One press for one page is fine;
/// one DIRECTION swallowing and the other not is not, and that is what the asymmetry above amounts to.
pub(crate) fn arrow_target(anchor: usize, in_flight: Option<usize>, forward: bool, last_page: usize) -> usize {
    let from = in_flight.unwrap_or(anchor);
    if forward {
        from.saturating_add(1).min(last_page)
    } else {
        from.saturating_sub(1)
    }
}

/// The year menu button: the "September 2024" text and a dropdown arrow, the control that opens the year panel
/// (`YearPickerMenuButton`, `DatePicker.kt:2243-2269`). material3 builds it from a `TextButton` whose elevation
/// and border it explicitly clears; winia's buttons carry no such parameters to clear.
///
/// A disabled button degrades to plain dimmed text with its dropdown glyph faded to alpha 0 (still
/// composed, so the label does not shift) and no interaction — the M3 specs docked figure shows the idle
/// group exactly so ("2025" with neither pill nor arrow) while the other group's list is open.
pub(crate) fn year_menu_button(
    ctx: &mut ComposeCtx,
    text: String,
    on_click: impl Fn() + Send + Sync + 'static,
    colors: &DatePickerColors,
    enabled: bool,
    tag: &str,
) {
    let description = text.clone();
    let color = if enabled {
        colors.navigation_content
    } else {
        DatePickerColors::disabled(colors.navigation_content)
    };
    Surface::new()
        .shape(Shape::Pill)
        .color(Color::TRANSPARENT)
        .content_color(color)
        .enabled(enabled)
        .selectable(false, on_click)
        .modifier(
            Modifier::new()
                .height(DatePickerDefaults::YEAR_MENU_BUTTON_HEIGHT)
                .test_tag(tag.to_string())
                // material3 repeats the button's text as its content description and makes it a polite live
                // region, so a reader announces the month as the arrows move it (`DatePicker.kt:2205-2216`).
                .semantics(crate::semantics::SemanticsConfig::new().content_description(description)),
        )
        .build(ctx, |ctx| {
            // The wrapper fills the button's 40 dp height and centres the row in it; a wrap-content
            // row would hug the top while the arrows around it centre in theirs. Height only: filling
            // the width as well stretches the surface full-bleed and the hover state layer with it.
            Stack::new()
                .alignment(Alignment::Center)
                .modifier(Modifier::new().fill_max_height())
                .build(ctx, |ctx| {
                    // material3's `TextButtonWithIconContentPadding` (a `TextButton` holding the
                    // dropdown glyph): 12 dp at the start, 16 dp at the end (`Button.kt:515-522`).
                    Row::new()
                        .modifier(
                            Modifier::new()
                                .padding_start(12.0)
                                .padding_end(16.0),
                        )
                        .arrangement(Arrangement::Start)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            ProvideTextStyle(WiniaTheme::typography().label_large.clone(), ctx, |ctx| {
                                Text::new(text).color(color).max_lines(1).build(ctx);
                            });
                            Spacer::horizontal(DatePickerDefaults::YEAR_MENU_ICON_SPACING).build(ctx);
                            // Faded, not removed: dropping the glyph would shrink the button and shift the
                            // label, and the alpha reads straight into a fade animation later.
                            let mut glyph = Modifier::new();
                            if !enabled {
                                glyph = glyph.alpha(0.0);
                            }
                            Icon::svg_path(ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH)
                                .tint(color)
                                .modifier(glyph)
                                .build(ctx);
                        });
                });
        });
}

/// The row the year list starts on when the panel opens: material3's
/// `max(0, displayedYear - yearRange.first - YearsInRow)` as an item index of a three-column grid
/// (`DatePicker.kt:2073-2080`), which is one row above the displayed year.
pub(crate) fn year_panel_first_row(state: &DatePickerState, model: &CalendarModel) -> usize {
    let year_range = state.year_range();
    let displayed_year = model.month_of_millis(state.displayed_month_millis()).year;
    let offset = (displayed_year - *year_range.start()).max(0) as usize;
    (offset / DatePickerDefaults::YEARS_PER_ROW).saturating_sub(1)
}

/// The year panel: three columns of years in a lazy list, `YEAR_PANEL_HEIGHT` tall over the divider that closes
/// it (`YearPicker`, `DatePicker.kt:2061-2116`).
///
/// material3 overlays this on the month calendar and keeps the calendar composed underneath
/// (`DatePicker.kt:1612-1660`); the modal picker composes the same shape — the calendar in a `Stack`'s first
/// child, this panel expanding over it in an `AnimatedVisibility` clipped to its own bounds — so the panel's
/// height matching the weekday row plus the grid it covers (335 + 1 dp of divider against 48 + 288) is what
/// keeps the container still rather than what hid the swap. The animation spec is winia's own: Compose reads
/// motion-scheme tokens and winia has no motion scheme.
pub(crate) fn year_panel(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    colors: &DatePickerColors,
    rows: &LazyListState,
    on_year_selected: impl Fn(i32) + Clone + Send + Sync + 'static,
) {
    let year_range = state.year_range();
    let first = *year_range.start();
    let count = year_range.clone().count();
    let row_count = count.div_ceil(DatePickerDefaults::YEARS_PER_ROW);
    let current_year = model.month_of_millis(state.today_millis()).year;
    // material3 highlights the DISPLAYED year, not the selected date's year — `YearPicker` passes
    // `selected = selectedYear == displayedYear` (`DatePicker.kt:2091`). The displayed year is what
    // the panel replaces and what picking one changes, so it is the one the user sees filled.
    let displayed_year = model.month_of_millis(state.displayed_month_millis()).year;
    let list_state = rows.clone();
    let cell_state = state.clone();
    let cell_colors = colors.clone();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::YEAR_PANEL_HEIGHT + DatePickerDefaults::DIVIDER_THICKNESS),
        )
        .arrangement(Arrangement::Start)
        .build(ctx, |ctx| {
            // A divider closes the panel at both ends; the list gives back 1 dp so the 336 dp
            // total still matches the weekday row plus the grid it replaces.
            Divider::horizontal().build(ctx);
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(DatePickerDefaults::YEAR_PANEL_HEIGHT - DatePickerDefaults::DIVIDER_THICKNESS),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        LazyColumn::new()
                            .modifier(Modifier::new().layout_weight(1.0).fill_max_height())
                            .state(list_state.clone())
                            .spacing(DatePickerDefaults::YEARS_VERTICAL_PADDING)
                            .items(
                                row_count,
                                |row| row as u64,
                                move |ctx, row| {
                                    Row::new()
                                        .modifier(Modifier::new().fill_max_width())
                                        .arrangement(Arrangement::SpaceEvenly)
                                        .alignment(Alignment::Center)
                                        .build(ctx, |ctx| {
                                            for column in 0..DatePickerDefaults::YEARS_PER_ROW {
                                                let index =
                                                    row * DatePickerDefaults::YEARS_PER_ROW + column;
                                                if index >= count {
                                                    break;
                                                }
                                                let year = first + index as i32;
                                                year_cell(
                                                    ctx,
                                                    &cell_colors,
                                                    cell_state
                                                        .selectable_dates()
                                                        .is_selectable_year(year),
                                                    year,
                                                    displayed_year == year,
                                                    year == current_year,
                                                    on_year_selected.clone(),
                                                );
                                            }
                                        });
                                },
                            )
                            .build(ctx);
                        // Always visible: the list is 2412 months tall and nothing else tells the user
                        // it scrolls (the bottom row is cut off mid-year otherwise).
                        LazyScrollbar::new(list_state)
                            .always_show(true)
                            .build(ctx);
                    });
            });
            Divider::horizontal().build(ctx);
        });
}

/// One year in the panel (`Year`, `DatePicker.kt:2120-2180`): a 72 × 36 stadium that fills with `Primary` when
/// it is the displayed year, and carries the same 1 dp outline around the current year that today gets. winia
/// notes `Pill` where material3 notes `CornerFull`; on a 72 × 36 box they are the same stadium.
pub(crate) fn year_cell(
    ctx: &mut ComposeCtx,
    colors: &DatePickerColors,
    enabled: bool,
    year: i32,
    selected: bool,
    current_year: bool,
    on_year_selected: impl Fn(i32) + Send + Sync + 'static,
) {
    let label = year.to_string();
    // material3 merges the description into the surface and clears the inner text's semantics
    // (`DatePicker.kt:2141-2162`); the label's colour role rides on the surface's content colour here.
    let description = format!("Navigate to {label}");
    let mut surface = Surface::new()
        .shape(Shape::Pill)
        .color(colors.year_container(selected, enabled))
        .content_color(colors.year_label(current_year, selected, enabled))
        .enabled(enabled)
        .selectable(selected, move || on_year_selected(year))
        .modifier(
            Modifier::new()
                .size(
                    DatePickerDefaults::YEAR_CELL_WIDTH,
                    DatePickerDefaults::YEAR_CELL_HEIGHT,
                )
                .test_tag(format!("{YEAR_CELL_TAG_PREFIX}{year}"))
                .semantics(crate::semantics::SemanticsConfig::new().content_description(description)),
        );
    if current_year && !selected {
        surface = surface.border(SurfaceBorder::new(
            DatePickerDefaults::TODAY_OUTLINE_WIDTH,
            colors.today_border,
        ));
    }
    surface.build(ctx, |ctx| {
        // Same centering as `day_cell`: the surface lays content out top-start, so without this
        // the label hugs the pill's edge and the today outline clips it (measured on "2026").
        Stack::new()
            .alignment(Alignment::Center)
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new(label).build(ctx);
            });
    });
}

/// The weekday header: the locale's names in row order, one cell per column (`WeekDays`,
/// `DatePicker.kt:1783-1830`).
pub(crate) fn weekday_row(ctx: &mut ComposeCtx, model: &CalendarModel, colors: &DatePickerColors) {
    let names = model.weekday_names();
    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::ACCESSIBLE_SIZE),
        )
        .arrangement(Arrangement::SpaceEvenly)
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                for (full, narrow) in names {
                    Stack::new()
                        .alignment(Alignment::Center)
                        .modifier(Modifier::new().size(
                            DatePickerDefaults::ACCESSIBLE_SIZE,
                            DatePickerDefaults::ACCESSIBLE_SIZE,
                        ))
                        .build(ctx, |ctx| {
                            Text::new(narrow)
                                .color(colors.weekday_content)
                                .modifier(Modifier::new().semantics(
                                    crate::semantics::SemanticsConfig::new()
                                        .content_description(full),
                                ))
                                .build(ctx);
                        });
                }
            });
        });
}

/// The month grid: six rows of seven slots, each slot a day (`Month`, `DatePicker.kt:1856-1890`).
///
/// `show_outside_month` decides what the leading and trailing slots do with the days that belong to the
/// neighbouring month, which [`MonthGrid`] always computes — the dates are true whether or not they are
/// drawn. The docked picker asks for them and material3 does not:
///
/// - Docked: drawn, at the specs' dimmed label. The M3 specs' *Docked date picker* anatomy lists
///   "Outside month date" among the grid's states, and gives it two tokens of its own.
/// - Modal: a `Spacer`, exactly as `Month` composes them (`DatePicker.kt:1870-1890`). The *Modal date
///   picker* anatomy on the same specs page has no "Outside month date" entry at all.
///
/// The slot still measures 48 dp either way, so the grid's geometry does not depend on the flag.
pub(crate) fn month_grid(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    grid: &MonthGrid,
    colors: &DatePickerColors,
    show_outside_month: bool,
) {
    let rows = grid.rows().map(<[DayCell]>::to_vec).collect::<Vec<_>>();
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_HEIGHT),
        )
        .arrangement(Arrangement::SpaceEvenly)
        .build(ctx, |ctx| {
            ProvideTextStyle(WiniaTheme::typography().body_large.clone(), ctx, |ctx| {
                for row in &rows {
                    Row::new()
                        .modifier(Modifier::new().fill_max_width())
                        .arrangement(Arrangement::SpaceEvenly)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            for cell in row {
                                // material3's empty cell, and it is NOT an empty node: `Month` composes a
                                // `Spacer` sized to the day's 48 dp so the row keeps its lattice
                                // (`DatePicker.kt:1876-1890`, whose comment says exactly that). Composing
                                // nothing collapses the row to zero and `SpaceEvenly` then shifts every
                                // row above it — measured on September 2024, whose trailing week is
                                // entirely outside the month: the rows moved 13.7 dp up, and a scan across
                                // the 10th's old centre measured a 31 dp chord instead of 39.
                                //
                                // An empty `Stack`, because winia's `Spacer` only spans one axis; both are
                                // childless `BoxLayout`s, so the slot measures the same.
                                if cell.is_outside_month && !show_outside_month {
                                    Stack::new()
                                        .modifier(Modifier::new().size(
                                            DatePickerDefaults::ACCESSIBLE_SIZE,
                                            DatePickerDefaults::ACCESSIBLE_SIZE,
                                        ))
                                        .build(ctx, |_| {});
                                    continue;
                                }
                                Stack::new()
                                    .alignment(Alignment::Center)
                                    .modifier(Modifier::new().size(
                                        DatePickerDefaults::ACCESSIBLE_SIZE,
                                        DatePickerDefaults::ACCESSIBLE_SIZE,
                                    ))
                                    .build(ctx, |ctx| {
                                        day_cell(ctx, state, model, cell, colors);
                                    });
                            }
                        });
                }
            });
        });
}

/// The selected day's background circle, drawn by its own node so that only it animates: it fades
/// 0 → full over [`DAY_POP_MILLIS`] when the selection lands (and back out when it leaves), radius
/// always full, and the label above it never moves.
/// The progress is peeked at paint time (ticks never recompose); per the `DrawNode` contract it stays
/// out of `node_key` — only the static color fingerprints the node.
///
/// How long the selected circle takes to fade in and out. material3 has no selection animation of its own
/// (the container colour switches outright, `DatePicker.kt:973-993`), so this is winia's own: a short,
/// symmetric ease-out that keeps the change legible without making the grid feel slow.
pub(crate) const DAY_POP_MILLIS: u64 = 220;
#[derive(Debug)]
struct DayCircleNode {
    pub(crate) progress: State<f32>,
    pub(crate) color: Color,
}

impl crate::modifier::DrawNode for DayCircleNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        let t = self.progress.peek().clamp(0.0, 1.0);
        if t <= 0.0 {
            return;
        }
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::PaintStyle::Fill);
        paint.set_color(crate::graphics::skia_color(Color {
            a: (t * 255.0).round() as u8,
            ..self.color
        }));
        canvas.draw_circle(
            skia_safe::Point::new(rect.center_x(), rect.center_y()),
            rect.width().min(rect.height()) / 2.0,
            &paint,
        );
    }
    fn node_key(&self) -> String {
        format!("day-circle:{:?}", self.color)
    }
}

/// One day of the grid: a 40 dp circle, outlined when it is today and not selected, filled when it is selected
/// (`Day`, `DatePicker.kt:1993-2058`).
///
/// An outside-month cell (`DayCell::is_outside_month`) is context: it takes the specs' dimmed label, gets no
/// today ring and no selection fill, and is not clickable — which `is_enabled` already carries, since
/// [`MonthGrid`] never enables one.
pub(crate) fn day_cell(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    cell: &DayCell,
    colors: &DatePickerColors,
) {
    let state_for_click = state.clone();
    let millis = cell.utc_time_millis;
    let description = day_content_description(model, cell);
    let outside = cell.is_outside_month;
    // Selection progress 0 → 1: every cell remembers one, so landing the selection here plays the
    // circle in, and moving it away plays the same value 1 → 0 (the node keeps drawing while the
    // progress is above zero, so the circle shrinks out instead of vanishing).
    //
    // An outside cell is pinned to 0: a date selected in another month can appear here, and a context cell
    // must not claim it with a fill the user cannot tap to keep.
    let pop = ctx.animate_float_as_state(
        if cell.is_selected && !outside { 1.0 } else { 0.0 },
        AnimationSpec::Tween(TweenSpec::new(
            std::time::Duration::from_millis(DAY_POP_MILLIS),
            crate::animation::interpolator::EaseOutCubic::new(),
        )),
    );
    let mut surface = Surface::new()
        .shape(Shape::Circle)
        .color(Color::TRANSPARENT)
        .content_color(if outside {
            colors.outside_month_label()
        } else {
            colors.day_label(cell.is_selected, cell.is_enabled, cell.is_today)
        })
        .enabled(cell.is_enabled)
        .selectable(cell.is_selected, move || {
            state_for_click.set_selected_date_millis(Some(millis));
        })
        .modifier(
            Modifier::new()
                .size(DatePickerDefaults::DAY_CELL, DatePickerDefaults::DAY_CELL)
                .semantics(
                    crate::semantics::SemanticsConfig::new().content_description(description),
                )
                .draw_node(DayCircleNode {
                    // The circle's own selected-container color, NOT `day_container(is_selected)`:
                    // on deselect the cell rebuilds unselected while the progress is still fading,
                    // and `TRANSPARENT` carries zeroed rgb — fading that paints a black disc instead
                    // of the primary one (measured). The node only draws while progress > 0, so the
                    // base stays valid across the whole out-play.
                    progress: pop,
                    color: colors.day_container(true, cell.is_enabled),
                }),
        );
    // No today ring on a context cell either: the specs draw "Today's date" and "Outside month date" as
    // separate states, and a ring would claim a cell that cannot be chosen.
    if cell.is_today && !cell.is_selected && !outside {
        surface = surface.border(SurfaceBorder::new(
            DatePickerDefaults::TODAY_OUTLINE_WIDTH,
            colors.today_border,
        ));
    }
    surface.build(ctx, |ctx| {
        // material3's `Day` centers its label in the 40 dp circle (`DatePicker.kt:1993-2058`); the surface
        // itself lays content out top-start, so the centering is explicit here.
        Stack::new()
            .alignment(Alignment::Center)
            .modifier(Modifier::new().fill_max_size())
            .build(ctx, |ctx| {
                Text::new(cell.day.to_string()).build(ctx);
            });
    });
}

