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
///
/// Non-exhaustive: construct it with [`FlowColumnLayout::new`] and the chainable setters, so new
/// internal animation state does not become part of a caller's struct literal.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FlowColumnLayout {
    /// Main-axis arrangement of the items inside each column (Compose `verticalArrangement`).
    pub arrangement: Arrangement,
    /// Default cross-axis alignment of an item inside its column (Compose `itemHorizontalAlignment`).
    /// A child's `align_self` overrides it.
    pub alignment: Alignment,
    /// Distribution of the free cross-axis space between columns (Compose `horizontalArrangement`).
    pub cross_arrangement: Arrangement,
    /// Gap between adjacent items in one column, in logical pixels.
    pub main_spacing: f32,
    /// Gap between adjacent columns, in logical pixels.
    pub cross_spacing: f32,
    /// Maximum number of items per column. `usize::MAX` means unlimited; zero produces an empty flow.
    pub max_items_in_column: usize,
    /// Maximum number of columns. `usize::MAX` means unlimited; zero produces an empty flow.
    /// `FlowOverflow::Visible` ignores this cap.
    pub max_lines: usize,
    /// What happens to items beyond the cross-axis budget.
    pub overflow: FlowOverflow,
    /// Direction used to mirror the final placements.
    pub direction: LayoutDirection,
    /// Shared per-item placement/fade runtime; only items carrying `animate_item` allocate state.
    pub(crate) item_animations: crate::layout::item_animation::ItemAnimationHost,
}

impl FlowColumnLayout {
    /// Creates an unlimited column with Start arrangements, zero spacing and Clip overflow.
    pub fn new() -> Self {
        FlowColumnLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            cross_arrangement: Arrangement::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_column: usize::MAX,
            max_lines: usize::MAX,
            overflow: FlowOverflow::Clip,
            direction: LayoutDirection::Ltr,
            item_animations: crate::layout::item_animation::ItemAnimationHost::default(),
        }
    }

    /// Sets the main-axis arrangement inside a column.
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    /// Sets the default cross-axis alignment of an item inside its column.
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    /// Sets the distribution of free cross-axis space between columns.
    pub fn cross_arrangement(mut self, a: Arrangement) -> Self { self.cross_arrangement = a; self }
    /// Sets the gap between items in one column.
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    /// Sets the gap between columns.
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    /// Sets the maximum number of items per column.
    pub fn max_items_in_column(mut self, m: usize) -> Self { self.max_items_in_column = m; self }
    /// Sets the maximum number of columns.
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    /// Selects the overflow policy.
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }
    /// Sets the layout direction used for mirroring.
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }

    pub(crate) fn item_animations(
        mut self,
        animations: crate::layout::item_animation::ItemAnimationHost,
    ) -> Self {
        self.item_animations = animations;
        self
    }
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
            self.cross_arrangement,
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
        self.item_animations.begin_frame();
        let mut seen = std::collections::HashSet::new();
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
            nodes[c].item_paint = None;
            let key = if nodes[c].slot_key != 0 {
                nodes[c].slot_key
            } else {
                nodes[c].id
            };
            if !seen.insert(key) {
                debug_assert!(false, "Flow item key {key} is used by more than one child");
                continue;
            }
            if placements[i].size == Size::ZERO {
                // Cross-axis Clip dropped this line, but the child is still live. Retire its
                // channels without an exit: finish_frame must not turn a visible-again card into a
                // ghost. (The zero placement also must not become a (0,0) animation origin.)
                self.item_animations.retire(key);
                continue;
            }
            let Some(spec) = nodes[c].modifier.animate_item_spec() else {
                // The child is still live, but its marker/spec is gone. Drop its channels and
                // picture; finish_frame would otherwise mistake it for a removed item and draw a
                // ghost over the live card.
                self.item_animations.retire(key);
                continue;
            };
            let position = nodes[c].position;
            self.item_animations
                .place(nodes, c, key, position, &spec);
        }
        self.item_animations.finish_frame(None);
    }

    fn min_intrinsic_width(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        super::intrinsic::min_cross(
            ctx,
            children,
            false,
            height,
            self.main_spacing,
            self.cross_spacing,
            self.max_lines,
            self.max_items_in_column,
            self.overflow,
        )
    }

    fn max_intrinsic_width(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        super::intrinsic::max_cross(
            ctx,
            children,
            false,
            height,
            self.main_spacing,
            self.cross_spacing,
            self.max_lines,
            self.max_items_in_column,
            self.overflow,
        )
    }

    fn min_intrinsic_height(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        super::intrinsic::min_main(
            ctx,
            children,
            false,
            width,
            self.main_spacing,
            self.cross_spacing,
            self.max_lines,
            self.max_items_in_column,
            self.overflow,
        )
    }

    fn max_intrinsic_height(
        &self,
        ctx: &mut crate::layout::IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        super::intrinsic::max_main(
            ctx,
            children,
            false,
            width,
            self.main_spacing,
            self.max_items_in_column,
            self.max_lines,
        )
    }
}


