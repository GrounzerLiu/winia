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
| Month column | `requiredHeight(RecommendedSizeForAccessibility * MaxCalendarRows)` = 48 × rows, `SpaceEvenly`; `MaxCalendarRows` rows of `DaysInWeek` cells, each `Row(fillMaxWidth, SpaceEvenly, CenterVertically)`; leading cells before `month.daysFromStartOfWeekToFirstOfMonth` and trailing cells stay empty — the modal picker does too; **the docked picker fills them, see "Outside-month days" below** | `:1856-1871` |
| Day cell | `Surface(shape = DateContainerShape, selected, enabled, onClick)` with `border = 1 dp todayDateBorderColor` **only when today and not selected**; the box is `requiredSize(40, 40)` and centres the text; semantics `text = AnnotatedString(description)`, `role = Role.Button`, `mergeDescendants = true`; the text itself sets `clearAndSetSemantics {}` | `:2005-2056` |
| Months list | `LazyRow` of `numberOfMonthsInRange(yearRange) = (last - first + 1) * 12` months (`:1733`, `:1963`), each in `Box(fillParentMaxWidth())`, with snap fling and a `horizontalScrollAxisRange` semantics override so a screen reader does not scroll months | `:1722-1750` |
| Month paging | `snapshotFlow { firstVisibleItemIndex }` → `yearOffset = index / 12`, `month = index % 12 + 1`, `onDisplayedMonthChange(getMonth(yearRange.first + yearOffset, month).startUtcTimeMillis)` | `:1763-1779` |
| Year picker overlay | `AnimatedVisibility` over the month column, `clipToBounds`, expand/shrink plus fade; height is `RecommendedSizeForAccessibility * (MaxCalendarRows + 1) - DividerDefaults.Thickness`, `padding(horizontal = 12)`; a divider follows it; picking a year closes the overlay and scrolls to `(year - yearRange.first) * 12 + displayedMonth.month - 1` | `:1619-1665` |
| Year menu button | `YearPickerMenuButton`: a `TextButton(shape = CircleShape, elevation = null, border = null)` holding the formatted month-year text and `Icons.Filled.ArrowDropDown` after `ButtonDefaults.IconSpacing` (8); the text repeats itself as the content description and is a polite live region; the month arrows are composed **only while the overlay is closed**, and the row's arrangement switches `SpaceBetween` → `Start` | `:2194-2269` |
| Year panel grid | `LazyVerticalGrid(GridCells.Fixed(YearsInRow = 3))`, `background(colors.containerColor)`, `SpaceEvenly` horizontally, `spacedBy(YearsVerticalPadding = 16)` vertically, `SelectionYearLabelTextFont` (BodyLarge); `initialFirstVisibleItemIndex = max(0, displayedYear - yearRange.first - YearsInRow)`; every year is `requiredSize(SelectionYearContainerWidth = 72, SelectionYearContainerHeight = 36)` | `:2061-2116`, `:2301-2304` |
| Year cell | `Surface(shape = SelectionYearStateLayerShape = CornerFull, selected, enabled, onClick)` with `border = 1 dp todayDateBorderColor` when it is the current year and not selected; container `yearContainerColor(selected, enabled)` (`Primary` when selected, otherwise transparent), label `yearContentColor(currentYear, selected, enabled)`; description is the `DatePickerNavigateToYearDescription` string | `:2120-2180`, `:1005-1046` |
| Modal dialog | `BasicAlertDialog(wrapContentHeight)` around a `Surface(requiredWidth(ContainerWidth = 360), heightIn(max = ContainerHeight = 568), shape = DatePickerDefaults.shape, color = colors.containerColor, tonalElevation = DatePickerDefaults.TonalElevation)`; the dialog contributes no padding — the picker is the surface | `DatePickerDialog.kt:51-61`, `DatePickerDialog.android.kt:85-94` |
| Modal body | `Column(verticalArrangement = SpaceBetween)`: the content in a `Box(weight(1f, fill = false))` — the `fill = false` is what lets the dialog collapse when the input mode is shorter — then the action row | `DatePickerDialog.android.kt:95-111` |
| Action row | `Box(align End, DialogButtonsPadding = PaddingValues(bottom = 8, end = 6))` holding an `AlertDialogFlowRow(mainAxisSpacing = 8, crossAxisSpacing = 12)` of the dismiss button then the confirm button, in `DialogTokens.ActionLabelTextColor` (`Primary`) and `ActionLabelTextFont` (LabelLarge) | `DatePickerDialog.android.kt:105-118` |
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

## RTL

Compose mirrors two independent things, and a picker needs both:

1. **Layout** — `Row` puts its first child at the *start* (the right edge under RTL), and
   `Arrangement.Start`/`End`, `paddingStart`/`paddingEnd` swap meaning with it. winia mirrors placements
   in `layout/flex.rs:302-303` and every container reads the ambient direction at compose time
   (`ui/layout_components.rs:110`), so this half needed no date picker work.
2. **Artwork** — only glyphs the caller marks auto-mirrored flip. material3's two month arrows are
   `Icons.AutoMirrored.Filled.KeyboardArrowLeft` / `…KeyboardArrowRight` (`DatePicker.kt:2225`, `:2232`),
   which is why the arrow keeps pointing outward in both directions; the year menu button's
   `Filled.ArrowDropDown` is not auto-mirrored, and is symmetric anyway.

Measured on the docked demo in RTL (picker content spans x = 36…372 in a 560 dp window), every part of the
picker mirrors correctly except the arrow artwork:

| Part | LTR | RTL | Correct? |
| --- | --- | --- | --- |
| Navigation group order | month x = 68, year x = 248 | month x = 256, year x = 68 | yes — `SpaceBetween` mirrors the pair |
| Arrow order inside a group | prev x = 40, next x = 156 | prev x = 344, next x = 228 | yes — `Row` mirrors the triple |
| Chevron artwork | prev `<`, next `>` | prev `<`, next `>` | **no** — both point inward |
| Weekday row | Sunday leftmost | Sunday x = 324 (rightmost), Saturday x = 36 | yes |
| Day grid | leading blanks left | blanks for Sun/Mon at x = 324/276, day 1 at x = 228 | yes |
| Action row | Cancel, OK | OK x = 61, Cancel x = 120 | yes — `Arrangement::End` mirrors to the left |
| Year menu button internals | text then `▾` | label x = 304 (right), `▾` x = 272 (left); 12 start pad on the right, 16 end pad on the left | yes |

