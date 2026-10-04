use super::*;

#[test]
fn demo_shape_shared_hero_composable_still_opens_a_switch() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let build = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        shared_hero_box(ctx, &scope, 60.0, 60.0, Color::RED, true);
                    } else {
                        // Nested like the demo's Row → clipped Column.
                        Column::new()
                            .modifier(Modifier::new().clip(Shape::Rectangle))
                            .build(ctx, |ctx| {
                                shared_hero_box(ctx, &scope, 300.0, 160.0, Color::BLUE, true);
                            });
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    build(&mut composer);
    show.set(false);
    build(&mut composer);
    assert_eq!(composer.shared_flights.len(), 1, "a switch flight opens");
    let f = composer.shared_flights.values().next().unwrap();
    assert_ne!(
        f.flight.source_slot, f.flight.target_slot,
        "distinct slots ⇒ a real switch, not a same-screen morph"
    );
    assert_eq!(composer.transition_layer.len(), 1, "leaving hero retained");

    // Run it out, then make sure the landed hero does NOT replay: the morph
    // detector compares the endpoint's layout against its baseline a frame or two
    // after the override is dropped, so a baseline frozen while the flight ran
    // turns the flight's own size change into a fresh morph (the demo's visible
    // "replay").
    for _ in 0..300 {
        if composer.shared_flights.is_empty() {
            break;
        }
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        build(&mut composer);
    }
    assert!(composer.shared_flights.is_empty(), "the switch flight completes");
    // The morph BASELINE must have tracked the flight's own layout. Otherwise it is
    // the pre-flight rect, so landing compares the hero against where it took off
    // from, opens a fresh same-screen morph, and the hero flies the whole path again
    // — the replay reported from the demo.
    //
    // HONESTY (review round 3 measured this): neither this assertion nor the
    // follow-up loops below fail when the pre-fix freeze is restored, because in
    // THIS scenario the baseline is refreshed on the switch frame itself, before the
    // target carries an override. They are in-range invariant guards; the assertions
    // that actually pin the guard are in
    // `morph_detector_skips_the_decision_but_updates_the_baseline` (both halves
    // verified to fail on their respective reverts).
    let scope_id = *composer
        .shared_live_map()
        .keys()
        .next()
        .map(|k| &k.0)
        .expect("a live scope");
    let base = composer
        .shared_last_bounds
        .get(&(scope_id, "hero".to_string()))
        .copied()
        .expect("baseline for the landed key");
    let landed = {
        let idx = marked_in(&composer)[0];
        let root = composer.layout_root_idx().expect("root");
        let nid = composer.arena_nodes()[idx].id;
        let (ax, ay) = crate::app::node_abs_position(composer.arena_nodes(), root, nid);
        let n = &composer.arena_nodes()[idx];
        (ax, ay, n.measured_size.width, n.measured_size.height)
    };
    assert!(
        (base.width - landed.2).abs() <= 1.0 && (base.height - landed.3).abs() <= 1.0,
        "the baseline must be the landed rect ({:.0}x{:.0}), not the take-off rect \
         ({:.0}x{:.0}) — otherwise the landing reads as a fresh morph",
        landed.2,
        landed.3,
        base.width,
        base.height
    );
    for _ in 0..5 {
        build(&mut composer);
        assert!(
            composer.shared_flights.is_empty(),
            "no follow-up flight may open after the switch lands: {} found",
            composer.shared_flights.len()
        );
    }
    crate::animation::clear_all_animations();
}

