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

## API

```rust
let state = PagerState::remember(ctx, 5);
HorizontalPager::new(state.clone())
    .modifier(Modifier::new().fill_max_size())
    .page_spacing(8.0)
    .content(|ctx, page| Text::new(format!("Page {}", page)).build(ctx))
    .build(ctx);
```

| Item | Mirrors |
| --- | --- |
| `PagerState::new(page_count)` / `PagerState::remember(ctx, n)` | `rememberPagerState(pageCount = { n })` |
| `current_page()` / `current_page_offset_fraction()` | `currentPage` / `currentPageOffsetFraction` |
| `target_page()` / `settled_page()` | `targetPage` / `settledPage` |
| `is_scroll_in_progress()` | `isScrollInProgress` |
| `scroll_to_page(page, px)` / `animate_scroll_to_page(page, px)` | `scrollToPage` / `animateScrollToPage` |
| `current_page_position()` | the continuous position (`currentPage + fraction`) |
| `layout_info()` | `PagerLayoutInfo` (page count, viewport size, page size, current page + fraction) |
| `list_state()` | the underlying `LazyListState` anchors |
| `HorizontalPager` / `VerticalPager` | `HorizontalPager` / `VerticalPager` |

Builder parameters, all mapped onto the lazy list: `.modifier`, `.content_padding(px)`
(`contentPadding`), `.page_spacing(px)` (`pageSpacing`), `.reverse_layout(bool)` (`reverseLayout`),
`.page_size(PageSize::Fill | PageSize::Fixed(px))` (`PageSize`), `.snap_position(..)` (`snapPosition`,
only visible for a fixed page smaller than the viewport), `.beyond_viewport_page_count(n)`
(`beyondViewportPageCount`), `.user_scroll_enabled(bool)` (`userScrollEnabled`), `.content(f)`.

## Behaviour

- **A page fills the viewport by default** (`PageSize::Fill`), so the snap step equals the viewport
  and every page boundary is a snap position. `PageSize::Fixed(px)` pins the page instead and the
  step becomes page + spacing; `SnapPosition::Center/End` then decides where the smaller page rests.
  (The snap step used to ignore `page_spacing`; it now includes it, which only matters for a list
  that both fills items and spaces them.)
- **`current_page` is the page CLOSEST to the snap position**, not the first visible one: halfway
  through a swipe the counter flips to the next page, and the offset fraction stays in `[-0.5, 0.5)`.
  This is Compose's definition and what makes a dot indicator behave.
- **The counter is derived, not driven**: it is recomputed from the list's anchors at the end of each
  build, so a programmatic `scroll_to_page` (which lands during LAYOUT) shows up on the next frame.
  That is the same eventual consistency Compose has while a scroll animates; a caller that needs the
  value immediately should read `list_state().first_visible()`.
- **Only a window of pages composes**: the lazy list's windowing applies unchanged, so a 200-page pager
  composes a handful.

## Differences from Compose

| Compose | winia |
| --- | --- |
| `PageSize.Fill` / `PageSize.Fixed(n)` | Both, through the lazy list's `fill_items` / `item_main_size`. |
| `beyondViewportPageCount` | `beyond_bounds` on the lazy list: the composition window is configurable. |
| `snapPosition` (`Start/Center/End`) | A snap offset inside the step. A filled page always rests at Start (offset 0), so it only shows with `PageSize::Fixed`. |
| `userScrollEnabled` | `user_scroll(false)` drops the wheel/drag channel; programmatic scrolling still works. |
| `flingBehavior`, `pageNestedScrollConnection`, `PagerSnapDistance` | Not exposed. Snapping is the lazy list's own (one page per gesture, Compose's `snapFlingBehavior`); a replaceable fling strategy would need a trait the list does not have. |
| `PagerScope` (`this.currentPage` inside content) | The page index is passed to the content closure instead. |

## Tests

`cargo test -p winia --lib components::pager` - starts on page 0; `scroll_to_page(3)` lands on page 3
(checked through both the list anchor and the page counter, across the layout-then-derive order); a
200-page pager composes only a window; the fraction stays under half a page.
`cargo test -p winia --lib layout::lazy_column` - the list underneath (window, snapping, anchors).
