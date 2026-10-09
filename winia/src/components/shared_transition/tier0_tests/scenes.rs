use super::*;

#[test]
fn a_panicking_scene_does_not_stay_on_the_scene_stack() {
    let _g = lock_serial();
    clear_nav_scenes();
    let info = NavSceneInfo {
        id: 0x5CE7E,
        scene_key: 0x5CE7F,
        visibility: std::sync::Arc::new(|| 1.0),
        is_prev: false,
    };
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_nav_scene(info, || panic!("a scene host failed while composing"))
    }));
    assert!(caught.is_err(), "the panic must propagate out of the scene");
    assert!(
        current_nav_scene().is_none(),
        "the scene stack must unwind with the panic"
    );
    assert!(
        nav_scene_visibility(0x5CE7E).is_none(),
        "the panicking scene's registry entry goes with the unwind (a scene nobody composes)"
    );
    // …and the next publish is attributed to ITS own scene, not to the dead one.
    let next_scene = NavSceneInfo {
        id: 0x5CE80,
        scene_key: 0x5CE81,
        visibility: std::sync::Arc::new(|| 1.0),
        is_prev: false,
    };
    let seen = with_nav_scene(next_scene, || current_nav_scene().map(|s| s.id));
    assert_eq!(seen, Some(0x5CE80), "a marker composed now belongs to the new scene");
    assert!(
        nav_scene_visibility(0x5CE80).is_some(),
        "a normally completed publish keeps its entry (a layer that Skips still resolves it)"
    );
    clear_nav_scenes();
}

#[test]
fn scope_is_transition_active_tracks_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    clear_scope_active_states();
    let mut composer = Composer::new();
    let show = State::new(true);

    frame(&mut composer, &show);
    show.set(false);
    frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");
    // Same global entry the coordinator syncs (keyed by scope_id).
    // Created on first read (false) — the next poll flips it, which is
    // exactly the subscription semantics users observe in composition.
    let scope_id = composer.shared_flights.values().next().expect("flight").flight.scope_id;
    let active = SharedTransitionScope::new(scope_id).is_transition_active();
    advance(&mut composer, &show);
    assert!(active.get(), "flag true while the flight is non-terminal");

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "flight completes");
    assert!(!active.get(), "flag false after teardown");
    crate::animation::clear_all_animations();
}

#[test]
fn scene_registry_drops_scenes_that_left_the_tree() {
    let _g = lock_serial();
    clear_nav_scenes();
    let info = |id: u64, is_prev: bool| NavSceneInfo {
        id,
        scene_key: id,
        visibility: std::sync::Arc::new(|| 1.0),
        is_prev,
    };
    with_nav_scene(info(11, true), || {});
    with_nav_scene(info(22, false), || {});
    assert!(nav_scene_visibility(11).is_some() && nav_scene_visibility(22).is_some());

    prune_nav_scenes(&HashSet::from([22]));
    assert!(
        nav_scene_visibility(11).is_none(),
        "a scene whose tag left the tree is dropped"
    );
    assert_eq!(
        nav_scene_is_prev(22),
        Some(false),
        "a live scene is kept, with its role intact"
    );
    // Equal counts (one stale, one live) must still prune: the guard that skipped this case kept a
    // stale entry alive.
    let info2 = |id: u64| NavSceneInfo {
        id,
        scene_key: id,
        visibility: std::sync::Arc::new(|| 1.0),
        is_prev: true,
    };
    with_nav_scene(info2(33), || {});
    assert_eq!(nav_scene_registry_len(), 2, "one live, one stale");
    prune_nav_scenes(&HashSet::from([22]));
    assert_eq!(nav_scene_registry_len(), 1, "the stale one goes even when the counts match");
    clear_nav_scenes();
}

