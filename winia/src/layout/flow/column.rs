//! FlowColumn: the measure policy and the builder for a vertically-flowing column.

use crate::layout::container::build_container;
use crate::layout::{
    Alignment, Arrangement, Constraints, LayoutDirection, LayoutNode, MeasurePolicy, Placement,
    VerticalAxis,
};
use crate::unit::Size;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

use super::{measure::measure_flow, FlowOverflow};

/// The FlowColumn measure policy: a vertical main axis that wraps when a column is too tall.
#[derive(Debug, Clone)]
pub struct FlowColumnLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub main_spacing: f32,
    pub cross_spacing: f32,
    pub max_items_in_column: usize,
    pub max_lines: usize,
    pub overflow: FlowOverflow,
    pub direction: LayoutDirection,
}

impl FlowColumnLayout {
    pub fn new() -> Self {
        FlowColumnLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_column: usize::MAX,
            max_lines: usize::MAX,
            overflow: FlowOverflow::Clip,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_column(mut self, m: usize) -> Self { self.max_items_in_column = m; self }
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for FlowColumnLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for FlowColumnLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        measure_flow::<VerticalAxis>(
            self.arrangement,
            self.alignment,
            self.main_spacing,
            self.cross_spacing,
            self.max_items_in_column,
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


/// A flowing column: the vertical main axis wraps once it is full. Mirrors `FlowColumn`, with the
/// same parameters, `max_lines`, and Clip/Visible overflow modes as FlowRow.
pub struct FlowColumn {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_column: usize,
    max_lines: usize,
    overflow: FlowOverflow,
}

impl FlowColumn {
    pub fn new() -> Self {
        FlowColumn {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_column: usize::MAX,
            max_lines: usize::MAX,
            overflow: FlowOverflow::Clip,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    pub fn max_items_in_column(mut self, m: usize) -> Self { self.max_items_in_column = m; self }
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }

    /// Deliberately not a macro (like Row/Column).
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.main_spacing);
        ctx.changed(&self.cross_spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        ctx.changed(&self.max_items_in_column);
        ctx.changed(&self.max_lines);
        ctx.changed(&self.overflow);
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&dir);
        build_container(
            ctx,
            self.modifier,
            FlowColumnLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .main_spacing(self.main_spacing)
                .cross_spacing(self.cross_spacing)
                .max_items_in_column(self.max_items_in_column)
                .max_lines(self.max_lines)
                .overflow(self.overflow)
                .direction(dir),
            content,
        );
    }
}

impl Default for FlowColumn {
    fn default() -> Self { Self::new() }
}
