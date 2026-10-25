    use super::*;
    use crate::layout::column::ColumnLayout;
    use crate::layout::row::RowLayout;
    use crate::modifier::{Modifier, ModifierElement};
    use crate::layout::{IntrinsicSize};

    fn text_modifier(content: &str) -> Modifier {
        Modifier::new().push(ModifierElement::TextContent {
            content: content.to_string(),
            font_size: 14.0,
            color: crate::graphics::Color::from_argb(255, 0, 0, 0),
            font_weight: crate::text::FontWeight::NORMAL,
            font_style: crate::text::FontSlant::Upright,
            max_lines: usize::MAX,
            align: crate::text::TextAlign::Left,
            overflow: crate::text::TextOverflow::Clip,
            soft_wrap: true,
            letter_spacing: 0.0,
            line_height: None,
        })
    }

    fn text_leaf(content: &str) -> LayoutNode {
        LayoutNode::new(text_modifier(content), None)
    }

    fn leaf_max_width(nodes: &mut Vec<LayoutNode>, idx: usize, other: f32) -> f32 {
        let policies: Vec<Box<dyn MeasurePolicy>> = Vec::new();
        intrinsic_size_of(nodes, &policies, idx, IntrinsicQuery::MaxWidth, other)
    }

    /// A match-parent child does not contribute to the box's intrinsic size — it does not size the
    /// box at all. The default intrinsic path runs the same two-pass measure, so the child must be
    /// excluded from the answer there too.
    #[test]
    fn a_box_intrinsic_ignores_a_match_parent_child() {
        use crate::layout::BoxLayout;
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::new(Modifier::new(), Some(1)),
            LayoutNode::new(Modifier::new().match_parent_size(), Some(2)),
        ];
        nodes[0].children = vec![1, 2];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![
            Box::new(BoxLayout::new()),
            Box::new(FixedPolicy(120.0, 40.0)),
            Box::new(FixedPolicy(300.0, 200.0)),
        ];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 100.0),
            120.0,
            "the match-parent child must not widen the box"
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 100.0),
            120.0
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 100.0),
            40.0
        );
    }

    /// A policy that only implements `measure` — the default intrinsic methods must reuse it
    /// (Compose: "make a best effort attempt … by reusing the measure method").
    #[derive(Debug)]
    struct FixedPolicy(f32, f32);

    impl MeasurePolicy for FixedPolicy {
        fn measure(
            &self,
            _nodes: &mut Vec<LayoutNode>,
            _policies: &[Box<dyn MeasurePolicy>],
            _children: &[usize],
            _constraints: Constraints,
        ) -> (Size, Vec<Placement>) {
            (Size::new(self.0, self.1), Vec::new())
        }

        fn place(&self, _nodes: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
    }

    /// A policy with the trait's DEFAULT intrinsics — only `measure` and `place`, like `BoxLayout`.
    /// Its `measure` really measures its children, so the children handed to it during an
    /// approximation must be SUBSTITUTED (Compose's `DefaultIntrinsicMeasurable`).
    #[derive(Debug)]
    struct DefaultBox;

    impl MeasurePolicy for DefaultBox {
        fn measure(
            &self,
            nodes: &mut Vec<LayoutNode>,
            policies: &[Box<dyn MeasurePolicy>],
            children: &[usize],
            constraints: Constraints,
        ) -> (Size, Vec<Placement>) {
            let mut size = Size::new(0.0, 0.0);
            for &c in children {
                let (child, _) = measure_node(nodes, policies, c, constraints.loosen());
                size = Size::new(size.width.max(child.width), size.height.max(child.height));
            }
            (size, Vec::new())
        }

        fn place(&self, _nodes: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
    }

    /// A child that COUNTS its real measurements and answers all four intrinsics explicitly:
    /// 20 wide at its minimum, 80 at its maximum, height = the width it is asked about.
    #[derive(Debug)]
    struct CountingLeaf {
        real_measures: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl MeasurePolicy for CountingLeaf {
        fn measure(
            &self,
            _nodes: &mut Vec<LayoutNode>,
            _policies: &[Box<dyn MeasurePolicy>],
            _children: &[usize],
            constraints: Constraints,
        ) -> (Size, Vec<Placement>) {
            self.real_measures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            (
                Size::new(constraints.constrain_width(80.0), constraints.constrain_height(40.0)),
                Vec::new(),
            )
        }

        fn place(&self, _nodes: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}

        fn min_intrinsic_width(&self, _c: &mut IntrinsicCtx<'_>, _n: &[usize], _h: f32) -> f32 {
            20.0
        }
        fn max_intrinsic_width(&self, _c: &mut IntrinsicCtx<'_>, _n: &[usize], _h: f32) -> f32 {
            80.0
        }
        fn min_intrinsic_height(&self, _c: &mut IntrinsicCtx<'_>, _n: &[usize], w: f32) -> f32 {
            w
        }
        fn max_intrinsic_height(&self, _c: &mut IntrinsicCtx<'_>, _n: &[usize], w: f32) -> f32 {
            w
        }
    }

    /// The composed failure: `Column.width(IntrinsicSize::Max) -> Stack -> AnimatedSize` used to
    /// reach the animated container's real `measure` (through `BoxLayout`'s default intrinsics) and
    /// start an animation. The substitution is what makes an intrinsic query above a plain container
    /// safe: the child is ASKED, never measured.
    #[test]
    fn a_defaulted_container_asks_its_child_instead_of_measuring_it() {
        let measures = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::new(Modifier::new(), Some(1)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![
            Box::new(DefaultBox),
            Box::new(CountingLeaf { real_measures: measures.clone() }),
        ];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 100.0),
            80.0,
            "the defaulted container answers with its child's max intrinsic width"
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 100.0),
            20.0,
            "…and with the child's minimum for the other width query"
        );
        assert_eq!(
            measures.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "no descendant may be really measured inside an intrinsic probe"
        );
        assert!(
            nodes[1].cached_constraints.is_none(),
            "and no probe constraint may be written into the child's cache"
        );

        // The frame's own pass still measures both, so the substitution is a probe-only path.
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(
            measures.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "a real layout measures the child exactly once"
        );
    }

    /// Compose's padding node offsets the constraints before the child's intrinsic is asked, so the
    /// space the content is told about is the INNER one — on both axes.
    #[test]
    fn padding_reduces_the_other_axis_before_the_content_is_asked() {
        let measures = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().padding_horizontal(10.0), Some(0)),
            LayoutNode::new(Modifier::new(), Some(1)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![
            Box::new(DefaultBox),
            Box::new(CountingLeaf { real_measures: measures }),
        ];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 100.0),
            80.0,
            "a height query must ask the content at the width the padding leaves it (100 - 20)"
        );
    }

    /// `width/height(IntrinsicSize)` declares an EXTREME, and Compose answers both of that axis's
    /// intrinsic queries with it (`foundation/layout/Intrinsic.kt:192-204`).
    #[test]
    fn an_intrinsic_width_modifier_answers_both_extremes_with_its_declaration() {
        let measures = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let leaf = || {
            let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(CountingLeaf {
                real_measures: measures.clone(),
            })];
            policies
        };
        let mut max_nodes = vec![LayoutNode::new(
            Modifier::new().width(IntrinsicSize::Max),
            Some(0),
        )];
        let max_policies = leaf();
        assert_eq!(
            intrinsic_size_of(&mut max_nodes, &max_policies, 0, IntrinsicQuery::MinWidth, 100.0),
            80.0,
            "width(Max) answers a MinWidth query with the content's maximum"
        );
        assert_eq!(
            intrinsic_size_of(&mut max_nodes, &max_policies, 0, IntrinsicQuery::MaxWidth, 100.0),
            80.0
        );

        let mut min_nodes = vec![LayoutNode::new(
            Modifier::new().width(IntrinsicSize::Min),
            Some(0),
        )];
        let min_policies = leaf();
        assert_eq!(
            intrinsic_size_of(&mut min_nodes, &min_policies, 0, IntrinsicQuery::MaxWidth, 100.0),
            20.0,
            "width(Min) answers a MaxWidth query with the content's minimum"
        );
        assert_eq!(
            intrinsic_size_of(&mut min_nodes, &min_policies, 0, IntrinsicQuery::MinWidth, 100.0),
            20.0
        );
    }

    #[test]
    fn the_default_intrinsic_query_reuses_measure() {
        let mut nodes = vec![LayoutNode::new(Modifier::new(), Some(0))];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(FixedPolicy(40.0, 25.0))];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 100.0),
            40.0,
            "width query = measure with an unbounded width"
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinWidth, 100.0),
            40.0
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 100.0),
            25.0
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MinHeight, 100.0),
            25.0
        );
    }

    #[test]
    fn a_text_leaf_answers_all_four_queries() {
        let mut nodes = vec![text_leaf("New file")];
        let max_w = leaf_max_width(&mut nodes, 0, f32::MAX);
        let min_w = intrinsic_size_of(&mut nodes, &[], 0, IntrinsicQuery::MinWidth, f32::MAX);
        assert!(max_w > 0.0, "the paragraph has a width (measured {max_w})");
        assert!(
            min_w > 0.0 && min_w < max_w,
            "the longest word is narrower than the line: min {min_w} < max {max_w}"
        );

        let height = intrinsic_size_of(&mut nodes, &[], 0, IntrinsicQuery::MaxHeight, 200.0);
        assert!(height > 0.0, "one line tall (measured {height})");
        assert_eq!(
            height,
            intrinsic_size_of(&mut nodes, &[], 0, IntrinsicQuery::MinHeight, 200.0),
            "a single line has the same min and max height"
        );
    }

    /// A parent that shrinks to its child's intrinsic width (`width(IntrinsicSize.Max)`) must leave the text
    /// on ONE line: the intrinsic it reads is the width it then hands over, so an answer narrower than the
    /// laid-out line makes the text wrap inside its own intrinsic width.
    ///
    /// Measured before the fix, on "复制" in `label_large`: the intrinsic came back 28.2 while the line lays
    /// out 29.0 wide, and the `IntrinsicSize::Max` column produced a 28.2-wide, 40-tall text — two stacked
    /// glyphs. That is the dropdown menu item whose label the user saw running vertically.
    #[test]
    fn an_intrinsic_width_parent_leaves_its_text_on_one_line() {
        for content in ["复制", "New file", "带图标", "⧉ 粘贴 Ctrl+V"] {
            // (a) With room to spare: the text's own single-line size.
            let roomy = text_in_a_column(content, Modifier::new().fill_max_width());
            // (b) Shrunk to the intrinsic: the parent's width comes from the same answer it then gives.
            let intrinsic = text_in_a_column(content, Modifier::new().width(IntrinsicSize::Max));
            assert!(
                intrinsic.0 >= roomy.0 - 0.001,
                "{content:?}: an intrinsic-width parent gave {:?} where the line needs {:?}",
                intrinsic,
                roomy
            );
            assert_eq!(
                intrinsic.1, roomy.1,
                "{content:?}: the text must stay on one line (height {:?} against {:?})",
                intrinsic.1, roomy.1
            );
        }
    }

    /// The `(width, height)` a text leaf ends up with inside a column carrying `modifier`.
    fn text_in_a_column(content: &str, modifier: Modifier) -> (f32, f32) {
        let mut composer = crate::runtime::composer::Composer::new();
        let content = content.to_string();
        composer.compose(move |ctx| {
            crate::layout::components::Column::new()
                .modifier(modifier.clone())
                .build(ctx, |ctx| {
                    crate::components::text::Text::new(content.clone()).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("a root");
        let nodes = composer.arena_nodes();
        let leaf = nodes[root].children[0];
        (nodes[leaf].measured_size.width, nodes[leaf].measured_size.height)
    }

    #[test]
    fn padding_is_added_to_a_content_measurement() {
        let bare = leaf_max_width(&mut vec![text_leaf("New file")], 0, f32::MAX);
        let mut padded = vec![LayoutNode::new(text_modifier("New file").padding_horizontal(12.0), None)];
        assert_eq!(
            leaf_max_width(&mut padded, 0, f32::MAX),
            bare + 24.0,
            "the node is as wide as its content plus its own padding"
        );
    }

    #[test]
    fn a_fixed_axis_short_circuits_before_padding() {
        // The pipeline tightens a node to its `size` BEFORE padding is offset into the inner
        // constraints, so the node measures `size` — not `size + padding`.
        let mut nodes = vec![LayoutNode::new(
            Modifier::new().size(100.0, 40.0).padding_horizontal(8.0),
            None,
        )];
        assert_eq!(leaf_max_width(&mut nodes, 0, f32::MAX), 100.0);
        assert_eq!(
            intrinsic_size_of(&mut nodes, &[], 0, IntrinsicQuery::MaxHeight, f32::MAX),
            40.0
        );
    }

    #[test]
    fn an_intrinsic_probe_does_not_write_the_text_render_cache() {
        let mut nodes = vec![text_leaf("New file")];
        let _ = leaf_max_width(&mut nodes, 0, f32::MAX);
        assert!(
            nodes[0].cached_paragraph.borrow().is_none(),
            "a probe runs at the query's width, so its paragraph must not become the render cache"
        );
        let _ = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 500.0, 0.0, 500.0));
        assert!(
            nodes[0].cached_paragraph.borrow().is_some(),
            "the measure pass is what caches the paragraph"
        );
    }

    #[test]
    fn an_intrinsic_probe_answers_through_a_size_range() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width_in(100.0, 300.0), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, f32::MAX),
            100.0,
            "sizeIn lifts the intrinsic answer to the declared minimum (Compose clamps it)"
        );
    }

    #[test]
    fn an_intrinsic_range_resolves_by_chain_order() {
        let mut max_outer = vec![
            LayoutNode::new(
                Modifier::new().max_width(200.0).min_width(300.0),
                Some(0),
            ),
            text_leaf("New file"),
        ];
        max_outer[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        assert_eq!(
            intrinsic_size_of(&mut max_outer, &policies, 0, IntrinsicQuery::MaxWidth, f32::MAX),
            200.0,
            "the outer max wins a conflict, exactly as it does in measure"
        );

        let mut min_outer = vec![
            LayoutNode::new(
                Modifier::new().min_width(300.0).max_width(200.0),
                Some(0),
            ),
            text_leaf("New file"),
        ];
        min_outer[0].children = vec![1];
        assert_eq!(
            intrinsic_size_of(&mut min_outer, &policies, 0, IntrinsicQuery::MaxWidth, f32::MAX),
            300.0,
            "the reverse chain puts the min outside"
        );
    }

    #[test]
    fn default_min_size_applies_to_an_intrinsic_probe() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().default_min_size(112.0, 0.0), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, f32::MAX),
            112.0,
            "UnspecifiedConstraintsNode coerces the intrinsic answer up to its minimum"
        );
    }

    #[test]
    fn a_fill_inside_an_intrinsic_request_does_not_pin_the_axis() {
        let expected =
            intrinsic_size_of(&mut vec![text_leaf("New file")], &[], 0, IntrinsicQuery::MaxWidth, 200.0);
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new().width(IntrinsicSize::Max).fill_max_width(),
                Some(0),
            ),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let (size, _) = measure_node(
            &mut nodes,
            &policies,
            0,
            Constraints::new(0.0, 300.0, 0.0, 200.0),
        );
        assert_eq!(
            size.width, expected,
            "the inner fill fills the intrinsic width; only a fill OUTSIDE the request pins the incoming max"
        );
        assert!(size.width < 300.0);
    }

    #[test]
    fn a_single_axis_wrap_does_not_center_the_other_axis() {
        // wrapContentWidth: the horizontal alignment applies, the vertical one is hard-coded to 0
        // by Compose's alignmentCallback even when the content is taller than the wrapper.
        let (dx, dy) = wrap_alignment_offset(
            ContentAlignment::new(Alignment::Center, Alignment::Center),
            Size::new(40.0, 500.0),
            Size::new(60.0, 300.0),
            LayoutDirection::Ltr,
            true,
            false,
        );
        assert_eq!(dx, 10.0);
        assert_eq!(
            dy, 0.0,
            "wrapContentWidth must leave the vertical offset at 0"
        );

        // wrapContentHeight mirrors it: the vertical alignment applies, the horizontal is 0.
        let (dx, dy) = wrap_alignment_offset(
            ContentAlignment::new(Alignment::Center, Alignment::End),
            Size::new(60.0, 40.0),
            Size::new(100.0, 300.0),
            LayoutDirection::Ltr,
            false,
            true,
        );
        assert_eq!(dx, 0.0, "wrapContentHeight must leave the horizontal offset at 0");
        assert_eq!(dy, 260.0);

        // Both axes included: the usual centring.
        let (dx, dy) = wrap_alignment_offset(
            ContentAlignment::new(Alignment::Center, Alignment::Center),
            Size::new(40.0, 500.0),
            Size::new(60.0, 300.0),
            LayoutDirection::Ltr,
            true,
            true,
        );
        assert_eq!((dx, dy), (10.0, -100.0));
    }

    #[test]
    fn a_negative_size_range_max_is_coerced_to_zero() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width_in(0.0, -1.0), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let (size, _) = measure_node(
            &mut nodes,
            &policies,
            0,
            Constraints::new(0.0, 300.0, 0.0, 200.0),
        );
        assert_eq!(size.width, 0.0, "Compose coerces a negative max to 0 instead of panicking");
    }

    #[test]
    fn an_intrinsic_probe_does_not_write_the_rich_text_render_cache() {
        let modifier = Modifier::new().push(crate::modifier::ModifierElement::RichTextContent {
            content: "New file".to_string(),
            drawables: Vec::new(),
            drawable_ranges: Vec::new(),
            spans: Vec::new(),
        });
        let mut nodes = vec![LayoutNode::leaf(modifier)];
        assert!(nodes[0].has_richtext_content);
        let _ = intrinsic_size_of(&mut nodes, &[], 0, IntrinsicQuery::MaxWidth, 400.0);
        assert!(
            nodes[0].cached_paragraph.borrow().is_none(),
            "the probe must not leave its paragraph in the render cache"
        );
        let _ = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert!(
            nodes[0].cached_paragraph.borrow().is_some(),
            "a real measure is what caches the paragraph"
        );
    }

    /// The trap the menu's old hand-written column had to work around: a label carrying `weight(1f)` has no
    /// width of its own, so a container that just measures it with an unbounded axis gets the CONSTRAINT back
    /// (measured at the time: an icon menu's items jumped from 112 to the 280 maximum). Row/Column price
    /// weighted children by weight unit instead.
    #[test]
    fn a_weighted_label_reports_its_own_width_not_the_constraint() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::new(text_modifier("New file").layout_weight(1.0), None),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let row_width = intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 600.0);
        let label = leaf_max_width(&mut vec![text_leaf("New file")], 0, 600.0);
        assert_eq!(
            row_width, label,
            "the weighted label's weight unit is its own width, so the row is one label wide"
        );
        assert!(row_width < 600.0, "and NOT the constraint it was offered");
    }

    #[test]
    fn a_row_answers_min_and_max_width_from_its_children() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            text_leaf("New"),
            text_leaf("file"),
        ];
        nodes[0].children = vec![1, 2];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let a = leaf_max_width(&mut vec![text_leaf("New")], 0, f32::MAX);
        let b = leaf_max_width(&mut vec![text_leaf("file")], 0, f32::MAX);
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 600.0),
            a + b
        );
    }

    #[test]
    fn a_row_with_spacing_reserves_the_gaps() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            text_leaf("New"),
            text_leaf("file"),
        ];
        nodes[0].children = vec![1, 2];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().spacing(10.0))];
        let a = leaf_max_width(&mut vec![text_leaf("New")], 0, f32::MAX);
        let b = leaf_max_width(&mut vec![text_leaf("file")], 0, f32::MAX);
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 600.0),
            a + b + 10.0
        );
    }

    #[test]
    fn a_column_answers_height_as_the_sum_of_its_children() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            text_leaf("Alpha"),
            text_leaf("Beta"),
        ];
        nodes[0].children = vec![1, 2];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(ColumnLayout::new())];
        let mut a_nodes = vec![text_leaf("Alpha")];
        let a = intrinsic_size_of(&mut a_nodes, &[], 0, IntrinsicQuery::MaxHeight, 200.0);
        let mut b_nodes = vec![text_leaf("Beta")];
        let b = intrinsic_size_of(&mut b_nodes, &[], 0, IntrinsicQuery::MaxHeight, 200.0);
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 200.0),
            a + b
        );
    }

    #[test]
    fn a_column_answers_width_as_its_widest_child() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            text_leaf("Alpha"),
            text_leaf("Beta and more"),
        ];
        nodes[0].children = vec![1, 2];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(ColumnLayout::new())];
        let narrow = leaf_max_width(&mut vec![text_leaf("Alpha")], 0, f32::MAX);
        let wide = leaf_max_width(&mut vec![text_leaf("Beta and more")], 0, f32::MAX);
        assert!(wide > narrow);
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 200.0),
            wide,
            "the column is as wide as its widest child"
        );
    }

    /// `Modifier.width(IntrinsicSize.Max)` — the pipeline's half of the contract: the node is
    /// tightened to the CONTENT's own measurement.
    #[test]
    fn width_intrinsic_size_max_tightens_the_node_to_its_content() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width(IntrinsicSize::Max), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        // `SpaceBetween` stretches to the incoming main axis when the main axis is bounded, so this
        // test can tell "as wide as the parent allows" (500) from "as wide as the content is".
        let policies: Vec<Box<dyn MeasurePolicy>> =
            vec![Box::new(RowLayout::new().arrangement(Arrangement::SpaceBetween))];
        let (size, _) =
            measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 500.0, 0.0, 500.0));
        let label = leaf_max_width(&mut vec![text_leaf("New file")], 0, 500.0);
        assert_eq!(size.width, label, "measured width is the content's own width");
    }

    #[test]
    fn width_intrinsic_size_min_answers_the_longest_word() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width(IntrinsicSize::Min), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let (size, _) =
            measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 500.0, 0.0, 500.0));
        let longest_word = intrinsic_size_of(
            &mut vec![text_leaf("New file")],
            &[],
            0,
            IntrinsicQuery::MinWidth,
            f32::MAX,
        );
        assert_eq!(size.width, longest_word);
        assert!(longest_word < leaf_max_width(&mut vec![text_leaf("New file")], 0, f32::MAX));
    }

    #[test]
    fn height_intrinsic_size_max_tightens_the_node_to_its_content() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().height(IntrinsicSize::Max), Some(0)),
            text_leaf("Alpha"),
            text_leaf("Beta"),
        ];
        nodes[0].children = vec![1, 2];
        // Same trick as the width case: `SpaceBetween` would otherwise stretch the column to the
        // incoming 500, so the assertion below can only hold if the height was tightened first.
        let policies: Vec<Box<dyn MeasurePolicy>> =
            vec![Box::new(ColumnLayout::new().arrangement(Arrangement::SpaceBetween))];
        let (size, _) =
            measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 200.0, 0.0, 500.0));
        let mut a_nodes = vec![text_leaf("Alpha")];
        let a = intrinsic_size_of(&mut a_nodes, &[], 0, IntrinsicQuery::MaxHeight, 200.0);
        let mut b_nodes = vec![text_leaf("Beta")];
        let b = intrinsic_size_of(&mut b_nodes, &[], 0, IntrinsicQuery::MaxHeight, 200.0);
        assert_eq!(size.height, a + b, "the column is exactly two lines tall");
    }

    #[test]
    fn an_incoming_maximum_still_overrides_an_intrinsic_width() {
        // The `size`/`width` forms keep Compose's `enforceIncoming = true`: the parent's maximum
        // gets the last word (Intrinsic.kt:34-49 "may override this value").
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().width(IntrinsicSize::Max), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let (size, _) =
            measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 20.0, 0.0, 500.0));
        assert_eq!(size.width, 20.0, "clamped to the incoming maximum");
    }

    #[test]
    fn required_width_intrinsic_size_ignores_the_incoming_maximum() {
        // `requiredWidth(IntrinsicSize)` is `enforceIncoming = false` (Intrinsic.kt:105/130).
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().required_width(IntrinsicSize::Max), Some(0)),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let (size, _) =
            measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 20.0, 0.0, 500.0));
        let label = leaf_max_width(&mut vec![text_leaf("New file")], 0, 500.0);
        assert_eq!(
            size.width, label,
            "the required form overflows the incoming maximum on purpose"
        );
        assert!(size.width > 20.0);
    }

    #[test]
    fn a_scrolled_axis_keeps_the_given_cross_axis() {
        // Compose's `Scroll` intrinsic block unbounds the axis that SCROLLS only when the query is
        // about the OTHER axis: `minIntrinsicHeight(width) = child.minIntrinsicHeight(width)` for a
        // vertical scroller (`foundation/Scroll.kt:497-515`). Unbounding the query axis instead
        // reports one long line for a wrapped text.
        const LONG: &str = "Alpha Beta Gamma Delta Epsilon Zeta Eta Theta Iota Kappa Lambda Mu";
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new().vertical_scroll(crate::modifier::ScrollState::new()),
                Some(0),
            ),
            text_leaf(LONG),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(ColumnLayout::new())];
        let at_120 = intrinsic_size_of(
            &mut vec![text_leaf(LONG)],
            &[],
            0,
            IntrinsicQuery::MaxHeight,
            120.0,
        );
        let one_line = intrinsic_size_of(
            &mut vec![text_leaf(LONG)],
            &[],
            0,
            IntrinsicQuery::MaxHeight,
            f32::MAX,
        );
        assert!(
            at_120 > one_line,
            "the fixture must actually wrap at 120 ({at_120} vs one line {one_line})"
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 120.0),
            at_120,
            "a vertical scroller answers a height query at the GIVEN width"
        );
    }

    #[test]
    fn a_subcomposing_policy_reports_its_last_measured_size_instead_of_composing() {
        // Compose has no intrinsic scope for a policy that composes during measurement, and a probe
        // runs outside the frame's compose step — so this one must not compose.
        #[derive(Debug)]
        struct SubcomposingPolicy;

        impl MeasurePolicy for SubcomposingPolicy {
            fn measure(
                &self,
                _nodes: &mut Vec<LayoutNode>,
                _policies: &[Box<dyn MeasurePolicy>],
                _children: &[usize],
                _constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                (Size::new(33.0, 44.0), Vec::new())
            }
            fn place(&self, _nodes: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
            fn subcomposes(&self) -> bool {
                true
            }
        }

        let mut nodes = vec![LayoutNode::new(Modifier::new(), Some(0))];
        nodes[0].measured_size = Size::new(11.0, 22.0);
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(SubcomposingPolicy)];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, 100.0),
            11.0,
            "the last measured width, not a fresh composition"
        );
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxHeight, 100.0),
            22.0
        );
    }

    /// The clamp and the padding have to be applied in the pipeline's order: padding sits INSIDE the size
    /// constraint (`Constraints::offset`), so a `sizeIn(112, 280)` item whose content plus 12dp of padding is
    /// 50 wide answers 112 — not 136. Clamping the padded content (`content.clamp(lo, hi) + padding`) reported
    /// the second number; it is what the old hand-written menu column had to do by hand.
    #[test]
    fn the_clamp_applies_inside_the_padding() {
        // A menu item, structurally: the item is the row itself (its `sizeIn` and `padding` are on the same
        // node as the policy), and its label is the weighted child (`weight(1f)`, here a plain child because
        // the arithmetic under test is the chain, not the weight).
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new()
                    .min_width(112.0)
                    .max_width(280.0)
                    .padding_horizontal(12.0),
                Some(0),
            ),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        assert_eq!(
            intrinsic_size_of(&mut nodes, &policies, 0, IntrinsicQuery::MaxWidth, f32::MAX),
            112.0,
            "the minimum width wins over the padded content"
        );
    }

    /// On the same axis, `fillMaxWidth` beats the intrinsic width request. Compose's `IntrinsicWidthNode`
    /// ends in `constraints.constrain(fixedWidth(intrinsic))`, and where the incoming constraints are already
    /// fixed (`ExposedDropdownMenu` forces `minWidth = maxWidth = menuWidth`) that pins the axis back to the
    /// incoming size. winia applies `fillMax` in the pipeline step AFTER the intrinsic one, so the intrinsic
    /// step skips that axis itself — otherwise it would tighten the node to the content and `fillMax` would
    /// then raise the minimum to that narrower maximum.
    #[test]
    fn fill_max_width_overrides_an_intrinsic_width_request() {
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new().fill_max_width().width(IntrinsicSize::Max),
                Some(0),
            ),
            text_leaf("New file"),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let (size, _) = measure_node(
            &mut nodes,
            &policies,
            0,
            Constraints::new(0.0, 300.0, 0.0, 200.0),
        );
        assert_eq!(
            size.width, 300.0,
            "the incoming maximum wins on the filled axis"
        );
    }
    /// A SINGLE-AXIS size must not publish a minimum-interactive content box.
    ///
    /// `Modifier::width()` fills the other axis with `Dimension::Auto`, `fixed_size()` matches any
    /// static pair, and `Dimension::auto.to_logical_px()` answers 0.0 — so reading the pair straight
    /// through published `(w, 0.0)`, and the renderer then built a zero-HEIGHT box: the node painted
    /// nothing at all. Only a fully declared pair is a box to paint.
    #[test]
    fn a_single_axis_size_publishes_no_minimum_interactive_content() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .minimum_interactive_component_size()
                    .width(200.0),
            );
            ctx.end_node();
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("a root");
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.width, 200.0);
        assert_eq!(
            nodes[root].measured_size.height, 48.0,
            "the touch target still floors the axis the caller did not declare"
        );
        assert!(
            nodes[root].minimum_interactive_content.is_none(),
            "a half-declared pair is not a box to paint"
        );
    }

    // ── focus groups, focus properties, on_focus_changed ──

    #[test]
    fn focus_properties_can_focus_false_is_not_focusable() {
        let m = Modifier::new().focusable().focus_properties_can_focus(false);
        assert!(!has_focusable_modifier(&LayoutNode::leaf(m)));

        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::leaf(Modifier::new().focusable()),
            LayoutNode::leaf(Modifier::new().focusable().focus_properties_can_focus(false)),
        ];
        nodes[0].children = vec![1, 2];
        let mut ids = Vec::new();
        collect_focusable_ids(&nodes, 0, &mut ids);
        assert_eq!(ids.len(), 1, "canFocus = false keeps the node out of the focus walk");
    }

    #[test]
    fn a_programmatic_request_to_a_non_focusable_node_keeps_the_old_focus() {
        let good = crate::modifier::FocusRequester::new();
        let bad = crate::modifier::FocusRequester::new();
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::leaf(Modifier::new().focusable().focus_requester(good.clone())),
            LayoutNode::leaf(
                Modifier::new()
                    .focusable()
                    .focus_requester(bad.clone())
                    .focus_properties_can_focus(false),
            ),
        ];
        nodes[0].children = vec![1, 2];
        assert!(focus_by_id(&mut nodes, 0, good.id()));
        assert!(nodes[1].focused);

        assert!(
            !focus_by_id(&mut nodes, 0, bad.id()),
            "a canFocus = false target refuses the request"
        );
        assert!(
            nodes[1].focused,
            "refusing must not clear the focus the user already had"
        );
        assert!(!nodes[2].focused);
    }

    #[test]
    fn a_focus_group_restores_its_last_focused_child() {
        // Focus order: A, B, [group: C, D], E.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::leaf(Modifier::new().focusable()),
            LayoutNode::leaf(Modifier::new().focusable()),
            LayoutNode::new(Modifier::new().focus_group(), Some(1)),
            LayoutNode::leaf(Modifier::new().focusable()),
            LayoutNode::leaf(Modifier::new().focusable()),
            LayoutNode::leaf(Modifier::new().focusable()),
        ];
        nodes[0].children = vec![1, 2, 3, 6];
        nodes[3].children = vec![4, 5];
        for (i, node) in nodes.iter_mut().enumerate() {
            node.id = i as u64;
        }

        // Focus D directly: the group remembers it.
        assert!(set_focus_by_id(&mut nodes, 0, 5));
        assert_eq!(nodes[3].focus_group_memory, Some(5));

        // Tab out to E, wrap to A, B, and enter the group at its first child C - the boundary
        // restores the group's active child D instead.
        assert!(focus_next(&mut nodes, 0));
        assert_eq!(get_focus_id(&nodes, 0), Some(6));
        assert!(focus_next(&mut nodes, 0));
        assert_eq!(get_focus_id(&nodes, 0), Some(1));
        assert!(focus_next(&mut nodes, 0));
        assert_eq!(get_focus_id(&nodes, 0), Some(2));
        assert!(focus_next(&mut nodes, 0));
        assert_eq!(
            get_focus_id(&nodes, 0),
            Some(5),
            "re-entering the group restores its active child"
        );
    }

    #[test]
    fn a_nested_focus_group_restores_the_outer_active_child() {
        // Tree: root { outer G( inner H(A, B), C ), D }. Focus order: A, B, C, D.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),          // 0 root
            LayoutNode::new(Modifier::new().focus_group(), Some(1)), // 1 outer G
            LayoutNode::new(Modifier::new().focus_group(), Some(2)), // 2 inner H
            LayoutNode::leaf(Modifier::new().focusable()),      // 3 A
            LayoutNode::leaf(Modifier::new().focusable()),      // 4 B
            LayoutNode::leaf(Modifier::new().focusable()),      // 5 C
            LayoutNode::leaf(Modifier::new().focusable()),      // 6 D
        ];
        nodes[0].children = vec![1, 6];
        nodes[1].children = vec![2, 5];
        nodes[2].children = vec![3, 4];
        for (i, node) in nodes.iter_mut().enumerate() {
            node.id = i as u64;
        }

        // Focus B (inside the inner group), then C (outside it, inside the outer group): the outer
        // group's active child becomes C while the inner group still remembers B.
        assert!(focus_node_by_id(&mut nodes, 0, 4));
        assert!(focus_node_by_id(&mut nodes, 0, 5));
        assert_eq!(nodes[1].focus_group_memory, Some(5));
        assert_eq!(nodes[2].focus_group_memory, Some(4));

        // Tab out to D, then wrap to A, the first focusable of the OUTER group. Entering that group
        // restores ITS active child C; using the nearest (inner) group would wrongly land on B.
        assert!(focus_node_by_id(&mut nodes, 0, 6));
        assert!(focus_next(&mut nodes, 0));
        assert_eq!(
            get_focus_id(&nodes, 0),
            Some(5),
            "the outer group's memory (C) wins over the inner group's stale B"
        );

        // Backwards: from D, Shift+Tab lands on the outer group's LAST focusable C, and the same
        // outer-first rule applies.
        assert!(focus_node_by_id(&mut nodes, 0, 6));
        assert!(focus_prev(&mut nodes, 0));
        assert_eq!(get_focus_id(&nodes, 0), Some(5));
    }

    #[test]
    fn on_focus_changed_reports_gain_and_loss() {
        let seen: std::sync::Arc<std::sync::Mutex<Vec<bool>>> = Default::default();
        let s = seen.clone();
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::leaf(
                Modifier::new()
                    .focusable()
                    .on_focus_changed(move |focused| s.lock().unwrap().push(focused)),
            ),
        ];
        nodes[0].children = vec![1];
        for (i, node) in nodes.iter_mut().enumerate() {
            node.id = i as u64;
        }

        // The initial "not focused" state is not a change: nothing has been reported yet.
        dispatch_focus_changed(&mut nodes, 0);
        assert!(seen.lock().unwrap().is_empty());

        assert!(set_focus_by_id(&mut nodes, 0, 1));
        dispatch_focus_changed(&mut nodes, 0);
        assert_eq!(*seen.lock().unwrap(), vec![true]);

        clear_focus(&mut nodes, 0);
        dispatch_focus_changed(&mut nodes, 0);
        assert_eq!(*seen.lock().unwrap(), vec![true, false]);
    }
