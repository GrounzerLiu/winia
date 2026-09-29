# Date pickers

winia's alignment target is material3's date picker family. The M3 spec page describes three variants; the
local androidx sources are the implementation truth for all of them, and every number in this document comes
from those sources rather than from the spec page's measurement images.

## Sources

| What | Where |
| --- | --- |
| Spec: elements, attributes and tokens per variant | <https://m3.material.io/components/date-pickers/specs> |
| Spec: the three variants, differences from M2 | <https://m3.material.io/components/date-pickers/overview> |
| Docked and modal calendar, the state classes, header, month picker, year picker | `target/compose-src/commonMain/androidx/compose/material3/DatePicker.kt` |
| The modal container | `target/compose-src/commonMain/androidx/compose/material3/DatePickerDialog.kt` |
| Modal date input | `target/compose-src/commonMain/androidx/compose/material3/DateInput.kt` |
| Range variants | `target/compose-src/commonMain/androidx/compose/material3/DateRangePicker.kt` and `DateRangeInput.kt` |
| Locale and date formatting | `target/compose-src/commonMain/androidx/compose/material3/CalendarLocale.kt` |
| Container tokens | `target/compose-src/commonMain/androidx/compose/material3/tokens/DatePickerModalTokens.kt`, `.../tokens/DateInputModalTokens.kt` |

The spec page's images carry the measurement callouts; the token files carry the same values in code, so the
numbers below are quoted from the token files.

## The three variants

Element counts and configuration lists are the spec page's own.

- **Docked date picker** — 11 elements. Opens from an onscreen input, like a text field; used in forms.
  Configurations: day selection, month selection, year selection.
- **Modal date picker** — 13 elements. Extends full screen; used for selecting a date range.
  Configurations: single date selection, date range selection, year selection.
- **Modal date input** — 8 elements: headline, supporting text, header, container, icon button, outlined text
  field, text buttons, divider. Configurations: single date input, date range input.
- **Element states** — every date and year element has default (enabled), disabled, hovered, focused and
  pressed (ripple).

## Container tokens

`DatePickerModalTokens` (`target/compose-src/commonMain/androidx/compose/material3/tokens/DatePickerModalTokens.kt`),
shared by the docked and modal calendars:

| Token | Value |
| --- | --- |
| `ContainerColor` | `SurfaceContainerHigh` |
| `ContainerElevation` | `Level3` |
| `ContainerWidth` | 360.0 |
| `ContainerHeight` | 568.0 |
| `ContainerShape` | `CornerExtraLarge` |
| `HeaderContainerWidth` | 360.0 |
| `HeaderContainerHeight` | 120.0 |
| `HeaderHeadlineColor` / `Font` | `OnSurfaceVariant` / `HeadlineLarge` |
| `HeaderSupportingTextColor` / `Font` | `OnSurfaceVariant` / `LabelLarge` |
| `DateContainerWidth` / `Height` / `Shape` | 40.0 / 40.0 / `CornerFull` |
| `DateLabelTextFont` | `BodyLarge` |
| `DateSelectedContainerColor` / `DateSelectedLabelTextColor` | `Primary` / `OnPrimary` |
| `DateStateLayerWidth` / `Height` / `Shape` | 40.0 / 40.0 / `CornerFull` |
| `DateTodayContainerOutlineColor` / `Width` | `Primary` / 1.0 |
| `DateTodayLabelTextColor` | `Primary` |
| `DateUnselectedLabelTextColor` | `OnSurface` |
| `WeekdaysLabelTextColor` / `Font` | `OnSurface` / `BodyLarge` |
| `SelectionYearContainerWidth` / `Height` | 72.0 / 36.0 |
| `SelectionYearLabelTextFont` | `BodyLarge` |
| `SelectionYearSelectedContainerColor` / `LabelTextColor` | `Primary` / `OnPrimary` |
| `SelectionYearStateLayerWidth` / `Height` / `Shape` | 72.0 / 36.0 / `CornerFull` |
| `SelectionYearUnselectedLabelTextColor` | `OnSurfaceVariant` |
| `RangeSelectionActiveIndicatorContainerColor` / `Height` / `Shape` | `SecondaryContainer` / 40.0 / `CornerFull` |
| `RangeSelectionContainerElevation` / `Shape` | `Level0` / `CornerNone` |
| `SelectionDateInRangeLabelTextColor` | `OnSecondaryContainer` |
| `RangeSelectionHeaderContainerHeight` | 128.0 |
| `RangeSelectionHeaderHeadlineFont` | `TitleLarge` |
| `RangeSelectionMonthSubheadColor` / `Font` | `OnSurfaceVariant` / `TitleSmall` |