The arrow failure is the one half winia did not do: `step_arrow` built its `Icon` without
`auto_mirror(true)`, so the layout carried the previous arrow to the right edge while the artwork stayed
put, and both arrows pointed at the label. `render.rs:513` applies the mirror
(`spec.auto_mirror && direction == Rtl`) and `Icon::auto_mirror` (`ui/icon.rs:606`) sets it, so the fix is
the flag on the arrow. After it: prev `>` at x = 344, next `<` at x = 228.

Two tests guard it, and neither uses an ink centroid — the chevron's centroid sits at ~11.5 either way
round (measured 11.516 under LTR, 11.484 under RTL), because the shape is near-symmetric about its own box,
so a centroid test cannot see the flip at all. `the_navigation_arrows_flip_their_artwork_under_rtl`
compares inked pixels against the *other* glyph through the real render pipeline: 17 of 576 pixels differ
under RTL against the next chevron's LTR rendering, 41 differ under LTR (17 is the figure
`the_month_arrows_draw_mirrored_chevrons` already records for this pair of paths).
`the_picker_composes_its_chevrons_as_auto_mirrored` composes the real `DockedDatePicker` and asserts the
`IconSpec` carries `auto_mirror`, so dropping the flag turns it red instead of silently passing.

### Open gaps next to the picker

Neither is the picker's own code, and both are recorded here because the docked picker's normal use puts
them directly on screen.

- **`TextField` has no direction handling at all** (`ui/text_field.rs`: no `LayoutDirection`, no
  `padding_start`/`padding_end`; the paddings are literal arithmetic). Measured: with the demo window in
  RTL, the surrounding text mirrors (x 24 → 60) while the field's `Date` label stays at x = 38 and its
  trailing icon at x = 268. Any caller that puts a `TextField` in an RTL window gets an LTR field.
- **`PopupPosition` is direction-blind** (`ui/overlay.rs:162`, applied at `app.rs:3667`). The enum names
  absolute corners, and the anchored branch is pure geometry — `BottomLeft => (ax, ay + ah)` — so an
  anchored popup aligns to the anchor's geometric left edge in both directions. There is no
  `BottomStart`/`TopStart`, which is what "follow the anchor's start edge" needs: this demo passes
  `PopupPosition::BottomLeft` and happens to look right only because its `TextField` does not mirror
  either. **Planned fix: add Start/End variants and migrate to them** — not to make the existing corners
  mirror, since a name that says "Left" should keep meaning left. It touches the shared placement path
  (`DropdownMenu`, `SearchBar` and `Tooltip` all pass a `PopupPosition`), so it is its own task;
  `docs/dropdown-menu.md` §4.15 carries the plan and the caller list.

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
| the cells outside a month are empty, and an unselectable year disables all of its dates | both guards removed together | exactly their 2 tests red: `the_cells_outside_a_month_are_empty`, `an_unselectable_day_or_year_disables_cells` (4 failed in that run, the other 2 from the row-count probe below). Superseded — the first is now `the_cells_outside_a_month_hold_the_neighbouring_month`, since winia fills those cells rather than leaving them empty |
| the grid is six rows tall | `MAX_CALENDAR_ROWS` set to 5 | 2 tests red: `a_month_grid_is_always_six_rows_of_seven`, and `today_and_the_selection_are_flagged_on_their_own_cells` because a five-row grid cuts a 30-day month short |
| a disabled day that is also today | today's role kept instead of the disabled day role | exactly 1 test red: `a_day_label_follows_material3s_precedence` |
| a year outside the year range | the guard dropped | exactly 1 test red: `a_year_outside_the_range_is_ignored` |
| the year panel is the calendar's height | `YEAR_PANEL_HEIGHT` set to the month alone | exactly 1 test red: `the_year_panel_is_as_tall_as_the_calendar_it_stands_in_for` |
| picking a year closes the panel | `year_panel_open.set(false)` dropped | exactly 1 test red: `date_picker_opens_its_year_panel_and_picks_a_year` (`picking a year closes the panel`) |
| the modal dialog adds no padding of its own | `.content_padding(0.0)` dropped, so winia's `BasicAlertDialog` inset its surface by the alert dialog's 24 dp | exactly 1 test red: `date_picker_dialog_is_the_modal_picker` — the picker inside is pushed 24 dp in and the tap on today misses its cell, which is the measured shape of that mistake |
| the two month arrows | the chevron constants swapped | exactly 1 test red: `the_month_arrows_draw_mirrored_chevrons` |

`DatePickerState` (`with`/`new` over a `DatePickerStateInit`, `remember_date_picker_state` for composition) then
carries the hoisted values material3's pickers read and drive: `selected_date_millis`,
`displayed_month_millis`, `display_mode`, `year_range`, `locale`, the `calendar_model` the state was built
with, and the caller's `SelectableDates` (the default `AllDates` allows everything). The coercion rules above
are the state's, and `today_millis` on the model is the system clock so tests can inject a fixed date.

`MonthGrid` then lays a month out the way the picker draws it: `MAX_CALENDAR_ROWS` (6) rows of `DAYS_IN_WEEK`
(7) cells, every cell carrying a day — the cells before the 1st and after the last day hold the neighbouring
month's days (see "Outside-month days" below; the docked picker draws them, the modal one does not) — each day
cell carrying its millis (month start + day − 1 days, a signed offset that walks across the month boundary on
its own), and the `is_today` / `is_selected` / `is_enabled` / `is_outside_month` flags — `is_enabled`
consulting `SelectableDates` for the day *and* its year, because material3 disables every date of an
unselectable year, and forced off for an outside cell.
`day_content_description` assembles what a cell announces, today's word first.

