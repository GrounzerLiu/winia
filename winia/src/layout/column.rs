//! Column layout — delegates to flex::VerticalAxis.
//!
//! Main axis = vertical (height), cross axis = horizontal (width).
//!
//! What it offers:
//! - per-child alignment: `Modifier::align_self(Alignment::Center)` overrides the Column default
//! - weight: `Modifier::layout_weight(2.0)` shares the leftover height proportionally
//! - spacing: a fixed gap between children
//!
//! The implementation delegates to `flex::measure_flex::<VerticalAxis>()`.

use super::constraints::Constraints;
use crate::unit::Size;
use super::flex;
use super::node::*;
use super::container::build_container;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

/// Column's layout policy.
#[derive(Debug, Clone)]
pub struct ColumnLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub spacing: f32,
    pub direction: LayoutDirection,
}

impl ColumnLayout {
    pub fn new() -> Self {
        ColumnLayout {
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
            direction: LayoutDirection::Ltr,
        }
    }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }
    pub fn direction(mut self, d: LayoutDirection) -> Self { self.direction = d; self }
}

impl Default for ColumnLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for ColumnLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        flex::measure_flex::<super::axis::VerticalAxis>(
            self.arrangement,
            self.alignment,
            self.spacing,
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

    // ── Intrinsic measurement ──
    //
    // Column is the vertical half of Compose's `IntrinsicMeasureBlocks` (RowColumnImpl.kt:261-369):
    // a HEIGHT query is the main axis (and prices weighted children by weight unit), a WIDTH query
    // is the cross axis (and first resolves how much main-axis room each child gets).

    /// `VerticalMinHeight`: `intrinsicMainAxisSize(measurables, minIntrinsicHeight(w), availableWidth)`
    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        flex::flex_intrinsic_main(ctx, children, IntrinsicQuery::MinHeight, width, self.spacing)
    }

    /// `VerticalMaxHeight`
    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        flex::flex_intrinsic_main(ctx, children, IntrinsicQuery::MaxHeight, width, self.spacing)
    }

    /// `VerticalMinWidth`: `intrinsicCrossAxisSize(measurables, maxIntrinsicHeight, minIntrinsicWidth, availableHeight)`
    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        flex::flex_intrinsic_cross(
            ctx,
            children,
            IntrinsicQuery::MaxHeight,
            IntrinsicQuery::MinWidth,
            height,
            self.spacing,
        )
    }

    /// `VerticalMaxWidth`
    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        flex::flex_intrinsic_cross(
            ctx,
            children,
            IntrinsicQuery::MaxHeight,
            IntrinsicQuery::MaxWidth,
            height,
            self.spacing,
        )
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_arrangement_still_mirrors_the_column_cross_axis() {
        // Absolute is about the MAIN axis. A Column's main axis is y, so the cross-axis mirror RTL
        // needs (Start on the right) has to keep working whatever the arrangement is.
        let column = ColumnLayout::new()
            .arrangement(Arrangement::Absolute(AbsoluteArrangement::Start))
            .alignment(Alignment::Start)
            .direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(30.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) =
            column.measure(&mut nodes, &[], &children, Constraints::new(100.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].position.x, 70.0, "the child hugs the RIGHT edge in RTL");
    }

    fn make_leaf(width: f32, height: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
        node.measured_size = Size::new(width, height);
        node
    }

    /// A weighted child with a height of its own, the shape material3's date picker dialog uses
    /// (`Box(Modifier.weight(1f, fill = false))` around a picker that sizes itself).
    fn make_weighted(height: f32, fill: bool) -> LayoutNode {
        use crate::modifier::Modifier;
        LayoutNode::leaf(Modifier::new().layout_weight_fill(1.0, fill).height(height))
    }

    /// Compose's `weight(weight, fill = false)`: the share is the child's MAXIMUM, and the container
    /// keeps what the child asked for. This is the half that lets a dialog be shorter than the cap
    /// it is allowed (`DatePickerDialog.android.kt:90-95`), so it is pinned here rather than only
    /// through a window.
    #[test]
    fn a_weight_that_does_not_fill_keeps_the_childs_own_height() {
        let mut nodes = vec![make_weighted(20.0, false), make_leaf(100.0, 30.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, 500.0),
        );
        assert_eq!(
            size.height, 50.0,
            "the column is content + sibling, not the 500 the parent offered"
        );
        assert_eq!(placements[0].size.height, 20.0, "the child keeps its own height");
        assert_eq!(placements[1].position.y, 20.0, "the sibling follows the content");
    }

    /// The default `Modifier::layout_weight` still fills: the share is exact, so the same two
    /// children come out at the parent's whole height.
    #[test]
    fn a_weight_that_fills_takes_its_whole_share() {
        let mut nodes = vec![make_weighted(20.0, true), make_leaf(100.0, 30.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, 500.0),
        );
        assert_eq!(size.height, 500.0, "a filling weight takes the whole bounded axis");
        assert_eq!(placements[0].size.height, 470.0, "500 less the sibling's 30");
    }

    /// A spreading arrangement does not grow a content-sized container: the same two children as the
    /// test above come out at their own 50 dp, and the sibling sits directly under the first rather
    /// than at the parent's bottom.
    ///
    /// This is Compose's rule — `mainAxisLayoutSize = max((fixedSpace + weightedSpace)
    /// .fastCoerceAtLeast(0), mainAxisMin)` (`RowColumnMeasurePolicy.kt:252`) never consults
    /// `mainAxisMax`, so a container with no explicit size hugs its content and SpaceBetween has no
    /// leftover to distribute. winia used to grow the container to the offered maximum instead; the
    /// pair of tests here is what pins which of the two it does.
    ///
    /// It also makes the date picker dialog's collapse work: a `weight(1f, fill = false)` box can only
    /// shorten the column if the arrangement leaves the size alone (`DatePickerDialog.android.kt:89-95`
    /// writes `SpaceBetween` there, and winia's dialog uses `Start` — after this alignment the two
    /// place identically).
    #[test]
    fn a_spreading_arrangement_does_not_grow_a_content_sized_container() {
        let mut nodes = vec![make_weighted(20.0, false), make_leaf(100.0, 30.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = ColumnLayout::new()
            .arrangement(Arrangement::SpaceBetween)
            .measure(
                &mut nodes,
                &[],
                &children,
                Constraints::new(0.0, 100.0, 0.0, 500.0),
            );
        assert_eq!(
            size.height, 50.0,
            "content height, not the 500 dp the parent offered"
        );
        assert_eq!(
            placements[1].position.y, 20.0,
            "with no leftover the sibling follows the content"
        );
    }

    /// A child that asks for more than its share comes out at its share, which is what Compose's
    /// `size()` does (`enforceIncoming = true`: `SizeNode.measure` runs
    /// `constraints.constrain(Constraints.fixed(...))` on the request). It used to come out at its
    /// own request and push its sibling out of the column; this test asserted that, as a recorded
    /// divergence, until `layout/node.rs` started coercing a `Size` into the incoming range.
    ///
    /// The overflow that a caller really wants is still available through `Modifier::required_size`,
    /// which keeps writing over the range — `shared_transition.rs`'s spilling hero relies on it.
    ///
    /// The companion cases are above: `a_weight_that_fills_takes_its_whole_share` (the share wins
    /// over a smaller request) and `a_weight_that_does_not_fill_keeps_the_childs_own_height`.
    #[test]
    fn an_oversized_non_filling_weight_is_coerced_into_its_share() {
        use crate::modifier::Modifier;
        let mut nodes = vec![
            LayoutNode::leaf(
                Modifier::new()
                    .layout_weight_fill(1.0, false)
                    .size(100.0, 600.0),
            ),
            make_leaf(100.0, 30.0),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 100.0, 0.0, 500.0),
        );
        assert_eq!(
            size.height, 500.0,
            "the share plus the sibling still add up to the parent's bound"
        );
        assert_eq!(
            placements[0].size.height, 470.0,
            "600 was coerced into the 470 dp share rather than overflowing it"
        );
        assert_eq!(
            placements[1].position.y, 470.0,
            "so the sibling stays inside the column"
        );
    }

    /// A spacing that does not fit collapses the child after it — in Compose too.
    ///
    /// Compose charges the gap to `fixedSpace` as it goes, so the next child is measured against what
    /// is left and can end up with nothing: `remaining = mainAxisMax - fixedSpace`, then
    /// `spaceAfterLastNoWeight = min(arrangementSpacingInt, (remaining - placeableMainAxisSize)
    /// .fastCoerceAtLeast(0))` and `fixedSpace += placeableMainAxisSize + spaceAfterLastNoWeight`
    /// (`RowColumnMeasurePolicy.kt:123-145`). The `min` there only stops `fixedSpace` from exceeding
    /// the axis — a 60 dp gap after a 40 dp child in a 100 dp column consumes the whole hundred, so
    /// the second child measures 0. winia's phase 1 subtracts the spacing up front and clamps the
    /// remaining at 0 (`layout/flex.rs`), which lands in the same place.
    ///
    /// This was written as a "known divergence" first, from reading the `min` alone and assuming
    /// Compose kept the child. It does not: the two agree, and this pins that they agree.
    #[test]
    fn a_spacing_too_large_for_the_axis_collapses_the_child_after_it() {
        let mut nodes = vec![make_leaf(100.0, 40.0), make_leaf(100.0, 40.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = ColumnLayout::new()
            .spacing(60.0)
            .measure(
                &mut nodes,
                &[],
                &children,
                Constraints::new(0.0, 100.0, 0.0, 100.0),
            );
        assert_eq!(placements[0].size.height, 40.0, "the first child keeps its height");
        assert_eq!(
            placements[1].size.height, 0.0,
            "the 60 dp gap took the rest of the axis, as it does in Compose"
        );
        assert_eq!(size.height, 100.0, "the column is still its bound");
    }

    /// A zero-sized child still charges its gap, and it does so in both implementations: winia
    /// charges the gap of every preceding non-weighted child (phase 1 of `layout/flex.rs`), which
    /// is Compose's `spaceAfterLastNoWeight = min(30, 100 - 0) = 30` — the second child is measured
    /// against 70 (`RowColumnMeasurePolicy.kt:123-145`).
    #[test]
    fn a_zero_sized_child_charges_its_gap() {
        let mut nodes = vec![make_leaf(0.0, 0.0), make_leaf(100.0, 100.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) = ColumnLayout::new()
            .spacing(30.0)
            .measure(
                &mut nodes,
                &[],
                &children,
                Constraints::new(0.0, 100.0, 0.0, 100.0),
            );
        assert_eq!(
            placements[1].size.height, 70.0,
            "the second child is measured against the axis less the empty child's 30 dp gap"
        );
    }

    #[test]
    fn a_column_stacks_its_children_top_to_bottom() {
        let column = ColumnLayout::new();
        let mut nodes = vec![make_leaf(100.0, 20.0), make_leaf(80.0, 30.0), make_leaf(120.0, 10.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = column.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.height, 60.0); // 20+30+10
        assert_eq!(placements[0].position.y, 0.0);
        assert_eq!(placements[1].position.y, 20.0);
        assert_eq!(placements[2].position.y, 50.0);
    }

    #[test]
    fn a_tight_parent_width_leaves_the_children_loose() {
        // A parent with a tight cross axis (a fill_max_size + padding Column): the children get a
        // LOOSE cross axis (min = 0), so they take their natural width instead of the parent's tight
        // one, as Compose's Column does.
        let mut nodes = vec![make_leaf(100.0, 20.0)];
        let children: Vec<usize> = vec![0];
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes, &[], &children,
            Constraints::new(432.0, 432.0, 0.0, 1000.0),
        );
        assert_eq!(placements[0].size.width, 100.0, "the child keeps its natural width, not the tight parent's");
        assert_eq!(size.width, 432.0, "the Column itself honours the tight parent (432); only the children are loose");
    }

    #[test]
    fn a_fill_max_width_child_fills_a_tight_parent() {
        // Under a tight parent a fill_max_width child still fills: an explicit fill beats the loose range.
        use crate::modifier::Modifier;
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().fill_max_width().height(20.0)),
        ];
        let children: Vec<usize> = vec![0];
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes, &[], &children,
            Constraints::new(432.0, 432.0, 0.0, 1000.0),
        );
        assert_eq!(placements[0].size.width, 432.0, "fill_max_width fills");
        assert_eq!(size.width, 432.0);
    }

    #[test]
    fn stretch_alignment_widens_the_child_to_the_container() {
        // Stretch alignment pulls the child to the container's width (a placement-time stretch, so the
        // loose range does not matter).
        let mut nodes = vec![make_leaf(100.0, 20.0)];
        let children: Vec<usize> = vec![0];
        let (size, placements) = ColumnLayout::new().alignment(Alignment::Stretch).measure(
            &mut nodes, &[], &children,
            Constraints::new(432.0, 432.0, 0.0, 1000.0),
        );
        assert_eq!(placements[0].size.width, 432.0, "a Stretch child fills the container's width");
        assert_eq!(size.width, 432.0);
    }

    #[test]
    fn spacing_separates_a_columns_children() {
        let column = ColumnLayout::new().spacing(5.0);
        let mut nodes = vec![make_leaf(100.0, 20.0), make_leaf(80.0, 30.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = column.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.height, 55.0); // 20+5+30
        assert_eq!(placements[1].position.y, 25.0);
    }

    #[test]
    fn end_alignment_pushes_each_child_to_the_end_edge() {
        let column = ColumnLayout::new().alignment(Alignment::End);
        let mut nodes = vec![make_leaf(50.0, 20.0), make_leaf(100.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_size, placements) = column.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(placements[0].position.x, 100.0 - 50.0);
        assert_eq!(placements[1].position.x, 0.0);
    }
}
pub struct Column {
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
}

impl Column {
    pub fn new() -> Self {
        Column {
            modifier: Modifier::new(),
            arrangement: Arrangement::Start,
            alignment: Alignment::Start,
            spacing: 0.0,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }

    pub fn arrangement(mut self, a: Arrangement) -> Self { self.arrangement = a; self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s; self }

    /// NOT a macro, on purpose: a macro would open an inner scope, the content's top-level
    /// `State::get()` would register on THAT scope, the parent would never learn about it and the
    /// content would stop re-running (the coupling dies).
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // Record the parameters: a later frame compares them against `slot.params` — unchanged
        // spacing/arrangement/alignment means the container Skips (the content does not re-run), a
        // change means Enter (it does; this closes the "parameter changed but the old value stayed"
        // hole).
        ctx.changed(&self.spacing);
        ctx.changed(&self.arrangement);
        ctx.changed(&self.alignment);
        // Direction: an explicit modifier wins over the CompositionLocal default.
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        build_container(
            ctx,
            self.modifier,
            ColumnLayout::new()
                .arrangement(self.arrangement)
                .alignment(self.alignment)
                .spacing(self.spacing)
                .direction(dir),
            content,
        );
    }
}

impl Default for Column {
    fn default() -> Self { Self::new() }
}
