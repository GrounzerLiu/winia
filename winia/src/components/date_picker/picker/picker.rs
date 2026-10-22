use super::*;

pub struct DatePicker {
    pub(crate) state: DatePickerState,
    pub(crate) title: Option<String>,
    /// Whether `title` is still material3's default. Compose hands the title a lambda that names the
    /// mode (`title = { if (displayMode == Input) DateInputTitle else DatePickerTitle }`,
    /// `DatePicker.kt:654`), so the default follows the mode while a caller's own title does not.
    pub(crate) title_is_default: bool,
    /// The colour roles, read from the theme when absent. [`DatePickerDialog`] forwards its own set here
    /// for its default content — a winia convenience, since material3's dialog uses its `colors` only for
    /// its own surface (`DatePickerDialog.android.kt:86`) and invokes the caller's slot with nothing
    /// (`:95`).
    pub(crate) colors: Option<DatePickerColors>,
    pub(crate) modifier: Modifier,
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
pub(crate) fn month_pages(
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
pub(crate) const MAX_WAIT_FRAMES: u8 = 60;

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
pub(crate) fn sync_month_pages(
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