`DatePicker` draws the **modal** variant (the docked one is `DockedDatePicker`, which has no title and no
headline): a `Column` at least `CONTAINER_WIDTH` (360) wide on
`surface_container_high`, a header of the title (`LabelLarge`, `OnSurfaceVariant`) over the headline
(`HeadlineLarge`, `OnSurfaceVariant`, one line) with the divider below them, and a body of the month
navigation (56 high: the year menu button at its start and, while the year panel is closed, the two chevrons
together at its end, each arrow enabled while the month has a neighbour inside the year range), then either the
weekday row (48 high, 48-wide cells, narrow names) with the 6 × 7 grid of 40 dp circular day `Surface`s — today
outlined 1 dp in `Primary` unless it is selected, a selected day filled with `Primary` — or, while the panel is
open, the year list: rows of three 72 × 36 `Pill`-shaped `Surface`s (`SelectionYearStateLayerShape` is
`CornerFull`, and on that box the two are the same stadium), the displayed year filled with `Primary`, the
current year outlined, 16 dp between rows, over a divider.
`DatePickerDefaults` carries every measurement with its token anchor, and `DatePickerColors` resolves the roles
from the theme, including the one material3 hardcodes for navigation (`DatePicker.kt:559`).

Deliberate deviations: the **docked** picker's grid fills its leading and trailing slots with the
neighbouring month's days where material3 — and the M3 specs' modal anatomy — leave them empty (the one
place winia follows the docked specs against the Compose source, see "Outside-month days"); the mode toggle
and the input body arrive with the input mode; the picker takes no `DatePickerColors` parameter yet and reads
the theme.

One deviation that used to be recorded here was **wrong** and is gone: this file claimed winia composes one
month at a time because "winia has no lazy row", with the arrows stepping the month directly. winia has had
`LazyRow` (`LazyList<HorizontalAxis>`, `ui/lazy_column.rs`) all along. Both pickers now compose material3's
shape — a paged `LazyRow` over every month in the year range, with the arrows animating the list rather than
writing the month — see "Swiping between months".

The year panel carries the second deviation. material3 overlays it on the month calendar inside an
`AnimatedVisibility` (expand plus fade) and keeps the calendar composed underneath; winia swaps the calendar out,
which shows the same picture because the panel is exactly as tall as what it replaces (335 + 1 dp of divider
against the weekday row's 48 plus the grid's 288) and paints the picker's own container colour behind the years —
the difference is the missing animation. Its list is a `LazyColumn` of *row* items rather than a
`LazyVerticalGrid`, and it opens on `year_panel_first_row = max(0, displayedYear - yearRange.first) / 3 - 1`,
which is material3's `initialFirstVisibleItemIndex` converted from a cell index to a row. material3 also scrolls
its month list to the picked year and lets the list write the displayed month back; winia has no month list, so
`DatePickerState::set_displayed_year` writes that month directly, keeping the month of year. The year menu button
is a plain transparent `Pill` `Surface` holding the label and the dropdown glyph, where material3 starts from a
`TextButton` and clears its elevation and border; the 8 dp between text and glyph is
`ButtonSmallTokens.IconLabelSpace`.

The modal variant, `DatePickerDialog`, is that modal picker in a dialog: winia opens the centred modal overlay
`BasicAlertDialog` provides, with the surface's own geometry — `width(CONTAINER_WIDTH)` and
`max_height(MODAL_CONTAINER_HEIGHT)` on the wrapper, no content padding, shape 28 (`CONTAINER_CORNER`), filled
with `colors.container` — and the action row under the content (`Row` at `MODAL_BUTTONS_SPACING` 8, padded 8
bottom and 6 end, inside `WiniaTheme::with_content_color(Primary)` and `ProvideTextStyle(LabelLarge)`).
Measured on the fixture: the dialog is `360 × 560` — the picker's own content, not the cap. That number is
the 120 dp header, 56 dp month navigation, 48 dp weekday row and 288 dp month, then the action row's 40 dp
button under its 8 dp inset. winia applies `CONTAINER_HEIGHT` as a `max_height` cap rather than a fixed
height, so a shorter content column collapses instead of being padded out; the 568 the fixture used to report
was the cap holding, not the content reaching it, and the two were only told apart once the box below had a
size of its own to report. See "The dialog is its content" below. The content defaults to a `DatePicker` over the dialog's state, carrying the
dialog's own `DatePickerColors`, and `DatePickerDialog::content` replaces it.

