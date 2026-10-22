    use crate::layout::node::{hit_test, LayoutNode};
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState};

    /// 构造：root(Column) → outer_scroll(0,184,420×536) → [内容0..7, 内层scroll, 内容8..]
    fn build_demo_tree() -> (Vec<LayoutNode>, usize) {
        let outer = ScrollState::new();
        let inner = ScrollState::new();
        let mut nodes = Vec::new();
        // idx 0: root Column（全窗口 420×720）
        nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 720.0)));
        nodes[0].measured_size = Size::new(420.0, 720.0);
        // idx 1: 外层 scroll（视口 536，顶部 y=184）
        nodes.push(LayoutNode::leaf(Modifier::new().vertical_scroll(outer).size(420.0, 536.0)));
        nodes[1].position = Offset::new(0.0, 184.0);
        nodes[1].measured_size = Size::new(420.0, 536.0);
        nodes[1].scroll_viewport_height = 536.0;
        nodes[0].children.push(1); // 外层 scroll 挂到 root
        // 内容 0..7：每行 37 高
        for i in 0..8 {
            let idx = nodes.len();
            nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 37.0)));
            nodes[idx].position = Offset::new(0.0, i as f32 * 37.0);
            nodes[idx].measured_size = Size::new(420.0, 37.0);
            nodes[1].children.push(idx);
        }
        // 内层 scroll：内容流 y = 8*37 = 296，视口 180
        let inner_idx = nodes.len();
        nodes.push(LayoutNode::leaf(Modifier::new().vertical_scroll(inner).size(420.0, 180.0)));
        nodes[inner_idx].position = Offset::new(0.0, 296.0);
        nodes[inner_idx].measured_size = Size::new(420.0, 180.0);
        nodes[inner_idx].scroll_viewport_height = 180.0;
        // 内层子项：一列（可滚动内容）
        let item_idx = nodes.len();
        nodes.push(LayoutNode::leaf(Modifier::new().size(420.0, 32.0)));
        nodes[item_idx].position = Offset::new(0.0, 0.0);
        nodes[item_idx].measured_size = Size::new(420.0, 32.0);
        nodes[inner_idx].children.push(item_idx);
        nodes[1].children.push(inner_idx);
        (nodes, 1) // 返回 outer_scroll idx
    }

    #[test]
    fn drag_target_on_outer_content_is_outer_not_inner() {
        let (nodes, outer_idx) = build_demo_tree();
        // 鼠标在外层内容区域：屏幕 y=250（外层顶部 184 + 内容 y=66 → 内容 1）
        let path = hit_test(&nodes, 0, 210.0, 250.0);
        assert!(!path.is_empty(), "应命中某节点");
        // 命中路径应包含外层 scroll，且**不含内层 scroll**
        assert!(path.contains(&outer_idx), "外层 scroll 应在命中路径");
        // 内层 scroll 视觉位置 y=480..660——y=250 不应命中
        let inner_idx = nodes[outer_idx].children[8]; // 内容 0..7 之后是内层
        assert!(!path.contains(&inner_idx), "鼠标在外层内容区域不应命中内层 scroll");
        // drag_scroll 目标选择：path 逆序找第一个 scroll → 应为外层
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&outer_idx), "drag 目标应是外层 scroll");
    }

    #[test]
    fn drag_target_on_inner_list_is_inner() {
        let (nodes, outer_idx) = build_demo_tree();
        // 鼠标在内层列表视觉区域：屏幕 y=500（内层 480..660）
        let path = hit_test(&nodes, 0, 210.0, 500.0);
        let inner_idx = nodes[outer_idx].children[8];
        assert!(path.contains(&inner_idx), "内层 scroll 应在命中路径");
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&inner_idx), "drag 目标应是内层 scroll");
    }

    /// 用户复现场景：外层已滚动（offset>0），内层列表视觉上移到外层内容
    /// 0-7 区域。此时鼠标在外层顶部内容区域按下——期望滚外层（鼠标视觉
    /// 在"外层内容"上），但若命中内层则 bug。
    #[test]
    fn drag_target_after_outer_scrolled_uses_visual_position() {
        let (mut nodes, outer_idx) = build_demo_tree();
        // 外层滚 offset=300：内层视觉 y = 184 + 296 - 300 = 180（上移到顶部区域）
        let outer = nodes[outer_idx].modifier.vertical_scroll_state().unwrap().clone();
        outer.offset.set(300.0);
        let inner_idx = nodes[outer_idx].children[8];
        // 内层视觉位置（渲染）应在 180..360——覆盖"外层内容 0-7"区域
        let (_, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[outer_idx]);
        let inner_visual_y = 184.0 + nodes[inner_idx].position.y - sdy;
        assert!(inner_visual_y < 400.0, "内层应上移到顶部区域（视觉 y={}）", inner_visual_y);

        // 情形 1：鼠标在 y=250（内层视觉覆盖区）——应命中内层（z-order）
        let path = hit_test(&nodes, 0, 210.0, 250.0);
        let contains_inner = path.contains(&inner_idx);
        assert!(contains_inner,
            "内层视觉覆盖 y=250（视觉 y={inner_visual_y}..{:.0}）应命中内层——z-order 语义",
            inner_visual_y + 180.0);
        let target = path.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target, Some(&inner_idx), "drag 目标应是内层（视觉覆盖区）");

        // 情形 2：鼠标在 y=440（内层视觉区下方，仍在外层视口内）——应命中
        // 外层内容（内层不覆盖该点）
        let path2 = hit_test(&nodes, 0, 210.0, 440.0);
        assert!(!path2.contains(&inner_idx),
            "y=440 在内层视觉区（{inner_visual_y}..{:.0}）下方，不应命中内层",
            inner_visual_y + 180.0);
        let target2 = path2.iter().rev().find(|&&i| {
            nodes[i].modifier.vertical_scroll_state().is_some()
                || nodes[i].modifier.horizontal_scroll_state().is_some()
        });
        assert_eq!(target2, Some(&outer_idx), "drag 目标应是外层（内层不覆盖处）");
    }

    /// 验证：真实 Composer 两次 compose（状态行文本变化触发重组）后，
    /// 外层/内层 scroll 容器的 slot_key 保持稳定（drag_scroll 依赖 slot_key
    /// 在 move 阶段定位节点——key 漂移会滚错目标）。
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
                        // 状态行（文本随 offset 变化 → 触发重组）
                        Text::new(format!("outer: {:.0} inner: {:.0}", outer.offset.get(), inner.offset.get()))
                            .font_size(12.0).build(ctx);
                        // 外层滚动区
                        Column::new()
                            .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(outer.clone()))
                            .build(ctx, |ctx| {
                                for i in 0..8 {
                                    Text::new(format!("页面内容 {i}"))
                                        .modifier(Modifier::new().padding(10.0).fill_max_width())
                                        .build(ctx);
                                }
                                // 内层列表
                                Column::new()
                                    .modifier(Modifier::new().fill_max_width().height(180.0).vertical_scroll(inner.clone()))
                                    .build(ctx, |ctx| {
                                        for i in 0..30 {
                                            Text::new(format!("内层列表项 {i}"))
                                                .modifier(Modifier::new().padding(8.0).fill_max_width())
                                                .build(ctx);
                                        }
                                    });
                            });
                    });
            });
        };

        // 帧 1
        build(&mut composer, &outer, &inner);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let collect_keys = |composer: &Composer| -> Vec<u64> {
            composer.arena_nodes().iter()
                .filter(|n| n.modifier.vertical_scroll_state().is_some())
                .map(|n| n.slot_key)
                .collect()
        };
        let keys1 = collect_keys(&composer);
        assert_eq!(keys1.len(), 2, "应有外层+内层两个 scroll 节点");
        // ⚠ 关键：两个 scroll 节点的 slot_key 必须**互不相同**——若碰撞，
        // drag_scroll move 阶段 find_node_id_by_slot_key 会返回第一个匹配
        // （可能滚错目标：按下外层、move 滚内层——用户报告的 bug）
        assert_ne!(keys1[0], keys1[1],
            "外层与内层 scroll 的 slot_key 碰撞！{keys1:?}——drag_scroll 会定位到错误节点");
        eprintln!("[keys] 帧1 scroll keys = {keys1:?}（互不相同 ✅）");

        // 帧 2：内层滚动（状态行文本变化 → 重组）
        inner.offset.set(29.0);
        build(&mut composer, &outer, &inner);
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let keys2 = collect_keys(&composer);
        assert_eq!(keys1, keys2,
            "scroll 容器 slot_key 跨重组必须稳定（drag_scroll move 阶段依赖）——\n帧1={keys1:?} 帧2={keys2:?}");
    }

    /// scroll 容器高度 clamp 回归（review B1）：`.height(180)` + padding 的
    /// 滚动容器，measured_size 必须精确为 180（内容再多也不撑开）。
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
                    // 30 项内容——总高远超 180
                    for i in 0..30 {
                        Text::new(format!("列表项 {i}"))
                            .modifier(Modifier::new().padding(8.0).fill_max_width())
                            .build(ctx);
                    }
                });
        });
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 720.0));
        let root = composer.layout_root_idx().unwrap();
        let n = &composer.arena_nodes()[root];
        assert_eq!(n.measured_size.height, 180.0,
            "scroll 容器高度必须 clamp 回 .height(180)，实际 {}", n.measured_size.height);
        assert!(n.scroll_viewport_height > 0.0, "viewport 应已回写（供滚动 clamp）");
    }

    /// scroll 容器无固定高度 + 无限父约束：随内容撑开（clamp 是 no-op）。
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
                        Text::new(format!("列表项 {i}"))
                            .modifier(Modifier::new().padding(8.0).fill_max_width())
                            .build(ctx);
                    }
                });
        });
        // 父约束高度无限（f32::MAX）——容器应随内容撑开
        composer.layout(Constraints::new(0.0, 420.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let n = &composer.arena_nodes()[root];
        assert!(n.measured_size.height > 0.0,
            "无固定高度 + 无限约束应随内容撑开（>0），实际 {}", n.measured_size.height);
    }

