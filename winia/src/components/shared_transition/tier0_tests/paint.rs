use super::*;

#[test]
fn a_ghost_keeps_painting_its_child_on_the_switch_frame() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let mut frame = |composer: &mut Composer, show: &State<bool>| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                if s.get() {
                    painted_list(ctx, &scope);
                } else {
                    painted_detail(ctx, &scope);
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };

    // Two frames: the shared scope and its marker are established on the first one.
    frame(&mut composer, &show);
    frame(&mut composer, &show);
    // The hero paints green before any switch (the probe would be meaningless otherwise).
    {
        let marked = marked_indices(&composer);
        if marked.is_empty() {
            let dump: Vec<(usize, (f32, f32), usize, bool)> = composer
                .arena_nodes()
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    (
                        i,
                        (n.measured_size.width, n.measured_size.height),
                        n.children.len(),
                        find_shared_marker(&n.modifier).is_some(),
                    )
                })
                .collect();
            panic!("no marked node in the tree; arena (idx, size, kids, marked) = {dump:?}");
        }
        let hero = *marked.first().expect("the marked hero in the tree");
        let (hx, hy) = node_center(&composer, hero);
        let mut surf = render_heads(&composer);
        let before = pixel_rgb(&mut surf, hx, hy);
        assert!(
            close_enough(before, (0, 255, 0), 40),
            "the hero must paint its green child before the switch, got {before:?}"
        );
    }

    show.set(false);
    frame(&mut composer, &show);
    let src_idx = *composer
        .transition_layer
        .first()
        .expect("the leaving end is retained in the transition layer");
    {
        let vis = composer.arena_nodes()[src_idx]
            .transition
            .clone()
            .expect("the detached source carries a visual");
        assert_eq!(vis.role, TransitionRole::Source, "the ghost is the leaving end");
        assert!(
            vis.progress < 0.5,
            "this is the frame the flight starts, so p is still ~0, got {}",
            vis.progress
        );
    }
    let (sx, sy) = node_center(&composer, src_idx);
    let mut surf = render_heads(&composer);
    let c = pixel_rgb(&mut surf, sx, sy);
    assert!(
        close_enough(c, (0, 255, 0), 40),
        "the leaving end must still paint its child on the switch frame, got {c:?}"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_overlay_escape_paints_outside_ancestor_clip() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let scroll = crate::modifier::ScrollState::new();

    let (p, l, tidx, (_, vp_y, _, _)) = escape_to_midflight(&mut composer, &show, &scroll, true);
    // The probe point is genuinely OUTSIDE the target's ancestor viewport —
    // i.e. paint there can only come from the elevation pass.
    let cy = l.y + l.height / 2.0;
    assert!(
        cy < vp_y,
        "mid-flight rect center ({cy}) must be above the target viewport ({vp_y})"
    );

    // The claim first: paint lands OUTSIDE the ancestor clip. In-tree
    // painting cannot do this — the ancestor clip is on the canvas and a
    // descendant can never un-set it (this is what fails without the layer).
    let mut surf = render_heads(&composer);
    let c = pixel_rgb(&mut surf, (l.x + l.width / 2.0) as i32, cy as i32);
    // Both ends paint (red ghost over blue target). Ghost-only would leave
    // G ≈ 255·p ≈ 127; with the target underneath it drops to ≈ 255·p·(1−p).
    assert!(
        c.1 < 90,
        "target escaped the ancestor clip and paints under the ghost at p={p}, got {c:?}"
    );

    // Then the mechanism that makes it possible.
    let tvis = composer.arena_nodes()[tidx]
        .transition
        .as_ref()
        .expect("target visual")
        .clone();
    assert!(tvis.elevated, "target flies in the layer by default");
    assert!(
        composer.transition_roots().contains(&tidx),
        "elevated target is a layer root"
    );

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        escape_advance(&mut composer, &show, &scroll, true);
    }
    assert!(composer.shared_flights.is_empty(), "flight completes");
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_overlay_opt_out_paints_in_tree_and_stays_clipped() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let scroll = crate::modifier::ScrollState::new();

    let (p, l, tidx, _) = escape_to_midflight(&mut composer, &show, &scroll, false);
    let tvis = composer.arena_nodes()[tidx]
        .transition
        .as_ref()
        .expect("target visual")
        .clone();
    assert!(!tvis.elevated, "renderInOverlay=false keeps the target in-tree");
    // Both ends paint the SAME lerped rect — opting out changes only the
    // clip, never the position. A mismatched-looking pair on screen is the
    // ancestor clip edge over the flying shape, not a transform divergence:
    // the entering end is cut, the detached leaving end cannot be.
    let svis = composer.arena_nodes()[composer.transition_layer[0]]
        .transition
        .as_ref()
        .expect("source visual")
        .clone();
    assert_eq!(
        svis.lerped(),
        tvis.lerped(),
        "leaving ghost and entering target share one flight rect"
    );
    // In-tree means the frozen ancestor sum is added back by the flight
    // transform (the layer path does not use it at all).
    let a = composer.shared_flights.values().next().expect("flight");
    assert_eq!(
        tvis.scroll, a.end_scroll,
        "opt-out target still carries its frozen ancestor scroll"
    );
    assert!(
        !composer.transition_roots().contains(&tidx),
        "opt-out target stays out of the layer"
    );

    // Ancestors still clip it: only the ghost shows through at the probe.
    let mut surf = render_heads(&composer);
    let c = pixel_rgb(
        &mut surf,
        (l.x + l.width / 2.0) as i32,
        (l.y + l.height / 2.0) as i32,
    );
    assert!(
        c.1 > 100 && c.0 > 200,
        "ghost-only paint at p={p} (target clipped in-tree), got {c:?}"
    );

    // ...and the target is NOT parked at its own layout rect: the flying
    // content only ever covers the LERPED rect, so a mid-flight probe over
    // the target's resting place shows background. This is what separates
    // "clipped while flying" from "stopped flying".
    let nodes = composer.arena_nodes();
    let id_to_idx: HashMap<u64, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let rest = abs_rect_upward(nodes, &id_to_idx, tidx);
    assert!(rest.y > l.y, "the target rests below the mid-flight rect");
    let mut surf2 = render_heads(&composer);
    let c2 = pixel_rgb(
        &mut surf2,
        (rest.x + rest.width / 2.0) as i32,
        (rest.y + rest.height / 2.0) as i32,
    );
    assert!(
        close_enough(c2, (255, 255, 255), 40),
        "nothing paints at the target's resting rect mid-flight, got {c2:?}"
    );

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        escape_advance(&mut composer, &show, &scroll, false);
    }
    assert!(composer.shared_flights.is_empty(), "flight completes");
    crate::animation::clear_all_animations();
}

