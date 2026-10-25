    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::constraints::Constraints;
    use crate::layout::components::Row;
    use crate::components::text::Text;

    /// The indicator runs on M3's `DefaultSpatial` token (`TabRow.kt:406`), whose damping 0.9 is what
    /// gives the slide its slight overshoot; a no-bouncy approximation (or the 0.6 this used) fails here.
    #[test]
    fn the_indicator_runs_on_m3s_default_spatial_spring() {
        match indicator_spring() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_default());
                assert_eq!(spec.damping_ratio, 0.9);
                assert_eq!(spec.stiffness, 700.0);
            }
            other => panic!("the indicator must be a spring, got {other:?}"),
        }
    }

    /// `TabTransition`'s direction rule, pinned: arriving uses the slower default spring, leaving the
    /// faster one. One spring for both directions (or a tween) fails here.
    #[test]
    fn the_content_colour_spec_follows_the_tab_transition_direction() {
        let arriving = tab_colour_spec(true);
        assert_eq!(arriving, crate::animation::SpringSpec::effects_default());
        assert_eq!(arriving.stiffness, 1600.0);
        let leaving = tab_colour_spec(false);
        assert_eq!(leaving, crate::animation::SpringSpec::effects_fast());
        assert_eq!(leaving.stiffness, 3800.0);
    }

    fn tab_row_layout(
        selected: usize,
        follow_content: bool,
        count: usize,
        direction: LayoutDirection,
    ) -> Composer {
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, direction, ctx, |ctx| {
                let mut row = TabRow::new(selected, move |ctx| {
                    for i in 0..count {
                        let label = format!("Tab {}", i);
                        Tab::new(i == selected, || {})
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                });
                if !follow_content {
                    row = row.secondary();
                }
                row.build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        c
    }

    #[test]
    fn tab_row_creates_dividers_and_indicator() {
        let c = tab_row_layout(0, true, 3, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [tab0, tab1, tab2, divider, indicator]
        assert_eq!(children.len(), 5, "should have 3 tabs + divider + indicator");
        // divider is at index 3 (1dp height)
        let div = &nodes[children[3]];
        assert_eq!(div.measured_size.height, 1.0);
        // indicator is at index 4
        let ind = &nodes[children[4]];
        assert_eq!(ind.measured_size.height, ACTIVE_INDICATOR_HEIGHT);
    }

    /// The caller's indicator is composed DURING measurement and sees the positions the row just
    /// computed — the reason Compose needs `SubcomposeLayout` for this slot, and the thing this branch's
    /// `layout::subcompose` facility now provides. The positions are measure-time data: a caller cannot
    /// know them while composing, which is what kept this API out of winia before.
    #[test]
    fn a_custom_indicator_is_composed_with_the_measured_positions() {
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Vec<(usize, TabPosition)>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        let mut composer = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        composer.compose(move |ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(1, |ctx| {
                    for i in 0..3 {
                        let label = format!("Tab {i}");
                        Tab::new(i == 1, || {})
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                })
                .indicator(move |ctx, scope| {
                    let selected = scope.selected_position().copied();
                    seen_in
                        .lock()
                        .unwrap()
                        .push((scope.tab_positions().len(), selected.unwrap_or(TabPosition::new(0.0, 0.0, 0.0))));
                    // What a caller draws: the selected tab's own width, centred on it.
                    let pos = scope.selected_position().copied().unwrap_or(TabPosition::new(0.0, 0.0, 0.0));
                    crate::layout::components::Spacer::vertical(ACTIVE_INDICATOR_HEIGHT)
                        .modifier(
                            crate::modifier::Modifier::new()
                                .fill_max_width()
                                .padding_vertical(0.0)
                                .offset(pos.left + (pos.width - pos.content_width) / 2.0, 0.0),
                        )
                        .build(ctx);
                })
                .build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let seen = seen.lock().unwrap().clone();
        assert!(!seen.is_empty(), "the indicator slot ran during the measurement");
        let (count, selected) = seen[0];
        assert_eq!(count, 3, "every tab's position was handed to the indicator");
        // 360 / 3 = 120 per tab, and tab 1 starts at 120.
        assert_eq!(
            (selected.left, selected.width),
            (120.0, 120.0),
            "the selected tab's position is the one the row measured: {selected:?}"
        );
        assert!(selected.content_width > 0.0, "and its content width is real: {selected:?}");
    }

    /// Supplying an indicator REPLACES the row's own bar (Compose's `indicator` parameter does the
    /// same). The default bar's node still exists — the children list is [tabs.., divider, indicator] —
    /// so "replaced" has to mean its geometry is zeroed: a node left without a placement would keep the
    /// previous frame's box and paint over whatever the caller drew.
    #[test]
    fn a_custom_indicator_replaces_the_default_bar() {
        let mut composer = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        composer.compose(move |ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |ctx| {
                    for i in 0..3 {
                        let label = format!("Tab {i}");
                        Tab::new(i == 0, || {})
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                })
                .indicator(|_ctx, _scope| {
                    // Deliberately emits nothing: the caller may draw through a modifier instead.
                })
                .build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = &nodes[root].children;
        let bar = &nodes[*children.last().unwrap()];
        assert_eq!(
            (bar.measured_size.width, bar.measured_size.height),
            (0.0, 0.0),
            "the default bar is zeroed when the caller supplies an indicator"
        );
    }

    /// The scrollable variant takes the same slot, and its positions are the ones its own tabs were
    /// placed with — the caller's indicator scrolls with them (Compose's scrollable indicator does too).
    #[test]
    fn a_scrollable_row_composes_a_custom_indicator_with_its_positions() {
        use std::sync::{Arc, Mutex};

        let seen: Arc<Mutex<Vec<TabPosition>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        let state = crate::modifier::ScrollState::new();
        let st_outer = state.clone();
        let st_inner = state.clone();
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(move |ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                ScrollableTabRow::new(1, move |ctx| {
                    for i in 0..4 {
                        let label = format!("Tab {i}");
                        let st = st_inner.clone();
                        Tab::new(i == 1, move || { let _ = st; })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                })
                .scroll_state(st_outer)
                .indicator(move |_ctx, scope| {
                    if let Some(pos) = scope.selected_position() {
                        seen_in.lock().unwrap().push(*pos);
                    }
                })
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 640.0));

        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let seen = seen.lock().unwrap().clone();
        assert!(!seen.is_empty(), "the scrollable row composed its indicator slot");
        // The scope's selected position is the row's own placement for tab 1.
        let tab1 = &nodes[children[1]];
        let selected = seen[0];
        assert!(
            (selected.left - tab1.position.x).abs() < 0.01,
            "the position handed to the indicator is the tab's own: {selected:?} vs x={}",
            tab1.position.x
        );
        assert!(
            (selected.width - tab1.measured_size.width).abs() < 0.01,
            "and its width matches: {selected:?} vs {}",
            tab1.measured_size.width
        );
    }

    #[test]
    fn tab_row_tabs_equal_width() {
        let c = tab_row_layout(1, true, 4, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // tab0 = children[0], tab1 = children[1], tab2 = children[2], tab3 = children[3]
        for i in 0..4 {
            assert_eq!(nodes[children[i]].measured_size.width, 90.0, "tab {} width", i);
        }
    }

    #[test]
    fn tab_row_primary_indicator_position() {
        let c = tab_row_layout(1, true, 3, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[4]];
        // selected=1, tabWidth=120.0, contentWidth≈33 (natural≈65-32)
        // 居中后: offset = 120 + (120-33)/2 = 163.5
        let tab_width = 120.0;
        let content_width = 33.0; // natural width - 32, min 24
        let expected_x = tab_width + (tab_width - content_width) / 2.0;
        assert!((ind.position.x - expected_x).abs() < 1.0, "indicator x={} expected≈{}", ind.position.x, expected_x);
        assert!(ind.measured_size.width >= 24.0, "indicator content width min 24");
        assert!(ind.measured_size.width < 120.0, "indicator content width < full tab");
    }

    #[test]
    fn tab_row_secondary_indicator_width() {
        let c = tab_row_layout(2, false, 4, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[5]];
        // selected=2, tabWidth=90.0, target_width=tabWidth (full)
        assert_eq!(ind.position.x, 180.0, "indicator offset for tab 2");
        assert_eq!(ind.measured_size.width, 90.0, "secondary indicator full width");
    }

    /// A fixed row prices its tabs through the intrinsic protocol rather than by measuring them. A
    /// tab whose label is `weight(1f)` MEASURES as wide as its whole slot (the weight eats the
    /// bounded space) but ANSWERS the intrinsic query with the label's own width, because Row's
    /// intrinsic block prices a weighted child by itself (`RowColumnImpl.kt:371-394`). Compose's fixed
    /// row asks `maxIntrinsicWidth(tabRowHeight)` for the indicator's content width
    /// (`TabRow.kt:456-459`), so a short weighted label floors the indicator at its 24dp touch target
    /// while a measurement-derived width would be 120 - 32 = 88.
    #[test]
    fn a_tab_row_prices_a_weighted_label_through_the_intrinsics() {
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |ctx| {
                    for _ in 0..3 {
                        Row::new().build(ctx, |ctx| {
                            Text::new("Tab")
                                .modifier(Modifier::new().layout_weight(1.0))
                                .build(ctx);
                        });
                    }
                })
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[4]];
        assert!(
            (ind.measured_size.width - MIN_INDICATOR_WIDTH).abs() < 0.5,
            "indicator width {} should come from the label's own intrinsic width (floored at {}), \
             not from the slot the weighted label measures into",
            ind.measured_size.width,
            MIN_INDICATOR_WIDTH
        );
    }

    /// Compose clamps a tab's content width into its slot: `min(maxIntrinsicWidth(tabRowHeight),
    /// tabWidth)` (`TabRow.kt:456-458`). A tab that refuses the incoming width (`required_width`) must
    /// therefore not widen the indicator past the slot it sits in — with tabWidth 120 the content box
    /// is 120 - 32 = 88.
    #[test]
    fn a_tab_row_clamps_a_wide_tabs_content_width_into_its_slot() {
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |ctx| {
                    for _ in 0..3 {
                        Row::new()
                            .modifier(Modifier::new().required_width(200.0))
                            .build(ctx, |ctx| {
                                Text::new("Tab").build(ctx);
                            });
                    }
                })
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[4]];
        assert!(
            (ind.measured_size.width - 88.0).abs() < 0.5,
            "indicator width {} should be the slot width minus the horizontal padding (88)",
            ind.measured_size.width
        );
    }

    #[test]
    fn tab_row_rtl_mirror() {
        let ltr = tab_row_layout(0, true, 2, LayoutDirection::Ltr);
        let rtl = tab_row_layout(0, true, 2, LayoutDirection::Rtl);
        let ltr_root = ltr.layout_root_idx().unwrap();
        let rtl_root = rtl.layout_root_idx().unwrap();
        let ltr_nodes = ltr.arena_nodes();
        let rtl_nodes = rtl.arena_nodes();
        let ltr_children = &ltr_nodes[ltr_root].children;
        let rtl_children = &rtl_nodes[rtl_root].children;

        // LTR: tab0 at x=0, tab1 at x=180
        assert_eq!(ltr_nodes[ltr_children[0]].position.x, 0.0);
        assert_eq!(ltr_nodes[ltr_children[1]].position.x, 180.0);
        // RTL: tab0 at x=180, tab1 at x=0
        assert_eq!(rtl_nodes[rtl_children[0]].position.x, 180.0);
        assert_eq!(rtl_nodes[rtl_children[1]].position.x, 0.0);
    }

    #[test]
    fn tab_row_0_tabs_does_not_panic() {
        let mut c = Composer::new();
        c.compose(|ctx| {
            let colors = crate::theme::ThemeColors::default_light();
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |_| {}).build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        // should not panic
    }

    #[test]
    fn tab_creates_content() {
        let mut c = Composer::new();
        c.compose(|ctx| {
            Tab::new(true, || {})
                .text(|ctx| Text::new("Tab").build(ctx))
                .build(ctx);
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        // Tab should have at least text + ripple
        assert!(nodes[root].children.len() >= 2, "Tab should have text + ripple children");
    }

    #[test]
    fn tab_leading_icon_lays_out_horizontally() {
        // LeadingIconTab：icon 左 + 8dp + text 右（水平排列），整体水平居中
        use crate::components::icon::{Icon};
        use crate::graphics::{IconSource};
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                Tab::new(true, || {})
                    .leading_icon()
                    .modifier(Modifier::new().size(180.0, 48.0))
                    .icon(|ctx| {
                        Icon::new(IconSource::svg(
                            r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="m354-287 126-76 126 77-33-144 111-96-146-13-58-136-58 135-146 13 111 97-33 143ZM233-120l65-281L80-590l288-25 112-265 112 265 288 25-218 189 65 281-247-149-247 149Zm247-350Z"/></svg>"#,
                        ))
                        .size(24.0)
                        .build(ctx);
                    })
                    .text(|ctx| Text::new("Favorites").build(ctx))
                    .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [icon, text, ripple]
        assert_eq!(children.len(), 3, "leading tab = icon + text + ripple");
        let icon = &nodes[children[0]];
        let text = &nodes[children[1]];
        let ripple = &nodes[children[2]];
        // icon 在 text 左侧（非 leading 是 icon 上 text 下）
        assert!(icon.position.x < text.position.x, "icon.x={} 应 < text.x={}", icon.position.x, text.position.x);
        // 垂直方向对齐：各自在 slot 内垂直居中（icon 24px → y=12，text 20px →
        // y=14——差异来自尺寸不同）。关键判据：text 顶不落于 icon 底之下
        // （非 leading 竖排时 text.y 远大于 icon.y + icon.h）
        assert!(text.position.y < icon.position.y + icon.measured_size.height + 2.0,
            "leading icon/text 应同一水平带：icon.y={} h={} text.y={}",
            icon.position.y, icon.measured_size.height, text.position.y);
        // 间距 = LEADING_ICON_TEXT_SPACING（icon 右缘到 text 左缘）
        let gap = text.position.x - (icon.position.x + icon.measured_size.width);
        assert!((gap - LEADING_ICON_TEXT_SPACING).abs() < 1.0, "icon-text 间距应=8，实际 {gap}");
        // 整组水平居中 + ripple 覆盖全 tab
        assert!(icon.position.x > 0.0, "leading 内容应居中（icon.x={}）", icon.position.x);
        assert_eq!(ripple.measured_size.width, nodes[root].measured_size.width);
    }

    #[test]
    fn tab_leading_icon_rtl_mirrors_icon_to_right() {
        // RTL 回归：LeadingIconTab 在 RTL 下 icon 应移右侧、text 移左侧
        //（用户实测：Tab Three 切 RTL 后不镜像——Tab 内部布局此前不感知方向）
        use crate::components::icon::{Icon};
        use crate::graphics::{IconSource};
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Rtl, ctx, |ctx| {
                Tab::new(true, || {})
                    .leading_icon()
                    .modifier(Modifier::new().size(180.0, 48.0))
                    .icon(|ctx| {
                        Icon::new(IconSource::svg(
                            r#"<svg xmlns="http://www.w3.org/2000/svg" height="24" viewBox="0 -960 960 960" width="24"><path d="m354-287 126-76 126 77-33-144 111-96-146-13-58-136-58 135-146 13 111 97-33 143Z"/></svg>"#,
                        ))
                        .size(24.0)
                        .build(ctx);
                    })
                    .text(|ctx| Text::new("Favorites").build(ctx))
                    .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let icon = &nodes[children[0]];
        let text = &nodes[children[1]];
        // RTL：icon 在 text 右侧（LTR 是 icon 在左）
        assert!(icon.position.x > text.position.x,
            "RTL leading icon.x={} 应 > text.x={}（icon 移右）",
            icon.position.x, text.position.x);
        // 间距保持 8dp（text 右缘到 icon 左缘）
        let gap = icon.position.x - (text.position.x + text.measured_size.width);
        assert!((gap - LEADING_ICON_TEXT_SPACING).abs() < 1.0, "RTL icon-text 间距应=8，实际 {gap}");
        // 同一水平带
        assert!(text.position.y < icon.position.y + icon.measured_size.height + 2.0);
    }

    #[test]
    fn tab_accepts_injected_interaction_source() {
        // interaction_source 注入：外部 remember 创建的源应被使用
        //（不 panic，渲染结构同内部创建路径）
        use crate::input::interaction::MutableInteractionSource;
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        let src: MutableInteractionSource = {
            let mut out = None;
            c.compose(|ctx| {
                let s = ctx.remember(|| MutableInteractionSource::new()).get();
                out = Some(s);
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                    let s = out.clone().unwrap();
                    Tab::new(true, || {})
                        .interaction_source(s)
                        .text(|ctx| Text::new("Tab").build(ctx))
                        .build(ctx);
                });
            });
            out.unwrap()
        };
        c.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        assert!(nodes[root].children.len() >= 2, "injected-source tab should build");
        let _ = src;
    }

    #[test]
    fn tab_content_version_ripple_covers_full_tab() {
        // P0-1 回归：自定义 content 版 ripple 必须全尺寸覆盖 tab slot
        // （BoxLayout(Center) 测子节点用 loosen——无尺寸则塌缩 0×0）
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                TabRow::new(0, |ctx| {
                    Tab::new(true, || {})
                        .content(|ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, Modifier::new().size(40.0, 20.0));
                            ctx.end_node();
                        })
                        .build(ctx);
                })
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [tab0, divider, indicator]
        let tab = &nodes[children[0]];
        let tab_children = &tab.children;
        // tab children = [content, ripple]
        assert_eq!(tab_children.len(), 2, "content 版应有 content + ripple");
        let ripple = &nodes[tab_children[1]];
        // content 版无 spec 高——tab 高度 = 内容高（20），宽度 = 整行（1 tab → 360）
        assert_eq!(ripple.measured_size.width, 360.0, "ripple 应全宽覆盖 tab slot");
        assert_eq!(ripple.measured_size.height, 20.0, "ripple 应全高覆盖 tab slot");
    }

    #[test]
    fn tab_row_selected_out_of_range_falls_back_to_origin() {
        // selected >= tab_count 时指示条归位 (0,0)，不 panic
        let c = tab_row_layout(99, true, 2, LayoutDirection::Ltr);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[3]]; // [tab0, tab1, divider, indicator]
        assert_eq!(ind.position.x, 0.0, "越界 selected 指示条 x=0");
        assert_eq!(ind.measured_size.width, 0.0, "越界 selected 指示条宽 0");
    }

    /// An intrinsic query above the row must not run the row's measure.
    ///
    /// `TabRowLayoutPolicy::measure` derives the indicator's target from `constraints.max_width` and
    /// starts a spring toward it, and the first call consumes the row's `initialized` flag. The trait's
    /// default answers an intrinsic by running exactly that measure, with the queried axis UNBOUNDED
    /// (`node.rs:1003-1051` — Compose's own default does the same, which is why Compose's animating
    /// nodes extend `LayoutModifierNodeWithPassThroughIntrinsics`, `AnimationModifier.kt:259-280`). An
    /// ancestor asking its children's width intrinsics therefore used to hand the row
    /// `max_width = f32::MAX`: the indicator's target became ~6e37 and the row was placed with it.
    #[test]
    fn an_intrinsic_query_above_a_tab_row_does_not_move_the_indicator() {
        use std::time::Duration;
        // Animation state is mutated by the row's own measure: hold the shared serial lock.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(move |ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                let key = ctx.next_key();
                match ctx.start_restartable_group(
                    key,
                    Modifier::new().width(crate::layout::IntrinsicSize::Max),
                    crate::layout::column::ColumnLayout::default(),
                ) {
                    GroupStatus::Enter => {
                        TabRow::new(0, |ctx| {
                            for i in 0..3 {
                                let label = format!("Tab {i}");
                                Tab::new(i == 0, || {})
                                    .text(move |ctx| Text::new(&label).build(ctx))
                                    .build(ctx);
                            }
                        })
                        .build(ctx);
                    }
                    GroupStatus::Skip => {}
                }
                ctx.end_restartable_group();
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        // The row's FIRST measurement snaps the indicator onto the selected tab. A probe that ran that
        // measurement for the row consumed the flag, so the first real layout instead started a spring
        // from zero — the bar slid in from the row's left edge.
        let first = indicator_x(&c);
        assert!(
            first > 0.0 && first < 60.0,
            "the first layout snaps the indicator onto tab 0, x={first}"
        );
        for _ in 0..60 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        }
        let x = indicator_x(&c);
        assert!(
            x >= 0.0 && x <= 360.0,
            "an intrinsic query must leave the indicator where the row's own layout puts it, x={x}"
        );
    }

    /// The scrollable row answers the same query. Its own arithmetic ignores the incoming constraints
    /// (it lays the tabs out at their natural widths), so the default's `measure` happens to place the
    /// bar in the same spot — what it does change is that the row now owns an animation it never asked
    /// for: the probe's measurement consumes `initialized`, so the row's first real measure takes the
    /// animating branch. An untouched first layout has nothing to animate.
    #[test]
    fn an_intrinsic_query_above_a_scrollable_tab_row_does_not_move_the_indicator() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let plain = scrollable_row_first_layout(false);
        let probed = scrollable_row_first_layout(true);
        assert_eq!(
            probed.0, plain.0,
            "an intrinsic query must not change the row's first layout (probed {}, plain {})",
            probed.0, plain.0
        );
        assert!(plain.0 > 0.0, "the row places the indicator inside itself, x={}", plain.0);
        assert!(!plain.1, "an untouched first layout registers no animation");
        assert!(
            !probed.1,
            "an intrinsic query must not make the row animate on its first layout"
        );
    }

    /// One layout of the same six-tab scrollable row, under a plain column or under one sized to
    /// `IntrinsicSize::Max` (which asks its child the intrinsics before measuring it). Answers the
    /// indicator's x and whether the layout registered an animation.
    fn scrollable_row_first_layout(probe: bool) -> (f32, bool) {
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        c.compose(move |ctx| {
            WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                let modifier = if probe {
                    Modifier::new().width(crate::layout::IntrinsicSize::Max)
                } else {
                    Modifier::new()
                };
                let key = ctx.next_key();
                match ctx.start_restartable_group(
                    key,
                    modifier,
                    crate::layout::column::ColumnLayout::default(),
                ) {
                    GroupStatus::Enter => {
                        ScrollableTabRow::new(0, |ctx| {
                            for i in 0..6 {
                                let label = format!("Tab {i}");
                                Tab::new(i == 0, || {})
                                    .text(move |ctx| Text::new(&label).build(ctx))
                                    .build(ctx);
                            }
                        })
                        .build(ctx);
                    }
                    GroupStatus::Skip => {}
                }
                ctx.end_restartable_group();
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        (indicator_x(&c), crate::animation::update_animations())
    }

    /// The row's own indicator node, found by the divider + bar pair it places last.
    fn indicator_x(c: &Composer) -> f32 {
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            let kids = nodes[n].children.clone();
            if let [.., divider, indicator] = kids[..] {
                if nodes[divider].measured_size.height == 1.0
                    && nodes[indicator].measured_size.height == ACTIVE_INDICATOR_HEIGHT
                {
                    return nodes[indicator].position.x;
                }
            }
            stack.extend(kids);
        }
        panic!("no indicator in the tree");
    }

    #[test]
    fn tab_row_indicator_animates_on_selected_change() {
        // 动画推进：selected 0→1 → 步进 update_animations → offset 单调变化并收敛到新 target
        use crate::runtime::state::State;
        use std::time::{Duration, Instant};
        // 动画注册表全局共享——持串行锁防并行 clear/竞态（与 lazy/animated_size 同）
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        fn make_scene(
            selected: State<usize>,
            colors: crate::theme::ThemeColors,
        ) -> impl FnOnce(&mut ComposeCtx) {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                    let sel = selected.clone();
                    TabRow::new(sel.get(), move |ctx| {
                        for i in 0..3 {
                            let label = format!("Tab {}", i);
                            let sel2 = selected.clone();
                            Tab::new(sel2.get() == i, move || {})
                                .text(move |ctx| Text::new(&label).build(ctx))
                                .build(ctx);
                        }
                    })
                    .build(ctx);
                });
            }
        }

        let selected = State::new(0usize);
        let colors = crate::theme::ThemeColors::default_light();

        let mut c = Composer::new();
        c.compose(make_scene(selected.clone(), colors.clone()));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        // 首帧：initialized=false → Reactive set 直接到位（tab0，居中 ≈ 43.5）
        {
            let root = c.layout_root_idx().unwrap();
            let nodes = c.arena_nodes();
            let children = &nodes[root].children;
            let ind = &nodes[children[4]]; // [tab0..2, divider, indicator]
            let x0 = ind.position.x;
            assert!(x0 > 0.0 && x0 < 120.0, "首帧指示条应在 tab0 内，x={x0}");
        }

        // 切到 tab1：recompose + layout → push_animatable 分支启动动画
        selected.set(1);
        assert!(c.recompose(make_scene(selected.clone(), colors)), "状态变化应触发重组");
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let tab_width = 120.0;
        let content_width = 33.0; // natural≈65-32
        let target = tab_width + (tab_width - content_width) / 2.0; // ≈163.5

        // 首帧动画刚启动——位置应开始离开 tab0 方向（不要求立即到位）
        let deadline = Instant::now() + Duration::from_millis(2000);
        let mut last_x = {
            let root = c.layout_root_idx().unwrap();
            let nodes = c.arena_nodes();
            let children = &nodes[root].children;
            nodes[children[4]].position.x
        };
        let mut converged = false;
        let mut frames = 0usize;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let root = c.layout_root_idx().unwrap();
            let nodes = c.arena_nodes();
            let children = &nodes[root].children;
            let x = nodes[children[4]].position.x;
            frames += 1;
            // 单调逼近 + 收敛到 target ±1
            if (x - last_x).abs() < 0.5 && (x - target).abs() < 1.0 {
                converged = true;
                break;
            }
            last_x = x;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(converged, "指示条应动画收敛到 {target}，实际 {last_x}");
        // The token is `DefaultSpatial` (stiffness 700), so this 120 px move settles in 26 of the
        // loop's 1/60 s steps (measured). The no-bouncy 200 winia used here needed about twice as long, and a stiffness
        // this size is the visible half of the change — `the_indicator_runs_on_m3s_default_spatial_spring`
        // pins the numbers, this pins the speed.
        assert!(
            frames <= 40,
            "the indicator should settle within 40 frames on the DefaultSpatial token, took {frames}"
        );
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[4]];
        assert!(
            (ind.position.x - target).abs() < 1.0,
            "最终指示条 x={} 应≈{}",
            ind.position.x,
            target
        );
    }

    #[test]
    fn tab_row_rtl_indicator_mirrors() {
        // RTL：selected=0 指示条应镜像到最右侧 tab（x=180，2 tabs）
        let c = tab_row_layout(0, true, 2, LayoutDirection::Rtl);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[3]]; // [tab0, tab1, divider, indicator]
        // tab0 在 RTL 下 x=180；指示条居中于该 slot
        let tab_width = 180.0;
        // contentWidth ≈ 35（natural≈67-32），居中偏移 = 180 + (180-35)/2 = 252.5
        assert!(
            (ind.position.x - 252.5).abs() < 1.0,
            "RTL 指示条 x={} 应≈252.5",
            ind.position.x
        );
    }

    // ── ScrollableTabRow ──

    fn scrollable_tab_row_layout(
        selected: usize,
        count: usize,
    ) -> (Composer, crate::modifier::ScrollState) {
        scrollable_tab_row_layout_dir(selected, count, LayoutDirection::Ltr)
    }

    fn scrollable_tab_row_layout_dir(
        selected: usize,
        count: usize,
        direction: LayoutDirection,
    ) -> (Composer, crate::modifier::ScrollState) {
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        let state = crate::modifier::ScrollState::new();
        let st = state.clone();
        c.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(colors, direction, ctx, |ctx| {
                let state_outer = st.clone();
                let state_inner = st.clone();
                ScrollableTabRow::new(selected, move |ctx| {
                    let state3 = state_inner.clone();
                    for i in 0..count {
                        let label = format!("Tab {}", i);
                        let sel = i == selected;
                        let st4 = state3.clone();
                        Tab::new(sel, move || { let _ = st4; })
                            .text(move |ctx| Text::new(&label).build(ctx))
                            .build(ctx);
                    }
                })
                .scroll_state(state_outer)
                .build(ctx);
            });
        });
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        (c, state)
    }

    #[test]
    fn scrollable_tab_row_lays_out_natural_widths_with_padding() {
        let (c, _) = scrollable_tab_row_layout(0, 3);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        // children = [tab0, tab1, tab2, divider, indicator]
        assert_eq!(children.len(), 5);
        // 每个 tab ≥ minTabWidth(90)，从 edgePadding(52) 起排
        let tab0 = &nodes[children[0]];
        let tab1 = &nodes[children[1]];
        assert!(tab0.position.x >= SCROLLABLE_TAB_ROW_EDGE_START_PADDING - 0.01, "tab0 x={}", tab0.position.x);
        assert!(tab0.measured_size.width >= 90.0 - 0.01, "tab0 宽={}", tab0.measured_size.width);
        assert!(
            (tab1.position.x - (tab0.position.x + tab0.measured_size.width)).abs() < 0.01,
            "tab1 紧接 tab0：tab0.x={} tab0.w={} tab1.x={}",
            tab0.position.x, tab0.measured_size.width, tab1.position.x
        );
    }

    #[test]
    fn scrollable_tab_row_indicator_centered_on_selected() {
        let (c, _) = scrollable_tab_row_layout(1, 4);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[5]]; // [tab0..3, divider, indicator]
        // tab1 起点 = 52 + w0；指示条居中于该 slot
        let tab1 = &nodes[children[1]];
        let expected = tab1.position.x + tab1.measured_size.width / 2.0;
        // 指示条中心 = x + width/2；Primary followContent → width < tab width
        let ind_center = ind.position.x + ind.measured_size.width / 2.0;
        assert!(
            (ind_center - expected).abs() < 1.0,
            "指示条应居中于 tab1（中心 {} vs tab1 中心 {}）",
            ind_center,
            expected
        );
    }

    #[test]
    fn scrollable_tab_row_scrolls_selected_into_view() {
        // 选中变化 → ScrollableTabData 触发居中滚动（offset 动画推进）
        use crate::runtime::state::State;
        use std::time::{Duration, Instant};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        fn make_scene(
            selected: State<usize>,
            scroll_state: crate::modifier::ScrollState,
            colors: crate::theme::ThemeColors,
        ) -> impl FnOnce(&mut ComposeCtx) {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Ltr, ctx, |ctx| {
                    let sel = selected.clone();
                    let st_outer = scroll_state.clone();
                    let st_inner = scroll_state.clone();
                    ScrollableTabRow::new(sel.get(), move |ctx| {
                        let st3 = st_inner.clone();
                        for i in 0..10 {
                            let label = format!("Tab {}", i);
                            let sel2 = selected.clone();
                            let st4 = st3.clone();
                            Tab::new(sel2.get() == i, move || { let _ = st4; })
                                .text(move |ctx| Text::new(&label).build(ctx))
                                .build(ctx);
                        }
                    })
                    .scroll_state(st_outer)
                    .build(ctx);
                });
            }
        }

        let selected = State::new(0usize);
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        let scroll_state = crate::modifier::ScrollState::new();

        c.compose(make_scene(selected.clone(), scroll_state.clone(), colors.clone()));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let off0 = scroll_state.offset.get();
        assert_eq!(off0, 0.0, "首帧 offset 0");

        // 切到最后一个 tab → 居中滚动（动画推进）
        selected.set(9);
        assert!(c.recompose(make_scene(selected.clone(), scroll_state.clone(), colors)));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let deadline = Instant::now() + Duration::from_millis(3000);
        let mut last = scroll_state.offset.get();
        let mut moved = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let cur = scroll_state.offset.get();
            if cur > last + 0.5 { moved = true; }
            if (cur - last).abs() < 0.5 && moved { break; }
            last = cur;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(moved, "选中变化应触发滚动动画（offset 单调增加）");
        // 内容总宽 = 2*52 + ΣtabW（10 个 ≥90）→ 远超视口 360，应滚到末尾附近
        assert!(last > 200.0, "应滚到末尾附近（offset={last}）");
    }

    #[test]
    fn scrollable_tab_row_centers_initial_selection() {
        // 回归：一开始 selected=9（10 tabs）——首帧 fling_limit 未回写，
        // 居中滚动应延迟到第二帧执行（此前首帧 target 恒 0 永不居中）
        use std::time::{Duration, Instant};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        let (mut c, scroll_state) = scrollable_tab_row_layout(9, 10);
        // 首帧布局：layout_seen=false → 不滚动（此时 fling_limit 尚为 0）
        assert_eq!(scroll_state.offset.get(), 0.0, "首帧不应滚动");
        // 第二帧：fling_limit 已回写 → 触发居中滚动
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let deadline = Instant::now() + Duration::from_millis(3000);
        let mut last = scroll_state.offset.get();
        let mut moved = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let cur = scroll_state.offset.get();
            if cur > last + 0.5 { moved = true; }
            if (cur - last).abs() < 0.5 && moved { break; }
            last = cur;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(moved, "初始选中 9 应触发居中滚动（第二帧起）");
        // 选中末尾 tab：内容总宽 > 视口 → offset 应滚到末尾附近（> 200）
        assert!(last > 200.0, "应滚到末尾附近（offset={last}）");
    }

    #[test]
    fn scrollable_tab_row_rtl_mirrors_tab_positions() {
        // RTL：tabs 从右往左排（tab0 在最右），滚动容器标记 scroll_reverse
        let (c, _) = scrollable_tab_row_layout_dir(0, 3, LayoutDirection::Rtl);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        // scroll_reverse 已由 node.rs 从 modifier 提取（render 平移镜像）
        assert!(nodes[root].scroll_reverse, "RTL 滚动容器应标记 scroll_reverse");
        let children = &nodes[root].children;
        // children = [tab0, tab1, tab2, divider, indicator]
        let tab0 = &nodes[children[0]];
        let tab1 = &nodes[children[1]];
        let tab2 = &nodes[children[2]];
        // 内容总宽 = 2*52 + Σw；tab0 在最右（x 最大）
        assert!(tab0.position.x > tab1.position.x, "tab0.x={} 应 > tab1.x={}（RTL 镜像）",
            tab0.position.x, tab1.position.x);
        assert!(tab1.position.x > tab2.position.x, "tab1.x={} 应 > tab2.x={}",
            tab1.position.x, tab2.position.x);
        // 右边缘 = 总宽 - edge_padding（tab0 右端对齐内容末端）
        let total = tab0.position.x + tab0.measured_size.width;
        assert!((total + SCROLLABLE_TAB_ROW_EDGE_START_PADDING - (c.arena_nodes()[children[3]].position.x + c.arena_nodes()[children[3]].measured_size.width)).abs() < 0.01,
            "tab0 右端应贴内容末端（总宽={total}）");
        // 左边缘 = edge_padding（tab2 左端对齐内容起点）
        assert!((tab2.position.x - SCROLLABLE_TAB_ROW_EDGE_START_PADDING).abs() < 0.01,
            "tab2.x={} 应 = edge_padding", tab2.position.x);
    }

    #[test]
    fn scrollable_tab_row_rtl_indicator_centered_on_selected() {
        // RTL：selected=1（中间 tab）→ 指示条居中于镜像后的 tab1 slot
        let (c, _) = scrollable_tab_row_layout_dir(1, 4, LayoutDirection::Rtl);
        let root = c.layout_root_idx().unwrap();
        let nodes = c.arena_nodes();
        let children = &nodes[root].children;
        let ind = &nodes[children[5]]; // [tab0..3, divider, indicator]
        let tab1 = &nodes[children[1]];
        let expected = tab1.position.x + tab1.measured_size.width / 2.0;
        let ind_center = ind.position.x + ind.measured_size.width / 2.0;
        assert!(
            (ind_center - expected).abs() < 1.0,
            "RTL 指示条应居中于 tab1（中心 {} vs tab1 中心 {}）",
            ind_center,
            expected
        );
    }

    #[test]
    fn scrollable_tab_row_rtl_scrolls_selected_into_view() {
        // RTL：选中变化 → 居中滚动（target 绕 available 镜像：
        // offset_rtl = available - offset_ltr——首帧不滚、第二帧起动画推进）
        use crate::runtime::state::State;
        use std::time::{Duration, Instant};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

        fn make_scene(
            selected: State<usize>,
            scroll_state: crate::modifier::ScrollState,
            colors: crate::theme::ThemeColors,
        ) -> impl FnOnce(&mut ComposeCtx) {
            move |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme_and_direction(colors, LayoutDirection::Rtl, ctx, |ctx| {
                    let sel = selected.clone();
                    let st_outer = scroll_state.clone();
                    let st_inner = scroll_state.clone();
                    ScrollableTabRow::new(sel.get(), move |ctx| {
                        let st3 = st_inner.clone();
                        for i in 0..10 {
                            let label = format!("Tab {}", i);
                            let sel2 = selected.clone();
                            let st4 = st3.clone();
                            Tab::new(sel2.get() == i, move || { let _ = st4; })
                                .text(move |ctx| Text::new(&label).build(ctx))
                                .build(ctx);
                        }
                    })
                    .scroll_state(st_outer)
                    .build(ctx);
                });
            }
        }

        let selected = State::new(0usize);
        let mut c = Composer::new();
        let colors = crate::theme::ThemeColors::default_light();
        let scroll_state = crate::modifier::ScrollState::new();

        c.compose(make_scene(selected.clone(), scroll_state.clone(), colors.clone()));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
        let off0 = scroll_state.offset.get();
        assert_eq!(off0, 0.0, "首帧 offset 0");

        // 切到最后一个 tab → RTL 镜像居中滚动（动画推进）
        selected.set(9);
        assert!(c.recompose(make_scene(selected.clone(), scroll_state.clone(), colors)));
        c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));

        let deadline = Instant::now() + Duration::from_millis(3000);
        let mut last = scroll_state.offset.get();
        let mut moved = false;
        while Instant::now() < deadline {
            crate::animation::update_animations();
            c.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
            let cur = scroll_state.offset.get();
            if cur > last + 0.5 { moved = true; }
            if (cur - last).abs() < 0.5 && moved { break; }
            last = cur;
            std::thread::sleep(Duration::from_millis(8));
        }
        assert!(moved, "RTL 选中变化应触发镜像居中滚动");
        // 内容总宽 = 2*52 + ΣtabW（10 个 ≥90）→ 远超视口 360
        // tab9 物理最左（x≈52）→ LTR centered = c - vw/2 < 0 → clamp 0；
        // RTL target = available - centered ≈ available（滚到末尾附近）
        assert!(last > 200.0, "RTL 应滚到末尾附近（offset={last}）");
    }
