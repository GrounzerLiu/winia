//! `LazyVerticalGrid` / `LazyHorizontalGrid` — Compose's lazy grids, on the same axis-generic core
//! as the lazy lists.
//!
//! Both are ONE type over an axis marker (`LazyGrid<A: LazyAxis>`), exactly like
//! `LazyColumn`/`LazyRow`: the axis differences live in `LazyAxis`, and the horizontal grid adds
//! nothing but the RTL mirroring the horizontal scroll modifier already performs.
//!
//! A grid is a list of LINES: `cells` items per line (Compose's `GridCells.Fixed`), or as many as fit
//! at a minimum size (`GridCells.Adaptive`). Each line is composed as one lazy item — a `Row` for a
//! vertical grid, a `Column` for a horizontal one — whose cells carry a `layout_weight` equal to their
//! span, so a two-cell item is twice as big. Everything else comes from the lazy list: the anchor
//! model, stable keys, the composition window, placement animation, fling and snapping.
//!
//! # Differences from Compose (see `docs/lazy-grid.md`)
//!
//! - The windowing unit is a LINE: `layout_info()` reports lines and `Modifier::animate_item` animates a
//!   whole line, where Compose windows per line but reports per ITEM (`LazyGridItemInfo`).
//! - `GridCells.Adaptive` needs the cross extent, which only a measure knows: the first frame composes
//!   one cell per line and the next corrects it — the same two-phase rule as the lazy window
//!   (docs/lazy-column.md 2.9).
//! - No `stickyHeader` (Compose's lazy grids do not have one either).

use super::{LazyAxis, LazyList, LazyListState};
use crate::layout::container::build_container;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::layout::Constraints;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
use crate::unit::{Offset, Size};
use std::marker::PhantomData;
use std::sync::Arc;

/// How many cells a line holds — Compose's `GridCells`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridCells {
    /// A fixed number of cells per line.
    Fixed(usize),
    /// As many cells as fit with each at least this big (the count is floored, so a cell can end up
    /// wider than the minimum and never narrower).
    Adaptive(f32),
}

impl GridCells {
    fn count(self, cross_extent: f32, spacing: f32) -> usize {
        match self {
            GridCells::Fixed(cells) => cells.max(1),
            GridCells::Adaptive(min_size) => {
                let min_size = min_size.max(1.0);
                // An UNBOUNDED cross axis has no cell count: Compose requires bounds here (it throws),
                // and letting it through saturated the cast below to usize::MAX, which put every item
                // into ONE line (measured: the list then had a single item and no scroll range). Both
                // sentinels count: infinity and the f32::MAX the scroll paths write.
                if !(cross_extent < f32::MAX) {
                    return 1;
                }
                if cross_extent <= 0.0 {
                    // Nothing measured yet: the grid composes one cell per line for this frame and
                    // re-chunks as soon as the cross extent is written back.
                    return 1;
                }
                let fit = ((cross_extent + spacing) / (min_size + spacing)).floor();
                (fit as usize).max(1)
            }
        }
    }
}

/// One line: its items and their spans (the weight each cell gets).
#[derive(Debug, Clone)]
struct Line {
    items: Vec<(usize, u32)>,
}

/// The key of one line: its first item's key folded together with the line's SHAPE (item count, the
/// grid's cell count and the cross spacing). Stable while the shape is stable — what the anchor needs —
/// and different as soon as the shape changes — what the Skip decision needs.
fn shape_key(first: u64, items: usize, cells: usize, spacing: f32) -> u64 {
    const PRIME: u64 = 0x1000_0000_01b3;
    let mut hash = first ^ 0xcbf2_9ce4_8422_2325;
    for value in [items as u64, cells as u64, spacing.to_bits() as u64] {
        hash ^= value;
        hash = hash.wrapping_mul(PRIME);
        hash ^= hash >> 29;
    }
    hash
}

