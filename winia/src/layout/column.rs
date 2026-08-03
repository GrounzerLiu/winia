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
