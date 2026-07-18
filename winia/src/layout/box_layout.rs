//! Box 布局 — 层叠子节点（Z 轴堆叠）
//!
//! 所有子节点获得相同的空间，类似 FrameLayout / Box

use super::constraints::Constraints;
use super::node::*;
use super::column::measure_node;

/// Box 布局策略 — 子节点层叠排列
///
/// 与 Column/Row 不同，Box 的每个子节点获得相同的约束，
/// Box 的尺寸取所有子节点中的最大值。
#[derive(Debug, Clone)]
pub struct BoxLayout {
    /// 子节点在 Box 中的对齐方式
    pub alignment: Alignment,
}

impl BoxLayout {
    pub fn new() -> Self {
        BoxLayout {
            alignment: Alignment::Start,
        }
    }

    pub fn alignment(mut self, a: Alignment) -> Self {
        self.alignment = a;
        self
    }
}

impl Default for BoxLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurePolicy for BoxLayout {
    fn measure(
        &self,
        children: &mut [LayoutNode],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let mut max_width: f32 = 0.0;
        let mut max_height: f32 = 0.0;
        let mut child_sizes: Vec<Size> = Vec::with_capacity(children.len());

        // 测量所有子节点，每个子节点获得相同的约束
        for child in children.iter_mut() {
            let (child_size, _) = measure_node(child, constraints.loosen());
            max_width = max_width.max(child_size.width);
            max_height = max_height.max(child_size.height);
            child_sizes.push(child_size);
        }

        let width = constraints.constrain_width(max_width);
        let height = constraints.constrain_height(max_height);

        // 为每个子节点计算在 Box 中的位置（根据 alignment）
        let placements: Vec<Placement> = child_sizes
            .iter()
            .map(|child_size| {
                let x = match self.alignment {
                    Alignment::Start => 0.0,
                    Alignment::End => width - child_size.width,
                    Alignment::Center => (width - child_size.width) / 2.0,
                    Alignment::Stretch => 0.0,
                };
                let y = match self.alignment {
                    Alignment::Start => 0.0,
                    Alignment::End => height - child_size.height,
                    Alignment::Center => (height - child_size.height) / 2.0,
                    Alignment::Stretch => 0.0,
                };
                let w = if self.alignment == Alignment::Stretch {
                    width
                } else {
                    child_size.width
                };
                let h = if self.alignment == Alignment::Stretch {
                    height
                } else {
                    child_size.height
                };
                Placement {
                    size: Size::new(w, h),
                    position: Point::new(x, y),
                }
            })
            .collect();

        (Size::new(width, height), placements)
    }

    fn place(&self, children: &mut [LayoutNode], placements: &[Placement]) {
        for (child, placement) in children.iter_mut().zip(placements.iter()) {
            child.position = placement.position;
            child.measured_size = placement.size;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_leaf(w: f32, h: f32) -> LayoutNode {
        use crate::modifier::Modifier;
        let mut node = LayoutNode::leaf(
            Modifier::new().size(w, h),
        );
        node.measured_size = Size::new(w, h);
        node
    }

    #[test]
    fn test_box_max_size() {
        let box_layout = BoxLayout::new();
        let mut children = vec![
            make_leaf(50.0, 30.0),
            make_leaf(100.0, 20.0),
            make_leaf(30.0, 80.0),
        ];

        let (size, placements) = box_layout.measure(
            &mut children,
            Constraints::UNBOUNDED,
        );

        // Box 取最大宽度 100，最大高度 80
        assert_eq!(size, Size::new(100.0, 80.0));
        assert_eq!(placements.len(), 3);
    }

    #[test]
    fn test_box_center_alignment() {
        let box_layout = BoxLayout::new().alignment(Alignment::Center);
        let mut children = vec![
            make_leaf(50.0, 30.0),
            make_leaf(100.0, 80.0),
        ];

        let (size, placements) = box_layout.measure(
            &mut children,
            Constraints::UNBOUNDED,
        );

        // max w=100, h=80 → 居中子节点
        assert_eq!(size, Size::new(100.0, 80.0));
        // 第一个 (50,30) 居中: x=(100-50)/2=25, y=(80-30)/2=25
        assert_eq!(placements[0].position, Point::new(25.0, 25.0));
        // 第二个 (100,80) 居中: x=(100-100)/2=0, y=0
        assert_eq!(placements[1].position, Point::new(0.0, 0.0));
    }
}