#[test]
fn circle_radius_follows_the_lerped_rect_on_both_ends() {
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
                    circle_list_screen(ctx, &scope);
                } else {
                    circle_detail_screen(ctx, &scope);
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

    let (vis, box_w, box_h) = {
        let n = &composer.arena_nodes()[marked_in(&composer)[0]];
        let cb = n.content_box();
        (n.transition.clone().expect("flight visual"), cb.width, cb.height)
    };
    let l = vis.lerped();
    let want = l.width.min(l.height) / 2.0;
    // `radii_pairs` returns values for use INSIDE the scaled canvas, so the DEVICE
    // radius is the pair times the scale the canvas actually applies — that is
    // `paint_scale`, NOT the plain axis ratios. Both axes are asserted: they only
    // coincide when the box and the lerped rect share an aspect ratio, and the
    // second review round measured this test passing while `radii_pairs` divided by
    // the axis ratios (the two factors cancelled, hiding an 8.7%-stretched corner).
    let (sx, sy) = vis.paint_scale(box_w, box_h);
    let dev_x = vis.radii_pairs(box_w, box_h)[0].0 * sx;
    let dev_y = vis.radii_pairs(box_w, box_h)[0].1 * sy;
    assert!(
        (dev_x - want).abs() <= 0.5 && (dev_y - want).abs() <= 0.5,
        "the painted corner must be round on BOTH axes: got ({dev_x}, {dev_y}), \
         want ({want}, {want}) for lerped {l:?} box ({box_w}, {box_h})"
    );
    // (Review R4: an extra "the radius sits between the two endpoint radii"
    // assertion used to live here — it was implied by the equality above, which
    // is strictly stronger, so it was removed rather than kept as decoration.)

    // …and the same must hold for the LEAVING ghost, which no other test can see:
    // `marked_in` walks the tree, but the source end is detached into the layer, so
    // only an arena scan reaches it (risk 18's open item).
    //
    // MEASURED, DO NOT OVERCLAIM: with the round-3 elliptical-corner defect restored
    // (`radii_pairs` dividing by the axis ratios) and the target assertion above
    // temporarily disabled to isolate this half, the GHOST assertion still PASSES —
    // the two scale factors evidently coincide for a detached source end (its own box
    // is what the axis ratios are relative to, and the render does not scale it the way
    // it scales a target). So this is an invariant guard against the ghost half going
    // unexamined, NOT a discriminator for that defect; the only measured discriminator
    // for it is the target assertion above.
    let src = composer
        .arena_nodes()
        .iter()
        .find(|n| {
            n.transition
                .as_ref()
                .is_some_and(|t| matches!(t.role, TransitionRole::Source))
        })
        .expect("the detached ghost carries a Source visual");
    let svis = src.transition.clone().expect("ghost visual");
    let (sbw, sbh) = {
        let cb = src.content_box();
        (cb.width, cb.height)
    };
    let sl = svis.lerped();
    let swant = sl.width.min(sl.height) / 2.0;
    let (ssx, ssy) = svis.paint_scale(sbw, sbh);
    let sdev_x = svis.radii_pairs(sbw, sbh)[0].0 * ssx;
    let sdev_y = svis.radii_pairs(sbw, sbh)[0].1 * ssy;
    assert!(
        (sdev_x - swant).abs() <= 0.5 && (sdev_y - swant).abs() <= 0.5,
        "the GHOST's painted corner must be round on both axes too: got \
         ({sdev_x}, {sdev_y}), want ({swant}, {swant}) for lerped {sl:?} box \
         ({sbw}, {sbh})"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn percent_to_fixed_corners_stay_aligned_end_to_end() {
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
                    circle_to_rect_list(ctx, &scope);
                } else {
                    circle_to_rect_detail(ctx, &scope);
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);

    for p in [0.25f32, 0.5, 0.75] {
        let fid = *composer.shared_flights.keys().next().expect("flight id");
        composer
            .shared_flights
            .get_mut(&fid)
            .expect("flight")
            .progress
            .set(p);
        frame(&mut composer);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));

        let nodes = composer.arena_nodes();
        let src = nodes
            .iter()
            .filter_map(|n| n.transition.as_ref())
            .find(|t| t.role == TransitionRole::Source)
            .expect("source visual");
        let tgt = nodes
            .iter()
            .filter_map(|n| n.transition.as_ref())
            .find(|t| t.role == TransitionRole::Target)
            .expect("target visual");
        let l = src.lerped();
        let (from_r, to_r) = (src.radii()[0], tgt.radii()[0]);
        // Percent end resolved on the lerped rect (min(l)/2), mixed by
        // progress toward the rectangle's 0.
        let want = (l.width.min(l.height) / 2.0) * (1.0 - p);
        assert!(
            (from_r - to_r).abs() <= 0.5,
            "p={p}: both ends must paint the same corner (from={from_r}, to={to_r})"
        );
        assert!(
            (to_r - want).abs() <= 1.0,
            "p={p}: percent resolved on the lerped rect then mixed (got {to_r}, want {want})"
        );
    }
    crate::animation::clear_all_animations();
}

