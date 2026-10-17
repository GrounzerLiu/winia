use super::*;

#[test]
fn opt_out_artifact_is_directional() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();

    // Forward: the destination screen clips its hero.
    let (p, n, maxd) = opt_out_ab_at_midflight(false);
    assert!(
        n > 0,
        "into a clipping destination the pass must matter (p={p}, n={n}, max_delta={maxd})"
    );

    // Back: the destination screen (list) has no clipping ancestor.
    let (p, n, maxd) = opt_out_ab_at_midflight(true);
    assert_eq!(
        n, 0,
        "back out of the container the flag must be a no-op (p={p}, n={n}, max_delta={maxd})"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn opt_out_keeps_the_entering_end_under_later_siblings() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    // The switch is read INSIDE the slot that builds the branches: reading
    // it a level up leaves this subtree skipped and the screen never
    // changes (same trap as a marker flag read in the wrong slot).
    let frame = |composer: &mut Composer| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_size())
                        .build(ctx, |ctx| {
                            if s.get() {
                                under_bar_list(ctx, &scope, false);
                            } else {
                                under_bar_detail(ctx, &scope, false);
                            }
                        });
                    // Pinned chrome, painted last → above the whole screen.
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(400.0, 60.0)
                                .background(Color::GREEN, Shape::Rectangle),
                        )
                        .build(ctx, |_| {});
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");

    // The flight is sampled at TWO pinned progresses, because the two things this test
    // asserts cannot hold at the same one — measured, not reasoned (a p -> pixel matrix at
    // eight probe points over p = 0.00..1.00, in both paint orders):
    //
    //   p <= 0.80  every probe inside the bar is pure green (0,255,0): the bar covers the
    //              hero, which is what the tree-order assertion below wants;
    //   p >= 0.90  the elevated hero paints over the bar: (26,21,205) at p=0.90 and
    //              (13,11,228) at p=0.95;
    //   p = 1.00   pure blue (0,0,255) everywhere — the flight has landed, so the
    //              "the ghost is still in flight" assertion would be vacuous there.
    //
    // So the tree-order pixel is read at P_TREE_ORDER (where the bar must win) and the
    // elevated one at P_ELEVATED, moving the SAME flight between them. Earlier revisions
    // pinned one p for both and passed only because the wall-clock loop happened to
    // overshoot to ~0.89, which is why this failed under a full parallel run and passed
    // alone. Nothing below depends on how far that loop got: it only has to get past
    // resolution, which the `>= 0.5` break already guarantees.
    let mut sample: Option<(f32, u64, usize)> = None;
    // Let the flight RESOLVE and start painting before pinning: a pin applied while
    // the flight is still `AwaitingBounds` leaves it with no visual, and the render
    // then shows the pinned bar where the hero should be (measured: green-dominant
    // (28,226,0) instead of the target's blue). Half-way through is past resolution.
    // Advance just past RESOLUTION — the pin below is what the assertions read, so this loop's
    // landing point does not matter (verified by starving it to two frames: still green).
    for _ in 0..40 {
        if flight_probe(&composer).is_some_and(|(p, _)| p >= 0.5) {
            break;
        }
        crate::animation::update_animations();
        frame(&mut composer);
    }
    // Pin the progress rather than trusting the wall-clock loop's landing point. Two values,
    // for the two paint orders; see the measured table above.
    const P_TREE_ORDER: f32 = 0.60;
    const P_ELEVATED: f32 = 0.95;
    let fid = *composer.shared_flights.keys().next().expect("flight alive");
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .set_progress(P_TREE_ORDER);
    frame(&mut composer);
    {
        let tslot = composer
            .shared_flights
            .values()
            .next()
            .and_then(|a| a.flight.target_slot)
            .expect("flight still alive in the window");
        let root = composer.layout_root_idx().expect("root");
        let tidx = find_idx_by_slot(composer.arena_nodes(), root, tslot)
            .expect("target node");
        sample = Some((P_TREE_ORDER, tslot, tidx));
    }
    let (p_at_probe, _tslot, tidx) =
        sample.expect("flight must be sampled inside its late window (machine load?)");

    // Same frame, one flag flipped: opt-out (as built) vs elevated.
    let (probe_x, probe_y) = (150.0, 40.0);
    let mut in_tree = render_heads(&composer);
    let c_tree = pixel_rgb(&mut in_tree, probe_x as i32, probe_y as i32);
    assert!(
        c_tree.1 > 180 && c_tree.2 < 80,
        "opt-out: pinned bar still covers the hero, got {c_tree:?}"
    );

    let tslot = _tslot;
    let root = composer.layout_root_idx().expect("root");
    let tidx = find_idx_by_slot(composer.arena_nodes(), root, tslot).expect("target");
    // Same flight, later moment: this is where the elevated hero is over the bar (measured).
    composer
        .shared_flights
        .get_mut(&fid)
        .expect("flight")
        .set_progress(P_ELEVATED);
    frame(&mut composer);
    composer.arena.nodes[tidx].transition.as_mut().unwrap().elevated = true;
    composer.elevated_roots.push(tidx);
    composer.rebuild_layer_order();
    let mut elevated = render_heads(&composer);
    let c_elev = pixel_rgb(&mut elevated, probe_x as i32, probe_y as i32);
    assert!(
        c_elev.2 > 150 && c_elev.1 < 120,
        "elevated: hero paints over the pinned bar, got {c_elev:?}"
    );
    // The ghost really is in flight at the sampled moment (otherwise the
    // "the bar stays on top" claim above would be vacuous): sample a point
    // the ghost covers but the bar does not. Over the white surface the
    // ghost pulls green down (255·p), where an untilted background is 255.
    let ghost = pixel_rgb(&mut elevated, probe_x as i32, 70);
    assert!(
        ghost.1 < 250,
        "the leaving ghost must tint the strip at p={p_at_probe}, got {ghost:?}"
    );
    // ...and over the bar itself the wash is bounded by the ghost's alpha.
    let wash = 255.0 * (1.0 - p_at_probe);
    assert!(
        c_tree.0 as f32 <= wash + 30.0,
        "at most the ghost wash (1-p)={wash:.0} shows over the bar, got red={} at p={p_at_probe}",
        c_tree.0
    );

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        frame(&mut composer);
    }
    crate::animation::clear_all_animations();
}