That forwarding is a winia convenience rather than a copy of Compose's wiring, and it is worth being
precise about because the two are easy to confuse. material3's `DatePickerDialog` reads its `colors` in
exactly one place — `color = colors.containerColor` on its own `Surface` (`DatePickerDialog.android.kt:86`)
— and then invokes the caller's slot with nothing at all (`:95  Box(Modifier.weight(1f, fill = false)) {
this@Column.content() }`). A Compose caller nesting a `DatePicker` is expected to pass `colors` down
themselves; there is no threading to port. winia has no caller for its default content to be anyone but
itself, so the dialog does it, and `DatePicker::colors` exists for that. Either way the result was wrong
before: `DatePickerDialog::colors()` reached the surface and stopped, so an overridden dialog showed a
theme-coloured calendar inside a caller-coloured container.

Two deviations there. material3 puts the content in a `Box(weight(1f, fill = false))` so the dialog collapses
when the input mode is shorter than the calendar; winia has no weights, so the content and the action row follow
one another in the `Column` — the row still lands at the end, because the column is only as tall as its content.
And `AlertDialogFlowRow`'s `crossAxisSpacing` (12) only matters when the two buttons wrap onto two lines, which
winia's `Row` does not do; the row here is not a `FlowRow`.

That needed one change outside this component: winia's `BasicAlertDialog` carried the alert dialog's 24 dp of
content padding on its surface, where Compose's `BasicAlertDialog` has none — the padding belongs to
`AlertDialog`'s own content column. It is now `BasicAlertDialog::content_padding(padding)`, still 24 dp by
default for `AlertDialog`, and the date picker dialog passes `0.0`. The chevron path data is the Material Icons 24 dp artwork, which this sandbox cannot byte-verify
against Google's assets — the same limitation the split button demo's `add` glyph carries (`web_fetch` refuses
non-public hosts and the Tavily extracts of the raw and jsdelivr SVGs came back empty) — so
`the_month_arrows_draw_mirrored_chevrons` measures the published data instead: each glyph inks 26–31 of the
576 pixels in its 24 dp box, the left one's ink centre sits left of the right one's, and the pair is a near
mirror (17 of 576 pixels differ).

Five UI tests drive the fixture (`winia/tests/ui_test.rs`), all five green: the container keeps its 360 dp
width and the sum of its rows (measured `(16, 78, 360, 512)` against 120 + 1 + 56 + 48 + 288), the selected day
is a filled 40 dp circle (measured 39 dp across on a scan through its centre) with today a hollow ring 40 dp
across, the arrows step the month and step back, a tap on the today cell moves the selection to it, and the year
menu button opens the panel: the container's rectangle is unchanged while it is open, a year cell measures
72 × 36, and tapping the next year keeps the month (`month: September 2025`) and closes the panel.

## Swiping between months

Both pickers put the calendar in a paged `LazyRow` over every month in the year range — 2412 pages for the
default `1900..=2100` — exactly as material3's `HorizontalMonthsList` does (`DatePicker.kt:1700-1761`). The
weekday row stays OUTSIDE the list, so it does not scroll with the months (`DatePicker.kt:1597-1604`).

| material3 | winia |
| --- | --- |
| `LazyRow` of `numberOfMonthsInRange(yearRange)` items | `LazyRow::items(page_count, …)` |
| `Box(Modifier.fillParentMaxWidth())` per item | `LazyRow::fill_items(true)` |
| `rememberSnapFlingBehavior(lazyListState)` | `LazyRow::snap_paging(true)` |
| `firstVisibleItemIndex` is the current month | `LazyListState::first_visible()` |
| arrows run `animateScrollToItem(index ± 1)` | the same |
| `canScrollForward` / `canScrollBackward` enable the arrows (`:1561-1562`) | the same |

### The snap, and the three ways it was got wrong first

Compose's `SnapFlingBehavior` (`foundation/gestures/snapping/SnapFlingBehavior.kt`) is not "decay, then
settle". material3 hands it a layout provider whose `calculateApproachOffset` returns **0**
(`DatePicker.kt:749-753`), and with that offset at zero `tryApproach` returns WITHOUT animating
(`SnapFlingBehavior.kt:165-176`). So the whole motion is one animation:

1. `calculateSnapOffset(velocity)` picks the target (`LazyListSnapLayoutInfoProvider.kt:66-100`). Its
   candidates are the two snap positions bracketing the current offset, and
   `calculateFinalSnappingItem` (`:139-145`) chooses between them by velocity alone — the NEARER one below
   `MinFlingVelocityDp = 400.dp`, otherwise the one in the direction of travel.
   **One gesture therefore moves at most one page, however hard it was flicked.** That ceiling is the
   entire feel of a pager.
2. The snap animation runs on `animationState.copy(value = 0f)` (`:150-158`) — the value resets to the
   current offset but the FLING's velocity is carried in, so the gesture's momentum continues into the
   settle.
3. The spec is **not** a tween and **not** `StiffnessMediumLow`. material3 hands `snapFlingBehavior`
   `MotionSchemeKeyTokens.DefaultEffects` (`DatePicker.kt:744`), which resolves through
   `MotionScheme.kt:276` → `defaultEffectsSpec()` (`:152-156`) to
   `spring(dampingRatio = SpringDefaultEffectsDamping, stiffness = SpringDefaultEffectsStiffness)`, and
   `StandardMotionTokens.kt:22-23` puts those at **1.0 and 1600.0** (the expressive scheme is the same).
   winia uses those. It previously used foundation's default
   (`spring(stiffness = Spring.StiffnessMediumLow)`, `SnapFlingBehavior.kt:238`) while this document and
   the code comment attributed it to material3 — which is how the wrong spring survived review. The old
   local constant was 400, but that figure is this repo's prior value, not a quoted Compose fact: the
   numeric constant lives in `androidx.compose.animation.core`, which `target/compose-src` does not
   mirror.

Also unlike the decay path, **a release is never filtered out before the snap runs.** Compose calls
`performFling` on every release (`Scrollable.kt:857-881`) and always computes a snap offset from it.
winia had two velocity floors in the way — 50 px/s at the call site and 1 px/s inside `fling_with_boundary`
— both written for the decay, and both of which a snap has no use for since it has no decay phase to
suppress. A drag carried past halfway and then let go with the finger nearly still fell through both and
came to rest between two months permanently: nothing else ever snaps the list back. Both floors now step
aside when a `SnapSpec` is configured.

The first version of this did all three differently, and each was visible:

| wrong | what it did | what it looked like |
| --- | --- | --- |
| ran a free exponential decay first, then snapped to the nearest boundary of wherever it stopped | with 2412 pages a hard flick banked thousands of pixels of decay | a flick jumped most of a year, and the DIRECTION came from where the decay happened to run out rather than from the gesture |
| no velocity threshold | a nudge and a flick were the same rule | a small push could not settle back on the month it started from |
| a 300 ms tween from rest | the list stopped dead, then moved again | two visible motions — "not smooth" |
| the wrong spring: foundation's default instead of material3's 1600 | softer, settling visibly slower than the picker it was ported from | the flick arrived, then kept creeping |
| kept the decay's 50 px/s / 1 px/s release floors | a slow drag released at rest never entered the fling at all | the calendar rested between two months and stayed there |

Measured after the fix, driving the debug server with real pointer drags (a local working script —
see the note on probes below):

| gesture (276 dp across a 336 dp page) | months moved | label transitions |
| --- | --- | --- |
| slow drag, pointer left (forward) | +1 | 1 — settled |
| slow drag, pointer right (back) | −1 | settled |
| flick left (3 steps, same distance) | +1 | settled |
| flick right | −1 | settled |
| 20 dp nudge | 0 | settled back on the same page |
| drag 82% of a page, held 0.6 s, released | +1 | settled on the boundary |
| drag 36% of a page, held 0.6 s, released | 0 | settled back on its own page |

The last two are the release-at-rest case, and they are also the one pair with a **versioned** guard:
`modifier::tests::a_paged_list_settles_even_when_it_is_released_at_rest` covers the floor inside
`fling_with_boundary`, `app::release_velocity_floor_tests::a_paged_list_flings_below_the_decay_floor`
covers the 50 px/s floor at the call site (with `an_ordinary_container_keeps_the_decay_floor` as its
control), and `winia/src/app.rs` carries both.

**A note on the probes named in this document.** They live under `tmp/`, which this repository's
`.gitignore` excludes along with `tools/` — deliberately, since they are diagnostic scripts rather than
product code. So a fresh checkout cannot run them, and a reader should treat the numbers above as the
durable record and the probe names as provenance for anyone who still has the working tree. Where a
measured claim has a versioned guard, the guard is named beside it; that is the half a regression can be
caught by.

### The two-way sync, and why the frame lag matters

material3 has two independent effects: `LaunchedEffect(monthIndex)` scrolls the list when the month changes
from outside (`:1544-1554`), and `snapshotFlow { firstVisibleItemIndex }` writes the page back into the month
(`:1763-1779`). winia's `sync_month_pages` does both.

Its one piece of memory is the month **this function last published**, not "what the month was last frame".
That distinction is load-bearing: `displayed_month_millis` is read at the top of `build`, so on the frame
after a write it still carries the value from BEFORE it — one frame behind the list. Comparing against that
lagging value makes each direction react to the other's past.

Measured with a debug trace, that is exactly what happened: clicking a month arrow made the picker
ping-pong between two pages forever, one `scroll_to_item` per frame, each cancelling the animation the last
one had started — `page=1500 month_index=1520` then `page=1520 month_index=1500`, repeating. Remembering
what we published instead removed the feedback path, and the arrows now settle with a single transition.
The versioned guard on the outcome is `ui::date_picker::tests::a_stuck_scroll_flag_cannot_freeze_the_month_sync_forever`,
which pins the OTHER half of the same mechanism — that the wait the guard introduces is bounded, so a
leaked `is_scrolling` cannot turn into a frozen month.

`pending` is the second piece: the page an outside change asked for. The list side stays quiet until the
list actually arrives there, so a month picked in the year panel is not overwritten by the page the list is
still leaving.

There is no "current page" field anywhere. The list's position IS the displayed month, which is what makes a
swipe and an arrow press indistinguishable and lets arrow enablement come from the list rather than being
recomputed from the year range.

### Stepping from the page in flight, and the asymmetry that made it necessary

**A deliberate step past Compose.** In Compose every arrow press reads `firstVisibleItemIndex ± 1`
(`DatePicker.kt:1569-1592`), and that index is the page whose span still contains the pixel offset — so
*during a forward animation it names the page being left*. A press then asks for the page already in
flight; the request is a no-op retarget and the press is lost. material3 knows: both handlers are wrapped
in `catch (_: IllegalArgumentException)` with the comment "the user clicked the 'next' arrow fast while
the list was still animating" (`:1571-1587`).

The swallow is one press for one page, which would be fine. What is not is that it is **one-directional**.
Scrolling backward flips the anchor the moment the offset leaves the old page's span, so `anchor - 1`
names a genuinely new page and the press lands; scrolling forward keeps naming the old page until the whole
page has moved, so `anchor + 1` does not. Reported from the demo and measured there — three presses 120 ms
apart:

| direction | presses | took effect |
| --- | --- | --- |
| `prev` | 3 | 2 |
| `next` | 3 | 0 |

winia therefore steps from **the page the arrows last requested** (`arrow_target`), not from the anchor,
and remembers that page until `sync_month_pages` sees the list arrive on it. Each press is worth exactly
one page in both directions, and with nothing in flight the rule degrades to the anchor — so isolated
presses behave exactly as before. Measured after the fix, same gesture: `prev` 6/6 and `next` 6/6 at
120 ms; `prev` 5/5 and `next` 5/5 at 60 ms; and three settled presses still move `[+1, +1, +1]` and
`[-1, -1, -1]`.

The guard is `ui::date_picker::tests::a_second_press_during_the_animation_steps_again_in_both_directions`,
which asserts the whole trajectory of targets (101/102/103 forward, 99/98/97 back) rather than a final
state — a fix that landed on the right page while skipping one would pass a last-value check.

The pager's derived anchor is not part of what gets drawn, so reading it from outside needs a probe:
`WINIA_DP_PAGE_PROBE=<tag>` makes the demo compose a `page:N off:M` readout (`DockedDatePicker::page_probe`),
which is how the numbers above were taken. Off by default, so the demo itself is unchanged.

### One framework change this needed

`LazyList`'s measure leaves the main axis unbounded so items wrap to their content, which is wrong for a
paged list: `fill_items(true)` pins each item to the viewport, and `snap_paging(true)` writes the snap
configuration back for the fling to read.

The related framework fix is in `ItemHeightCache::set_uniform`. The 2412 pages are priced at the flat
`LAZY_ITEM_ESTIMATED_HEIGHT` until each is measured, and a page is several times that, so every position
past the measured window is wrong — and so is every fling that reaches it. `fill_items` guarantees the item
size, so the cache is told it instead of guessing; measured, the page positions went from meaningless
(~78 000 for the pages after the visible one) to exact multiples of 336.

## Outside-month days

**Docked only.** The docked picker's grid fills its leading and trailing slots with the neighbouring
month's days — September 2026 opens with August's 30th and 31st and closes with October's 1st through 10th.
The modal picker's grid leaves those slots empty.

That split is not a preference: the M3 specs page draws the two variants differently, and Compose agrees
with the modal one. Read off the specs page, the two anatomies are separate lists:

| | Docked date picker anatomy | Modal date picker anatomy |
| --- | --- | --- |
| grid states | Unselected date, Today's date, **Outside month date**, Selected date | Today's date, Unselected date, Selected date |
| header | (none — "Outlined text field" belongs to the caller) | Headline, Supporting text, Header |
| rest | Month/Year menu button, Icon button, Weekdays label text, Text buttons, Container | Container, Icon button(s), Weekdays, Menu button, Text buttons, Divider |

The **docked** list carries "Outside month date" and the specs give it its own two tokens:

| Specs token | Value | winia |
| --- | --- | --- |
| Date unselected outside month label text color | `#1D1B20` | `theme.on_surface` — the baseline value of `#1D1B20`, which is also what the plain unselected day role resolves to (verified: `day_content` in the default light scheme is rgb(29, 27, 32), the hex exactly) |
| Date unselected outside month label text opacity | `0.38` | `DISABLED_ALPHA` (`ColorScheme.kt:1518`), which winia already carries at 0.38 |