#[test]
fn circle_shape_fills_a_non_square_box_like_pill() {
    let _g = lock_serial();
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(320.0, 170.0)
                    .background(Color::RED, Shape::Circle),
            );
            ctx.end_node();
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let mut surface = render_heads(&composer);
    // Near the left edge at mid height: a true circle (diameter 170, centred)
    // leaves this empty; a stadium paints it.
    let near_edge = pixel_rgb(&mut surface, 12, 85);
    assert!(
        !close_enough(near_edge, (255, 255, 255), 12),
        "Circle must fill the box like Pill (percent-50 corners), got {near_edge:?}"
    );
    // …and the corner is rounded, so the very corner stays background.
    let corner = pixel_rgb(&mut surface, 2, 2);
    assert!(
        close_enough(corner, (255, 255, 255), 12),
        "…while the corner stays cut, got {corner:?}"
    );
}

#[test]
fn corner_endpoints_are_exact_for_every_shape_pair() {
    use crate::unit::Size;
    use crate::modifier::Modifier;
    use crate::transition::{SharedBounds, TransitionRole, TransitionVisual};

    // (source shape, target shape) at 320x170 — non-square on purpose.
    let cases: [(Shape, Shape); 5] = [
        (Shape::Circle, Shape::Rectangle),
        (Shape::Rectangle, Shape::Circle),
        (Shape::Pill, Shape::RoundedRect { corner_radius: 24.0 }),
        (Shape::Circle, Shape::RoundedRect { corner_radius: 24.0 }),
        (Shape::TopRoundedRect { radius: 50.0 }, Shape::Circle),
    ];
    for (from_shape, to_shape) in cases {
        let from = shared_shape_radii(
            &Modifier::new().background(Color::RED, from_shape),
            320.0,
            170.0,
        );
        let to = shared_shape_radii(
            &Modifier::new().background(Color::BLUE, to_shape),
            320.0,
            170.0,
        );
        let mk = |progress: f32| TransitionVisual {
            start: SharedBounds::new(0.0, 0.0, 320.0, 170.0),
            end: SharedBounds::new(0.0, 0.0, 320.0, 170.0),
            progress,
            role: TransitionRole::Target,
            scene_alpha: None,
            radius_from: from,
            radius_to: to,
            radius_from_auto: shared_shape_radius_is_auto(&Modifier::new().background(Color::RED, from_shape)),
            radius_to_auto: shared_shape_radius_is_auto(&Modifier::new().background(Color::BLUE, to_shape)),
            clip: false,
            link_slot: None,
            scroll: (0.0, 0.0),
            flight: 1,
            path: PathMotion::Linear,
            bounds_fx: None,
            elevated: false,
        scale_parts: Some((ContentScale::FillBounds, ImageAlignment::TopStart)),
            remeasure: false,
        };
        let at0 = mk(0.0).radii();
        let at1 = mk(1.0).radii();
        for i in 0..4 {
            assert!(
                (at0[i] - from[i]).abs() <= 0.5,
                "p=0 must be the source's own radius ({from_shape:?}->{to_shape:?} corner {i}): got {} want {}",
                at0[i],
                from[i]
            );
            assert!(
                (at1[i] - to[i]).abs() <= 0.5,
                "p=1 must be the target's own radius ({from_shape:?}->{to_shape:?} corner {i}): got {} want {}",
                at1[i],
                to[i]
            );
        }
    }
    // A non-square Circle is a percent corner (min/2), which is what makes the
    // endpoints above hold for it too.
    assert_eq!(
        shared_shape_radii(
            &Modifier::new().background(Color::RED, Shape::Circle),
            320.0,
            170.0
        ),
        [85.0; 4],
        "Circle resolves min(w,h)/2 against its own box, like Compose"
    );
    let _ = Size::new(1.0, 1.0);
}

