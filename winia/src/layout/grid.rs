//! Grid — Compose's `Grid` (`Grid.kt`, foundation-layout 1.13).
//!
//! A grid places its children into explicit column and row TRACKS. Each track is fixed logical
//! pixels, a percentage of the container, or a weight that shares what the fixed and percentage
//! tracks left over; a child may span several tracks, and the gaps between tracks are separate
//! values per axis.
//!
//! Auto-placement follows Compose's `GridFlow` (`Grid.kt:527`): `Row` fills one row at a time,
//! `Column` one column at a time, and the `Dense` variants restart the scan for every child so holes
//! left by a span get filled by a later, smaller one.
//!
//! # v1 scope (differences from Compose, recorded here and in `docs/grid.md`)
//!
//! - `GridTrackSpec::FitContent` (an intrinsic track) is not implemented: every track is
//!   Fixed, Percent or Weight. Compose resolves an intrinsic track from the items' intrinsic sizes,
//!   which needs a second measurement pass over every child.
//! - Rows are declared or implicit. A declared row uses its track spec; an IMPLICIT row (a row the
//!   children need but `Grid::rows` does not declare) is sized by the tallest child that spans exactly
//!   that one row. A child spanning several implicit rows contributes to the LAST of them, which is a
//!   simplification of Compose's distribution.
//! - `GridScope.gridItem(areaId, alignment)` (named areas) is not implemented; a child declares its
//!   span with `Modifier::grid_item(column_span, row_span)`.
//! - A child is measured at its CELL's width and, for a declared row, at that row's height; it is then
//!   aligned inside the cell with `Modifier::grid_item_aligned` (default: stretch both axes, as Compose's
//!   `Alignment.TopStart` default is replaced here by the grid's own fill behaviour).

use super::container::build_container;
use super::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::layout::{ContentAlignment, Constraints, LayoutDirection};
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
use crate::unit::{Offset, Size};

// ── Track specs ──

/// The size of one column or row track — Compose's `GridTrackSpec` (`Grid.kt:571-750`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridTrackSpec {
    /// A fixed logical-pixel size — Compose's `column(100.dp)` / `row(100.dp)`.
    Fixed(f32),
    /// A fraction of the container's available space on that axis — Compose's
    /// `column(0.5f)` / `row(0.5f)`.
    Percent(f32),
    /// A share of the space the fixed and percentage tracks left over — Compose's
    /// `column(1.fr)`. On an unbounded axis (or one with nothing left over) a weighted track is 0.
    Weight(f32),
}

impl GridTrackSpec {
    /// A fixed track of `px` logical pixels.
    pub fn fixed(px: f32) -> Self {
        GridTrackSpec::Fixed(px)
    }

    /// A track taking `fraction` of the container's available space on that axis.
    pub fn percent(fraction: f32) -> Self {
        GridTrackSpec::Percent(fraction)
    }

    /// A track sharing what the other tracks leave over, in proportion to `weight`.
    pub fn weight(weight: f32) -> Self {
        GridTrackSpec::Weight(weight)
    }
}

// ── Flow ──

/// Where auto-placement looks for the next free cell — Compose's `GridFlow` (`Grid.kt:527-560`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridFlow {
    /// Fill a row before moving to the next one (Compose's default).
    #[default]
    Row,
    /// Fill a column before moving to the next one.
    Column,
    /// Like `Row`, but every child restarts the scan from the first cell, so a hole left by a
    /// span is filled by a later one.
    RowDense,
    /// Like `Column`, but every child restarts the scan from the first cell.
    ColumnDense,
}

impl GridFlow {
    fn dense(self) -> bool {
        matches!(self, GridFlow::RowDense | GridFlow::ColumnDense)
    }

    fn by_column(self) -> bool {
        matches!(self, GridFlow::Column | GridFlow::ColumnDense)
    }
}

// ── The policy ──

/// Grid's measure policy (Compose's `GridMeasurePolicy`, `Grid.kt:850-925`).
#[derive(Debug, Clone)]
pub struct GridLayout {
    /// The column tracks, left to right (right to left under RTL). An empty list produces an empty grid.
    pub columns: Vec<GridTrackSpec>,
    /// The declared row tracks, top to bottom. Rows beyond this list are implicit and content-sized.
    pub rows: Vec<GridTrackSpec>,
    /// The gap between rows, in logical pixels.
    pub row_gap: f32,
    /// The gap between columns, in logical pixels.
    pub column_gap: f32,
    /// Where auto-placement looks next.
    pub flow: GridFlow,
    /// The direction the final placements are mirrored for.
    pub direction: LayoutDirection,
}