#[test]
fn mid_flight_paint_stays_inside_the_lerped_rect() {
    let _g = lock_serial();
    for (resize, placeholder) in [
        (ResizeMode::RemeasureToBounds, PlaceHolderSize::ContentSize),
        (ResizeMode::scale_to_bounds(), PlaceHolderSize::AnimatedSize),
    ] {
        crate::animation::clear_all_animations();
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
        let fid = *composer.shared_flights.keys().next().expect("flight id");
        composer
            .shared_flights
            .get_mut(&fid)
            .expect("flight")
            .progress
            .set(0.5);
        frame(&mut composer);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let l = flight_probe(&composer).expect("mid-flight").1;
        assert!(
            (l.width - 210.0).abs() <= 1.0 && (l.height - 130.0).abs() <= 1.0,
            "pinned to t=.5 ({l:?})"
        );

        // The green band is 60px of the content's height: SCALED into the
        // 130px lerped rect it ends at ~39px, so y=50 must be hero colour;
        // drawn 1:1 (the crop bug) it would still be band green there.
        let mut surface = render_heads(&composer);
        if matches!(resize, ResizeMode::ScaleToBounds { .. }) {
            let hero_ref = pixel_rgb(&mut surface, 100, 100);
            let scaled_away = pixel_rgb(&mut surface, 100, 50);
            assert!(
                close_enough(scaled_away, hero_ref, 24),
                "content must be SCALED into the lerped rect, not cropped \
                 ({resize:?}/{placeholder:?}): y50={scaled_away:?} vs hero={hero_ref:?}"
            );
        }

        // Inside the lerped rect: the flying pair is painted (a crossfade of
        // red and blue, so just require "not background").
        let inside = pixel_rgb(&mut surface, 40, 40);
        assert!(
            !close_enough(inside, (255, 255, 255), 12),
            "the hero paints inside the lerped rect ({resize:?}/{placeholder:?}): {inside:?}"
        );
        // Outside the lerped rect — but well inside the 300x200 target box — must be
        // background: neither a re-measured content nor a 1:1-drawn copy may spill
        // there.
        //
        // REVIEW ROUND 3 CORRECTION: these were removed by the R4 pass as "vacuous",
        // and that verdict was WRONG for the crop class — with `paint_scale` forced
        // to (1,1) (i.e. the content drawn unscaled, exactly the crop bug the band
        // probe's comment names) the whole test passed without them and FAILS with
        // them ("nothing of the hero may paint outside the lerped rect at (250,100)
        // … : (126, 126, 255)"). They are genuinely vacuous only for the frame-delta
        // STRETCH bug, which the band probe catches. Both classes need a probe, so
        // both are kept.
        for (x, y) in [(250, 100), (100, 160)] {
            let out = pixel_rgb(&mut surface, x, y);
            assert!(
                close_enough(out, (255, 255, 255), 12),
                "nothing of the hero may paint outside the lerped rect at \
                 ({x},{y}) ({resize:?}/{placeholder:?}): {out:?}"
            );
        }
    }
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_arc_flight_paints_off_the_straight_line() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    arc_frame(&mut composer, &show);
    show.set(false);
    arc_frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "arc flight opened");
    {
        let a = composer.shared_flights.values().next().expect("flight");
        assert_eq!(a.path, PathMotion::ArcBelow, "path resolves from the target marker");
    }

    // Diagonal travel (260px + 100px centers) bends substantially at
    // mid. The hero is only 40px, so probes discriminate paint: the
    // arc's outer edge is inside arc paint but outside straight paint
    // (and vice versa). A render path silently painting straight fails
    // both probes.
    let mut bent = false;
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let probe = composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let e = a.flight.end.expect("end resolved after first poll");
            let straight = a.start.lerp(&e, p);
            let nodes = composer.arena_nodes();
            let tslot = a.flight.target_slot.expect("target");
            let root = composer.layout_root_idx().expect("root");
            let tidx = find_idx_by_slot(nodes, root, tslot).expect("target");
            let arced = nodes[tidx].transition.as_ref().expect("visual").lerped();
            (p, straight, arced)
        });
        if let Some((p, straight, arced)) = probe {
            if p > 0.4 && p < 0.6 {
                let (scx, scy) = (
                    straight.x + straight.width / 2.0,
                    straight.y + straight.height / 2.0,
                );
                let (acx, acy) = (
                    arced.x + arced.width / 2.0,
                    arced.y + arced.height / 2.0,
                );
                let (dx, dy) = (acx - scx, acy - scy);
                let dev = (dx * dx + dy * dy).sqrt();
                assert!(
                    dev > 20.0,
                    "arc bends off the straight line, deviation {dev} at p={p}"
                );
                // Unit bend direction, 8px inside the paint edge (clear of
                // the 8px rounded corners at the edge middle).
                let (ux, uy) = (dx / dev, dy / dev);
                let m = arced.height / 2.0 - 8.0;
                let mut surf = render_heads(&composer);
                let outer = pixel_rgb(
                    &mut surf,
                    (acx + ux * m) as i32,
                    (acy + uy * m) as i32,
                );
                assert!(
                    outer.0 > 140 && outer.1 < 150 && outer.2 > 50,
                    "arc outer edge paints hero blend, got {outer:?} at p={p}"
                );
                let mut surf2 = render_heads(&composer);
                let inner = pixel_rgb(
                    &mut surf2,
                    (scx - ux * m) as i32,
                    (scy - uy * m) as i32,
                );
                assert!(
                    close_enough(inner, (255, 255, 255), 40),
                    "straight-side mirror is background, got {inner:?} at p={p}"
                );
                bent = true;
                break;
            }
        }
        arc_advance(&mut composer, &show);
    }
    assert!(bent, "flight must pass through the bend window");

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        arc_advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "arc flight completes");
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    // Absolute center (node_center is layout-relative — the hero nests
    // inside a Row here).
    let root = composer.layout_root_idx().expect("root");
    let nid = composer.arena_nodes()[marked[0]].id;
    let (ax, ay) = crate::app::node_abs_position(composer.arena_nodes(), root, nid);
    let (ex, ey) = (
        (ax + composer.arena_nodes()[marked[0]].measured_size.width / 2.0) as i32,
        (ay + composer.arena_nodes()[marked[0]].measured_size.height / 2.0) as i32,
    );
    let mut surf = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
        "settled end state shows the detail hero"
    );
    crate::animation::clear_all_animations();
}
