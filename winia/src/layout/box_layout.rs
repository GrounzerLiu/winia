//! Box 布局 — 层叠子节点（Z 轴堆叠）
//!
//! 所有子节点获得相同的空间，类似 FrameLayout / Box

use super::constraints::Constraints;
use crate::unit::{Offset, Size};
use super::node::*;
use super::node::measure_node;

/// Box 布局策略 — 子节点层叠排列
///
/// 与 Column/Row 不同，Box 的每个子节点获得相同的约束，
/// Box 的尺寸取所有子节点中的最大值。
#[derive(Debug, Clone)]
pub struct BoxLayout {
    /// 子节点在 Box 中的对齐方式（单轴值同时作用于两个轴——见 [`ContentAlignment`]）
    pub alignment: Alignment,
    /// 两个轴各自的对齐（Compose 的二维 `contentAlignment`）。设了就用它，
    /// 否则退回上面的 `alignment`。
    pub content_alignment: Option<ContentAlignment>,
}

impl BoxLayout {
    pub fn new() -> Self {
        BoxLayout {
            alignment: Alignment::Start,
            content_alignment: None,
        }
    }

    /// The same value on both axes — `Start` is Compose's `TopStart`, `End` is `BottomEnd`.
    pub fn alignment(mut self, a: Alignment) -> Self {
        self.alignment = a;
        self
    }

    /// Each axis on its own, Compose's 2-D `contentAlignment` (`TopEnd`, `BottomCenter`, …).
    pub fn content_alignment(mut self, a: ContentAlignment) -> Self {
        self.content_alignment = Some(a);
        self
    }

    /// The alignment in force, whichever of the two was set.
    pub fn effective_alignment(&self) -> ContentAlignment {
        self.content_alignment.unwrap_or_else(|| ContentAlignment::both(self.alignment))
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
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let mut max_width: f32 = 0.0;
        let mut max_height: f32 = 0.0;
        let mut child_sizes: Vec<Size> = Vec::with_capacity(children.len());

        // 测量所有子节点，每个子节点获得相同的约束
        for &c in children {
            let (child_size, _) = measure_node(nodes, policies, c, constraints.loosen());
            max_width = max_width.max(child_size.width);
            max_height = max_height.max(child_size.height);
            child_sizes.push(child_size);
        }

        let width = constraints.constrain_width(max_width);
        let height = constraints.constrain_height(max_height);

        // 为每个子节点计算在 Box 中的位置（根据 alignment）
        let align = self.effective_alignment();
        let space = Size::new(width, height);
        let placements: Vec<Placement> = child_sizes
            .iter()
            .map(|child_size| {
                let (x, y) = align.anchor(*child_size, space);
                let size = align.child_size(*child_size, space);
                Placement {
                    size,
                    position: Offset::new(x, y),
                }
            })
            .collect();

        (Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &c) in children.iter().enumerate() {
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
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
        let mut nodes = vec![
            make_leaf(50.0, 30.0),
            make_leaf(100.0, 20.0),
            make_leaf(30.0, 80.0),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();

        let (size, placements) = box_layout.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::UNBOUNDED,
        );

        // Box 取最大宽度 100，最大高度 80
        assert_eq!(size, Size::new(100.0, 80.0));
        assert_eq!(placements.len(), 3);
    }

    #[test]
    fn test_box_center_alignment() {
        let box_layout = BoxLayout::new().alignment(Alignment::Center);
        let mut nodes = vec![
            make_leaf(50.0, 30.0),
            make_leaf(100.0, 80.0),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();

        let (size, placements) = box_layout.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::UNBOUNDED,
        );

        // max w=100, h=80 → 居中子节点
        assert_eq!(size, Size::new(100.0, 80.0));
        // 第一个 (50,30) 居中: x=(100-50)/2=25, y=(80-30)/2=25
        assert_eq!(placements[0].position, Offset::new(25.0, 25.0));
        // 第二个 (100,80) 居中: x=(100-100)/2=0, y=0
        assert_eq!(placements[1].position, Offset::new(0.0, 0.0));
    }

    /// The mixed corners the one-axis [`Alignment`] cannot express: each axis on its own, Compose's
    /// `Box(contentAlignment = Alignment.TopEnd)` and `BottomStart`.
    #[test]
    fn a_two_axis_alignment_puts_a_child_in_the_mixed_corners() {
        for (alignment, expected) in [
            // (100, 80) box, (50, 30) child: right edge, top.
            (ContentAlignment::TOP_END, Offset::new(50.0, 0.0)),
            // left edge, bottom.
            (ContentAlignment::BOTTOM_START, Offset::new(0.0, 50.0)),
            (ContentAlignment::TOP_CENTER, Offset::new(25.0, 0.0)),
        ] {
            let box_layout = BoxLayout::new().content_alignment(alignment);
            let mut nodes = vec![make_leaf(50.0, 30.0), make_leaf(100.0, 80.0)];
            let children: Vec<usize> = (0..nodes.len()).collect();
            let (_, placements) = box_layout.measure(
                &mut nodes,
                &[],
                &children,
                Constraints::UNBOUNDED,
            );
            assert_eq!(
                placements[0].position, expected,
                "{alignment:?} should place the 50x30 child at {expected:?}"
            );
        }
    }
}