#[test]
fn a_peer_composer_does_not_prune_another_composers_live_scene() {
    let _g = lock_serial();
    clear_nav_scenes();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();

    // A hosts a scene: publish it AND leave its tag in A's arena.
    let scene_id = 0x5CE7Eu64;
    with_nav_scene(
        NavSceneInfo {
            id: scene_id,
            scene_key: 0x5CE7F,
            visibility: std::sync::Arc::new(|| 1.0),
            is_prev: false,
        },
        || {},
    );
    a.compose(|ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_size().scene_tag(scene_id))
            .build(ctx, |_| {});
    });
    a.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    a.poll_shared_flights();

    // B is a peer with NO scene host at all, but with a flight of its own, so its pass runs.
    let show_b = State::new(true);
    let w_b = State::new(120.0f32);
    let b_frame = |b: &mut Composer, show: &State<bool>| {
        let (s, ww) = (show.clone(), w_b.clone());
        b.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        mixed_list_screen(ctx, &scope, &ww);
                    } else {
                        mixed_detail_screen(ctx, &scope, &ww);
                    }
                });
            });
        });
        b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        b.poll_shared_flights();
    };
    b_frame(&mut b, &show_b);
    show_b.set(false);
    b_frame(&mut b, &show_b);
    assert!(
        b.shared_flights.values().any(|f| !is_terminal(f.flight.phase)),
        "the peer really has a flight running"
    );

    // …and the registry is over the threshold, so a scan happens at all.
    for i in 0..(NAV_SCENE_PRUNE_THRESHOLD + 1) {
        with_nav_scene(
            NavSceneInfo {
                id: 0x1000 + i as u64,
                scene_key: 0x2000 + i as u64,
                visibility: std::sync::Arc::new(|| 1.0),
                is_prev: true,
            },
            || {},
        );
    }

    // App order: main poll, peer poll, then the window-level cross poll.
    let mut all: Vec<&mut Composer> = vec![&mut a, &mut b];
    Composer::poll_cross_flights(&mut all);

    assert_eq!(
        nav_scene_is_prev(scene_id),
        Some(false),
        "the live scene survives a frame in which a scene-less peer polled (registry len {})",
        nav_scene_registry_len()
    );
    assert!(
        nav_scene_registry_len() < NAV_SCENE_PRUNE_THRESHOLD,
        "the stale entries are gone (len {})",
        nav_scene_registry_len()
    );
    clear_nav_scenes();
    crate::animation::clear_all_animations();
}

#[test]
fn scope_activity_is_a_union_over_the_frames_composers() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    clear_scope_active_states();
    let mut a = Composer::new();
    let mut b = Composer::new(); // a peer composer with no flights of its own
    let show = State::new(true);
    let w = State::new(120.0f32);

    let a_frame = |a: &mut Composer, b: &mut Composer, show: &State<bool>, w: &State<f32>| {
        let (s, ww) = (show.clone(), w.clone());
        cross_frame(
            a,
            b,
            move |ctx| {
                SharedTransitionLayout::new().build(ctx, |ctx| {
                    let scope = current_shared_scope().expect("scope");
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        if s.get() {
                            mixed_list_screen(ctx, &scope, &ww);
                        } else {
                            mixed_detail_screen(ctx, &scope, &ww);
                        }
                    });
                });
            },
            |ctx| {
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |_| {});
            },
        );
    };

    a_frame(&mut a, &mut b, &show, &w);
    // Pin the precondition this test exists for: with a cross flight or a stashed source the cross poll
    // would take its BUSY path, which synced the union before this round and would make the test pass
    // for the wrong reason.
    assert!(
        !a.has_cross_flights() && a.pending_cross.is_empty() && !b.has_cross_flights(),
        "the frame must be cross-idle for the idle-path sync to be under test"
    );
    // Read the flag BEFORE the switch, the way composition does: the sync only updates scopes somebody
    // has already read (map entries), so a read after the fact would find a freshly created entry.
    let scope_id = a
        .arena_nodes()
        .iter()
        .find_map(|n| find_shared_marker(&n.modifier).map(|m| m.scope_id))
        .expect("a marked node carries the scope id");
    let active = SharedTransitionScope::new(scope_id).is_transition_active();
    show.set(false);
    a_frame(&mut a, &mut b, &show, &w);
    assert!(
        a.shared_flights.values().any(|f| !is_terminal(f.flight.phase)),
        "the switch opened a flight in the main composer"
    );
    assert!(
        active.get(),
        "a main-composer scope must stay active when a peer composer polled after it"
    );
    clear_scope_active_states();
    clear_nav_scenes();
    crate::animation::clear_all_animations();
}

