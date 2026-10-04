use super::*;

#[test]
fn chrome_nested_inside_chrome_keeps_its_own_layer_entry() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let frame = |composer: &mut Composer, show: &State<bool>| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        if s.get() {
                            under_bar_list(ctx, &scope, true);
                        } else {
                            under_bar_detail(ctx, &scope, true);
                        }
                    });
                    // Outer chrome, with an INNER chrome marker inside it.
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(400.0, 60.0)
                                .background(Color::GREEN, Shape::Rectangle)
                                .render_in_shared_transition_scope_overlay(&scope, 0.0),
                        )
                        .build(ctx, |ctx| {
                            Column::new()
                                .modifier(
                                    Modifier::new()
                                        .size(200.0, 30.0)
                                        .render_in_shared_transition_scope_overlay(&scope, 0.0),
                                )
                                .build(ctx, |_| {});
                        });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };

    frame(&mut composer, &show);
    show.set(false);
    frame(&mut composer, &show);
    assert!(
        composer.shared_flights.values().any(|f| !is_terminal(f.flight.phase)),
        "a flight must be running for chrome to elevate"
    );
    let marked: Vec<usize> = composer
        .arena_nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| find_scope_overlay_marker(&n.modifier).is_some())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(marked.len(), 2, "both chrome markers exist in the tree");
    assert_eq!(
        composer.scope_overlay_roots.len(),
        2,
        "both chrome markers are layer roots, so the inner keeps its own z and clip escape"
    );
    for idx in &marked {
        assert_eq!(
            composer.arena_nodes()[*idx].paint,
            PaintDisposition::InLayer,
            "every chrome root is painted by the layer"
        );
    }
    assert!(
        composer.layer_order.iter().filter(|i| marked.contains(i)).count() == 2,
        "…and both are in the layer's draw order (layer_order={:?})",
        composer.layer_order
    );
    crate::animation::clear_all_animations();
}

#[test]
fn scope_overlay_chrome_covers_both_flight_ends() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    // Heroes elevated: the A/B must isolate the CHROME elevation, so the
    // flight has to be in the layer too (otherwise the bar covers it by
    // plain tree order and the comparison proves nothing).
    chrome_frame(&mut composer, &show, 1.0, true);
    show.set(false);
    chrome_frame(&mut composer, &show, 1.0, true);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");
    let bar = chrome_bar_index(&composer);

    for _ in 0..200 {
        match flight_probe(&composer) {
            Some((p, _)) if p >= 0.9 => break,
            None => break,
            _ => {}
        }
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        chrome_frame(&mut composer, &show, 1.0, true);
    }

    // The claim first: chrome stays on top of the flight. Without the
    // chrome elevation the flying pair (both ends are layer roots) paints
    // over the bar instead — this is what fails pre-feature.
    let (probe_x, probe_y) = (150.0, 40.0);
    let p_at_probe = flight_probe(&composer).map(|(p, _)| p).unwrap_or(1.0);
    let mut covered = render_heads(&composer);
    let c_cov = pixel_rgb(&mut covered, probe_x as i32, probe_y as i32);
    assert!(
        c_cov.1 > 180 && c_cov.2 < 80,
        "chrome stays on top of both ends, got {c_cov:?}"
    );
    // The one thing chrome cannot hide is the leaving ghost's own opacity:
    // the ghost is a layer root too, but at z 0 the bar is drawn after it,
    // so what remains is a wash bounded by (1 - p) over the bar's colour.
    let wash = 255.0 * (1.0 - p_at_probe);
    assert!(
        c_cov.0 as f32 <= wash + 45.0,
        "at most the ghost wash (1-p) shows over the bar, got red={} at p={p_at_probe}",
        c_cov.0
    );

    // Paint order = hit order: the bar is painted last, so a tap inside the
    // overlap must reach the BAR, not the hero flying under it (a pinned
    // bar's buttons would otherwise be dead for the whole flight).
    {
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("root");
        let path = hit_test_with_flights(
            nodes,
            root,
            composer.transition_roots(),
            probe_x,
            probe_y,
        );
        assert!(
            path.contains(&bar),
            "tap in the overlap must reach the chrome, got {path:?}"
        );
    }

    // Then the mechanism.
    assert!(
        composer.arena_nodes()[bar].paint == crate::layout::node::PaintDisposition::InLayer,
        "chrome elevates while the scope is transitioning"
    );
    assert!(
        composer.transition_roots().contains(&bar),
        "chrome joins the layer"
    );

    // Same frame, one thing removed: without the elevation the flight wins
    // again — on screen AND for input.
    composer.arena.nodes[bar].paint = crate::layout::node::PaintDisposition::InTree;
    composer.scope_overlay_roots.clear();
    composer.rebuild_layer_order();
    let mut bare = render_heads(&composer);
    let c_bare = pixel_rgb(&mut bare, probe_x as i32, probe_y as i32);
    assert!(
        c_bare.2 > 120,
        "without the chrome elevation the flight covers the bar, got {c_bare:?}"
    );
    {
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("root");
        let path = hit_test_with_flights(
            nodes,
            root,
            composer.transition_roots(),
            probe_x,
            probe_y,
        );
        assert!(
            !path.contains(&bar),
            "unelevated chrome gets no special hit routing, got {path:?}"
        );
    }

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        chrome_frame(&mut composer, &show, 1.0, true);
    }
    assert!(composer.shared_flights.is_empty(), "flight completes");
    crate::animation::clear_all_animations();
}