`DateInputModalTokens` (`.../tokens/DateInputModalTokens.kt`) is the spec's own token set for the modal date
input, and **nothing consumes it**: a grep over `target/compose-src` finds no reference to it outside its own
file. The realised modal date input is a `DatePicker` in `DisplayMode.Input` inside `DatePickerDialog`, so it
actually runs on `DatePickerModalTokens` (360 × 568) and its own header tokens (120 high, `HeadlineLarge` +
`LabelLarge`). The values below are therefore spec-only — mirrored for fidelity, not read by any code path:

| Token | Value |
| --- | --- |
| `ContainerColor` | `Surface` |
| `ContainerElevation` | `Level3` |
| `ContainerSurfaceTintLayerColor` | `SurfaceTint` |
| `ContainerWidth` | 328.0 |
| `ContainerHeight` | 512.0 |
| `ContainerShape` | `CornerExtraLarge` |
| `HeaderContainerWidth` | 328.0 |
| `HeaderContainerHeight` | 120.0 |
| `HeaderHeadlineColor` / `Font` | `OnSurfaceVariant` / `HeadlineLarge` |
| `HeaderSupportingTextColor` / `Font` | `OnSurfaceVariant` / `LabelLarge` |

## The calendar's public surface

Every anchor below is in `target/compose-src/commonMain/androidx/compose/material3/DatePicker.kt` unless the
path says otherwise.

| API | Shape |
| --- | --- |
| `DatePicker` (`:168`) | `DatePicker(state: DatePickerState, modifier: Modifier = Modifier, dateFormatter: DatePickerFormatter = remember { DatePickerDefaults.dateFormatter() }, colors: DatePickerColors = DatePickerDefaults.colors(), title: (@Composable () -> Unit)? = { DatePickerDefaults.DatePickerTitle(...) }, headline: (@Composable () -> Unit)? = { DatePickerDefaults.DatePickerHeadline(...) }, showModeToggle: Boolean = true, focusRequester: FocusRequester? = remember { FocusRequester() })` |
| `DatePickerState` (interface, `:244`) | `var selectedDateMillis: Long?`, `var displayedMonthMillis: Long`, `var displayMode: DisplayMode`, `val yearRange: IntRange`, `val selectableDates: SelectableDates`, `val locale: CalendarLocale` — every millis value is the **start of the day in UTC** |
| `SelectableDates` (`:286`) | `fun isSelectableDate(utcTimeMillis: Long) = true`, `fun isSelectableYear(year: Int) = true` |
| `DatePickerFormatter` (`:302`) | `fun formatMonthYear(monthMillis: Long?, locale: CalendarLocale): String?` (`:311`), `fun formatDate(dateMillis: Long?, locale: CalendarLocale, forContentDescription: Boolean = false): String?` (`:322`) |
| `DisplayMode` (`:332`) | value class over `Int`: `Picker` (0), `Input` (1) |
| `rememberDatePickerState` (`:368`) | `initialSelectedDateMillis: Long? = null`, `initialDisplayedMonthMillis: Long? = initialSelectedDateMillis`, `yearRange: IntRange = DatePickerDefaults.YearRange`, `initialDisplayMode: DisplayMode = Picker`, `selectableDates: SelectableDates = DatePickerDefaults.AllDates`; the state is a `rememberSaveable` whose `selectableDates` is re-applied on every composition (`:386-389`) |
| `DatePickerState(locale, …)` (`:423`) | the same parameters plus an explicit `locale` |

`DatePickerDefaults` (`:442`): `colors()` / `colors(…)`, `dateFormatter(yearSelectionSkeleton = YearMonthSkeleton, selectedDateSkeleton = YearAbbrMonthDaySkeleton, selectedDateDescriptionSkeleton = YearMonthWeekdayDaySkeleton)` (`:628`), `DatePickerTitle(displayMode, modifier, contentColor)` → the `DatePickerTitle` / `DateInputTitle` string (`:646`), `DatePickerHeadline(selectedDateMillis, displayMode, dateFormatter, modifier, contentColor)` (`:679`, see below), `YearRange = IntRange(1900, 2100)` (`:764`), `TonalElevation = ElevationTokens.Level0` (`:767`), `shape` = `DatePickerModalTokens.ContainerShape.value` (`:771`), `AllDates` (`:774`).

