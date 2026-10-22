    use super::*;
    use crate::runtime::composer::Composer;

    /// The three springs this component names, pinned to their androidx tokens: item alpha
    /// `DefaultEffects` (1.0/1600), item size `FastSpatial` (0.9/1400), container expand
    /// `DefaultSpatial` (0.9/700). winia used one no-bouncy spring at 200/400 for all three.
    #[test]
    fn the_rail_springs_are_the_tokens_its_sources_name() {
        match rail_alpha_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::effects_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (1.0, 1600.0));
            }
            other => panic!("alpha must be a spring, got {other:?}"),
        }
        match rail_item_size_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("item size must be a spring, got {other:?}"),
        }
        match rail_container_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 700.0));
            }
            other => panic!("the container must be a spring, got {other:?}"),
        }
        // The modal form swaps the container token for the fast one
        // (`WideNavigationRail.kt:269`: `modalAnimationSpec = FastSpatial`), and the wide rail ITEM
        // morphs on `DefaultSpatial` rather than the plain item's `FastSpatial`
        // (`NavigationItem.kt:486-491`, `:1232-1237`).
        match modal_rail_container_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("the modal container must be a spring, got {other:?}"),
        }
        match wide_rail_item_morph_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 700.0));
            }
            other => panic!("the wide rail item morph must be a spring, got {other:?}"),
        }
    }

    /// `WideNavigationRail::arrangement` had no test of its own: the API was added for the navigation
    /// suite and only exercised through it. Start keeps the stack at the top, Center splits the slack,
    /// End pushes it to the bottom. This builds NO header: the header's own position under a non-default
    /// arrangement is covered by `an_arranged_rail_with_a_primary_action_keeps_its_items_inside`.
    #[test]
    fn the_rail_arrangement_moves_the_items_only() {
        let first_item_y = |arrangement: Arrangement| -> f32 {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let state = WideNavigationRailState::new(ctx);
                WideNavigationRail::new(state, |ctx| {
                    for i in 0..3 {
                        WideNavigationRailItem::new(
                            i == 0,
                            |ctx| icon_leaf(ctx, NAVIGATION_RAIL_ICON_SIZE),
                            |ctx| crate::components::Text::new("Item").build(ctx),
                        )
                        .build(ctx);
                    }
                })
                .arrangement(arrangement)
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 96.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut found: Option<f32> = None;
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                y: f32,
                out: &mut Option<f32>,
            ) {
                let n = &nodes[idx];
                let ay = y + n.position.y;
                // The wide rail's collapsed items are 64 tall (`WIDE_RAIL_ITEM_MIN_HEIGHT`).
                if out.is_none() && (n.measured_size.height - 64.0).abs() < 0.5 {
                    *out = Some(ay);
                }
                for &child in &n.children {
                    walk(nodes, child, ay, out);
                }
            }
            walk(nodes, root, 0.0, &mut found);
            found.expect("the rail lays its items out")
        };

        let start = first_item_y(Arrangement::Start);
        let center = first_item_y(Arrangement::Center);
        let end = first_item_y(Arrangement::End);
        assert!(
            end > start + 50.0,
            "End pushes the items down (start {start}, end {end})"
        );
        assert!(
            center > start + 20.0 && center < end - 20.0,
            "Center sits between Start and End (start {start}, center {center}, end {end})"
        );
    }

    fn icon_leaf(ctx: &mut ComposeCtx, size: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(size, size));
        ctx.end_node();
    }

    fn make_item(selected: bool, always: bool) -> NavigationRailItem {
        NavigationRailItem::new(selected, |ctx| icon_leaf(ctx, NAVIGATION_RAIL_ICON_SIZE))
            .label(|ctx| crate::components::Text::new("Home").build(ctx))
            .always_show_label(always)
            .on_click(|| {})
    }

    fn compose_layout(item: NavigationRailItem) -> Composer {
        let mut composer = Composer::new();
        composer.compose(|ctx| item.build(ctx));
        composer.layout(Constraints::new(0.0, f32::MAX, 0.0, f32::MAX));
        composer
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, index: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn navigation_rail_tokens_match_androidx_main() {
        assert_eq!(NAVIGATION_RAIL_WIDTH, 80.0, "NarrowContainerWidth");
        assert_eq!(NAVIGATION_RAIL_ITEM_HEIGHT, 56.0, "= ActiveIndicatorWidth");
        assert_eq!(NAVIGATION_RAIL_INDICATOR_WIDTH, 56.0, "ActiveIndicatorWidth");
        assert_eq!(NAVIGATION_RAIL_INDICATOR_HEIGHT, 32.0, "ActiveIndicatorHeight");
        assert_eq!(NAVIGATION_RAIL_ICON_SIZE, 24.0, "BaselineItemTokens.IconSize");
        assert_eq!(INDICATOR_H_PADDING, 16.0);
        assert_eq!(INDICATOR_V_PADDING_WITH_LABEL, 4.0);
        assert_eq!(INDICATOR_V_PADDING_NO_LABEL, 16.0, "无标签→56x56 圆形胶囊");
        assert_eq!(ITEM_ICON_LABEL_GAP, 4.0, "NavigationRailItemVerticalPadding");
        assert_eq!(RAIL_VERTICAL_PADDING, 4.0, "NavigationRailVerticalPadding");
        assert_eq!(HEADER_SPACER, 8.0, "NavigationRailHeaderPadding");
        // The wide rail uses a DIFFERENT token for the same slot and it is much larger: androidx computes
        // `headerOffset = headerPlaceable.height + WNRHeaderPadding` (`WideNavigationRail.kt:427`), with
        // WNRHeaderPadding taken from `NavigationRailBaselineItemTokens.HeaderSpaceMinimum`
        // (`WideNavigationRail.kt:1395-1396`). The plain rail's gap is 4 + 8 + 4 = 16 — its 8dp Spacer
        // sits inside a Column whose `spacedBy(4)` adds 4 on each side (`NavigationRail.kt:126-140`) —
        // so the two rails are deliberately not equal: 40 against 16.
        assert_eq!(
            WIDE_RAIL_HEADER_PADDING, 40.0,
            "WNRHeaderPadding = HeaderSpaceMinimum"
        );
        assert_eq!(WIDE_RAIL_ITEM_H_PADDING, 20.0, "WNRItemHorizontalPadding");
        // The four the wide ITEM's own geometry rides on. They are pinned as LITERALS here and not
        // merely exercised through the constants: a test that reads the same constant the
        // production code reads stays green when that constant is changed back to the value the
        // commit fixed — which is how `WIDE_START_ICON_LABEL_GAP` could have gone back to 4 with
        // all 22 tests passing.
        assert_eq!(
            WIDE_RAIL_ITEM_MIN_HEIGHT, 64.0,
            "TopIconItemMinHeight = BaselineItem.ContainerHeight (collapsed)"
        );
        assert_eq!(WIDE_RAIL_ITEM_MIN_HEIGHT_EXPANDED, 48.0, "minimumA11ySize (expanded)");
        assert_eq!(WIDE_RAIL_ITEM_GAP_EXPANDED, 0.0, "itemVerticalSpacedBy when expanded");
        assert_eq!(WIDE_START_ICON_LABEL_GAP, 8.0, "HorizontalItem.IconLabelSpace (Start)");
        assert_eq!(
            WIDE_START_INDICATOR_V_PADDING, 16.0,
            "ItemStartIconIndicatorVerticalPadding = (56 - 24) / 2"
        );
    }

    #[test]
    fn rail_item_colors_follow_v0_11_0_tokens() {
        let theme = crate::theme::ThemeColors::default_light();
        let c = rail_item_colors(&theme);
        assert_eq!(c.selected_icon, theme.on_secondary_container, "ItemActiveIcon");
        assert_eq!(c.selected_label, theme.secondary, "ItemActiveLabelText");
        assert_eq!(c.indicator, theme.secondary_container, "ItemActiveIndicator");
        assert_eq!(c.unselected_icon, theme.on_surface_variant, "ItemInactiveIcon");
        assert_eq!(c.disabled_icon, with_alpha_factor(theme.on_surface_variant, DISABLED_ALPHA));
    }

    #[test]
    fn selected_item_geometry_matches_androidx_place_label_and_icon() {
        // 无界宽度单测：containerW = max(iconW, 胶囊全宽 56) = 56
        let composer = compose_layout(make_item(true, true));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple = child(nodes, root, 3);

        // contentH = 24 + 4 + 4 + 16 = 48；vPad = ((56-48)/2).max(4) = 4
        let v_pad = ((NAVIGATION_RAIL_ITEM_HEIGHT - 48.0) / 2.0).max(INDICATOR_V_PADDING_WITH_LABEL);
        assert_eq!(nodes[root].measured_size.height, NAVIGATION_RAIL_ITEM_HEIGHT);
        // 指示器：56x32（progress=1 全宽），y = iconY - 4 = 0
        assert_eq!(indicator.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_RAIL_INDICATOR_HEIGHT);
        assert_eq!(indicator.position.y, v_pad - INDICATOR_V_PADDING_WITH_LABEL);
        // 图标：y = vPad
        assert_eq!(icon.position.y, v_pad);
        assert_eq!(icon.measured_size, Size::new(24.0, 24.0));
        // label：y = iconY + iconH + 4 + 4
        assert_eq!(
            label.position.y,
            v_pad + NAVIGATION_RAIL_ICON_SIZE + INDICATOR_V_PADDING_WITH_LABEL + ITEM_ICON_LABEL_GAP
        );
        // ripple：恒定全尺寸、选中态与胶囊重合、z 最后
        assert_eq!(ripple.measured_size, indicator.measured_size);
        assert_eq!(ripple.position, indicator.position);
    }

    #[test]
    fn unselected_item_without_always_label_centers_icon() {
        let composer = compose_layout(make_item(false, false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let ripple = child(nodes, root, 3);

        // progress=0：offset = ((56-24)/2 - 4)x1 = 12 → 图标居中 (56-24)/2 = 16
        assert_eq!(icon.position.y, (NAVIGATION_RAIL_ITEM_HEIGHT - NAVIGATION_RAIL_ICON_SIZE) / 2.0);
        assert_eq!(indicator.position.y, icon.position.y - INDICATOR_V_PADDING_WITH_LABEL);
        // 彩色胶囊收拢
        assert_eq!(indicator.measured_size.width, 0.0);
        // ripple 恒定全尺寸且跟随动画位置
        assert_eq!(ripple.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(ripple.measured_size.height, NAVIGATION_RAIL_INDICATOR_HEIGHT);
        assert_eq!(
            ripple.position.x + ripple.measured_size.width / 2.0,
            indicator.position.x + indicator.measured_size.width / 2.0,
            "ripple 与胶囊中心对齐");
        assert_eq!(ripple.position.y, indicator.position.y);
    }

    #[test]
    fn no_label_item_gets_circular_full_size_pill() {
        // 无标签：vPadNoLabel=16 → 指示器 56x56（CornerFull 即圆形）
        let composer = compose_layout(
            NavigationRailItem::new(true, |ctx| icon_leaf(ctx, NAVIGATION_RAIL_ICON_SIZE))
                .on_click(|| {}),
        );
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let indicator = child(nodes, root, 0);
        // placeIcon：全部居中；胶囊 56x56
        assert_eq!(nodes[root].measured_size.height, NAVIGATION_RAIL_ITEM_HEIGHT);
        assert_eq!(indicator.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_RAIL_ITEM_HEIGHT);
    }

    #[test]
    fn bounded_item_hugs_content_instead_of_filling_width() {
        // androidx：Box(widthIn(min=80)) 内容居中——item 不横向填满约束
        let mut composer = Composer::new();
        composer.compose(|ctx| make_item(true, true).build(ctx));
        composer.layout(Constraints::new(0.0, 220.0, 0.0, NAVIGATION_RAIL_ITEM_HEIGHT));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.width, NAVIGATION_RAIL_WIDTH);
    }

    #[test]
    fn disabled_item_has_no_interaction_elements() {
        let composer = compose_layout(make_item(true, true).enabled(false));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert!(nodes[root].modifier.clickable_interaction().is_none());
        let ripple_node = child(nodes, root, 3);
        let indicator = child(nodes, root, 0);
        assert!(ripple_node.modifier.ripple_interaction().is_none(), "禁用项无状态层载体");
        assert!(indicator.modifier.ripple_interaction().is_none(), "禁用项指示器无 ripple");
    }

    #[test]
    fn unselected_item_hover_shows_state_layer_on_full_pill_rect() {
        let _serial = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = crate::theme::ThemeColors::default_light();
        let container = theme.surface;
        let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

        let source = MutableInteractionSource::new();
        let src_for_item = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationRail::new(move |ctx| {
                NavigationRailItem::new(false, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("B").build(ctx))
                    .interaction_source(src_for_item.clone())
                    .on_click(|| {})
                    .build(ctx);
            })
            .build(ctx)
        });
        composer.layout(Constraints::new(0.0, 120.0, 0.0, 400.0));

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
        let mut surface = skia_safe::surfaces::raster_n32_premul((120, 400)).expect("rail surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, surface.canvas());

        // 布局：rail 宽 120、item 宽 80 居中（起点 x=20）、顶部 padding 4。
        // 胶囊全局矩形 = x 32..88（item_x 20 + ripple_x 12），y 4..36；
        // 取 (40, 12)（胶囊内、避开图标 48..72）
        let px = pixel_at(&mut surface, 40, 12);
        assert_ne!(px, to_rgba(container), "悬浮未选中项应显示状态层");
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
    // ── WideNavigationRail ──

    #[test]
    fn wide_rail_width_endpoints_match_tokens() {
        // 收起：容器宽 96（CollapsedTokens.ContainerWidth）
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            assert!(!state.is_expanded(), "默认收起");
            WideNavigationRail::new(state, |ctx| {
                NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                    .label(|ctx| crate::components::Text::new("A").build(ctx))
                    .on_click(|| {})
                    .build(ctx);
            })
            .header(|ctx| icon_leaf(ctx, 24.0))
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.width, WIDE_RAIL_COLLAPSED_WIDTH);

        // Expanded, the width is the MINIMUM unless the content wants more — `ContainerWidthMaximum`
        // is a ceiling, not the target (`WideNavigationRail.kt:387-405`, and upstream's own
        // `rail_expanded_size` test expects `ContainerWidthMinimum` for exactly this shape of tree).
        // `wide_rail_expands_past_the_minimum_for_a_wide_header` below covers the other end.
        let build_expanded = |max_w: f32| {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let state = WideNavigationRailState::new(ctx);
                state.expand();
                WideNavigationRail::new(state, |ctx| {
                    NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                        .label(|ctx| crate::components::Text::new("A").build(ctx))
                        .on_click(|| {})
                        .build(ctx);
                })
                .header(|ctx| icon_leaf(ctx, 24.0))
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, max_w, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size.width
        };
        assert_eq!(build_expanded(400.0), WIDE_RAIL_EXPANDED_MIN_WIDTH);
        // A window narrower than the minimum gets what it has (the `.min(c.max_width)`), and a window
        // narrower still than the COLLAPSED width cannot collapse below it either.
        assert_eq!(build_expanded(200.0), 200.0);
        assert_eq!(build_expanded(90.0), 90.0);
    }

    /// RTL mirrors the wide rail's HEADER, and only the header.
    ///
    /// `WIDE_INDICATOR_H_PADDING` is a LEADING value: 16 is not "left", it is the leading edge, which
    /// under RTL is the right one. The header is the only child that moves — an item sits at
    /// `(width - w) / 2`, which is symmetric, so mirroring leaves it exactly where it was. That
    /// asymmetry is the whole reason `rtl` is captured at compose time and carried into the policy
    /// (placement runs after composition, where `direction::current()` is no longer the truth).
    ///
    /// Upstream resolves the same value through `Modifier.padding(start = 24.dp)` in its own sample
    /// and through `placeRelative` inside the layout; a leading inset is direction-relative in both.
    #[test]
    fn wide_rail_header_mirrors_under_rtl_and_items_do_not() {
        const RAIL_W: f32 = 400.0;
        const HEADER: f32 = 56.0;
        const ITEM: f32 = 96.0;

        // (header x, item x, rail width) for one direction.
        let geometry = |direction: crate::layout::LayoutDirection| {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme_and_direction(
                    crate::theme::ThemeColors::default_light(),
                    direction,
                    ctx,
                    |ctx| {
                        let state = WideNavigationRailState::new(ctx);
                        state.expand();
                        WideNavigationRail::new(state, |ctx| {
                            NavigationRailItem::new(true, |ctx| icon_leaf(ctx, ITEM))
                                .label(|ctx| crate::components::Text::new("A").build(ctx))
                                .on_click(|| {})
                                .build(ctx);
                        })
                        .header(|ctx| icon_leaf(ctx, HEADER))
                        .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, RAIL_W, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let rail_w = nodes[root].measured_size.width;
            // children[0] is the header slot, children[1] the first item.
            (
                nodes[nodes[root].children[0]].position.x,
                nodes[nodes[root].children[1]].position.x,
                rail_w,
            )
        };

        let (ltr_header, ltr_item, ltr_w) = geometry(crate::layout::LayoutDirection::Ltr);
        let (rtl_header, rtl_item, rtl_w) = geometry(crate::layout::LayoutDirection::Rtl);

        // The header is one 16dp leading inset in from the leading edge.
        assert_eq!(ltr_header, WIDE_INDICATOR_H_PADDING, "LTR header hugs the left edge");
        assert_eq!(
            rtl_header,
            ltr_w - WIDE_INDICATOR_H_PADDING - HEADER,
            "RTL header hugs the right edge"
        );
        // ...and the item does NOT mirror: its x is the same in both directions.
        assert_eq!(ltr_item, rtl_item, "an item is centred, so mirroring is a no-op");
        assert_eq!(ltr_w, rtl_w, "and the rail itself is the same width either way");
    }

    /// The other end of the same rule: the rail only travels past the minimum when an element is
    /// wider than it, and it stops at `ContainerWidthMaximum` however wide that element is. Upstream's
    /// `rail_expanded_maxSize` asserts the same with a 400dp header spacer.
    #[test]
    fn wide_rail_expands_past_the_minimum_for_a_wide_header() {
        let width_with_header = |header_w: f32| {
            let mut composer = Composer::new();
            composer.compose(move |ctx| {
                let state = WideNavigationRailState::new(ctx);
                state.expand();
                WideNavigationRail::new(state, |ctx| {
                    NavigationRailItem::new(true, |ctx| icon_leaf(ctx, 24.0))
                        .label(|ctx| crate::components::Text::new("A").build(ctx))
                        .on_click(|| {})
                        .build(ctx);
                })
                .header(move |ctx| icon_leaf(ctx, header_w))
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size.width
        };
        // A 100dp header is still narrower than the 220dp minimum, so the rail stays at 220.
        assert_eq!(width_with_header(100.0), WIDE_RAIL_EXPANDED_MIN_WIDTH);
        // A 300dp header pushes it to the header's own width...
        assert_eq!(width_with_header(300.0), 300.0);
        // ...and a 500dp one stops at the ceiling.
        assert_eq!(width_with_header(500.0), WIDE_RAIL_EXPANDED_MAX_WIDTH);
    }

    /// The wide rail's vertical rhythm changes with expansion, and BOTH halves of it are upstream
    /// tokens rather than constants:
    ///
    /// - the gap between items is `itemVerticalSpacedBy`, 4 collapsed and **0** expanded
    ///   (`WideNavigationRail.kt:279-283`) — an expanded item carries its own spacing in the pill;
    /// - the item's own floor is `itemMinHeight`, `TopIconItemMinHeight` (64) collapsed and
    ///   `minimumA11ySize` (48) expanded (`:284-289`).
    ///
    /// winia used 4 and 56 at both ends. This pins the four numbers by reading the first two items'
    /// placed positions, which is what the difference actually shows up as.
    #[test]
    fn wide_rail_item_gap_and_floor_follow_the_expansion() {
        // (first item y, second item y, first item height) for one expansion progress.
        //
        // The item's morph and the rail's gap are TWO separate progress reads: the rail policy owns
        // the gap, the item policy owns its own geometry, and winia does not push the rail's progress
        // into its items — the caller wires `.progress()` (as `navigation_rail_demo` does). So both
        // are driven from the same value here, which is what a caller has to do.
        let geometry = |progress: f32| {
            let mut composer = Composer::new();
            composer.compose(move |ctx| {
                let state = WideNavigationRailState::new(ctx);
                if progress > 0.5 {
                    state.expand();
                }
                WideNavigationRail::new(state, move |ctx| {
                    for name in ["A", "B"] {
                        WideNavigationRailItem::new(
                            false,
                            |ctx| icon_leaf(ctx, 24.0),
                            move |ctx| crate::components::Text::new(name).build(ctx),
                        )
                        .progress(State::new(progress))
                        .on_click(|| {})
                        .build(ctx);
                    }
                })
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let first = child(nodes, root, 0);
            let second = child(nodes, root, 1);
            (
                first.position.y,
                second.position.y,
                first.measured_size.height,
            )
        };

        let (y0, y1, h0) = geometry(0.0);
        // Collapsed: the item is `TopIconItemMinHeight` tall and the next one starts one 4dp gap later.
        assert_eq!(h0, WIDE_RAIL_ITEM_MIN_HEIGHT, "collapsed floor = TopIconItemMinHeight");
        assert_eq!(y1 - y0 - h0, RAIL_VERTICAL_PADDING, "collapsed gap = ItemVerticalSpace");

        let (y0e, y1e, h0e) = geometry(1.0);
        // Expanded: the pill is 56 tall and the items touch — no gap at all.
        assert_eq!(h0e, NAVIGATION_RAIL_ICON_SIZE + WIDE_START_INDICATOR_V_PADDING * 2.0);
        assert_eq!(y1e - y0e - h0e, WIDE_RAIL_ITEM_GAP_EXPANDED, "expanded gap = 0");
    }

    #[test]
    fn wide_rail_item_morphs_between_top_and_start_layouts() {
        // p=0（收起）：Top 布局——胶囊 56x32 在图标后、图标标签垂直堆叠
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::components::Text::new("Home").build(ctx),
            )
            .progress(State::new(0.0))
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let root_size = (nodes[root].measured_size.width, nodes[root].measured_size.height);
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        assert_eq!(indicator.measured_size.width, NAVIGATION_RAIL_INDICATOR_WIDTH);
        assert_eq!(indicator.measured_size.height, NAVIGATION_RAIL_INDICATOR_HEIGHT);
        // The collapsed floor is TopIconItemMinHeight = BaselineItem.ContainerHeight = 64 — the
        // token upstream names as "defaultHeight" in
        // item_topIconPosition_withLongLabel_automaticallyResizesHeight. The wide rail is NOT the
        // plain rail here: that one is 56.
        assert_eq!(root_size.1, WIDE_RAIL_ITEM_MIN_HEIGHT);
        // 图标在标签上方（垂直堆叠）
        assert!(icon.position.y < label.position.y);
        // 中心点对齐（居中排列——宽度不同 x 不同）
        assert_eq!(icon.position.x + icon.measured_size.width / 2.0, label.position.x + label.measured_size.width / 2.0);

        // p=1（展开）：Start 布局——胶囊包裹 [icon+gap+label]，高 56，水平排列
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::components::Text::new("Home").build(ctx),
            )
            .progress(State::new(1.0))
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let root_size = (nodes[root].measured_size.width, nodes[root].measured_size.height);
        let ripple = child(nodes, root, 3);
        let indicator = child(nodes, root, 0);
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        // 水平排列：label 在 icon 右侧、同垂直中心
        assert!(icon.position.x < label.position.x, "Start 布局图标在左");
        // The Start endpoint anchors at `WNRItemHorizontalPadding` (20) and puts the icon one
        // `FullWidthLeadingSpace` (16) further in, so the icon sits 36 from the leading edge
        // (`placeAnimatedLabelAndIcon`, `NavigationItem.kt:1143-1152`). Both x are FIXED — they must
        // not drift as the container widens (regression: a centred implementation pushed the icon
        // right by 20 during the expand animation).
        assert_eq!(indicator.position.x, WIDE_RAIL_ITEM_H_PADDING);
        assert_eq!(icon.position.x, WIDE_RAIL_ITEM_H_PADDING + WIDE_INDICATOR_H_PADDING);
        assert_eq!(
            label.position.x,
            WIDE_RAIL_ITEM_H_PADDING + WIDE_INDICATOR_H_PADDING + NAVIGATION_RAIL_ICON_SIZE + WIDE_START_ICON_LABEL_GAP,
        );
        assert_eq!(
            icon.position.y + icon.measured_size.height / 2.0,
            label.position.y + label.measured_size.height / 2.0,
        );
        // The Start pill is 56 tall: max(iconH, labelH) + 2 x `ItemStartIconIndicatorVerticalPadding`
        // (16) — `NavigationRailHorizontalItemTokens.ActiveIndicatorHeight`. It was 40 when the code
        // used the TOP endpoint's 4dp padding and 4dp gap for both ends.
        assert_eq!(ripple.measured_size.height, 56.0);
        // The expanded floor is minimumA11ySize (48); the content here is taller than that, so the
        // item settles at the pill own height.
        assert_eq!(
            root_size.1,
            56.0f32.max(WIDE_RAIL_ITEM_MIN_HEIGHT_EXPANDED),
            "expanded item height = the 56 pill"
        );
        let content_w = 24.0 + WIDE_START_ICON_LABEL_GAP + label.measured_size.width;
        assert_eq!(
            ripple.measured_size.width,
            content_w + WIDE_INDICATOR_H_PADDING * 2.0,
        );
        // 选中态彩色胶囊与 ripple 重合
        assert_eq!(indicator.position, ripple.position);
    }

    #[test]
    fn wide_rail_item_target_width_expands_when_expanded() {
        // 收起：拥抱内容 min 96；展开：填满轨宽（androidx item 为全宽命中目标）
        let cases: [(bool, f32); 2] = [(false, WIDE_RAIL_COLLAPSED_WIDTH), (true, 220.0)];
        for (expanded, expected_w) in cases {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let progress = ctx.animate_float_as_state(
                    if expanded { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
                        damping_ratio: 1.0, stiffness: 400.0, mass: 1.0, threshold: 0.01,
                    }),
                );
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("Home").build(ctx),
                )
                .progress(progress)
                .on_click(|| {})
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            assert_eq!(
                nodes[root].measured_size.width,
                expected_w,
                "expanded={}",
                expanded
            );
        }
    }

    /// icon_position 覆盖：progress=0（收起）但钉死 Start——布局应为横排位形
    /// （图标左 label 右、胶囊高 56 = HorizontalItem.ActiveIndicatorHeight），
    /// 不随容器变形进度插值
    #[test]
    fn wide_rail_item_icon_position_override_pins_start_layout() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WideNavigationRailItem::new(
                true,
                |ctx| icon_leaf(ctx, 24.0),
                |ctx| crate::components::Text::new("Home").build(ctx),
            )
            .progress(State::new(0.0))
            .icon_position(crate::components::navigation_bar::NavigationItemIconPosition::Start)
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 220.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let icon = child(nodes, root, 1);
        let label = child(nodes, root, 2);
        let ripple = child(nodes, root, 3);
        assert!(icon.position.x < label.position.x, "钉死 Start：图标在 label 左");
        assert_eq!(icon.position.y + 12.0, label.position.y + label.measured_size.height / 2.0,
            "同垂直中心");
        assert_eq!(
            ripple.measured_size.height,
            NAVIGATION_RAIL_ICON_SIZE + WIDE_START_INDICATOR_V_PADDING * 2.0,
            "Start pill = HorizontalItem.ActiveIndicatorHeight"
        );
    }

    /// 三态语义：target 立即翻转 / current 跟随进度收敛侧 / is_animating 中段为真
    #[test]
    fn wide_rail_state_target_current_is_animating() {
        let mut composer = Composer::new();
        let mut state_ref = None;
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state_ref = Some(state);
        });
        let state = state_ref.unwrap();
        assert!(!state.target() && !state.current() && !state.is_animating(), "初始全收起");

        state.expand();
        assert!(state.target(), "target 立即翻转");
        assert!(state.current(), "进度未挂载时 current 退化为 target");
        assert!(!state.is_animating());

        // 模拟组件挂载进度 + 动画中间帧
        let progress = State::new(0.5);
        state.attach_progress(progress.clone());
        assert!(state.is_animating(), "进度 0.5 → 动画中");
        assert!(!state.current(), "进度 0.5 → 视觉仍收起");

        progress.set(1.0);
        assert!(state.current(), "进度 1.0 → 视觉展开");
        assert!(!state.is_animating(), "进度收敛 → 动画结束");

        state.collapse();
        assert!(!state.target() && state.current(), "target 翻转、current 尚未跟随");
    }

    #[test]
    fn wide_rail_state_toggles_expansion() {
        let mut composer = Composer::new();
        let mut state_ref = None;
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state.toggle();
            state_ref = Some(state);
        });
        let state = state_ref.unwrap();
        assert!(state.is_expanded(), "toggle 后应展开");
        state.collapse();
        assert!(!state.is_expanded());
        state.expand();
        assert!(state.is_expanded());
    }

    /// The modal form: collapsed it is an ordinary rail and registers no overlay; expanded it raises a
    /// modal overlay, and that `modal` flag is what draws the scrim (upstream opens a
    /// `ModalWideNavigationRailDialog` for the same reason).
    #[test]
    fn modal_rail_is_a_plain_rail_until_it_expands() {
        // Collapsed: no overlay, and the rail is on screen at the collapsed width.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            ModalWideNavigationRail::new(state, |ctx| {
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("A").build(ctx),
                )
                .on_click(|| {})
                .build(ctx);
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert!(
            composer.take_overlays().is_empty(),
            "a collapsed modal rail opens no overlay"
        );
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(
            composer.arena_nodes()[root].measured_size.width,
            WIDE_RAIL_COLLAPSED_WIDTH,
            "and what is on screen is the collapsed rail"
        );

        // Expanded: exactly one overlay, and it is modal — that flag is the scrim.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            state.expand();
            ModalWideNavigationRail::new(state, |ctx| {
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("A").build(ctx),
                )
                .on_click(|| {})
                .build(ctx);
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "an expanded modal rail opens one overlay");
        assert!(
            overlays[0].modal,
            "and it is modal: that flag is what draws the scrim"
        );
        // `TopStart`, NOT `Center`, and not `TopLeft` either: `Center` positions the overlay by its own
        // measured size in the middle of the window (which is where the container used to land), `TopStart`
        // resolves against the direction the way `Alignment.TopStart` does, and `TopLeft` is an absolute
        // corner that would not mirror under RTL.
        assert_eq!(
            overlays[0].position,
            PopupPosition::TopStart,
            "the modal container hangs off the leading edge"
        );
    }

    /// Expanding the modal rail must NOT move the page: upstream keeps the collapsed rail in the layout
    /// while the panel is up — "Keep this rail even when expanded so that screen layout doesn't
    /// change" (`WideNavigationRail.kt:570-571`) — and composes its content only while collapsed
    /// (`:585-590`).
    ///
    /// winia used to drop the whole node on expansion, which took a 96dp column out of the caller's
    /// layout and pulled every column after it to the left. This pins the sibling's x.
    ///
    /// `hide_on_collapse` is the opposite rule and stays the opposite rule: there is no in-flow rail
    /// in either state, so the caller owns the container.
    #[test]
    fn expanding_the_modal_rail_does_not_move_its_siblings() {
        // (rail column width, follower x) for one expansion state.
        let geometry = |expand: bool, hide_on_collapse: bool| {
            let mut composer = Composer::new();
            composer.compose(move |ctx| {
                crate::layout::components::Row::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, |ctx| {
                    let state = WideNavigationRailState::new(ctx);
                    if expand {
                        state.expand();
                    }
                    ModalWideNavigationRail::new(state, |ctx| {
                        WideNavigationRailItem::new(
                            true,
                            |ctx| icon_leaf(ctx, 24.0),
                            |ctx| crate::components::Text::new("A").build(ctx),
                        )
                        .on_click(|| {})
                        .build(ctx);
                    })
                    .hide_on_collapse(hide_on_collapse)
                    .build(ctx);
                    // The follower: its x is what "the page moved" would show up as.
                    let key = ctx.next_key();
                    ctx.start_leaf(key, Modifier::new().size(100.0, 50.0));
                    ctx.end_node();
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let children = nodes[root].children.clone();
            // With `hide_on_collapse` there is no rail node at all, so the follower is the first
            // child; otherwise it follows the rail.
            if children.len() == 1 {
                (0.0, nodes[children[0]].position.x)
            } else {
                (
                    nodes[children[0]].measured_size.width,
                    nodes[children[1]].position.x,
                )
            }
        };

        let (w_collapsed, x_collapsed) = geometry(false, false);
        let (w_expanded, x_expanded) = geometry(true, false);
        assert_eq!(w_collapsed, WIDE_RAIL_COLLAPSED_WIDTH, "collapsed rail column");
        assert_eq!(
            w_expanded, WIDE_RAIL_COLLAPSED_WIDTH,
            "the in-flow rail stays collapsed-sized while the panel is up"
        );
        assert_eq!(
            x_collapsed, x_expanded,
            "and the column after it does not move"
        );

        // The dismissible variant is the other rule: no in-flow rail at all, in either state.
        let (w_hidden_collapsed, _) = geometry(false, true);
        let (w_hidden_expanded, _) = geometry(true, true);
        assert_eq!(w_hidden_collapsed, 0.0, "hide_on_collapse draws nothing while collapsed");
        assert_eq!(w_hidden_expanded, 0.0, "and nothing while expanded");
    }

    /// `expanded_header_top_padding` (upstream's parameter of the same name) moves the expanded panel's
    /// content down by exactly that much, and leaves the collapsed rail alone.
    ///
    /// That is what upstream documents the parameter for: "It's usually needed in order to align the
    /// content of the rail between the collapsed and expanded animation"
    /// (`WideNavigationRail.kt:508-510`). The first item is the one thing that CAN line up between the
    /// two states — their pitches differ (68 collapsed against 56 expanded), so the items after it
    /// cannot.
    #[test]
    fn expanded_header_top_padding_moves_the_panel_content_only() {
        // (collapsed first-item y, expanded first-item y) for one padding value.
        let first_item_ys = |padding: f32| {
            // Collapsed: the padding must not reach this branch at all.
            let mut collapsed = Composer::new();
            collapsed.compose(|ctx| {
                ModalWideNavigationRail::new(WideNavigationRailState::new(ctx), |ctx| {
                    WideNavigationRailItem::new(
                        true,
                        |ctx| icon_leaf(ctx, 24.0),
                        |ctx| crate::components::Text::new("A").build(ctx),
                    )
                    .on_click(|| {})
                    .build(ctx);
                })
                .header(|ctx| icon_leaf(ctx, 24.0))
                .expanded_header_top_padding(padding)
                .build(ctx);
            });
            collapsed.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            let croot = collapsed.layout_root_idx().unwrap();
            let cnodes = collapsed.arena_nodes();
            let collapsed_y = cnodes[cnodes[croot].children[1]].position.y;

            // Expanded: the panel is an overlay, so it has to be measured through the descriptor.
            let mut expanded = Composer::new();
            expanded.compose(|ctx| {
                let state = WideNavigationRailState::new(ctx);
                state.expand();
                ModalWideNavigationRail::new(state, |ctx| {
                    WideNavigationRailItem::new(
                        true,
                        |ctx| icon_leaf(ctx, 24.0),
                        |ctx| crate::components::Text::new("A").build(ctx),
                    )
                    .on_click(|| {})
                    .build(ctx);
                })
                .header(|ctx| icon_leaf(ctx, 24.0))
                .expanded_header_top_padding(padding)
                .build(ctx);
            });
            expanded.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            let overlays = expanded.take_overlays();
            assert_eq!(overlays.len(), 1, "an expanded modal rail opens one overlay");
            // Compose the overlay's own tree so its policy runs, then read the first item's y.
            let content = &overlays[0].content;
            let mut panel = Composer::new();
            panel.compose(|ctx| content(ctx));
            panel.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            let proot = panel.layout_root_idx().unwrap();
            let pnodes = panel.arena_nodes();
            let expanded_y = pnodes[pnodes[proot].children[1]].position.y;

            (collapsed_y, expanded_y)
        };

        let (collapsed_y, base_y) = first_item_ys(0.0);
        let (collapsed_y_padded, padded_y) = first_item_ys(64.0);

        // The collapsed rail is untouched by the parameter...
        assert_eq!(
            collapsed_y, collapsed_y_padded,
            "the padding belongs to the expanded panel only"
        );
        // ...and the expanded panel's first item moves down by exactly the padding.
        assert_eq!(
            padded_y - base_y,
            64.0,
            "the panel's first item moves down by the padding"
        );
    }

    /// `hide_on_collapse` (upstream's dismissible variant): collapsed, the rail is not drawn at all — the
    /// caller owns its offscreen position — but expanding still raises the modal layer.
    #[test]
    fn hide_on_collapse_draws_nothing_until_expanded() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let state = WideNavigationRailState::new(ctx);
            ModalWideNavigationRail::new(state, |ctx| {
                WideNavigationRailItem::new(
                    true,
                    |ctx| icon_leaf(ctx, 24.0),
                    |ctx| crate::components::Text::new("A").build(ctx),
                )
                .on_click(|| {})
                .build(ctx);
            })
            .hide_on_collapse(true)
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let widths: Vec<f32> = composer
            .arena_nodes()
            .iter()
            .map(|n| n.measured_size.width)
            .collect();
        assert!(
            !widths.contains(&WIDE_RAIL_COLLAPSED_WIDTH),
            "collapsed with hide_on_collapse, no rail is drawn: {widths:?}"
        );
        assert!(composer.take_overlays().is_empty(), "and no overlay either");
    }
