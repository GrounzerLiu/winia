//! Grid — Compose's `Grid` (`Grid.kt`, foundation-layout 1.13).
//!
//! A grid places its children into explicit column and row TRACKS. Each track is fixed logical
//! pixels, a percentage of the container, or a weight that shares what the fixed and percentage
//! tracks left over; a child may span several tracks, and the gaps between tracks are separate
//! values per axis.
//!
//! Auto-placement follows Compose's `GridFlow` (`Grid.kt:527-560`): `Row` fills one row at a time and
//! `Column` one column at a time. Compose has only those two; the `Dense` variants are winia's
//! extension, and they restart the scan for every child so a hole a span left is filled by a later,
//! smaller one.
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
//! - Named areas ARE implemented (`Grid::areas` + `Modifier::grid_item_in_area`); the Compose form with
//!   a per-item `alignment` inside the area is not — pass the alignment through `grid_item_aligned`.
//! - A child declares its
//!   span with `Modifier::grid_item(column_span, row_span)`.
//! - A child is measured at its CELL's width and, for a declared row, at that row's height; it is then
//!   aligned inside the cell with `Modifier::grid_item_aligned`. The default is Compose's
//!   `Alignment.TopStart` (`Grid.kt:158-226`), so a child that does not fill its cell sits at the
//!   cell's top-left unless its own chain says otherwise.

use super::container::build_container;
use super::node::{intrinsic_size_of, measure_node, IntrinsicQuery, LayoutNode, MeasurePolicy, Placement};
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
    /// `column(1.fr)`. On a bounded axis it takes a share of what is left over; on an UNBOUNDED axis
    /// it falls back to its items' smallest intrinsic size (Compose starts a flex track at
    /// min-content, `Grid.kt:1833-1845`).
    Weight(f32),
    /// Sized by the items in the track — Compose's intrinsic (auto) track, `GridTrackSize` with no
    /// fixed value. Its width is the largest MAX intrinsic width among the items that occupy exactly
    /// that track (an item spanning several tracks does not size them, which is the v1 rule recorded in
    /// the module header).
    FitContent,
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

// ── Named areas ──

/// A named block of cells — Compose's `GridConfigurationScope.area(areaId, rows, columns)`
/// (`Grid.kt:399-440`). Both ranges are INCLUSIVE, as Compose's `IntRange` is.
///
/// A child declares the area with `Modifier::grid_item_in_area(id)`; the grid reserves the cells before
/// auto-placement runs, so the rest of the children flow around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridArea {
    /// The identifier a child names through `Modifier::grid_item_in_area`.
    pub id: u64,
    /// The first and last row, inclusive.
    pub rows: (u32, u32),
    /// The first and last column, inclusive.
    pub columns: (u32, u32),
}

impl GridArea {
    pub fn new(id: u64, rows: (u32, u32), columns: (u32, u32)) -> Self {
        GridArea { id, rows, columns }
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
    /// Named areas, reserved before auto-placement runs.
    pub areas: Vec<GridArea>,
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
            areas: Vec::new(),
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

    pub fn areas(mut self, areas: Vec<GridArea>) -> Self {
        self.areas = areas;
        self
    }

    pub fn direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }
}

/// Resolves one axis's tracks into absolute sizes.
///
/// `available` is the container's maximum on that axis. Compose's `availableTrackSpace` is that
/// minus the gaps BETWEEN the tracks (`Grid.kt:634-639`), and both the percentage and the weight
/// tracks size against it — a two-half-width grid with a 10 px gap therefore fills its axis exactly
/// instead of overflowing by the gap.
///
/// An unbounded axis (a scroll container hands its content `f32::MAX`) gives the percentage and
/// weight tracks ZERO. Compose falls back to the items' max intrinsic width for a percentage track
/// and to min-content for a weight track; winia has no intrinsic tracks in v1, and measuring children
/// against `f32::MAX` would let a `fill_max_width` child explode. Recorded as a difference in
/// `docs/grid.md`.
fn resolve_tracks(specs: &[GridTrackSpec], available: f32, gap: f32) -> Vec<f32> {
    resolve_axis(specs, available, gap, false, |_| (0.0, 0.0))
}

/// Resolves one axis with an intrinsic fallback: `intrinsic(i)` answers `(max, min)` for track `i`,
/// taken from the items that occupy exactly it.
fn resolve_axis(
    specs: &[GridTrackSpec],
    available: f32,
    gap: f32,
    // `true` answers a MIN intrinsic query: the intrinsic-driven tracks take the items' minimum
    // instead of their maximum.
    prefer_min: bool,
    intrinsic: impl Fn(usize) -> (f32, f32),
) -> Vec<f32> {
    let gaps = gap * specs.len().saturating_sub(1) as f32;
    let bounded = available < f32::MAX;
    let basis = if bounded { (available - gaps).max(0.0) } else { 0.0 };
    let mut fixed_total = 0.0f32;
    let mut percent_total = 0.0f32;
    let mut weight_total = 0.0f32;
    for spec in specs {
        match *spec {
            GridTrackSpec::Fixed(px) => fixed_total += px.max(0.0),
            GridTrackSpec::Percent(fraction) => percent_total += fraction.max(0.0),
            GridTrackSpec::Weight(weight) => weight_total += weight.max(0.0),
            // An intrinsic track sizes from its items, so it takes nothing out of the axis up front.
            GridTrackSpec::FitContent => {}
        }
    }
    // Fixed, percentage and intrinsic tracks first: the weights share whatever they leave over.
    let mut sizes = vec![0.0f32; specs.len()];
    let mut intrinsic_total = 0.0f32;
    for (i, spec) in specs.iter().enumerate() {
        match *spec {
            GridTrackSpec::Fixed(px) => sizes[i] = px.max(0.0),
            GridTrackSpec::Percent(fraction) => {
                sizes[i] = if bounded {
                    fraction.max(0.0) * basis
                } else {
                    // Compose sizes a percentage track by its items' intrinsic width when the axis is
                    // unbounded, max for a max query and min for a min one (`Grid.kt:1645-1663`).
                    let (max, min) = intrinsic(i);
                    if prefer_min { min.max(0.0) } else { max.max(0.0) }
                };
            }
            GridTrackSpec::Weight(_) => {
                if !bounded {
                    // ... and starts a flex track at min-content (`Grid.kt:1833-1845`).
                    sizes[i] = intrinsic(i).1.max(0.0);
                }
            }
            GridTrackSpec::FitContent => {
                let (max, min) = intrinsic(i);
                sizes[i] = if prefer_min { min.max(0.0) } else { max.max(0.0) };
                intrinsic_total += sizes[i];
            }
        }
    }
    let remaining = if bounded {
        (basis - fixed_total - percent_total * basis - intrinsic_total).max(0.0)
    } else {
        0.0
    };
    if weight_total > 0.0 && bounded {
        for (i, spec) in specs.iter().enumerate() {
            if let GridTrackSpec::Weight(weight) = *spec {
                sizes[i] = remaining * (weight.max(0.0) / weight_total);
            }
        }
    }
    sizes
}