So `DatePickerColors::outside_month_label()` is the plain day role at `DisabledAlpha` — the same expression
`day_label(_, false, _)` already returns for a disabled unselected day. It is spelled out as its own role
anyway, because outside days are NOT disabled (they are context), and material3 has no colour for them, so
the match with the disabled expression is the only thing joining the two and a reader should not have to
rediscover it.

The **modal** list has no such entry, and neither does material3: `Month` composes a `Spacer` in those
cells (`DatePicker.kt:1870-1890`), and a grep for `outsideMonth` / `outside_month` across
`target/compose-src` finds nothing. So the modal picker is not deviating from anything by leaving them
empty — it is following both.

`MonthGrid` itself computes the neighbouring days either way, because the dates are true whether or not
anyone draws them; `month_grid` takes a `show_outside_month` flag and the two callers pass opposite values.
`DayCell::is_outside_month` carries the fact, `is_enabled` is forced off for it regardless of
`SelectableDates`, and `day_cell` draws it dimmed. `the_docked_grid_draws_the_neighbouring_months_days_and_the_modal_one_does_not`
pins the split by counting day cells in the composed tree and splitting them on whether the cell's `Surface`
is clickable: the docked grid has inert outside cells and the modal grid has none, while the two agree on
the number of choosable ones. It counts rather than reads labels, because a month page is a `LazyRow` now —
42 drawn labels for one page is not a number the tree can be asked for. Flipping the modal call to `true`
puts inert cells on the modal side and goes red.

