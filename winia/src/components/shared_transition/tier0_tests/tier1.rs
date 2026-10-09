use super::*;

#[test]
fn tier1_elevated_target_hittable_outside_its_ancestors() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(88);
    let show_a = State::new(true);
    let show_b = State::new(false);
    let scroll = crate::modifier::ScrollState::new();

    escape_cross_frame(&mut a, &mut b, &show_a, &show_b, &scope, &scroll);
    show_a.set(false);
    show_b.set(true);
    escape_cross_frame(&mut a, &mut b, &show_a, &show_b, &scope, &scroll);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 opens in the main map");

    let bt = marked_in(&b);
    assert_eq!(bt.len(), 1, "one peer endpoint");
    let tidx = bt[0];

    let mut probed = None;
    for _ in 0..200 {
        if let Some(p) = a.shared_flights.values().next().map(|f| f.progress.peek()) {
            if (0.4..=0.6).contains(&p) {
                let vis = b.arena_nodes()[tidx]
                    .transition
                    .clone()
                    .expect("peer target visual");
                let vp = nearest_viewport_abs(&b, tidx).expect("clipped ancestor");
                probed = Some((p, vis.lerped(), vp, vis));
                break;
            }
        }
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    let (p, l, (_, vp_y, _, _), vis) = probed.expect("flight passes mid-way");
    let (cx, cy) = (l.x + l.width / 2.0, l.y + l.height / 2.0);
    assert!(cy < vp_y, "probe is above the peer viewport ({cy} < {vp_y})");

    let bn = b.arena_nodes();
    let root = b.layout_root_idx().expect("peer root");
    let plain = crate::layout::node::hit_test(bn, root, cx, cy);
    assert!(
        !plain.contains(&tidx),
        "the plain walk cannot reach the clipped target (path={plain:?})"
    );
    // The claim: the layer-aware walk does reach it, at the animated rect.
    let path = hit_test_with_flights(bn, root, b.transition_roots(), cx, cy);
    assert!(
        path.contains(&tidx),
        "layer-aware hit test reaches the flying target at p={p} (path={path:?})"
    );
    // Then the mechanism that makes it possible.
    assert!(vis.elevated, "peer target flies in its own layer");

    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(a.shared_flights.is_empty(), "Tier1 completes");
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_peer_drops_the_layout_override_at_teardown() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let (mut a, mut b) = (Composer::new(), Composer::new());
    let scope = SharedTransitionScope::new(88);
    let (show_a, show_b) = (State::new(true), State::new(false));
    let frame = |a: &mut Composer, b: &mut Composer, sa: &State<bool>, sb: &State<bool>| {
        let (s1, s2, sc1, sc2) = (sa.clone(), sb.clone(), scope.clone(), scope.clone());
        cross_frame(
            a,
            b,
            |ctx| {
                shell(ctx, |ctx| {
                    if s1.get() {
                        hero_leaf_ph(ctx, 120.0, 80.0, Color::RED, &sc1, PlaceHolderSize::AnimatedSize);
                    }
                })
            },
            |ctx| {
                shell(ctx, |ctx| {
                    if s2.get() {
                        hero_leaf_ph(ctx, 300.0, 160.0, Color::BLUE, &sc2, PlaceHolderSize::AnimatedSize);
                    }
                })
            },
        );
    };
    frame(&mut a, &mut b, &show_a, &show_b);
    show_a.set(false);
    show_b.set(true);
    frame(&mut a, &mut b, &show_a, &show_b);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 flight in the main map");

    let peer = marked_in(&b);
    assert_eq!(peer.len(), 1, "one peer endpoint");
    let tidx = peer[0];
    assert!(
        b.arena_nodes()[tidx].flight_measure.is_some(),
        "the peer node carries the per-frame override"
    );
    // The cross-poll runs after the peer's layout, so the override first applies on
    // the peer's NEXT layout. DETERMINISTIC (no `xadvance`): a wall-clock advance
    // lets the engine drive the flight, so on a loaded machine the flight can
    // already be over by the time we pin — that made this test fail only in the
    // full parallel suite. Pin the progress, then drive one non-advancing frame +
    // one compose-free layout: the peer must report the ANIMATED size
    // (120x80 -> 300x160 at t=.5 = 210x120), not the resting one.
    assert_eq!(a.shared_flights.len(), 1, "the flight is still in flight");
    let fid = *a.shared_flights.keys().next().expect("flight id");
    a.shared_flights
        .get_mut(&fid)
        .expect("flight")
        .progress
        .set(0.5);
    frame(&mut a, &mut b, &show_a, &show_b);
    b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    let reported = b.arena_nodes()[tidx].measured_size;
    assert!(
        (reported.width - 210.0).abs() <= 1.0 && (reported.height - 120.0).abs() <= 1.0,
        "…and it reports the ANIMATED size, not the resting one (got {reported:?})"
    );

    // The peer must NOT open its own flight for a key a Tier-1 flight owns:
    // its override-driven size change looks like a layout morph, but the
    // flight lives in the MAIN map, so the peer cannot see it — and the
    // phantom morph's idle frame would delete the override every frame.
    let peer_flights_at_start = b.next_flight_id;
    xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    assert!(
        b.shared_flights.is_empty(),
        "the peer owns no flight for a Tier-1 key ({} found)",
        b.shared_flights.len()
    );
    assert_eq!(
        b.next_flight_id, peer_flights_at_start,
        "no phantom morph is opened per frame"
    );
    assert_eq!(
        b.arena_nodes()[tidx]
            .transition
            .as_ref()
            .map(|t| t.role),
        Some(TransitionRole::Target),
        "the peer's visual is the Tier-1 target, not a morph"
    );

    for _ in 0..300 {
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(a.shared_flights.is_empty(), "flight completes");
    frame(&mut a, &mut b, &show_a, &show_b);
    assert!(
        b.arena_nodes()[tidx].flight_measure.is_none(),
        "the peer override is dropped at teardown"
    );
    assert_eq!(
        b.arena_nodes()[tidx].measured_size,
        crate::unit::Size::new(300.0, 160.0),
        "…and the peer's resting size comes back"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_flight_pivot_alignment_mid_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    frame(&mut composer, &show);
    show.set(false);
    frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");

    // Drive into a mid-flight window where a pivot error is unambiguous:
    // the lerped rect must leave a clear margin above it. The old
    // translate-then-scale order scaled about the canvas origin, shifting
    // paint by pos*(scale-1) — the target painted ~25px too high here.
    let mut aligned = false;
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let probe = composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let e = a.flight.end.expect("end resolved after first poll");
            (p, a.start.lerp(&e, p))
        });
        if let Some((p, l)) = probe {
            if p > 0.3 && p < 0.7 && l.height < 128.0 && l.y > 24.0 {
                // Lerped center must show blended hero paint (both ends
                // coincide here by construction).
                let mut surf = render_heads(&composer);
                let cx = (l.x + l.width / 2.0) as i32;
                let cy = (l.y + l.height / 2.0) as i32;
                let c = pixel_rgb(&mut surf, cx, cy);
                assert!(
                    c.0 > 140 && c.2 > 50,
                    "lerped center shows blended flight paint, got {c:?}"
                );
                // Above the lerped top must be background: with the pivot
                // bug the target painted into this strip.
                let mut surf2 = render_heads(&composer);
                let above = pixel_rgb(&mut surf2, cx, (l.y - 12.0) as i32);
                assert!(
                    close_enough(above, (255, 255, 255), 40),
                    "no flight paint above the lerped rect, got {above:?}"
                );
                aligned = true;
                break;
            }
        }
        advance(&mut composer, &show);
    }
    assert!(aligned, "flight must pass through the alignment window");
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_main_to_overlay_opens_and_completes() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(77);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert!(a.shared_flights.is_empty() && b.shared_flights.is_empty());
    assert!(a.pending_cross.is_empty() && b.pending_cross.is_empty());

    // Switch: A drops, B shows.
    show_a.set(false);
    show_b.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 opens in the main map");
    assert!(b.shared_flights.is_empty(), "nothing stored peer-side");
    let fid = *a.shared_flights.keys().next().unwrap();
    {
        let f = &a.shared_flights[&fid];
        assert_eq!((f.source_cid, f.target_cid), (a.composer_id, b.composer_id));
        assert_eq!(f.flight.phase, FlightPhase::Flying, "resolved same frame");
    }
    assert_eq!(a.transition_layer.len(), 1, "source retained in owner arena");
    assert!(a.has_cross_flights(), "z-order helper sees it");
    // Target visuals live in B in B's frame.
    let bt = marked_in(&b);
    assert_eq!(bt.len(), 1);
    assert!(matches!(
        b.arena_nodes()[bt[0]].transition.clone(),
        Some(ref v) if v.role == TransitionRole::Target
    ));

    // p=0 frame paint: source opaque at A's hero center.
    let (sx, sy) = node_center(&a, a.transition_layer[0]);
    let mut surf0 = render_cross(&a, &b, (0.0, 0.0));
    assert!(
        close_enough(pixel_rgb(&mut surf0, sx, sy), (255, 0, 0), 30),
        "p=0 matches the pre-switch frame"
    );

    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(a.shared_flights.is_empty(), "Tier1 completes");
    assert!(a.transition_layer.is_empty(), "retained source freed in owner arena");
    assert!(b.transition_layer.is_empty());
    for idx in marked_in(&b) {
        assert!(b.arena_nodes()[idx].transition.is_none(), "target visuals cleared");
    }
    // End paint: BLUE detail hero in B.
    let bt = marked_in(&b);
    assert_eq!(bt.len(), 1);
    let (ex, ey) = node_center(&b, bt[0]);
    let mut surf = render_cross(&a, &b, (0.0, 0.0));
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
        "ends on the overlay hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_scope_flag_tracks_cross_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    clear_scope_active_states();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(90);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    show_a.set(false);
    show_b.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 opens in the main map");

    // Same global entry the union sync writes (keyed by scope_id).
    let active = scope.is_transition_active();
    xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    assert!(active.get(), "flag true while the cross flight is non-terminal");

    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(a.shared_flights.is_empty(), "Tier1 completes");
    assert!(!active.get(), "flag false after cross teardown");
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_reverse_overlay_to_main() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(78);
    // Dialog open with hero; main heroless.
    let show_a = State::new(false);
    let show_b = State::new(true);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    // Reverse: B drops, A shows.
    show_b.set(false);
    show_a.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 lives in the main map either way");
    let fid = *a.shared_flights.keys().next().unwrap();
    {
        let f = &a.shared_flights[&fid];
        assert_eq!((f.source_cid, f.target_cid), (b.composer_id, a.composer_id));
    }
    // Retained in the OVERLAY arena, not main.
    assert!(a.transition_layer.is_empty());
    assert_eq!(b.transition_layer.len(), 1);

    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(a.shared_flights.is_empty());
    assert!(b.transition_layer.is_empty(), "overlay-retained freed via owner ref");
    // Ends RED in main.
    let at = marked_in(&a);
    assert_eq!(at.len(), 1);
    let (ex, ey) = node_center(&a, at[0]);
    let mut surf = render_cross(&a, &b, (0.0, 0.0));
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (255, 0, 0), 30),
        "reverse ends on the main hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_cancel_when_target_vanishes() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(79);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    show_a.set(false);
    show_b.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1);
    xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    // Target vanishes with no counterpart (dialog torn down around flight).
    show_b.set(false);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert!(a.shared_flights.is_empty(), "stale Tier1 cancelled");
    assert!(a.transition_layer.is_empty(), "retained freed");
    assert!(b.shared_flights.is_empty() && b.transition_layer.is_empty());
    // Main tree carries no visuals.
    if let Some(root) = a.layout_root_idx() {
        let nodes = a.arena_nodes();
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            assert!(nodes[idx].transition.is_none(), "no stale visuals");
            stack.extend(nodes[idx].children.iter().copied());
        }
    }
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_unmatched_stash_freed_same_frame() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(80);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    // Plain removal: A drops its hero, B never shows a counterpart.
    show_a.set(false);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert!(a.shared_flights.is_empty(), "no flight without counterpart");
    assert!(a.pending_cross.is_empty(), "stash drained");
    assert!(a.transition_layer.is_empty(), "retained freed same frame (invisible)");
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_origin_offsets_end() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    // Overlay composited at an offset (mirrors screen_pos translation).
    b.screen_origin = (50.0, 60.0);
    let scope = SharedTransitionScope::new(81);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    show_a.set(false);
    show_b.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1);
    let fid = *a.shared_flights.keys().next().unwrap();
    let f = &a.shared_flights[&fid];
    // B-local (0,0,300,160) mapped to window coords.
    let end = f.flight.end.expect("end resolved");
    assert!(
        (end.x - 50.0).abs() < 0.01 && (end.y - 60.0).abs() < 0.01,
        "flight end in window coords, got ({}, {})",
        end.x,
        end.y
    );
    // …while B's own visuals stay in B's canvas frame: window origin
    // (0,0) renders at (-50,-60) in B's translated pass.
    let bt = marked_in(&b);
    assert_eq!(bt.len(), 1);
    let vis = b.arena_nodes()[bt[0]].transition.clone().expect("target visuals");
    assert!(
        (vis.start.x + 50.0).abs() < 0.01 && (vis.start.y + 60.0).abs() < 0.01,
        "writer-frame visuals subtract the origin, got ({}, {})",
        vis.start.x,
        vis.start.y
    );
    crate::animation::clear_all_animations();
}