impl GridLayout {
    /// A grid with the given column tracks and nothing else.
    pub fn new(columns: Vec<GridTrackSpec>) -> Self {
        GridLayout {
            columns,
            rows: Vec::new(),
            row_gap: 0.0,
            column_gap: 0.0,
            flow: GridFlow::Row,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn rows(mut self, rows: Vec<GridTrackSpec>) -> Self {
        self.rows = rows;
        self
    }

    /// The gap on both axes.
    pub fn gap(mut self, gap: f32) -> Self {
        self.row_gap = gap.max(0.0);
        self.column_gap = gap.max(0.0);
        self
    }

    pub fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap.max(0.0);
        self
    }

    pub fn column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap.max(0.0);
        self
    }

    pub fn flow(mut self, flow: GridFlow) -> Self {
        self.flow = flow;
        self
    }

    pub fn direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }
}

/// Resolves one axis's tracks into absolute sizes.
///
/// `available` is the container's maximum on that axis; an unbounded axis leaves the
/// percentage and weight tracks at zero, the way Compose gives a weighted track nothing when the
/// axis is infinite.
fn resolve_tracks(specs: &[GridTrackSpec], available: f32, gap: f32) -> Vec<f32> {
    let gaps = gap * specs.len().saturating_sub(1) as f32;
    let bounded = available < f32::MAX;
    let basis = if bounded { available } else { 0.0 };
    let mut fixed_total = 0.0f32;
    let mut percent_total = 0.0f32;
    let mut weight_total = 0.0f32;
    for spec in specs {
        match *spec {
            GridTrackSpec::Fixed(px) => fixed_total += px.max(0.0),
            GridTrackSpec::Percent(fraction) => percent_total += fraction.max(0.0),
            GridTrackSpec::Weight(weight) => weight_total += weight.max(0.0),
        }
    }
    let remaining = if bounded {
        (available - fixed_total - percent_total * basis - gaps).max(0.0)
    } else {
        0.0
    };
    specs
        .iter()
        .map(|spec| match *spec {
            GridTrackSpec::Fixed(px) => px.max(0.0),
            GridTrackSpec::Percent(fraction) => fraction.max(0.0) * basis,
            GridTrackSpec::Weight(weight) => {
                if weight_total > 0.0 {
                    remaining * (weight.max(0.0) / weight_total)
                } else {
                    0.0
                }
            }
        })
        .collect()
}

/// One child's occupied cells, in track indices.
#[derive(Debug, Clone, Copy)]
struct Cell {
    column: usize,
    column_span: usize,
    row: usize,
    row_span: usize,
}

/// A growable occupancy map: `rows[row][column]`.
struct Occupancy {
    rows: Vec<Vec<bool>>,
}

impl Occupancy {
    fn new() -> Self {
        Occupancy { rows: Vec::new() }
    }

    fn is_free(&self, row: usize, column: usize) -> bool {
        self.rows
            .get(row)
            .and_then(|r| r.get(column))
            .copied()
            .unwrap_or(false)
            == false
    }

    fn fits(&self, row: usize, column: usize, row_span: usize, column_span: usize, columns: usize) -> bool {
        if column + column_span > columns {
            return false;
        }
        for r in row..row + row_span {
            for c in column..column + column_span {
                if !self.is_free(r, c) {
                    return false;
                }
            }
        }
        true
    }

    fn reserve(&mut self, row: usize, column: usize, row_span: usize, column_span: usize) {
        while self.rows.len() < row + row_span {
            self.rows.push(vec![false; 0]);
        }
        for r in row..row + row_span {
            let line = &mut self.rows[r];
            if line.len() < column + column_span {
                line.resize(column + column_span, false);
            }
            for c in column..column + column_span {
                line[c] = true;
            }
        }
    }
}