/// Splits `count` items into lines, honouring each item's span — Compose's `LazyGridIntervalContent`
/// chunking: an item that does not fit the room left in the current line starts the next one.
fn chunk_lines(count: usize, cells: usize, span: &(dyn Fn(usize) -> u32 + Send + Sync)) -> Vec<Line> {
    let cells = cells.max(1);
    let mut lines: Vec<Line> = Vec::new();
    let mut current: Vec<(usize, u32)> = Vec::new();
    let mut used = 0usize;
    for index in 0..count {
        let want = (span(index).max(1) as usize).min(cells);
        if !current.is_empty() && used + want > cells {
            lines.push(Line { items: std::mem::take(&mut current) });
            used = 0;
        }
        current.push((index, want as u32));
        used += want;
    }
    if !current.is_empty() {
        lines.push(Line { items: current });
    }
    lines
}

/// The item source of a grid, stored until `build`.
struct GridContent {
    count: usize,
    key: Arc<dyn Fn(usize) -> u64 + Send + Sync>,
    span: Arc<dyn Fn(usize) -> u32 + Send + Sync>,
    content: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync>,
}

/// A lazy grid — Compose's `LazyVerticalGrid` / `LazyHorizontalGrid`. The axis is the type parameter.
pub struct LazyGrid<A: LazyAxis> {
    cells: GridCells,
    spacing: f32,
    cross_spacing: f32,
    content_padding: (f32, f32),
    content_padding_cross: (f32, f32),
    reverse_layout: bool,
    beyond_bounds: Option<usize>,
    state: Option<LazyListState>,
    modifier: Modifier,
    content: Option<GridContent>,
    marker: PhantomData<A>,
}

/// The vertical lazy grid.
pub type LazyVerticalGrid = LazyGrid<crate::layout::axis::VerticalAxis>;
/// The horizontal lazy grid. RTL mirrors it through the scroll modifier, exactly as `LazyRow` does.
pub type LazyHorizontalGrid = LazyGrid<crate::layout::axis::HorizontalAxis>;

impl<A: LazyAxis> Default for LazyGrid<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: LazyAxis> LazyGrid<A> {
    pub fn new() -> Self {
        LazyGrid {
            cells: GridCells::Fixed(2),
            spacing: 0.0,
            cross_spacing: 0.0,
            content_padding: (0.0, 0.0),
            content_padding_cross: (0.0, 0.0),
            reverse_layout: false,
            beyond_bounds: None,
            state: None,
            modifier: Modifier::new(),
            content: None,
            marker: PhantomData,
        }
    }

    /// How many cells a line holds.
    pub fn cells(mut self, cells: GridCells) -> Self {
        self.cells = cells;
        self
    }

    /// The gap between lines (Compose's main-axis arrangement).
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    /// The gap between cells inside a line.
    pub fn cross_spacing(mut self, spacing: f32) -> Self {
        self.cross_spacing = spacing.max(0.0);
        self
    }

    /// The main-axis padding before the first line and after the last.
    pub fn content_padding(mut self, before: f32, after: f32) -> Self {
        self.content_padding = (before.max(0.0), after.max(0.0));
        self
    }

    /// The cross-axis padding before the first cell and after the last.
    pub fn content_padding_cross(mut self, before: f32, after: f32) -> Self {
        self.content_padding_cross = (before.max(0.0), after.max(0.0));
        self
    }

    /// Lays the lines out from the far edge (Compose's `reverseLayout`).
    pub fn reverse_layout(mut self, reverse: bool) -> Self {
        self.reverse_layout = reverse;
        self
    }

    /// Composes this many extra lines on each side of the viewport.
    pub fn beyond_bounds(mut self, lines: usize) -> Self {
        self.beyond_bounds = Some(lines);
        self
    }

    pub fn state(mut self, state: LazyListState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Items with position keys and a span of one cell.
    pub fn items(
        self,
        count: usize,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        self.items_with(count, |index| index as u64, |_| 1, content)
    }

    /// Items with a key factory, a SPAN factory and the content — Compose's
    /// `item(span = { GridItemSpan(n) })`.
    pub fn items_with(
        self,
        count: usize,
        key: impl Fn(usize) -> u64 + Send + Sync + 'static,
        span: impl Fn(usize) -> u32 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize) + Send + Sync + 'static,
    ) -> Self {
        self.with_content(GridContent {
            count,
            key: Arc::new(key),
            span: Arc::new(span),
            content: Arc::new(content),
        })
    }

