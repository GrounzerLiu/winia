# Grid

Place children into explicit column and row TRACKS, with spans, gaps and a flow direction.

## Sources

Compose foundation `Grid` (`reference/compose/foundation-layout-1.13.0-beta01/commonMain/androidx/compose/foundation/layout/Grid.kt`):

| Compose | here |
| --- | --- |
| `Grid(config, modifier)` (`:94`) | `Grid::new(vec![…])` |
| `GridConfigurationScope.column(size/weight/percentage)` / `row(…)` (`:341-370`) | `GridTrackSpec::fixed` / `weight` / `percent` |
| `GridConfigurationScope.gap/columnGap/rowGap` (`:444-474`) | `Grid::gap` / `column_gap` / `row_gap` |
| `GridFlow` (`:527-560`) | `GridFlow` |
| `GridScope.Modifier.gridItem(columnSpan, rowSpan, alignment)` (`:158-226`) | `Modifier::grid_item` / `grid_item_aligned` |
| `GridMeasurePolicy` (`:850-925`) | `GridLayout` (`layout/grid.rs`) |

## API

The cells in the demo stretch explicitly (`grid_item_aligned(…, ContentAlignment::STRETCH)`), because
a tag's rect is then the resolved track — which is what the UI test reads.

```rust
Grid::new(vec![
    GridTrackSpec::fixed(90.0),      // a fixed 90 dp column
    GridTrackSpec::weight(1.0),      // one share of what is left
    GridTrackSpec::weight(2.0),      // two shares
])
.rows(vec![GridTrackSpec::fixed(48.0)])   // optional: declared rows, otherwise implicit
.gap(8.0)
.flow(GridFlow::Row)                       // Row | Column | RowDense | ColumnDense
.modifier(Modifier::new().fill_max_width())
.build(ctx, |ctx| {
    Card::new().build(ctx);                                        // cell (0, 0)
    Card::new().modifier(Modifier::new().grid_item(2, 1)).build(ctx);        // two columns wide
    Card::new().modifier(Modifier::new()
        .grid_item_aligned(1, 2, ContentAlignment::CENTER)).build(ctx);      // two rows tall, centred
});
```

## Layout

1. **Columns** resolve from their specs. A fixed track is its value, a percentage track is that
   fraction of the container's maximum on the axis, and a weighted track shares what the fixed and
   percentage tracks plus the gaps leave over. On an UNBOUNDED axis the percentage and weight tracks
   are 0, as in Compose.
2. **Placement** walks the cells in the flow's order: row-major for `Row`/`RowDense`, column-major
   for `Column`/`ColumnDense`. A `Dense` flow restarts the scan for every child, so a hole a span left
   behind is filled by a later child; a sparse flow never goes back. A `Column` flow wraps at the
   DECLARED row count — with no declared rows the column never fills up and every child keeps going
   down it, which is what Compose does too.
3. **Children** are measured at their CELL's width and, when the row is declared, at that row's
   height. A child is then placed inside the cell by its alignment — the default is Compose's
   `Alignment.TopStart`, so a child that does not fill its cell keeps its own size and hugs the
   cell's top-left, while `ContentAlignment::STRETCH` (or a `fill_max_size` in the child's chain)
   fills it — and the whole grid is mirrored on x under RTL.
4. **Rows**: a declared row keeps its track spec; an IMPLICIT row (one the children need but
   `rows` does not declare) is as tall as the tallest child that spans exactly that one row. A child
   spanning several implicit rows contributes to the LAST of them.
