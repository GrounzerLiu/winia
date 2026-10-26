# LazyVerticalGrid / LazyHorizontalGrid

A windowed grid: items flow into LINES, and only the lines near the viewport are composed.

## Sources

Compose foundation `LazyVerticalGrid` / `LazyHorizontalGrid` and `GridCells`, under
`reference/compose/foundation-1.13.0-beta01/commonMain/androidx/compose/foundation/lazy/grid/`.

## API

```rust
LazyVerticalGrid::new()
    .cells(GridCells::Fixed(3))          // or GridCells::Adaptive(120.0)
    .spacing(8.0)                        // between lines
    .cross_spacing(8.0)                  // between cells
    .content_padding(16.0, 16.0)
    .modifier(Modifier::new().fill_max_size())
    .items(count, |ctx, index| { /* one cell */ })
    .build(ctx);

// Spans: an item may take several cells (Compose's GridItemSpan).
LazyVerticalGrid::new()
    .cells(GridCells::Fixed(3))
    .items_with(count, |i| i as u64, |i| if i == 0 { 3 } else { 1 }, |ctx, i| { /* ... */ })
    .build(ctx);
```

`LazyVerticalGrid = LazyGrid<VerticalAxis>` and `LazyHorizontalGrid = LazyGrid<HorizontalAxis>` — one
type over the same axis marker the lists use, so the horizontal grid is the vertical one with the
main axis swapped (and RTL handled by the horizontal scroll modifier, exactly as `LazyRow` does).

## Layout

1. **Chunking.** `cells` items per line, or as many as fit at the minimum size for
   `GridCells::Adaptive`. An item whose span does not fit the room left in a line starts the next
   one, and an item spanning the whole line gets a line of its own.
2. **Composition.** Each line is ONE lazy item — a `Row` for a vertical grid, a `Column` for a
   horizontal one — whose cells carry `layout_weight(span)`, so a two-cell item is twice as big as a
   one-cell item. A line's key is its first item's key, so a stable key keeps its line.
3. **Windowing, anchors and scrolling** are the lazy list's: the same `LazyListState`, the same anchor
   correction on data changes, the same fling and snapping. A grid item's main-axis offset is its
   LINE's offset.

## Behaviour

| | vertical grid | horizontal grid |
| --- | --- | --- |
| line | `Row` | `Column` |
| scroll | `vertical_scroll` | `horizontal_scroll`, mirrored under RTL |
| `spacing` | between rows | between columns |
| `cross_spacing` | between cells in a row | between cells in a column |
| adaptive basis | the viewport WIDTH | the viewport HEIGHT |

`GridCells::Adaptive` reads the cross extent from `LazyListState::viewport_cross_size()`, which the
lazy policy writes back on every measure. Before the first measure it is 0, so the first frame
composes one cell per line and the next corrects it — the same two-phase rule the lazy window follows.

## Differences from Compose

- **The windowing and reporting unit is a LINE.** `layout_info()` reports lines, and
  `Modifier::animate_item` animates a whole line. Compose windows per line too, but its
  `LazyGridItemInfo` is per ITEM, and its item placement animation is per item.
- **No `stickyHeader`.** Compose's lazy grids do not offer one either.
- **`GridCells.Adaptive` floors the cell count**, so a cell can be wider than the minimum and never
  narrower.
- **A jump takes a LINE index** (`LazyListState::scroll_to_item`), not an item index.

## Tests

`winia/src/layout/lazy/grid.rs` (`layout::lazy::grid::tests`): chunking with a fixed count, a span
that does not fit, the adaptive cell count, and an end-to-end vertical grid (two lines, three equally
weighted cells, and the anchor jumping to the second line).