/// A flowing column: items run down the vertical main axis and wrap into a new column when the
/// current one is full (menus, vertical tag lists, wrapping toolbars).
///
/// Mirrors `FlowColumn(verticalArrangement, horizontalArrangement, maxItemsInEachColumn)`:
/// `main_spacing` is the gap inside a column, `cross_spacing` the gap between columns,
/// `cross_arrangement` the distribution of free cross space, `max_items_in_column` the per-column
/// cap, `max_lines` the column cap (Clip mode; Visible bypasses it), and `overflow` selects Clip
/// or Visible. Children may carry `layout_weight` / `layout_weight_fill`; line breaking uses
/// their natural size. A child root marked with `Modifier::animate_item(...)` gets a
/// two-dimensional paint animation when a reflow moves it; the marker carries its own placement
/// and fade curves. Use `ctx.key` / `ctx.keyed_range` so its identity survives the move. There is
/// no overflow-indicator API yet; see the `layout::flow` module docs.
pub struct FlowColumn {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    cross_arrangement: Arrangement,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_column: usize,
    max_lines: usize,
    overflow: FlowOverflow,
}

impl FlowColumn {
    /// Creates a column with Start arrangements, zero spacing, unlimited items and Clip overflow.
    pub fn new() -> Self {
        FlowColumn {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            cross_arrangement: Arrangement::Start,
            main_spacing: 0.0,
            cross_spacing: 0.0,
            max_items_in_column: usize::MAX,
            max_lines: usize::MAX,
            overflow: FlowOverflow::Clip,
        }
    }

    /// Appends the caller's modifier. The modifier is placed on the flow container itself, so
    /// layout, paint and semantics attributes travel with the container.
    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    /// Sets the main-axis arrangement inside a column.
    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    /// Sets the default cross-axis alignment of an item inside its column; `align_self` overrides it.
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    /// Sets the distribution of free cross-axis space between columns.
    pub fn cross_arrangement(mut self, a: Arrangement) -> Self { self.cross_arrangement = a; self }
    /// Sets the gap between items in one column.
    pub fn main_spacing(mut self, s: f32) -> Self { self.main_spacing = s; self }
    /// Sets the gap between columns.
    pub fn cross_spacing(mut self, s: f32) -> Self { self.cross_spacing = s; self }
    /// Sets the maximum number of items per column. Zero produces an empty flow.
    pub fn max_items_in_column(mut self, m: usize) -> Self { self.max_items_in_column = m; self }
    /// Sets the maximum number of columns. Zero produces an empty flow; Visible ignores the cap.
    pub fn max_lines(mut self, m: usize) -> Self { self.max_lines = m; self }
    /// Selects the overflow policy.
    pub fn overflow(mut self, o: FlowOverflow) -> Self { self.overflow = o; self }

    /// Composes the column and its children.
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
        ctx.changed(&self.max_items_in_column);
        ctx.changed(&self.max_lines);
        ctx.changed(&self.overflow);
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&dir);
        // The host is cheap and always present; item state is only allocated for markers.
        let item_animations = ctx
            .remember_at_key(ctx.position_key(0x666c6f77), || {
                crate::layout::item_animation::ItemAnimationHost::default()
            })
            .peek();
        let modifier = self
            .modifier
            .then(Modifier::new().with_item_animation_host(item_animations.clone()));
        build_container(
            ctx,
            modifier,
            FlowColumnLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .cross_arrangement(self.cross_arrangement)
                .main_spacing(self.main_spacing)
                .cross_spacing(self.cross_spacing)
                .max_items_in_column(self.max_items_in_column)
                .max_lines(self.max_lines)
                .overflow(self.overflow)
                .item_animations(item_animations)
                .direction(dir),
            content,
        );
    }
}

impl Default for FlowColumn {
    fn default() -> Self { Self::new() }
}
