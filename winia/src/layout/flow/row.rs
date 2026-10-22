//! FlowRow: the measure policy and the builder for a horizontally-flowing row.

use crate::layout::container::build_container;
use crate::layout::{
    Alignment, Arrangement, Constraints, HorizontalAxis, LayoutDirection, LayoutNode,
    MeasurePolicy, Placement,
};
use crate::unit::Size;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

use super::{measure::measure_flow, FlowOverflow};

/// The FlowRow measure policy: a horizontal main axis that wraps when a line is too wide.
#[derive(Debug, Clone)]
pub struct FlowRowLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub cross_arrangement: Arrangement,
    pub main_spacing: f32,
    pub cross_spacing: f32,
    pub max_items_in_row: usize,
    pub max_lines: usize,
    pub overflow: FlowOverflow,
    pub direction: LayoutDirection,
}

impl FlowRowLayout {
    pub fn new() -> Self {
        FlowRowLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            cross_arrangement: Arrangement::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_row: usize::MAX,
            max_lines: usize::MAX,
            overflow: FlowOverflow::Clip,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn cross_arrangement(mut self, a: Arrangement) -> Self { self.cross_arrangement = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_row(mut self, m: usize) -> Self { self.max_items_in_row = m; self }
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for FlowRowLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for FlowRowLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        measure_flow::<HorizontalAxis>(
            self.arrangement,
            self.alignment,
            self.cross_arrangement,
            self.main_spacing,
            self.cross_spacing,
            self.max_items_in_row,
            self.max_lines,
            self.overflow,
            self.direction,
            nodes,
            policies,
            children,
            &constraints,
        )
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
        }
    }
}


/// A flowing row: a line wraps once the main axis is full (chip groups, tag clouds, wrapping
/// toolbars).
///
/// Mirrors `FlowRow(horizontalArrangement, verticalArrangement, maxItemsInEachRow)`:
/// `main_spacing` is the gap inside a line, `cross_spacing` the gap between lines,
/// `cross_arrangement` the distribution of the free cross space, and `max_items_in_row` the
/// per-line cap (unlimited by default), `max_lines` the row cap (Clip mode; Visible bypasses it),
/// and `overflow` chooses between Compose's Clip (drop items past the cross-axis budget) and
/// Visible (keep painting them). v1 has no weight or overflow-indicator
/// API; see the `layout::flow` module docs.
pub struct FlowRow {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    cross_arrangement: Arrangement,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_row: usize,
    max_lines: usize,
    overflow: FlowOverflow,
}

impl FlowRow {
    pub fn new() -> Self {
        FlowRow {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            cross_arrangement: Arrangement::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_row: usize::MAX,
            max_lines: usize::MAX,
            overflow: FlowOverflow::Clip,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn cross_arrangement(mut self, a: Arrangement) -> Self { self.cross_arrangement = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_row(mut self, m: usize) -> Self { self.max_items_in_row = m; self }
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }

    /// Deliberately not a macro (like Row/Column): a State.get() at the content's top level has to
    /// bubble up to the parent.
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.main_spacing);
        ctx.changed(&self.cross_spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        ctx.changed(&self.cross_arrangement);
        // max_items_in_row is a usize with PartialEq, so it can be declared directly.
        ctx.changed(&self.max_items_in_row);
        ctx.changed(&self.max_lines);
        ctx.changed(&self.overflow);
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&dir);
        build_container(
            ctx,
            self.modifier,
            FlowRowLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .cross_arrangement(self.cross_arrangement)
                .main_spacing(self.main_spacing)
                .cross_spacing(self.cross_spacing)
                .max_items_in_row(self.max_items_in_row)
                .max_lines(self.max_lines)
                .overflow(self.overflow)
                .direction(dir),
            content,
        );
    }
}

impl Default for FlowRow {
    fn default() -> Self { Self::new() }
}
