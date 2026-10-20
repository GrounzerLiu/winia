# Pager

One full-viewport page at a time, with snapping: Compose's `HorizontalPager` / `VerticalPager`.

## Sources

`reference/compose/foundation-1.13.0-beta01/commonMain/androidx/compose/foundation/pager/` -
`Pager.kt` (the two builders), `PagerState.kt` (currentPage / settledPage / targetPage /
currentPageOffsetFraction / scrollToPage / animateScrollToPage), `PageSize.kt` (Fill / Fixed),
`PagerMeasure.kt` (how the counter is computed), plus
`gestures/snapping/SnapPosition.kt` for the page placement shift.

winia does not add a second layout engine for this: a page is a lazy item that fills the viewport, and
paging is the lazy list's snap. The pieces that already existed are
`LazyList::fill_items` (`fillParentMaxWidth`/`Height`) and `LazyList::snap_paging`
(`snapFlingBehavior`), both of which the date picker already used for its months.

```rust
let state = PagerState::remember(ctx, 5);
HorizontalPager::new(state.clone())
    .modifier(Modifier::new().fill_max_size())
    .page_spacing(8.0)
    .build(ctx, |ctx, page| Text::new(format!("Page {}", page)).build(ctx));
```

## Variants

| Constructor | Axis | Use for |
| --- | --- | --- |
| `HorizontalPager::new(state)` | main axis horizontal | carousels, onboarding, galleries |
| `VerticalPager::new(state)` | main axis vertical | full-screen feeds, story-style content |

Both are `Pager<A>` over the same lazy list; the axis decides which dimension is the page size, which
modifier attaches (`.horizontal_scroll` / `.vertical_scroll`) and which dimension a fling moves.

| Item | Mirrors |
| --- | --- |
| `PagerState::new(page_count)` / `PagerState::remember(ctx, n)` | `rememberPagerState(pageCount = { n })` |
| `current_page()` / `current_page_offset_fraction()` | `currentPage` / `currentPageOffsetFraction` |
| `target_page()` / `settled_page()` | `targetPage` / `settledPage` |
| `is_scroll_in_progress()` | `isScrollInProgress` |
| `scroll_to_page(page, fraction)` / `animate_scroll_to_page(page, fraction)` | `scrollToPage` / `animateScrollToPage` |
| `current_page_position()` | the continuous position (`currentPage + fraction`) |
| `layout_info()` | `PagerLayoutInfo`: page count, viewport size, page size, current page + fraction |
| `list_state()` | the underlying `LazyListState` anchors |

Builder parameters, all mapped onto the lazy list: `.modifier`, `.content_padding(px)`
(`contentPadding`), `.page_spacing(px)` (`pageSpacing`), `.reverse_layout(bool)` (`reverseLayout`),
`.page_size(PageSize::Fill | PageSize::Fixed(px))` (`PageSize`), `.snap_position(..)` (`snapPosition`,
only visible for a fixed page smaller than the viewport), `.beyond_viewport_page_count(n)`
(`beyondViewportPageCount`), `.user_scroll_enabled(bool)` (`userScrollEnabled`),
`.snap_positional_threshold(f32)` (`snapPositionalThreshold`), `.snap_animation_spec(SpringSpec)`
(`snapAnimationSpec`), `.max_pages_per_fling(n)` (`PagerSnapDistance.atMost(n)`),
`.page_nested_scroll_connection(conn)` (`pageNestedScrollConnection`), `.fling_behavior(behavior)`
(`flingBehavior`, see [fling-behavior.md](fling-behavior.md)). The content is a
`build(ctx, |ctx, page| ...)` parameter, the `Fn(&mut ComposeCtx, T)` shape `docs/component-authoring.md`
1.3 reserves for a container that hands its content a scope value.

## Layout