The headline (`:679-729`): text is `dateFormatter.formatDate(selectedDateMillis, locale)` or the
`DatePickerHeadline` / `DateInputHeadline` string; `LiveRegionMode.Polite` plus a content description built by
`formatHeadlineDescription(<headline description string>, dateFormatter.formatDate(…, forContentDescription = true))`;
`maxLines = 1`.

## State machine

`DatePickerStateImpl` (`:1183`) holds `_selectedDate`, `_displayedMonthMillis`, `_displayMode`, `yearRange`,
`locale`, a `calendarModel = createCalendarModel(locale)` and a `selectableDates` that the `remember` wrapper
refreshes each composition.

- The initial selection is canonicalised through `calendarModel.getCanonicalDate(initialSelectedDateMillis)`
  and becomes `null` when its year is outside `yearRange` (`:1195-1205`).
- Setting `selectedDateMillis` canonicalises again and applies the same year test; a `null` clears it
  (`:1207-1216`).
- Setting `displayMode` (`:1226-1232`) recomputes `displayedMonthMillis = calendarModel.getMonth(selectedDateMillis).startUtcTimeMillis`
  when there is a selection, then stores the mode — that is what makes the picker open on the selected month.
- The state is saved as a list (`:1243-…`).

## Layout

| Element | Rule | Anchor |
| --- | --- | --- |
| Container | `Column` with `sizeIn(minWidth = DatePickerModalTokens.ContainerWidth)` (360), `semantics { isContainer = true }`, `background(colors.containerColor)` | `:1353-1364` |
| Header | `Column(fillMaxWidth)`; `defaultMinSize(minHeight)` is applied **only when a title is present**; `SpaceBetween`; title style is `DatePickerModalTokens.HeaderSupportingTextFont`, headline style `HeaderHeadlineFont` (passed in by `DatePicker`, `:217`) | `:1671-1698`, `:1680-1685` |
| Header row | headline takes `weight(1f)`, the mode toggle sits after it; `SpaceBetween` when both exist, `Start` / `End` otherwise; a `HorizontalDivider(colors.dividerColor)` follows **only when a title, headline or toggle is present** | `:1372-1395` |
| Mode toggle | `Icons.Filled.Edit` in picker mode, `Icons.Filled.DateRange` in input mode; content colour is `colors.headlineContentColor`; descriptions are the `DatePickerSwitchToInputMode` / `DatePickerSwitchToCalendarMode` strings | `:1402-1425` |
| Mode switch | `AnimatedContent` between calendar and input, `-48.dp` parallax, spatial/effects motion-scheme specs, `SizeTransform(clip = true)` | `:1432-1497` |
| Months navigation | `padding(horizontal = DatePickerHorizontalPadding)` = 12; `yearPickerText = dateFormatter.formatMonthYear(displayedMonthMillis, locale) ?: "-"`; next/previous call `animateScrollToItem(index ± 1)` and swallow `IllegalArgumentException` | `:1559-1595`, `:1569-1592` |
| Weekday row | `defaultMinSize(minHeight = RecommendedSizeForAccessibility)` = 48, `fillMaxWidth`, `SpaceEvenly`; labels `sizeIn(40, 40)` then `size(LocalMinimumInteractiveComponentSize)`, content description per label, `colors.weekdayContentColor`, `WeekdaysLabelTextFont` | `:1796-1827` |
| Month column | `requiredHeight(RecommendedSizeForAccessibility * MaxCalendarRows)` = 48 × rows, `SpaceEvenly`; `MaxCalendarRows` rows of `DaysInWeek` cells, each `Row(fillMaxWidth, SpaceEvenly, CenterVertically)`; leading cells before `month.daysFromStartOfWeekToFirstOfMonth` and trailing cells stay empty | `:1856-1871` |
| Day cell | `Surface(shape = DateContainerShape, selected, enabled, onClick)` with `border = 1 dp todayDateBorderColor` **only when today and not selected**; the box is `requiredSize(40, 40)` and centres the text; semantics `text = AnnotatedString(description)`, `role = Role.Button`, `mergeDescendants = true`; the text itself sets `clearAndSetSemantics {}` | `:2005-2056` |
| Months list | `LazyRow` of `numberOfMonthsInRange(yearRange) = (last - first + 1) * 12` months (`:1733`, `:1963`), each in `Box(fillParentMaxWidth())`, with snap fling and a `horizontalScrollAxisRange` semantics override so a screen reader does not scroll months | `:1722-1750` |
| Month paging | `snapshotFlow { firstVisibleItemIndex }` → `yearOffset = index / 12`, `month = index % 12 + 1`, `onDisplayedMonthChange(getMonth(yearRange.first + yearOffset, month).startUtcTimeMillis)` | `:1763-1779` |
| Year picker overlay | `AnimatedVisibility` over the month column, `clipToBounds`, expand/shrink plus fade; height is `RecommendedSizeForAccessibility * (MaxCalendarRows + 1) - DividerDefaults.Thickness`, `padding(horizontal = 12)`; a divider follows it; picking a year closes the overlay and scrolls to `(year - yearRange.first) * 12 + displayedMonth.month - 1` | `:1619-1665` |
| Constants | `RecommendedSizeForAccessibility = 48.dp`, `MonthYearHeight = 56.dp`, `DatePickerHorizontalPadding = 12.dp`, `DatePickerModeTogglePadding = PaddingValues(end = 12.dp, bottom = 12.dp)`, `DatePickerTitlePadding = PaddingValues(start = 24.dp, end = 12.dp, top = 16.dp)`, `DatePickerHeadlinePadding = PaddingValues(start = 24.dp, end = 12.dp, bottom = 12.dp)`, `YearsVerticalPadding = 16.dp` | `:2293-2301` |