    /// Items over a data list, with a key factory, a span factory and the content.
    pub fn items_from<T: Send + Sync + 'static>(
        self,
        list: impl Into<Arc<Vec<T>>>,
        key: impl Fn(&T) -> u64 + Send + Sync + 'static,
        span: impl Fn(&T) -> u32 + Send + Sync + 'static,
        content: impl Fn(&mut ComposeCtx, usize, &T) + Send + Sync + 'static,
    ) -> Self {
        let list: Arc<Vec<T>> = list.into();
        let count = list.len();
        let key = Arc::new(key);
        let span = Arc::new(span);
        let content = Arc::new(content);
        let (k, s, c) = (key.clone(), span.clone(), content.clone());
        let (lk, ls, lc) = (list.clone(), list.clone(), list.clone());
        self.items_with(
            count,
            move |index| k(&lk[index]),
            move |index| s(&ls[index]),
            move |ctx, index| c(ctx, index, &lc[index]),
        )
    }

    fn with_content(mut self, content: GridContent) -> Self {
        self.content = Some(content);
        self
    }

    /// Chunks the items into lines and hands them to the lazy list, which owns the window, the
    /// anchor, the scroll and the clipping.
    pub fn build(self, ctx: &mut ComposeCtx) {
        let Some(content) = self.content else {
            return;
        };
        // Resolve the state BEFORE the cross extent: without this, a grid that does not pass
        // `.state(...)` had its state created inside LazyList::build, so the grid never read the cross
        // extent, never registered a dependency on it and stayed at ONE cell per line forever (Compose
        // computes the slots from the measure constraints, so it has no such contract).
        let state = match self.state.clone() {
            Some(state) => state,
            None => ctx.remember(LazyListState::new).get(),
        };
        let cross = state.viewport_cross_size();
        let cells_per_line = self.cells.count(cross, self.cross_spacing);
        let lines = chunk_lines(content.count, cells_per_line, content.span.as_ref());
        let line_keys: Vec<u64> = lines
            .iter()
            .map(|line| {
                let first = line.items.first().map(|(index, _)| (content.key)(*index)).unwrap_or(0);
                // The line's identity carries its SHAPE as well as its first item's key: appending an
                // item INSIDE the last line (or changing the cell count or the cross spacing) changes
                // nothing else the lazy list compares, so the stale line was skipped and the new items
                // never appeared (measured: 7 items fixed(3) then 9 items stayed 3/3/1).
                shape_key(first, line.items.len(), cells_per_line, self.cross_spacing)
            })
            .collect();
        let lines = Arc::new(lines);
        let line_keys = Arc::new(line_keys);
        let content = content.content;
        let cross_spacing = self.cross_spacing;
        let mut list = LazyList::<A>::new_list()
            .state(state)
            .modifier(self.modifier)
            .spacing(self.spacing)
            .content_padding(self.content_padding.0, self.content_padding.1)
            .content_padding_cross(self.content_padding_cross.0, self.content_padding_cross.1)
            .reverse_layout(self.reverse_layout);
        if let Some(lines) = self.beyond_bounds {
            list = list.beyond_bounds(lines);
        }
        list.items(
            lines.len(),
            move |line| line_keys[line],
            move |ctx, line| {
                build_line::<A>(
                    ctx,
                    &lines[line].items,
                    cells_per_line,
                    cross_spacing,
                    content.as_ref(),
                );
            },
        )
        .build(ctx);
    }
}

