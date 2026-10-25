    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::components::sheet_state::SheetValue;
    use crate::unit::Density;

    #[test]
    fn the_peek_anchor_is_the_token_in_logical_px() {
        // `sheetPeekHeight` reaches the anchors as a height, so it must be a logical length.
        // The bug this pins converted it with `Dp::to_px`, which at 1.5x asked for 84 logical
        // px instead of 56 — the sheet peeked 1.5x too far and the content reserved 1.5x too
        // much space for it.
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        let st = state.clone();
        let mut c = Composer::new();
        // The density has to cover the LAYOUT as well as the compose: the sheet's
        // `on_size_changed` callback runs during measure (as it does in the app, where
        // `with_density` wraps both phases), and a compose-only wrapper left that callback
        // reading density 1.0 — where `to_px` happens to equal the correct value, hiding a
        // regression in exactly the site this test is meant to catch.
        crate::runtime::density::with_density(Density::from_density(1.5), || {
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st.clone())
                    .sheet_peek_height(SCAFFOLD_SHEET_PEEK_HEIGHT)
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("content").build(ctx); },
                    );
            });
            // The anchors are computed in the LAYOUT now (`ScaffoldStackPolicy` — Compose's own place for them,
            // `val layoutHeight = constraints.maxHeight`), so a first build no longer touches them and this is
            // where the peek's unit shows up first.
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            assert_eq!(
                state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded),
                720.0 - SCAFFOLD_SHEET_PEEK_HEIGHT.value(),
                "the layout's anchors use the peek as a logical length"
            );
        });
        // The peek reaches the layout three ways, so all three are asserted: the first-frame
        // anchors (before layout), the ones the sheet's own size callback re-computes, and the
        // space the content reserves for the collapsed sheet. A single-site regression has to
        // fail here — the first version of this test only checked the anchor, which the size
        // callback overwrites, so re-introducing the bug at the initialisation site left it
        // green.
        let expected_anchor = 720.0 - SCAFFOLD_SHEET_PEEK_HEIGHT.value();
        assert_eq!(
            state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded),
            expected_anchor,
            "the sheet's size callback (during measure) uses the peek as a logical length"
        );

        // Structure: root -> page Column -> the weighted body Column -> the peek-inset Stack -> content.
        let root = c.layout_root_idx().expect("laid out");
        let nodes = c.arena_nodes();
        let page = nodes[root].children[0];
        let body = nodes[page].children[0];
        let inset = nodes[body].children[0];
        assert_eq!(
            nodes[inset].modifier.get_padding_vertical().1,
            SCAFFOLD_SHEET_PEEK_HEIGHT.value(),
            "the content reserves exactly one peek at the bottom, in logical px"
        );
    }

    /// A sheet whose CONTENT changes height must re-anchor: recomputing the anchors used to live in the sheet's
    /// own `on_size_changed` (which runs inside the measure, so it always saw the new height), and that job moved
    /// to `ScaffoldStackPolicy`. This pins that the height still reaches the anchors when nothing but the content
    /// changed — the Expanded anchor is `layoutHeight - sheetHeight`, so growth must lower it by the same amount.
    #[test]
    fn a_content_height_change_moves_the_expanded_anchor() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let tall = State::new(false);
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        fn frame(c: &mut Composer, state: &SheetState, tall: &State<bool>) {
            let st = state.clone();
            let tall = tall.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st.clone())
                    .build(
                        ctx,
                        {
                            let tall = tall.clone();
                            move |ctx| {
                                let h = if tall.get() { 300.0 } else { 120.0 };
                                crate::layout::components::Column::new()
                                    .modifier(Modifier::new().fill_max_width().height(h))
                                    .build(ctx, |_| {});
                            }
                        },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        for _ in 0..3 {
            frame(&mut c, &state, &tall);
        }
        let short = state.anchored_draggable().position_of(&SheetValue::Expanded);
        tall.set(true);
        for _ in 0..3 {
            frame(&mut c, &state, &tall);
        }
        let grown = state.anchored_draggable().position_of(&SheetValue::Expanded);
        assert!(
            (short - grown - 180.0).abs() < 1.0,
            "growing the content by 180 must lower the Expanded anchor by 180 ({short} -> {grown})"
        );
        crate::animation::clear_all_animations();
    }

    /// Compose forces the sheet to be at least as tall as its peek — `Modifier.requiredHeightIn(min =
    /// peekHeight)` on the Surface (`BottomSheetScaffold.kt:262-264`). Without it a content shorter than
    /// the peek leaves the sheet just as tall as that content while the anchor still puts its TOP at
    /// `layoutHeight - peek`, so the strip between the sheet's bottom edge and the window's bottom edge
    /// shows the page underneath - the collapsed sheet looks like it is floating.
    #[test]
    fn a_short_sheet_still_reaches_its_peek_height() {
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            BottomSheetScaffold::new()
                .sheet_state(st.clone())
                .sheet_peek_height(Dp(96.0))
                .sheet_drag_handle(false)
                .build(
                    ctx,
                    |ctx| {
                        crate::components::Text::new("s").font_size(8.0).build(ctx);
                    },
                    |ctx| {
                        crate::components::Text::new("page").build(ctx);
                    },
                );
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        let root = c.layout_root_idx().expect("laid out");
        let sheet = c.arena_nodes()[root].children[1];
        assert_eq!(
            c.arena_nodes()[sheet].measured_size.height, 96.0,
            "the sheet is at least as tall as its peek (its content is 11)"
        );
        assert_eq!(
            c.arena_nodes()[sheet].position.y, 720.0 - 96.0,
            "so the collapsed sheet covers the window's bottom edge, with no page showing under it"
        );
    }

    /// Compose measures the top bar FIRST and hands the body `maxHeight = layoutHeight - topBarHeight`
    /// (`BottomSheetScaffold.kt:427-431`) before placing the body under the bar. Structurally the same here:
    /// a Column whose body is weighted takes what the bar leaves, and the body is placed below it.
    #[test]
    fn a_top_bar_takes_its_height_before_the_body() {
        fn body_geometry(with_bar: bool) -> (f32, f32) {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                let mut scaffold = BottomSheetScaffold::new();
                if with_bar {
                    scaffold = scaffold.top_bar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().fill_max_width().height(40.0))
                            .build(ctx, |_| {});
                    });
                }
                scaffold.build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().fill_max_size().test_tag("body"))
                            .build(ctx, |_| {});
                    },
                );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let body = node_with_tag(nodes, "body");
            let (_, y) = absolute_position(nodes, root, body).expect("the body is under the root");
            (y, nodes[body].measured_size.height)
        }

        // The tagged node lives inside the peek-inset Stack, so its height is the scaffold's minus the
        // 56 px the collapsed sheet reserves — what matters here is the 40 px the bar takes off the top.
        let (y_without, h_without) = body_geometry(false);
        assert_eq!(
            (y_without, h_without),
            (0.0, 720.0 - 56.0),
            "with no bar the body starts at the top and is only inset by the peek"
        );
        let (y_with, h_with) = body_geometry(true);
        assert_eq!(y_with, 40.0, "the body starts under the top bar");
        assert_eq!(h_with, 720.0 - 40.0 - 56.0, "and gets exactly what the bar left");
    }

    /// A node's absolute position: the arena stores each position relative to its parent, so a test that
    /// wants to know where a nested node ended up has to walk down from the root adding them up.
    fn absolute_position(
        nodes: &[crate::layout::node::LayoutNode],
        root: usize,
        target: usize,
    ) -> Option<(f32, f32)> {
        if root == target {
            return Some((0.0, 0.0));
        }
        for &child in &nodes[root].children {
            if let Some((x, y)) = absolute_position(nodes, child, target) {
                return Some((nodes[child].position.x + x, nodes[child].position.y + y));
            }
        }
        None
    }

    /// The index of the node carrying `tag`.
    fn node_with_tag(nodes: &[crate::layout::node::LayoutNode], tag: &str) -> usize {
        nodes
            .iter()
            .position(|n| {
                n.modifier.elements().iter().any(|el| {
                    matches!(el, crate::modifier::ModifierElement::TestTag { tag: t } if t == tag)
                })
            })
            .unwrap_or_else(|| panic!("no node tagged {tag}"))
    }

    /// The snackbar sits centred, just above the sheet while it is partially expanded, and on the bottom
    /// edge otherwise — Compose's rule (`BottomSheetScaffold.kt:441-446`). Compose can do it in one pass
    /// because its layout measures every child before placing any; here the snackbar's own height comes from
    /// the previous frame (the documented one-frame caveat), so the helper below composes and lays out
    /// twice, as a real app does.
    #[test]
    fn the_snackbar_sits_above_the_sheet_until_it_is_expanded() {
        // It drives a settle animation, so it holds the animation lock like every other such test.
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        /// One frame of the scaffold: compose, then lay out.
        /// `pad` gives the slot its own vertical padding, the way a real snackbar host does — Compose measures
        /// the host INCLUDING that padding, so the padded box's bottom lands on the sheet's top edge.
        fn frame(c: &mut Composer, state: &SheetState, pad: f32) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(move |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .padding_vertical(pad)
                                    .size(200.0, 48.0)
                                    .test_tag("snack"),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }

        fn snackbar_y(state: &SheetState, settle_expanded: bool) -> f32 {
            let mut c = Composer::new();
            frame(&mut c, state, 0.0);
            if settle_expanded {
                state.expand();
                let start = std::time::Instant::now();
                while start.elapsed() < std::time::Duration::from_millis(600) {
                    crate::animation::update_animations();
                    std::thread::sleep(std::time::Duration::from_millis(8));
                    if (state.offset()
                        - state.anchored_draggable().position_of(&SheetValue::Expanded))
                    .abs()
                        < 0.5
                    {
                        break;
                    }
                }
            }
            // A second frame: the snackbar's height has been measured by now, so the rule can place it.
            frame(&mut c, state, 0.0);
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let snack = node_with_tag(nodes, "snack");
            absolute_position(nodes, root, snack)
                .expect("the snackbar is under the root")
                .1
        }

        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let peek_y = snackbar_y(&state, false);
        assert_eq!(
            peek_y,
            720.0 - 56.0 - 48.0,
            "partially expanded: the snackbar's bottom edge sits on the sheet's top (the default 56 px \
             peek, minus its own 48)"
        );

        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let expanded_y = snackbar_y(&state, true);
        assert_eq!(expanded_y, 720.0 - 48.0, "expanded: it drops to the bottom edge");
    }

    /// A scaffold in a box that is NOT the window must anchor from that box. Compose's scaffold layout takes
    /// `layoutHeight` from the constraints it is given (`BottomSheetScaffold.kt:275-276`); before `4c16daf`
    /// winia read `crate::layout::window_size().1` in the build and only converged once the sheet's own
    /// `on_size_changed` had reported a height, so the panel started on the window's anchor and took about 25
    /// frames to reach its own.
    ///
    /// The assertion is therefore made on the FIRST layout, not on a settled offset: a polling version of this
    /// test passed on the pre-`4c16daf` sources as well (measured there: `early_partial_anchor=664`,
    /// `frames_to_344=25`), which made it pin nothing.
    #[test]
    fn a_nested_scaffold_anchors_from_its_own_height() {
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        /// One frame: compose and lay out. The frame matters because the anchors are computed in the layout.
        fn frame(c: &mut Composer, state: &SheetState) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                // A box that is SHORTER than the window: the case where the scaffold's own height and the
                // window's differ. (The settings shell cannot show it — its body takes the whole window, so
                // there the two are equal and the difference is invisible.)
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width().height(400.0))
                    .build(ctx, move |ctx| {
                        BottomSheetScaffold::new()
                            .sheet_state(st.clone())
                            .build(
                                ctx,
                                |ctx| { crate::components::Text::new("sheet").build(ctx); },
                                |ctx| { crate::components::Text::new("page").build(ctx); },
                            );
                    });
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        frame(&mut c, &state);
        // Its own height is 400, so the partial anchor is 400 - 56 = 344; the window's would be 664.
        let first = state.anchored_draggable().position_of(&SheetValue::PartiallyExpanded);
        assert!(
            (first - 344.0).abs() < 0.5,
            "the FIRST layout's anchors use the scaffold's own height, not the window's ({first} was used)"
        );
        // And the sheet settles onto that anchor.
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(600) {
            frame(&mut c, &state);
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(8));
            if (state.offset() - 344.0).abs() < 0.5 {
                break;
            }
        }
        assert!(
            (state.offset() - 344.0).abs() < 0.5,
            "the panel settles on its own anchor ({} was reached)",
            state.offset()
        );
        crate::animation::clear_all_animations();
    }

    /// A slot that pads ITSELF must be placed by its padded extent: Compose measures whatever host it is given,
    /// and M3's `SnackbarHost` carries its margins inside that host.
    ///
    /// This used to be ignored: a chain of `padding_vertical(16).size(200, 48)` reported `200x48` where
    /// Compose reports `200x80`, because the node subtracted the whole chain's padding AFTER the size
    /// walk. Padding is a chain step now (`SizeStep::Padding`, `docs/modifier-node.md`), so the chain
    /// measures 80 and the slot is placed by its padded extent — the component's rule is exact for both
    /// an unpadded and a padded slot.
    #[test]
    fn a_snackbar_slot_that_pads_itself_is_measured_with_the_padding() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        fn frame(c: &mut Composer, state: &SheetState) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .padding_vertical(16.0)
                                    .size(200.0, 48.0)
                                    .test_tag("snack"),
                            )
                            .build(ctx, |_| {});
                    })
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        frame(&mut c, &state);
        frame(&mut c, &state);
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let card = node_with_tag(nodes, "snack");
        let y = absolute_position(nodes, root, card).expect("under the root").1;
        // The slot's BOX is the padded extent (48 + 16 + 16 = 80) and it is placed by that box: its
        // bottom sits on the sheet's top (720 - 56), so its top is at 584. The card's content starts
        // 16 px inside it, at 600 — that half is the slot's own padding, not this component's rule.
        assert_eq!(
            y,
            720.0 - 56.0 - 80.0,
            "the slot is placed by its PADDED extent"
        );
    }

    /// The demo's own slot shape, to bisect where its displacement comes from: a padded Stack holding a
    /// Column holding a Row that carries the card's background, padding, a Text and a Button. The card must sit
    /// exactly where the measured slot puts it — the same relation the plain-Column test above pins.
    #[test]
    fn the_demo_shaped_snackbar_slot_is_placed_by_its_measured_height() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            BottomSheetScaffold::new()
                .sheet_state(st)
                .snackbar(|ctx| {
                    let colors = crate::theme::WiniaTheme::colors();
                    crate::layout::components::Stack::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0).test_tag("snack-slot"))
                        .build(ctx, |ctx| {
                            crate::layout::components::Row::new()
                                .modifier(
                                    Modifier::new()
                                        .fill_max_width()
                                        .background(colors.surface_container_highest, crate::graphics::Shape::rounded(8.0))
                                        .padding(12.0)
                                        .test_tag("snack-card"),
                                )
                                .spacing(12.0)
                                .build(ctx, |ctx| {
                                    crate::components::Text::new("saved").font_size(13.0).build(ctx);
                                    crate::components::Button::text()
                                        .on_click(|| {})
                                        .build(ctx, |ctx| crate::components::Text::new("undo").build(ctx));
                                });
                        });
                })
                .build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
        });
        let constraints = Constraints::new(0.0, 480.0, 0.0, 720.0);
        for _ in 0..3 {
            c.layout(constraints);
        }
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let slot = node_with_tag(nodes, "snack-slot");
        let card = node_with_tag(nodes, "snack-card");
        let slot_h = nodes[slot].measured_size.height;
        let slot_y = absolute_position(nodes, root, slot).expect("under the root").1;
        let card_y = absolute_position(nodes, root, card).expect("under the root").1;
        assert_eq!(
            slot_y,
            720.0 - 56.0 - slot_h,
            "the slot's padded box rests on the panel's top edge"
        );
        assert_eq!(
            card_y,
            slot_y + 16.0,
            "and the card starts inside it, below the slot's own 16 px of padding"
        );
        crate::animation::clear_all_animations();
    }

    /// The layout half of the snackbar's placement: an `absolute_offset` whose value is computed AT LAYOUT
    /// TIME (a closure, the same `Dimension::Dynamic` path the snackbar uses) must move the NODE, not merely
    /// the number the build computed. Written while chasing the demo, where the rule's `y=528` and the drawn
    /// card disagreed by ~90 px: this says whether the offset reaches the arena at all.
    ///
    /// A CHARACTERISATION test, not a discriminator: `Modifier::get_absolute_offset` already resolved
    /// `Dimension::Dynamic(f) => f()` before any of the sheet commits (`winia/src/modifier.rs`), so no revert of
    /// theirs can fail it. It pins the behaviour the snackbar's rule depends on, nothing more.
    #[test]
    fn a_layout_time_absolute_offset_moves_the_node() {
        fn placed_with(offset: crate::layout::sizing::Dimension) -> f32 {
            let mut c = Composer::new();
            let mut slot = Some(offset);
            c.compose(move |ctx| {
                let value = slot.take().expect("one frame");
                crate::layout::components::Stack::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, move |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .absolute_offset(0.0, value)
                                    .size(100.0, 50.0)
                                    .test_tag("moved"),
                            )
                            .build(ctx, |_| {});
                    });
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            absolute_position(nodes, root, node_with_tag(nodes, "moved"))
                .expect("under the root")
                .1
        }

        assert_eq!(placed_with(300.0f32.into()), 300.0, "a literal offset moves the node");
        assert_eq!(
            placed_with(crate::layout::sizing::Dimension::Dynamic(std::sync::Arc::new(|| 200.0))),
            200.0,
            "and so does one computed at layout time"
        );
    }

    /// The snackbar's offset must follow the slot's height WITHOUT a recompose in between: the slot is measured
    /// during the layout pass, so the frame that measures it is the frame that has to place it. Two layouts in a
    /// row with no `compose` between them is exactly that, and the demo is where it went wrong (rule `y=528`,
    /// drawn card ~90 px lower). This pins the layout half; the demo's remaining displacement is the renderer's.
    #[test]
    fn the_snackbar_offset_follows_a_height_measured_in_the_same_pass() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            // A box that is shorter than the window, so "the scaffold's own height" and "the window's" are
            // different numbers; the second phase below is what reads the difference.
            crate::layout::components::Column::new()
                .modifier(Modifier::new().fill_max_width().height(400.0))
                .build(ctx, move |ctx| {
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().padding_vertical(16.0))
                            .build(ctx, |ctx| {
                                crate::layout::components::Column::new()
                                    .modifier(Modifier::new().size(200.0, 48.0).test_tag("snack"))
                                    .build(ctx, |_| {});
                            });
                    })
                    .build(
                        ctx,
                        |ctx| {
                            // 200 tall, so `Expanded` is an anchor of its own rather than the partial one.
                            crate::layout::components::Column::new()
                                .modifier(Modifier::new().fill_max_width().height(200.0))
                                .build(ctx, |_| {});
                        },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
                });
        });
        let constraints = Constraints::new(0.0, 480.0, 0.0, 720.0);
        c.layout(constraints);
        c.layout(constraints);
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let card = node_with_tag(nodes, "snack");
        let y = absolute_position(nodes, root, card).expect("under the root").1;
        // The slot measures 80 (a 48 px card with 16 px of padding on both sides), the sheet's partial anchor
        // is at 400 - 56 = 344 (the scaffold's OWN height), so the slot's top is 264 and the card inside it
        // starts 16 lower.
        assert_eq!(
            y,
            400.0 - 56.0 - 80.0 + 16.0,
            "the card sits inside a slot placed by the height the same pass measured"
        );

        // The branch that actually READS the scaffold's own height is the expanded one (`h_self - snack_h`), so
        // a box shorter than the window is where the two diverge: 400 - 80, not 720 - 80. The content is 200
        // tall, so `Expanded` is a distinct anchor (400 - 200 = 200) and the rule really switches branches —
        // with content no taller than the peek the two anchors coincide and this could not be asserted. Freezing
        // the height at its build-time value, or taking it from the window, both fail this.
        state.expand();
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(600) {
            c.layout(constraints);
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(8));
            if (state.offset() - (400.0 - 200.0)).abs() < 0.5 {
                break;
            }
        }
        let nodes = c.arena_nodes();
        let root = c.layout_root_idx().expect("laid out");
        let card = node_with_tag(nodes, "snack");
        let expanded_y = absolute_position(nodes, root, card).expect("under the root").1;
        assert_eq!(
            state.value_for_offset(),
            SheetValue::Expanded,
            "the panel reached the expanded anchor"
        );
        assert_eq!(
            expanded_y,
            400.0 - 80.0 + 16.0,
            "expanded, the card rests on the box's bottom edge (its own 400), not the window's 720"
        );
        crate::animation::clear_all_animations();
    }

    /// While the sheet animates between two anchors, the snackbar stays on the panel's top edge until the
    /// panel has travelled past the MIDPOINT — Compose's rule reads `currentValue`, which is
    /// `anchors.closestAnchor(offset)` there (`AnchoredDraggableState.updateIfNeeded`). winia's
    /// `current_value` flips the moment `expand()` is called, so the snackbar used to jump to the window's
    /// bottom edge while the panel was still where it started (measured in a frame-loop probe: panel at 663.7
    /// of 664, snackbar already at the bottom).
    #[test]
    fn the_snackbar_waits_until_the_sheet_passes_the_midpoint() {
        let _serial =
            crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let mut c = Composer::new();
        fn frame(c: &mut Composer, state: &SheetState) {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_state(st)
                    .snackbar(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().size(200.0, 48.0).test_tag("snack"))
                            .build(ctx, |_| {});
                    })
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        }
        frame(&mut c, &state);
        frame(&mut c, &state);
        let collapsed_y = {
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let snack = node_with_tag(nodes, "snack");
            absolute_position(nodes, root, snack).expect("under the root").1
        };

        state.expand();
        // One frame: the tween has barely moved, so the sheet's position is still at the partial anchor.
        crate::animation::update_animations();
        frame(&mut c, &state);
        let y_after_one_frame = {
            let nodes = c.arena_nodes();
            let root = c.layout_root_idx().expect("laid out");
            let snack = node_with_tag(nodes, "snack");
            absolute_position(nodes, root, snack).expect("under the root").1
        };
        assert!(
            (y_after_one_frame - collapsed_y).abs() < 1.0,
            "the snackbar stays where it was while the panel is still at its starting anchor \
             ({y_after_one_frame} against {collapsed_y})"
        );
        crate::animation::clear_all_animations();
    }

    /// `sheetShape` and composable `sheetDragHandle`.
    #[test]
    fn an_explicit_shape_and_a_supplied_handle_replace_the_built_ins() {
        let custom_shape = Shape::RoundedRect { corner_radius: 3.0 };

        fn sheet_background_shape(c: &Composer) -> Option<Shape> {
            c.arena_nodes()
                .iter()
                .find(|node| {
                    node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::AbsoluteOffset { .. })
                    })
                })
                .and_then(|node| {
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { shape, .. } => {
                            Some(shape.eval())
                        }
                        _ => None,
                    })
                })
        }

        fn frame(with_custom_shape: bool, with_custom_handle: bool, shape: Option<Shape>) -> Composer {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                let mut scaffold = BottomSheetScaffold::new();
                if with_custom_shape {
                    scaffold = scaffold.sheet_shape(shape.clone().expect("a shape was passed"));
                }
                if with_custom_handle {
                    scaffold = scaffold.sheet_drag_handle_content(|ctx| {
                        crate::layout::components::Column::new()
                            .modifier(Modifier::new().size(80.0, 6.0).test_tag("custom-handle"))
                            .build(ctx, |_| {});
                    });
                }
                scaffold.build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            c
        }

        let default_shape = frame(false, false, None);
        assert!(
            matches!(
                sheet_background_shape(&default_shape),
                Some(Shape::TopRoundedRect { .. })
            ),
            "the built-in shape stays a top-rounded rectangle"
        );
        let explicit = frame(true, false, Some(custom_shape.clone()));
        assert_eq!(
            sheet_background_shape(&explicit),
            Some(custom_shape.clone()),
            "an explicit shape replaces it"
        );

        // The built-in handle is a 32x4 node with the outline-variant fill; the supplied one carries a tag
        // and that size is gone.
        let builtin = frame(false, false, None);
        let has_32x4 = |c: &Composer| {
            c.arena_nodes().iter().any(|n| {
                n.modifier.elements().iter().any(|el| match el {
                    crate::modifier::ModifierElement::Size { width, height } => {
                        let fixed = |v: &crate::layout::sizing::SizeValue| match v {
                            crate::layout::sizing::SizeValue::Length(
                                crate::layout::sizing::Dimension::Fixed(px),
                            ) => *px,
                            _ => f32::NAN,
                        };
                        fixed(width) == 32.0 && fixed(height) == 4.0
                    }
                    _ => false,
                })
            })
        };
        assert!(has_32x4(&builtin), "the built-in handle is drawn by default");

        let custom = frame(false, true, None);
        node_with_tag(custom.arena_nodes(), "custom-handle");
        assert!(
            !has_32x4(&custom),
            "a supplied handle replaces the built-in one"
        );
    }

    /// The handle's numbers are the tokens Compose names: `DockedDragHandleWidth/Height` 32x4,
    /// `DragHandleVerticalPadding` 22 (`SheetDefaults.kt:410-431`, `:543`) and its colour
    /// `DockedDragHandleColor` = `OnSurfaceVariant` (`tokens/SheetBottomTokens.kt`). winia used a 12 px
    /// vertical padding and `outline_variant`, which made the collapsed sheet 20 px shorter than Compose's
    /// and painted the bar a dimmer grey.
    #[test]
    fn the_drag_handle_numbers_are_the_tokens() {
        let theme = crate::theme::WiniaTheme::colors();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                BottomSheetScaffold::new().build(
                    ctx,
                    |ctx| { crate::components::Text::new("sheet").build(ctx); },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
            });
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        let nodes = c.arena_nodes();
        let bar = nodes
            .iter()
            .position(|n| {
                n.modifier.elements().iter().any(|el| match el {
                    crate::modifier::ModifierElement::Size { width, height } => {
                        let fixed = |v: &crate::layout::sizing::SizeValue| match v {
                            crate::layout::sizing::SizeValue::Length(
                                crate::layout::sizing::Dimension::Fixed(px),
                            ) => *px,
                            _ => f32::NAN,
                        };
                        fixed(width) == 32.0 && fixed(height) == 4.0
                    }
                    _ => false,
                })
            })
            .expect("the built-in 32x4 bar");
        let colour = nodes[bar]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color, .. } => Some(color.eval()),
                _ => None,
            })
            .expect("the bar paints a background");
        assert_eq!(colour, theme.on_surface_variant, "DockedDragHandleColor is onSurfaceVariant");

        // 4 px of bar plus 22 above and below.
        let row = nodes
            .iter()
            .find(|n| n.children.contains(&bar))
            .expect("the bar's row");
        assert_eq!(
            row.measured_size.height,
            4.0 + 2.0 * SCAFFOLD_DRAG_HANDLE_VERTICAL_PADDING,
            "the handle's box is the bar plus DragHandleVerticalPadding on both sides"
        );
    }

    /// The WHEEL is user input, exactly like a finger: Compose dispatches it with
    /// `NestedScrollSource.UserInput` (`MouseWheelScrollingLogic.kt:303-307`), and the sheet's connection
    /// accepts user input (`SheetDefaults.kt:454-470`). Watch the sign: a NEGATIVE `y` means the content moves
    /// up — a finger dragging up, or the wheel rotated DOWN — and that is the delta which expands the sheet
    /// first; a positive one reaches the sheet only with what the list could not use. Neither marks the sheet
    /// as dragging, because a nested delta is not the panel's own gesture. `Fling` and `SideEffect` must not
    /// move the sheet at all (no production call site passes them today; the guard is insurance and matches M3).
    #[test]
    fn a_wheel_drives_the_sheet_like_a_finger_and_a_fling_does_not() {
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        state.set_skip_hidden_state(true);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            let scroll = ctx.remember(|| crate::layout::LazyListState::new()).get();
            BottomSheetScaffold::new()
                .sheet_state(st.clone())
                .build(
                    ctx,
                    move |ctx| {
                        // A scrollable inside the sheet: this is what the nested-scroll chain hands the
                        // leftover delta to (`SheetNested`).
                        let rows = ctx.remember(|| std::sync::Arc::new((0..60).map(|i| (i as u64, format!("row {i}"))).collect::<Vec<_>>())).get();
                        crate::layout::LazyColumn::new()
                            .state(scroll.clone())
                            .modifier(Modifier::new().test_tag("sheet-scroll").fill_max_width())
                            .items_from(rows.clone(), |(id, _)| *id, |ctx, _i, (_, label)| {
                                crate::components::Text::new(label.clone())
                                    .modifier(Modifier::new().fill_max_width().height(40.0))
                                    .build(ctx);
                            })
                            .build(ctx);
                    },
                    |ctx| { crate::components::Text::new("page").build(ctx); },
                );
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        assert!(!state.anchored_draggable().is_dragging(), "nothing drags yet");

        let root = c.layout_root_idx().expect("laid out");
        let target = node_with_tag(c.arena_nodes(), "sheet-scroll");
        let density = crate::unit::Density::from_density(1.0);
        let dispatch = |c: &mut Composer, delta: f32, source| {
            crate::app::scroll::dispatch_nested_scroll_delta(
                c.arena_nodes_mut(),
                root,
                target,
                crate::nested_scroll::ScrollDelta::new(0.0, delta),
                source,
                density,
            )
        };

        // An upward wheel expands the sheet, like a finger (expands-first: the list gets nothing yet).
        let before = state.offset();
        dispatch(&mut c, -20.0, crate::nested_scroll::NestedScrollSource::Wheel);
        assert!(
            (state.offset() - (before - 20.0)).abs() < 0.5,
            "an upward wheel moves the sheet ({} against {})",
            state.offset(),
            before - 20.0
        );
        assert!(
            !state.anchored_draggable().is_dragging(),
            "a nested delta is not the panel's own gesture, so the re-target gate stays open"
        );

        // A FLING is a side effect: it must not move the sheet.
        let settled = state.offset();
        dispatch(&mut c, -20.0, crate::nested_scroll::NestedScrollSource::Fling);
        assert_eq!(state.offset(), settled, "a fling does not move the sheet");
    }

    /// The sheet's shadow follows `sheetShadowElevation` (Compose's default is `ElevationTokens.Level1`,
    /// 1 dp) — zero asks for no shadow at all, which `Modifier::shadow` drops.
    #[test]
    fn the_sheet_shadow_follows_its_elevation() {
        fn sheet_shadow_count(elevation: f32) -> usize {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_shadow_elevation(Dp(elevation))
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            c.arena_nodes()
                .iter()
                .filter(|node| {
                    node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::AbsoluteOffset { .. })
                    })
                })
                .map(|node| {
                    usize::from(
                        node.modifier
                            .graphics_layer_params()
                            .is_some_and(|p| p.shadow_elevation > 0.0),
                    )
                })
                .max()
                .unwrap_or(0)
        }
        // `Modifier::shadow` puts the elevation on a graphics layer now.
        assert_eq!(sheet_shadow_count(1.0), 1, "the default elevation draws a shadow");
        assert_eq!(sheet_shadow_count(0.0), 0, "zero elevation draws none");
    }

    /// The sheet's content colour follows its container, as Compose's
    /// `sheetContentColor = contentColorFor(sheetContainerColor)` does: a scheme container maps to its
    /// paired content colour, a custom one falls back to the ambient colour, and an explicit
    /// `sheet_content_color` wins. Before this, the panel simply inherited whatever the page provided.
    #[test]
    fn the_sheets_content_colour_follows_its_container() {
        let theme = crate::theme::WiniaTheme::colors();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Color>::new()));
        let observe = |sink: std::rc::Rc<std::cell::RefCell<Vec<Color>>>| {
            move |ctx: &mut ComposeCtx| {
                sink.borrow_mut().push(crate::theme::WiniaTheme::content_color());
            }
        };
        let mut c = Composer::new();
        let s1 = seen.clone();
        let s2 = seen.clone();
        let s3 = seen.clone();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(480.0, 720.0);
            crate::theme::WiniaTheme::with_content_color(
                theme.tertiary,
                ctx,
                move |ctx| {
                    // A scheme container: its pair decides, not the ambient colour.
                    BottomSheetScaffold::new()
                        .sheet_container_color(theme.surface_container_low)
                        .build(
                            ctx,
                            move |ctx| observe(s1.clone())(ctx),
                            |ctx| { crate::components::Text::new("page").build(ctx); },
                        );
                    // A colour that is not in the scheme: the ambient one is the fallback.
                    BottomSheetScaffold::new()
                        .sheet_container_color(Color::from_argb(255, 1, 2, 3))
                        .build(
                            ctx,
                            move |ctx| observe(s2.clone())(ctx),
                            |ctx| { crate::components::Text::new("page").build(ctx); },
                        );
                    // Explicit wins over both.
                    BottomSheetScaffold::new()
                        .sheet_container_color(theme.surface_container_low)
                        .sheet_content_color(Color::from_argb(255, 9, 9, 9))
                        .build(
                            ctx,
                            move |ctx| observe(s3.clone())(ctx),
                            |ctx| { crate::components::Text::new("page").build(ctx); },
                        );
                },
            );
        });
        c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
        let got = seen.borrow();
        assert_eq!(got.len(), 3, "three sheets composed");
        assert_eq!(got[0], theme.on_surface, "surfaceContainerLow -> onSurface");
        assert_eq!(got[1], theme.tertiary, "a custom container falls back to the ambient colour");
        assert_eq!(got[2], Color::from_argb(255, 9, 9, 9), "an explicit content colour wins");
    }

    /// The sheet's own background: the token colour by default, tinted by `surface_tint` when a tonal
    /// elevation is asked for AND the container is the scheme's `surface` (Compose's
    /// `surfaceColorAtElevation`), untinted otherwise.
    #[test]
    fn the_sheets_tonal_elevation_only_tints_a_surface_coloured_sheet() {
        let theme = crate::theme::WiniaTheme::colors();
        let painted = |container: Color, tonal: f32| -> Color {
            let mut c = Composer::new();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(480.0, 720.0);
                BottomSheetScaffold::new()
                    .sheet_container_color(container)
                    .sheet_tonal_elevation(Dp(tonal))
                    .build(
                        ctx,
                        |ctx| { crate::components::Text::new("sheet").build(ctx); },
                        |ctx| { crate::components::Text::new("page").build(ctx); },
                    );
            });
            c.layout(Constraints::new(0.0, 480.0, 0.0, 720.0));
            // The sheet's background is the first painted background belonging to a node that also
            // carries the sheet's own absolute offset.
            c.arena_nodes()
                .iter()
                .find_map(|node| {
                    let is_sheet = node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::AbsoluteOffset { .. })
                    });
                    if !is_sheet {
                        return None;
                    }
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { color, .. } => Some(color.eval()),
                        _ => None,
                    })
                })
                .expect("the sheet paints a background")
        };
        let alpha = ((4.5 * (3.0f32 + 1.0).ln()) + 2.0) / 100.0;
        assert_eq!(
            painted(theme.surface, 3.0),
            theme.surface.overlay(theme.surface_tint, alpha),
            "a surface-coloured sheet is tinted, with the published alpha"
        );
        assert_eq!(
            painted(theme.surface_container_low, 3.0),
            theme.surface_container_low,
            "any other container is left alone"
        );
        assert_eq!(painted(theme.surface, 0.0), theme.surface, "and zero elevation tints nothing");
    }

    #[test]
    fn the_centred_sheet_is_not_mirrored_in_rtl() {
        // `Modifier::offset` mirrors its x under RTL, but a centring inset is
        // direction-independent: the panel must sit `pad_x` from the LEFT edge in both
        // directions. The bug this pins put it at -pad_x in RTL (measured -130 for a 640-wide
        // sheet in a 900-wide window, i.e. 130px clipped off the left and 130px of dead space
        // at the right). It was unreachable before the `to_logical` fix only because a HiDPI
        // window used to clamp the sheet to its full width, leaving pad_x at 0.
        let state = SheetState::new(SheetValue::PartiallyExpanded);
        let st = state.clone();
        let mut c = Composer::new();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(900.0, 720.0);
            crate::theme::WiniaTheme::with_theme_and_direction(
                crate::theme::WiniaTheme::colors(),
                crate::layout::LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    BottomSheetScaffold::new()
                        .sheet_state(st.clone())
                        .build(
                            ctx,
                            |ctx| { crate::components::Text::new("sheet").build(ctx); },
                            |ctx| { crate::components::Text::new("content").build(ctx); },
                        );
                },
            );
        });
        c.layout(Constraints::new(0.0, 900.0, 0.0, 720.0));
        // The sheet is the second child of the root — the page is the first. Found by
        // position rather than by width: a width match is ambiguous (the page is just as
        // wide) and stops matching when the window is narrower than the token.
        let root = c.layout_root_idx().expect("laid out");
        let sheet = c.arena_nodes()[root].children[1];
        assert_eq!(
            c.arena_nodes()[sheet].measured_size.width, 640.0,
            "the sheet is 640 wide in a 900-wide window (the token, not the window)"
        );
        assert_eq!(
            c.arena_nodes()[sheet].position.x, 130.0,
            "and it is centred from the left edge in RTL too, not mirrored to -130"
        );
    }
