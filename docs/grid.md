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
| `GridConfigurationScope.area(areaId, rows, columns)` (`:399-440`) | `Grid::areas(vec![GridArea::new(...)])` + `Modifier::grid_item_in_area(id)` |
| `GridMeasurePolicy` (`:850-925`) | `GridLayout` (`layout/grid.rs`) |

## API

The cells in the demo stretch explicitly (`grid_item_aligned(…, ContentAlignment::STRETCH)`), because
a tag's rect is then the resolved track — which is what the UI test reads.

```rust
Grid::new(vec![
    GridTrackSpec::fixed(90.0),      // a fixed 90 dp column
    GridTrackSpec::weight(1.0),      // one share of what is left
    GridTrackSpec::weight(2.0),      // two shares
    GridTrackSpec::FitContent,       // as wide as the widest item in it
])
.rows(vec![GridTrackSpec::fixed(48.0)])   // optional: declared rows, otherwise implicit
.gap(8.0)
.flow(GridFlow::Row)                       // Row | Column | RowDense | ColumnDense
.areas(vec![GridArea::new(7, (0, 1), (0, 1))])   // rows 0..=1, columns 0..=1, reserved first
.modifier(Modifier::new().fill_max_width())
.build(ctx, |ctx| {
    Card::new().build(ctx);                                        // cell (0, 0)
    Card::new().modifier(Modifier::new().grid_item(2, 1)).build(ctx);        // two columns wide
    Card::new().modifier(Modifier::new()
        .grid_item_aligned(1, 2, ContentAlignment::CENTER)).build(ctx);      // two rows tall, centred
});
```

## Layout

1. **Columns** resolve from their specs. A fixed track is its value; an INTRINSIC
   (`GridTrackSpec::FitContent`) track is as wide as the widest MAX intrinsic size among the items that
   occupy exactly it; a percentage track is that fraction of Compose's `availableTrackSpace` — the axis
   minus the gaps BETWEEN the tracks — and a weighted track shares what the fixed, percentage and
   intrinsic tracks leave of that same space, so a two-half-width grid with a gap fills its axis
   exactly. On an UNBOUNDED axis (a grid inside a horizontal scroll, say) a percentage track falls back
   to its items' max intrinsic width and a weighted one to their min content, as Compose does.
   A child spanning several tracks then runs Compose's pass 1.5 (`distributeSpanningSpace`,
   `Grid.kt:2178-2262`): when what it needs exceeds the tracks plus the gaps inside the span, the
   deficit is split evenly between the spanned tracks that can grow — intrinsic and weighted ones,
   never fixed or percentage.
2. **Placement** walks the cells in the flow's order: row-major for `Row`/`RowDense`, column-major
   for `Column`/`ColumnDense`. A `Dense` flow restarts the scan for every child, so a hole a span left
   behind is filled by a later child; a sparse flow never goes back. (Dense is winia's extension —
   Compose's `GridFlow` has only `Row` and `Column`.) A `Column` flow wraps at the DECLARED row count
   and clamps a row span to it; with NO declared rows it never wraps at all, so every child keeps
   going down the same column however tall the ones before it were (`Grid.kt:1301-1309`).
3. **Children** are measured at their CELL's width and, when the row is declared, at that row's
   height. A child is then placed inside the cell by its alignment — the default is Compose's
   `Alignment.TopStart`, so a child that does not fill its cell keeps its own size and hugs the
   cell's top-left, while `ContentAlignment::STRETCH` (or a `fill_max_size` in the child's chain)
   fills it — and the whole grid is mirrored on x under RTL.
4. **Rows**: a declared row keeps its track spec; an IMPLICIT row (one the children need but
   `rows` does not declare) is as tall as the tallest child that spans exactly that one row. A child
   spanning several implicit rows contributes to the LAST of them.
5. **Size**: the tracks plus the gaps between them, clamped into the incoming range the way Compose's
   `constrainWidth`/`constrainHeight` do (`Grid.kt:898-899`) — three 80 dp columns with two 8 dp gaps in
   a 200 dp parent report 200, not 256. Declared tracks count even when
   no child reaches them: a grid of three 80 dp columns is 240 wide with no items, and a declared row
   nobody fills still contributes its height. Under RTL the mirror lands inside the REPORTED width, so
   an incoming minimum wider than the tracks shifts the whole grid instead of leaving it off-centre.

## Differences from Compose

- **`GridFlow::RowDense` / `ColumnDense` are winia's extension**: Compose's `GridFlow` has only
  `Row` and `Column` (`Grid.kt:531-536`).
- **No `GridItemSpan` max-line helper**: a span is a plain `u32` and is clamped to the tracks that
  exist (Compose's `GridItemSpan(maxLineSpan)` means "the rest of the line").
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

`winia/src/layout/grid.rs` (`layout::grid::tests`, 29 cases):

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
| `a_percentage_grid_with_gaps_fills_its_axis` | the percentage basis excluding the gaps |
| `a_fit_content_column_sizes_to_its_items` | an intrinsic track taking the widest item |
| `a_named_area_reserves_its_cells` | an area placed before auto-placement flows around it |
| `a_spanning_item_grows_the_intrinsic_tracks_it_covers` | Compose's pass 1.5 deficit split |
| `an_intrinsic_query_reports_the_resolved_tracks` | the four intrinsic overrides |
| `an_unknown_area_id_falls_back_to_auto_placement` | an id the grid does not know |
| `an_unbounded_axis_sizes_percentage_and_weight_tracks_from_content` | the unbounded fallback |
| `a_multi_row_span_splits_its_height_between_implicit_rows` | a two-row child sharing its rows |
| `an_overflowing_grid_is_clamped_to_the_axis` | 3 x 80 + 2 x 8 inside a 200 dp parent |
| `a_column_flow_without_declared_rows_never_wraps` | a column flow past a tall child |
| `a_column_flow_clamps_a_span_to_the_declared_rows` | a span taller than the declared rows |
| `an_extreme_row_span_is_clamped` | `grid_item(1, u32::MAX)` staying finite |
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