/// One line's geometry: Compose's slot table (`LazyGridSlots`). Each cell occupies `span` slots of
/// `unit = (cross - (cells - 1) * spacing) / cells`, so a cell is `span * unit + (span - 1) * spacing`
/// across and starts at `slot * (unit + spacing)`.
///
/// A `layout_weight` chain cannot express that: weights normalize over the line's OWN children, so a
/// partial line stretched its cells across the whole line (measured: one cell 300 dp wide where three
/// slots of 98.67 were correct) and a spanning cell lost the spacing it covers (96/192 where Compose
/// gives 92/196 for spans [1, 2] at 300 dp with 12 dp of spacing).
#[derive(Debug)]
struct LineLayout {
    cells: usize,
    spacing: f32,
    vertical: bool,
    /// `(start slot, span)` per child, in composition order.
    slots: Vec<(usize, usize)>,
}

impl MeasurePolicy for LineLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let cells = self.cells.max(1);
        let spacing = self.spacing.max(0.0);
        let (cross_max, main_max) = if self.vertical {
            (constraints.max_width, constraints.max_height)
        } else {
            (constraints.max_height, constraints.max_width)
        };
        let bounded = cross_max < f32::MAX;
        let unit = if bounded {
            ((cross_max - (cells as f32 - 1.0) * spacing) / cells as f32).max(0.0)
        } else {
            0.0
        };
        let mut placements = Vec::with_capacity(children.len());
        let mut cross_used = 0.0f32;
        let mut main_used = 0.0f32;
        for (i, &child) in children.iter().enumerate() {
            let (start, span) = self.slots.get(i).copied().unwrap_or((i, 1));
            let span = span.max(1);
            let cross = unit * span as f32 + (span as f32 - 1.0) * spacing;
            let child_constraints = if self.vertical {
                if bounded {
                    Constraints::new(cross, cross, 0.0, main_max)
                } else {
                    Constraints::new(0.0, f32::MAX, 0.0, main_max)
                }
            } else if bounded {
                Constraints::new(0.0, main_max, cross, cross)
            } else {
                Constraints::new(0.0, main_max, 0.0, f32::MAX)
            };
            let (size, _) = measure_node(nodes, policies, child, child_constraints);
            let start_cross = start as f32 * (unit + spacing);
            let position = if self.vertical {
                Offset::new(start_cross, 0.0)
            } else {
                Offset::new(0.0, start_cross)
            };
            let cell_cross = if self.vertical { size.width } else { size.height };
            let cell_main = if self.vertical { size.height } else { size.width };
            cross_used = cross_used.max(start_cross + cell_cross);
            main_used = main_used.max(cell_main);
            placements.push(Placement { size, position });
        }
        // A line spans the whole grid across the cross axis even when its slots do not fill it.
        let line_cross = if bounded { cross_max } else { cross_used };
        let size = if self.vertical {
            Size::new(line_cross, main_used)
        } else {
            Size::new(main_used, line_cross)
        };
        (size, placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &child) in children.iter().enumerate() {
            if let Some(placement) = placements.get(i) {
                nodes[child].position = placement.position;
                nodes[child].measured_size = placement.size;
            }
        }
    }
}

