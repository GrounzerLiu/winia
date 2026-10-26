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
use crate::layout::{Column, Row, Stack};
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
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
        let cross = self
            .state
            .as_ref()
            .map(|state| state.viewport_cross_size())
            .unwrap_or(0.0);
        let cells_per_line = self.cells.count(cross, self.cross_spacing);
        let lines = chunk_lines(content.count, cells_per_line, content.span.as_ref());
        let line_keys: Vec<u64> = lines
            .iter()
            .map(|line| line.items.first().map(|(index, _)| (content.key)(*index)).unwrap_or(0))
            .collect();
        let lines = Arc::new(lines);
        let line_keys = Arc::new(line_keys);
        let content = content.content;
        let cross_spacing = self.cross_spacing;
        let mut list = LazyList::<A>::new_list()
            .state(self.state.unwrap_or_else(LazyListState::new))
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
                build_line::<A>(ctx, &lines[line].items, cross_spacing, content.as_ref());
            },
        )
        .build(ctx);
    }
}

/// Composes one line: a Row for a vertical grid, a Column for a horizontal one. A cell's
/// `layout_weight` is its span, so a two-cell item takes twice the space of a one-cell item.
fn build_line<A: LazyAxis>(
    ctx: &mut ComposeCtx,
    items: &[(usize, u32)],
    cross_spacing: f32,
    content: &(dyn Fn(&mut ComposeCtx, usize) + Send + Sync),
) {
    let weights: Vec<f32> = items.iter().map(|(_, span)| *span as f32).collect();
    if A::VERTICAL {
        Row::new()
            .modifier(Modifier::new().fill_max_width())
            .spacing(cross_spacing)
            .build(ctx, |ctx| {
                for (i, (index, _)) in items.iter().enumerate() {
                    Stack::new()
                        .modifier(Modifier::new().layout_weight(weights[i]))
                        .build(ctx, |ctx| content(ctx, *index));
                }
            });
    } else {
        Column::new()
            .modifier(Modifier::new().fill_max_height())
            .spacing(cross_spacing)
            .build(ctx, |ctx| {
                for (i, (index, _)) in items.iter().enumerate() {
                    Stack::new()
                        .modifier(Modifier::new().layout_weight(weights[i]))
                        .build(ctx, |ctx| content(ctx, *index));
                }
            });
    }
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
}
