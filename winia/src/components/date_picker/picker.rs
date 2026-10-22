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
pub struct DatePicker {
    state: DatePickerState,
    title: Option<String>,
    /// Whether `title` is still material3's default. Compose hands the title a lambda that names the
    /// mode (`title = { if (displayMode == Input) DateInputTitle else DatePickerTitle }`,
    /// `DatePicker.kt:654`), so the default follows the mode while a caller's own title does not.
    title_is_default: bool,
    /// The colour roles, read from the theme when absent. [`DatePickerDialog`] forwards its own set here
    /// for its default content — a winia convenience, since material3's dialog uses its `colors` only for
    /// its own surface (`DatePickerDialog.android.kt:86`) and invokes the caller's slot with nothing
    /// (`:95`).
    colors: Option<DatePickerColors>,
    modifier: Modifier,
}

impl DatePicker {
    /// A picker over `state`, with material3's default title.
    pub fn new(state: DatePickerState) -> Self {
        Self {
            state,
            title: Some(DatePickerDefaults::TITLE.to_string()),
            title_is_default: true,
            colors: None,
            modifier: Modifier::new(),
        }
    }

    /// The picker's colour roles (`colors`), read from the theme by default.
    ///
    /// Material3 has no such parameter on `DatePicker` itself — the colours are whatever the composable
    /// that built it passed (`DatePicker.kt:172` is its own parameter default) — but winia needs the
    /// setter so [`DatePickerDialog`] can hand its set down. The gap it closes was visible: the dialog
    /// painted its surface from `colors.container` and then let the default content re-derive a fresh
    /// set from the theme, so an overridden dialog got a calendar in the theme's colours inside a
    /// container in the caller's.
    ///
    /// This is a winia convenience, not a port of Compose's wiring — see [`DatePickerDialog::build`].
    pub fn colors(mut self, colors: DatePickerColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// The title above the headline. `None` drops the title slot, and with it the header's minimum height and
    /// the divider (`DatePicker.kt:1680-1685`, `:1392-1394`).
    pub fn title(mut self, title: Option<impl Into<String>>) -> Self {
        self.title = title.map(Into::into);
        self.title_is_default = false;
        self
    }

    /// A modifier for the container (`modifier`).
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Composes the picker.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let colors = self.colors.unwrap_or_else(|| DatePickerColors::from_theme(&WiniaTheme::colors()));
        let model = self.state.calendar_model().clone();
        let month = model.month_of_millis(self.state.displayed_month_millis());
        let state = self.state.clone();
        let title = self.title.clone();
        let title_is_default = self.title_is_default;
        // material3 keeps the year panel's visibility in a `rememberSaveable` inside the picker
        // (`DatePicker.kt:1557`). winia keeps it in a remembered `State`, so a toggle recomposes the picker.
        let year_panel_open = ctx.remember(|| false);
        // The panel's row list, which the toggle scrolls to the row above the displayed year.
        let year_rows = ctx.remember(LazyListState::new).get();
        // The paged month list. Its position IS the displayed month (see `sync_month_pages`), so the
        // arrows animate it rather than writing the month — the same two-way sync material3 has
        // between `monthsListState` and `displayedMonthMillis` (`DatePicker.kt:1541-1592`). It is
        // seeded by that sync's `scroll_to_item`, which is the anchor-authoritative jump: a
        // constructor cannot do it, because the pixel offset it needs comes from the measure.
        let month_rows = ctx.remember(LazyListState::new).get();
        // The page the month arrows have already asked for but the list has not reached, so a press
        // during the animation continues from it instead of restating it (see `arrow_target`).
        let month_step_in_flight = ctx.remember(|| None::<usize>);
        // Page 0 is the January of the range's first year, so a page index converts to a month by
        // counting from here — the same reference `month_pages` composes against.
        let first_month = model.month_of(*state.year_range().start(), 1).start_utc_time_millis;
        // `SwitchableDateEntryContent` reads the mode here and hands the toggle to the header
        // (`DatePicker.kt:1432`, `:1467-1480`). Reading it registers this slot as a reader, so the
        // calendar and the text field replace each other rather than both being composed.
        let display_mode = state.display_mode();
        let on_toggle_display_mode = {
            let state = state.clone();
            move || {
                let next = match state.display_mode() {
                    DisplayMode::Picker => DisplayMode::Input,
                    DisplayMode::Input => DisplayMode::Picker,
                };
                state.set_display_mode(next);
            }
        };
        let on_toggle_year_panel = {
            let year_panel_open = year_panel_open.clone();
            let year_rows = year_rows.clone();
            let state = state.clone();
            let model = model.clone();
            move || {
                let open = !year_panel_open.get();
                if open {
                    year_rows.scroll_to_item(year_panel_first_row(&state, &model), 0.0);
                }
                year_panel_open.set(open);
            }
        };
        // material3's `onYearSelected` both shows the picked year and closes the panel (`DatePicker.kt:1643-1652`).
        let on_year_selected = {
            let year_panel_open = year_panel_open.clone();
            let state = state.clone();
            move |year: i32| {
                state.set_displayed_year(year);
                year_panel_open.set(false);
            }
        };
        // material3 pins the picker's width with `sizeIn(minWidth = 360)` on a column that wraps its content,
        // and those numbers are exact: seven slots of the 48 dp accessibility size (336) plus the 12 dp of
        // horizontal padding on both sides. winia stretches an auto-width child to the width its parent
        // offers, which would spread the weekday row and the grid past that lattice, so the width is pinned.
        let container = self
            .modifier
            .then(Modifier::new().width(DatePickerDefaults::CONTAINER_WIDTH))
            .background(colors.container, Shape::Rectangle);

        Column::new()
            .modifier(container)
            .arrangement(Arrangement::Start)
            .build(ctx, |ctx| {
                // Compose's default title names the mode it is asking about (`DatePicker.kt:654`), so the
                // default follows the mode while a title the caller supplied stands on its own.
                let title = if title_is_default && display_mode == DisplayMode::Input {
                    Some(DatePickerDefaults::INPUT_TITLE.to_string())
                } else {
                    title.clone()
                };
                header(ctx, &state, title.as_deref(), &colors, display_mode, on_toggle_display_mode);
                Column::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .padding_horizontal(DatePickerDefaults::HORIZONTAL_PADDING),
                    )
                    .arrangement(Arrangement::Start)
                    .build(ctx, |ctx| {
                        // `SwitchableDateEntryContent` picks between the two by the state's display
                        // mode (`DatePicker.kt:1432`). Before this, `set_display_mode(DisplayMode::Input)`
                        // stored a value nothing read and the modal picker stayed a calendar; the
                        // mode toggle in the header is what reaches it now.
                        if display_mode == DisplayMode::Input {
                            date_input_content(ctx, &state);
                            return;
                        }
                        let open = year_panel_open.get();
                        sync_month_pages(ctx, &state, &model, &month_rows, first_month, &month_step_in_flight);
                        months_navigation(
                            ctx,
                            &state,
                            &month,
                            open,
                            on_toggle_year_panel,
                            &colors,
                            &month_rows,
                            &month_step_in_flight,
                        );
                        // material3 overlays the year panel on the calendar and keeps the calendar composed
                        // underneath (`DatePicker.kt:1596-1617`): a `Box` whose first child is the weekday row
                        // plus the month grid, and whose second is `AnimatedVisibility(clipToBounds, enter =
                        // expandVertically + fadeIn(initialAlpha = 0.6f), exit = shrinkVertically + fadeOut)`.
                        // winia used to swap the calendar out instead, which looked the same only because the
                        // panel is exactly as tall as what it covers — and carried no motion at all.
                        //
                        // The shape is built in BOTH states, not just while the panel is open: a structure that
                        // appears and disappears with the toggle would rebuild the calendar's slots (and its
                        // `remember`ed month rows) on every open, which is also why `AnimatedVisibility` owns
                        // the open/closed half of this.
                        //
                        // The spec: Compose reads motion-scheme tokens (`DefaultEffects` for the expand and the
                        // fade in, `FastEffects` for the fade out); winia has no motion scheme, so both sides
                        // run one spring — the same substitution the rest of the crate makes.
                        Stack::new()
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |ctx| {
                                // A Column, not bare siblings: the Stack overlaps what it holds, and the
                                // weekday row has to stay above the grid it belongs to.
                                Column::new()
                                    .modifier(Modifier::new().fill_max_width().test_tag(CALENDAR_PANEL_TAG))
                                    .arrangement(Arrangement::Start)
                                    .build(ctx, |ctx| {
                                        weekday_row(ctx, &model, &colors);
                                        // The modal picker leaves the neighbouring month's slots empty, as material3
                                        // does and as the M3 specs' modal anatomy has no state for.
                                        month_pages(ctx, &state, &model, &colors, &month_rows, false);
                                    });
                                AnimatedVisibility::new(year_panel_open.clone())
                                    .enter(
                                        VisibilityTransition::fade_in(AnimationSpec::Spring(
                                            crate::animation::SpringSpec::default(),
                                        ))
                                        .with_alpha_from(YEAR_PANEL_INITIAL_ALPHA)
                                        .with_expand(),
                                    )
                                    .exit(
                                        VisibilityTransition::fade_out(AnimationSpec::Spring(
                                            crate::animation::SpringSpec::default(),
                                        ))
                                        .with_expand(),
                                    )
                                    .build(ctx, |ctx| {
                                        year_panel(ctx, &state, &model, &colors, &year_rows, on_year_selected);
                                    });
                            });
                    });
            });
    }
}

