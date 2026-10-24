//! Box 布局 — 层叠子节点（Z 轴堆叠）
//!
//! 所有子节点获得相同的空间，类似 FrameLayout / Box

use super::constraints::Constraints;
use crate::unit::{Offset, Size};
use super::node::*;
use super::node::measure_node;
use super::container::build_container;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;

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
            // Compose resolves placement with the MEASURE SCOPE's direction on every Box
            // (`Box.kt:157`, `:199`, `:213-220`), not only on the builders that remember to set
            // it. Defaulting here covers every hand-built policy (Surface, Card, IconButton, …),
            // whose `new()` runs during composition where the local is valid.
            direction: crate::layout::direction::current(),
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

        // Compose's Box reports `max(child, constraints.min)` and never consults the maximum
        // (`Box.kt:146-147`, `:164-171`). Coercing into the max, as this did, cropped a child that
        // ignores its constraints (`required_size`) and told the parent the box fits when it does
        // not; `Row`/`Column` were aligned with Compose in the same way.
        let width = max_width.max(constraints.min_width);
        let height = max_height.max(constraints.min_height);

        // 为每个子节点计算在 Box 中的位置（根据 alignment）
        //
        // A child may carry its own alignment — Compose's `BoxScope.align` (`Box.kt:205-221`: the
        // child's own alignment wins, otherwise the box's). The two-axis value wins over the
        // axis-symmetric `align_self`, which wins over the box's `contentAlignment`.
        let align = self.effective_alignment();
        let space = Size::new(width, height);
        let placements: Vec<Placement> = child_sizes
            .iter()
            .enumerate()
            .map(|(i, child_size)| {
                // Precedence: the child's own two-axis value, then its axis-symmetric one, then the
                // box's alignment. `Stretch` is skipped in the one-axis fallback: it means "fill the
                // CROSS axis" in a Row/Column, and a Box would read it as fill-both-axes (which it
                // cannot honour without resizing the child, something Compose's `BoxScope.align`
                // never does).
                let child_align = nodes[children[i]]
                    .modifier
                    .get_content_align_self()
                    .or_else(|| {
                        nodes[children[i]]
                            .modifier
                            .get_align_self()
                            .filter(|a| *a != Alignment::Stretch)
                            .map(ContentAlignment::both)
                    })
                    .unwrap_or(align);
                let (x, y) = child_align.anchor(*child_size, space, self.direction);
                let size = child_align.child_size(*child_size, space);
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
    fn a_box_takes_the_size_of_its_largest_child() {
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
    fn center_alignment_centres_every_child() {
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

    /// Compose's `BoxScope.align`: a child's own alignment wins over the box's `contentAlignment`
    /// (`Box.kt:205-221`). Both carriers are read — the two-axis one first, then the axis-symmetric
    /// `align_self`, which a Box applies to BOTH axes.
    #[test]
    fn a_child_alignment_wins_over_the_boxes_own_and_covers_both_carriers() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0).content_align_self(ContentAlignment::BOTTOM_END)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0).align_self(Alignment::Center)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0)),
        ];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let (size, placements) = BoxLayout::new()
            .alignment(Alignment::Start)
            .measure(&mut nodes, &[], &children, Constraints::fixed(400.0, 400.0));
        assert_eq!(size, Size::new(400.0, 400.0));
        assert_eq!(
            placements[0].position,
            Offset::new(350.0, 350.0),
            "content_align_self(BOTTOM_END) overrides the box's Start"
        );
        assert_eq!(
            placements[1].position,
            Offset::new(175.0, 175.0),
            "align_self(Center) applies to both axes in a Box"
        );
        assert_eq!(
            placements[2].position,
            Offset::new(0.0, 0.0),
            "a child with neither keeps the box's alignment"
        );
    }

    /// Compose's Box reports `max(child, constraints.min)` and never looks at the maximum
    /// (`Box.kt:146-147`, `:164-171`): a child that ignores its constraints overflows the box
    /// instead of being cropped to it.
    #[test]
    fn an_overflowing_child_is_reported_not_cropped() {
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().required_size(500.0, 50.0))];
        let children: Vec<usize> = vec![0];
        let (size, _) = BoxLayout::new().measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(0.0, 200.0, 0.0, 200.0),
        );
        assert_eq!(size, Size::new(500.0, 50.0), "the content size, not the 200 maximum");
    }

    /// The box still floors at the incoming minimum — that half of the old `constrain` was right.
    #[test]
    fn a_content_sized_box_is_floored_by_the_minimum() {
        // `required_size`, not `size`: a plain `size` is coerced into the incoming range, so the child
        // would come out at the minimum and the assertion would hold vacuously.
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().required_size(20.0, 10.0))];
        let children: Vec<usize> = vec![0];
        let (size, _) = BoxLayout::new().measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(100.0, 300.0, 50.0, 300.0),
        );
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    /// Compose's empty Box is `layout(constraints.minWidth, constraints.minHeight)` (`Box.kt:128-130`).
    /// `max(0, min)` preserves it, and nothing else pinned it.
    #[test]
    fn an_empty_box_takes_the_minimum() {
        let (size, placements) = BoxLayout::new().measure(
            &mut Vec::new(),
            &[],
            &[],
            Constraints::new(120.0, 400.0, 60.0, 400.0),
        );
        assert_eq!(size, Size::new(120.0, 60.0));
        assert!(placements.is_empty());
    }

    /// The public builder has to carry both new knobs into the policy: `Stack::propagate_min_constraints`
    /// and `content_alignment` are what a caller writes, and a builder that drops one is silent.
    #[test]
    fn the_stack_builder_carries_propagation_and_alignment() {
        use crate::runtime::composer::Composer;
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            Stack::new()
                .content_alignment(ContentAlignment::BOTTOM_END)
                .propagate_min_constraints(true)
                .modifier(Modifier::new().size(200.0, 60.0))
                .build(ctx, |ctx| {
                    crate::components::Text::new("x").build(ctx);
                });
        }));
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root().unwrap();
        let child = composer.arena_nodes()[root.children[0]].measured_size;
        assert_eq!(
            (child.width, child.height),
            (200.0, 60.0),
            "propagation passed the box's minimum down"
        );
    }
}
pub struct Stack {
    modifier: Modifier,
    alignment: Alignment,
    content_alignment: Option<ContentAlignment>,
    /// Compose's `Box(propagateMinConstraints = true)` — the box's own minimum is passed to its
    /// children instead of being loosened away. See `BoxLayout::propagate_min_constraints`.
    propagate_min_constraints: bool,
}