/// Composes one line: the cells in slot order, measured by [`LineLayout`].
fn build_line<A: LazyAxis>(
    ctx: &mut ComposeCtx,
    items: &[(usize, u32)],
    cells: usize,
    cross_spacing: f32,
    content: &(dyn Fn(&mut ComposeCtx, usize) + Send + Sync),
) {
    let mut slots = Vec::with_capacity(items.len());
    let mut used = 0usize;
    for (_, span) in items {
        let span = (*span).max(1) as usize;
        slots.push((used, span));
        used += span;
    }
    let modifier = if A::VERTICAL {
        Modifier::new().fill_max_width()
    } else {
        Modifier::new().fill_max_height()
    };
    // The line's SHAPE is declared here, so a changed chunking re-enters the line instead of keeping
    // its previous geometry (the line key carries the same facts, which is what makes the enclosing
    // item group run this code at all).
    ctx.changed(&cells);
    ctx.changed(&used);
    ctx.changed(&items.len());
    ctx.changed(&cross_spacing);
    build_container(
        ctx,
        modifier,
        LineLayout {
            cells,
            spacing: cross_spacing,
            vertical: A::VERTICAL,
            slots,
        },
        |ctx| {
            for (index, _) in items {
                content(ctx, *index);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Constraints;
    use crate::runtime::composer::Composer;

    fn spans(list: &[u32]) -> impl Fn(usize) -> u32 + Send + Sync {
        let list = list.to_vec();
        move |index| list.get(index).copied().unwrap_or(1)
    }

    #[test]
    fn fixed_cells_chunk_the_items_into_lines() {
        let lines = chunk_lines(7, 3, &spans(&[]));
        let shape: Vec<Vec<usize>> = lines.iter().map(|l| l.items.iter().map(|(i, _)| *i).collect()).collect();
        assert_eq!(shape, vec![vec![0, 1, 2], vec![3, 4, 5], vec![6]]);
    }

    #[test]
    fn a_span_that_does_not_fit_starts_the_next_line() {
        // cells = 3; items 0 and 1 take one cell each, item 2 needs two and does not fit the one that
        // is left, so it opens the next line and item 3 joins it.
        let lines = chunk_lines(4, 3, &spans(&[1, 1, 2, 1]));
        let shape: Vec<Vec<usize>> = lines.iter().map(|l| l.items.iter().map(|(i, _)| *i).collect()).collect();
        assert_eq!(shape, vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn an_adaptive_grid_floors_the_count_that_fits() {
        assert_eq!(GridCells::Adaptive(100.0).count(320.0, 0.0), 3);
        assert_eq!(GridCells::Adaptive(100.0).count(320.0, 10.0), 3, "320 + 10 over 110");
        assert_eq!(GridCells::Adaptive(100.0).count(0.0, 0.0), 1, "nothing measured yet");
        assert_eq!(GridCells::Fixed(4).count(0.0, 0.0), 4, "a fixed count ignores the extent");
    }

    /// End to end: six items in a three-cell vertical grid compose as two line Rows, the cells share
    /// the width equally, and the lazy list's anchor moves them. Two compose+layout rounds are needed,
    /// exactly as the lazy list's own tests do: the first frame composes the window from an empty cache.
    #[test]
    fn a_vertical_grid_composes_one_row_per_line_and_scrolls() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer, state: &LazyListState| {
            let state = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Fixed(3))
                    .state(state)
                    .spacing(4.0)
                    .cross_spacing(2.0)
                    .modifier(Modifier::new().fill_max_width().height(40.0))
                    .items(6, |ctx, index| {
                        crate::layout::Stack::new()
                            .modifier(
                                Modifier::new()
                                    .size(10.0, 40.0)
                                    .test_tag(format!("cell-{index}")),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 40.0));
        };
        frame(&mut composer, &state);
        frame(&mut composer, &state);

        let root = composer.layout_root().unwrap();
        let lines: Vec<usize> = root.children.clone();
        assert_eq!(lines.len(), 2, "six items in three-cell lines");
        assert_eq!(composer.arena_nodes()[lines[0]].position.y, 0.0);
        assert_eq!(
            composer.arena_nodes()[lines[1]].position.y, 44.0,
            "the second line starts one 40 dp line plus the 4 dp spacing down"
        );
        // The line's root holds the Row; the Row holds the three equally weighted cells.
        let row = composer.arena_nodes()[lines[0]].children[0];
        let cells = composer.arena_nodes()[row].children.clone();
        assert_eq!(cells.len(), 3, "three cells in a line");
        let widths: Vec<f32> = cells
            .iter()
            .map(|c| composer.arena_nodes()[*c].measured_size.width)
            .collect();
        assert!(
            widths.iter().all(|w| (w - widths[0]).abs() < 0.01),
            "the cells share the width equally: {widths:?}"
        );

        // The anchor: jumping to line 1 puts the 44 dp offset on it.
        state.scroll_to_item(1, 0.0);
        frame(&mut composer, &state);
        frame(&mut composer, &state);
        assert_eq!(state.first_visible(), 1, "the anchor is on the second line");
        assert_eq!(state.offset(), 44.0, "one line plus one spacing");
    }

    /// A line is a wrapper node in the lazy list; its single child is the line, whose children are the
    /// cells. These two helpers read that shape off the arena.
    fn line_shapes(composer: &Composer) -> Vec<usize> {
        let Some(root) = composer.layout_root() else { return Vec::new() };
        root.children
            .clone()
            .into_iter()
            .map(|wrapper| {
                composer.arena_nodes()[wrapper]
                    .children
                    .first()
                    .map(|line| composer.arena_nodes()[*line].children.len())
                    .unwrap_or(0)
            })
            .collect()
    }

    fn cell_widths(composer: &Composer) -> Vec<Vec<f32>> {
        let Some(root) = composer.layout_root() else { return Vec::new() };
        root.children
            .clone()
            .into_iter()
            .map(|wrapper| {
                let Some(line) = composer.arena_nodes()[wrapper].children.first().copied() else {
                    return Vec::new();
                };
                composer.arena_nodes()[line]
                    .children
                    .iter()
                    .map(|cell| composer.arena_nodes()[*cell].measured_size.width)
                    .collect()
            })
            .collect()
    }

    fn grid_frame(composer: &mut Composer, constraints: Constraints, build: impl Fn(&mut ComposeCtx)) {
        composer.compose(crate::compose!(|ctx| {
            build(ctx);
        }));
        composer.layout(constraints);
    }

    fn cell(ctx: &mut ComposeCtx, index: usize) {
        crate::layout::Stack::new()
            .modifier(Modifier::new().size(10.0, 40.0).test_tag(format!("g{index}")))
            .build(ctx, |_| {});
    }

    /// A PARTIAL line keeps the slot width. Weights normalize over the line's own children, so the last
    /// line of any grid whose item count is not a multiple of the cell count stretched its item across
    /// the whole line (measured: 300 dp where a three-cell grid wants 98.67).
    #[test]
    fn a_partial_line_keeps_the_slot_width() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        for _ in 0..2 {
            let s = state.clone();
            grid_frame(&mut composer, Constraints::new(0.0, 300.0, 0.0, 300.0), |ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Fixed(3))
                    .state(s.clone())
                    .cross_spacing(2.0)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .items(4, cell)
                    .build(ctx);
            });
        }
        let widths = cell_widths(&composer);
        assert_eq!(widths.len(), 2, "four items in three-cell lines: {widths:?}");
        assert_eq!(widths[1].len(), 1, "the second line holds one cell: {widths:?}");
        assert!(
            (widths[1][0] - (300.0 - 2.0 * 2.0) / 3.0).abs() < 0.5,
            "the partial line keeps the slot width instead of the whole line: {widths:?}"
        );
    }

    /// A spanning cell OWNS the spacing inside it: spans [1, 2] on a 300 dp line with 12 dp between
    /// cells are 92 / 196 (Compose's slot table), not 96 / 192 (weights over the leftover).
    #[test]
    fn a_span_owns_the_spacing_inside_it() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        for _ in 0..2 {
            let s = state.clone();
            grid_frame(&mut composer, Constraints::new(0.0, 300.0, 0.0, 300.0), |ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Fixed(3))
                    .state(s.clone())
                    .cross_spacing(12.0)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .items_with(
                        2,
                        |index| index as u64,
                        |index| if index == 1 { 2 } else { 1 },
                        cell,
                    )
                    .build(ctx);
            });
        }
        let widths = cell_widths(&composer);
        let unit = (300.0 - 2.0 * 12.0) / 3.0;
        assert!((widths[0][0] - unit).abs() < 0.5, "one slot: {widths:?}");
        assert!(
            (widths[0][1] - (2.0 * unit + 12.0)).abs() < 0.5,
            "two slots plus the spacing between them: {widths:?}"
        );
    }

    /// A changed cell count re-chunks EVERY line: the line key carries the shape, so the lines are
    /// rebuilt instead of skipped (measured before the fix: [2, 3, 1] instead of [3, 3, 1]).
    #[test]
    fn changing_the_cell_count_rechunks_every_line() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        for cells in [2usize, 2, 3, 3] {
            let s = state.clone();
            grid_frame(&mut composer, Constraints::new(0.0, 300.0, 0.0, 300.0), |ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Fixed(cells))
                    .state(s.clone())
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .items(7, cell)
                    .build(ctx);
            });
        }
        assert_eq!(line_shapes(&composer), vec![3, 3, 1]);
    }

    /// Appending items INSIDE the last line shows them: the line count and the first keys do not
    /// change, so only a shape-carrying key can tell the lazy list the content moved (measured before
    /// the fix: 7 items then 9 items stayed 3/3/1 instead of 3/3/3).
    #[test]
    fn appending_inside_the_last_line_shows_the_new_items() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        for count in [7usize, 7, 9, 9] {
            let s = state.clone();
            grid_frame(&mut composer, Constraints::new(0.0, 300.0, 0.0, 300.0), |ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Fixed(3))
                    .state(s.clone())
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .items(count, cell)
                    .build(ctx);
            });
        }
        assert_eq!(line_shapes(&composer), vec![3, 3, 3]);
    }

    /// A changed cross spacing rebuilds the lines (measured before the fix: the cells kept 100 dp).
    #[test]
    fn changing_the_cross_spacing_rebuilds_the_lines() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        for spacing in [0.0f32, 0.0, 12.0, 12.0] {
            let s = state.clone();
            grid_frame(&mut composer, Constraints::new(0.0, 300.0, 0.0, 300.0), |ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Fixed(3))
                    .state(s.clone())
                    .cross_spacing(spacing)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .items(6, cell)
                    .build(ctx);
            });
        }
        let widths = cell_widths(&composer);
        assert!(
            (widths[0][0] - (300.0 - 2.0 * 12.0) / 3.0).abs() < 0.5,
            "the cells pick up the new spacing: {widths:?}"
        );
    }

    /// An unbounded cross axis has no cell count: it falls back to one cell per line instead of
    /// saturating the cast to usize::MAX and putting every item into one line.
    #[test]
    fn an_unbounded_cross_axis_falls_back_to_one_cell() {
        for cross in [f32::INFINITY, f32::MAX] {
            let state = LazyListState::new();
            let mut composer = Composer::new();
            for _ in 0..2 {
                let s = state.clone();
                grid_frame(&mut composer, Constraints::new(0.0, cross, 0.0, 300.0), |ctx| {
                    LazyVerticalGrid::new()
                        .cells(GridCells::Adaptive(100.0))
                        .state(s.clone())
                        .modifier(Modifier::new().height(300.0))
                        .items(6, cell)
                        .build(ctx);
                });
            }
            let shapes = line_shapes(&composer);
            assert!(
                shapes.len() > 1 && shapes.iter().all(|cells| *cells == 1),
                "cross {cross}: one cell per line, never one giant line: {shapes:?}"
            );
        }
    }

    /// Technical check of the Adaptive default path: WITHOUT `.state(...)` the grid must still learn
    /// the cross extent (it remembers a state of its own). Before the fix the internal state was
    /// created inside `LazyList::build`, so the grid never read the cross extent and stayed at one
    /// cell per line forever.
    #[test]
    fn an_adaptive_grid_without_a_state_still_chunks_after_the_first_measure() {
        let mut composer = Composer::new();
        for _ in 0..3 {
            grid_frame(&mut composer, Constraints::new(0.0, 320.0, 0.0, 300.0), |ctx| {
                LazyVerticalGrid::new()
                    .cells(GridCells::Adaptive(100.0))
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .items(6, cell)
                    .build(ctx);
            });
        }
        let shapes = line_shapes(&composer);
        assert!(
            shapes.iter().all(|cells| *cells == 3),
            "three 100 dp cells fit the 320 dp width without an external state: {shapes:?}"
        );
    }
}
