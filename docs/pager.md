# Pager

One full-viewport page at a time, with snapping: Compose's `HorizontalPager` / `VerticalPager`.

## Sources

`reference/compose/foundation-1.13.0-beta01/commonMain/androidx/compose/foundation/pager/` -
`Pager.kt` (the two builders), `PagerState.kt` (currentPage / settledPage / targetPage /
currentPageOffsetFraction / scrollToPage / animateScrollToPage), `PageSize.kt` (Fill / Fixed).

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
| `scroll_to_page(page, px)` / `animate_scroll_to_page(page, px)` | `scrollToPage` / `animateScrollToPage` |
| `current_page_position()` | the continuous position (`currentPage + fraction`) |
| `layout_info()` | `PagerLayoutInfo`: page count, viewport size, page size, current page + fraction |
| `list_state()` | the underlying `LazyListState` anchors |

Builder parameters, all mapped onto the lazy list: `.modifier`, `.content_padding(px)`
(`contentPadding`), `.page_spacing(px)` (`pageSpacing`), `.reverse_layout(bool)` (`reverseLayout`),
`.page_size(PageSize::Fill | PageSize::Fixed(px))` (`PageSize`), `.snap_position(..)` (`snapPosition`,
only visible for a fixed page smaller than the viewport), `.beyond_viewport_page_count(n)`
(`beyondViewportPageCount`), `.user_scroll_enabled(bool)` (`userScrollEnabled`). The content is a
`build(ctx, |ctx, page| ...)` parameter, the `Fn(&mut ComposeCtx, T)` shape `docs/component-authoring.md`
1.3 reserves for a container that hands its content a scope value.

## Layout

- **A page fills the main axis by default** (`PageSize::Fill`): the item's main constraint is pinned to
  the viewport, so one page is one snap position. `PageSize::Fixed(px)` pins the item instead and the
  step becomes page + spacing, which lets the next page peek in.
- **`page_spacing` is part of the step.** It is added between items, so the snap distance is
  `page + spacing` (it used to be the page alone, which was only right for `spacing = 0`).
- **`snap_position` places a fixed page** inside the viewport: `Start` puts it at the leading edge
  (offset 0), `Center` at `(viewport - page) / 2`, `End` at `viewport - page`. A filled page always
  rests at Start because there is no leftover space.
- **`content_padding`** is main-axis padding before the first page and after the last;
  `.content_padding_cross` on the underlying list is not exposed here.
- **RTL: not mirrored.** The lazy list has no layout-direction handling today - a horizontal
  `LazyRow`/pager lays out from the left in every direction, and `reverse_layout` is the only flip.
  Compose mirrors a horizontal pager under RTL. This is a gap in the lazy list, not in the pager, and
  it is listed under Differences.
- The pages are placed by the lazy list's measure: the policy measures the window, places each page at
  `offset + cumulative(item + spacing)`, and writes `first_visible_index` / `first_visible_offset`,
  `viewport_main` and the snap step back for the pager to derive from.

## Behaviour

- **`current_page` is the page CLOSEST to the snap position**, not the first visible one: halfway
  through a swipe the counter flips to the next page, and the offset fraction stays in `[-0.5, 0.5)`.
  This is Compose's definition and what makes a dot indicator behave.
- **The counter is derived and published at the end of the pass** (`side_effect`): it is recomputed
  from the anchors the pass observed, so a programmatic `scroll_to_page` (which lands during LAYOUT)
  shows up on the next frame - the same eventual consistency Compose has while a scroll animates. A
  caller that needs the value immediately should read `list_state().first_visible()`.
- **Only a window of pages composes** (`beyond_viewport_page_count`, Compose's
  `beyondViewportPageCount`), so a 200-page pager composes a handful.
- **One gesture moves at most one page**, because the snap is the lazy list's own: below
  `MinFlingVelocityDp` (400 dp/s) it settles on the nearer page, above it on the next one.
- **`user_scroll_enabled(false)`** drops the wheel/drag channel but keeps the offset, so
  `scroll_to_page` and an external animation still move the pager.

## Differences from Compose

| Compose | winia |
| --- | --- |
| `PageSize.Fill` / `PageSize.Fixed(n)` | Both, through the lazy list's `fill_items` / `item_main_size`. |
| `beyondViewportPageCount` | `beyond_bounds` on the lazy list: the composition window is configurable. |
| `snapPosition` (`Start/Center/End`) | A snap offset inside the step; a filled page always rests at Start, so it only shows with `PageSize::Fixed`. |
| `userScrollEnabled` | `user_scroll(false)` drops the wheel/drag channel; programmatic scrolling still works. |
| `flingBehavior`, `pageNestedScrollConnection`, `PagerSnapDistance` | Not exposed. Snapping is the lazy list's own (one page per gesture); a replaceable fling strategy would need a trait the list does not have. |
| `PagerScope` (`this.currentPage` inside content) | The page index is the content parameter instead. |
| RTL mirrors the pages | Not mirrored: the lazy list has no layout-direction handling. |

## Tests

`cargo test -p winia --lib components::pager` - starts on page 0; `scroll_to_page(3)` lands on page 3
(checked through both the list anchor and the page counter, across the layout-then-derive order); a
200-page pager composes only a window; `beyond_viewport_page_count(0)` composes fewer still; a fixed
page keeps its size and the viewport is reported; `user_scroll_enabled(false)` removes the scroll
channel; the fraction stays under half a page.
`cargo test -p winia --lib layout::lazy_column` - the list underneath (window, snapping, anchors).
