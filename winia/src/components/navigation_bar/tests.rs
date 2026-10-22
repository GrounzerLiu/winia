    use super::*;
    use crate::runtime::composer::Composer;

    /// The two item progressions, pinned to M3: the alpha rides `DefaultEffects`, the indicator size
    /// `FastSpatial` (0.9/1400). winia used one no-bouncy spring at 200/400 for both.
    #[test]
    fn the_item_progressions_run_on_m3s_tokens() {
        match nav_alpha_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::effects_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (1.0, 1600.0));
            }
            other => panic!("alpha must be a spring, got {other:?}"),
        }
        match nav_size_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("size must be a spring, got {other:?}"),
        }
    }

    fn icon_leaf(ctx: &mut ComposeCtx, size: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(size, size));
        ctx.end_node();
    }

    fn make_item(selected: bool, always: bool) -> NavigationBarItem {
        NavigationBarItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_BAR_ICON_SIZE))
            .label(|ctx| crate::components::Text::new("Home").build(ctx))
            .always_show_label(always)
            .on_click(|| {})
    }

    fn compose_layout(item: NavigationBarItem) -> Composer {
        let mut composer = Composer::new();
        composer.compose(|ctx| item.build(ctx));
        composer.layout(Constraints::new(0.0, f32::MAX, 0.0, f32::MAX));
        composer
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, index: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn navigation_bar_tokens_match_androidx_main() {
        assert_eq!(NAVIGATION_BAR_HEIGHT, 80.0, "TallContainerHeight");
        assert_eq!(NAVIGATION_BAR_ITEM_SPACING, 8.0, "NavigationBarItemHorizontalPadding");
        assert_eq!(NAVIGATION_BAR_INDICATOR_WIDTH, 56.0, "ActiveIndicatorWidth");
        assert_eq!(NAVIGATION_BAR_INDICATOR_HEIGHT, 32.0, "ActiveIndicatorHeight");
        assert_eq!(NAVIGATION_BAR_ICON_SIZE, 24.0, "IconSize");
        assert_eq!(INDICATOR_HORIZONTAL_PADDING, 16.0);
        assert_eq!(INDICATOR_VERTICAL_PADDING, 4.0);
        assert_eq!(INDICATOR_TO_LABEL_PADDING, 4.0, "NavigationBarIndicatorToLabelPadding");
        assert_eq!(DISABLED_ALPHA, 0.38);
        // 水平 item（NavigationBarHorizontalItemTokens / ShortNavigationBar.kt）
        assert_eq!(NAVIGATION_BAR_H_INDICATOR_HEIGHT, 40.0, "Horizontal ActiveIndicatorHeight");
        assert_eq!(H_INDICATOR_VERTICAL_PADDING, 8.0);
        assert_eq!(H_INDICATOR_HORIZONTAL_PADDING, 16.0, "ActiveIndicatorLeadingSpace");
        assert_eq!(START_ICON_TO_LABEL_PADDING, 4.0, "ItemActiveIndicatorIconLabelSpace");
    }

    #[test]
    fn item_colors_follow_v0_11_0_tokens() {
        let theme = crate::theme::ThemeColors::default_light();
        let c = NavigationBarItemColors::from_theme(&theme);
        assert_eq!(c.selected_icon, theme.on_secondary_container, "ItemActiveIconColor");
        assert_eq!(c.selected_label, theme.secondary, "ItemActiveLabelTextColor(v0_11_0)");
        assert_eq!(c.indicator, theme.secondary_container, "ItemActiveIndicatorColor");
        assert_eq!(c.unselected_icon, theme.on_surface_variant, "ItemInactiveIconColor");
        assert_eq!(c.unselected_label, theme.on_surface_variant, "ItemInactiveLabelTextColor");
        assert_eq!(c.disabled_icon, with_alpha_factor(theme.on_surface_variant, 0.38));
        assert_eq!(c.disabled_label, with_alpha_factor(theme.on_surface_variant, 0.38));
        // 颜色解析分支对齐 iconColor/textColor
        assert_eq!(c.icon_color(true, true), c.selected_icon);
        assert_eq!(c.icon_color(false, true), c.unselected_icon);
        assert_eq!(c.icon_color(true, false), c.disabled_icon);
        assert_eq!(c.text_color(false, false), c.disabled_label);
    }

    #[test]
    fn bar_splits_width_equally_with_spacing_and_rtl_mirrors() {
        let build_items = |ctx: &mut ComposeCtx| {
            for i in 0..3 {
                NavigationBarItem::new(i == 0, |ctx| icon_leaf(ctx, 24.0))
                    .on_click(|| {})
                    .build(ctx);
            }
        };
        let mut ltr = Composer::new();
        ltr.compose(|ctx| {
            NavigationBar::new(build_items)
                .modifier(Modifier::new().test_tag("bar"))
                .build(ctx)
        });
        ltr.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
        let root = ltr.layout_root_idx().unwrap();
        let nodes = ltr.arena_nodes();
        let item_w = (360.0 - 2.0 * NAVIGATION_BAR_ITEM_SPACING) / 3.0;
        for i in 0..3usize {
            let item = child(nodes, root, i);
            assert_eq!(item.measured_size.width, item_w, "item 等分宽度");
            assert_eq!(
                item.position.x,
                i as f32 * (item_w + NAVIGATION_BAR_ITEM_SPACING),
                "LTR 第 i 个 item 的 x"
            );
            assert_eq!(item.measured_size.height, NAVIGATION_BAR_HEIGHT);
        }

        let build_items_rtl = |ctx: &mut ComposeCtx| {
            for i in 0..3 {
                NavigationBarItem::new(i == 0, |ctx| icon_leaf(ctx, 24.0))
                    .on_click(|| {})
                    .build(ctx);
            }
        };
        let mut rtl = Composer::new();
        rtl.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                crate::theme::ThemeColors::default_light(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| NavigationBar::new(build_items_rtl).build(ctx),
            );
        });
        rtl.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
        let rroot = rtl.layout_root_idx().unwrap();
        let rnodes = rtl.arena_nodes();
        let first = child(rnodes, rroot, 0);
        assert_eq!(first.position.x, 360.0 - item_w, "RTL 下第一个 item 镜像到最右");
    }

    #[test]
    fn selected_item_geometry_matches_androidx_place_label_and_icon() {
        // 无界宽度单测：containerW = iconW + 2x44 = 112
        let composer = compose_layout(make_item(true, true));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple_node = child(nodes, root, 3);

        // contentH = 24 + 4 + 4 + 16(labelMedium 行高) = 48；vPad = (80-48)/2 = 16
        let v_pad = (NAVIGATION_BAR_HEIGHT - 48.0) / 2.0;
        assert_eq!(nodes[root].measured_size.height, NAVIGATION_BAR_HEIGHT);
        // 指示器：宽 = 24+32 = 56（progress=1 全宽），y = iconY - 4，水平居中
        assert_eq!(indicator.measured_size.width, NAVIGATION_BAR_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_BAR_INDICATOR_HEIGHT);
        assert_eq!(indicator.position.y, v_pad - INDICATOR_VERTICAL_PADDING);
        assert_eq!(
            (nodes[root].measured_size.width - indicator.measured_size.width) / 2.0,
            indicator.position.x,
            "指示器水平居中"
        );
        // ripple：恒定全尺寸——选中态与胶囊重合
        assert_eq!(ripple_node.measured_size, indicator.measured_size);
        assert_eq!(ripple_node.position, indicator.position);
        // 图标：y = vPad（选中顶部位）
        assert_eq!(icon.position.y, v_pad);
        assert_eq!(icon.measured_size, Size::new(24.0, 24.0));
        // label：y = iconY + iconH + 4 + 4
        assert_eq!(
            label.position.y,
            v_pad + NAVIGATION_BAR_ICON_SIZE + INDICATOR_VERTICAL_PADDING + INDICATOR_TO_LABEL_PADDING
        );
        // The label shares the indicator's axis: both are centred on the container. Nothing pinned the
        // label's x before this, so the centring basis could have drifted with no test noticing. Note that
        // M3 centres it elsewhere — its `labelXTopIcon` ignores the measured width and centres the label in
        // a fixed 72dp band (iconW + 2x(IndicatorHorizontalPadding + ItemHorizontalPadding)). That is a
        // different basis, not a different result on this geometry; running upstream to compare is not
        // possible here, so this test pins what winia actually does rather than claiming parity.
        assert_eq!(
            label.position.x + label.measured_size.width / 2.0,
            indicator.position.x + indicator.measured_size.width / 2.0,
            "label 居中在与指示器相同的轴上"
        );
    }

    #[test]
    fn unselected_item_without_always_label_centers_icon() {
        let composer = compose_layout(make_item(false, false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let ripple = child(nodes, root, 3);

        // progress=0：offset = (28-16)x1 = 12 → 图标垂直居中 (80-24)/2 = 28
        assert_eq!(icon.position.y, (NAVIGATION_BAR_HEIGHT - NAVIGATION_BAR_ICON_SIZE) / 2.0);
        assert_eq!(indicator.position.y, icon.position.y - INDICATOR_VERTICAL_PADDING);
        // 彩色指示器宽度收拢为 0（progress=0）
        assert_eq!(indicator.measured_size.width, 0.0);
        // 状态层载体（ripple 节点）恒定全尺寸——未选中悬浮仍有完整胶囊热区，
        // 且跟随指示器的动画位置（与可见图标对齐）
        assert_eq!(ripple.measured_size.width, NAVIGATION_BAR_INDICATOR_WIDTH);
        assert_eq!(ripple.measured_size.height, NAVIGATION_BAR_INDICATOR_HEIGHT);
        assert_eq!(ripple.position.x, indicator.position.x, "ripple 与胶囊同 x");
        assert_eq!(ripple.position.y, indicator.position.y, "ripple 跟随胶囊动画位置");
    }

    fn make_horizontal_item(selected: bool) -> NavigationBarItem {
        NavigationBarItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_BAR_ICON_SIZE))
            .label(|ctx| crate::components::Text::new("Home").build(ctx))
            .icon_position(NavigationItemIconPosition::Start)
            .on_click(|| {})
    }

    #[test]
    fn horizontal_item_geometry_matches_androidx_place_label_and_start_icon() {
        // 有界宽 120：验证内容组居中 + 指示器包裹整组（StartIconMeasurePolicy 数学）
        let mut composer = Composer::new();
        composer.compose(|ctx| make_horizontal_item(true).build(ctx));
        composer.layout(Constraints::new(0.0, 120.0, 0.0, NAVIGATION_BAR_HEIGHT));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple_node = child(nodes, root, 3);

        assert_eq!(nodes[root].measured_size, Size::new(120.0, NAVIGATION_BAR_HEIGHT));
        let label_w = label.measured_size.width;
        // 内容组宽 = iconW + gap(4) + labelW；iconX = (containerW - contentW)/2
        let content_w = NAVIGATION_BAR_ICON_SIZE + START_ICON_TO_LABEL_PADDING + label_w;
        let icon_x = (120.0 - content_w) / 2.0;
        // 全部垂直居中
        assert_eq!(icon.position, Offset::new(icon_x, (NAVIGATION_BAR_HEIGHT - 24.0) / 2.0));
        assert_eq!(
            label.position,
            Offset::new(icon_x + NAVIGATION_BAR_ICON_SIZE + START_ICON_TO_LABEL_PADDING, (NAVIGATION_BAR_HEIGHT - label.measured_size.height) / 2.0)
        );
        // 指示器：宽 = contentW + leading+trailing(2x16)，高 = max(24,labelH)+2x8 = 40，居中
        assert_eq!(indicator.measured_size.width, content_w + H_INDICATOR_HORIZONTAL_PADDING * 2.0);
        assert_eq!(indicator.measured_size.height, NAVIGATION_BAR_H_INDICATOR_HEIGHT);
        assert_eq!(
            indicator.position,
            Offset::new((120.0 - indicator.measured_size.width) / 2.0, (NAVIGATION_BAR_HEIGHT - NAVIGATION_BAR_H_INDICATOR_HEIGHT) / 2.0)
        );
        // ripple：恒定全尺寸、与选中态胶囊重合（Constraints.fixed(totalW, h)）
        assert_eq!(ripple_node.measured_size, indicator.measured_size);
        assert_eq!(ripple_node.position, indicator.position);
    }

    #[test]
    fn horizontal_item_without_label_falls_back_to_circular_indicator() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationBarItem::new(true, |ctx| icon_leaf(ctx, NAVIGATION_BAR_ICON_SIZE))
                .icon_position(NavigationItemIconPosition::Start)
                .on_click(|| {})
                .build(ctx)
        });
        composer.layout(Constraints::new(0.0, 120.0, 0.0, NAVIGATION_BAR_HEIGHT));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        // 无 label → TopIconOrIconOnlyMeasurePolicy：56x32 圆形胶囊（同垂直）
        assert_eq!(indicator.measured_size.width, NAVIGATION_BAR_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_BAR_INDICATOR_HEIGHT);
    }

    #[test]
    fn always_show_label_keeps_positions_stable_when_unselected() {
        let selected = compose_layout(make_item(true, true));
        let unselected = compose_layout(make_item(false, true));
        let sn = selected.arena_nodes();
        let un = unselected.arena_nodes();
        let s_icon = child(sn, selected.layout_root_idx().unwrap(), 1);
        let u_icon = child(un, unselected.layout_root_idx().unwrap(), 1);
        assert_eq!(s_icon.position.y, u_icon.position.y, "alwaysShowLabel 时图标位置不随选中变化");
    }

    #[test]
    fn enabled_item_indicator_carries_pill_ripple_and_dynamic_background() {
        let composer = compose_layout(make_item(true, true));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // 节点拆分：children[0] = 彩色胶囊、最后一个子节点 = ripple（状态层载体）
        let indicator = child(nodes, root, 0);
        let ripple_node = child(nodes, root, 3);
        // ripple 挂在独立节点上且为 Pill 形状
        let ripple = ripple_node
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Ripple { shape, .. } => Some(shape),
                _ => None,
            })
            .expect("ripple 节点应有 ripple");
        assert_eq!(ripple.as_ref().map(|s| s.eval()), Some(Shape::Pill));
        // The background is a dynamic colour: at progress = 1 it is the full-alpha indicator colour.
        let theme = crate::theme::ThemeColors::default_light();
        let background = indicator
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color, .. } => Some(color.clone()),
                _ => None,
            })
            .expect("the coloured indicator has a dynamic background");
        assert_eq!(background.eval(), theme.secondary_container);
        // 职责分离：ripple 节点无背景、胶囊节点无 ripple
        assert!(ripple_node.modifier.elements().iter().all(|el| !matches!(
            el,
            crate::modifier::ModifierElement::Background { .. }
        )));
        assert!(indicator.modifier.elements().iter().all(|el| !matches!(
            el,
            crate::modifier::ModifierElement::Ripple { .. }
        )));
    }

    fn pixel_at(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8, u8) {
        let mut pixels = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        surface.read_pixels(&info, &mut pixels, 4, (x, y));
        (pixels[0], pixels[1], pixels[2], pixels[3])
    }

    #[test]
    fn unselected_item_hover_shows_state_layer_on_full_pill_rect() {
        let _serial = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = crate::theme::ThemeColors::default_light();
        let container = theme.surface_container;
        let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

        // item1（未选中）用自备交互源——测试手动发射 Hover Enter
        let source = MutableInteractionSource::new();
        let src_for_item = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationBar::new(move |ctx| {
                NavigationBarItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("A").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
                NavigationBarItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("B").build(ctx))
                    .interaction_source(src_for_item.clone())
                    .on_click(|| {})
                    .build(ctx);
                NavigationBarItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("C").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
            })
            .build(ctx)
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));

        // 悬浮未选中项 → 状态层透明度动画（500ms tween，真实时钟）——
        // 轮询推进至收敛（STATE_LAYER_HOVER 0.08）
        source.emit_hover_enter();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
        while std::time::Instant::now() < deadline {
            crate::animation::update_animations();
            if source.hover_opacity_value() >= 0.079 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }

        let root = composer.layout_root_idx().unwrap();
        let mut surface = skia_safe::surfaces::raster_n32_premul((360, 80)).expect("navbar surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, surface.canvas());

        // 未选中项胶囊矩形内（避开图标 168..192）取色：item1 x 起点 = 114.67+8，
        // ripple_x = (item_w - 56)/2 → 胶囊左缘全局 ≈152；取 (158, 28)
        // 像素 = 容器色上叠加 onSurfaceVariant @8% —— 必然偏离纯容器色
        let mut s = surface;
        let px = pixel_at(&mut s, 158, 28);
        assert_ne!(px, to_rgba(container), "悬浮未选中项应显示状态层（完整胶囊热区）");
        // 无悬浮的 item2 同位置保持纯容器色（对照）
        assert_eq!(pixel_at(&mut s, 320, 28), to_rgba(container), "无悬浮项不应有状态层");
    }

    #[test]
    fn disabled_item_has_no_interaction_elements() {
        let composer = compose_layout(make_item(true, true).enabled(false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert!(nodes[root].modifier.clickable_interaction().is_none());
        // 禁用项：ripple 节点 Modifier 为空（无状态层载体）、彩色胶囊无 ripple
        let ripple_node = child(nodes, root, 3);
        let indicator = child(nodes, root, 0);
        assert!(ripple_node.modifier.ripple_interaction().is_none(), "禁用项无状态层载体");
        assert!(indicator.modifier.ripple_interaction().is_none(), "禁用项指示器无 ripple");
    }

    #[test]
    fn scaffold_bottom_bar_integrates_navigation_bar() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            crate::components::Scaffold::new(|ctx, _| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_size().test_tag("content"));
                ctx.end_node();
            })
            .top_bar(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_width().height(64.0));
                ctx.end_node();
            })
            .bottom_bar(|ctx| {
                NavigationBar::new(|ctx| {
                    for i in 0..3 {
                        NavigationBarItem::new(i == 0, |ctx| icon_leaf(ctx, 24.0))
                            .label(move |ctx| {
                                crate::components::Text::new(format!("Item{i}")).build(ctx);
                            })
                            .on_click(|| {})
                            .build(ctx);
                    }
                })
                .build(ctx);
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // Scaffold 子节点顺序：[top, bottom, content, fab]
        let bottom_slot = child(nodes, root, 1);
        let content = child(nodes, root, 2);
        assert_eq!(bottom_slot.measured_size.height, NAVIGATION_BAR_HEIGHT, "bottom bar 高度 80dp");
        assert_eq!(content.position.y, 64.0);
        assert_eq!(content.measured_size.height, 640.0 - 64.0 - NAVIGATION_BAR_HEIGHT);
        // NavigationBar 内 item 等分（bottom slot -> bar -> items）
        let bar_idx = nodes[bottom_slot_idx(nodes, root)].children[0];
        let item_w = (360.0 - 2.0 * NAVIGATION_BAR_ITEM_SPACING) / 3.0;
        assert_eq!(child(nodes, bar_idx, 0).measured_size.width, item_w);
    }

    fn bottom_slot_idx(nodes: &[LayoutNode], root: usize) -> usize {
        nodes[root].children[1]
    }