- **A page fills the main axis by default** (`PageSize::Fill`): the item's main constraint is pinned to
  the viewport LESS the main-axis `content_padding` (Compose's `mainAxisAvailableSize`), so a padded
  pager's page is `viewport - before - after` and one page is one snap position. `PageSize::Fixed(px)`
  pins the item instead and the step becomes page + spacing, which lets the next page peek in.
- **`page_spacing` is part of the step.** It is added between items, so the snap distance is
  `page + spacing` (it used to be the page alone, which was only right for `spacing = 0`).
- **`snap_position` shifts the PAGE, not the scroll target** (which is what Compose's
  `SnapPosition.position()` does): every item is placed `Start`=0 / `Center`=(viewport-page)/2 /
  `End`=viewport-page pixels further along the main axis, and the snap positions stay on clean page
  boundaries `pad_before + k * step`. That is why a centred fixed page rests exactly on a boundary,
  so a centred page of ANY index, including the first, rests at fraction 0.
- **The content is that much longer** (`content_h` includes the shift), so the LAST page can reach
  its rest position too.
- **`content_padding`** is main-axis padding before the first page and after the last;
  `.content_padding_cross` on the underlying list is not exposed here.
- **RTL: not mirrored.** The lazy list has no layout-direction handling today - a horizontal
  `LazyRow`/pager lays out from the left in every direction, and `reverse_layout` is the only flip.
  Compose mirrors a horizontal pager under RTL. This is a gap in the lazy list, not in the pager, and
  it is listed under Differences.
- The pages are placed by the lazy list's measure in CONTENT coordinates
  (`pad_before + cumulative(item + spacing) + snap shift`); the scroll offset is applied by the scroll
  modifier when it paints, not in the placement. The policy writes `first_visible_index` /
  `first_visible_offset`, `viewport_main` and the snap step back; the pager derives the page from the
  scroll offset and the step, so it only needs `viewport_main` for the `Fill` page size (one frame
  stale on the very first pass: `Center`/`End` behave as `Start` until it lands).

## Behaviour

- **`current_page` is the page CLOSEST to the snap position**, not the first visible one: halfway
  through a swipe the counter flips to the next page, and the offset fraction stays in `[-0.5, 0.5)`.
  This is Compose's definition and what makes a dot indicator behave. It is computed from the
  scroll offset divided by the STEP (page + spacing), never by the viewport: for `PageSize::Fixed`
  the viewport division made the counter lag a whole page and scaled the fraction by page/viewport.
- **`scroll_to_page(page, fraction)` / `animate_scroll_to_page` take a page FRACTION**, as Compose
  does (`0.0` = the rest position, `1.0` = a whole page past it), converted with the page size the
  last frame resolved. `target_page` is the page a programmatic scroll asked for; the derive resets
  it to the settled page once the scroll comes to rest.
- **The counter is a derived subscription** (`remember_derived_state` over the list's offset, page
  size, spacing and padding, all States): it moves on every scroll tick, whatever moved the list - a
  drag, a fling, the wheel, a programmatic jump. A programmatic `scroll_to_page` lands during LAYOUT,
  so the counter picks it up on the next frame; a caller that needs the value immediately should read
  `list_state().first_visible()`. The four counter States go out in one `write_batch`, so a
  synchronous observer never sees the new page next to the old fraction.
- **Only a window of pages composes** (`beyond_viewport_page_count`, Compose's
  `beyondViewportPageCount`), so a 200-page pager composes a handful.
- **Not every path re-snaps, and that is on purpose.** A wheel notch, a drag release and a fling all
  settle on a page. A resize that changes the page size, a scrollbar-thumb drag, and a programmatic
  `scroll_to_page(page, fraction)` with a fractional offset can leave the pager off the grid - the
  last one by design (Compose's `scrollToPage(pageOffsetFraction)`), and a generic "snap whatever is
  off-grid" pass would fight it. Those paths need their own decision, not a background heal.
- **One gesture moves at most one page**, because the snap is the lazy list's own: below
  `MinFlingVelocityDp` (400 dp/s) it settles on the nearer page, above it on the next one.
- **`user_scroll_enabled(false)`** drops the wheel/drag channel but keeps the offset, so
  `scroll_to_page` and an external animation still move the pager.
- **Two fling knobs are exposed**, the rest of `PagerDefaults.flingBehavior` is not.
  `snap_positional_threshold(f32)` (`snapPositionalThreshold`, default `0.5`) is how far into the page
  a release BELOW the fling floor has to be before it carries on: `0.3` turns a page from 30% of the
  way, `0.9` demands almost a whole page. A flick above `MinFlingVelocityDp` still goes the way it was
  thrown, whatever the threshold says. `snap_animation_spec(SpringSpec)` (`snapAnimationSpec`) is the
  spring the settle runs on; the default is material3's `DefaultEffects` (stiffness 1600, damping 1.0),
  and the wheel's page step uses the same spec, so one pager has one settle curve.
- **`max_pages_per_fling(n)`** is Compose's `PagerSnapDistance.atMost(n)`: how many pages one flick may
  carry, `1` by default. Above the fling floor the target is the page the release starts in, plus as
  many pages as the decay's asymptote (`v0 / friction`) suggests, rounded to whole pages, never fewer
  than the one page it was thrown toward, and capped by this number. So `1` is exactly the old
  one-page-per-gesture rule, and `3` lets a hard flick cross a gallery in one motion. The wheel is NOT
  affected: a notch is still one page.
- **`fling_behavior(behavior)`** replaces the strategy a release runs through entirely (Compose's
  `flingBehavior`). The built-in one is what the three knobs above configure; a custom strategy is
  asked once per release and answers with a `FlingPlan` — a spring to somewhere, momentum, or nothing
  — which the frame clock executes. See [fling-behavior.md](fling-behavior.md) for the trait, the
  contracts and what a strategy cannot do (drive the scroll per frame).
- **One wheel NOTCH moves one page** (`ScrollState::snap_page`), which also gives the date picker's
  months the same feel; a non-paged list keeps scrolling smoothly. A notch is a discrete
  `LineDelta`; a continuous `PixelDelta` stream (a precision touchpad, a high-resolution wheel) is a
  different thing and must not page per EVENT - a flick at 120 Hz ran a pager a dozen pages that way.
  So the events of one gesture bank travel in `WheelPager` (`app.rs`) and a page is armed only once a
  real notch's worth has accumulated (`PIXEL_PAGE_TRAVEL` = 60 logical px = three 20 px lines); a gap
  of `SESSION_GAP` (400 ms), a reversal, or a different container starts the bank over, and at most one
  page turns per event. A notch with no page left in that direction never pixel-scrolls the pager —
  that path cancels the settle in flight and nudges the offset, which strands the pager between two
  pages until the next notch re-derives a target. The event goes to the NESTED chain instead (an
  ancestor may still use it, which is what keeps a carousel inside a list from freezing that list at its
  last page), and the pager's bar pulses when nobody moves. Compose reaches a similar result by a different route: its mouse-wheel logic
  synthesizes a velocity (up to 1 dp/ms, `MouseWheelScrollingLogic.kt:250-256,308-310`), and the snap
  fling behavior runs on wheel (`shouldBeTriggeredByMouseWheel`, default true) so a notch at or above
  `MinFlingVelocityDp` (400 dp/s) advances a page. winia's step is exact - one page per notch whatever
  its magnitude - and it applies to any paged container rather than only to a pager. The same routing
  serves the overlay layers, each banking its own travel: a pager inside a menu pages like one in the
  main tree.

## Differences from Compose

| Compose | winia |
| --- | --- |
| `PageSize.Fill` / `PageSize.Fixed(n)` | Both, through the lazy list's `fill_items` / `item_main_size`. |
| `beyondViewportPageCount` | `beyond_bounds` on the lazy list: the composition window is configurable. |
| `snapPosition` (`Start/Center/End`) | A snap offset inside the step; a filled page always rests at Start, so it only shows with `PageSize::Fixed`. |
| `userScrollEnabled` | `user_scroll_enabled(false)` drops the wheel/drag channel; programmatic scrolling still works. |
| `flingBehavior` — a replaceable `TargetedFlingBehavior` | `fling_behavior(behavior)`, over the plan-shaped trait in [fling-behavior.md](fling-behavior.md): the strategy returns the motion (target + curve, momentum, or nothing) and the frame clock runs it. It cannot drive the scroll per frame the way Compose's suspending `performFling` does. |
| `PagerSnapDistance` (default `atMost(1)`) | `max_pages_per_fling(n)`; the default `1` matches Compose's, and unlike Compose there is no `0` ("no fling at all") — turn user scrolling off instead. |
| `pageNestedScrollConnection` | A hook: `page_nested_scroll_connection(conn)` attaches the caller's `NestedScrollConnection` to the pager's list. Compose's DEFAULT connection (a nested pre-scroll past the pager's end turns a page) is not ported. |
| `PagerScope` (`this.currentPage` inside content) | The page index is the content parameter instead. |
| RTL mirrors the pages | Not mirrored: the lazy list has no layout-direction handling. |
| The wheel takes the DOMINANT axis and is ignored by a container on the other axis (`NonTouchScrollingLogic.kt:255-264`), so a plain vertical wheel does not move a `HorizontalPager` | A vertical wheel IS routed to a horizontal PAGER (and only to one, so a vertical ancestor keeps its wheel); non-paging horizontal lists still ignore it. |

## Tests

`cargo test -p winia --lib components::pager` - starts on page 0; `scroll_to_page(3)` lands on page 3
(checked through both the list anchor and the page counter, across the layout-then-derive order); a
200-page pager composes only a window; `beyond_viewport_page_count(0)` composes fewer still AND still
pages (the snap spec does not depend on a second composed page); a fixed page keeps its size and the
viewport is reported; a padded `Fill` page MEASURES `viewport - before - after` (asserted on the page
node, not only on the reported size); a centred fixed pager reaches its last page; a page step
survives the list rebuilding itself mid-spring; `user_scroll_enabled(false)` removes the scroll
channel; the fraction stays inside `[-0.5, 0.5)`, including past the last page; the two fling knobs
reach the snap spec the measure writes back, the defaults are 0.5 and material3's `DefaultEffects`
spring, a slow release at 40% of a page turns it under `0.3` but not under the default `0.5`, a
4000 px/s flick lands one page out by default and three with `max_pages_per_fling(3)`, and a caller's
nested-scroll connection rides on the list's modifier while a default pager attaches none, and a
pager hands a custom `fling_behavior` to the list's scroll state — a release then lands where the
strategy says instead of on a page boundary.

`cargo test -p winia --lib positional_threshold` / `settle_spring` - the threshold decision itself
(`snap_target` at 40% and 60%, above and below the fling floor) and that `snap_settle` returns the
spring the spec carries.

`cargo test -p winia --lib wheel` - one notch pages on its own delta; tiny `PixelDelta` events bank a
notch before any of them pages; a notch drops travel a touchpad banked; a reversal and a pause each
start the bank over; one oversized event turns one page and carries nothing; two containers never
share one gesture's travel; a vertical wheel reaches a horizontal PAGER but not a plain row; a
banking event is held rather than pixel-scrolled.
`cargo test -p winia --lib layout::lazy_column` - the list underneath (window, snapping, anchors).
