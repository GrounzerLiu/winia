//! Row 布局 — 水平排列子节点（对齐 Compose Row）
//!
//! 主轴 = 水平（宽度），交叉轴 = 垂直（高度）
//!
//! 特性:
//! - per-child 对齐: `Modifier::align_self(Alignment::Center)` 覆盖 Row 默认对齐
//! - weight 权重: `Modifier::weight(2.0)` 按比例分配剩余宽度
//! - spacing: 子节点间固定间距
//!
//! 实现委托到 `flex::measure_flex::<HorizontalAxis>()`。

use super::constraints::Constraints;
use super::flex;
use super::node::*;

/// Row 布局策略
#[derive(Debug, Clone)]
pub struct RowLayout {
    pub arrangement: Arrangement,
    pub alignment: Alignment,
    pub spacing: f32,
    pub direction: LayoutDirection,
}

impl RowLayout {
    pub fn new() -> Self {
        RowLayout {
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

impl Default for RowLayout {
    fn default() -> Self { Self::new() }
}

impl MeasurePolicy for RowLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        flex::measure_flex::<flex::HorizontalAxis>(
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
    // Row is the horizontal half of Compose's `IntrinsicMeasureBlocks` (RowColumnImpl.kt:261-369):
    // a WIDTH query is the main axis and prices weighted children by weight unit, a HEIGHT query is
    // the cross axis and first resolves how much main-axis room each child gets.

    /// `HorizontalMinWidth`: `intrinsicMainAxisSize(measurables, minIntrinsicWidth(h), availableHeight)`
    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        flex::flex_intrinsic_main(ctx, children, IntrinsicQuery::MinWidth, height, self.spacing)
    }

    /// `HorizontalMaxWidth`
    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        flex::flex_intrinsic_main(ctx, children, IntrinsicQuery::MaxWidth, height, self.spacing)
    }

    /// `HorizontalMinHeight`: `intrinsicCrossAxisSize(measurables, maxIntrinsicWidth, minIntrinsicHeight, availableWidth)`
    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        flex::flex_intrinsic_cross(
            ctx,
            children,
            IntrinsicQuery::MaxWidth,
            IntrinsicQuery::MinHeight,
            width,
            self.spacing,
        )
    }

    /// `HorizontalMaxHeight`
    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        flex::flex_intrinsic_cross(
            ctx,
            children,
            IntrinsicQuery::MaxWidth,
            IntrinsicQuery::MaxHeight,
            width,
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

    #[test]
    fn test_row_simple() {
        let row = RowLayout::new();
        let mut nodes = vec![make_leaf(10.0, 100.0), make_leaf(30.0, 80.0), make_leaf(20.0, 120.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 60.0); // 10+30+20
        assert_eq!(placements[0].position.x, 0.0);
        assert_eq!(placements[1].position.x, 10.0);
        assert_eq!(placements[2].position.x, 40.0);
    }

    #[test]
    fn test_row_spacing() {
        let row = RowLayout::new().spacing(5.0);
        let mut nodes = vec![make_leaf(20.0, 100.0), make_leaf(30.0, 80.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 55.0); // 20+5+30
        assert_eq!(placements[1].position.x, 25.0);
    }

    #[test]
    fn test_row_center_alignment() {
        let row = RowLayout::new().alignment(Alignment::Center);
        let mut nodes = vec![make_leaf(50.0, 20.0), make_leaf(50.0, 100.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(placements[0].position.y, (100.0 - 20.0) / 2.0);
        assert_eq!(placements[1].position.y, 0.0);
    }

    #[test]
    fn test_row_rtl_mirror() {
        // RTL：子节点从右到左排列（第一个子在最右）
        let row = RowLayout::new().direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(10.0, 100.0), make_leaf(30.0, 80.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
        assert_eq!(size.width, 40.0);
        // 容器宽 40：第一个子（10 宽）在最右 → x = 30
        assert_eq!(placements[0].position.x, 30.0);
        assert_eq!(placements[1].position.x, 0.0);
    }

    #[test]
    fn test_row_rtl_uses_own_width_not_constraint() {
        // 回归：wrap Row 的 RTL 镜像必须用行自身宽度（此前误用 incoming
        // max——子节点被推到约束宽度，wrap 行内容消失/错位）
        let row = RowLayout::new().direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(20.0, 50.0), make_leaf(30.0, 50.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) = row.measure(&mut nodes, &[], &children, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].position.x, 30.0, "第一个子（20 宽）在最右");
        assert_eq!(placements[1].position.x, 0.0);
    }

    #[test]
    fn test_row_rtl_space_between_mirrors_full_width() {
        // SpaceBetween 分配的是容器多余的主轴空间，而宽度由约束给出（min = max = 100），不是这个排列撑
        // 出来的——这正是 Compose 的行为：`Row(SpaceBetween)` 要靠 `fillMaxWidth()` 才有宽度可分配。
        // 对齐前后的差别见 `flex.rs` 的 `measured_main`。
        let row = RowLayout::new()
            .direction(LayoutDirection::Rtl)
            .arrangement(Arrangement::SpaceBetween);
        let mut nodes = vec![make_leaf(20.0, 50.0), make_leaf(30.0, 50.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = row.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(100.0, 100.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 100.0, "宽度来自约束");
        // LTR 位置：0 / 70（间距 50）；RTL 镜像后：80 / 0
        assert_eq!(placements[0].position.x, 80.0, "第一个子在最右");
        assert_eq!(placements[1].position.x, 0.0, "第二个子在最左");
    }
}