/// Auto-placement: gives every child its cells. See [`GridFlow`].
fn place_cells(
    spans: &[(u32, u32)],
    columns: usize,
    flow: GridFlow,
    declared_rows: Option<usize>,
) -> Vec<Cell> {
    let mut occupancy = Occupancy::new();
    let mut out = Vec::with_capacity(spans.len());
    let mut cursor = (0usize, 0usize);
    // A column flow wraps at the DECLARED row count; with no declared rows the rows are unbounded
    // and every child keeps filling the current column. A row flow grows rows freely either way, so
    // the bound there only caps the defensive scan.
    let row_bound = match declared_rows {
        Some(rows) if flow.by_column() => rows.max(1),
        _ => spans.len() + 1,
    };
    for &(column_span, row_span) in spans {
        // A span wider than the grid is clamped instead of pushed out of it.
        let column_span = (column_span.max(1) as usize).min(columns);
        let row_span = row_span.max(1) as usize;
        let start = if flow.dense() { (0, 0) } else { cursor };
        let (row, column) = find_cell(
            &occupancy,
            start,
            row_span,
            column_span,
            columns,
            flow.by_column(),
            row_bound,
        );
        occupancy.reserve(row, column, row_span, column_span);
        out.push(Cell { column, column_span, row, row_span });
        cursor = if flow.by_column() {
            (row + row_span, column)
        } else {
            (row, column + column_span)
        };
    }
    out
}

/// Scans for the first free rectangle of `row_span` x `column_span` cells, starting at `start` and
/// walking in the flow's order. Falls back to the first free cell in the last row when the bounded
/// scan finds nothing (only reachable with a span wider than the grid, which the builder rejects).
fn find_cell(
    occupancy: &Occupancy,
    start: (usize, usize),
    row_span: usize,
    column_span: usize,
    columns: usize,
    by_column: bool,
    row_bound: usize,
) -> (usize, usize) {
    let mut k = 0usize;
    // In a column flow the rows are a fixed set that the scan wraps around; in a row flow they grow.
    let row_mod = if by_column { row_bound.max(1) } else { row_bound + 1 };
    loop {
        let (row, column) = if by_column {
            // Column-major: walk the rows of one column, then the next column.
            let linear = start.0 + k;
            let column = start.1 + linear / row_mod;
            let row = linear % row_mod;
            (row, column)
        } else {
            // Row-major: walk the columns of one row, then the next row.
            let linear = start.1 + k;
            let row = start.0 + linear / columns;
            let column = linear % columns;
            (row, column)
        };
        if column + column_span <= columns
            && occupancy.fits(row, column, row_span, column_span, columns)
        {
            return (row, column);
        }
        k += 1;
        if k > (row_mod + 1) * columns {
            break;
        }
    }
    // Only reachable for a span that cannot fit the grid at all; the caller clamps the span, so
    // this is a defensive fallback rather than a normal path.
    (row_bound, 0)
}