#[test]
fn scope_overlay_chrome_follows_the_transition_window() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    chrome_frame(&mut composer, &show, 1.0, false);
    let bar = chrome_bar_index(&composer);
    assert!(
        composer.arena_nodes()[bar].paint == PaintDisposition::InTree,
        "idle: in tree"
    );
    assert!(
        !composer.transition_roots().contains(&bar),
        "idle: not a layer root"
    );
    assert!(composer.transition_roots().is_empty(), "no layer when idle");

    // ...and after the flight finishes it goes back to the tree.
    show.set(false);
    chrome_frame(&mut composer, &show, 1.0, false);
    assert!(
        composer.arena_nodes()[bar].paint == crate::layout::node::PaintDisposition::InLayer,
        "flying: elevated"
    );
    for _ in 0..300 {
        if composer.shared_flights.is_empty() {
            break;
        }
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        chrome_frame(&mut composer, &show, 1.0, false);
    }
    assert!(composer.shared_flights.is_empty(), "flight completes");
    // Membership is decided by the poll, so the settled state is the NEXT
    // frame's (the completing frame still draws the previous decision —
    // same pixels, just a different pass).
    chrome_frame(&mut composer, &show, 1.0, false);
    let bar = chrome_bar_index(&composer);
    assert!(
        composer.arena_nodes()[bar].paint == PaintDisposition::InTree,
        "settled: back in tree order"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn scope_overlay_z_orders_chrome_against_flights() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    for (chrome_z, expect_bar_last) in [(-1.0f32, false), (1.0, true)] {
        let mut composer = Composer::new();
        let show = State::new(true);
        chrome_frame(&mut composer, &show, chrome_z, false);
        show.set(false);
        chrome_frame(&mut composer, &show, chrome_z, false);
        assert_eq!(composer.shared_flights.len(), 1, "flight opened");
        let bar = chrome_bar_index(&composer);
        let order = composer.transition_roots().to_vec();
        assert_eq!(
            order.last().copied() == Some(bar),
            expect_bar_last,
            "chrome z={chrome_z} in layer order {order:?}"
        );
        crate::animation::clear_all_animations();
    }
}

