use super::*;

#[test]
fn flight_measure_frame_reaches_the_parent_layout() {
    use crate::unit::Size;
    use crate::transition::{FlightMeasure, FlightMeasureFrame};
    let _g = lock_serial();
    let frame_state = State::new(FlightMeasureFrame::IDLE);
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(120.0, 60.0)
                        .background(Color::RED, Shape::rounded(8.0)),
                )
                .build(ctx, |ctx| {
                    let key = ctx.next_key();
                    ctx.start_leaf(key, Modifier::new().fill_max_width().height(20.0));
                    ctx.end_node();
                });
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new().size(40.0, 10.0));
            ctx.end_node();
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let root = composer.layout_root_idx().expect("root");
    let (marked, sibling, child) = {
        let nodes = composer.arena_nodes();
        let marked = nodes[root].children[0];
        (marked, nodes[root].children[1], nodes[marked].children[0])
    };
    assert_eq!(composer.arena_nodes()[marked].measured_size, Size::new(120.0, 60.0));
    assert_eq!(
        composer.arena_nodes()[sibling].position.y,
        60.0,
        "sibling starts below the natural size"
    );

    // What the coordinator does every frame: attach the frame and RE-SEED
    // the node's slot key, because `layout()` resets `layout_dirty` on the
    // whole tree at the start of every pass (a folded parent would never
    // descend into the override otherwise). `content` and `reported` carry
    // DIFFERENT sizes here on purpose: the content box drives layout/hit,
    // the reported size is what the parent observes.
    let key = composer.arena_nodes()[marked].slot_key;
    composer.arena.nodes[marked].flight_measure = Some(FlightMeasure {
        frame: frame_state.clone(),
        owner: FlightKey { cid: composer.composer_id, id: 1 },
    });
    frame_state.set(FlightMeasureFrame {
        content: Some(Size::new(200.0, 90.0)),
        reported: Some(Size::new(140.0, 70.0)),
    });
    composer.layout_dirty_keys.insert(key);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

    let nodes = composer.arena_nodes();
    assert_eq!(
        nodes[child].measured_size.width, 200.0,
        "RemeasureToBounds: the content re-measures at the ANIMATED width"
    );
    assert_eq!(
        nodes[sibling].position.y, 70.0,
        "PlaceHolderSize: the parent places siblings by the REPORTED size"
    );
    assert_eq!(
        nodes[marked].measured_size,
        Size::new(140.0, 70.0),
        "…and that is what `measured_size` holds"
    );
    assert_eq!(
        nodes[marked].content_box(),
        Size::new(200.0, 90.0),
        "…while the content box keeps the measured size for paint/clip/hit"
    );

    // One frame later: tick the state and lay out WITHOUT re-seeding. The
    // measure-time read must have registered a LAYOUT dependency, which is
    // what keeps the override reactive (no recomposition involved).
    frame_state.set(FlightMeasureFrame {
        content: Some(Size::new(160.0, 66.0)),
        reported: Some(Size::new(120.0, 50.0)),
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let nodes = composer.arena_nodes();
    assert_eq!(
        nodes[child].measured_size.width, 160.0,
        "a frame tick alone drives the re-measure (layout dependency)"
    );
    assert_eq!(nodes[sibling].position.y, 50.0, "…and the parent re-places");

    // Dropping the override restores the natural layout in one pass (the
    // teardown contract: clear + seed once).
    composer.arena.nodes[marked].flight_measure = None;
    composer.layout_dirty_keys.insert(key);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let nodes = composer.arena_nodes();
    assert_eq!(nodes[marked].measured_size, Size::new(120.0, 60.0));
    assert_eq!(nodes[marked].content_box(), Size::new(120.0, 60.0));
    assert_eq!(nodes[sibling].position.y, 60.0);
    assert_eq!(nodes[child].measured_size.width, 120.0);
}

#[test]
fn flight_layout_contract_matrix() {
    let _g = lock_serial();
    for (resize, placeholder, sibling_moves, content_follows) in [
        (ResizeMode::scale_to_bounds(), PlaceHolderSize::JumpCut, false, false),
        (ResizeMode::scale_to_bounds(), PlaceHolderSize::AnimatedSize, true, false),
        (ResizeMode::RemeasureToBounds, PlaceHolderSize::ContentSize, false, true),
        (ResizeMode::RemeasureToBounds, PlaceHolderSize::AnimatedSize, true, true),
    ] {
        crate::animation::clear_all_animations();
        LAYOUT_SCENARIO_BUILDS.store(0, std::sync::atomic::Ordering::Relaxed);
        let mut composer = Composer::new();
        let show = State::new(true);
        let (r, ph) = (resize.clone(), placeholder);
        let frame = |composer: &mut Composer| {
            let (s, rr, pp) = (show.clone(), r.clone(), ph);
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    if s.get() {
                        list_layout_screen(ctx, &scope, rr.clone(), pp);
                    } else {
                        detail_layout_screen(ctx, &scope, rr, pp);
                    }
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer.poll_shared_flights();
        };
        frame(&mut composer);
        show.set(false);
        frame(&mut composer);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened ({resize:?}/{placeholder:?})");
        let builds_at_start = LAYOUT_SCENARIO_BUILDS.load(std::sync::atomic::Ordering::Relaxed);
        assert!(builds_at_start > 0, "the counter must be live");

        // DETERMINISTIC sample: pin the flight's progress instead of
        // sampling the wall clock. Writers run after layout, so the poll
        // below writes the p=0.5 frame, and one compose-free `layout()`
        // consumes the seeded key exactly like the real frame loop does.
        let fid = *composer.shared_flights.keys().next().expect("flight id");
        composer
            .shared_flights
            .get_mut(&fid)
            .expect("flight")
            .progress
            .set(0.5);
        frame(&mut composer);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

        // 120x60 → 300x200 at t = .5 ⇒ animated 210x130, target 300x200.
        let (marked, child, sibling) = {
            let nodes = composer.arena_nodes();
            let marked = marked_in(&composer)[0];
            let parent = nodes[marked].parent_id.expect("parent");
            let root = composer.layout_root_idx().expect("root");
            let pidx =
                crate::layout::node::find_node_by_id(nodes, root, parent).expect("parent index");
            let sibling = nodes[pidx]
                .children
                .iter()
                .copied()
                .find(|&c| c != marked)
                .expect("sibling");
            (marked, nodes[marked].children[0], sibling)
        };
        let nodes = composer.arena_nodes();
        let (child_w, sibling_y, marked_size, content_box) = (
            nodes[child].measured_size.width,
            nodes[sibling].position.y,
            nodes[marked].measured_size,
            nodes[marked].content_box(),
        );
        let ctx = format!("{resize:?}/{placeholder:?}");

        // CONTENT: re-measured at the animated width, or kept at the target.
        let want_child = if content_follows { 210.0 } else { 300.0 };
        assert!(
            (child_w - want_child).abs() <= 1.0,
            "content width ({ctx}): got {child_w}, want {want_child}"
        );
        // CONTENT BOX: always the size the content was measured at — what
        // render/clip/hit use, never the placeholder size.
        let want_box = if content_follows { (210.0, 130.0) } else { (300.0, 200.0) };
        assert!(
            (content_box.width - want_box.0).abs() <= 1.0
                && (content_box.height - want_box.1).abs() <= 1.0,
            "content box ({ctx}): got {content_box:?}, want {want_box:?}"
        );
        // PARENT: AnimatedSize reflows the sibling, ContentSize/JumpCut hold.
        let want_sibling = if sibling_moves { 130.0 } else { 200.0 };
        assert!(
            (sibling_y - want_sibling).abs() <= 1.0,
            "sibling y ({ctx}): got {sibling_y}, want {want_sibling}"
        );
        let want_reported = if sibling_moves { 130.0 } else { 200.0 };
        assert!(
            (marked_size.height - want_reported).abs() <= 1.0,
            "reported height ({ctx}): got {}, want {want_reported}",
            marked_size.height
        );

        assert_eq!(
            LAYOUT_SCENARIO_BUILDS.load(std::sync::atomic::Ordering::Relaxed),
            builds_at_start,
            "the layout contract must not recompose ({ctx})"
        );

        // Teardown is the ONE place that must run on real time: completion waits
        // for the animation engine to release the progress state, and the engine
        // owns that clock (pinning `progress` and pumping a few frames does NOT
        // finish the flight — measured). The loop is capped, and the message
        // says so, because a stalled machine fails here without any logic being
        // wrong (review R4's determinism point).
        for _ in 0..300 {
            if composer.shared_flights.is_empty() {
                break;
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            frame(&mut composer);
        }
        assert!(
            composer.shared_flights.is_empty(),
            "the flight did not finish within the loop cap ({ctx}) — this is a \
             wall-clock cap, not necessarily a logic failure"
        );
        // The landed hero must NOT replay: with the override dropped in the
        // poll, the morph detector compares the endpoint's layout against its
        // baseline a frame or two later, and a frozen baseline turns the
        // flight's own size change into a fresh morph (the demo's visible
        // "replay").
        for _ in 0..4 {
            frame(&mut composer);
            assert!(
                composer.shared_flights.is_empty(),
                "no follow-up flight may open after the flight lands ({ctx}): {} found",
                composer.shared_flights.len()
            );
        }
        frame(&mut composer);
        let marked = marked_in(&composer)[0];
        let nodes = composer.arena_nodes();
        assert!(
            nodes[marked].flight_measure.is_none(),
            "the override is dropped at teardown"
        );
        assert!(
            (nodes[marked].measured_size.width - 300.0).abs() <= 2.0,
            "the natural size comes back ({resize:?}/{placeholder:?}: {})",
            nodes[marked].measured_size.width
        );
        // …and the whole flight (teardown included) still recomposed nothing.
        assert_eq!(
            LAYOUT_SCENARIO_BUILDS.load(std::sync::atomic::Ordering::Relaxed),
            builds_at_start,
            "no recomposition through the end of the flight ({ctx})"
        );
    }
    crate::animation::clear_all_animations();
}

#[test]
fn default_contract_does_not_touch_the_layout() {
    use crate::layout::node::MEASURE_COUNT;
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                if s.get() {
                    list_layout_screen(
                        ctx,
                        &scope,
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                    );
                } else {
                    detail_layout_screen(
                        ctx,
                        &scope,
                        ResizeMode::scale_to_bounds(),
                        PlaceHolderSize::JumpCut,
                    );
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");
    let marked = marked_in(&composer)[0];
    assert!(
        composer.arena_nodes()[marked].flight_measure.is_none(),
        "the default contract attaches no override"
    );

    // A second layout with identical constraints must fold completely. This runs
    // BEFORE the live-count control: the control's extra `layout()` would flush a
    // dirty key the (buggy) poll had seeded, and the assertion below would then see
    // zero and pass. Review round 3 measured exactly that: with the control first,
    // this test passed with the unconditional-seed regression restored. The control
    // block moved to the end, where it still proves the counter can move.
    MEASURE_COUNT.with(|c| c.set(0));
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let measured = MEASURE_COUNT.with(|c| c.get());
    assert_eq!(
        measured, 0,
        "the default path must not re-measure anything (got {measured})"
    );

    // LIVE-COUNT CONTROL (review R4): prove the counter can move in THIS
    // configuration — otherwise a counter stuck at zero by construction would pass
    // the assertion above identically.
    MEASURE_COUNT.with(|c| c.set(0));
    let marked_key = composer.arena_nodes()[marked].slot_key;
    composer.layout_dirty_keys.insert(marked_key);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let control = MEASURE_COUNT.with(|c| c.get());
    assert!(
        control > 0,
        "control: seeding the key must re-measure something (got {control})"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn shared_element_re_measures_and_honours_animated_size() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                if s.get() {
                    element_list_screen(ctx, &scope, PlaceHolderSize::AnimatedSize);
                } else {
                    element_detail_screen(ctx, &scope, PlaceHolderSize::AnimatedSize);
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    let fid = *composer.shared_flights.keys().next().expect("flight id");
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .progress
        .set(0.5);
    frame(&mut composer);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

    let (marked, child, sibling) = {
        let nodes = composer.arena_nodes();
        let marked = marked_in(&composer)[0];
        let parent = nodes[marked].parent_id.expect("parent");
        let root = composer.layout_root_idx().expect("root");
        let pidx =
            crate::layout::node::find_node_by_id(nodes, root, parent).expect("parent index");
        let sibling = nodes[pidx]
            .children
            .iter()
            .copied()
            .find(|&c| c != marked)
            .expect("sibling");
        (marked, nodes[marked].children[0], sibling)
    };
    let nodes = composer.arena_nodes();
    assert!(
        (nodes[child].measured_size.width - 210.0).abs() <= 1.0,
        "sharedElement re-measures its content (got {})",
        nodes[child].measured_size.width
    );
    assert!(
        (nodes[sibling].position.y - 130.0).abs() <= 1.0,
        "…and AnimatedSize still reflows the parent (got {})",
        nodes[sibling].position.y
    );
    crate::animation::clear_all_animations();
}

#[test]
fn elevated_remeasure_hit_lands_on_the_same_child_as_the_tree_walk() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                if s.get() {
                    stacked_list_screen(
                        ctx,
                        &scope,
                        ResizeMode::RemeasureToBounds,
                        PlaceHolderSize::ContentSize,
                    );
                } else {
                    stacked_detail_screen(
                        ctx,
                        &scope,
                        ResizeMode::RemeasureToBounds,
                        PlaceHolderSize::ContentSize,
                    );
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    let fid = *composer.shared_flights.keys().next().expect("flight id");
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .progress
        .set(0.5);
    frame(&mut composer);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

    let root = composer.layout_root_idx().expect("root");
    let marked = marked_in(&composer)[0];
    let (bands, content_h) = {
        let nodes = composer.arena_nodes();
        (nodes[marked].children.clone(), nodes[marked].content_box().height)
    };
    assert!((content_h - 130.0).abs() <= 1.0, "content box is the animated height");
    // Tap inside the lerped rect. y=80 must map to the 5th 20px band: a
    // wrong mapping stays INSIDE the content box (so it is not rejected)
    // but lands on a different band — that is what makes this load-bearing.
    let (x, y) = (50.0, 80.0);
    let layer = hit_test_with_flights(
        composer.arena_nodes(),
        root,
        composer.transition_roots(),
        x,
        y,
    );
    let tree = hit_test(composer.arena_nodes(), root, x, y);
    let want = bands[4];
    assert_eq!(
        layer.last().copied(),
        Some(want),
        "the elevated route maps the tap 1:1 into the content box, got {layer:?}"
    );
    assert_eq!(
        layer.last().copied(),
        tree.last().copied(),
        "layer and tree walks must agree"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_default_contract_does_not_touch_the_peer_layout() {
    use crate::layout::node::MEASURE_COUNT;
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let (mut a, mut b) = (Composer::new(), Composer::new());
    let scope = SharedTransitionScope::new(82);
    let (show_a, show_b) = (State::new(true), State::new(false));
    let frame = |a: &mut Composer, b: &mut Composer| {
        let (sa, sb, sca, scb) = (show_a.clone(), show_b.clone(), scope.clone(), scope.clone());
        cross_frame(
            a,
            b,
            |ctx| {
                shell(ctx, |ctx| {
                    if sa.get() {
                        hero_leaf_bounds(ctx, 120.0, 80.0, Color::RED, &sca);
                    }
                })
            },
            |ctx| {
                shell(ctx, |ctx| {
                    if sb.get() {
                        hero_leaf_bounds(ctx, 300.0, 160.0, Color::BLUE, &scb);
                    }
                })
            },
        );
    };
    frame(&mut a, &mut b);
    show_a.set(false);
    show_b.set(true);
    frame(&mut a, &mut b);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 flight opens");
    let tidx = marked_in(&b)[0];
    assert!(
        b.arena_nodes()[tidx].flight_measure.is_none(),
        "the default contract attaches no override on the peer either"
    );

    // Order matters: the real assertion runs FIRST. The control's extra `layout()`
    // would flush a dirty key the (buggy) poll had seeded, making the assertion
    // below see zero — review round 3 measured this test passing with the
    // unconditional-seed regression restored when the control came first.
    MEASURE_COUNT.with(|c| c.set(0));
    b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let measured = MEASURE_COUNT.with(|c| c.get());
    assert_eq!(
        measured, 0,
        "the default contract must not re-measure the peer (got {measured})"
    );

    // Control: the seeding path CAN move the counter in this composer.
    MEASURE_COUNT.with(|c| c.set(0));
    let key = b.arena_nodes()[tidx].slot_key;
    b.layout_dirty_keys.insert(key);
    b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let control = MEASURE_COUNT.with(|c| c.get());
    assert!(control > 0, "control: seeding must re-measure (got {control})");
    crate::animation::clear_all_animations();
}

#[test]
fn ghost_tap_maps_identity_into_a_remeasure_target() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                if s.get() {
                    stacked_list_screen(
                        ctx,
                        &scope,
                        ResizeMode::RemeasureToBounds,
                        PlaceHolderSize::ContentSize,
                    );
                } else {
                    stacked_detail_screen(
                        ctx,
                        &scope,
                        ResizeMode::RemeasureToBounds,
                        PlaceHolderSize::ContentSize,
                    );
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    let fid = *composer.shared_flights.keys().next().expect("flight id");
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .progress
        .set(0.5);
    frame(&mut composer);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

    let root = composer.layout_root_idx().expect("root");
    let marked = marked_in(&composer)[0];
    let bands = composer.arena_nodes()[marked].children.clone();
    // The lerped rect is 210x130 (120x60 -> 300x200 at t=.5). y=65 is its
    // middle: a 1:1 map keeps 65 (band 3), a map through the target size
    // scales it to 100 (band 5).
    let layer = hit_test_with_flights(
        composer.arena_nodes(),
        root,
        composer.transition_roots(),
        50.0,
        65.0,
    );
    assert_eq!(
        layer.last().copied(),
        Some(bands[3]),
        "the ghost maps the tap into the target's content box, got {layer:?}"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn remeasure_end_is_never_scaled_by_the_frame_delta() {
    use crate::transition::{SharedBounds, TransitionRole, TransitionVisual};
    let mk = |remeasure: bool| TransitionVisual {
        start: SharedBounds::new(0.0, 0.0, 150.0, 150.0),
        end: SharedBounds::new(0.0, 0.0, 320.0, 170.0),
        progress: 0.25,
        role: TransitionRole::Target,
        scene_alpha: None,
        radius_from: [0.0; 4],
        radius_to: [0.0; 4],
        radius_from_auto: false,
        radius_to_auto: false,
        clip: false,
        link_slot: None,
        scroll: (0.0, 0.0),
        flight: 1,
        path: PathMotion::Linear,
        bounds_fx: None,
        elevated: false,
        remeasure,
        scale_parts: None,
    };
    // The box the layout actually used is the PREVIOUS poll's animated size.
    let (box_w, box_h) = (150.0, 150.0);
    let remeasured = mk(true);
    assert_eq!(
        remeasured.paint_scale(box_w, box_h),
        (1.0, 1.0),
        "a re-measured end is drawn 1:1 on the lerped rect"
    );
    let lerped = remeasured.lerped();
    assert!(
        (lerped.width - box_w).abs() > 1.0,
        "…and the box really does trail the lerped rect ({lerped:?} vs {box_w}x{box_h})"
    );
    // The scaled mode keeps scaling its (natural) box into the lerped rect.
    let (sx, sy) = mk(false).paint_scale(300.0, 200.0);
    assert!(
        (sx - lerped.width / 300.0).abs() <= 1e-6 && (sy - lerped.height / 200.0).abs() <= 1e-6,
        "ScaleToBounds still scales its content box (got {sx},{sy})"
    );
}

#[test]
fn same_frame_double_compose_keeps_the_hero_measured() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer, twice: bool| {
        let s = show.clone();
        let mut once = |composer: &mut Composer| {
            composer.compose(|ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    if s.get() {
                        switch_frame_list(ctx, &scope);
                    } else {
                        switch_frame_detail(ctx, &scope);
                    }
                });
            });
        };
        once(composer);
        if twice {
            once(composer);
        }
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer, false);
    show.set(false);
    frame(&mut composer, true);

    // The hero must be reachable FROM THE ROOT (i.e. in the tree) and be the entering end with
    // a non-zero size. An earlier version of this assertion searched the whole arena for a node
    // that merely had the right width, and it matched the DETACHED leaving-end ghost (which is
    // out of the tree by design and has `role = Source`) — so it stayed green even with the
    // in-tree hero zero-sized and unmounted. Review measured that: `predicate matches = [1, 4]`.
    let in_tree = marked_indices(&composer);
    let hero = in_tree.iter().copied().find(|&i| {
        let n = &composer.arena_nodes()[i];
        matches!(
            n.transition.as_ref().map(|t| t.role.clone()),
            Some(TransitionRole::Target)
        ) && n.measured_size.width > 1.0
            && n.measured_size.height > 1.0
    });
    assert!(
        hero.is_some(),
        "the entering end must be in the tree and measured after a double-composed switch \
         frame; arena nodes with a marker: {in_tree:?}"
    );
    crate::animation::clear_all_animations();
}