Measured on the running demo, comparing an outside day against an in-month day in the same row:
outside label peak luma 114.0, in-month 229.0, container 43.7 — an implied alpha of
`(114.0 − 43.7) / (229.0 − 43.7) = 0.379` against the specs' 0.38.

**Outside days are context and are never selectable.** Tapping one cannot move the selection into a month
the grid is not showing. Measured with the debug server: selecting an in-month day moves the painted
selection disc to (258, 417), tapping the outside "30" leaves it at (258, 417), and tapping the next
in-month day moves it again to (289, 416) — inert, not dead.

An outside cell carries its real flags: `utc_time_millis`, `is_today` and `is_selected` are all computed
from the date, so a day selected in another month still reports itself as selected and announces its own
date to a screen reader. `day_cell` is what declines to draw it as such — the selection fill is pinned to 0
and the today ring is suppressed, since the specs draw "Today's date" and "Outside month date" as separate
states and a ring would claim a cell the user cannot choose. The millis arithmetic needs no special case:
the month start is the 1st at 00:00 UTC, so a signed day offset walks into the neighbouring month on its own
and `date_of_millis` resolves the day number.

### The modal picker's empty slots are sized, not absent

Skipping an outside-month cell outright is wrong, and material3's own source says why: `Month` does not
leave a hole, it composes a `Spacer` measured to the day's 48 dp, with the comment "Match the spacer's
minimum size to the Day's required size. This will ensure an aligned layout"
(`DatePicker.kt:1876-1890`).

Composing nothing collapses that row to zero height, and the grid's `Column` is `SpaceEvenly`, so the
leftover space redistributes and every row above shifts. Measured on September 2024 in the modal picker,
whose trailing week is *entirely* outside the month and therefore entirely empty: the five surviving rows
moved up by 13.7 dp, and `date_picker_paints_a_forty_dp_day_with_a_one_dp_ring_around_today` — which scans
a chord through the 10th's centre from a lattice formula — measured that chord at **31 dp instead of 39**.
That test is the standing guard: it went red on this and green again once the slot was sized.

winia's `Spacer` only spans one axis (`vertical`/`horizontal`), so the slot is an empty `Stack` instead —
both are childless `BoxLayout`s, so the slot measures identically.

`MonthGrid` fills every cell, and `cells()`, `rows()` and `cell()` return `DayCell` rather than
`Option<DayCell>` — a public signature change, so both `cargo check --examples` and `cargo check --tests`
were re-run.


A second fixture drives the modal variant (`fixture_date_picker_dialog.rs`, one test): the dialog is an
overlay, measures 360 × 560, a tap on the today cell moves the selection the page reads out, the dismiss button
closes it and the page's button re-opens it. The overlay entry outlives the state that closes it while the
dialog's exit motion plays, so that test polls `overlay_count` rather than reading it once — the same wait the
popup test uses.

One trap that measurement found is recorded in the year test: a `test_tag` inside a scrolled list reports the
item's position in the list's **content** coordinates rather than the viewport's. The picked year came back at
y = 2316 while it is drawn at 237 — exactly the list's 2079 dp of scroll — so tapping the rectangle `find_tag`
returns lands outside the window and does nothing at all. The test takes the cell's x from the tag (the list does
not scroll sideways) and computes y from the panel's top.

