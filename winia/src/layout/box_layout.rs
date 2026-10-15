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
    /// The layout direction the horizontal half of the alignment mirrors under — Compose resolves
    /// `contentAlignment` with the measure scope's `layoutDirection`, so `Start` is the LEFT edge in
    /// LTR and the RIGHT one in RTL (`Alignment.kt:114-121`). Captured at composition, like
    /// `Row`/`Column` do (`layout/components.rs:61`).
    pub direction: LayoutDirection,
    /// Pass this box's own MINIMUM constraints down to its children instead of loosening them —
    /// Compose's `Box(propagateMinConstraints = true)`. Off by default, as there.
    ///
    /// It matters where something FORCES a size on the box: without it the box reports the forced
    /// size while its child still hugs its own content, so the box grows and the thing that paints
    /// does not. `Surface` sets it (`material3/Surface.kt:219`), which is why a Compose button
    /// widens with its box; winia's Button wraps its visual box in a min-interactive container and
    /// that container needs the same.
    pub propagate_min_constraints: bool,
}

impl BoxLayout {
    pub fn new() -> Self {
        BoxLayout {
            alignment: Alignment::Start,
            content_alignment: None,
            direction: LayoutDirection::Ltr,
            propagate_min_constraints: false,
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

    pub fn direction(mut self, d: LayoutDirection) -> Self {
        self.direction = d;
        self
    }

    /// See [`BoxLayout::propagate_min_constraints`].
    pub fn propagate_min_constraints(mut self, on: bool) -> Self {
        self.propagate_min_constraints = on;
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

        // 测量所有子节点，每个子节点获得相同的约束——除非开了 propagate_min_constraints，
        // 这时把本盒子的 MIN 约束一并传下去（Compose 的 `Box(propagateMinConstraints = true)`）
        let child_constraints = if self.propagate_min_constraints {
            constraints
        } else {
            constraints.loosen()
        };
        for &c in children {
            let (child_size, _) = measure_node(nodes, policies, c, child_constraints);
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
                let (x, y) = align.anchor(*child_size, space, self.direction);
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

    /// The horizontal half mirrors under RTL, as Compose's `Alignment` does: `Start` is the left
    /// edge in LTR and the RIGHT one in RTL (`Alignment.kt:114-121`), and the vertical half never
    /// mirrors.
    #[test]
    fn a_sided_alignment_mirrors_under_rtl() {
        for (direction, name, expected_x) in [
            (LayoutDirection::Ltr, "Ltr", 50.0),
            (LayoutDirection::Rtl, "Rtl", 0.0),
        ] {
            let box_layout = BoxLayout::new()
                .content_alignment(ContentAlignment::TOP_END)
                .direction(direction);
            let mut nodes = vec![make_leaf(50.0, 30.0), make_leaf(100.0, 80.0)];
            let children: Vec<usize> = (0..nodes.len()).collect();
            let (_, placements) =
                box_layout.measure(&mut nodes, &[], &children, Constraints::UNBOUNDED);
            // TopEnd on a 100x80 box with a 50x30 child: right edge in LTR, left edge in RTL.
            assert_eq!(
                placements[0].position,
                Offset::new(expected_x, 0.0),
                "TopEnd in {name} should put the child at x={expected_x}"
            );
        }
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

    /// `propagate_min_constraints` is Compose's `Box(propagateMinConstraints = true)`: the box's
    /// MINIMUM goes to the children instead of being loosened away.
    ///
    /// The case that needs it: a parent FORCES a size on the box. Without propagation the box
    /// reports the forced size while its child still hugs its content — the box grows and whatever
    /// paints inside it does not, which is exactly what a ButtonGroup item's press growth looked
    /// like before `Button`'s touch-target container set this (the visual box stayed its content
    /// width in a box that had grown, and the difference showed as a gap).
    #[test]
    fn propagate_min_constraints_passes_the_minimum_to_the_children() {
        let forced = Constraints::new(100.0, 100.0, 0.0, 100.0);
        let leaf = |nodes: &mut Vec<LayoutNode>| {
            nodes.clear();
            nodes.push(make_leaf(20.0, 10.0) as LayoutNode);
            vec![0usize]
        };

        let mut nodes = Vec::new();
        let children = leaf(&mut nodes);
        let loose = BoxLayout::new();
        let (_, placements) = loose.measure(&mut nodes, &[], &children, forced);
        assert_eq!(
            placements[0].size.width, 20.0,
            "by default the child keeps its own width"
        );

        let mut nodes = Vec::new();
        let children = leaf(&mut nodes);
        let propagating = BoxLayout::new().propagate_min_constraints(true);
        let (_, placements) = propagating.measure(&mut nodes, &[], &children, forced);
        assert_eq!(
            placements[0].size.width, 100.0,
            "with propagation the forced minimum reaches the child, so the child paints that wide"
        );
    }
}