/// The paged month list (material3's `HorizontalMonthsList`, `DatePicker.kt:1700-1761`): one page per
/// month across the whole year range — `(last - first + 1) * 12`, 2412 for the default range — so the
/// calendar can be swiped. material3's items are `Box(fillParentMaxWidth())`, which winia reaches with
/// [`crate::layout::LazyRow::fill_items`], and its `rememberSnapFlingBehavior` with
/// [`crate::layout::LazyRow::snap_paging`], so a fast swipe settles on a whole month.
///
/// `show_outside_month` is what separates the two variants: the docked picker draws the neighbouring
/// month's days in the padding cells, the modal one leaves them empty (see [`MonthGrid`]).
fn month_pages(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    colors: &DatePickerColors,
    list: &LazyListState,
    show_outside_month: bool,
) {
    let range = state.year_range();
    let first_month = model.month_of(*range.start(), 1).start_utc_time_millis;
    let page_count = CalendarModel::number_of_months_in_range(&range) as usize;
    let today = state.today_millis();
    let selected = state.selected_date_millis();
    let selectable = state.selectable_dates();
    let list_state = list.clone();
    let list_model = model.clone();
    let list_state_for_pages = state.clone();
    let list_colors = colors.clone();

    LazyRow::new()
        .state(list_state)
        .fill_items(true)
        .snap_paging(true)
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(DatePickerDefaults::MONTH_HEIGHT),
        )
        .items(page_count, |index: usize| index as u64, move |ctx, index| {
            let month = list_model.plus_months(first_month, index as i64);
            let grid = MonthGrid::of(month, selected, today, selectable.as_ref());
            month_grid(ctx, &list_state_for_pages, &list_model, &grid, &list_colors, show_outside_month);
        })
        .build(ctx);
}

