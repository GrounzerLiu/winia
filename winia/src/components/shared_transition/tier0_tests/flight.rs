use super::*;

#[cfg(feature = "anim-trace")]
#[test]
fn anim_trace_records_a_headless_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    crate::anim_trace::capture_start();
    let mut composer = Composer::new();
    let show = State::new(true);
    frame(&mut composer, &show);
    show.set(false);
    // The switch frame itself opens the flight; the loop below then runs it out.
    frame(&mut composer, &show);
    assert!(!composer.shared_flights.is_empty(), "the switch opens a flight");
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        advance(&mut composer, &show);
    }
    let recs = crate::anim_trace::capture_take();
    crate::anim_trace::capture_stop();

    let of = |role: &str| -> Vec<crate::anim_trace::TraceRecord> {
        recs.iter()
            .map(|(_, _, r)| r.clone())
            .filter(|r| r.kind == crate::anim_trace::TraceKind::Flight)
            .filter(|r| r.role == Some(role))
            .collect()
    };
    let source = of("Source");
    let target = of("Target");
    assert!(
        !source.is_empty() && !target.is_empty(),
        "a flight must be traced for both ends (source {}, target {})",
        source.len(),
        target.len()
    );
    let painted_w = |r: &crate::anim_trace::TraceRecord| r.painted.map(|p| p.w).unwrap_or(0.0);
    // Both ends draw the SAME lerped rect, which starts at the source's size (this scene's list
    // hero is 120x80) and finishes at the target's (the detail hero is 300x160).
    assert!(
        (painted_w(&target[0]) - painted_w(&source[0])).abs() < 1.0,
        "both ends must draw the same rect at the start ({:.0} vs {:.0})",
        painted_w(&target[0]),
        painted_w(&source[0])
    );
    assert!(
        painted_w(&source[0]) < 150.0,
        "…which is the source's own size, got {:.0}",
        painted_w(&source[0])
    );
    let last = target.last().expect("target records");
    assert!(
        painted_w(last) > 280.0,
        "…and finish at the target rect, got {:.0}",
        painted_w(last)
    );
    // Opacity crossfades: the leaving end starts opaque and ends invisible, the entering one the
    // reverse. Both are recorded, in three parts.
    let a = |r: &crate::anim_trace::TraceRecord| r.alpha.unwrap_or(-1.0);
    assert!(a(&source[0]) > 0.9, "the leaving end starts opaque");
    assert!(a(source.last().unwrap()) < 0.1, "…and ends invisible");
    assert!(a(&target[0]) < 0.1, "the entering end starts invisible");
    assert!(a(last) > 0.9, "…and ends opaque");
    assert!(
        source[0].effective_alpha.is_some() && source[0].layout.is_some(),
        "each record also carries the composited alpha and the layout rect"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_flight_completes_end_to_end() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    frame(&mut composer, &show);
    assert!(composer.shared_flights.is_empty(), "no flight before any switch");
    assert!(composer.transition_layer.is_empty());

    // Switch list → detail.
    show.set(false);
    frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "switch opens exactly one flight");
    let fid = *composer.shared_flights.keys().next().unwrap();
    assert_eq!(
        composer.shared_flights[&fid].flight.phase,
        FlightPhase::Flying,
        "target resolved in the same post-layout poll"
    );
    assert_eq!(composer.transition_layer.len(), 1, "source retained + detached");
    // Exactly one live marked node (the target); it carries Target visuals.
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1, "one live endpoint (target)");
    let tvis = composer.arena_nodes()[marked[0]].transition.clone();
    assert!(matches!(tvis, Some(ref v) if v.role == TransitionRole::Target));
    // Retained source carries Source visuals at progress ~0.
    let src_idx = composer.transition_layer[0];
    let svis = composer.arena_nodes()[src_idx].transition.clone();
    assert!(matches!(svis, Some(ref v) if v.role == TransitionRole::Source && v.progress < 0.5));

    // p=0 frame paint: source opaque at its start center.
    let (sx, sy) = node_center(&composer, src_idx);
    let mut surf0 = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf0, sx, sy), (255, 0, 0), 30),
        "p=0 must match the pre-switch frame (source opaque)"
    );

    // Advance into a visible mid state (poll — timing-immune).
    let mut mid_seen = false;
    for _ in 0..40 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let p = composer.shared_flights.values().next().map(|a| a.progress.peek()).unwrap_or(1.0);
        if p > 0.2 && p < 0.95 {
            mid_seen = true;
            break;
        }
        advance(&mut composer, &show);
    }
    assert!(mid_seen, "flight must pass through a visible mid state");
    // Mid-flight paint moved/faded at the start center.
    let mut surf_mid = render_heads(&composer);
    let c = pixel_rgb(&mut surf_mid, sx, sy);
    assert!(!close_enough(c, (255, 0, 0), 30), "mid-flight paint moved/faded, got {c:?}");

    // Run to completion.
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "flight completes");
    assert!(composer.transition_layer.is_empty(), "retained source freed");
    for idx in marked_indices(&composer) {
        assert!(
            composer.arena_nodes()[idx].transition.is_none(),
            "target visuals cleared — natural render resumes"
        );
    }
    // End-state paint: detail hero center BLUE.
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let (ex, ey) = node_center(&composer, marked[0]);
    let mut surf_end = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf_end, ex, ey), (0, 0, 255), 30),
        "end state shows the detail hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn marker_flag_refreshes_only_when_read_in_the_marker_slot() {
    let _g = lock_serial();
    let marker_flag = |composer: &Composer| -> Option<bool> {
        composer
            .arena_nodes()
            .iter()
            .find_map(|n| find_shared_marker(&n.modifier).map(|m| m.render_in_overlay))
    };

    // Read INSIDE the marker's slot → refreshed.
    let flag = State::new(true);
    let mut composer = Composer::new();
    let build = |composer: &mut Composer| {
        let f = flag.clone();
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    let inner = f.get();
                    hero_leaf(ctx, 120.0, 80.0, Color::RED, &scope, inner);
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    };
    build(&mut composer);
    assert_eq!(marker_flag(&composer), Some(true), "initial flag");
    flag.set(false);
    build(&mut composer);
    assert_eq!(
        marker_flag(&composer),
        Some(false),
        "a flag read in the marker's own slot must reach the arena"
    );

    // Read in an OUTER slot → this is the trap, documented as such.
    let flag = State::new(true);
    let mut composer = Composer::new();
    let build = |composer: &mut Composer| {
        let f = flag.clone();
        composer.compose(|ctx| {
            let outer = f.get();
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    hero_leaf(ctx, 120.0, 80.0, Color::RED, &scope, outer);
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    };
    build(&mut composer);
    flag.set(false);
    build(&mut composer);
    assert_eq!(
        marker_flag(&composer),
        Some(true),
        "outer read leaves the subtree skipped — the marker keeps its cached flag \
         (this is why the demo reads the toggle inside the screen slot)"
    );

    // Nested builders, exactly the demo's shape, with the flag read inside
    // the composable that creates the marked node (the demo's `hero_box`
    // helper). That is the pattern that keeps the arena marker fresh.
    let flag = State::new(true);
    let show = State::new(false);
    let mut composer = Composer::new();
    let build = |composer: &mut Composer| {
        let (f, s) = (flag.clone(), show.clone());
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("scope");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        gap_leaf(ctx, 10.0, 60.0);
                        flag_hero(ctx, &f, &scope, 60.0, 60.0, Color::RED);
                    } else {
                        Row::new()
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |ctx| {
                                gap_leaf(ctx, 40.0, 10.0);
                                Column::new()
                                    .modifier(Modifier::new().clip(Shape::Rectangle))
                                    .build(ctx, |ctx| {
                                        flag_hero(ctx, &f, &scope, 120.0, 80.0, Color::BLUE);
                                    });
                            });
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    };
    build(&mut composer);
    assert_eq!(marker_flag(&composer), Some(true), "nested initial");
    flag.set(false);
    build(&mut composer);
    assert_eq!(
        marker_flag(&composer),
        Some(false),
        "a flag read inside the composable that builds the marker reaches it \
         through any nesting depth"
    );
}

