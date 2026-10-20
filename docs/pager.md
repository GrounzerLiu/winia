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
| `list_state()` | the underlying `LazyListState` (`layoutInfo` in Compose) |
| `HorizontalPager` / `VerticalPager` | `HorizontalPager` / `VerticalPager` |

Builder parameters that map onto the lazy list: `.modifier`, `.content_padding(px)` (`contentPadding`),
`.page_spacing(px)` (`pageSpacing`), `.reverse_layout(bool)` (`reverseLayout`), `.content(f)`.

## Behaviour

- **A page fills the viewport** along the main axis, so the snap step equals the viewport and every
  page boundary is a snap position.
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
| `PageSize.Fill` / `PageSize.Fixed(n)` | Fill only (`fill_items`). A fixed page needs a main-size knob the lazy list does not have yet. |
| `beyondViewportPageCount` | Not configurable: the lazy window is a fixed window around the viewport. |
| `snapPosition` (`SnapPosition.Start/Center/End`) | Boundary snapping only. |
| `userScrollEnabled`, `flingBehavior`, `pageNestedScrollConnection`, `PagerSnapDistance` | Not exposed: the list's scroll modifier and snap are used as they are. |
| `PagerScope` (`this.currentPage` inside content) | The page index is passed to the content closure instead. |
| `PagerState.layoutInfo` | `list_state()` exposes the anchors; there is no layout-info struct. |

## Tests

`cargo test -p winia --lib components::pager` - starts on page 0; `scroll_to_page(3)` lands on page 3
(checked through both the list anchor and the page counter, across the layout-then-derive order); a
200-page pager composes only a window; the fraction stays under half a page.
`cargo test -p winia --lib layout::lazy_column` - the list underneath (window, snapping, anchors).