#[test]
fn two_keys_fly_together() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(82);
    let show_a = State::new(true);
    let show_b = State::new(false);

    let pair_frame = |a: &mut Composer, b: &mut Composer| {
        let (sa, sb) = (show_a.clone(), show_b.clone());
        let (sca, scb) = (scope.clone(), scope.clone());
        cross_frame(
            a,
            b,
            |ctx| shell(ctx, |ctx| {
                if sa.get() {
                    hero_keyed_leaf(ctx, 100.0, 60.0, Color::RED, &sca, "k1");
                    hero_keyed_leaf(ctx, 100.0, 60.0, Color::GREEN, &sca, "k2");
                }
            }),
            |ctx| shell(ctx, |ctx| {
                if sb.get() {
                    hero_keyed_leaf(ctx, 200.0, 120.0, Color::BLUE, &scb, "k1");
                    hero_keyed_leaf(ctx, 200.0, 120.0, Color::BLUE, &scb, "k2");
                }
            }),
        );
    };
    let pair_advance = |a: &mut Composer, b: &mut Composer| {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        pair_frame(a, b);
    };

    pair_frame(&mut a, &mut b);
    show_a.set(false);
    show_b.set(true);
    pair_frame(&mut a, &mut b);
    assert_eq!(a.shared_flights.len(), 2, "one Tier1 flight per key");
    assert_eq!(a.transition_layer.len(), 2, "both sources retained");

    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        pair_advance(&mut a, &mut b);
    }
    assert!(a.shared_flights.is_empty(), "both complete");
    assert!(a.transition_layer.is_empty(), "both freed");
    for idx in marked_in(&b) {
        assert!(b.arena_nodes()[idx].transition.is_none());
    }
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_superseded_by_overlay_tier0() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(83);
    let show_a = State::new(true);
    let show_b = State::new(false);
    let shift_b = State::new(false);

    // B content with an optional gap above the hero (content switch).
    let bframe = |a: &mut Composer, b: &mut Composer| {
        let (sa, sb, sh) = (show_a.clone(), show_b.clone(), shift_b.clone());
        let (sca, scb) = (scope.clone(), scope.clone());
        cross_frame(
            a,
            b,
            |ctx| shell(ctx, |ctx| {
                if sa.get() {
                    hero_leaf(ctx, 120.0, 80.0, Color::RED, &sca, true);
                }
            }),
            |ctx| shell(ctx, |ctx| {
                // Macro'd screens (like list/detail): plain closures would
                // collide gap-over-hero at one position in test-fallback keys.
                if sb.get() {
                    if sh.get() {
                        b_shifted(ctx, &scb);
                    } else {
                        b_plain(ctx, &scb);
                    }
                }
            }),
        );
    };
    let badvance = |a: &mut Composer, b: &mut Composer| {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        bframe(a, b);
    };

    bframe(&mut a, &mut b);
    show_a.set(false);
    show_b.set(true);
    bframe(&mut a, &mut b);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 main→overlay opens");
    badvance(&mut a, &mut b);
    badvance(&mut a, &mut b);
    // Overlay switches its own content mid-Tier1: overlay Tier0 opens,
    // Tier1 yields (staleness-cancel, no leak, no double ghost).
    shift_b.set(true);
    bframe(&mut a, &mut b);
    assert!(a.shared_flights.is_empty(), "Tier1 yields to the newer Tier0");
    assert_eq!(b.shared_flights.len(), 1, "overlay Tier0 owns the key now");
    assert!(a.transition_layer.is_empty(), "Tier1 retained freed on yield");

    for _ in 0..200 {
        if b.shared_flights.is_empty() {
            break;
        }
        badvance(&mut a, &mut b);
    }
    assert!(b.shared_flights.is_empty(), "Tier0 completes");
    assert!(b.transition_layer.is_empty());
    // End paint: shifted BLUE hero (gap pushed it to y=100).
    let bt = marked_in(&b);
    assert_eq!(bt.len(), 1);
    let (ex, ey) = node_center(&b, bt[0]);
    let mut surf = render_cross(&a, &b, (0.0, 0.0));
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
        "overlay Tier0 end state paints"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_reverse_flies_back() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(84);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    show_a.set(false);
    show_b.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1);
    xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    // Reverse mid-flight: the old Tier1 cancels AND a reverse Tier1 opens
    // in the same cross-pass (stashed B-source × fresh A-target) — no snap.
    show_b.set(false);
    show_a.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    assert_eq!(a.shared_flights.len(), 1, "reverse Tier1 replaces the cancelled one");
    let fid = *a.shared_flights.keys().next().unwrap();
    {
        let f = &a.shared_flights[&fid];
        assert_eq!((f.source_cid, f.target_cid), (b.composer_id, a.composer_id));
        assert_eq!(f.flight.phase, FlightPhase::Flying);
    }
    assert_eq!(b.transition_layer.len(), 1, "B-side source retained");
    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(a.shared_flights.is_empty() && b.shared_flights.is_empty());
    assert!(a.transition_layer.is_empty() && b.transition_layer.is_empty());
    // Ends RED in main.
    let at = marked_in(&a);
    assert_eq!(at.len(), 1);
    assert!(a.arena_nodes()[at[0]].transition.is_none());
    let (ex, ey) = node_center(&a, at[0]);
    let mut surf = render_cross(&a, &b, (0.0, 0.0));
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (255, 0, 0), 30),
        "reversed back to the list hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier1_midflight_progress_visible() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(85);
    let show_a = State::new(true);
    let show_b = State::new(false);

    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    show_a.set(false);
    show_b.set(true);
    xframe(&mut a, &mut b, &show_a, &show_b, &scope);
    let mut mid_seen = false;
    for _ in 0..40 {
        if a.shared_flights.is_empty() {
            break;
        }
        let p = a.shared_flights.values().next().map(|f| f.progress.peek()).unwrap_or(1.0);
        if p > 0.2 && p < 0.95 {
            mid_seen = true;
            // Both ends carry same-progress visuals across composers.
            for idx in marked_in(&b) {
                let v = b.arena_nodes()[idx].transition.clone().expect("target visuals");
                assert!((v.progress - p).abs() < 0.001);
            }
            break;
        }
        xadvance(&mut a, &mut b, &show_a, &show_b, &scope);
    }
    assert!(mid_seen, "Tier1 passes through a visible mid state");
    crate::animation::clear_all_animations();
}