#[test]
fn nav_scene_host_pairs_one_flight_and_suppresses_the_other_copy() {
    let _g = lock_serial();
    clear_nav_scenes();
    clear_scope_active_states();
    crate::animation::clear_all_animations();
    let main = NavBackStack::<NavRoute>::with_initial(NavRoute::List);
    let peer = NavBackStack::<NavRoute>::with_initial(NavRoute::List);
    let mut composer = Composer::new();

    // Frame 0: one scene, so the key has exactly one live end and there is nothing to pair.
    nav_frame(&mut composer, &main, &peer);
    assert!(composer.shared_flights.is_empty(), "no flight before the push");
    assert_eq!(
        marked_indices(&composer).len(),
        1,
        "one live end of the key before the push"
    );

    // The push: from this frame on the display composes the leaving (List) AND the entering
    // (Detail) scene at once, so the key has two live ends at the same time.
    main.push(NavRoute::Detail);
    nav_frame(&mut composer, &main, &peer);

    // ── (a) ONE flight for the key, pairing the two live ends across the two scenes ──
    let flights: Vec<FlightId> = composer
        .shared_flights
        .iter()
        .filter(|(_, a)| a.flight.key == "hero" && !is_terminal(a.flight.phase))
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(
        flights.len(),
        1,
        "two live ends, ONE flight — a flight per live end would crossfade each scene against itself"
    );
    let fid = flights[0];
    let sources = nav_ends(&composer, TransitionRole::Source);
    let targets = nav_ends(&composer, TransitionRole::Target);
    assert_eq!(sources.len(), 1, "exactly one leaving end is a flight end");
    assert_eq!(targets.len(), 1, "exactly one entering end is a flight end");
    {
        let a = &composer.shared_flights[&fid];
        assert_ne!(
            a.flight.source_slot, a.flight.target_slot,
            "the flight spans two different ends"
        );
        assert_eq!(
            (a.start.x, a.start.y, a.start.width, a.start.height),
            (0.0, 0.0, 120.0, 80.0),
            "the flight starts at the LEAVING hero's rect"
        );
        assert_eq!(
            a.flight.target_slot,
            Some(composer.arena_nodes()[targets[0]].slot_key),
            "the flight's target is the node wearing the Target role, not the placeholder"
        );
        assert_eq!(
            a.source_idx,
            Some(sources[0]),
            "the source ghost is the detached leaving end"
        );
    }
    {
        let t = &composer.arena_nodes()[targets[0]];
        assert_eq!(
            (t.measured_size.width, t.measured_size.height),
            (300.0, 160.0),
            "the flight lands on the DETAIL hero — pairing the leaving end with itself would land on \
             the 120x80 copy"
        );
    }
    let (target_scene, target_is_prev) = nav_scene_of(&composer, targets[0]);
    assert_eq!(
        target_is_prev,
        Some(false),
        "the flight's target is the ENTERING scene's end"
    );

    // ── (b) exactly one copy is a placeholder, and it is neither end of the flight ──
    let placeholders = nav_placeholders(&composer);
    assert_eq!(
        placeholders.len(),
        1,
        "exactly one copy a running flight does not own, got {placeholders:?}"
    );
    let ph = placeholders[0];
    assert!(
        ph != sources[0] && ph != targets[0],
        "the placeholder is a copy NEITHER end of the flight owns (node {ph})"
    );
    let (ph_scene, ph_is_prev) = nav_scene_of(&composer, ph);
    assert_eq!(
        ph_is_prev,
        Some(true),
        "the placeholder is the LEAVING scene re-composing its own copy of the flying key"
    );

    // ── (d) the two ends publish DIFFERENT scene tags, each keeping its role ──
    assert_ne!(
        ph_scene, target_scene,
        "the two live ends are published under different scene tags"
    );
    assert_eq!(
        nav_scene_is_prev(target_scene),
        Some(false),
        "the entering layer keeps its role (the peer host publishes the same route, and must not \
         overwrite either id)"
    );

    // ── (c) the placeholder paints nothing, while the flight itself does ──
    // Sample a mid-flight frame: at p ≈ 0 the flight still covers the placeholder's own rect (it
    // starts exactly there), which would make "no colour here" ambiguous.
    let mut sampled = None;
    for _ in 0..40 {
        match composer.shared_flights.get(&fid) {
            Some(a) if !is_terminal(a.flight.phase) => {
                let p = a.progress.peek();
                if (0.25..=0.85).contains(&p) {
                    sampled = Some(p);
                    break;
                }
            }
            _ => break,
        }
        nav_advance(&mut composer, &main, &peer);
    }
    let p = sampled.expect("a mid-flight frame (p in 0.25..=0.85) the raster probe can read");

    // Node indices are not stable across frames, so re-read the ends on the sampled frame — which
    // doubles as the "stable id across two consecutive frames" half of (d).
    let targets = nav_ends(&composer, TransitionRole::Target);
    let placeholders = nav_placeholders(&composer);
    assert_eq!(
        (targets.len(), placeholders.len()),
        (1, 1),
        "still one flight end and one placeholder mid-flight (p = {p:.2})"
    );
    let (target_idx, ph_idx) = (targets[0], placeholders[0]);
    assert_eq!(
        nav_scene_of(&composer, ph_idx),
        (ph_scene, Some(true)),
        "the leaving scene keeps its id AND its role one frame later"
    );
    assert_eq!(
        nav_scene_of(&composer, target_idx),
        (target_scene, Some(false)),
        "the entering scene keeps its id AND its role one frame later"
    );

    let mut surf = render_heads(&composer);
    let (px, py) = node_center(&composer, ph_idx);
    let ph_rgb = pixel_rgb(&mut surf, px, py);
    assert!(
        close_enough(ph_rgb, (255, 255, 255), 8),
        "the placeholder is not painted: its own rect keeps the cleared background, got {ph_rgb:?} \
         at ({px},{py}) (p = {p:.2})"
    );
    let lerped = composer.arena_nodes()[target_idx]
        .transition
        .as_ref()
        .expect("the target still carries the flight's per-frame visuals")
        .lerped();
    let (lx, ly) = (
        (lerped.x + lerped.width / 2.0) as i32,
        (lerped.y + lerped.height / 2.0) as i32,
    );
    let flight_rgb = pixel_rgb(&mut surf, lx, ly);
    assert!(
        !close_enough(flight_rgb, (255, 255, 255), 8),
        "the flight's own lerped rect DOES paint — without this the probe above proves nothing, \
         got {flight_rgb:?} at ({lx},{ly})"
    );

    // The flight completes and the disposition goes back to ordinary tree content.
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        nav_advance(&mut composer, &main, &peer);
    }
    assert!(composer.shared_flights.is_empty(), "the flight completes");
    assert_eq!(
        nav_placeholders(&composer).len(),
        0,
        "no placeholder survives its flight (the disposition is transient, never sticky)"
    );
    assert_eq!(
        marked_indices(&composer).len(),
        1,
        "only the entering scene's copy is left in the tree"
    );

    clear_nav_scenes();
    clear_scope_active_states();
    crate::animation::clear_all_animations();
}