impl MeasurePolicy for GridLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let n = children.len();
        let columns = self.columns.len();
        if n == 0 || columns == 0 {
            return (Size::ZERO, Vec::new());
        }

        // 1. The column widths. They do not depend on the children (no intrinsic tracks in v1).
        let widths = resolve_tracks(&self.columns, constraints.max_width, self.column_gap);

        // 2. Where every child goes.
        let spans: Vec<(u32, u32)> = children
            .iter()
            .map(|&c| {
                nodes[c]
                    .modifier
                    .get_grid_item()
                    .map(|(column_span, row_span, _)| (column_span, row_span))
                    .unwrap_or((1, 1))
            })
            .collect();
        let declared_rows = if self.rows.is_empty() { None } else { Some(self.rows.len()) };
        let cells = place_cells(&spans, columns, self.flow, declared_rows);

        // 3. The row tracks: declared ones resolve from their specs, implicit ones start at zero and
        // grow to the tallest child that spans exactly them (step 4).
        let row_count = cells.iter().map(|c| c.row + c.row_span).max().unwrap_or(0);
        let declared_heights = resolve_tracks(&self.rows, constraints.max_height, self.row_gap);
        let declared_table = offset_table(&declared_heights, self.row_gap);

        // 4. Measure each child at its cell's width, and at its row's height when that row is declared
        // (an implicit row has no height until the children say what it is, so the container's own
        // maximum is the limit there).
        let column_offsets = offset_table(&widths, self.column_gap);
        let mut child_sizes = Vec::with_capacity(n);
        for (i, &child) in children.iter().enumerate() {
            let cell = cells[i];
            let left = column_offsets[cell.column];
            let width = (column_offsets[cell.column + cell.column_span] - left - self.column_gap)
                .max(0.0);
            let last_declared = cell.row + cell.row_span <= declared_heights.len();
            let height = if last_declared {
                (declared_table[cell.row + cell.row_span] - declared_table[cell.row] - self.row_gap)
                    .max(0.0)
            } else {
                constraints.max_height
            };
            let (size, _) =
                measure_node(nodes, policies, child, Constraints::new(0.0, width, 0.0, height));
            child_sizes.push(size);
        }

        // 5. The row heights: the declared tracks, then the implicit rows from their children.
        let mut heights = declared_heights.clone();
        heights.resize(row_count, 0.0);
        for (i, cell) in cells.iter().enumerate() {
            if cell.row + cell.row_span <= self.rows.len() {
                // A declared row keeps its track spec; a child taller than it overflows the cell.
                continue;
            }
            // A child spanning several implicit rows contributes to the LAST one (module docs).
            let target = cell.row + cell.row_span - 1;
            heights[target] = heights[target].max(child_sizes[i].height);
        }

        // 6. Place: the cell's origin, then the child's alignment inside it, then the RTL mirror.
        let row_offsets = offset_table(&heights, self.row_gap);
        let total_width = column_offsets[columns] - self.column_gap;
        let total_height = row_offsets[row_count] - self.row_gap;
        let mut placements = Vec::with_capacity(n);
        for (i, cell) in cells.iter().enumerate() {
            let cell_width = column_offsets[cell.column + cell.column_span]
                - column_offsets[cell.column]
                - self.column_gap;
            let cell_height = row_offsets[cell.row + cell.row_span]
                - row_offsets[cell.row]
                - self.row_gap;
            let default_alignment = ContentAlignment::STRETCH;
            let alignment = nodes[children[i]]
                .modifier
                .get_grid_item()
                .and_then(|(_, _, alignment)| alignment)
                .unwrap_or(default_alignment);
            let space = Size::new(cell_width.max(0.0), cell_height.max(0.0));
            let size = alignment.child_size(child_sizes[i], space);
            // The anchor is pure geometry here: the RTL flip is applied once, to the finished x.
            let (dx, dy) = alignment.anchor(size, space, LayoutDirection::Ltr);
            let mut x = column_offsets[cell.column] + dx;
            let y = row_offsets[cell.row] + dy;
            if self.direction == LayoutDirection::Rtl {
                x = total_width - x - size.width;
            }
            placements.push(Placement { size, position: Offset::new(x, y) });
        }

        let measured = Size::new(
            total_width.max(constraints.min_width),
            total_height.max(constraints.min_height),
        );
        (measured, placements)
    }
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &child) in children.iter().enumerate() {
            nodes[child].position = placements[i].position;
            nodes[child].measured_size = placements[i].size;
        }
    }
}

/// The running offset of every track boundary: `out[i]` is where track `i` starts, and
/// `out[len]` is the end of the last track plus one gap.
fn offset_table(sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(sizes.len() + 1);
    let mut running = 0.0;
    for size in sizes {
        out.push(running);
        running += size.max(0.0) + gap;
    }
    // `out[i]` is where track i starts, so `out[n]` is one gap past the last track's end: a cell
    // spanning [c, c+s) is `out[c + s] - out[c] - gap` wide, and the container is `out[n] - gap`.
    out.push(running);
    out
}

// ── The builder ──

/// A grid of explicit tracks — Compose's `Grid` (`Grid.kt:94`).
///
/// ```ignore
/// Grid::new(vec![GridTrackSpec::fixed(80.0), GridTrackSpec::weight(1.0)])
///     .gap(8.0)
///     .build(ctx, |ctx| {
///         Card::new().build(ctx);                       // cell (0, 0)
///         Card::new().modifier(Modifier::new().grid_item(1, 2)).build(ctx); // two rows tall
///     });
/// ```
pub struct Grid {
    columns: Vec<GridTrackSpec>,
    rows: Vec<GridTrackSpec>,
    row_gap: f32,
    column_gap: f32,
    flow: GridFlow,
    modifier: Modifier,
}

impl Grid {
    /// A grid with the given column tracks and implicit (content-sized) rows.
    pub fn new(columns: Vec<GridTrackSpec>) -> Self {
        Grid {
            columns,
            rows: Vec::new(),
            row_gap: 0.0,
            column_gap: 0.0,
            flow: GridFlow::Row,
            modifier: Modifier::new(),
        }
    }

    /// Declares the row tracks. Rows beyond the list stay implicit.
    pub fn rows(mut self, rows: Vec<GridTrackSpec>) -> Self {
        self.rows = rows;
        self
    }