## Colour roles

`DatePickerColors` (`:836-860`) carries: `containerColor`, `titleContentColor`, `headlineContentColor`,
`weekdayContentColor`, `subheadContentColor`, `navigationContentColor`, `yearContentColor`,
`disabledYearContentColor`, `currentYearContentColor`, `selectedYearContentColor`,
`disabledSelectedYearContentColor`, `selectedYearContainerColor`, `disabledSelectedYearContainerColor`,
`dayContentColor`, `disabledDayContentColor`, `selectedDayContentColor`, `disabledSelectedDayContentColor`,
`selectedDayContainerColor`, `disabledSelectedDayContainerColor`, `todayContentColor`, `todayDateBorderColor`,
`dayInSelectionRangeContainerColor`, `dayInSelectionRangeContentColor`, `dividerColor`, and
`dateTextFieldColors` (the input mode's text field).

## Day content description

`dayContentDescription(rangeSelectionEnabled, isToday, isStartDate, isEndDate, isInRange)` (`:1967-1990`) joins,
comma-separated: the range words (only when range selection is on) and then `DatePickerTodayDescription` when
the day is today; `null` when nothing applies. The range words come from `DateRangePickerStartHeadline`,
`DateRangePickerEndHeadline` and `DateRangePickerDayInRange`.

## What the local read settled

- **The state setters coerce, they do not throw.** `selectedDateMillis = …` canonicalises the timestamp and
  stores `null` when the year is outside `yearRange` (`DatePicker.kt:1209-1218`); `displayedMonthMillis = …`
  *drops* the write when the month's year is outside the range (`:1153-1159`); `DateRangePickerState.setSelection`
  clears both dates when the range is inverted (`DateRangePicker.kt:619-638`). All three are documented as
  `@throws IllegalArgumentException` (`DatePicker.kt:250-251`, `:259-261`, `:419-420`). The code is the ground
  truth; winia follows the code and cites the disagreement.
- **`plusMonths` ignores a non-positive count** in the Android implementation — it returns its input
  (`internal/CalendarModelImpl.android.kt:118-120`). The month list only ever adds, so this is unreachable from
  the UI; winia computes the real month in both directions (a deliberate deviation).
- **The dialog** (`androidMain/.../DatePickerDialog.android.kt`): `requiredWidth(360)` (`:83`),
  `heightIn(max = 568)` (`:84`), `shape = DatePickerDefaults.shape` = `CornerSize(28)` (`:85`),
  `tonalElevation = 0` (`:87`), `Column(Arrangement.SpaceBetween)` (`:89`), the content in
  `Box(Modifier.weight(1f, fill = false))` (`:95`), the buttons in `Box(Modifier.align(Alignment.End))` with
  `DialogButtonsPadding = PaddingValues(bottom = 8, end = 6)` (`:97`, `:116`) laid out by `AlertDialogFlowRow`
  (main axis 8, cross axis 12, `:102-118`), and `DialogProperties(usePlatformDefaultWidth = false)` by default
  (`DatePickerDialog.kt:59`). The dialog owns no state: it invokes `dismissButton` and `confirmButton` and
  forwards `onDismissRequest` to the underlying alert dialog (`:106-107`, `:77`).
- **Icons** (`internal/Icons.kt`): the mode toggle is `Filled.Edit` → `Filled.DateRange` (`:139`, `:169`,
  `DatePicker.kt:1413/1420`), the month arrows are `AutoMirrored.Filled.KeyboardArrowLeft/Right` (`:34`, `:60`),
  and the year menu button is `Filled.ArrowDropDown` (`:226`).
- **Disabled colours** are the enabled role with `DisabledAlpha = 0.38f` (`ColorScheme.kt:1518`), applied to
  every `disabled*` member of `DatePickerColors` (`DatePicker.kt:564-593`).
- **The English prose is not in this checkout.** Strings are resource ids (`m3c_date_picker_*`,
  `androidMain/.../internal/Strings.android.kt:86-176`); the `res/values/strings.xml` values are absent, so the
  exact wording is unverifiable locally and winia supplies its own literals. Which id is used where is exact:
  see the report's per-anchor table (`DatePickerSwitchToInputMode`, `…CalendarMode`, `…PreviousMonth`,
  `…NextMonth`, `…DaySelection`, `…YearSelection`, `DatePickerYearPickerPaneTitle`,
  `DatePickerNavigateToYearDescription`, `DatePickerTodayDescription`, `DatePickerTitle`, `DateInputTitle`,
  `DatePickerHeadline`, `DateInputHeadline`, `…NoSelectionDescription`, `…NoInputDescription`,
  `DateRangePickerStartHeadline`, `DateRangePickerEndHeadline`, `DateRangePickerDayInRange`,
  `DateInputInvalidForPattern`, `DateInputInvalidYearRange`, `DateInputInvalidNotAllowed`, `DateInputLabel`).