The day-tap test guards a pitfall whose symptom points the wrong way, so both halves are recorded. The taps
looked dead: the arrows took one, nothing inside the month grid did, the day cell's handler never appeared to
run, and a clickable on the picker's own container — whose box covers the whole grid — stayed silent below the
grid's top. In fact every tap reached its handler and every handler changed the state; the picture was frozen.
`DatePicker::build` was a plain `fn` rather than `#[composable]`, so the nodes it composes had no
per-statement key base. A month has a different number of day cells every month, so the node count inside the
grid changes; the next compose handed a node a `slot_key` another node already held, winia's `[dup-key]` guard
panicked in `winia/src/core/materialize.rs`, and every later frame was skipped with the previous picture kept
(`[render-panic] … 本帧已跳过，上帧画面保留`). The window went on showing September's selection while the state
underneath moved on, and the hit test on the half-materialized arena stopped finding the day cells at all —
which is what made the press look consumed. With `#[composable]` back, a 4 dp scan down the container's first
grid column advances the month and the selection on every tap. Turning the attribute off again makes exactly 1
test red: `date_picker_selects_the_day_that_is_tapped`.

Next: the input mode — `DatePicker(state, displayMode = Input)`, the header's mode toggle, and the
`DateInputContent` the public picker switches to (material3 1.5 has no public `DateInput`).

## Known gaps

Found in review, verified against `target/compose-src`, deliberately NOT fixed in the pass that found them.
Each is a real divergence, not a guess.

### Accessibility

- **Day and year cells carry no role or state.** `day_cell` and `year_cell` set a content description and
  nothing else. Compose builds each cell on `Surface(selected, onClick)` and then adds
  `role = Role.Button` with `mergeDescendants = true` (`DatePicker.kt:2013-2016`, `:2149-2152`), so the
  node also carries `selected`, `enabled` and a click action. winia's `Surface` contributes no semantics of
  its own, and `Surface::selectable` adds none either. A screen reader announces a named region, not a
  button.
- **The header headline announces nothing.** Compose gives it both `liveRegion = LiveRegionMode.Polite` and
  `contentDescription = headlineDescription` (`DatePicker.kt:719-728`), so picking a date is spoken. winia's
  headline carries no semantics config. `SemanticsConfig::live_region` already exists.
- **The nav buttons claim a live region they do not set.** The comment above the month/year nav button says
  Compose "makes it a polite live region, so a reader announces the month as the arrows move it"
  (`DatePicker.kt:2205-2216`), and the code sets only the content description. Compose sets
  `liveRegion = Polite` plus the description on that `Text` (`DatePicker.kt:2208-2214`), inside a `TextButton`
  that supplies the button role and click action.
- **The weekday label's description is on the wrong node.** It hangs off the inner `Text` — a glyph-sized
  node — so its reported a11y bounds are far smaller than the 48 dp cell, and the inner text keeps its own
  `"S"` where Compose's is cleared. Compose puts the config on the 48 dp `Box` with `clearAndSetSemantics`
  (`DatePicker.kt:1803-1819`).

### State and API surface

- **`remember_date_picker_state` exposes none of Compose's five parameters**
  (`initialSelectedDateMillis`, `initialDisplayedMonthMillis`, `yearRange`, `initialDisplayMode`,
  `selectableDates` — `DatePicker.kt:368-374`). A hoisted picker cannot be given an initial selection,
  displayed month, year range or date policy without hand-rolling `DatePickerState::with(..)` plus a
  `remember`, which is what both fixtures do.
- **`selectable_dates` is frozen at construction.** Compose holds it in a `mutableStateOf`
  (`DatePicker.kt:1133`) and re-applies the caller's instance every composition (`:386-389`), so a policy
  closing over state stays live. winia's is captured when the state is built and a grid goes stale.
- **Small things worth a pass**: `horizontalScrollAxisRange = 0..0` on the months list so AT traverses days
  instead of scrolling months (`DatePicker.kt:1726-1729`); `paneTitle` on the year panel (`:1634`);
  `PlainTooltip` on the month arrows (`:2281-2289`); `CHECK_PATH` is a public constant nothing draws, and
  `month_list` marks the displayed month with a fill only; `MODE_TOGGLE_PADDING` and `CONTAINER_HEIGHT` are
  public and unused, the dialog using a duplicate `MODAL_CONTAINER_HEIGHT`; `step_displayed_month` is now
  reachable only from tests since both pickers' arrows drive the list, so `step_arrow`'s doc still
  describing them as calling it is stale; the headline row uses `SpaceBetween` where Compose uses `Start`
  whenever there is no mode toggle (`DatePicker.kt:1373-1378`); and the header height is pinned to exactly
  120 where Compose applies it as a `defaultMinSize` minimum (`DatePicker.kt:1680-1685`), so a long locale
  title clips.

## Input mode

`DisplayMode::Input` swaps the calendar for a text field the user types a date into. It exists on the
modal picker only; `DockedDatePicker` is calendar-only, as in Material 3. The swap happens where the
build path reads the mode, and the header's toggle writes it — before that, `set_display_mode` stored a
value nothing read and switching did nothing at all.

The entry is eight digits with the locale's delimiters between them. What the user sees is ten
characters; what the field holds is eight. `DateVisualTransformation` owns the difference in both
directions.

| Piece | Where | Material 3 |
| --- | --- | --- |
| `DateInputFormat`, `DateInputFieldOrder` | `date_picker.rs` (locale-driven) | `DateInput.kt:392` |
| `CalendarModel::parse`, `format_with_pattern` | `date_picker.rs` | `CalendarModel.kt` |
| `DatePickerState::validate_date_input` | `date_picker.rs` | `DateInputValidator`, `DateInput.kt:282-358` |
| `DateVisualTransformation`, `DateOffsetMapping` | `date_picker.rs` | `DateInput.kt:392-439` |
| `date_input_content` | `date_picker.rs` | `DateInputContent`, `DateInput.kt:59-113` |
| `display_mode_toggle` | `date_picker.rs` | `DisplayModeToggleButton`, `DatePicker.kt:1402-1424` |

The pattern belongs to `CalendarLocale` rather than to a platform locale lookup: winia carries no
locale database, so a caller that wants a locale supplies one whose input format carries its own
pattern.

Both of the header's own strings follow the mode, not just the headline: the title reads
`Enter date` while the field is showing, because a field that asks for a date is not headed
`Select date` (`DatePicker.kt:654`). Only the default follows — a title the caller passes stands in
both modes, which is why `DatePicker` remembers whether its title is still the default rather than
comparing the string.