impl Stack {
    pub fn new() -> Self {
        Stack {
            modifier: Modifier::new(),
            alignment: Alignment::Start,
            content_alignment: None,
            propagate_min_constraints: false,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn alignment(mut self, a: Alignment) -> Self { self.alignment = a; self }

    /// Each axis on its own — Compose's 2-D `Box(contentAlignment = …)`.
    pub fn content_alignment(mut self, a: ContentAlignment) -> Self {
        self.content_alignment = Some(a);
        self
    }

    /// Compose's `Box(propagateMinConstraints = true)`.
    pub fn propagate_min_constraints(mut self, on: bool) -> Self {
        self.propagate_min_constraints = on;
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // 参数暂存（参数相等跳过——同 Column）
        ctx.changed(&self.alignment);
        ctx.changed(&self.content_alignment);
        ctx.changed(&self.propagate_min_constraints);
        // 方向：modifier 覆盖 > CompositionLocal 默认（同 Column/Row）——对齐的水平轴靠它镜像
        let dir = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        // content 闭包自动成为组合 scope（与 Column 一致）
        let mut policy = match self.content_alignment {
            Some(a) => BoxLayout::new().content_alignment(a).direction(dir),
            None => BoxLayout::new().alignment(self.alignment).direction(dir),
        };
        policy = policy.propagate_min_constraints(self.propagate_min_constraints);
        build_container(ctx, self.modifier, policy, content);
    }
}

impl Default for Stack {
    fn default() -> Self { Self::new() }
}