- **Platform seams to replace** (`expect`/`actual`): `DatePickerDialog`, `CalendarLocale`, `defaultLocale()`,
  `Int.toLocalString()`, `createCalendarModel()`, `formatWithSkeleton()`, `Strings`/`getString`/`formatString`,
  `formatHeadlineDescription`, `formatDatePickerNavigateToYearString`. Behind them: `java.time`, `WeekFields`,
  `android.icu.text.DateFormat` / `android.text.format.DateFormat.getBestDateTimePattern`, and `java.text`
  (`SimpleDateFormat`, `DateFormatSymbols`, `NumberFormat`). winia replaces the lot with the date arithmetic in
  `winia/src/ui/date_picker.rs` plus a caller-supplied locale and formatter.
- **Not mirrored on purpose**: `DatePickerColors.equals`/`hashCode` ignore `navigationContentColor`,
  `dividerColor` and `dateTextFieldColors` (`DatePicker.kt:1045-1102`) — an upstream omission, not behaviour.

## winia status

`winia/src/ui/date_picker.rs` holds the calendar model the rest of the component needs:
`CalendarDate` (proleptic Gregorian, `days_from_civil`/`civil_from_days`, day-of-week in `java.time`'s
Monday-is-1 numbering), `CalendarMonth` (length, `days_from_start_of_week_to_first_of_month`,
`start_utc_time_millis`, `end_utc_time_millis`, `index_in`), `CalendarLocale` (weekday and month names plus the
first day of the week, since winia has no locale database), and `CalendarModel` (`month_of_millis`, `month_of`,
`plus_months`, `canonical_date`, `canonical_millis`, `format_month_year`, `format_date`,
`number_of_months_in_range`).

Measured, each by turning the rule off and watching the specific test fail:

| Rule | Turned off | Result |
| --- | --- | --- |
| 1970-01-01 is Thursday (`+3` in the weekday anchor) | anchor moved to `+4` | 3 tests red: `the_epoch_is_a_thursday`, `the_month_offset_counts_from_the_locales_first_day`, `dates_format_as_the_header_and_the_field_show_them`; the arithmetic-only tests stayed green |
| the month offset wraps behind the locale's first day | dropped the `+ 7` wrap | exactly 1 test red: `the_month_offset_counts_from_the_locales_first_day` |
| both millisecond setters test the year range | filter and guard removed | exactly 2 tests red: `a_selection_outside_the_year_range_is_dropped`, `showing_a_month_outside_the_year_range_is_ignored` |
| switching mode snaps the calendar to the selection's month | snap removed | exactly 1 test red: `switching_mode_pulls_the_calendar_to_the_selected_month` |
| the cells outside a month are empty, and an unselectable year disables all of its dates | both guards removed together | exactly their 2 tests red: `the_cells_outside_a_month_are_empty`, `an_unselectable_day_or_year_disables_cells` (4 failed in that run, the other 2 from the row-count probe below) |
| the grid is six rows tall | `MAX_CALENDAR_ROWS` set to 5 | 2 tests red: `a_month_grid_is_always_six_rows_of_seven`, and `today_and_the_selection_are_flagged_on_their_own_cells` because a five-row grid cuts a 30-day month short |
| a disabled day that is also today | today's role kept instead of the disabled day role | exactly 1 test red: `a_day_label_follows_material3s_precedence` |
| the two month arrows | the chevron constants swapped | exactly 1 test red: `the_month_arrows_draw_mirrored_chevrons` |