#[test]
fn scope_overlay_chrome_elevates_in_a_peer_composer() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut a = Composer::new();
    let mut b = Composer::new();
    let scope = SharedTransitionScope::new(99);
    let show_a = State::new(true);
    let show_b = State::new(false);

    let sa = show_a.clone();
    let sb = show_b.clone();
    let (sca, scb) = (scope.clone(), scope.clone());
    // Owner: the leaving hero. Peer (dialog): the arriving hero plus a
    // pinned bar marked for the same scope.
    let frame = |a: &mut Composer, b: &mut Composer| {
        let (x, y) = (sa.clone(), sb.clone());
        let (ka, kb) = (sca.clone(), scb.clone());
        a.compose(|ctx| {
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if x.get() {
                    hero_leaf(ctx, 60.0, 60.0, Color::RED, &ka, true);
                }
            });
        });
        b.compose(|ctx| {
            Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                if y.get() {
                    Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                        hero_leaf(ctx, 300.0, 160.0, Color::BLUE, &kb, true);
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .size(400.0, 60.0)
                                    .background(Color::GREEN, Shape::Rectangle)
                                    .render_in_shared_transition_scope_overlay(&kb, 1.0),
                            )
                            .build(ctx, |_| {});
                    });
                }
            });
        });
        a.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        b.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        a.poll_shared_flights();
        b.poll_shared_flights();
        let mut all: Vec<&mut Composer> = vec![a, b];
        Composer::poll_cross_flights(&mut all);
    };

    frame(&mut a, &mut b);
    show_a.set(false);
    show_b.set(true);
    frame(&mut a, &mut b);
    assert_eq!(a.shared_flights.len(), 1, "Tier1 flight opened");
    assert!(b.shared_flights.is_empty(), "peer owns no flight map entry");

    let bar = b
        .arena_nodes()
        .iter()
        .position(|n| find_scope_overlay_marker(&n.modifier).is_some())
        .expect("peer chrome bar");
    assert!(
        b.arena_nodes()[bar].paint == PaintDisposition::InLayer,
        "peer chrome must elevate for a flight it does not own"
    );
    assert!(
        b.transition_roots().contains(&bar),
        "peer chrome joins the peer's layer"
    );

    for _ in 0..200 {
        if a.shared_flights.is_empty() {
            break;
        }
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        frame(&mut a, &mut b);
    }
    assert!(a.shared_flights.is_empty(), "Tier1 completes");
    frame(&mut a, &mut b);
    let bar = b
        .arena_nodes()
        .iter()
        .position(|n| find_scope_overlay_marker(&n.modifier).is_some())
        .expect("peer chrome bar");
    assert!(
        b.arena_nodes()[bar].paint == PaintDisposition::InTree,
        "peer chrome returns to tree order when the flight ends"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn scope_overlay_equal_z_order_is_target_then_ghost_then_chrome() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    chrome_frame(&mut composer, &show, 0.0, true);
    show.set(false);
    chrome_frame(&mut composer, &show, 0.0, true);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");

    let bar = chrome_bar_index(&composer);
    let nodes = composer.arena_nodes();
    let order = composer.transition_roots().to_vec();
    let pos = |idx: usize| order.iter().position(|&i| i == idx);
    let bar_at = pos(bar).expect("chrome in the layer");
    assert_eq!(
        bar_at,
        order.len() - 1,
        "chrome is painted last at equal z, got {order:?}"
    );
    let target = composer
        .arena_nodes()
        .iter()
        .position(|n| {
            n.transition
                .as_ref()
                .is_some_and(|t| t.role == TransitionRole::Target && t.elevated)
        })
        .expect("elevated target");
    assert!(
        pos(target).expect("target in the layer") < bar_at,
        "the entering end sits under the ghost and the chrome, got {order:?}"
    );
    let _ = nodes;
    crate::animation::clear_all_animations();
}

#[test]
fn morph_never_elevates_even_with_the_flag_on() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let w = State::new(80.0f32);
    let morph_frame = |composer: &mut Composer| {
        let ww = w.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(ww.get(), 60.0)
                                .background(Color::BLUE, Shape::rounded(8.0))
                                .shared_element(
                                    scope.shared_content_state("hero"),
                                    BoundsTransform::default(),
                                    PlaceHolderSize::JumpCut,
                                    PathMotion::Linear,
                                    0.0,
                                    true,
                                ),
                        )
                        .build(ctx, |_| {});
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    morph_frame(&mut composer);
    // A morph requires a running transition (Compose semantics), so declare one before the size
    // change that is supposed to open it.
    mark_scope_active(&composer);
    w.set(200.0);
    morph_frame(&mut composer);
    assert_eq!(composer.shared_flights.len(), 1, "morph flight opened");
    let idx = marked_in(&composer)[0];
    let vis = composer.arena_nodes()[idx].transition.clone().expect("morph visual");
    assert_eq!(vis.role, TransitionRole::Morph, "same-screen resize");
    assert!(!vis.elevated, "a morph must stay in-tree");
    assert!(
        !composer.transition_roots().contains(&idx),
        "a morph must not join the layer"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn layered_chrome_is_not_painted_twice() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let translucent = |composer: &mut Composer, show: &State<bool>| {
        let s = show.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Stack::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_size())
                        .build(ctx, |ctx| {
                            if s.get() {
                                under_bar_list(ctx, &scope, true);
                            } else {
                                under_bar_detail(ctx, &scope, true);
                            }
                        });
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(400.0, 60.0)
                                .background(
                                    Color { r: 0, g: 255, b: 0, a: 128 },
                                    Shape::Rectangle,
                                )
                                .render_in_shared_transition_scope_overlay(&scope, 1.0),
                        )
                        .build(ctx, |_| {});
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    translucent(&mut composer, &show);
    show.set(false);
    translucent(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");

    // Inside the bar, clear of the flight's lerped rect (which stays left).
    let (px, py) = (370i32, 30i32);
    let mut surf = render_heads(&composer);
    let c = pixel_rgb(&mut surf, px, py);
    // One translucent green over white: 0.5*green + 0.5*white.
    assert!(
        c.0 > 100 && c.2 > 100,
        "a single draw leaves the background showing through, got {c:?}"
    );
    assert!(
        !close_enough(c, (63, 255, 63), 20),
        "the bar must not be composited twice, got {c:?}"
    );
    crate::animation::clear_all_animations();
}