#[test]
fn writer_never_clobbers_another_flights_override() {
    use crate::transition::{FlightMeasure, FlightMeasureFrame};
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
    let marked = marked_in(&composer)[0];

    // A foreign flight owns this node's override (another composer's id).
    let foreign_state = State::new(FlightMeasureFrame::IDLE);
    let foreign = FlightMeasure {
        frame: foreign_state.clone(),
        owner: FlightKey { cid: composer.composer_id + 1000, id: 1 },
    };
    composer.arena.nodes[marked].flight_measure = Some(foreign);
    // Run frames of the real flight that targets this node. The default
    // contract writes an IDLE frame, which used to CLEAR the override.
    for _ in 0..3 {
        frame(&mut composer);
    }
    let kept = composer.arena_nodes()[marked].flight_measure.clone();
    assert!(
        kept.is_some_and(|f| f.owner.cid == composer.composer_id + 1000),
        "a foreign flight's override must survive this flight's writes"
    );

    // …and the CLEAR side must be just as namespaced: tearing down THIS
    // composer's flights must not drop a different flight's override.
    let key = composer.arena_nodes()[marked].slot_key;
    composer.clear_transition_for_slot(key, FlightKey { cid: composer.composer_id, id: 1 });
    assert!(
        composer.arena_nodes()[marked]
            .flight_measure
            .as_ref()
            .is_some_and(|f| f.owner.cid == composer.composer_id + 1000),
        "a teardown for another flight must leave the foreign override alone"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_writer_skips_a_node_that_is_not_the_shared_endpoint() {
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
                    list_layout_screen(ctx, &scope, ResizeMode::scale_to_bounds(), PlaceHolderSize::JumpCut);
                } else {
                    detail_layout_screen(ctx, &scope, ResizeMode::RemeasureToBounds, PlaceHolderSize::AnimatedSize);
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

    // The slot now holds something that is NOT this flight's endpoint: strip the
    // marker (no compose, so the tree is left as mutated) and run the writer.
    composer.arena.nodes[marked].modifier = Modifier::new();
    composer.arena.nodes[marked].transition = None;
    composer.arena.nodes[marked].flight_measure = None;
    composer.poll_shared_flights();

    assert!(
        composer.arena_nodes()[marked].transition.is_none(),
        "the writer must not paint a flight onto a node that is not its endpoint"
    );
    assert!(
        composer.arena_nodes()[marked].flight_measure.is_none(),
        "…nor attach its layout override there"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn switch_frame_layout_uses_the_animated_size() {
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
                    switch_frame_list(ctx, &scope);
                } else {
                    switch_frame_detail(ctx, &scope);
                }
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
        // The app loop's bounded extra pass (`app.rs`), taken once per frame.
        if composer.take_layout_override_fresh() {
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        }
    };
    frame(&mut composer);
    show.set(false);
    frame(&mut composer);
    assert_eq!(composer.shared_flights.len(), 1, "the switch opened a flight");

    // The frame that just ran is the SWITCH frame: the probe must sit right under the
    // animated (source) hero, not under the detail hero's natural height.
    let probe_y = {
        let nodes = composer.arena_nodes();
        let probe = nodes
            .iter()
            .find(|n| {
                (n.measured_size.width - 40.0).abs() < 0.5
                    && (n.measured_size.height - 20.0).abs() < 0.5
            })
            .expect("the probe leaf (40x20)");
        probe.position.y
    };
    assert!(
        probe_y < 120.0,
        "the parent must be laid out at the ANIMATED size on the switch frame \
         (probe y {probe_y}; the detail hero's natural 240 would put it well below 120)"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_bouncy_spring_overshoot_renders_then_settles() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    spring_frame(&mut composer, &show, PathMotion::Linear);
    show.set(false);
    spring_frame(&mut composer, &show, PathMotion::Linear);
    assert_eq!(composer.shared_flights.len(), 1, "flight opened");

    // The old `p >= 0.999` gate reaped the flight on first passage —
    // before the overshoot peak — so no live frame ever exceeded 1.0.
    let mut max_p = 0.0f32;
    let mut overshoot_painted = false;
    for _ in 0..400 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let probe = composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let e = a.flight.end.expect("end resolved after first poll");
            (p, a.start.lerp(&e, p))
        });
        if let Some((p, l)) = probe {
            max_p = max_p.max(p);
            // First live frame past the end: the target (alpha clamped
            // to 1, source gone) paints the overshot rect — its center
            // must read BLUE, proving paint follows the overshoot.
            if p > 1.0 && !overshoot_painted {
                let mut surf = render_heads(&composer);
                let cx = (l.x + l.width / 2.0) as i32;
                let cy = (l.y + l.height / 2.0) as i32;
                let c = pixel_rgb(&mut surf, cx, cy);
                assert!(
                    c.2 > 150 && c.0 < 120,
                    "overshot rect paints the arrived target, got {c:?} at p={p}"
                );
                overshoot_painted = true;
            }
        }
        spring_advance(&mut composer, &show, PathMotion::Linear);
    }
    assert!(max_p > 1.0, "bouncy spring overshoots past 1.0 while alive, got {max_p}");
    assert!(overshoot_painted, "overshoot frames must render before settle");
    assert!(composer.shared_flights.is_empty(), "flight settles after overshoot");
    assert!(composer.transition_layer.is_empty(), "retained source freed");
    // Settle lands exactly on the detail hero.
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let (ex, ey) = node_center(&composer, marked[0]);
    let mut surf = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
        "settled end state shows the detail hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_retarget_restarts_from_visual() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    frame(&mut composer, &show);
    show.set(false);
    frame(&mut composer, &show);
    for _ in 0..3 {
        advance(&mut composer, &show);
    }
    assert_eq!(composer.shared_flights.len(), 1, "mid-flight");
    // Navigate back mid-flight: old flight cancelled, exactly one fresh
    // flight opens, exactly one node retained (no leak, no double).
    show.set(true);
    frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "retarget replaces, never duplicates");
    assert_eq!(composer.transition_layer.len(), 1, "old retained freed, new retained");

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "retargeted flight completes");
    assert!(composer.transition_layer.is_empty());
    // Back on the list: RED hero at its natural spot.
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let (ex, ey) = node_center(&composer, marked[0]);
    let mut surf = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (255, 0, 0), 30),
        "navigated back to the list hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_shared_bounds_enter_exit_slide_with_flight() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    bounds_frame(&mut composer, &show);
    show.set(false);
    bounds_frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "bounds flight opened");
    {
        let a = composer.shared_flights.values().next().expect("flight");
        let (enter, exit) = a.bounds_fx.clone().expect("bounds pair carried");
        assert!(enter.fade && exit.fade, "test pair is slide+fade");
        assert!(enter.slide.is_some() && exit.slide.is_some());
    }

    // Tween-linear flight: at p≈0.5 the target sits 80px above the
    // lerped center (slide-in from Up) and the ghost 80px below it
    // (slide-out Down) — three decisive probes, no overlap.
    let mut slid = false;
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let probe = composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let e = a.flight.end.expect("end resolved after first poll");
            let l = a.start.lerp(&e, p);
            ((l.x + l.width / 2.0) as i32, (l.y + l.height / 2.0) as i32, p)
        });
        if let Some((cx, cy, p)) = probe {
            if p > 0.45 && p < 0.55 {
                // Target paint (faded blue) 80px above the lerped center.
                let mut surf = render_heads(&composer);
                let up = pixel_rgb(&mut surf, cx, cy - 80);
                assert!(
                    up.2 > 150 && up.0 < 150,
                    "enter slide lifts target paint above, got {up:?} at p={p}"
                );
                // Ghost paint (faded red) 80px below it.
                let mut surf2 = render_heads(&composer);
                let down = pixel_rgb(&mut surf2, cx, cy + 80);
                assert!(
                    down.0 > 200 && down.2 < 200,
                    "exit slide drops ghost paint below, got {down:?} at p={p}"
                );
                // The lerped center itself is background (both moved away).
                let mut surf3 = render_heads(&composer);
                let mid = pixel_rgb(&mut surf3, cx, cy);
                assert!(
                    close_enough(mid, (255, 255, 255), 40),
                    "lerped center vacated by both slides, got {mid:?} at p={p}"
                );
                slid = true;
                break;
            }
        }
        bounds_advance(&mut composer, &show);
    }
    assert!(slid, "flight must pass through the slide window");

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        bounds_advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "bounds flight completes");
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let (ex, ey) = node_center(&composer, marked[0]);
    let mut surf = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
        "settled end state shows the detail hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_shared_bounds_expand_wipes_from_edge() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    expand_frame(&mut composer, &show);
    show.set(false);
    expand_frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "expand flight opened");

    // Tween-linear flight at p≈0.5: the target covers the TOP half of
    // the lerped rect (grown down from the top edge); the bottom half
    // is background. Ghost mirrors from the same edge.
    let mut wiped = false;
    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        let probe = composer.shared_flights.values().next().map(|a| {
            let p = a.progress.peek();
            let e = a.flight.end.expect("end resolved after first poll");
            let l = a.start.lerp(&e, p);
            (p, (l.x + l.width / 2.0) as i32, l.y as i32, l.height)
        });
        if let Some((p, cx, top, h)) = probe {
            if p > 0.45 && p < 0.55 {
                // Just inside the top edge: grown paint — ghost red over
                // target blue (both wipe from the top edge, both faded).
                let mut surf = render_heads(&composer);
                let grown = pixel_rgb(&mut surf, cx, top + 10);
                assert!(
                    grown.0 > 140 && grown.1 < 120 && grown.2 > 100,
                    "top edge shows grown paint, got {grown:?} at p={p}"
                );
                // Just inside the bottom edge: not yet grown.
                let mut surf2 = render_heads(&composer);
                let pending = pixel_rgb(&mut surf2, cx, (top as f32 + h - 10.0) as i32);
                assert!(
                    close_enough(pending, (255, 255, 255), 40),
                    "bottom edge still background, got {pending:?} at p={p}"
                );
                wiped = true;
                break;
            }
        }
        expand_advance(&mut composer, &show);
    }
    assert!(wiped, "flight must pass through the wipe window");

    for _ in 0..200 {
        if composer.shared_flights.is_empty() {
            break;
        }
        expand_advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "expand flight completes");
    crate::animation::clear_all_animations();
}