    /// The gap on both axes.
    pub fn gap(mut self, gap: f32) -> Self {
        self.row_gap = gap.max(0.0);
        self.column_gap = gap.max(0.0);
        self
    }

    /// The gap between rows.
    pub fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap.max(0.0);
        self
    }

    /// The gap between columns.
    pub fn column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap.max(0.0);
        self
    }

    /// Where auto-placement looks next (Compose's `flow`).
    pub fn flow(mut self, flow: GridFlow) -> Self {
        self.flow = flow;
        self
    }

    /// Appends a modifier. The caller's chain comes last, so a `fill_max_width` or a
    /// `layout_weight` in it can still take effect.
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // Declare every parameter that decides the layout, so an unchanged grid Skips.
        ctx.changed(&self.columns);
        ctx.changed(&self.rows);
        ctx.changed(&self.row_gap);
        ctx.changed(&self.column_gap);
        ctx.changed(&self.flow);
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        build_container(
            ctx,
            self.modifier,
            GridLayout::new(self.columns)
                .rows(self.rows)
                .row_gap(self.row_gap)
                .column_gap(self.column_gap)
                .flow(self.flow)
                .direction(direction),
            content,
        );
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::Size;

    fn make_leaf(width: f32, height: f32) -> LayoutNode {
        let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
        node.measured_size = Size::new(width, height);
        node
    }

    fn spanning_leaf(width: f32, height: f32, column_span: u32, row_span: u32) -> LayoutNode {
        let mut node = LayoutNode::leaf(
            Modifier::new().size(width, height).grid_item(column_span, row_span),
        );
        node.measured_size = Size::new(width, height);
        node
    }

    fn run(policy: &GridLayout, nodes: &mut Vec<LayoutNode>, constraints: Constraints) -> (Size, Vec<Placement>) {
        let children: Vec<usize> = (0..nodes.len()).collect();
        policy.measure(nodes, &[], &children, constraints)
    }

    /// Fixed columns: each child goes into its own track, and the tracks wrap into implicit rows.
    #[test]
    fn fixed_columns_place_each_child_in_its_track() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(70.0)]);
        let mut nodes: Vec<LayoutNode> = (0..4).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(120.0, 20.0));
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0));
        assert_eq!(placements[1].position, Offset::new(50.0, 0.0));
        assert_eq!(placements[2].position, Offset::new(0.0, 10.0));
        assert_eq!(placements[3].position, Offset::new(50.0, 10.0));
    }

    /// The gaps are separate per axis and count between tracks, not at the edges.
    #[test]
    fn gaps_sit_between_the_tracks() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(70.0)])
            .column_gap(10.0)
            .row_gap(4.0);
        let mut nodes: Vec<LayoutNode> = (0..3).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size, Size::new(130.0, 24.0), "50 + 10 + 70 wide, 10 + 4 + 10 tall");
        assert_eq!(placements[1].position, Offset::new(60.0, 0.0));
        assert_eq!(placements[2].position, Offset::new(0.0, 14.0));
    }

    /// Percent takes a fraction of the axis, weight shares what is left over after the other tracks.
    #[test]
    fn percent_and_weight_share_the_bounded_axis() {
        let policy = GridLayout::new(vec![
            GridTrackSpec::fixed(100.0),
            GridTrackSpec::percent(0.5),
            GridTrackSpec::weight(1.0),
        ]);
        let mut nodes: Vec<LayoutNode> = (0..3).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 100.0));
        assert_eq!(size.width, 400.0, "100 fixed + 200 percent + 100 left over for the weight");
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 100.0);
        assert_eq!(placements[2].position.x, 300.0);
    }

    /// An unbounded axis gives the percentage and weight tracks nothing, as Compose does.
    #[test]
    fn an_unbounded_axis_leaves_percent_and_weight_at_zero() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(40.0), GridTrackSpec::weight(1.0)]);
        let mut nodes: Vec<LayoutNode> = (0..2).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::UNBOUNDED);
        assert_eq!(size.width, 40.0);
        assert_eq!(placements[1].position.x, 40.0);
    }

    /// A child that spans two columns is measured at their combined width and sits across both.
    #[test]
    fn a_spanning_child_covers_the_tracks_it_spans() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(70.0)])
            .column_gap(10.0);
        let mut nodes = vec![spanning_leaf(40.0, 20.0, 2, 1), make_leaf(10.0, 10.0)];
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(placements[0].size, Size::new(130.0, 20.0), "stretched across 50 + 10 + 70");
        assert_eq!(placements[1].position, Offset::new(0.0, 20.0), "the next child starts a new row");
    }

    /// An implicit row is as tall as the tallest child that spans exactly that row.
    #[test]
    fn implicit_rows_size_to_their_tallest_child() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)]);
        let mut nodes = vec![make_leaf(10.0, 30.0), make_leaf(10.0, 12.0), make_leaf(10.0, 8.0)];
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size.height, 38.0, "row 1 is 30 tall, row 2 is 8");
        assert_eq!(placements[2].position.y, 30.0);
    }

    /// Dense flow restarts the scan for every child, so a hole left by a span gets filled by a later
    /// child that still fits it. A sparse scan never goes back.
    #[test]
    fn dense_flow_fills_the_hole_a_span_left() {
        // Three columns; the second child needs two, so it drops to the next row and leaves (0, 2)
        // free. The third child is single-cell and fits that hole.
        let columns = vec![
            GridTrackSpec::fixed(50.0),
            GridTrackSpec::fixed(50.0),
            GridTrackSpec::fixed(50.0),
        ];
        let mut sparse_nodes = vec![
            spanning_leaf(80.0, 10.0, 2, 1),
            spanning_leaf(80.0, 10.0, 2, 1),
            make_leaf(10.0, 10.0),
        ];
        let sparse = GridLayout::new(columns.clone()).flow(GridFlow::Row);
        let (_, sparse_placements) =
            run(&sparse, &mut sparse_nodes, Constraints::new(0.0, 300.0, 0.0, 300.0));

        let mut dense_nodes = vec![
            spanning_leaf(80.0, 10.0, 2, 1),
            spanning_leaf(80.0, 10.0, 2, 1),
            make_leaf(10.0, 10.0),
        ];
        let dense = GridLayout::new(columns).flow(GridFlow::RowDense);
        let (_, dense_placements) =
            run(&dense, &mut dense_nodes, Constraints::new(0.0, 300.0, 0.0, 300.0));

        assert_eq!(
            sparse_placements[2].position,
            Offset::new(100.0, 10.0),
            "sparse keeps walking forward from the previous child"
        );
        assert_eq!(
            dense_placements[2].position,
            Offset::new(100.0, 0.0),
            "dense restarts the scan and fills the hole in row 0"
        );
    }

    /// Column flow fills a column before moving right. The row count has to be declared: with
    /// unbounded rows a column never fills up, exactly as in Compose.
    #[test]
    fn column_flow_fills_a_column_first() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)])
            .rows(vec![GridTrackSpec::fixed(10.0), GridTrackSpec::fixed(10.0)])
            .flow(GridFlow::Column);
        let mut nodes: Vec<LayoutNode> = (0..3).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(100.0, 20.0));
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0));
        assert_eq!(placements[1].position, Offset::new(0.0, 10.0), "the second child goes BELOW the first");
        assert_eq!(placements[2].position, Offset::new(50.0, 0.0), "the third starts the second column");
    }

    /// RTL mirrors the whole grid on the x axis.
    #[test]
    fn rtl_mirrors_the_columns() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(70.0)])
            .direction(LayoutDirection::Rtl);
        let mut nodes: Vec<LayoutNode> = (0..2).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size.width, 120.0);
        assert_eq!(placements[0].position.x, 70.0, "the first column is the rightmost");
        assert_eq!(placements[1].position.x, 0.0);
    }

    /// The cell alignment places the child inside its cell; the default stretches it.
    #[test]
    fn a_cell_alignment_places_the_child_inside_its_cell() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(100.0)]);
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new()
                .size(20.0, 10.0)
                .grid_item_aligned(1, 1, ContentAlignment::CENTER),
        )];
        nodes[0].measured_size = Size::new(20.0, 10.0);
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].size, Size::new(20.0, 10.0), "a centred child keeps its size");
        assert_eq!(placements[0].position, Offset::new(40.0, 0.0), "centred in the 100-wide cell");

        let mut stretched = vec![make_leaf(20.0, 10.0)];
        let (_, placements) = run(&policy, &mut stretched, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].size, Size::new(100.0, 10.0), "the default stretches the cell width");
    }


    #[test]
    fn an_empty_grid_is_zero() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0)]);
        let mut nodes: Vec<LayoutNode> = Vec::new();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::ZERO);
        assert!(placements.is_empty());
    }
}