/// How long [`sync_month_pages`] will wait on something before it stops waiting: for a jump it issued
/// to be picked up, or for a scroll in flight to finish first.
///
/// ~1 s at 60 Hz, longer than any settle or spring. It is a module-level constant rather than a local
/// so the test can name the same number the code uses — see
/// `a_stuck_scroll_flag_cannot_freeze_the_month_sync_forever`, which is the guard on this bound.
pub(super) const MAX_WAIT_FRAMES: u8 = 60;

/// material3's two-way sync between the page list and `displayedMonthMillis`, in one place:
/// `LaunchedEffect(monthIndex)` scrolls the list when the month changes from OUTSIDE it
/// (`DatePicker.kt:1544-1554`), and `snapshotFlow { firstVisibleItemIndex }` writes the list's page
/// back into the month (`DatePicker.kt:1763-1779`).
///
/// There is no separate "current page" state: the list's position IS the displayed month, and the
/// arrows animate the list rather than writing the month themselves. That is what keeps a swipe and
/// an arrow press indistinguishable, and what lets arrow enablement come from the list
/// (`canScrollForward` / `canScrollBackward`) instead of being recomputed from the year range.
///
/// The one piece of memory is `published` — the month THIS function last wrote into the state. That is
/// what makes the two directions distinguishable, and it has to be that rather than "did the month
/// change since last frame": `displayed_month_millis` is read at the top of `build`
/// (`DatePicker::build`), so on the frame after a write it still carries the value from BEFORE it, one
/// frame behind the list. Comparing against a lagging value makes each direction react to the other's
/// past and the two chase each other — measured on a month arrow, the picker ping-ponged 1500 ⇄ 1520
/// forever, one `scroll_to_item` per frame, each cancelling the animation the last one started.
///
/// `pending` holds the page an outside change asked for, so the list side stays quiet until the list
/// actually arrives there instead of publishing the page it is still leaving.
pub(super) fn sync_month_pages(
    ctx: &mut ComposeCtx,
    state: &DatePickerState,
    model: &CalendarModel,
    list: &LazyListState,
    first_month: i64,
    step_in_flight: &State<Option<usize>>,
) {
    let range = state.year_range();
    let page_of = |month_millis: i64| {
        model
            .month_of_millis(month_millis)
            .index_in(&range)
            .max(0) as usize
    };
    let month_of = |page: usize| model.plus_months(first_month, page as i64).start_utc_time_millis;

    let published = ctx.remember(|| i64::MIN);
    let pending = ctx.remember(|| None::<usize>);
    // Consecutive frames spent WAITING — either for a jump this function issued to be picked up, or for
    // a scroll in flight to finish before the next one.
    //
    // Both waits are bounded, and that is not defensive padding. `is_scrolling()` is the same state the
    // drag, fling, programmatic-jump and cancellation paths all write
    // (`lazy_column.rs:997,1028` hand it to the list's `ScrollState`), and two of those can leave it set:
    // `cancel_animation_by_id` and `clear_animations_for_states` drop an animation with `retain`
    // (`animation.rs:580-583`, `:43-49`), never running its finish callback, and both `drag_scroll_up`
    // paths return before their reset when the drag's target node has gone (`app.rs:3021`,
    // `app.rs:3936`). Compose's guard is safe because a single owner maintains that flag with a
    // guaranteed reset; here it is shared, so an UNBOUNDED wait would turn a leaked flag into a calendar
    // frozen on a stale month for the life of the window — strictly worse than the gesture cancellation
    // the guard exists to prevent.
    // `remember` hands back a `State<T>`, so this slot holds the count itself. Writing it every frame is
    // free: `set_reactive` compares before notifying (`state.rs:390-393`), so re-setting the same count
    // dirties nothing and cannot loop the composition.
    let waited = ctx.remember(|| 0u8);

    let displayed = state.displayed_month_millis();
    let actual = list.first_visible();

    match pending.get() {
        // An outside change is still being carried out. Say nothing until the list arrives; the page
        // it is leaving is not news. Past the bound, give up on the arrival instead: falling through
        // lands in the branch below, which republishes wherever the list actually is, so a target the
        // list can never reach costs a momentary disagreement rather than a dead sync.
        Some(target) if actual != target => {
            if waited.get() < MAX_WAIT_FRAMES {
                waited.set(waited.get() + 1);
                return;
            }
            pending.set(None);
        }
        Some(_) => {
            pending.set(None);
            waited.set(0);
        }
        None => {}
    }

    if displayed != published.get() {
        // The month holds something this function did not put there, so it came from outside — the
        // year panel, or a caller writing the month. Take the list to it.
        let target = page_of(displayed);
        if actual == target {
            published.set(displayed);
            waited.set(0);
            // An outside change moved the list; whatever an arrow had asked for is superseded.
            step_in_flight.set(None);
        } else if list.is_scrolling() && waited.get() < MAX_WAIT_FRAMES {
            // A drag or a fling is still running, so this has to wait. Compose guards the same way and
            // for the same reason — "The DatePicker has other actions that can trigger a scroll and
            // update the displayedMonthMillis as they do so, hence we check here for isScrollInProgress
            // and only scroll to the monthIndex when there is none in progress"
            // (`DatePicker.kt:1545-1547`, guard at `:1548-1553`). Without it `scroll_to_item` calls
            // `cancel_animation`, so the jump would land by killing the gesture the user was in the
            // middle of. Leaving `published` alone is what makes this retry: `displayed != published`
            // still holds next frame.
            waited.set(waited.get() + 1);
        } else {
            list.scroll_to_item(target, 0.0);
            pending.set(Some(target));
            published.set(displayed);
            waited.set(0);
        }
        return;
    }
    waited.set(0);

    // Nothing came from outside, so any difference is the LIST having moved — a swipe, or an arrow's
    // animation partway through. Publish where it is so the nav row and the headline follow.
    let landed = month_of(actual);
    if landed != displayed {
        state.set_displayed_month_millis(landed);
        published.set(landed);
    }
    // The arrows' in-flight page is released once the list actually stands on it: from then on the
    // anchor names the same page, so the two agree and the memory has nothing left to add. It is also
    // released by any motion the arrows did not ask for — a swipe, or an outside jump — so a later
    // press steps from where the list really is rather than from a request the user has overridden.
    if step_in_flight.get() == Some(actual) {
        step_in_flight.set(None);
    }
}

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
    state: DatePickerState,
    visible: bool,
    on_dismiss_request: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    confirm_button: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    content: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    modifier: Modifier,
    shape: Option<Shape>,
    colors: Option<DatePickerColors>,
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
fn header(
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
fn months_navigation(
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
fn step_arrow(
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
pub(super) fn arrow_target(anchor: usize, in_flight: Option<usize>, forward: bool, last_page: usize) -> usize {
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
fn year_menu_button(
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
pub(super) fn year_panel_first_row(state: &DatePickerState, model: &CalendarModel) -> usize {
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
fn year_panel(
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
fn year_cell(
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
fn weekday_row(ctx: &mut ComposeCtx, model: &CalendarModel, colors: &DatePickerColors) {
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
fn month_grid(
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
const DAY_POP_MILLIS: u64 = 220;
#[derive(Debug)]
struct DayCircleNode {
    progress: State<f32>,
    color: Color,
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
fn day_cell(
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
enum DockedPanel {
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
    state: DatePickerState,
    on_confirm: Option<Arc<dyn Fn() + Send + Sync>>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    modifier: Modifier,
    /// A tag for a readout node reporting the pager's derived anchor, or `None`.
    ///
    /// Diagnostic only, and behind a caller-supplied tag rather than always on: the anchor
    /// (`LazyListState::first_visible`) is written back inside the measure every frame and is not part
    /// of what gets drawn, so a probe outside the process cannot read it — the tree shows positions,
    /// which is the pixel offset the anchor is derived FROM. The month arrows compute their target as
    /// `anchor ± 1`, so anything that goes wrong asymmetrically between the two directions lives here,
    /// and this is the only way to see it.
    page_probe: Option<String>,
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
fn docked_navigation(
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
fn month_list(
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
fn docked_action_row(
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
