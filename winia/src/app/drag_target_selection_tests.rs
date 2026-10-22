    use crate::layout::node::{hit_test, LayoutNode};
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState};

    /// Builds: root(Column) -> outer_scroll(0,184,420x536) -> [content 0..7, inner scroll, content 8..].
    fn build_demo_tree() -> (Vec<LayoutNode>, usize) {
        let outer = ScrollState::new();
        let inner = ScrollState::new();
        let mut nodes = Vec::new();
        // idx 0: root Column (the whole window, 420x720).
        nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 720.0)));
        nodes[0].measured_size = Size::new(420.0, 720.0);
        // idx 1: the outer scroll (viewport 536, top at y=184).
        nodes.push(LayoutNode::leaf(Modifier::new().vertical_scroll(outer).size(420.0, 536.0)));
        nodes[1].position = Offset::new(0.0, 184.0);
        nodes[1].measured_size = Size::new(420.0, 536.0);
        nodes[1].scroll_viewport_height = 536.0;
        nodes[0].children.push(1); // attach the outer scroll to the root
        // Content 0..7: 37 high each.
        for i in 0..8 {
            let idx = nodes.len();
            nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 37.0)));
            nodes[idx].position = Offset::new(0.0, i as f32 * 37.0);
            nodes[idx].measured_size = Size::new(420.0, 37.0);
            nodes[1].children.push(idx);
        }
        // Inner scroll: the content flow sits at y = 8*37 = 296, viewport 180.
        let inner_idx = nodes.len();
        nodes.push(LayoutNode::leaf(Modifier::new().vertical_scroll(inner).size(420.0, 180.0)));
        nodes[inner_idx].position = Offset::new(0.0, 296.0);
        nodes[inner_idx].measured_size = Size::new(420.0, 180.0);
        nodes[inner_idx].scroll_viewport_height = 180.0;
        // The inner child: one column (scrollable content).
        let item_idx = nodes.len();
        nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 32.0)));
        nodes[item_idx].position = Offset::new(0.0, 0.0);
        nodes[item_idx].measured_size = Size::new(420.0, 32.0);
        nodes[inner_idx].children.push(item_idx);
        nodes[1].children.push(inner_idx);
        (nodes, 1) // return the outer_scroll idx
    }

    #[test]
    fn drag_target_on_outer_content_is_outer_not_inner() {
        let (nodes, outer_idx) = build_demo_tree();
        // The mouse is over the outer content: screen y=250 (outer top 184 + content y=66 -> content 1).
        let path = hit_test(&nodes, 0, 210.0, 250.0);
        assert!(!path.is_empty(), "something should be hit");
        // The hit path should contain the outer scroll and **not the inner scroll**.
        assert!(path.contains(&outer_idx), "the outer scroll should be on the hit path");
        // The inner scroll sits visually at y=480..660, so y=250 must not hit it.
        let inner_idx = nodes[outer_idx].children[8]; // the inner scroll follows content 0..7
        assert!(!path.contains(&inner_idx), "a mouse over the outer content must not hit the inner scroll");
        // drag_scroll's target selection walks the path in reverse for the first scroll: the outer one.
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&outer_idx), "the drag target should be the outer scroll");
    }

    #[test]
    fn drag_target_on_inner_list_is_inner() {
        let (nodes, outer_idx) = build_demo_tree();
        // The mouse is over the inner list's visual area: screen y=500 (inner 480..660).
        let path = hit_test(&nodes, 0, 210.0, 500.0);
        let inner_idx = nodes[outer_idx].children[8];
        assert!(path.contains(&inner_idx), "the inner scroll should be on the hit path");
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&inner_idx), "the drag target should be the inner scroll");
    }

    /// The user-reported scenario: the outer list has scrolled (offset > 0), so the inner list has
    /// visually moved up into the outer content's 0-7 band. Pressing in that band should scroll the
    /// outer list (that is what the mouse is visually over); hitting the inner list is the bug.
    #[test]
    fn drag_target_after_outer_scrolled_uses_visual_position() {
        let (mut nodes, outer_idx) = build_demo_tree();
        // Outer scrolled to offset=300: the inner visual y = 184 + 296 - 300 = 180 (moved up to the top band).
        let outer = nodes[outer_idx].modifier.vertical_scroll_state().unwrap().clone();
        outer.offset.set(300.0);
        let inner_idx = nodes[outer_idx].children[8];
        // The inner's visual (rendered) position should be 180..360, covering the "outer content 0-7" band.
        let (_, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[outer_idx]);
        let inner_visual_y = 184.0 + nodes[inner_idx].position.y - sdy;
        assert!(inner_visual_y < 400.0, "the inner should have moved up into the top band (visual y={})", inner_visual_y);

        // Case 1: the mouse at y=250 (covered by the inner's visual area) should hit the inner (z-order).
        let path = hit_test(&nodes, 0, 210.0, 250.0);
        let contains_inner = path.contains(&inner_idx);
        assert!(contains_inner,
            "the inner's visual area covers y=250 (visual y={inner_visual_y}..{:.0}), so z-order says it hits the inner",
            inner_visual_y + 180.0);
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&inner_idx), "the drag target should be the inner (the visually covered area)");

        // Case 2: the mouse at y=440 (below the inner's visual area, still inside the outer viewport)
        // should hit the outer content, since the inner does not cover that point.
        let path2 = hit_test(&nodes, 0, 210.0, 440.0);
        assert!(!path2.contains(&inner_idx),
            "y=440 is below the inner's visual area ({inner_visual_y}..{:.0}), so it must not hit the inner",
            inner_visual_y + 180.0);
        let target2 = path2.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target2, Some(&outer_idx), "the drag target should be the outer (where the inner does not cover)");
    }

    /// Verifies that after a real Composer composes twice (a change in the status line's text triggers
    /// recomposition) the outer and inner scroll containers keep stable slot_keys. drag_scroll locates
    /// its node by slot_key during the move phase, so a drifting key scrolls the wrong target.
    #[test]
    fn scroll_slot_keys_stable_across_recompose() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::Column;
        use crate::components::Text;
        let mut composer = Composer::new();
        let outer = ScrollState::new();
        let inner = ScrollState::new();

        let build = |composer: &mut Composer, outer: &ScrollState, inner: &ScrollState| {
            composer.compose(|ctx| {
                crate::layout::Column::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                        // The status line (its text follows the offset, so it triggers recomposition).
                        Text::new(format!("outer: {:.0} inner: {:.0}", outer.offset.get(), inner.offset.get()))
                            .font_size(12.0).build(ctx);
                        // The outer scroll area.
                        Column::new()
                            .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(outer.clone()))
                            .build(ctx, |ctx| {
                                for i in 0..8 {
                                    Text::new(format!("page content {i}"))
                                        .modifier(Modifier::new().padding(10.0).fill_max_width())
                                        .build(ctx);
                                }
                                // The inner list.
                                Column::new()
                                    .modifier(Modifier::new().fill_max_width().height(180.0).vertical_scroll(inner.clone()))
                                    .build(ctx, |ctx| {
                                        for i in 0..30 {
                                            Text::new(format!("inner list item {i}"))
                                                .modifier(Modifier::new().padding(8.0).fill_max_width())
                                                .build(ctx);
                                        }
                                    });
                            });
                    });
            });
        };

        // Frame 1.
        build(&mut composer, &outer, &inner);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let collect_keys = |composer: &Composer| -> Vec<u64> {
            composer.arena_nodes().iter()
                .filter(|n| n.modifier.vertical_scroll_state().is_some())
                .map(|n| n.slot_key)
                .collect()
        };
        let keys1 = collect_keys(&composer);
        assert_eq!(keys1.len(), 2, "there should be two scroll nodes, outer and inner");
        // Key point: the two scroll nodes' slot_keys must **differ**. On a collision,
        // find_node_id_by_slot_key in drag_scroll's move phase returns the first match, which can
        // scroll the wrong target (press on the outer, move, and the inner scrolls: the user-reported bug).
        assert_ne!(keys1[0], keys1[1],
            "the outer and inner scroll slot_keys collided! {keys1:?}: drag_scroll would locate the wrong node");
        eprintln!("[keys] frame 1 scroll keys = {keys1:?} (all distinct)");

        // Frame 2: the inner scrolls (the status line's text changes, so it recomposes).
        inner.offset.set(29.0);
        build(&mut composer, &outer, &inner);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let keys2 = collect_keys(&composer);
        assert_eq!(keys1, keys2,
            "a scroll container's slot_key must be stable across recomposition (drag_scroll's move phase relies on it)\nframe1={keys1:?} frame2={keys2:?}");
    }

    /// Regression for the scroll container's height clamp (review B1): a scroll container with
    /// `.height(180)` plus padding must measure exactly 180, however much content it holds.
    #[test]
    fn scroll_container_height_clamps_to_fixed_height() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::Column;
        use crate::components::Text;
        let mut composer = Composer::new();
        let inner = ScrollState::new();
        composer.compose(|ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_width().height(180.0).padding(4.0).vertical_scroll(inner.clone()))
                .build(ctx, |ctx| {
                    // 30 items, whose total height is far past 180.
                    for i in 0..30 {
                        Text::new(format!("list item {i}"))
                            .modifier(Modifier::new().padding(8.0).fill_max_width())
                            .build(ctx);
                    }
                });
        });
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let root = composer.layout_root_idx().unwrap();
        let n = &composer.arena_nodes()[root];
        assert_eq!(n.measured_size.height, 180.0,
            "the scroll container's height must clamp back to .height(180), got {}", n.measured_size.height);
        assert!(n.scroll_viewport_height > 0.0, "the viewport should be written back for the scroll clamp");
    }

    /// A scroll container with no fixed height under unbounded parent constraints grows with its
    /// content (the clamp is a no-op).
    #[test]
    fn scroll_container_grows_with_unbounded_parent() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::Column;
        use crate::components::Text;
        let mut composer = Composer::new();
        let inner = ScrollState::new();
        composer.compose(|ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_width().vertical_scroll(inner.clone()))
                .build(ctx, |ctx| {
                    for i in 0..5 {
                        Text::new(format!("list item {i}"))
                            .modifier(Modifier::new().padding(8.0).fill_max_width())
                            .build(ctx);
                    }
                });
        });
        // The parent constraint's height is unbounded (f32::MAX), so the container should grow with its content.
        composer.layout(Constraints::new(0.0, 420.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let n = &composer.arena_nodes()[root];
        assert!(n.measured_size.height > 0.0,
            "with no fixed height and unbounded constraints it should grow with its content (>0), got {}", n.measured_size.height);
    }

