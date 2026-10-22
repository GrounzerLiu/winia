    use super::*;

    /// The constants, against both token files.
    #[test]
    fn the_button_group_constants_are_the_tokens() {
        assert_eq!(BUTTON_GROUP_HEIGHT, 40.0, "ButtonGroupSmallTokens.ContainerHeight");
        assert_eq!(BUTTON_GROUP_BETWEEN_SPACE, 12.0, "ButtonGroupSmallTokens.BetweenSpace");
        assert_eq!(
            CONNECTED_BUTTON_GROUP_BETWEEN_SPACE, 2.0,
            "ConnectedButtonGroupSmallTokens.BetweenSpace"
        );
        assert_eq!(BUTTON_GROUP_EXPANDED_RATIO, 0.15, "ButtonGroupDefaults.ExpandedRatio");
        assert_eq!(
            CONNECTED_INNER_CORNER_SIZE, 8.0,
            "InnerCornerCornerSize is CornerValueSmall"
        );
        assert_eq!(
            CONNECTED_PRESSED_INNER_CORNER_SIZE, 4.0,
            "PressedInnerCornerCornerSize is CornerValueExtraSmall"
        );
    }

    /// The connected corners, item by item, in both directions. The START corners are
    /// `CornerFull` (20 on a 40 dp item) and the inner pair is 8, tightening to 4 while pressed;
    /// a middle item is 8 all round.
    #[test]
    fn the_connected_shapes_are_the_tokens() {
        let lead = ButtonGroupDefaults::connected_leading_shapes(LayoutDirection::Ltr);
        assert_eq!(lead.shape, Shape::corners(20.0, 8.0, 8.0, 20.0), "LTR leading");
        assert_eq!(
            lead.pressed_shape,
            Shape::corners(20.0, 4.0, 4.0, 20.0),
            "LTR leading, pressed"
        );
        assert_eq!(lead.checked_shape, Shape::Pill, "checked pops out as a stadium");

        assert_eq!(
            ButtonGroupDefaults::connected_leading_shapes(LayoutDirection::Rtl).shape,
            Shape::corners(8.0, 20.0, 20.0, 8.0),
            "under RTL the START corners are the right ones"
        );

        let mid = ButtonGroupDefaults::connected_middle_shapes();
        assert_eq!(mid.shape, Shape::rounded(8.0), "a middle item is ShapeDefaults.Small");
        assert_eq!(mid.pressed_shape, Shape::rounded(4.0));

        let trail = ButtonGroupDefaults::connected_trailing_shapes(LayoutDirection::Ltr);
        assert_eq!(trail.shape, Shape::corners(8.0, 20.0, 20.0, 8.0), "LTR trailing");
        assert_eq!(trail.pressed_shape, Shape::corners(4.0, 20.0, 20.0, 4.0));
    }

    /// `item_shapes` picks by position, and the trailing test is `index + 1 >= count` — a
    /// two-item row must not let index 1 fall through to the middle set.
    #[test]
    fn item_shapes_pick_by_position() {
        let ltr = LayoutDirection::Ltr;
        assert_eq!(
            ButtonGroupDefaults::item_shapes(0, 2, ltr).shape,
            ButtonGroupDefaults::connected_leading_shapes(ltr).shape
        );
        assert_eq!(
            ButtonGroupDefaults::item_shapes(1, 2, ltr).shape,
            ButtonGroupDefaults::connected_trailing_shapes(ltr).shape
        );
        assert_eq!(
            ButtonGroupDefaults::item_shapes(1, 3, ltr).shape,
            ButtonGroupDefaults::connected_middle_shapes().shape,
            "the middle of three is a middle item"
        );
        assert_eq!(
            ButtonGroupDefaults::item_shapes(2, 3, ltr).shape,
            ButtonGroupDefaults::connected_trailing_shapes(ltr).shape
        );
    }

    /// `shape_for` follows upstream's `shapeByInteraction` precedence: pressed wins.
    #[test]
    fn the_pressed_shape_beats_the_checked_one() {
        let shapes = ButtonGroupDefaults::connected_leading_shapes(LayoutDirection::Ltr);
        assert_eq!(shapes.shape_for(false, false), shapes.shape);
        assert_eq!(shapes.shape_for(false, true), shapes.checked_shape);
        assert_eq!(shapes.shape_for(true, false), shapes.pressed_shape);
        assert_eq!(shapes.shape_for(true, true), shapes.pressed_shape);
    }

    fn leaf(ctx: &mut ComposeCtx, w: f32, h: f32) {
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new().size(w, h));
        ctx.end_node();
    }

    /// A squeezed icon button still paints its DECLARED square, not the box it was forced into.
    ///
    /// The renderer sizes the painted container from `minimum_interactive_content`, and that field
    /// has to be the size the component declared (`size(40, 40)` for an IconButton) rather than the
    /// MEASURED box: a ButtonGroup forces the node to the slot, so reading the measurement back
    /// painted the icon button as an oval — measured in the demo at 47.33 x 39.33 where the circle
    /// is 40 x 40.
    #[test]
    fn a_squeezed_icon_button_paints_its_declared_square() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .add(
                                ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                                    .press_source(pressed.clone()),
                            )
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path(
                                            "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z",
                                        )
                                        .size(24.0)
                                        .build(ctx);
                                    });
                            })
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (the item's node width, the content size the renderer will paint)
        let painted = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let item = nodes[root].children[1];
            let (cw, ch) = nodes[item]
                .minimum_interactive_content
                .expect("an IconButton publishes its declared content size");
            (nodes[item].measured_size.width, cw, ch)
        };
        build(&mut composer);
        let (node_w, cw, ch) = painted(&composer);
        assert_eq!((node_w, cw, ch), (48.0, 40.0, 40.0), "at rest");

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (node_w, cw, ch) = painted(&composer);
        assert!(node_w < 48.0, "the press squeezed the node, got {node_w}");
        assert_eq!(
            (cw, ch),
            (40.0, 40.0),
            "the painted container is still the declared 40 x 40 square — reading the measured box \
             back made it an oval"
        );
    }

    /// The row's geometry, measured on a real composition: the items sit at the group's spacing
    /// and the box is the token's height. This is the whole of what `ButtonGroupPolicy` claims to
    /// do — an item's own width is its own business.
    #[test]
    fn the_row_spaces_its_items_by_its_token() {
        let measured = |connected: bool| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                let group = if connected { ButtonGroup::connected() } else { ButtonGroup::new() };
                group
                    .item(|ctx, _source| leaf(ctx, 60.0, 40.0))
                    .item(|ctx, _source| leaf(ctx, 60.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let root_size = nodes[root].measured_size;
            let children = nodes[root].children.clone();
            let second_x = nodes[children[1]].position.x;
            (root_size, second_x)
        };

        let (plain, plain_x) = measured(false);
        assert_eq!(plain.height, BUTTON_GROUP_HEIGHT, "the group is the token's height");
        assert_eq!(plain_x, 60.0 + BUTTON_GROUP_BETWEEN_SPACE, "a plain group spaces by 12");
        assert_eq!(plain.width, 120.0 + BUTTON_GROUP_BETWEEN_SPACE);

        let (connected, connected_x) = measured(true);
        assert_eq!(connected.height, BUTTON_GROUP_HEIGHT);
        assert_eq!(
            connected_x,
            60.0 + CONNECTED_BUTTON_GROUP_BETWEEN_SPACE,
            "and a connected one by 2, so its items share a container"
        );
        assert_eq!(connected.width, 120.0 + CONNECTED_BUTTON_GROUP_BETWEEN_SPACE);
    }

    /// Weighted items share what the unweighted ones leave, proportionally — upstream's
    /// `(max - fixed - spacingTotal) / totalWeight * weight` (`ButtonGroup.kt:610-650`).
    #[test]
    fn weighted_items_share_the_space_left_over() {
        // Two equal weights in a 400 dp row, with the pair's 12 dp between them.
        let (size, second_x, second_w) = weighted_row(400.0, &[(1.0, true), (1.0, true)]);
        assert_eq!(second_x, 194.0 + BUTTON_GROUP_BETWEEN_SPACE, "the first takes half the leftover");
        assert_eq!(second_w, 194.0, "and so does the second");
        assert_eq!(size.width, 400.0, "the two fill the row");

        // 3:1 splits the same 388 dp into 291 and 97.
        let (_, three_x, one_w) = weighted_row(400.0, &[(3.0, true), (1.0, true)]);
        assert_eq!(three_x, 291.0 + BUTTON_GROUP_BETWEEN_SPACE);
        assert_eq!(one_w, 97.0);

        // An unweighted item keeps its natural 60 dp and the weighted one takes the rest.
        let (_, natural_x, weighted_w) = weighted_row(400.0, &[(0.0, false), (1.0, true)]);
        assert_eq!(natural_x, 60.0 + BUTTON_GROUP_BETWEEN_SPACE, "the natural item did not move");
        assert_eq!(weighted_w, 328.0, "400 - 60 - 12");

        // Nothing weighted: every item is natural, so the row is 120 + 12 wide.
        let (size, _, _) = weighted_row(400.0, &[(0.0, false), (0.0, false)]);
        assert_eq!(size.width, 120.0 + BUTTON_GROUP_BETWEEN_SPACE);
    }

    /// Lays out a group of `(weight, weighted)` items over a `width`-wide box. An unweighted or
    /// weighted-by-zero item is a 60 x 40 leaf, so a weighted one is visibly different from it.
    fn weighted_row(width: f32, spec: &[(f32, bool)]) -> (Size, f32, f32) {
        let spec = spec.to_vec();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let mut group = ButtonGroup::new();
            for (weight, weighted) in &spec {
                let w = *weight;
                if *weighted {
                    group = group.weighted_item(w, |ctx, _source| leaf(ctx, 60.0, 40.0));
                } else {
                    group = group.item(|ctx, _source| leaf(ctx, 60.0, 40.0));
                }
            }
            group.build(ctx);
        });
        composer.layout(Constraints::new(0.0, width, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let size = nodes[root].measured_size;
        let children = nodes[root].children.clone();
        (
            size,
            nodes[children[1]].position.x,
            nodes[children[1]].measured_size.width,
        )
    }

    /// The press growth — upstream's `Modifier.animateWidth` and its loop at
    /// `ButtonGroup.kt:729-770`. A middle item grows by `expandedRatio * width / 2` and takes
    /// that much OFF BOTH neighbours, so the row's total width does not move; the neighbours'
    /// `compressionLimit` (24 by default) is what caps it.
    #[test]
    fn a_pressed_item_grows_by_squeezing_its_neighbours() {
        // `update_animations` drives the process-wide table, so this takes the same lock the
        // other animation tests hold.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let middle = src.clone();
            composer.compose(move |ctx| {
                ButtonGroup::new()
                    .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                    .add(ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0)).press_source(middle))
                    .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let widths = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let children = nodes[root].children.clone();
            (
                nodes[children[0]].measured_size.width,
                nodes[children[1]].measured_size.width,
                nodes[children[2]].measured_size.width,
            )
        };

        build(&mut composer);
        assert_eq!(widths(&composer), (100.0, 100.0, 100.0), "at rest every item is natural");

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (left, middle, right) = widths(&composer);
        // `expandedRatio * 100 / 2` = 7.5, well under the 24 dp cap, taken off both neighbours.
        assert!(
            (middle - 115.0).abs() < 0.5,
            "the pressed middle item grows by 2 x 7.5, got {middle}"
        );
        assert!((left - 92.5).abs() < 0.5, "and its left neighbour gives up 7.5, got {left}");
        assert!((right - 92.5).abs() < 0.5, "so does its right neighbour, got {right}");
        assert!(
            (left + middle + right - 300.0).abs() < 0.5,
            "the row's total width must not move: it is a redistribution"
        );
    }

    /// The overflow: the indicator is measured first and the items fill what is left of the row,
    /// greedily, from the start — upstream's loop at `ButtonGroup.kt:695-711`. Three 100 dp items
    /// and a 40 dp indicator in a 340 dp row leave `340 - 40 - 12 = 288` for the items, which
    /// holds two of them (100, then 100 out of the 176 left) and not the third.
    #[test]
    fn the_overflow_fits_as_many_items_as_the_indicator_leaves_room_for() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ButtonGroup::new()
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .overflow_indicator(|ctx, _state| leaf(ctx, 40.0, 40.0))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 340.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        assert_eq!(children.len(), 4, "three items and the indicator");

        assert_eq!(nodes[children[0]].position.x, 0.0);
        assert_eq!(nodes[children[0]].measured_size.width, 100.0);
        assert_eq!(nodes[children[1]].position.x, 112.0, "the second item follows the first + 12");
        // The one that did not fit keeps its NATURAL size and is parked outside the row. A zero
        // size here is a feedback loop, not a tidier answer: the next measure folds the clean
        // node and reads that zero back as "it fits" — which is what the flicker test below, and
        // the weighted case in `a_parked_weighted_item_keeps_a_size`, are about.
        assert_eq!(
            nodes[children[2]].measured_size.width, 100.0,
            "the item that did not fit keeps its natural width"
        );
        assert!(
            nodes[children[2]].position.x >= 340.0,
            "and it is parked outside the row, got x={}",
            nodes[children[2]].position.x
        );
        assert_eq!(
            nodes[children[3]].position.x, 224.0,
            "and the indicator sits right after the last item that did fit"
        );
        assert_eq!(nodes[children[3]].measured_size.width, 40.0);
        assert_eq!(
            nodes[root].measured_size.width, 264.0,
            "the row is the two items, their gaps and the indicator"
        );
    }


    /// The menu's rows are picked by ITEM index out of a list that is INDEX-PAIRED with the
    /// items — the bug this pins.
    ///
    /// The rows used to be collected with `filter_map`, which COMPACTED them, so a cut taken at
    /// an item count indexed a SHORTER list: with only the LAST of three items given a menu the
    /// menu came out EMPTY and that item was unreachable, and with only the FIRST one given a
    /// menu it listed the item that WAS on screen.
    ///
    /// A unit test rather than an end-to-end one on purpose: a `State` write during composition
    /// is DEFERRED, so opening the menu the way a click does (flip the state, recompose) cannot
    /// be reproduced inside one composition, and a second composition of an unchanged group
    /// Skips. This pins the rule itself; the demo is the end-to-end case.
    #[test]
    fn the_menus_rows_are_picked_by_item_index() {
        // Only the THIRD item declared a menu, and the third is the one that did not fit.
        let mut rows = [None, None, Some("third")];
        assert_eq!(hidden_rows(&mut rows, 2), vec!["third"]);

        // Only the FIRST item declared a menu, and the first two fit: there is nothing hidden,
        // so the menu gets no row. The compacted form handed it the on-screen item instead.
        let mut rows = [Some("first"), None, None];
        assert!(
            hidden_rows(&mut rows, 2).is_empty(),
            "no item is hidden, so the menu has no row"
        );
        assert_eq!(
            hidden_rows(&mut rows, 0),
            vec!["first"],
            "and the same row IS the hidden one once nothing is visible"
        );

        // Every hidden item's row comes out, in item order, and each is taken once.
        let mut rows = [None, Some("b"), Some("c")];
        assert_eq!(hidden_rows(&mut rows, 1), vec!["b", "c"]);
        assert!(hidden_rows(&mut rows, 1).is_empty(), "the rows are consumed");
    }

    /// A parked item keeps a REAL size even when it is a WEIGHTED one.
    ///
    /// `natural` is what a parked item is placed at, and the pass that fills it used to skip
    /// weighted items — so one that did not fit was parked at the `0 x height` it was
    /// initialised with, the zero-width placement this policy's own note forbids (a later
    /// measure folds a clean node and reads that zero back).
    #[test]
    fn a_parked_weighted_item_keeps_a_size() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ButtonGroup::new()
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .weighted_item(1.0, |ctx, _source| leaf(ctx, 60.0, 40.0))
                .overflow_indicator(|ctx, _state| leaf(ctx, 40.0, 40.0))
                .build(ctx);
        });
        // 100 dp cannot hold the first item, so NOTHING fits and both items are parked.
        composer.layout(Constraints::new(0.0, 100.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        assert!(nodes[children[1]].position.x > 100.0, "the weighted item is parked outside");
        assert!(
            nodes[children[1]].measured_size.width > 0.0,
            "a parked item keeps its natural size, weighted or not, got {}",
            nodes[children[1]].measured_size.width
        );
    }

    /// The FLICKER guard: with no indicator the laid-out item count must not depend on the
    /// available width, swept one dp at a time.
    ///
    /// A weighted item's width is derived FROM the budget, so the widths plus the gaps sum to
    /// exactly it — `widths[i] <= remaining` is then decided by floating point on the last item,
    /// and the row dropped it at some widths and kept it at others. Resizing the window walked
    /// through those widths, so the last button blinked.
    #[test]
    fn a_weighted_row_without_an_indicator_never_drops_an_item() {
        let placed = |width: f32| -> usize {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                ButtonGroup::new()
                    .item(|ctx, _source| leaf(ctx, 82.0, 40.0))
                    .weighted_item(1.0, |ctx, _source| leaf(ctx, 60.0, 40.0))
                    .weighted_item(2.0, |ctx, _source| leaf(ctx, 60.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, width, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .filter(|&&c| nodes[c].measured_size.width > 0.0)
                .count()
        };

        // A fine sweep, because the boundary is a floating-point one.
        let mut width = 180.0f32;
        while width <= 512.0 {
            assert_eq!(
                placed(width),
                3,
                "the row dropped an item at {width} — that is the blink"
            );
            width += 1.0;
        }
    }

    /// Without an indicator the row never overflows: all three items are laid out even though they
    /// are wider than the box, which is what a plain Row does too.
    #[test]
    fn without_an_indicator_nothing_overflows() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ButtonGroup::new()
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 340.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        assert_eq!(children.len(), 3);
        for (i, &child) in children.iter().enumerate() {
            assert_eq!(nodes[child].measured_size.width, 100.0, "item {i} is still laid out");
        }
    }

    /// THE REGRESSION GUARD for the source identity: the source an item is handed must be the
    /// SAME one after a recomposition.
    ///
    /// It was not. The list of sources was grown from `remember(..).get()`, which hands back a
    /// clone — so pushing into it stored nothing, and every build created a fresh batch. The
    /// button got a different source each recomposition, which reset its press state on the spot:
    /// no state layer, no ripple, and a group that read `pressed = false` for ever. Pressing the
    /// source from the first build must be visible through the last one.
    #[test]
    fn the_source_an_item_gets_is_the_same_after_a_recomposition() {
        use std::sync::{Arc, Mutex};
        let seen: Arc<Mutex<Vec<MutableInteractionSource>>> = Arc::new(Mutex::new(Vec::new()));
        let mut composer = crate::runtime::composer::Composer::new();
        // The spacing is alternated so the group's own slot is DIRTY on the second pass and it
        // re-enters: a clean slot skips its closure, and the item would never be handed anything
        // twice to compare.
        let mut build = |composer: &mut crate::runtime::composer::Composer, spacing: f32| {
            let seen = seen.clone();
            composer.compose(move |ctx| {
                ButtonGroup::new()
                    .spacing(spacing)
                    .item(move |ctx, source| {
                        seen.lock().unwrap().push(source);
                        leaf(ctx, 100.0, 40.0);
                    })
                    .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        build(&mut composer, 12.0);
        build(&mut composer, 13.0);

        let captured = seen.lock().unwrap().clone();
        assert!(
            captured.len() >= 2,
            "the item composed twice, so it was handed a source twice — got {}",
            captured.len()
        );
        captured[0].emit_press();
        assert!(
            captured[captured.len() - 1].state(true).pressed,
            "the later build's source must be the same one, not a fresh one"
        );
    }

    /// THE ROW'S WIDTH IS THE SUM OF ITS SLOTS, never of what the children reported.
    ///
    /// A button floors at its 48 dp touch target (`minimum_interactive_component_size`), so a
    /// neighbour that a press has squeezed REPORTS 48 for a narrower slot. Neither the row's
    /// width nor the node's may come from that number: taking the advance from it made the row
    /// GROW on every press — the demo's five icon buttons overflowed their own width and
    /// everything to the right of the pressed item moved — and leaving the node at the reported
    /// width made the squeezed item shrink about its CENTRE, so its leading edge drifted away
    /// from the item before it and opened a gap. Upstream never reads either: its positions come
    /// from `widths[]` and its layout size from `mainSpace` / `mainAxisLayoutSize`
    /// (`ButtonGroup.kt:688`, `:783`).
    #[test]
    fn the_row_keeps_its_width_when_a_squeezed_child_cannot_shrink() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                ButtonGroup::new()
                    .add(
                        ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                            .press_source(pressed.clone()),
                    )
                    // Floored at 48 by the SAME modifier a real button uses, so it cannot shrink
                    // and has no spare room to give.
                    .item(|ctx, _source| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().minimum_interactive_component_size());
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let widths = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let children = nodes[root].children.clone();
            (
                nodes[root].measured_size.width,
                nodes[children[0]].measured_size.width,
                nodes[children[1]].measured_size.width,
            )
        };

        build(&mut composer);
        assert_eq!(widths(&composer), (160.0, 100.0, 48.0), "at rest");

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }

        // The floored neighbour reports 48 whatever slot it is given — but its NODE is the slot it
        // was given (33 = 48 - 15), so it shrinks from its trailing edge rather than about its
        // centre, and the pressed item grows into the 115 it was given. The ROW is the sum of the
        // slots, so it does not move at all.
        assert_eq!(
            widths(&composer),
            (160.0, 115.0, 33.0),
            "the pressed item grows into its slot, the squeezed neighbour is the slot it was \
             given (not the 48 it reports), and the row stays the width it started at"
        );
    }

    /// The CONTENT of a squeezed item stays centred in the box the row paints.
    ///
    /// The node is the slot's width, but the child was measured — and its content centred — in
    /// the box the child REPORTED, 48 dp, its touch target. Narrowing the node then left the
    /// content off centre: measured in the demo, the heart inside a squeezed icon item sat 2.0 px
    /// right of its container. `place` shifts the subtree by half the difference.
    #[test]
    fn a_squeezed_items_content_follows_its_container_to_the_trailing_edge() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .add(
                                ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                                    .press_source(pressed.clone()),
                            )
                            // A real icon button: a 40 dp container inside the 48 dp touch
                            // target, which is the only shape whose content the modifier
                            // centres.
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path(
                                            "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z",
                                        )
                                        .size(24.0)
                                        .build(ctx);
                                    });
                            })
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (the item's width, where its content sits inside the item's own box). The content is the
        // node's child: the minimum-interactive modifier places it, and the row re-anchors it when
        // it squeezes the slot.
        let geometry = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let item = nodes[root].children[1];
            let glyph = nodes[item].children[0];
            (
                nodes[item].measured_size.width,
                nodes[glyph].position.x,
                nodes[glyph].measured_size.width,
            )
        };
        build(&mut composer);
        let (slot, glyph_x, glyph_w) = geometry(&composer);
        assert_eq!(slot, 48.0, "at rest the item is its touch target");
        assert_eq!(
            glyph_x + glyph_w / 2.0,
            slot / 2.0,
            "and its content is centred in it"
        );

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (slot, glyph_x, glyph_w) = geometry(&composer);
        assert!(slot < 48.0, "the press squeezed it, got {slot}");
        // The item AFTER the pressed one is its TRAILING neighbour, so it holds its trailing edge
        // and gives up width on the leading side: the container (and the glyph inside it) moves
        // towards the trailing edge instead of staying put — centring would have moved BOTH edges.
        //   slot 33, inset 4, container 25 wide at x = 33 - 25 - 4 = 4, glyph centred in it.
        assert_eq!(glyph_x, 4.5, "the content follows the container towards the trailing edge");
    }
    /// The painted box follows a press: the pressed item's container widens BOTH ways, and each
    /// neighbour keeps its OUTER edge and gives up width on the side that faces the press.
    ///
    /// This is the half a test of `measured_size` cannot see. The row's SLOTS were already right when
    /// the demo still showed nothing moving — the container is a separate box (`paint_rect` reads
    /// `minimum_interactive_painted`), so the previous test of the slots stayed green through it.
    #[test]
    fn a_press_widens_the_painted_box_and_holds_the_outer_edges() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                            .size(24.0)
                                            .build(ctx);
                                    });
                            })
                            .add(
                                ButtonGroupItem::new(|ctx, source| {
                                    crate::components::icon_button::IconButton::new()
                                        .interaction_source(source)
                                        .on_click(|| {})
                                        .build(ctx, |ctx| {
                                            crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                                .size(24.0)
                                                .build(ctx);
                                        });
                                })
                                .press_source(pressed.clone()),
                            )
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                            .size(24.0)
                                            .build(ctx);
                                    });
                            })
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (left, right) of every item's painted container, in the row's own coordinates — the node's
        // own position plus the box the row wrote on it.
        let boxes = |composer: &crate::runtime::composer::Composer| -> Vec<(f32, f32)> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .map(|&c| {
                    let (left, _, w, _) = nodes[c]
                        .minimum_interactive_painted
                        .expect("an IconButton item publishes its painted box");
                    (nodes[c].position.x + left, nodes[c].position.x + left + w)
                })
                .collect()
        };

        build(&mut composer);
        let rest = boxes(&composer);
        assert_eq!(rest.len(), 3, "three items");
        for (l, r) in &rest {
            assert!(
                (r - l - 40.0).abs() < 0.01,
                "at rest each container is the declared 40 dp square, got {}",
                r - l
            );
        }

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let held = boxes(&composer);
        assert!(
            (held[0].0 - rest[0].0).abs() < 0.01,
            "the leading neighbour keeps its leading edge: {} -> {}",
            rest[0].0,
            held[0].0
        );
        assert!(
            held[0].1 < rest[0].1 - 1.0,
            "and gives up width on the trailing side: {} -> {}",
            rest[0].1,
            held[0].1
        );
        assert!(
            held[1].0 < rest[1].0 - 1.0 && held[1].1 > rest[1].1 + 1.0,
            "the pressed item grows BOTH ways: {:?} -> {:?}",
            rest[1],
            held[1]
        );
        assert!(
            (held[2].1 - rest[2].1).abs() < 0.01,
            "the trailing neighbour keeps its trailing edge: {} -> {}",
            rest[2].1,
            held[2].1
        );
        assert!(
            held[2].0 > rest[2].0 + 1.0,
            "and gives up width on the leading side: {} -> {}",
            rest[2].0,
            held[2].0
        );
    }

    /// A group WITH an overflow indicator wraps every item in a layer (`wrap_item`), so the node the
    /// row sees is that WRAPPER — which carries no size, publishes no declared box, and therefore made
    /// the whole painted-box step a no-op. The item inside then kept its own un-squeezed container and
    /// ran over its neighbour's slot. The steering has to reach through the wrapper to the node that
    /// owns the container.
    #[test]
    fn an_icon_item_in_an_overflow_group_is_steered_through_the_wrapper() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .add(
                                ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                                    .press_source(pressed.clone()),
                            )
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                            .size(24.0)
                                            .build(ctx);
                                    });
                            })
                            .overflow_indicator(|ctx, _state| leaf(ctx, 40.0, 40.0))
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (the wrapper the row sees, the item inside it, that item's painted box)
        let wrapped = |composer: &crate::runtime::composer::Composer| -> Option<(f32, f32, f32)> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let wrapper = nodes[root].children[1];
            let inner = *nodes[wrapper].children.first()?;
            assert_ne!(wrapper, inner, "the row wrapped the item, so these are two nodes");
            let (left, _, w, h) = nodes[inner].minimum_interactive_painted?;
            Some((left, w, h))
        };

        build(&mut composer);
        let (left, w, h) = wrapped(&composer).expect("the wrapped item is steered too");
        assert_eq!(
            (left, w, h),
            (4.0, 40.0, 40.0),
            "at rest: the declared square, inset 4 inside its slot"
        );

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (left, w, _) = wrapped(&composer).expect("still steered after the press");
        assert!(
            (w - 25.0).abs() < 0.5,
            "the squeezed container narrows with its slot, got {w}"
        );
        assert_eq!(left, 4.0, "and it holds its trailing edge: the press is on its leading side");
    }