5. **Size**: the tracks plus the gaps between them, floored by the incoming minimum and never clamped
   to the maximum (a child that overflows its cell grows nothing, but a declared track larger than the
   container is reported as-is — Compose's grid behaves the same way). Declared tracks count even when
   no child reaches them: a grid of three 80 dp columns is 240 wide with no items, and a declared row
   nobody fills still contributes its height. Under RTL the mirror lands inside the REPORTED width, so
   an incoming minimum wider than the tracks shifts the whole grid instead of leaving it off-centre.

## Differences from Compose

- **No intrinsic track** (`GridTrackSpec.FitContent` / `GridTrackSize.Auto`): every track is fixed,
  percentage or weighted. An intrinsic track needs a second measurement pass over the children.
- **No named areas** (`areaId`): a child declares its span directly.
- **Intrinsic queries fall back to the `MeasurePolicy` default** (a measure-based approximation),
  where Compose overrides all four to size its intrinsic tracks (`Grid.kt:2007-2078`). Since winia
  has no intrinsic tracks there is nothing to be exact about, but a `Grid` under a
  `Modifier.width(IntrinsicSize.Min)` parent therefore reports the largest child, not the resolved
  track sum.
- **No `GridItemSpan` max-line helper**; a span is a plain `u32` and is clamped to the column count.
- **Implicit-row sizing is simplified**: a multi-row child sizes only the last row it covers instead
  of being distributed across them.
- **A child in an IMPLICIT row is measured against the container's own maximum height**, not against
  infinity, so a `fill_max_height` child there grows the row to the container's height. Compose hands
  an auto row `Constraints(maxHeight = Infinity)`, which makes such a child wrap; winia avoids the
  infinity because its own `fill_max_height` treats `f32::MAX` as a real bound.
- **A column flow that runs out of declared cells keeps placing into implicit ROWS** below the
  declared ones; Compose would add implicit COLUMNS to the right instead. Both keep every child in its
  own cell.
- A cell's default alignment is Compose's `Alignment.TopStart`; winia adds the `STRETCH` value on
  top of it, which Compose expresses with `fillMaxSize()` inside the item instead.

## Tests

`winia/src/layout/grid.rs` (`layout::grid::tests`, 11 cases):

| test | pins |
| --- | --- |
| `fixed_columns_place_each_child_in_its_track` | tracks left to right and the wrap into implicit rows |
| `gaps_sit_between_the_tracks` | per-axis gaps, and that they do not count at the edges |
| `percent_and_weight_share_the_bounded_axis` | 100 fixed + 50 % + weight in a 400 box |
| `an_unbounded_axis_leaves_percent_and_weight_at_zero` | the unbounded rule |
| `a_spanning_child_covers_the_tracks_it_spans` | a two-column span keeps the child's size (TopStart) and the next child's row |
| `implicit_rows_size_to_their_tallest_child` | implicit row heights |
| `dense_flow_fills_the_hole_a_span_left` | dense vs sparse placement |
| `column_flow_fills_a_column_first` | column-major order, wrapping at the declared rows |
| `rtl_mirrors_the_columns` | RTL mirroring (stretched children, so the assertion is about the tracks) |
| `a_cell_alignment_places_the_child_inside_its_cell` | centred, the TopStart default and an explicit STRETCH |
| `a_span_wider_than_the_grid_is_clamped` | a 5-column span inside a 2-column grid |
| `a_negative_gap_is_clamped_in_measure` | a negative gap set straight on the public field |
| `more_children_than_declared_cells_never_overlap` | a column flow past the declared cell count (the regression behind the review) |
| `a_grid_without_children_reports_its_declared_tracks` | the tracks of an empty grid |
| `an_unused_declared_row_still_counts` | a declared row no child reaches |
| `a_grid_without_columns_places_every_child` | one placement per child even with no columns (the engine calls `place` unconditionally) |
| `a_tall_row_span_does_not_swallow_a_later_child` | the placement fallback walking past an eight-row span |
| `a_negative_gap_does_not_inflate_the_weighted_track` | the gap clamp running before the tracks resolve |

UI (`winia/tests/ui_grid_test.rs`, fixture scenario `grid`, driven by `tests/ui_fixtures/fixture_all.rs`)
reads the same five cases off a real window: fixed pitch 88, weight ratio 2x, a 146-wide span, the
dense hole and a centred child. Run it with
`cargo test -p winia --features debug-server --test ui_grid_test`.

## Demo

`cargo run -p winia --example grid_demo` — five cards, one per behaviour, every cell tagged.