/// The `(max, min)` intrinsic of the items that occupy exactly one track of the given axis.
fn track_intrinsic(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    children: &[usize],
    cells: &[Cell],
    track: usize,
    by_row: bool,
    other: impl Fn(&Cell) -> f32,
) -> (f32, f32) {
    use crate::layout::{intrinsic_size_of, IntrinsicQuery};
    let (max_query, min_query) = if by_row {
        (IntrinsicQuery::MaxHeight, IntrinsicQuery::MinHeight)
    } else {
        (IntrinsicQuery::MaxWidth, IntrinsicQuery::MinWidth)
    };
    let mut max = 0.0f32;
    let mut min = 0.0f32;
    for (i, &child) in children.iter().enumerate() {
        let cell = cells[i];
        let single = if by_row {
            cell.row_span == 1 && cell.row == track
        } else {
            cell.column_span == 1 && cell.column == track
        };
        if !single {
            continue;
        }
        let other_value = other(&cell);
        max = max.max(intrinsic_size_of(nodes, policies, child, max_query, other_value));
        min = min.max(intrinsic_size_of(nodes, policies, child, min_query, other_value));
    }
    (max, min)
}

/// A defensive ceiling on a row span inside a grid with implicit rows (see `place_cells`).
const MAX_IMPLICIT_ROW_SPAN: usize = 4096;

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
    areas: &[GridArea],
    area_ids: &[Option<u64>],
) -> Vec<Cell> {
    let mut occupancy = Occupancy::new();
    let mut out: Vec<Cell> = Vec::with_capacity(spans.len());
    // Named areas claim their cells FIRST, so everything auto-placed flows around them.
    let mut claimed: Vec<Option<Cell>> = vec![None; spans.len()];
    for (i, area_id) in area_ids.iter().enumerate() {
        let Some(id) = area_id else { continue };
        let Some(area) = areas.iter().find(|area| area.id == *id) else {
            continue; // An unknown id falls back to auto-placement.
        };
        let column = (area.columns.0 as usize).min(columns.saturating_sub(1));
        let last_column = (area.columns.1 as usize).min(columns.saturating_sub(1));
        let column_span = last_column.saturating_sub(column) + 1;
        let row = area.rows.0 as usize;
        let row_span = (area.rows.1 as usize).saturating_sub(row) + 1;
        occupancy.reserve(row, column, row_span, column_span);
        claimed[i] = Some(Cell { column, column_span, row, row_span });
    }
    let mut cursor = (0usize, 0usize);
    // A column flow wraps at the DECLARED row count; with no declared rows the rows are unbounded
    // and every child keeps filling the current column. A row flow grows rows freely either way, so
    // the bound there only caps the defensive scan.
    let declared = declared_rows.map(|rows| rows.max(1));
    let row_bound = match declared {
        Some(rows) if flow.by_column() => rows,
        _ => spans.len() + 1,
    };
    // A column flow wraps at the DECLARED row count only. Without declared rows the rows are
    // unbounded — Compose's `explicitRowCount == 0` branch never wraps (`Grid.kt:1301-1309`) — so a
    // child keeps going DOWN the same column however tall the children before it were.
    let row_wrap = if flow.by_column() { declared } else { None };
    for (index, &(column_span, row_span)) in spans.iter().enumerate() {
        if let Some(cell) = claimed[index] {
            out.push(cell);
            continue;
        }
        // A span wider than the grid is clamped instead of pushed out of it, and a row span can never
        // usefully exceed the cells that exist: an unclamped `u32::MAX` made `fits` walk billions of
        // rows and `reserve` allocate them (a public API, so it has to be defensive).
        let column_span = (column_span.max(1) as usize).min(columns);
        let row_span = (row_span.max(1) as usize).min(match declared {
            // Never taller than the rows that exist.
            Some(rows) => rows,
            // With implicit rows a tall span is legitimate (one card spanning a column's worth of
            // rows), so the cap is only a guard against `u32::MAX`: unclamped it made `fits` walk
            // billions of rows and `reserve` allocate them.
            None => MAX_IMPLICIT_ROW_SPAN,
        });
        let start = if flow.dense() { (0, 0) } else { cursor };
        let (row, column) = find_cell(
            &occupancy,
            start,
            row_span,
            column_span,
            columns,
            flow.by_column(),
            row_wrap,
            spans.len(),
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
/// walking in the flow's order. When the flow-order scan runs out (a column flow with more children
/// than declared cells, which is a normal path), it falls back to the first genuinely free cell.
fn find_cell(
    occupancy: &Occupancy,
    start: (usize, usize),
    row_span: usize,
    column_span: usize,
    columns: usize,
    by_column: bool,
    // For a column flow: the declared row count the scan wraps at, or `None` when the rows are
    // unbounded (no declared rows — the scan then never wraps).
    row_wrap: Option<usize>,
    children: usize,
) -> (usize, usize) {
    let mut k = 0usize;
    loop {
        let (row, column) = if by_column {
            match row_wrap {
                // Column-major, wrapping at the declared row count: walk one column, then the next.
                Some(rows) => {
                    let linear = start.0 + k;
                    (linear % rows, start.1 + linear / rows)
                }
                // Column-major with unbounded rows: keep going down the same column.
                None => (start.0 + k, start.1),
            }
        } else {
            // Row-major: walk the columns of one row, then the next row.
            let linear = start.1 + k;
            (start.0 + linear / columns, linear % columns)
        };
        let inside_rows = match row_wrap {
            Some(rows) => row + row_span <= rows,
            None => true,
        };
        if column + column_span <= columns
            && inside_rows
            && occupancy.fits(row, column, row_span, column_span, columns)
        {
            return (row, column);
        }
        k += 1;
        let cap = match row_wrap {
            Some(rows) => (rows + 1) * columns,
            None => children + 1024,
        };
        if k > cap {
            break;
        }
    }
    // The flow-order scan gave up: every cell of the declared grid is taken, which a column flow with
    // more children than cells reaches. Fall back to the first genuinely FREE cell — two children must
    // never land on the same cell, which a fixed position did. Rows are unbounded, so walking them
    // upward always finds one; the cap only stops a malformed occupancy map from looping forever.
    let row_cap = row_wrap.unwrap_or(children) + children + 1024;
    let mut row = 0;
    while row < row_cap {
        for column in 0..columns {
            if column + column_span <= columns
                && occupancy.fits(row, column, row_span, column_span, columns)
            {
                return (row, column);
            }
        }
        row += 1;
    }
    // Unreachable for a well-formed occupancy map (the loop alone would find the row below the last
    // reserved one), but the last resort must still be a FREE row: `rows.len()` is one past every
    // reservation.
    (occupancy.rows.len(), 0)
}


/// Compose's pass 1.5 (`distributeSpanningSpace`, `Grid.kt:2178-2262`): an item spanning several
/// tracks may need more room than they add up to, and the tracks that CAN grow — intrinsic and
/// weighted ones, never fixed or percentage — absorb the deficit evenly. Implicit tracks count as
/// intrinsic (Compose treats a track past the spec list as `Auto`).
fn distribute_spanning(
    specs: &[GridTrackSpec],
    sizes: &mut [f32],
    cells: &[Cell],
    by_row: bool,
    gap: f32,
    mut required: impl FnMut(usize) -> f32,
) {
    for (index, cell) in cells.iter().enumerate() {
        let (track, span) = if by_row {
            (cell.row, cell.row_span)
        } else {
            (cell.column, cell.column_span)
        };
        if span <= 1 || track >= sizes.len() {
            continue;
        }
        let end = (track + span).min(sizes.len());
        let spanned = end - track;
        // The gaps INSIDE the span are part of the room it already has.
        let current = sizes[track..end].iter().sum::<f32>()
            + gap * spanned.saturating_sub(1) as f32;
        let growable: Vec<usize> = (track..end)
            .filter(|i| {
                !matches!(
                    specs.get(*i).copied().unwrap_or(GridTrackSpec::FitContent),
                    GridTrackSpec::Fixed(_) | GridTrackSpec::Percent(_)
                )
            })
            .collect();
        if growable.is_empty() {
            continue;
        }
        let deficit = required(index) - current;
        if deficit <= 0.0 {
            continue;
        }
        let share = deficit / growable.len() as f32;
        for i in growable {
            sizes[i] += share;
        }
    }
}


impl GridLayout {
    /// The cells an intrinsic query sees: the placement is `measure`'s, taken from the children's
    /// modifiers rather than from a measurement.
    fn intrinsic_cells(&self, ctx: &mut crate::layout::IntrinsicCtx<'_>, children: &[usize]) -> Vec<Cell> {
        let columns = self.columns.len();
        if columns == 0 {
            return Vec::new();
        }
        let spans: Vec<(u32, u32)> = children
            .iter()
            .map(|&c| {
                ctx.nodes[c]
                    .modifier
                    .get_grid_item()
                    .map(|(column_span, row_span, _)| (column_span, row_span))
                    .unwrap_or((1, 1))
            })
            .collect();
        let area_ids: Vec<Option<u64>> = children
            .iter()
            .map(|&c| ctx.nodes[c].modifier.get_grid_item_area())
            .collect();
        let declared = if self.rows.is_empty() { None } else { Some(self.rows.len()) };
        place_cells(&spans, columns, self.flow, declared, &self.areas, &area_ids)
    }

    /// Resolves the column tracks inside an intrinsic query — the same rules as `measure`, with the
    /// items' intrinsics coming from the probe.
    fn intrinsic_columns(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        cells: &[Cell],
        available: f32,
        height: f32,
        prefer_min: bool,
    ) -> Vec<f32> {
        let columns = self.columns.len();
        let mut per_track = vec![(0.0f32, 0.0f32); columns];
        for (i, &child) in children.iter().enumerate() {
            let Some(cell) = cells.get(i).copied() else { continue };
            if cell.column_span != 1 || cell.column >= columns {
                continue;
            }
            let max = ctx.child_intrinsic(child, IntrinsicQuery::MaxWidth, height);
            let min = ctx.child_intrinsic(child, IntrinsicQuery::MinWidth, height);
            per_track[cell.column].0 = per_track[cell.column].0.max(max);
            per_track[cell.column].1 = per_track[cell.column].1.max(min);
        }
        let gap = self.column_gap.max(0.0);
        let mut sizes = resolve_axis(&self.columns, available, gap, prefer_min, |i| per_track[i]);
        distribute_spanning(&self.columns, &mut sizes, cells, false, gap, |index| {
            ctx.child_intrinsic(children[index], IntrinsicQuery::MaxWidth, height)
        });
        sizes
    }

    /// Resolves the row tracks inside an intrinsic query, given the widths the columns already got:
    /// a row's intrinsic height depends on how wide its cell is.
    fn intrinsic_rows(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        cells: &[Cell],
        column_widths: &[f32],
        available: f32,
        prefer_min: bool,
    ) -> Vec<f32> {
        let column_gap = self.column_gap.max(0.0);
        let offsets = offset_table(column_widths, column_gap);
        let cell_width = |cell: &Cell| -> f32 {
            match (offsets.get(cell.column), offsets.get(cell.column + cell.column_span)) {
                (Some(start), Some(end)) => (end - start - column_gap).max(0.0),
                _ => available,
            }
        };
        let tracks = self.rows.len().max(
            cells
                .iter()
                .map(|c| c.row + c.row_span)
                .max()
                .unwrap_or(0),
        );
        let mut per_track = vec![(0.0f32, 0.0f32); tracks];
        for (i, &child) in children.iter().enumerate() {
            let Some(cell) = cells.get(i).copied() else { continue };
            if cell.row_span != 1 || cell.row >= tracks {
                continue;
            }
            let width = cell_width(&cell);
            let max = ctx.child_intrinsic(child, IntrinsicQuery::MaxHeight, width);
            let min = ctx.child_intrinsic(child, IntrinsicQuery::MinHeight, width);
            per_track[cell.row].0 = per_track[cell.row].0.max(max);
            per_track[cell.row].1 = per_track[cell.row].1.max(min);
        }
        let gap = self.row_gap.max(0.0);
        let mut sizes = resolve_axis(&self.rows, available, gap, prefer_min, |i| {
            per_track.get(i).copied().unwrap_or((0.0, 0.0))
        });
        distribute_spanning(&self.rows, &mut sizes, cells, true, gap, |index| {
            let width = cell_width(&cells[index]);
            ctx.child_intrinsic(children[index], IntrinsicQuery::MaxHeight, width)
        });
        sizes
    }
}

/// The extent one axis's tracks need: their sizes plus the gaps between them.
fn tracks_extent(sizes: &[f32], gap: f32) -> f32 {
    sizes.iter().sum::<f32>() + gap.max(0.0) * sizes.len().saturating_sub(1) as f32
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
        if n == 0 {
            // No child places anything, but the declared tracks still exist: a grid of three 80 dp
            // columns is 240 wide before it has a single item.
            let widths = resolve_tracks(&self.columns, constraints.max_width, self.column_gap.max(0.0));
            let heights = resolve_tracks(&self.rows, constraints.max_height, self.row_gap.max(0.0));
            let width = widths.iter().sum::<f32>()
                + self.column_gap.max(0.0) * widths.len().saturating_sub(1) as f32;
            let height = heights.iter().sum::<f32>()
                + self.row_gap.max(0.0) * heights.len().saturating_sub(1) as f32;
            return (
                Size::new(width.max(constraints.min_width), height.max(constraints.min_height)),
                Vec::new(),
            );
        }
        // The gap fields are public, so a caller can set a negative one without going through the
        // builder's clamp. Normalise FIRST: the track resolution below derives the weighted tracks from
        // what the fixed/percentage ones and the gaps leave over, so a raw negative gap would hand the
        // weights more space than the axis has.
        let column_gap = self.column_gap.max(0.0);
        let row_gap = self.row_gap.max(0.0);
        if columns == 0 {
            // No track to put anything in. The engine calls `place` unconditionally, so every child
            // still needs a placement — a zero-size one at the origin.
            return (
                Size::ZERO,
                (0..n)
                    .map(|_| Placement { size: Size::ZERO, position: Offset::new(0.0, 0.0) })
                    .collect(),
            );
        }

        // 1. Where every child goes. The cells depend only on the spans and the flow, so they come
        // first — the intrinsic tracks need to know who sits in them.
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
        let area_ids: Vec<Option<u64>> = children
            .iter()
            .map(|&c| nodes[c].modifier.get_grid_item_area())
            .collect();
        let declared_rows = if self.rows.is_empty() { None } else { Some(self.rows.len()) };
        let cells = place_cells(&spans, columns, self.flow, declared_rows, &self.areas, &area_ids);

        // 2. The column widths: fixed and percentage tracks resolve against the axis, intrinsic tracks
        // ask their items, and on an unbounded axis the percentage/weight tracks fall back to intrinsic
        // sizes instead of collapsing to zero.
        let column_intrinsic: Vec<(f32, f32)> = (0..columns)
            .map(|track| {
                track_intrinsic(nodes, policies, children, &cells, track, false, |_| {
                    constraints.max_height
                })
            })
            .collect();
        let mut widths = resolve_axis(&self.columns, constraints.max_width, column_gap, false, |i| {
            column_intrinsic[i]
        });
        distribute_spanning(&self.columns, &mut widths, &cells, false, column_gap, |index| {
            intrinsic_size_of(
                nodes,
                policies,
                children[index],
                IntrinsicQuery::MaxWidth,
                constraints.max_height,
            )
        });
        let column_offsets = offset_table(&widths, column_gap);

        // 3. The row tracks: declared ones resolve from their specs, intrinsic ones ask the items at the
        // width their cell gives them; implicit rows start at zero and grow in step 4.
        let declared_heights = {
            let cell_width = |cell: &Cell| -> f32 {
                (column_offsets[cell.column + cell.column_span]
                    - column_offsets[cell.column]
                    - column_gap)
                    .max(0.0)
            };
            let row_intrinsic: Vec<(f32, f32)> = (0..self.rows.len())
                .map(|track| {
                    track_intrinsic(nodes, policies, children, &cells, track, true, cell_width)
                })
                .collect();
            resolve_axis(&self.rows, constraints.max_height, row_gap, false, |i| {
                row_intrinsic[i]
            })
        };
        let mut declared_heights = declared_heights;
        distribute_spanning(&self.rows, &mut declared_heights, &cells, true, row_gap, |index| {
            let cell = cells[index];
            let width = (column_offsets[cell.column + cell.column_span]
                - column_offsets[cell.column]
                - column_gap)
                .max(0.0);
            intrinsic_size_of(
                nodes,
                policies,
                children[index],
                IntrinsicQuery::MaxHeight,
                width,
            )
        });
        let declared_table = offset_table(&declared_heights, row_gap);
        let row_count = cells
            .iter()
            .map(|c| c.row + c.row_span)
            .max()
            .unwrap_or(0)
            .max(self.rows.len());

        // 4. Measure each child at its cell's width, and at its row's height when that row is declared
        // (an implicit row has no height until the children say what it is, so the container's own
        // maximum is the limit there).
        let mut child_sizes = Vec::with_capacity(n);
        for (i, &child) in children.iter().enumerate() {
            let cell = cells[i];
            let width = (column_offsets[cell.column + cell.column_span]
                - column_offsets[cell.column]
                - column_gap)
                .max(0.0);
            let last_declared = cell.row + cell.row_span <= declared_heights.len();
            let height = if last_declared {
                (declared_table[cell.row + cell.row_span] - declared_table[cell.row] - row_gap)
                    .max(0.0)
            } else {
                constraints.max_height
            };
            let (size, _) =
                measure_node(nodes, policies, child, Constraints::new(0.0, width, 0.0, height));
            child_sizes.push(size);
        }

        // 5. The row heights: the declared tracks, then the implicit rows from their children. A child
        // spanning several implicit rows splits its height between them, so a card two rows tall does
        // not make only the last row carry it.
        let mut heights = declared_heights.clone();
        heights.resize(row_count, 0.0);
        for (i, cell) in cells.iter().enumerate() {
            let implicit: Vec<usize> = (cell.row..cell.row + cell.row_span)
                .filter(|row| *row >= self.rows.len())
                .collect();
            if implicit.is_empty() {
                // A declared row keeps its track spec; a child taller than it overflows the cell.
                continue;
            }
            let share = child_sizes[i].height / implicit.len() as f32;
            for row in implicit {
                heights[row] = heights[row].max(share);
            }
        }

        // 6. Place: the cell's origin, then the child's alignment inside it, then the RTL mirror.
        let row_offsets = offset_table(&heights, row_gap);
        let total_width = column_offsets[columns] - column_gap;
        let total_height = row_offsets[row_count] - row_gap;
        let mut placements = Vec::with_capacity(n);
        for (i, cell) in cells.iter().enumerate() {
            let cell_width = column_offsets[cell.column + cell.column_span]
                - column_offsets[cell.column]
                - column_gap;
            let cell_height = row_offsets[cell.row + cell.row_span]
                - row_offsets[cell.row]
                - row_gap;
            // Compose's `gridItem` defaults to `Alignment.TopStart` (Grid.kt:158-226): a child that
            // does not fill its cell sits at the cell's top-left, and a `fill_max_size` in the chain is
            // what stretches it.
            let default_alignment = ContentAlignment::TOP_START;
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

        // Compose constrains the grid's own size (`Grid.kt:898-899`): the tracks may add up to more
        // than the parent offered, and the parent has to hear about the smaller box it actually got.
        let measured = Size::new(
            total_width.clamp(constraints.min_width, constraints.max_width),
            total_height.clamp(constraints.min_height, constraints.max_height),
        );
        // The mirror above ran against `total_width` because that is where the tracks end. A minimum
        // wider than the tracks leaves the grid occupying a wider box, so shift the whole result by the
        // difference instead of letting the grid sit off to one side.
        if self.direction == LayoutDirection::Rtl && measured.width > total_width {
            let shift = measured.width - total_width;
            for placement in &mut placements {
                placement.position.x += shift;
            }
        }
        (measured, placements)
    }
    // ── The four intrinsic queries (Compose's `Grid.kt:2007-2078`) ──
    //
    // The trait's default would run `measure` with a probe constraint, which is already track-aware,
    // but it cannot answer a MIN query differently from a MAX one and it re-places the whole grid. These
    // resolve the tracks from the probe directly.

    fn min_intrinsic_width(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        if self.columns.is_empty() {
            return 0.0;
        }
        let cells = self.intrinsic_cells(ctx, children);
        let sizes = self.intrinsic_columns(ctx, children, &cells, f32::MAX, height, true);
        tracks_extent(&sizes, self.column_gap)
    }

    fn max_intrinsic_width(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        if self.columns.is_empty() {
            return 0.0;
        }
        let cells = self.intrinsic_cells(ctx, children);
        let sizes = self.intrinsic_columns(ctx, children, &cells, f32::MAX, height, false);
        tracks_extent(&sizes, self.column_gap)
    }

    fn min_intrinsic_height(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        if self.columns.is_empty() || self.rows.is_empty() {
            return 0.0;
        }
        let cells = self.intrinsic_cells(ctx, children);
        let widths = self.intrinsic_columns(ctx, children, &cells, width, f32::MAX, false);
        let heights = self.intrinsic_rows(ctx, children, &cells, &widths, f32::MAX, true);
        tracks_extent(&heights, self.row_gap)
    }

    fn max_intrinsic_height(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        if self.columns.is_empty() || self.rows.is_empty() {
            return 0.0;
        }
        let cells = self.intrinsic_cells(ctx, children);
        let widths = self.intrinsic_columns(ctx, children, &cells, width, f32::MAX, false);
        let heights = self.intrinsic_rows(ctx, children, &cells, &widths, f32::MAX, false);
        tracks_extent(&heights, self.row_gap)
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
    areas: Vec<GridArea>,
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
            areas: Vec::new(),
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

    /// Declares NAMED AREAS (Compose's `area(areaId, rows, columns)`): the cells are reserved before
    /// auto-placement runs, so the other children flow around them. A child names one with
    /// `Modifier::grid_item_in_area(id)`.
    pub fn areas(mut self, areas: Vec<GridArea>) -> Self {
        self.areas = areas;
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // Declare every parameter that decides the layout, so an unchanged grid Skips.
        ctx.changed(&self.columns);
        ctx.changed(&self.rows);
        ctx.changed(&self.row_gap);
        ctx.changed(&self.column_gap);
        ctx.changed(&self.flow);
        ctx.changed(&self.areas);
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
                .areas(self.areas)
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
        let (size, placements) = policy.measure(nodes, &[], &children, constraints);
        // The engine calls `place` with whatever measure returned, so the harness does too: a policy
        // that reports fewer placements than children would panic there, and must fail here first.
        policy.place(nodes, &children, &placements);
        (size, placements)
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

    /// A child that spans two columns is measured at their combined width and sits across both.
    #[test]
    fn a_spanning_child_covers_the_tracks_it_spans() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(70.0)])
            .column_gap(10.0);
        let mut nodes = vec![spanning_leaf(40.0, 20.0, 2, 1), make_leaf(10.0, 10.0)];
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        // The default alignment is TopStart, so the child keeps its own size inside the wide cell.
        assert_eq!(placements[0].size, Size::new(40.0, 20.0), "the child keeps its own size in the cell");
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0), "and sits at the cell's top-left");
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
        // Stretch explicitly: the mirror is about the TRACKS, and the default TopStart would leave the
        // children at their own 10 dp width.
        let mut nodes: Vec<LayoutNode> = (0..2)
            .map(|_| {
                let mut node = LayoutNode::leaf(
                    Modifier::new()
                        .size(10.0, 10.0)
                        .grid_item_aligned(1, 1, ContentAlignment::STRETCH),
                );
                node.measured_size = Size::new(10.0, 10.0);
                node
            })
            .collect();
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

        // The default is Compose's TopStart: a child that does not fill its cell keeps its own size and
        // hugs the top-left corner, while an explicit STRETCH fills the cell.
        let mut top_start = vec![make_leaf(20.0, 10.0)];
        let (_, placements) = run(&policy, &mut top_start, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].size, Size::new(20.0, 10.0), "TopStart keeps the child's size");
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0));

        let mut stretched = vec![LayoutNode::leaf(
            Modifier::new()
                .size(20.0, 10.0)
                .grid_item_aligned(1, 1, ContentAlignment::STRETCH),
        )];
        stretched[0].measured_size = Size::new(20.0, 10.0);
        let (_, placements) = run(&policy, &mut stretched, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].size, Size::new(100.0, 10.0), "STRETCH fills the cell width");
    }



    /// A span wider than the grid is clamped to it instead of being pushed outside.
    #[test]
    fn a_span_wider_than_the_grid_is_clamped() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)]);
        let mut nodes = vec![spanning_leaf(10.0, 10.0, 5, 1)];
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size.width, 100.0);
        assert_eq!(placements[0].size.width, 10.0, "the child keeps its own size (TopStart)");
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0));
    }

    /// The gap fields are public, so a negative gap can bypass the builder's clamp; the measure has to
    /// normalise it, or the tracks would overlap.
    #[test]
    fn a_negative_gap_is_clamped_in_measure() {
        let mut policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)]);
        policy.column_gap = -20.0;
        policy.row_gap = -20.0;
        let mut nodes: Vec<LayoutNode> = (0..2).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size, Size::new(100.0, 10.0), "the negative gap behaves as 0");
        assert_eq!(placements[1].position.x, 50.0);
    }

    /// A column flow with more children than declared cells must keep going into implicit rows — and
    /// two children must NEVER share a cell.
    #[test]
    fn more_children_than_declared_cells_never_overlap() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)])
            .rows(vec![GridTrackSpec::fixed(20.0), GridTrackSpec::fixed(20.0)])
            .flow(GridFlow::Column);
        let mut nodes: Vec<LayoutNode> = (0..6).map(|_| make_leaf(10.0, 10.0)).collect();
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 300.0));
        let mut cells: Vec<(i32, i32)> = placements
            .iter()
            .map(|p| (p.position.x as i32, p.position.y as i32))
            .collect();
        cells.sort_unstable();
        let unique = cells.len();
        cells.dedup();
        assert_eq!(cells.len(), unique, "every child sits in its own cell: {cells:?}");
        assert!(
            placements.iter().any(|p| p.position.y >= 40.0),
            "the extra children wrap into implicit rows: {placements:?}"
        );
    }

    /// A column list is required; without one the grid measures to nothing and still gives every child
    /// a placement. `place` runs in this harness, so a regression panics here — the engine calls
    /// `place` unconditionally (`measure/engine.rs`), and an empty placement list used to index out of
    /// bounds on the first layout of `Grid::default()`.
    #[test]
    fn a_grid_without_columns_places_every_child() {
        let policy = GridLayout::new(Vec::new());
        let mut nodes: Vec<LayoutNode> = (0..2).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::ZERO);
        assert_eq!(placements.len(), 2, "one placement per child, so place() cannot index out of bounds");
        assert!(placements.iter().all(|p| p.size == Size::ZERO));
        assert_eq!(nodes[0].measured_size, Size::ZERO);
    }

    /// A tall row span must not push a later child into cells it already covers. The flow-order scan
    /// gives up long before the span's rows are free, and the fallback has to walk up past them.
    ///
    /// The rows are DECLARED at 20 dp each so the y coordinate reveals the row index: implicit rows
    /// above a span collapse to zero height, which would hide the overlap.
    #[test]
    fn a_tall_row_span_does_not_swallow_a_later_child() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0)])
            .rows((0..8).map(|_| GridTrackSpec::fixed(20.0)).collect())
            .flow(GridFlow::Row);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(10.0, 10.0).grid_item(1, 8)),
            make_leaf(10.0, 10.0),
        ];
        nodes[0].measured_size = Size::new(10.0, 10.0);
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 400.0));
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0), "the tall span starts at the top");
        assert_eq!(
            placements[1].position,
            Offset::new(0.0, 160.0),
            "the next child starts BELOW the eight 20 dp rows the span covers"
        );
    }

    /// The negative-gap clamp has to happen before the tracks resolve: a weighted track derives its
    /// size from what the fixed tracks and the gaps leave over.
    #[test]
    fn a_negative_gap_does_not_inflate_the_weighted_track() {
        let mut policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::weight(1.0)]);
        policy.column_gap = -20.0;
        let mut nodes: Vec<LayoutNode> = (0..2).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, _) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size.width, 400.0, "a clamped gap leaves the weights exactly what the axis has left");
    }

    /// A grid without children still occupies its DECLARED tracks: three 80 dp columns are 240 wide
    /// before a single item arrives. Rows that no child reaches count the same way.
    #[test]
    fn a_grid_without_children_reports_its_declared_tracks() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(80.0); 3])
            .rows(vec![GridTrackSpec::fixed(20.0), GridTrackSpec::fixed(20.0)]);
        let mut nodes: Vec<LayoutNode> = Vec::new();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size, Size::new(240.0, 40.0));
        assert!(placements.is_empty());
    }

    /// A declared row the children never reach still takes its space.

    /// The percentage basis excludes the gaps between tracks (Compose's `availableTrackSpace`,
    /// `Grid.kt:634-639`), so two half-width tracks plus a gap fill the axis exactly instead of
    /// overflowing by the gap.

    /// An intrinsic (FitContent) track takes the widest MAX intrinsic of the items in it, so a column
    /// of labels is as wide as the longest one without any fixed size.

    /// A named area reserves its cells BEFORE auto-placement, so the rest of the children flow around
    /// it instead of landing inside (Compose's `area(...)` + `gridItem(areaId)`).

    /// Compose's pass 1.5: a child spanning several INTRINSIC tracks pushes them wider when what it
    /// needs does not fit what they already add up to (`Grid.kt:2178-2262`).
    #[test]
    fn a_spanning_item_grows_the_intrinsic_tracks_it_covers() {
        let policy = GridLayout::new(vec![
            GridTrackSpec::FitContent,
            GridTrackSpec::FitContent,
        ])
        .column_gap(10.0);
        let mut nodes = vec![
            make_leaf(40.0, 10.0),
            make_leaf(40.0, 10.0),
            LayoutNode::leaf(Modifier::new().size(200.0, 10.0).grid_item(2, 1)),
        ];
        nodes[2].measured_size = Size::new(200.0, 10.0);
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        // The tracks start at 40 each; the span needs 200 against the 90 it has (40 + 10 + 40), so the
        // 110 deficit splits evenly and both end at 95.
        assert_eq!(size.width, 200.0, "95 + 10 + 95");
        assert_eq!(placements[2].position.x, 0.0);
    }

    /// The grid answers its own intrinsic queries from the resolved tracks instead of falling back to
    /// "measure me and report that" (`Grid.kt:2007-2078`).
    #[test]
    fn an_intrinsic_query_reports_the_resolved_tracks() {
        use crate::layout::IntrinsicCtx;
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(40.0), GridTrackSpec::fixed(60.0)])
            .column_gap(10.0);
        let mut nodes = vec![make_leaf(10.0, 10.0)];
        let children = vec![0usize];
        let mut ctx = IntrinsicCtx {
            nodes: &mut nodes,
            policies: &[],
            policy_idx: 0,
        };
        assert_eq!(
            policy.max_intrinsic_width(&mut ctx, &children, f32::MAX),
            110.0,
            "40 + 10 + 60"
        );
        let mut ctx = IntrinsicCtx {
            nodes: &mut nodes,
            policies: &[],
            policy_idx: 0,
        };
        assert_eq!(policy.min_intrinsic_width(&mut ctx, &children, f32::MAX), 110.0);
    }

    #[test]
    fn a_named_area_reserves_its_cells() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0); 2])
            .areas(vec![GridArea::new(7, (0, 1), (0, 1))]);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(10.0, 10.0).grid_item_in_area(7)),
            make_leaf(10.0, 10.0),
        ];
        nodes[0].measured_size = Size::new(10.0, 10.0);
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 400.0));
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0), "the area starts at its first row and column");
        assert_eq!(
            placements[1].position,
            Offset::new(0.0, 10.0),
            "the auto-placed child wraps below the reserved 2x2 block"
        );
    }

    /// An unknown area id falls back to auto-placement instead of dropping the child.
    #[test]
    fn an_unknown_area_id_falls_back_to_auto_placement() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0)]);
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new().size(10.0, 10.0).grid_item_in_area(99),
        )];
        nodes[0].measured_size = Size::new(10.0, 10.0);
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(placements[0].position, Offset::new(0.0, 0.0));
    }

    #[test]
    fn a_fit_content_column_sizes_to_its_items() {
        let policy = GridLayout::new(vec![
            GridTrackSpec::FitContent,
            GridTrackSpec::fixed(40.0),
        ]);
        let mut nodes = vec![make_leaf(70.0, 10.0), make_leaf(10.0, 10.0), make_leaf(30.0, 10.0)];
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size.width, 110.0, "70 from the widest item + 40 fixed");
        assert_eq!(placements[1].position.x, 70.0, "the fixed track starts after the intrinsic one");
        assert_eq!(placements[2].position.x, 0.0, "the third child wraps to the next row");
    }

    /// On an UNBOUNDED axis a percentage track falls back to its items' max intrinsic width and a
    /// weighted one to their min content, instead of collapsing to zero (`Grid.kt:1645-1663`,
    /// `:1833-1845`).
    #[test]
    fn an_unbounded_axis_sizes_percentage_and_weight_tracks_from_content() {
        let policy = GridLayout::new(vec![
            GridTrackSpec::percent(0.5),
            GridTrackSpec::weight(1.0),
        ]);
        let mut nodes = vec![make_leaf(60.0, 10.0), make_leaf(80.0, 10.0)];
        let (size, placements) = run(&policy, &mut nodes, Constraints::UNBOUNDED);
        assert_eq!(size.width, 140.0, "60 max-intrinsic for the percentage track, 80 min-content for the weight");
        assert_eq!(placements[1].position.x, 60.0);
    }

    /// A child spanning several IMPLICIT rows splits its height between them instead of charging the
    /// last one only, so a two-row card shares the row it sits in with anything beside it.
    #[test]
    fn a_multi_row_span_splits_its_height_between_implicit_rows() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)]);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(10.0, 40.0).grid_item(1, 2)),
            make_leaf(10.0, 10.0),
        ];
        nodes[0].measured_size = Size::new(10.0, 40.0);
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 400.0));
        assert_eq!(size.height, 40.0, "20 + 20, not 10 + 40");
        assert_eq!(placements[1].position, Offset::new(50.0, 0.0), "the sibling sits in the first row");
    }

    #[test]
    fn a_percentage_grid_with_gaps_fills_its_axis() {
        let policy = GridLayout::new(vec![
            GridTrackSpec::percent(0.5),
            GridTrackSpec::percent(0.5),
        ])
        .column_gap(10.0);
        let mut nodes: Vec<LayoutNode> = (0..2).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 100.0, "45 + 10 + 45, not 50 + 10 + 50");
        assert_eq!(placements[1].position.x, 55.0);
    }

    /// A column flow without declared rows never wraps: the column keeps growing downward, as Compose's
    /// `explicitRowCount == 0` branch does (`Grid.kt:1301-1309`).
    #[test]
    fn a_column_flow_without_declared_rows_never_wraps() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)])
            .flow(GridFlow::Column);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(10.0, 10.0).grid_item(1, 5)),
            make_leaf(10.0, 10.0),
        ];
        nodes[0].measured_size = Size::new(10.0, 10.0);
        let (_, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 400.0));
        assert_eq!(
            placements[1].position.x, 0.0,
            "the second child stays in the same column, below the five rows the first one covers: {:?}",
            placements[1].position
        );
    }

    /// A grid whose tracks add up to more than the parent offers reports the parent's maximum, as
    /// Compose's `constrainWidth`/`constrainHeight` do (`Grid.kt:898-899`).
    #[test]
    fn an_overflowing_grid_is_clamped_to_the_axis() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(80.0); 3]).column_gap(8.0);
        let mut nodes: Vec<LayoutNode> = (0..3).map(|_| make_leaf(10.0, 10.0)).collect();
        let (size, _) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size.width, 200.0, "3 x 80 + 2 x 8 = 256, reported as the 200 the parent allows");
    }

    /// In a column flow a row span is clamped to the declared rows, so a child never spills into an
    /// implicit row that the flow is not supposed to create (Compose: `Grid.kt:1217-1223`).
    #[test]
    fn a_column_flow_clamps_a_span_to_the_declared_rows() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0), GridTrackSpec::fixed(50.0)])
            .rows(vec![GridTrackSpec::fixed(20.0), GridTrackSpec::fixed(20.0)])
            .flow(GridFlow::Column);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(10.0, 10.0).grid_item(1, 3)),
            make_leaf(10.0, 10.0),
        ];
        nodes[0].measured_size = Size::new(10.0, 10.0);
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 400.0));
        assert_eq!(size.height, 40.0, "two declared rows of 20, no implicit third");
        assert_eq!(
            placements[1].position,
            Offset::new(50.0, 0.0),
            "the span was clamped to the two rows, so the next child starts the second column"
        );
    }

    /// A `u32::MAX` row span used to make the placement walk billions of rows and allocate them. The
    /// public `u32` API has to survive it.
    #[test]
    fn an_extreme_row_span_is_clamped() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0)]);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(10.0, 10.0).grid_item(1, u32::MAX)),
            make_leaf(10.0, 10.0),
        ];
        nodes[0].measured_size = Size::new(10.0, 10.0);
        let (size, placements) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 400.0));
        assert_eq!(placements.len(), 2);
        assert!(size.height.is_finite(), "no runaway span: {size:?}");
    }

    #[test]
    fn an_unused_declared_row_still_counts() {
        let policy = GridLayout::new(vec![GridTrackSpec::fixed(50.0)])
            .rows(vec![GridTrackSpec::fixed(20.0), GridTrackSpec::fixed(30.0)]);
        let mut nodes = vec![make_leaf(10.0, 10.0)];
        let (size, _) = run(&policy, &mut nodes, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size.height, 50.0, "20 + 30 even though only the first row holds a child");
    }
}
