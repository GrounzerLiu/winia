use super::*;

#[test]
fn placeholder_copies_are_not_hittable() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        SharedTransitionLayout::new().build(ctx, |ctx| {
            let scope = current_shared_scope().expect("scope");
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                hero_leaf(ctx, 120.0, 80.0, Color::RED, &scope, true);
            });
        });
    });
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();

    let idx = marked_indices(&composer)[0];
    let (cx, cy) = {
        let nodes = composer.arena_nodes();
        (nodes[idx].content_box().width / 2.0, nodes[idx].content_box().height / 2.0)
    };
    let root = composer.layout_root_idx().expect("root");

    // Control: in the tree it is hittable, so the two assertions below mean something.
    assert!(
        hit_test(composer.arena_nodes(), root, cx, cy).contains(&idx),
        "control: an in-tree marked node is hittable"
    );

    composer.arena.nodes[idx].paint = PaintDisposition::Placeholder;
    let hit = hit_test(composer.arena_nodes(), root, cx, cy);
    assert!(
        !hit.contains(&idx),
        "an unpainted placeholder must not be hit (path={hit:?})"
    );

    // …and the exclusion is the frame's transient state, not a permanent one.
    composer.arena.nodes[idx].paint = PaintDisposition::InTree;
    assert!(
        hit_test(composer.arena_nodes(), root, cx, cy).contains(&idx),
        "back in the tree it is hittable again"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn remap_hit_identity_endpoints_miss_outside() {
    let vis = TransitionVisual {
        start: SharedBounds::new(0.0, 0.0, 100.0, 50.0),
        end: SharedBounds::new(200.0, 0.0, 100.0, 50.0),
        progress: 0.0,
        role: TransitionRole::Target,
        scene_alpha: None,
        radius_from: [0.0; 4],
        radius_to: [0.0; 4],
        clip: false,
        link_slot: None,
        scroll: (0.0, 0.0),
        flight: 0,
        path: PathMotion::Linear,
        bounds_fx: None,
        scale_parts: Some((ContentScale::FillBounds, ImageAlignment::TopStart)),
        elevated: false,
        remeasure: false,
        radius_from_auto: false,
        radius_to_auto: false,
    };
    assert_eq!(
        vis.remap_hit(10.0, 10.0, 0.0, 0.0, 100.0, 50.0),
        Some((10.0, 10.0)),
        "p=0 remaps identically"
    );
    assert_eq!(
        vis.remap_hit(500.0, 500.0, 0.0, 0.0, 100.0, 50.0),
        None,
        "visual miss passes through"
    );
    let mut done = vis.clone();
    done.progress = 1.0;
    assert_eq!(
        done.remap_hit(210.0, 10.0, 0.0, 0.0, 100.0, 50.0),
        Some((10.0, 10.0)),
        "p=1 maps the end rect back into layout space"
    );
}

#[test]
fn hit_routing_reaches_target_mid_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    frame(&mut composer, &show);
    show.set(false);
    frame(&mut composer, &show);
    advance(&mut composer, &show);
    advance(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "mid-flight");

    // Click inside the lerped ghost but outside the target natural rect
    // (early flight: ghost ≈ start (0,0,120,80), target at (0,100,…)).
    let nodes = composer.arena_nodes();
    let root = composer.layout_root_idx().unwrap();
    let routed = hit_test_with_flights(nodes, root, &composer.transition_roots(), 60.0, 40.0);
    let plain = hit_test(nodes, root, 60.0, 40.0);
    // Target hero index for comparison.
    let target = marked_indices(&composer);
    assert_eq!(target.len(), 1);
    let target_idx = target[0];
    assert_eq!(
        routed.last().copied(),
        Some(target_idx),
        "ghost/target-visual click routes into the live target subtree"
    );
    // MAJOR #3: the legacy walk reaches the live target through its own
    // remap too — no input blackout on transitioning endpoints (the old
    // v1 skip deliberately murdered this; the ghost prefix above only
    // adds detached-source routing on top).
    assert_eq!(
        plain.last().copied(),
        Some(target_idx),
        "plain walk reaches the live target via its own visual remap"
    );
    crate::animation::clear_all_animations();
}
