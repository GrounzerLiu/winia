    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::constraints::Constraints;
    use crate::modifier::Modifier as M;

    fn icon(ctx: &mut ComposeCtx, size: f32) {
        let k = ctx.next_key();
        ctx.start_leaf(k, M::new().size(size, size));
        ctx.end_node();
    }

    fn scaffold() -> NavigationSuiteScaffold {
        NavigationSuiteScaffold::new(
            |items| {
                for i in 0..3 {
                    let _ = i;
                    items.item(
                        i == 0,
                        |ctx| icon(ctx, 24.0),
                        |ctx| crate::components::Text::new("Home").build(ctx),
                        || {},
                    );
                }
            },
            |ctx| {
                let k = ctx.next_key();
                ctx.start_leaf(k, M::new());
                ctx.end_node();
            },
        )
    }

    #[test]
    fn default_type_follows_window_size_classes() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
        crate::layout::adaptive::set_window_size(700.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        // Expanded width still gets the COLLAPSED rail: upstream's rule never returns the expanded
        // one — expanding is the rail's own state, or an explicit `layout_type(..)`.
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        // A short window gets the horizontal bar ...
        crate::layout::adaptive::set_window_size(1000.0, 400.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarMedium);
        // ... but the WIDTH test comes first: a window that is both narrow and short is Compact.
        crate::layout::adaptive::set_window_size(500.0, 400.0);
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
    }

    /// The primary action follows the SHAPE. Upstream's contract is "inside vertical navigation
    /// components as part of their header, and above horizontal navigation components"
    /// (`NavigationSuite.kt:183-187`), so a rail layout passes it as the rail's `header` and a bar
    /// layout floats it over the bar. Bar-only was wrong, and it is why a wide window showed no FAB.
    #[test]
    fn the_primary_action_becomes_the_rails_header() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut found: Option<(f32, f32)> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            y: f32,
            out: &mut Option<(f32, f32)>,
        ) {
            let n = &nodes[idx];
            let (ax, ay) = (x + n.position.x, y + n.position.y);
            if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                *out = Some((ax, ay));
            }
            for &child in &n.children {
                walk(nodes, child, ax, ay, out);
            }
        }
        walk(nodes, root, 0.0, 0.0, &mut found);
        let (x, y) = found.expect("the rail layout carries the primary action in its header");
        assert!(x >= 0.0 && x <= 96.0, "inside the 96dp rail, x={x}");
        assert!(y < 200.0, "above the rail's items, y={y}");
    }


    /// RTL: the action is inset from the LOGICAL end, which is the LEFT edge when the layout runs
    /// right-to-left. The alignment mirrors itself, `absolute_offset` does not, so the sign is flipped by
    /// hand — without that it was pushed 16dp off the left edge instead of inset from it.
    #[test]
    fn the_primary_action_is_inset_from_the_logical_end_under_rtl() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        crate::layout::direction::provides(LayoutDirection::Rtl, || {
            composer.compose(|ctx| {
                scaffold()
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut found: Option<f32> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            out: &mut Option<f32>,
        ) {
            let n = &nodes[idx];
            let ax = x + n.position.x;
            if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                *out = Some(ax);
            }
            for &child in &n.children {
                walk(nodes, child, ax, out);
            }
        }
        walk(nodes, root, 0.0, &mut found);
        let x = found.expect("the action is composed");
        assert!(
            (x - 16.0).abs() < 0.5,
            "16dp in from the LEFT edge under RTL, got {x}"
        );
    }
    /// The action must never steal clicks from what sits beside it. It used to: the insets were PADDING,
    /// padding grows the node, and hit testing walks node rects (`content_box()` is `measured_size`) — so
    /// the container reached down over the bar and the part of the last item to its right went dead
    /// (reported on the demo, `Profile`). The insets are an offset now, and this is the guard.
    #[test]
    fn the_primary_action_does_not_swallow_the_item_beside_it() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0).test_tag("suite-action"));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let action = nodes
            .iter()
            .position(|n| n.modifier.get_test_tag() == Some("suite-action"))
            .expect("the action is composed");
        let hits = |x: f32, y: f32| crate::layout::node::hit_test(nodes, root, x, y);
        // On the action itself (it is 448..504 x 568..624): hittable.
        assert!(
            hits(470.0, 590.0).contains(&action),
            "the action answers for its own rect"
        );
        // On the LAST bar item, to the right of the action's x-range in the bar's own row (y 640..720):
        // the action must not be in that path.
        let beside = hits(470.0, 680.0);
        assert!(beside.contains(&root), "the point is inside the layout");
        assert!(
            !beside.contains(&action),
            "a click beside the action reaches what is there, not the action"
        );
    }
    /// `primaryActionContent` floats above the bar: 16dp clear of it, 16dp in from the trailing edge
    /// with the default `Alignment::End` (`NavigationSuiteScaffoldLayout` `:456-470`).
    #[test]
    fn the_primary_action_floats_above_the_bar() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // The Stack's children are [the Column, the action's offset container]; the container is
        // END-aligned and carries the insets, so the payload's ABSOLUTE position is the two
        // positions added up.
        let children = &nodes[root].children;
        assert_eq!(children.len(), 2, "the stack holds the bar layout and the action");
        let container = &nodes[children[1]];
        let fab = &nodes[container.children[0]];
        assert_eq!(fab.measured_size.width, 56.0);
        let abs_x = container.position.x + fab.position.x;
        let abs_y = container.position.y + fab.position.y;
        assert!(
            (abs_x - (500.0 - 56.0 - 16.0)).abs() < 0.01,
            "16dp in from the trailing edge, x={abs_x}"
        );
        assert!(
            (abs_y - (700.0 - 56.0 - 16.0 - 80.0)).abs() < 0.01,
            "16dp above the 80dp bar, y={abs_y}"
        );
    }


    /// `NavigationSuite` renders the ONE shape it is given, with no scaffold around it, through the same
    /// helpers the scaffold's shape host uses: a bar type gives the 80dp bar, a rail type the 96dp
    /// collapsed rail, and the classic type the 80dp classic rail.
    #[test]
    fn the_standalone_suite_renders_the_shape_it_is_given() {
        let size = |t: NavigationSuiteType| -> (f32, f32) {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                NavigationSuite::new(
                    |items| {
                        for _ in 0..3 {
                            items.item(
                                true,
                                |ctx| icon(ctx, 24.0),
                                |ctx| crate::components::Text::new("A").build(ctx),
                                || {},
                            );
                        }
                    },
                    t,
                )
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let s = nodes[root].measured_size;
            (s.width, s.height)
        };
        let bar = size(NavigationSuiteType::ShortNavigationBarCompact);
        assert!((bar.1 - 80.0).abs() < 0.5, "a bar is 80dp tall, got {bar:?}");
        let rail = size(NavigationSuiteType::WideNavigationRailCollapsed);
        assert!((rail.0 - 96.0).abs() < 0.5, "a collapsed wide rail is 96dp wide, got {rail:?}");
        let classic = size(NavigationSuiteType::NavigationRail);
        assert!((classic.0 - 80.0).abs() < 0.5, "the classic rail is 80dp wide, got {classic:?}");
    }

        /// `NavigationSuiteScaffoldLayout` places three slots by shape, with upstream's numbers.
    ///
    /// A BAR layout gives the content the height the navigation leaves, pins the navigation to the bottom
    /// and floats the action 16dp above it at the caller's alignment. A RAIL layout puts the navigation at
    /// the start and the content beside it, and PARKS the action off-view — a vertical shape keeps it in
    /// its own header, which is what upstream's rail branch means by never placing it.
    #[test]
    fn the_scaffold_layout_places_the_three_slots() {
        let probe = |t: NavigationSuiteType| -> Vec<(f32, f32, f32, f32)> {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                NavigationSuiteScaffoldLayout::new(
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                        ctx.end_node();
                    },
                    // Fills, so the height it is given is the height it reports.
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().fill_max_size());
                        ctx.end_node();
                    },
                )
                .layout_type(t)
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .map(|&c| {
                    let n = &nodes[c];
                    (
                        n.position.x,
                        n.position.y,
                        n.measured_size.width,
                        n.measured_size.height,
                    )
                })
                .collect()
        };

        let bar = probe(NavigationSuiteType::ShortNavigationBarCompact);
        assert_eq!(bar.len(), 3, "navigation, action, content");
        assert_eq!((bar[0].0, bar[0].1), (0.0, 620.0), "the navigation is pinned to the bottom");
        assert_eq!(bar[2].3, 620.0, "and the content gets the height it leaves");
        assert_eq!(
            (bar[1].0, bar[1].1),
            (428.0, 548.0),
            "the action is 16dp above the navigation and 16dp in from the end"
        );

        let rail = probe(NavigationSuiteType::WideNavigationRailCollapsed);
        assert_eq!((rail[0].0, rail[0].1), (0.0, 0.0), "the rail sits at the start");
        assert_eq!(rail[2].0, 96.0, "and the content starts after it");
        assert!(
            rail[1].0 > 500.0,
            "the action is parked off-view for a vertical shape, got {}",
            rail[1].0
        );
    }

        /// The unbounded sentinel this crate actually produces is `f32::MAX`, not `INFINITY`, and it IS
    /// finite — `is_finite()` lets it through. A rail measured against it must not compute a slack of
    /// ~3.4e38 and place its items out at that coordinate.
    #[test]
    fn an_unbounded_rail_computes_no_slack() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .layout_type(NavigationSuiteType::WideNavigationRailCollapsed)
                .vertical_arrangement(Arrangement::End)
                .build(ctx);
        });
        // `f32::MAX` is the sentinel the pipeline rewrites a child's max to under a scroll container;
        // `Constraints::UNBOUNDED` uses INFINITY and was already caught.
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, f32::MAX));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut worst = 0.0f32;
        fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, ay: f32, worst: &mut f32) {
            let n = &nodes[idx];
            let y = ay + n.position.y;
            if !y.is_finite() || y > 1.0e9 {
                *worst = y;
            }
            for &child in &n.children {
                walk(nodes, child, y, worst);
            }
        }
        walk(nodes, root, 0.0, &mut worst);
        assert!(
            worst.abs() < 1.0,
            "no placement may be pushed to the sentinel, got y={worst}"
        );
    }

    /// `Stretch` is not a horizontal placement for a floating action, so it takes the LOGICAL trailing
    /// edge — the right one under LTR, the left one under RTL. It used to fall through to the physical
    /// right edge in both directions, which put it at the leading edge under RTL.
    #[test]
    fn a_stretched_action_takes_the_logical_trailing_edge() {
        let probe = |rtl: bool| -> f32 {
            let mut composer = Composer::new();
            let mut run = || {
                composer.compose(|ctx| {
                    NavigationSuiteScaffoldLayout::new(
                        |ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                            ctx.end_node();
                        },
                        |ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, M::new().fill_max_size());
                            ctx.end_node();
                        },
                    )
                    .layout_type(NavigationSuiteType::ShortNavigationBarCompact)
                    .primary_action_alignment(Alignment::Stretch)
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
                });
                composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            };
            if rtl {
                crate::layout::direction::provides(LayoutDirection::Rtl, run);
            } else {
                run();
            }
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[nodes[root].children[1]].position.x
        };
        assert!(
            (probe(false) - (500.0 - 56.0 - PRIMARY_ACTION_PADDING)).abs() < 0.5,
            "LTR: the trailing edge is the right one"
        );
        assert!(
            (probe(true) - PRIMARY_ACTION_PADDING).abs() < 0.5,
            "RTL: the trailing edge is the LEFT one"
        );
    }

    /// A rail that has a primary action AND a non-default arrangement: the header occupies its own
    /// height plus `WNRHeaderPadding` above the items, so the slack has to reserve it. Without that the
    /// items are pushed past the rail's bottom by exactly `header + 40` — 88dp for a 56dp action.
    #[test]
    fn an_arranged_rail_with_a_primary_action_keeps_its_items_inside() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let last_item_bottom = |arrangement: Arrangement| -> (f32, f32) {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                scaffold()
                    .layout_type(NavigationSuiteType::WideNavigationRailCollapsed)
                    .vertical_arrangement(arrangement)
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut rail_bottom = 0.0f32;
            let mut last_bottom = 0.0f32;
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                ax: f32,
                ay: f32,
                rail_bottom: &mut f32,
                last_bottom: &mut f32,
            ) {
                let n = &nodes[idx];
                let (x, y) = (ax + n.position.x, ay + n.position.y);
                let s = n.measured_size;
                // The rail is the full-height column at the leading edge.
                if x <= 0.5 && s.width > 80.0 && s.width < 200.0 && s.height > 500.0 {
                    *rail_bottom = y + s.height;
                }
                // Its items are the 64-tall rows inside it.
                if x < 96.0 && s.width > 40.0 && (s.height - 64.0).abs() < 0.5 {
                    *last_bottom = (y + s.height).max(*last_bottom);
                }
                for &child in &n.children {
                    walk(nodes, child, x, y, rail_bottom, last_bottom);
                }
            }
            walk(nodes, root, 0.0, 0.0, &mut rail_bottom, &mut last_bottom);
            (last_bottom, rail_bottom)
        };
        for arrangement in [Arrangement::End, Arrangement::Center] {
            let (last, rail) = last_item_bottom(arrangement);
            assert!(
                rail > 0.0,
                "the rail is laid out at a full height, got {rail}"
            );
            assert!(
                last <= rail + 0.5,
                "{arrangement:?}: the last item's bottom {last} must stay inside the rail {rail}"
            );
        }
    }

    /// `vertical_arrangement` reaches the rail: `End` pushes the ITEMS to the bottom of the
    /// rail, `Start` (the default, upstream's `Top`) leaves them at `TopSpace`. Only a rail with SLACK
    /// can tell them apart, which is why the suite's rail fills the window's height.
    #[test]
    fn the_vertical_arrangement_moves_the_rails_items() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let first_item_y = |arrangement: Arrangement| -> f32 {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                scaffold()
                    .layout_type(NavigationSuiteType::WideNavigationRailCollapsed)
                    .vertical_arrangement(arrangement)
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut y: Option<f32> = None;
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                x: f32,
                cy: f32,
                out: &mut Option<f32>,
            ) {
                let n = &nodes[idx];
                let (ax, ay) = (x + n.position.x, cy + n.position.y);
                let s = n.measured_size;
                if out.is_none() && ax < 96.0 && s.width > 40.0 && s.height == 64.0 {
                    *out = Some(ay);
                }
                for &child in &n.children {
                    walk(nodes, child, ax, ay, out);
                }
            }
            walk(nodes, root, 0.0, 0.0, &mut y);
            y.expect("the rail's first item is laid out")
        };
        let start = first_item_y(Arrangement::Start);
        let end = first_item_y(Arrangement::End);
        assert!(
            end > start + 100.0,
            "End pushes the stack to the bottom: start {start}, end {end}"
        );
    }

        /// The RAIL layout under RTL — the half the earlier RTL bug's class had not been checked on. The Row
    /// mirrors, so the rail docks on the RIGHT, and the primary action keeps its inset inside it.
    ///
    /// Measured in the demo first (921x544): LTR rail at x=0 with the action at 20; RTL rail at 825
    /// (= 921 - 96) with the action at 845, i.e. the same 20dp.
    #[test]
    fn the_rail_layout_docks_on_the_right_under_rtl() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        crate::layout::direction::provides(LayoutDirection::Rtl, || {
            composer.compose(|ctx| {
                scaffold()
                    .layout_type(NavigationSuiteType::WideNavigationRailCollapsed)
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut rail_x: Option<f32> = None;
        let mut action_x: Option<f32> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            rail: &mut Option<f32>,
            action: &mut Option<f32>,
        ) {
            let n = &nodes[idx];
            let ax = x + n.position.x;
            let s = n.measured_size;
            if rail.is_none() && s.width == 96.0 && s.height > 300.0 {
                *rail = Some(ax);
            }
            if action.is_none() && s.width == 56.0 && s.height == 56.0 {
                *action = Some(ax);
            }
            for &child in &n.children {
                walk(nodes, child, ax, rail, action);
            }
        }
        walk(nodes, root, 0.0, &mut rail_x, &mut action_x);
        let rail = rail_x.expect("the rail is laid out");
        assert!(
            (rail - (1000.0 - 96.0)).abs() < 1.0,
            "the rail docks on the RIGHT edge under RTL, got {rail}"
        );
        let action = action_x.expect("the action is composed");
        assert!(
            (action - (rail + 20.0)).abs() < 1.0,
            "and the action keeps its 20dp inset inside it, rail={rail} action={action}"
        );
    }

        /// The two legacy types that had NO coverage at all.
    ///
    /// `NavigationBar` is the short bar with the icon ABOVE the label (the only visible difference from
    /// `ShortNavigationBarMedium`, whose icon sits to the left), and `NavigationDrawer` is a permanent
    /// drawer sheet: a 240..360dp column at the leading edge whose rows are the drawer's own items.
    #[test]
    fn the_legacy_bar_and_drawer_render_their_classic_shapes() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        // (the first 24x24 icon, the first text, a 56-tall row that spans >=200, the widest 80-tall box)
        type Seen = (Option<(f32, f32)>, Option<(f32, f32)>, Option<(f32, f32)>, Option<f32>);
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            y: f32,
            out: &mut Seen,
        ) {
            let n = &nodes[idx];
            let (ax, ay) = (x + n.position.x, y + n.position.y);
            let s = n.measured_size;
            let has_text = n.modifier.elements().iter().any(|el| {
                matches!(el, crate::modifier::ModifierElement::TextContent { .. })
            });
            if out.0.is_none() && s.width == 24.0 && s.height == 24.0 {
                out.0 = Some((ax, ay));
            }
            if out.1.is_none() && has_text {
                out.1 = Some((ax, ay));
            }
            if out.2.is_none() && s.height == 56.0 && s.width >= 200.0 {
                out.2 = Some((ax, ay));
            }
            if s.height == 80.0 && s.width > 900.0 {
                out.3 = Some(s.width);
            }
            for &child in &n.children {
                walk(nodes, child, ax, ay, out);
            }
        }
        let probe = |t: NavigationSuiteType| -> Seen {
            let mut composer = Composer::new();
            composer.compose(|ctx| scaffold().layout_type(t).build(ctx));
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out: Seen = (None, None, None, None);
            walk(nodes, root, 0.0, 0.0, &mut out);
            out
        };

        let (icon, label, _, bar_width) = probe(NavigationSuiteType::NavigationBar);
        let icon = icon.expect("the legacy bar draws its icons");
        let label = label.expect("and its labels");
        assert!(
            icon.1 < label.1,
            "the icon is ABOVE the label (the Top position), icon y={} label y={}",
            icon.1,
            label.1
        );
        assert!(
            bar_width.is_some_and(|w| w > 900.0),
            "and the classic bar spans the window, got {bar_width:?}"
        );

        let (_, label, row, bar) = probe(NavigationSuiteType::NavigationDrawer);
        let row = row.expect("the drawer renders its own item rows");
        assert!(row.0 < 360.0, "in a leading-edge column, x={}", row.0);
        assert!(label.is_some(), "and those rows carry labels");
        assert!(bar.is_none(), "and there is no 80dp bar in this shape");
    }

    /// Every layout type the policy promises to place, not just the two the first test picked. The
    /// split is bar vs vertical: a bar pins the navigation to the bottom and the content above it; a
    /// vertical shape puts the navigation at the leading edge with the content beside it and parks the
    /// action off-view.
    #[test]
    fn every_layout_type_gets_its_familys_placement() {
        let probe = |t: NavigationSuiteType| -> Vec<(f32, f32, f32, f32)> {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                NavigationSuiteScaffoldLayout::new(
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                        ctx.end_node();
                    },
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().fill_max_size());
                        ctx.end_node();
                    },
                )
                .layout_type(t)
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .map(|&c| {
                    let n = &nodes[c];
                    (n.position.x, n.position.y, n.measured_size.width, n.measured_size.height)
                })
                .collect()
        };

        // The whole bar family: navigation pinned to the bottom, content above it, action 16dp up.
        for t in [
            NavigationSuiteType::ShortNavigationBarCompact,
            NavigationSuiteType::ShortNavigationBarMedium,
            NavigationSuiteType::NavigationBar,
        ] {
            let slots = probe(t);
            assert_eq!(slots.len(), 3, "{t:?}: navigation, action, content");
            assert_eq!(
                (slots[0].0, slots[0].1),
                (0.0, 620.0),
                "{t:?}: the navigation is pinned to the bottom"
            );
            assert_eq!(slots[2].3, 620.0, "{t:?}: the content gets the rest");
            // The 16 is a LITERAL, not `PRIMARY_ACTION_PADDING`: a test that reads the same constant
            // the production code reads stays green if the constant is changed, which is how the RTL
            // test's magnitude went unpinned. `PrimaryActionContentPadding` is 16dp.
            assert!(
                (slots[1].1 - (700.0 - 56.0 - 16.0 - 80.0)).abs() < 0.5,
                "{t:?}: the action floats 16dp above the navigation, got {}",
                slots[1].1
            );
        }

        // The whole vertical family: navigation at the leading edge, content beside it, action parked.
        for t in [
            NavigationSuiteType::WideNavigationRailCollapsed,
            NavigationSuiteType::WideNavigationRailExpanded,
            NavigationSuiteType::NavigationRail,
            NavigationSuiteType::NavigationDrawer,
        ] {
            let slots = probe(t);
            assert_eq!(
                (slots[0].0, slots[0].1),
                (0.0, 0.0),
                "{t:?}: the navigation sits at the leading edge"
            );
            assert_eq!(
                slots[2].0, slots[0].2,
                "{t:?}: the content starts where the navigation ends"
            );
            assert!(
                (slots[1].0 - (500.0 + 1000.0)).abs() < 0.5,
                "{t:?}: the action is parked at max_width + 1000, got {}",
                slots[1].0
            );
        }

        // `None` is the vertical branch too — upstream's `isNavigationBar` is the same three types, so
        // a `None` layout still measures the navigation slot and offsets the content by it. That is
        // correct for what `None` means ("no navigation"): the caller passes an EMPTY navigation and
        // the offset is zero. This case passes an empty one, which is the reachable, meaningful shape;
        // supplying real navigation content with `None` would be a caller error, not a placement rule.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationSuiteScaffoldLayout::new(
                |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().size(0.0, 0.0));
                    ctx.end_node();
                },
                |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().fill_max_size());
                    ctx.end_node();
                },
            )
            .layout_type(NavigationSuiteType::None)
            .primary_action(|ctx| {
                let k = ctx.next_key();
                ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                ctx.end_node();
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let slots: Vec<(f32, f32, f32, f32)> = nodes[root]
            .children
            .iter()
            .map(|&c| {
                let n = &nodes[c];
                (n.position.x, n.position.y, n.measured_size.width, n.measured_size.height)
            })
            .collect();
        assert_eq!(slots.len(), 3, "the three slots stay addressable");
        assert_eq!((slots[2].0, slots[2].1), (0.0, 0.0), "an empty navigation leaves the content at 0");
        assert_eq!(slots[2].2, 500.0, "and the content takes the whole width");
        // The action is what tells the two families apart here: with a zero-width navigation the
        // content assertions above hold identically under either branch, and only the parked x does
        // not. `max_width + 1000` is the vertical family's park, so this is the assertion that would
        // catch `None` being classified as a bar (where it would sit at 500 - 56 - 16 = 428).
        assert!(
            (slots[1].0 - (500.0 + 1000.0)).abs() < 0.5,
            "None is the VERTICAL branch: the action is parked at max_width + 1000, got {}",
            slots[1].0
        );
    }

    /// A scaffold that stops building (its parent Skipped it) must not leave `is_animating` stuck on:
    /// the mounted `State` keeps animating on its own, and `is_animating` peeks it, so the value
    /// settles even when no further build runs. This is what makes the mount safe.
    #[test]
    fn is_animating_settles_even_if_no_build_runs_again() {
        let state = NavigationSuiteScaffoldState::new(NavigationSuiteScaffoldValue::Visible);
        let mid = crate::runtime::state::State::new(0.5f32);
        state.attach_progress(mid.clone());
        assert!(state.is_animating(), "mid-flight reads as animating");
        // The tween reaches its end without a rebuild; `peek` sees the new value.
        mid.set(1.0);
        assert!(
            !state.is_animating(),
            "the settled value must be reported even when no build runs again"
        );
    }

    /// `target_value` flips with the request; `is_animating` reports the SCAFFOLD's real progress,
    /// which the scaffold mounts into the state. A veto leaves both where they were.
    #[test]
    fn the_state_reports_a_target_and_the_real_animation() {
        let state = NavigationSuiteScaffoldState::new(NavigationSuiteScaffoldValue::Visible);
        assert_eq!(state.target_value(), NavigationSuiteScaffoldValue::Visible);
        assert_eq!(state.current_value(), NavigationSuiteScaffoldValue::Visible);
        assert!(
            !state.is_animating(),
            "nothing is mounted yet, so there is nothing to report"
        );

        state.hide();
        assert_eq!(state.target_value(), NavigationSuiteScaffoldValue::Hidden);
        assert_eq!(
            state.target_value(),
            state.current_value(),
            "the value flips at request time — the animation is the layout progress, not a second value"
        );

        // Mount a progress the way the scaffold does: mid-flight reports animating, the ends do not.
        let mid = crate::runtime::state::State::new(0.5f32);
        state.attach_progress(mid.clone());
        assert!(state.is_animating(), "half-way is animating");
        mid.set(0.0);
        assert!(!state.is_animating(), "settled hidden is not animating");
        mid.set(1.0);
        assert!(!state.is_animating(), "settled visible is not animating");

        // A veto leaves the target where it was, exactly like the current value.
        let vetoed = NavigationSuiteScaffoldState::with_confirm(
            NavigationSuiteScaffoldValue::Visible,
            |_| false,
        );
        vetoed.hide();
        assert_eq!(vetoed.target_value(), NavigationSuiteScaffoldValue::Visible);
        assert_eq!(vetoed.current_value(), NavigationSuiteScaffoldValue::Visible);
    }

    /// Degenerate inputs must not panic: a suite with NO items still lays out, and the scaffold layout
    /// with NO primary action still keeps the slot alive (its policy indexes `children[1]`).
    #[test]
    fn a_suite_with_no_items_and_no_action_still_lays_out() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);

        // No items at all, in every shape.
        for t in [
            NavigationSuiteType::ShortNavigationBarCompact,
            NavigationSuiteType::WideNavigationRailCollapsed,
            NavigationSuiteType::NavigationRail,
            NavigationSuiteType::NavigationDrawer,
        ] {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                NavigationSuite::new(|_items| {}, t).build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let s = nodes[root].measured_size;
            // Not `is_finite()`: the unbounded sentinel in this crate is `f32::MAX`, which IS finite
            // (`layout/flex.rs`), so a layout that measured at the sentinel would slip through. Any
            // real measurement here is at most the constraint.
            assert!(
                s.width < f32::MAX && s.height < f32::MAX,
                "{t:?}: a zero-item suite must measure at a real size, not the sentinel, got {s:?}"
            );
            assert!(
                s.width > 0.0 && s.height > 0.0,
                "{t:?}: and it still occupies something, got {s:?}"
            );
        }

        // `NavigationSuiteScaffoldLayout` with NO primary action: its policy indexes `children[1]` as the
        // action, so the slot has to stay alive even when the caller gives none.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            NavigationSuiteScaffoldLayout::new(
                |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                    ctx.end_node();
                },
                |ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().fill_max_size());
                    ctx.end_node();
                },
            )
            .layout_type(NavigationSuiteType::ShortNavigationBarCompact)
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(
            nodes[root].children.len(),
            3,
            "navigation, action placeholder and content stay addressable"
        );
        let placeholder = &nodes[nodes[root].children[1]];
        assert!(
            placeholder.measured_size.width == 0.0 && placeholder.measured_size.height == 0.0,
            "the placeholder occupies nothing, got {:?}",
            placeholder.measured_size
        );
    }

    /// The legacy shapes mirror too. A classic rail docks on the RIGHT under RTL (its 80dp column),
    /// and a drawer's 240..360 column does the same — neither is a bare-x placement.
    #[test]
    fn the_legacy_shapes_mirror_under_rtl() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let probe = |t: NavigationSuiteType| -> f32 {
            let mut composer = Composer::new();
            let mut run = || {
                composer.compose(|ctx| scaffold().layout_type(t).build(ctx));
                composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            };
            crate::layout::direction::provides(LayoutDirection::Rtl, run);
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let nav = &nodes[nodes[root].children[0]];
            nav.position.x
        };

        let rail_x = probe(NavigationSuiteType::NavigationRail);
        assert!(
            (rail_x - (1000.0 - 80.0)).abs() < 0.5,
            "the classic rail docks on the right under RTL, got {rail_x}"
        );

        let drawer_x = probe(NavigationSuiteType::NavigationDrawer);
        // The drawer fills its `sizeIn(240, 360)` band to the MAXIMUM here (its items want the wide
        // side), so the docked right edge is 1000 - 360. Measured, not assumed: the first version of
        // this assertion guessed 240 and read 640.
        assert!(
            (drawer_x - (1000.0 - 360.0)).abs() < 0.5,
            "the drawer column docks on the right under RTL at 1000 - 360, got {drawer_x}"
        );
    }

    /// The classic shapes space their items BY HAND: `Center` and `End` get a leading weighted spacer
    /// inside the shape's own column (where the weight is height), and `Center` gets a trailing one too.
    /// Without the axis being vertical, or without the trailing spacer, `Center` would equal `End`.
    #[test]
    fn the_legacy_shapes_honour_the_vertical_arrangement() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        // The classic shapes' own item rows are 56dp tall and sit inside the shape's leading column.
        // Only the FIRST y is compared: the spacers move the whole stack, so the first row is the
        // sensitive one. The x bound keeps the window-wide content out of the sample.
        let rows = |t: NavigationSuiteType, arrangement: Arrangement| -> (f32, f32) {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                scaffold()
                    .layout_type(t)
                    .vertical_arrangement(arrangement)
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let (mut first, mut last) = (f32::MAX, f32::MIN);
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                x: f32,
                y: f32,
                first: &mut f32,
                last: &mut f32,
            ) {
                let n = &nodes[idx];
                let (ax, ay) = (x + n.position.x, y + n.position.y);
                // The classic rail is 80 wide and the drawer's column at most 360: staying inside the
                // leading column keeps the window-wide content out of the sample.
                if ax < 360.0
                    && (n.measured_size.height - 56.0).abs() < 0.5
                    && n.measured_size.width >= 40.0
                {
                    *first = (*first).min(ay);
                    *last = (*last).max(ay);
                }
                for &child in &n.children {
                    walk(nodes, child, ax, ay, first, last);
                }
            }
            walk(nodes, root, 0.0, 0.0, &mut first, &mut last);
            (first, last)
        };

        for t in [
            NavigationSuiteType::NavigationRail,
            NavigationSuiteType::NavigationDrawer,
        ] {
            let (start_first, _) = rows(t, Arrangement::Start);
            let (end_first, _) = rows(t, Arrangement::End);
            let (center_first, _) = rows(t, Arrangement::Center);
            assert!(
                end_first > start_first + 50.0,
                "{t:?}: End pushes the items down (start {start_first}, end {end_first})"
            );
            assert!(
                center_first > start_first + 20.0 && center_first < end_first - 20.0,
                "{t:?}: Center sits between Start and End (start {start_first}, center {center_first}, end {end_first})"
            );
        }
    }

    /// The legacy types render the CLASSIC components, not the short bar / wide rail: `NavigationRail`
    /// is the 80dp classic rail and takes the primary action as its header, as upstream's own mapping
    /// does (`NavigationSuite.kt:632-658`).
    #[test]
    fn the_legacy_navigation_rail_uses_the_classic_component() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold()
                .layout_type(NavigationSuiteType::NavigationRail)
                .primary_action(|ctx| {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                    ctx.end_node();
                })
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let rail = &nodes[nodes[root].children[0]];
        assert!(
            (rail.measured_size.width - 80.0).abs() < 0.5,
            "the classic rail is 80dp wide (the wide one is 96), got {}",
            rail.measured_size.width
        );
        let mut action_x: Option<f32> = None;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            out: &mut Option<f32>,
        ) {
            let n = &nodes[idx];
            let ax = x + n.position.x;
            if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                *out = Some(ax);
            }
            for &child in &n.children {
                walk(nodes, child, ax, out);
            }
        }
        walk(nodes, root, 0.0, &mut action_x);
        let x = action_x.expect("the classic rail carries the primary action as its header");
        assert!(x < 80.0, "inside the rail, x={x}");
    }

    /// `NavigationSuiteScaffoldLayout` mirrors with the layout, as upstream's `placeRelative` does: a RAIL
    /// layout docks the navigation on the RIGHT under RTL with the content beside it, and a BAR layout
    /// insets the action from the LOGICAL end, i.e. the left edge.
    #[test]
    fn the_scaffold_layout_mirrors_under_rtl() {
        let probe = |t: NavigationSuiteType| -> Vec<(f32, f32)> {
            let mut composer = Composer::new();
            crate::layout::direction::provides(LayoutDirection::Rtl, || {
                composer.compose(|ctx| {
                    NavigationSuiteScaffoldLayout::new(
                        |ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                            ctx.end_node();
                        },
                        |ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, M::new().fill_max_size());
                            ctx.end_node();
                        },
                    )
                    .layout_type(t)
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
                });
                composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            });
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .map(|&c| (nodes[c].position.x, nodes[c].position.y))
                .collect()
        };

        let rail = probe(NavigationSuiteType::WideNavigationRailCollapsed);
        assert!(
            (rail[0].0 - (500.0 - 96.0)).abs() < 1.0,
            "the rail docks on the RIGHT under RTL, got {}",
            rail[0].0
        );
        assert!(rail[2].0 < 1.0, "and the content starts at the left, got {}", rail[2].0);

        let bar = probe(NavigationSuiteType::ShortNavigationBarCompact);
        assert!(
            (bar[1].0 - PRIMARY_ACTION_PADDING).abs() < 0.5,
            "the action is inset from the logical (left) edge, got {}",
            bar[1].0
        );
    }

    /// The layout entry honours the caller's ALIGNMENT, in both directions: an `End` action is inset from
    /// the trailing edge, a `Start` one from the leading edge, and each mirrors under RTL.
    #[test]
    fn the_scaffold_layout_honours_the_action_alignment() {
        let probe = |alignment: Alignment, rtl: bool| -> f32 {
            let mut composer = Composer::new();
            let mut run = || {
                composer.compose(|ctx| {
                    NavigationSuiteScaffoldLayout::new(
                        |ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, M::new().width(96.0).height(80.0));
                            ctx.end_node();
                        },
                        |ctx| {
                            let k = ctx.next_key();
                            ctx.start_leaf(k, M::new().fill_max_size());
                            ctx.end_node();
                        },
                    )
                    .layout_type(NavigationSuiteType::ShortNavigationBarCompact)
                    .primary_action_alignment(alignment)
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
                });
                composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            };
            if rtl {
                crate::layout::direction::provides(LayoutDirection::Rtl, run);
            } else {
                run();
            }
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[nodes[root].children[1]].position.x
        };
        assert!(
            (probe(Alignment::End, false) - (500.0 - 56.0 - PRIMARY_ACTION_PADDING)).abs() < 0.5,
            "End/LTR is inset from the right"
        );
        assert!(
            (probe(Alignment::Start, false) - PRIMARY_ACTION_PADDING).abs() < 0.5,
            "Start/LTR is inset from the left"
        );
        assert!(
            (probe(Alignment::End, true) - PRIMARY_ACTION_PADDING).abs() < 0.5,
            "End/RTL is inset from the logical (left) edge"
        );
        assert!(
            (probe(Alignment::Start, true) - (500.0 - 56.0 - PRIMARY_ACTION_PADDING)).abs() < 0.5,
            "Start/RTL is inset from the logical (right) edge"
        );
    }

    /// `NavigationSuiteDefaults::colors()` delegates to each shape's own component default, which is
    /// what upstream's `colors()` does — and those defaults are NOT all the same value.
    #[test]
    fn the_suite_colors_default_to_each_shapes_own_component_default() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let c = NavigationSuiteDefaults::colors(&theme);
        assert_eq!(
            c.short_navigation_bar_container,
            theme.surface_container,
            "ShortNavigationBarDefaults.containerColor"
        );
        assert_eq!(
            c.wide_navigation_rail_container,
            theme.surface,
            "WideNavigationRailDefaults.colors()"
        );
        assert_eq!(c.short_navigation_bar_content, theme.on_surface);
        assert_eq!(c.navigation_drawer_container, theme.surface, "the standard container");
        let items = NavigationSuiteDefaults::item_colors(&theme);
        assert_eq!(
            items.short_navigation_bar,
            crate::components::navigation_bar::NavigationBarItemColors::from_theme(&theme),
            "the item colours are the bar's own"
        );
    }

    /// `NavigationSuiteScaffoldState` animates the navigation away and back, and the PRIMARY ACTION
    /// rides that animation: upstream places it at
    /// `layoutHeight - h - 16 - navigationHeight * animationProgress` (`:461-468`), so hiding a bar
    /// layout leaves it floating 16dp above the bottom edge instead of removing it with the bar.
    #[test]
    fn the_state_hides_the_suite_and_the_action_rides_the_animation() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let state = NavigationSuiteScaffoldState::new(NavigationSuiteScaffoldValue::Visible);
        let mut composer = Composer::new();
        let mut build = |c: &mut Composer, settle: bool| {
            let st = state.clone();
            c.compose(move |ctx| {
                scaffold()
                    .state(st.clone())
                    .primary_action(|ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            c.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
            if settle {
                for _ in 0..400 {
                    if !crate::animation::update_animations() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    let st = state.clone();
                    c.compose(move |ctx| {
                        scaffold()
                            .state(st.clone())
                            .primary_action(|ctx| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, M::new().width(56.0).height(56.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                    });
                    c.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
                }
            }
        };
        // (the bar CONTAINER's height, the action's y). The bar itself keeps its 80dp measured size —
        // the morph container around it is what scales, and that is what the eye sees.
        let probe = |c: &Composer| -> (f32, Option<f32>) {
            let mut bar_h = -1.0f32;
            let mut action_y: Option<f32> = None;
            fn walk(
                nodes: &[crate::layout::node::LayoutNode],
                idx: usize,
                y: f32,
                parent_h: f32,
                bar_h: &mut f32,
                action: &mut Option<f32>,
            ) {
                let n = &nodes[idx];
                let ay = y + n.position.y;
                if n.measured_size.height == 80.0 && n.measured_size.width > 300.0 {
                    *bar_h = parent_h;
                }
                if n.measured_size.width == 56.0 && n.measured_size.height == 56.0 {
                    *action = Some(ay);
                }
                for &child in &n.children {
                    walk(nodes, child, ay, n.measured_size.height, bar_h, action);
                }
            }
            let root = c.layout_root_idx().unwrap();
            walk(c.arena_nodes(), root, 0.0, -1.0, &mut bar_h, &mut action_y);
            (bar_h, action_y)
        };
        build(&mut composer, false);
        assert_eq!(probe(&composer).0, 80.0, "visible: the bar's container is full height");
        assert_eq!(probe(&composer).1, Some(548.0), "action 16dp above the 80dp bar");

        state.hide();
        build(&mut composer, true);
        // The tween settles within its threshold, so the container keeps a sliver rather than an exact
        // zero — the tolerance is the engine's, not the layout's.
        assert!(
            probe(&composer).0 < 4.0,
            "hidden: the bar's container has animated away, got {}",
            probe(&composer).0
        );
        let y = probe(&composer).1.expect("the action is still there when the suite is hidden");
        assert!(
            (y - 628.0).abs() < 4.0,
            "and it came down with the bar: 16dp from the bottom edge, got {y}"
        );

        state.show();
        build(&mut composer, true);
        assert!(
            probe(&composer).0 > 76.0,
            "shown again, got {}",
            probe(&composer).0
        );
        let y = probe(&composer).1.expect("the action is still there");
        assert!((y - 548.0).abs() < 4.0, "and it went back up with the bar, got {y}");
    }

    /// `confirmValueChange`: a vetoed request changes nothing (upstream's hook, same contract).
    #[test]
    fn a_veto_keeps_the_suite_visible() {
        let vetoing = NavigationSuiteScaffoldState::with_confirm(
            NavigationSuiteScaffoldValue::Visible,
            |target| target == NavigationSuiteScaffoldValue::Visible,
        );
        vetoing.hide();
        assert!(vetoing.is_visible(), "the veto held");
        assert_eq!(vetoing.current_value(), NavigationSuiteScaffoldValue::Visible);
    }

    /// 窄窗 → 底栏形态：Column 根，bar 在底部（高 80、贴底）
    #[test]
    fn narrow_window_renders_bottom_bar() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(500.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // Stack 根（primary action 的宿主）→ Column → 最后子项 = bar（高 80、y = 700-80 = 620）
        let column = &nodes[*nodes[root].children.last().unwrap()];
        let bar = &nodes[*column.children.last().unwrap()];
        assert_eq!(bar.measured_size.height, 80.0);
        assert!((bar.position.y - 620.0).abs() < 0.01, "bar 贴底 y={}", bar.position.y);
    }

    /// 宽窗 → **收起**宽轨（上游默认从不展开）：Row 根，rail 在前（宽 96），内容填余下
    #[test]
    fn wide_window_renders_a_collapsed_rail() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = &nodes[root].children;
        // Row 根：首子 = rail（宽 220），末子 = content（填余下 780）
        let rail = &nodes[children[0]];
        let content = &nodes[children[children.len() - 1]];
        assert_eq!(rail.measured_size.width, 96.0, "收起轨宽 96");
        assert!((content.measured_size.width - 904.0).abs() < 0.01,
            "内容填余下 width={}", content.measured_size.width);
    }

    /// 中宽窗 → 收起轨（96）
    #[test]
    fn medium_window_renders_collapsed_rail() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(700.0, 700.0);
        let mut composer = Composer::new();
        composer.compose(|ctx| scaffold().build(ctx));
        composer.layout(Constraints::new(0.0, 700.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let rail = &nodes[nodes[root].children[0]];
        assert_eq!(rail.measured_size.width, 96.0, "收起轨宽 96");
    }

    /// 尺寸 State 驱动形态切换（resize 响应式通路——app 层 set() → 依赖方重组）
    #[test]
    fn window_size_state_drives_suite_type() {
        crate::layout::adaptive::reset_window_size_state();
        let size = crate::runtime::state::State::new((500.0f32, 700.0f32));
        crate::layout::adaptive::set_window_size_state(size.clone());
        assert_eq!(navigation_suite_type(), NavigationSuiteType::ShortNavigationBarCompact);
        size.set((1000.0, 700.0));
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        size.set((700.0, 700.0));
        assert_eq!(navigation_suite_type(), NavigationSuiteType::WideNavigationRailCollapsed);
        crate::layout::adaptive::reset_window_size_state();
    }

    /// 显式 layout_type 覆盖自动推算
    #[test]
    fn explicit_layout_type_overrides_auto() {
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(1000.0, 700.0); // 自动会选展开轨
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            scaffold().layout_type(NavigationSuiteType::None).build(ctx);
        });
        composer.layout(Constraints::new(0.0, 1000.0, 0.0, 700.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // None：根 = content 容器（仅 1 个内容子节点——无 rail/bar 子树）
        assert_eq!(nodes[root].children.len(), 1, "None 形态仅 content，无导航子树");
    }

    /// morph policy：容器尺寸 = 子尺寸 × progress（宽度轴），子全尺寸原点放置。
    /// progress 须在组合内创建（owner queue——set 通知才能路由到 layout 重测）
    #[test]
    fn morph_policy_scales_container_by_progress() {
        let holder: std::cell::RefCell<Option<State<f32>>> = std::cell::RefCell::new(None);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let p = ctx.key("morph_progress", |ctx| {
                    let existing = holder.borrow().clone();
                    existing.unwrap_or_else(|| {
                        let s = ctx.remember(|| 0.5f32);
                        *holder.borrow_mut() = Some(s.clone());
                        s
                    })
                });
                let k = ctx.next_key();
                let policy = SuiteMorphPolicy { progress: p, axis: MorphAxis::Width };
                match ctx.start_restartable_group(
                    k,
                    M::new().clip(Shape::Rectangle),
                    policy,
                ) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        icon(ctx, 24.0);
                    }
                }
                ctx.end_restartable_group();
            });
            composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
        };
        build(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // 子项 = 24x24 图标：容器宽 = 24 × progress
        assert!((nodes[root].measured_size.width - 12.0).abs() < 0.01,
            "progress 0.5 → 容器宽 = 24x0.5，实际 {}", nodes[root].measured_size.width);
        assert!((nodes[root].measured_size.height - 24.0).abs() < 0.01,
            "高度轴不缩放（宽度轴 morph）");

        // set 通知 → pending → layout 重测（layout_deps 通路）
        holder.borrow().as_ref().unwrap().set(0.25);
        build(&mut composer);
        let nodes = composer.arena_nodes();
        assert!((nodes[root].measured_size.width - 6.0).abs() < 0.01,
            "progress 0.25 → 容器宽 6，实际 {}", nodes[root].measured_size.width);
    }

    /// 异形切换（rail→bar）：收拢（rail 宽度收缩）→ 换形 → 展开（bar 高度生长）
    #[test]
    fn suite_type_switch_collapses_then_expands() {
        crate::layout::adaptive::reset_window_size_state();
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, lt: NavigationSuiteType| {
            composer.compose(|ctx| {
                NavigationSuiteScaffold::new(
                    |items| {
                        items.item(
                            true,
                            |ctx| icon(ctx, 24.0),
                            |ctx| crate::components::Text::new("A").build(ctx),
                            || {},
                        );
                    },
                    |ctx| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, M::new());
                        ctx.end_node();
                    },
                )
                .layout_type(lt)
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 900.0, 0.0, 700.0));
        };

        build(&mut composer, NavigationSuiteType::WideNavigationRailExpanded);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[nodes[root].children[0]].measured_size.width, 220.0, "初始展开轨");

        // 切换 → Collapsing：rail 宽度开始收缩（< 220）
        build(&mut composer, NavigationSuiteType::ShortNavigationBarCompact);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut saw_shrinking = false;
        let mut saw_bar = false;
        loop {
            crate::animation::update_animations();
            build(&mut composer, NavigationSuiteType::ShortNavigationBarCompact);
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let column = &nodes[*nodes[root].children.last().unwrap()];
            let bar = &nodes[*column.children.last().unwrap()];
            if bar.measured_size.height == 80.0 && nodes[root].children.len() >= 1 {
                // bar 完整出现（高 80 贴底）→ 过渡完成
                saw_bar = true;
                break;
            }
            // 收拢期：rail 分支仍在，宽度应小于全宽
            if nodes[root].measured_size.width > 0.0 {
                let first = &nodes[nodes[root].children[0]];
                if first.measured_size.width < 220.0 && first.measured_size.width > 0.0 {
                    saw_shrinking = true;
                }
            }
            if std::time::Instant::now() > deadline {
                panic!("过渡 1.5s 未完成");
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        assert!(saw_shrinking, "应观察到 rail 收缩中间帧（非跳变）");
        assert!(saw_bar, "过渡完成应渲染底栏");
    }
