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
   height. A child is then placed inside the cell by its alignment (default: stretch both axes), and
   the whole grid is mirrored on x under RTL.
4. **Rows**: a declared row keeps its track spec; an IMPLICIT row (one the children need but
   `rows` does not declare) is as tall as the tallest child that spans exactly that one row. A child
   spanning several implicit rows contributes to the LAST of them.
5. **Size**: the tracks plus the gaps between them, floored by the incoming minimum and never clamped
   to the maximum (a child that overflows its cell grows nothing, but a declared track larger than the
   container is reported as-is — Compose's grid behaves the same way).

## Differences from Compose

- **No intrinsic track** (`GridTrackSpec.FitContent` / `GridTrackSize.Auto`): every track is fixed,
  percentage or weighted. An intrinsic track needs a second measurement pass over the children.
- **No named areas** (`areaId`): a child declares its span directly.
- **No `GridItemSpan` max-line helper**; a span is a plain `u32` and is clamped to the column count.
- **Implicit-row sizing is simplified**: a multi-row child sizes only the last row it covers instead
  of being distributed across them.
- Cell alignment defaults to STRETCH on both axes here; Compose's `gridItem` defaults to
  `Alignment.TopStart` (top-left), so a child with no explicit alignment behaves differently.

## Tests

`winia/src/layout/grid.rs` (`layout::grid::tests`, 11 cases):

| test | pins |
| --- | --- |
| `fixed_columns_place_each_child_in_its_track` | tracks left to right and the wrap into implicit rows |
| `gaps_sit_between_the_tracks` | per-axis gaps, and that they do not count at the edges |
| `percent_and_weight_share_the_bounded_axis` | 100 fixed + 50 % + weight in a 400 box |
| `an_unbounded_axis_leaves_percent_and_weight_at_zero` | the unbounded rule |
| `a_spanning_child_covers_the_tracks_it_spans` | a two-column span's width and the next child's row |
| `implicit_rows_size_to_their_tallest_child` | implicit row heights |
| `dense_flow_fills_the_hole_a_span_left` | dense vs sparse placement |
| `column_flow_fills_a_column_first` | column-major order, wrapping at the declared rows |
| `rtl_mirrors_the_columns` | RTL mirroring |
| `a_cell_alignment_places_the_child_inside_its_cell` | centred vs stretched children |
| `an_empty_grid_is_zero` | an empty grid |

UI (`winia/tests/ui_grid_test.rs`, fixture scenario `grid`, driven by `tests/ui_fixtures/fixture_all.rs`)
reads the same five cases off a real window: fixed pitch 88, weight ratio 2x, a 146-wide span, the
dense hole and a centred child. Run it with
`cargo test -p winia --features debug-server --test ui_grid_test`.

## Demo

`cargo run -p winia --example grid_demo` — five cards, one per behaviour, every cell tagged.
