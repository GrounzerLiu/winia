//! Column 布局 — 委托到 flex::VerticalAxis
//!
//! 主轴 = 垂直（高度），交叉轴 = 水平（宽度）
//!
//! 特性:
//! - per-child 对齐: `Modifier::align_self(Alignment::Center)` 覆盖 Column 默认对齐
//! - weight 权重: `Modifier::weight(2.0)` 按比例分配剩余高度
//! - spacing: 子节点间固定间距
//!
//! 实现委托到 `flex::measure_flex::<VerticalAxis>()`。

use super::constraints::Constraints;
use super::flex;
use super::node::*;

/// Column 布局策略
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
        flex::measure_flex::<flex::VerticalAxis>(
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

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The `fill = false` collapse does NOT survive a spreading arrangement, and that is worth
    /// pinning where someone changing `SpaceBetween` will see it.
    ///
    /// winia's `SpaceBetween` (and `SpaceAround`/`SpaceEvenly`) stretches a container to the main
    /// axis its parent offers — a deliberate deviation from Compose, whose `SpaceBetween` only
    /// distributes leftover space and never grows the container to its maximum. So the space a
    /// non-filling child saves is spent on the gap before its next sibling instead of shortening the
    /// column: the same two children as the test above come out the full 500 tall, with the sibling
    /// pushed to the bottom. material3's date picker dialog is the worked example — it needs both
    /// the `weight(1f, fill = false)` box AND `Arrangement::Start` here, where the source says
    /// `SpaceBetween` (`DatePickerDialog.android.kt:89-95`).
    #[test]
    fn a_non_filling_weight_does_not_shrink_a_space_between_container() {
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
            size.height, 500.0,
            "a spreading arrangement still takes the parent's whole main axis"
        );
        assert_eq!(
            placements[1].position.y, 470.0,
            "the saved space became the gap, not a shorter column"
        );
    }

    /// Records where "the share is a MAXIMUM" stops being true, so that closing the gap cannot pass
    /// unnoticed. This is NOT the behaviour to want: Compose clamps the child to its share
    /// (`constraints.constrain(targetConstraints)` under `enforceIncoming = true`), while winia's
    /// `size`/`height` raise min and max together and override the incoming maximum before flex runs.
    ///
    /// Measured before writing this: the column reports its own 500 dp bound, the child takes 600 dp
    /// anyway, and the sibling is pushed to y = 600 — out of the column it belongs to. A `fill = true`
    /// child asking for the same 600 dp is placed in its 470 dp share instead, which is why the flag
    /// is what decides this.
    #[test]
    fn an_oversized_non_filling_weight_overflows_its_share() {
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
        assert_eq!(size.height, 500.0, "the column still reports its own bound");
        assert_eq!(
            placements[0].size.height, 600.0,
            "the child kept its own size rather than being clamped to the 470 dp share"
        );
        assert_eq!(
            placements[1].position.y, 600.0,
            "and the sibling fell outside the column"
        );
    }

    #[test]
    fn test_column_simple() {
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
    fn test_column_cross_axis_loose_in_tight_parent() {
        // tight 交叉轴父（如 fill_max_size+padding 的 Column）：子节点交叉轴
        // 松约束（min=0）——自然宽，不继承父 tight（Compose Column 语义）
        let mut nodes = vec![make_leaf(100.0, 20.0)];
        let children: Vec<usize> = vec![0];
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes, &[], &children,
            Constraints::new(432.0, 432.0, 0.0, 1000.0),
        );
        assert_eq!(placements[0].size.width, 100.0, "子节点自然宽（不撑满 tight 父）");
        assert_eq!(size.width, 432.0, "Column 自身受 tight 父约束（432），松的只是子节点");
    }

    #[test]
    fn test_column_child_fill_max_width_in_tight_parent() {
        // tight 父下 fill_max_width 子节点仍撑满（显式 fill 优先于松约束）
        use crate::modifier::Modifier;
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().fill_max_width().height(20.0)),
        ];
        let children: Vec<usize> = vec![0];
        let (size, placements) = ColumnLayout::new().measure(
            &mut nodes, &[], &children,
            Constraints::new(432.0, 432.0, 0.0, 1000.0),
        );
        assert_eq!(placements[0].size.width, 432.0, "fill_max_width 撑满");
        assert_eq!(size.width, 432.0);
    }

    #[test]
    fn test_column_stretch_child_in_tight_parent() {
        // Stretch 对齐：子节点拉伸到容器宽（放置期拉伸——stretch 语义不受松约束影响）
        let mut nodes = vec![make_leaf(100.0, 20.0)];
        let children: Vec<usize> = vec![0];
        let (size, placements) = ColumnLayout::new().alignment(Alignment::Stretch).measure(
            &mut nodes, &[], &children,
            Constraints::new(432.0, 432.0, 0.0, 1000.0),
        );
        assert_eq!(placements[0].size.width, 432.0, "Stretch 子节点撑满容器宽");
        assert_eq!(size.width, 432.0);
    }

    #[test]
    fn test_column_spacing() {
        let column = ColumnLayout::new().spacing(5.0);
        let mut nodes = vec![make_leaf(100.0, 20.0), make_leaf(80.0, 30.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = column.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.height, 55.0); // 20+5+30
        assert_eq!(placements[1].position.y, 25.0);
    }

    #[test]
    fn test_column_end_alignment() {
        let column = ColumnLayout::new().alignment(Alignment::End);
        let mut nodes = vec![make_leaf(50.0, 20.0), make_leaf(100.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_size, placements) = column.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(placements[0].position.x, 100.0 - 50.0);
        assert_eq!(placements[1].position.x, 0.0);
    }
}

#[cfg(test)]
mod zz_probe2 {
    use super::*;
    #[test]
    fn zz_probe_oversized_weighted_child() {
        use crate::modifier::Modifier;
        // The share is 500-30 = 470; the child declares 600.
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().layout_weight_fill(1.0, false).size(100.0, 600.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 30.0)),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = ColumnLayout::new()
            .measure(&mut nodes, &[], &children, Constraints::new(0.0, 100.0, 0.0, 500.0));
        println!("ZZ2 fill=false size(600): container {}, child {} at y={}, sibling y={}",
                 size.height, placements[0].size.height, placements[0].position.y, placements[1].position.y);

        let mut nodes2 = vec![
            LayoutNode::leaf(Modifier::new().layout_weight(1.0).size(100.0, 600.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 30.0)),
        ];
        let children2: Vec<usize> = (0..nodes2.len()).collect();
        let (size2, p2) = ColumnLayout::new()
            .measure(&mut nodes2, &[], &children2, Constraints::new(0.0, 100.0, 0.0, 500.0));
        println!("ZZ2 fill=true  size(600): container {}, child {} at y={}, sibling y={}",
                 size2.height, p2[0].size.height, p2[0].position.y, p2[1].position.y);
    }
}