`DatePickerState` (`with`/`new` over a `DatePickerStateInit`, `remember_date_picker_state` for composition) then
carries the hoisted values material3's pickers read and drive: `selected_date_millis`,
`displayed_month_millis`, `display_mode`, `year_range`, `locale`, the `calendar_model` the state was built
with, and the caller's `SelectableDates` (the default `AllDates` allows everything). The coercion rules above
are the state's, and `today_millis` on the model is the system clock so tests can inject a fixed date.

`MonthGrid` then lays a month out the way the picker draws it: `MAX_CALENDAR_ROWS` (6) rows of `DAYS_IN_WEEK`
(7) cells, the cells before the 1st and after the last day empty, each day cell carrying its millis (month
start + day − 1 days), and the `is_today` / `is_selected` / `is_enabled` flags — the last one consulting
`SelectableDates` for the day *and* its year, because material3 disables every date of an unselectable year.
`day_content_description` assembles what a cell announces, today's word first.

`DatePicker` draws the docked variant: a `Column` at least `CONTAINER_WIDTH` (360) wide on
`surface_container_high`, a header of the title (`LabelLarge`, `OnSurfaceVariant`) over the headline
(`HeadlineLarge`, `OnSurfaceVariant`, one line) with the divider below them, and a body of the month
navigation (56 high, a chevron either side, each arrow enabled while the month has a neighbour inside the year
range), the weekday row (48 high, 48-wide cells, narrow names) and the 6 × 7 grid of 40 dp circular day
`Surface`s — today outlined 1 dp in `Primary` unless it is selected, a selected day filled with `Primary`.
`DatePickerDefaults` carries every measurement with its token anchor, and `DatePickerColors` resolves the roles
from the theme, including the one material3 hardcodes for navigation (`DatePicker.kt:559`).

Deliberate deviations: the picker composes one month at a time (winia has no lazy row, so material3's
`LazyRow` of 2412 months with its snap fling is out, and the arrows step a month); the mode toggle and the
input body arrive with the input mode; the picker takes no `DatePickerColors` parameter yet and reads the
theme. The chevron path data is the Material Icons 24 dp artwork, which this sandbox cannot byte-verify
against Google's assets — the same limitation the split button demo's `add` glyph carries (`web_fetch` refuses
non-public hosts and the Tavily extracts of the raw and jsdelivr SVGs came back empty) — so
`the_month_arrows_draw_mirrored_chevrons` measures the published data instead: each glyph inks 26–31 of the
576 pixels in its 24 dp box, the left one's ink centre sits left of the right one's, and the pair is a near
mirror (17 of 576 pixels differ).

Four UI tests drive the fixture (`winia/tests/ui_test.rs`), three of them green: the container keeps its 360 dp
width and the sum of its rows (measured `(16, 78, 360, 512)` against 120 + 1 + 56 + 48 + 288), the selected day
is a filled 40 dp circle (measured 39 dp across on a scan through its centre) with today a hollow ring 40 dp
across, and the arrows step the month and step back.

The fourth — a tap on a day cell — is ignored, and the reproduction stays in the tree rather than a passing
assertion that would hide it. Measured: the arrows do take a tap, but nothing inside the month grid does. A
temporary `test_tag` on the day cell put the first one at `(32, 306, 40, 40)`, and a tap on that centre (and on
the painted today cell, and 14 dp above its label) leaves the selection unchanged. A clickable attached to the
picker's *container*, whose box covers the whole grid, does not fire for taps inside the grid either, so the
press is consumed there and never reaches a handler; `UiTest::tap` and the synthetic `UiTest::click` behave the
same. Isolating a lone `Surface::selectable` in a fixture is the next step: that decides between winia's
`Surface` interaction and this grid's nesting of `Stack` and `Row`.

Next: that isolation, the year picker panel (3 columns, 72 × 36 cells), then `DatePickerDialog` and the input
mode.
