//! FlowRow: the measure policy and the builder for a horizontally-flowing row.

use crate::layout::container::build_container;
use crate::layout::{
    Alignment, Arrangement, Constraints, HorizontalAxis, LayoutDirection, LayoutNode,
    MeasurePolicy, Placement,
};
use crate::unit::Size;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

use super::{measure::measure_flow, placement::PlacementAnimations, FlowOverflow};

/// The FlowRow measure policy: a horizontal main axis that wraps when a line is too wide.
///
/// Non-exhaustive: construct it with [`FlowRowLayout::new`] and the chainable setters, so new
/// internal animation state does not become part of a caller's struct literal.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FlowRowLayout {
    /// Main-axis arrangement of the items inside each line (Compose `horizontalArrangement`).
    pub arrangement: Arrangement,
    /// Default cross-axis alignment of an item inside its line (Compose `itemVerticalAlignment`).
    /// A child's `align_self` overrides it.
    pub alignment: Alignment,
    /// Distribution of the free cross-axis space between lines (Compose `verticalArrangement`).
    pub cross_arrangement: Arrangement,
    /// Gap between adjacent items in one line, in logical pixels.
    pub main_spacing: f32,
    /// Gap between adjacent lines, in logical pixels.
    pub cross_spacing: f32,
    /// Maximum number of items per line. `usize::MAX` means unlimited; zero produces an empty flow.
    pub max_items_in_row: usize,
    /// Maximum number of lines. `usize::MAX` means unlimited; zero produces an empty flow.
    /// `FlowOverflow::Visible` ignores this cap.
    pub max_lines: usize,
    /// What happens to items beyond the cross-axis budget.
    pub overflow: FlowOverflow,
    /// Direction used to mirror the final placements.
    pub direction: LayoutDirection,
    /// Per-child offset states for `animate_item` placement animations; `None` disables the
    /// scan and keeps the default Flow free of animation bookkeeping.
    pub(crate) placement_animations: Option<PlacementAnimations>,
}

impl FlowRowLayout {
    /// Creates an unlimited row with Start arrangements, zero spacing and Clip overflow.
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
            placement_animations: None,
        }
    }

    /// Sets the main-axis arrangement inside a line.
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    /// Sets the default cross-axis alignment of an item inside its line.
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    /// Sets the distribution of free cross-axis space between lines.
    pub fn cross_arrangement(mut self, a: Arrangement) -> Self { self.cross_arrangement = a; self }
    /// Sets the gap between items in one line.
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    /// Sets the gap between lines.
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    /// Sets the maximum number of items per line.
    pub fn max_items_in_row(mut self, m: usize) -> Self { self.max_items_in_row = m; self }
    /// Sets the maximum number of lines.
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    /// Selects the overflow policy.
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }
    /// Sets the layout direction used for mirroring.
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }

    pub(crate) fn placement_animations(mut self, animations: Option<PlacementAnimations>) -> Self {
        self.placement_animations = animations;
        self
    }
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
        if let Some(animations) = &self.placement_animations {
            animations.place(nodes, children, placements);
        }
    }

    fn min_intrinsic_width(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        super::intrinsic::min_main(
            ctx,
            children,
            true,
            height,
            self.main_spacing,
            self.cross_spacing,
            self.max_lines,
            self.max_items_in_row,
            self.overflow,
        )
    }

    fn max_intrinsic_width(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        super::intrinsic::max_main(
            ctx,
            children,
            true,
            height,
            self.main_spacing,
            self.max_items_in_row,
            self.max_lines,
        )
    }

    fn min_intrinsic_height(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        super::intrinsic::min_cross(
            ctx,
            children,
            true,
            width,
            self.main_spacing,
            self.cross_spacing,
            self.max_lines,
            self.max_items_in_row,
            self.overflow,
        )
    }

    fn max_intrinsic_height(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        super::intrinsic::max_cross(
            ctx,
            children,
            true,
            width,
            self.main_spacing,
            self.cross_spacing,
            self.max_lines,
            self.max_items_in_row,
            self.overflow,
        )
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
/// Visible (keep painting them). Children may carry `layout_weight` /
/// `layout_weight_fill`; line breaking uses their natural size. With `animate_placement(true)`, a
/// child root marked with `Modifier::animate_item()` gets a two-dimensional paint spring when a
/// reflow moves it; use `ctx.key` / `ctx.keyed_range` so its identity survives the move. There is no overflow-indicator
/// API yet; see the `layout::flow` module docs.
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
    /// Whether a change in placement should spring instead of jumping.
    animate_placement: bool,
}

impl FlowRow {
    /// Creates a row with Start arrangements, zero spacing, unlimited items and Clip overflow.
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
            animate_placement: false,
        }
    }

    /// Appends the caller's modifier. The modifier is placed on the flow container itself, so
    /// layout, paint and semantics attributes travel with the container.
    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    /// Sets the main-axis arrangement inside a line.
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    /// Sets the default cross-axis alignment of an item inside its line; `align_self` overrides it.
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    /// Sets the distribution of free cross-axis space between lines.
    pub fn cross_arrangement(mut self, a: Arrangement) -> Self { self.cross_arrangement = a; self }
    /// Sets the gap between items in one line.
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    /// Sets the gap between lines.
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    /// Sets the maximum number of items per line. Zero produces an empty flow.
    pub fn max_items_in_row(mut self, m: usize) -> Self { self.max_items_in_row = m; self }
    /// Sets the maximum number of lines. Zero produces an empty flow; Visible ignores the cap.
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    /// Selects the overflow policy.
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }
    /// Enables placement springs for children marked with `Modifier::animate_item()`.
    /// Defaults to `false`, which keeps the common Flow path free of animation bookkeeping.
    pub fn animate_placement(mut self, animate: bool) -> Self { self.animate_placement = animate; self }

    /// Composes the row and its children.
    ///
    /// The content closure is deliberately not wrapped in a component scope (like `Row` and
    /// `Column`): a `State::get()` at the top of the content must bubble to the parent that
    /// composed it. Every builder parameter that changes the layout is declared with
    /// `ctx.changed` before the restartable group starts.
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
        ctx.changed(&self.animate_placement);
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&dir);
        // Registered unconditionally so toggling the flag cannot shift later remember slots;
        // the table itself stays allocation-free until the flag is on.
        let placement_table = ctx
            .remember_at_key(ctx.position_key(0x666c6f77), || PlacementAnimations::default())
            .peek();
        let placement_animations = self.animate_placement.then_some(placement_table);
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
                .placement_animations(placement_animations)
                .direction(dir),
            content,
        );
    }
}

impl Default for FlowRow {
    fn default() -> Self { Self::new() }
}