#[test]
fn tier0_double_retarget_settles() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);

    frame(&mut composer, &show);
    show.set(false);
    frame(&mut composer, &show);
    for _ in 0..2 {
        advance(&mut composer, &show);
    }
    show.set(true);
    frame(&mut composer, &show);
    for _ in 0..2 {
        advance(&mut composer, &show);
    }
    show.set(false);
    frame(&mut composer, &show);
    assert_eq!(composer.shared_flights.len(), 1, "exactly one flight after double retarget");
    assert!(composer.transition_layer.len() <= 1, "never more than one retained node");

    for _ in 0..300 {
        if composer.shared_flights.is_empty() {
            break;
        }
        advance(&mut composer, &show);
    }
    assert!(composer.shared_flights.is_empty(), "settles");
    assert!(composer.transition_layer.is_empty(), "nothing retained");
    let marked = marked_indices(&composer);
    assert_eq!(marked.len(), 1);
    let (ex, ey) = node_center(&composer, marked[0]);
    let mut surf = render_heads(&composer);
    assert!(
        close_enough(pixel_rgb(&mut surf, ex, ey), (0, 0, 255), 30),
        "ends on the detail hero"
    );
    crate::animation::clear_all_animations();
}

#[test]
fn switch_frame_opens_one_flight_per_marked_key_and_absorbs_later_rect_changes() {
    let _g = lock_serial();
    crate::animation::clear_all_animations();
    let mut composer = Composer::new();
    let show = State::new(true);
    let w = State::new(120.0f32);

    let mixed_frame = |composer: &mut Composer| {
        let (s, ww) = (show.clone(), w.clone());
        composer.compose(|ctx| {
            SharedTransitionLayout::new().build(ctx, |ctx| {
                let scope = current_shared_scope().expect("inside SharedTransitionLayout");
                Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
                    if s.get() {
                        mixed_list_screen(ctx, &scope, &ww);
                    } else {
                        mixed_detail_screen(ctx, &scope, &ww);
                    }
                });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.poll_shared_flights();
    };
    let mixed_advance = |composer: &mut Composer| {
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(16));
        mixed_frame(composer);
    };

    mixed_frame(&mut composer);
    // A morph now requires a running transition, so the order is: switch first (that IS the
    // transition), then the layout-only size change that morphs alongside it. The switch frame alone
    // already opens TWO flights — the hero's switch and a morph for the marked leaf, whose rect the
    // new layout moved — which is the rule working: a rect change inside a transition morphs.
    show.set(false);
    mixed_frame(&mut composer);
    let summary = |composer: &Composer| {
        composer
            .shared_flights
            .values()
            .map(|a| {
                format!(
                    "{}(src={}, target={:?})",
                    a.flight.key,
                    a.source_idx.is_some(),
                    a.flight.target_slot.is_some()
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    assert_eq!(
        composer.shared_flights.len(),
        2,
        "the switch frame opens the hero's switch plus the morph: {}",
        summary(&composer)
    );
    assert_eq!(
        composer
            .shared_flights
            .values()
            .filter(|a| a.source_idx.is_none())
            .count(),
        0,
        "both are switches: in this scene the second marked key changes slot too, so it switches \
         rather than morphs — a morph is covered where the key keeps its slot ({})",
        summary(&composer)
    );
    // Layout-only size change, no compose: both keys already own a flight, so nothing new opens
    // (a second flight for one key would fight for the same visuals).
    w.set(300.0);
    composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    composer.poll_shared_flights();
    assert_eq!(
        composer.shared_flights.len(),
        2,
        "a rect change during a flight is absorbed by that key's flight: {}",
        summary(&composer)
    );

    for _ in 0..300 {
        if composer.shared_flights.is_empty() {
            break;
        }
        mixed_advance(&mut composer);
    }
    assert!(composer.shared_flights.is_empty(), "both complete");
    assert!(composer.transition_layer.is_empty(), "nothing retained");
    for idx in marked_indices(&composer) {
        assert!(composer.arena_nodes()[idx].transition.is_none(), "all visuals cleared");
    }
    crate::animation::clear_all_animations();
}