Validation runs in Compose's order, so a real date outside `yearRange` reports the range rather than
the pattern, and a date the policy refuses reports the policy:

1. the digits do not parse — `Date does not match expected pattern: MM/DD/YYYY`
2. the year is outside the range — `Date out of expected year range 1900 - 2100`
3. the date is not selectable — `Date not allowed: {date}`

A refused entry is refused as a whole: it is drawn, it is announced through `SemanticsState::error`,
and it does not become the selection. A shorter entry is not judged at all — it clears the error and
leaves the selection empty, so a half-typed date never looks like a rejected one.

### Deliberate deviations

- **Offset mapping is one position looser than Compose's.** `DateInput.kt:413-420` branches on
  `<= firstDelimiterOffset - 1` and `<= secondDelimiterOffset - 1`, which is one too tight: for
  `MM/dd/yyyy` its forward map puts typed offset 4 at displayed offset 5, and its backward map reads
  that 5 as 3, so the caret jumps back a character the moment it crosses a delimiter. Winia branches
  on the offsets themselves, which makes each side the exact inverse of the other at every position.
  Backward to forward stays non-bijective on purpose: a delimiter is zero-width in the stored text,
  so both sides of one belong at the same offset. `every_caret_position_maps_to_the_same_digit_both_ways`
  pins this.
- **No animated mode switch.** Compose runs the two modes through `AnimatedContent` with a 48 dp
  parallax and a clipped `SizeTransform` (`DatePicker.kt:1457-1524`). winia swaps them and resizes at
  once.
- **No soft-keyboard hints.** Compose sets `KeyboardType.Number`, `autoCorrectEnabled = false` and
  `ImeAction.Done` (`DateInput.kt:163-227`). winia has no IME hint channel at all — no
  `ImeAction`, no keyboard type — and no autocorrect to switch off, so the three have no target. The
  field refuses non-digits on entry, which is the part a user notices on a desktop.
- **The field takes focus by itself, and the keys that follow land on it.** The field asks for focus
  300 ms after it appears, Material 3's `MotionTokens.DurationMedium2` (`DateInput.kt:259-266`);
  `focused_tags()` reports `date-picker-input-field` with no click, and keys typed straight after it
  reach the field — eight Backspaces clear the selection and a full entry commits one. Only a real
  window can show where a key lands, so `the_entry_field_takes_focus_and_typing_without_a_click`
  measures it there. An earlier note here said the key did not arrive; nothing between that reading
  and this one touched key or focus routing, and `UiTest::launch` only checks that
  `target/debug/fixture_all.exe` exists rather than rebuilding it, so the likeliest explanation is a
  fixture binary older than the focus work. That is an inference, not a measurement — what is
  measured is that the current build delivers the keys, and the test is what holds it there.

### The dialog is its content

material3 wraps the dialog's content in `Box(Modifier.weight(1f, fill = false))`
(`DatePickerDialog.android.kt:95`) and says why: "Fill is false to support collapsing the dialog's height
when switching to input mode". The box's share is a MAXIMUM rather than the exact size a weight normally
hands out, so the box reports whatever the picker asks for and the column ends up content + buttons.

winia had no `fill` on a weight at all — `Modifier::layout_weight` always took the whole share — so the
dialog could only ever be as tall as the `CONTAINER_HEIGHT` cap. `Modifier::layout_weight_fill(weight, fill)`
is that missing half: with `fill = false` the share becomes the child's main-axis maximum and the parent
keeps the child's own measurement. The measure half is `FlexAxis::build_phase2`, matching Compose's
`createConstraints(mainAxisMin = if (parentData.fill) childMainAxisSize else 0, mainAxisMax =
childMainAxisSize, isPrioritizing = true)` (`RowColumnMeasurePolicy.kt:195-207`).

Measured on the fixture, the dialog's rect (`find_tag_in_overlay` on `dpi-dialog`):

| Mode | Before | After |
| --- | --- | --- |
| Input | 360 × 568 (the cap, with the content ending around y = 274) | 360 × 240 |
| Picker | 360 × 568 | 360 × 560 |

Both figures are the picker's own content, so they can be re-derived: 240 is the 120 dp header, the
outlined field's 56, its 16 dp bottom inset while no error shows and the 48 dp action row; 560 is the
120 dp header, 56 dp month navigation, 48 dp weekday row, 288 dp month and the same 48 dp action row.

`the_dialog_is_as_tall_as_the_mode_it_shows` asserts both, in each mode, so a stretch creeping back
into either fails rather than passing under a loose bound. The comparison is against the whole logical
pixel: the debug tree serialises measured sizes with `{:.0}` (`debug.rs:593`), so the assertion's real
tolerance is ±0.5 rather than zero. Both figures also assume the dialog's default content — the title is
what gives the header its 120 dp, and a caller passing `.title(None)` drops the header to 44 and the
dialog with it.

One deviation rides along: the dialog's `Column` uses `Arrangement::Start` where the source says
`SpaceBetween`. winia's `SpaceBetween` stretches a container to the main axis its parent offers — pinned
by `layout/row.rs`'s `test_row_rtl_space_between_mirrors_full_width` and by
`layout/column.rs`'s `a_non_filling_weight_does_not_shrink_a_space_between_container`, measured here
rather than documented on the `Arrangement` variants themselves — which is the one thing
that would hold this dialog at the cap regardless of the box. In Compose the column here is content +
buttons, so its leftover space is zero and `SpaceBetween` places exactly as `Start` does; the switch keeps
winia's own semantics for that arrangement intact instead of widening them for one dialog.

### Still open

- `remember_date_picker_state` still takes only a locale, against Compose's five parameters.
- `selectable_dates` is frozen when the state is built; Compose re-reads the caller's policy every
  composition.
- Day and year cells contribute no `Role.Button`, `selected` or `enabled`; the headline's
  `headlineDescription` and the toggle's polite live region are done, the rest are not.