#[test]
fn morph_detector_skips_the_decision_but_updates_the_baseline() {
    use crate::transition::{FlightMeasure, FlightMeasureFrame};
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let build = |composer: &mut Composer, w: f32, h: f32| {
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    shape_hero_screen(ctx, &scope, w, h, Color::RED, Shape::rounded(8.0));
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    build(&mut composer, 120.0, 80.0);
    let idx = marked_in(&composer)[0];
    let scope_id = *composer
        .shared_live_map()
        .keys()
        .next()
        .map(|k| &k.0)
        .expect("a live scope");
    let bkey = (scope_id, "hero".to_string());
    let before = *composer.shared_last_bounds.get(&bkey).expect("baseline");
    assert!((before.width - 120.0).abs() <= 1.0, "baseline starts at the hero ({before:?})");

    // A flight now OWNS this node's size: override attached, layout changed. No
    // compose, so the mutated tree is what the next poll sees.
    let mut measure_state = State::new(FlightMeasureFrame {
        content: Some(crate::unit::Size::new(300.0, 160.0)),
        reported: Some(crate::unit::Size::new(300.0, 160.0)),
    });
    {
        let n = &mut composer.arena.nodes[idx];
        n.measured_size = crate::unit::Size::new(300.0, 160.0);
        n.flight_measure = Some(FlightMeasure {
            frame: measure_state.clone(),
            owner: FlightKey { cid: composer.composer_id, id: 9 },
        });
    }
    measure_state.set(FlightMeasureFrame {
        content: Some(crate::unit::Size::new(300.0, 160.0)),
        reported: Some(crate::unit::Size::new(300.0, 160.0)),
    });
    composer.poll_shared_flights();

    assert!(
        composer.shared_flights.is_empty(),
        "the override-driven size change must NOT open a phantom flight ({} found)",
        composer.shared_flights.len()
    );
    let after = *composer.shared_last_bounds.get(&bkey).expect("baseline");
    assert!(
        (after.width - 300.0).abs() <= 1.0 && (after.height - 160.0).abs() <= 1.0,
        "…while the baseline must track the override-driven layout ({after:?}) — \
         freezing it makes the flight's own change look like a fresh morph at landing"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn skia_clamps_an_oversized_rrect_radius_to_half_the_shorter_side() {
    let rect = skia_safe::Rect::new(0.0, 0.0, 300.0, 100.0);
    let stored = |r: f32| {
        skia_safe::RRect::new_rect_xy(rect, r, r)
            .radii(skia_safe::rrect::Corner::UpperLeft)
            .x
    };
    assert_eq!(stored(10.0), 10.0, "a radius that fits is stored as asked");
    assert_eq!(stored(50.0), 50.0, "exactly half the shorter side is allowed");
    assert_eq!(stored(51.0), 50.0, "one past it is clamped");
    assert_eq!(stored(150.0), 50.0, "far past it is clamped too");
}

#[test]
fn morph_size_change_flies_without_slot_churn() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let w = State::new(120.0f32);

    morph_compose(&mut composer, &w);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    assert!(composer.shared_flights.is_empty(), "steady size opens nothing");
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let slot = composer.arena_nodes()[marked[0]].slot_key;

    // Layout-only size change (no compose call at all), with NO transition running: Compose ties a
    // morph to a transition, and a bare layout change (what a window resize produces) must open
    // nothing — it used to animate every marked node whose rect moved.
    w.set(300.0);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    assert!(
        composer.shared_flights.is_empty(),
        "a layout change outside a transition must open nothing"
    );

    // The same kind of change INSIDE a transition does morph — the mechanism under test.
    mark_scope_active(&composer);
    w.set(340.0);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    assert_eq!(composer.shared_flights.len(), 1, "size delta opens a morph flight");
    let fid = *composer.shared_flights.keys().next().unwrap();
    {
        let a = &composer.shared_flights[&fid];
        assert!(a.source_idx.is_none(), "morph detaches nothing");
        assert_eq!((a.flight.source_slot, a.flight.target_slot), (Some(slot), Some(slot)));
        assert_eq!(a.flight.phase, FlightPhase::Flying);
    }
    // Same slot, Morph role, opacity untouched.
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    assert_eq!(composer.arena_nodes()[marked[0]].slot_key, slot, "no slot churn");
    let vis = composer.arena_nodes()[marked[0]].transition.clone().expect("morph visuals");
    assert_eq!(vis.role, TransitionRole::Morph);
    assert_eq!(vis.alpha(), 1.0);

    // Run to completion with layout-only advances (zero compose calls).
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        advance_layout_only(&mut composer);
    }
    assert!(composer.shared_flights.is_empty(), "morph completes");
    for idx in marked_indices(&composer) {
        assert!(composer.arena_nodes()[idx].transition.is_none(), "visuals cleared");
    }
    // End paint: 300-wide GREEN covers x=290.
    let mut surf = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf, 290, 40), (0, 255, 0), 30),
        "grown box paints at the new size"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn morph_container_child_hittable_mid_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let w = State::new(120.0f32);

    morph_parent_compose(&mut composer, &w);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    assert!(composer.shared_flights.is_empty(), "steady size opens nothing");

    // A morph requires a running transition (Compose semantics), so declare one before the size
    // change that is supposed to open it.
    mark_scope_active(&composer);
    // Layout-only size change (no compose call at all).
    w.set(300.0);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    assert_eq!(composer.shared_flights.len(), 1, "size delta opens a morph flight");

    // Tap the plain child through the flight transform: at progress ~0
    // the 300-wide layout is scaled 0.4x into the 120-wide lerped rect,
    // so visual (10, 25) maps to layout (25, 25) — the gap child's heart.
    let nodes = composer.arena_nodes();
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let container = marked[0];
    assert_eq!(
        nodes[container].transition.as_ref().map(|t| t.role),
        Some(TransitionRole::Morph),
        "container carries the morph visual"
    );
    let child = nodes[container].children[0];
    let root = composer.layout_root_idx().expect("root");
    let path = hit_test_with_flights(nodes, root, composer.transition_roots(), 10.0, 25.0);
    assert_eq!(
        path.last().copied(),
        Some(child),
        "tap descends into the morphing container's child, got {path:?}"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn morph_baseline_keys_on_endpoint_identity() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    // Settle key "ka" (writes its baseline).
    keyed_frame(&mut composer, &show);
    keyed_frame(&mut composer, &show);
    assert!(composer.shared_flights.is_empty());
    let slot_a = composer.arena_nodes()[marked_indices(&composer)[0]].slot_key;

    // Swap to a DIFFERENT key at the same call-site slot with a
    // different size. Slot-keyed baselines would seed a spurious morph
    // from "ka"'s rect; identity-keyed baselines start clean.
    show.set(false);
    keyed_frame(&mut composer, &show);
    let slot_b = composer.arena_nodes()[marked_indices(&composer)[0]].slot_key;
    assert_eq!(slot_a, slot_b, "same call-site slot reused across the key swap (test premise)");
    assert!(
        composer.shared_flights.is_empty(),
        "key swap opens no morph flight"
    );
    for idx in marked_indices(&composer) {
        assert!(
            composer.arena_nodes()[idx].transition.is_none(),
            "fresh key carries no morph visual"
        );
    }
    crate::animation::clear_all_animations();
}
