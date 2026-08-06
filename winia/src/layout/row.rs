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
    fn test_row_rtl_fill_container() {
        // RTL + fill_max_width：容器 100 宽，子镜像到右侧
        let row = RowLayout::new().direction(LayoutDirection::Rtl);
        let mut nodes = vec![make_leaf(20.0, 50.0), make_leaf(30.0, 50.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (_, placements) = row.measure(&mut nodes, &[], &children, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(placements[0].position.x, 80.0, "第一个子在最右");
        assert_eq!(placements[1].position.x, 50.0);
    }
}
