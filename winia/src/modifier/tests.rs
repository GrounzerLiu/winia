    use super::*;

    /// `snap_target` is Compose's `calculateSnapOffset` + `calculateFinalOffset` for a uniformly
    /// paged list, and the property that matters is its CEILING: one gesture moves at most one page.
    #[test]
    fn a_snapping_fling_moves_at_most_one_page() {
        const STEP: f32 = 336.0;
        const MIN_FLING: f32 = 400.0;
        let snap = SnapInfo { step: STEP, min_fling_velocity: MIN_FLING, ..Default::default() };
        let limit = STEP * 10.0;

        // Anywhere inside a page, any velocity: the answer is one of the two neighbours, never further.
        for offset in [0.0f32, 1.0, 100.0, 167.9, 168.1, 335.0] {
            for velocity in [-9000.0f32, -MIN_FLING, -MIN_FLING + 1.0, 0.0, MIN_FLING, MIN_FLING - 1.0, 9000.0] {
                let target = snap_target(offset, velocity, &snap, limit);
                assert!(
                    (target - offset).abs() <= STEP + 0.5,
                    "offset {offset} at velocity {velocity} moved {} — more than one page",
                    (target - offset).abs()
                );
                assert_eq!(
                    (target / STEP - (target / STEP).round()).abs() < 1e-3,
                    true,
                    "target {target} is not a page boundary"
                );
            }
        }

        // A hard flick goes the way the gesture went; a slow one settles on the CLOSER page — which is
        // how a small push returns the picker to the month it started on.
        assert_eq!(snap_target(100.0, 9000.0, &snap, limit), STEP, "hard forward -> next page");
        assert_eq!(snap_target(100.0, -9000.0, &snap, limit), 0.0, "hard back -> previous page");
        assert_eq!(snap_target(100.0, 10.0, &snap, limit), 0.0, "a nudge -> the nearer page");
        assert_eq!(snap_target(300.0, 10.0, &snap, limit), STEP, "a nudge past halfway -> the next");

        // And the ends of the list hold.
        assert_eq!(snap_target(0.0, -9000.0, &snap, limit), 0.0);
        assert_eq!(snap_target(limit, 9000.0, &snap, limit), limit);
    }

    /// `positional_threshold` is Compose's `snapPositionalThreshold`: under the fling floor it is the
    /// ONLY thing deciding the target, so 0.3 lets a slow drag turn the page from 30% while the
    /// default 0.5 still sends it back. Above the floor the velocity decides and the knob is not
    /// consulted.
    #[test]
    fn the_positional_threshold_decides_a_slow_release() {
        const STEP: f32 = 200.0;
        let mut snap = SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() };
        let limit = STEP * 5.0;

        assert_eq!(snap_target(80.0, 0.0, &snap, limit), 0.0, "0.5 at 40% of the page: back");
        assert_eq!(snap_target(120.0, 0.0, &snap, limit), STEP, "0.5 at 60%: on to the next page");

        snap.positional_threshold = 0.3;
        assert_eq!(snap_target(80.0, 0.0, &snap, limit), STEP, "0.3 at 40%: already carried on");

        snap.positional_threshold = 0.9;
        assert_eq!(snap_target(120.0, 0.0, &snap, limit), 0.0, "0.9 at 60%: still not enough");
        snap.positional_threshold = 0.0;
        assert_eq!(
            snap_target(0.0, 0.0, &snap, limit),
            0.0,
            "a release resting on a boundary never turns a page, not even at threshold 0"
        );
        assert_eq!(
            snap_target(120.0, 9000.0, &snap, limit),
            STEP,
            "above the fling floor the velocity decides, whatever the threshold says"
        );
        assert_eq!(snap_target(120.0, -9000.0, &snap, limit), 0.0);
    }

    /// `max_pages_per_fling` is Compose's `PagerSnapDistance.atMost`: the default `1` keeps the
    /// one-page-per-gesture rule, and a bigger ceiling lets a hard flick carry as many pages as the
    /// decay estimate suggests (4000 px/s at 4.2 friction is ~952 px, ~2.8 pages at 336 px each).
    #[test]
    fn a_flick_carries_as_many_pages_as_the_ceiling_allows() {
        const STEP: f32 = 336.0;
        let mut snap = SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() };
        let limit = STEP * 10.0;

        assert_eq!(
            snap_target(0.0, 4000.0, &snap, limit),
            STEP,
            "the default ceiling is one page, however hard the flick"
        );
        snap.max_pages_per_fling = 3;
        assert_eq!(
            snap_target(0.0, 4000.0, &snap, limit),
            STEP * 3.0,
            "two and a bit pages of travel round to three"
        );
        assert_eq!(
            snap_target(0.0, 500.0, &snap, limit),
            STEP,
            "a gentle flick above the floor still moves exactly one page"
        );
        assert_eq!(
            snap_target(STEP * 9.0, 4000.0, &snap, limit),
            limit,
            "the end of the list still holds"
        );
        assert_eq!(
            snap_target(STEP * 2.0, -4000.0, &snap, limit),
            0.0,
            "and so does its start"
        );
    }

    /// A caller can settle on its own spring; the default is material3's `DefaultEffects`.
    #[test]
    fn the_settle_spring_is_the_one_the_spec_carries() {
        let custom = crate::animation::SpringSpec {
            stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_FAST,
            damping_ratio: crate::animation::SpringSpec::DAMPING_RATIO_SPATIAL,
            ..Default::default()
        };
        let snap = SnapInfo { settle: custom.clone(), ..Default::default() };
        match snap.settle_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec.stiffness, 3800.0);
                assert_eq!(spec.damping_ratio, 0.9);
            }
            other => panic!("a settle is a spring, got {other:?}"),
        }
    }

    /// The snap spec's spring is material3's `DefaultEffects`, not foundation's default. This looks
    /// like a constant to assert and is: 1600 is what `StandardMotionTokens.kt:23` says. (The numeric
    /// value of foundation's `StiffnessMediumLow` is not mirrored in `target/compose-src`, so the test
    /// pins what this repo uses, not the size of the gap.)
    #[test]
    fn the_snap_spring_is_material3s_default_effects() {
        // What every snapping container settles on unless a caller supplies its own spec.
        match SnapInfo::default().settle_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec.stiffness, 1600.0, "material3's DefaultEffects stiffness");
                assert_eq!(
                    spec.damping_ratio, 1.0,
                    "SpringDefaultEffectsDamping, and Compose's default for a stiffness-only spring"
                );
            }
            other => panic!("a snap settles on a spring, got {other:?}"),
        }
    }

    /// A snapping fling runs ONE animation that carries the gesture's velocity, so it never stops
    /// between a decay and a settle. `is_scroll_in_progress` is up for the whole of it and comes back
    /// down when the spring lands.
    #[test]
    fn a_snapping_fling_runs_one_motion_and_lands_on_the_boundary() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        const STEP: f32 = 336.0;
        let scroll = ScrollState::new();
        scroll.fling_limit.set(STEP * 10.0);
        scroll.snap.set(Some(SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() }));
        scroll.offset.set(100.0);

        scroll.fling(4000.0);
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            frames += 1;
            if !crate::animation::has_animation_for_state(scroll.offset.state_id()) || frames > 400 {
                break;
            }
        }

        assert_eq!(
            scroll.offset.get(),
            STEP,
            "a hard forward fling advances exactly one page"
        );
        assert!(
            !scroll.is_scroll_in_progress.get(),
            "and reports the scroll finished once it lands"
        );
    }

    /// `snap_page` is the wheel's page step: one call moves one snap position forward, a second call
    /// during the spring steps from where the first is HEADING (reading the live offset would re-target
    /// the same page), a reversed container steps the other way, and a step past the end is refused
    /// rather than clamped into the trailing padding.
    #[test]
    fn a_page_step_walks_one_position_at_a_time_and_refuses_the_end() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        const STEP: f32 = 200.0;
        let scroll = ScrollState::new();
        // Three 200 px pages (snap positions 0/200/400) in a 200 px viewport, plus 100 px of trailing
        // content padding: the limit is 500 while the LAST snap position is 400.
        scroll.fling_limit.set(500.0);
        scroll.snap.set(Some(SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() }));

        assert!(scroll.snap_page(true, false), "page 0 -> 1 starts a step");
        assert_eq!(
            *scroll.step_goal.lock().unwrap(),
            Some(STEP),
            "a second notch steps from where the first is heading, not from the live offset"
        );
        assert!(scroll.snap_page(true, false), "page 1 -> 2 starts the next step");
        assert_eq!(*scroll.step_goal.lock().unwrap(), Some(STEP * 2.0));
        assert!(
            !scroll.snap_page(true, false),
            "past the last snap position (2 * 200) the step is refused, not clamped into the 100 px of padding"
        );

        // Reverse layout mirrors the direction: a "backward" notch steps forward.
        crate::animation::clear_all_animations();
        let reversed = ScrollState::new();
        reversed.fling_limit.set(1000.0);
        reversed.snap.set(Some(SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() }));
        assert!(reversed.snap_page(false, true));
        assert_eq!(*reversed.step_goal.lock().unwrap(), Some(STEP));

        // A container without a snap spec never pages.
        let plain = ScrollState::new();
        assert!(!plain.snap_page(true, false));
        crate::animation::clear_all_animations();
    }

    /// A step that is CANCELLED leaves no goal behind: the next notch steps from where the list
    /// actually is. A drag, a fling or a programmatic jump cancels the spring without firing its
    /// completion callback (which is the only other thing that clears the goal), so the goal is
    /// invalidated by the absence of the animation too.
    #[test]
    fn a_cancelled_page_step_leaves_no_goal_behind() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        const STEP: f32 = 200.0;
        let scroll = ScrollState::new();
        scroll.fling_limit.set(1000.0);
        scroll.snap.set(Some(SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() }));

        assert!(scroll.snap_page(true, false), "page 0 -> 1 starts a step");
        assert_eq!(*scroll.step_goal.lock().unwrap(), Some(STEP));

        // The jump/drag path: cancel the spring directly, which never runs the step's `done`.
        crate::animation::cancel_animation(&scroll.offset);
        assert!(!crate::animation::has_animation_for_state(scroll.offset.state_id()));

        assert!(scroll.snap_page(true, false), "the next notch still steps");
        assert_eq!(
            *scroll.step_goal.lock().unwrap(),
            Some(STEP),
            "one page from the live offset, not the abandoned target (which would be 2 * STEP)"
        );
        crate::animation::clear_all_animations();
    }

    /// The release velocity gate must not apply to a snap. This is the case that was broken: a drag
    /// carried more than halfway and then let go with the finger nearly still — under 50 px/s, and
    /// under `fling_with_boundary`'s own 1 px/s floor — reached neither the decay nor the snap, so the
    /// list came to rest between two pages and nothing ever moved it again.
    ///
    /// A release at rest is reachable rather than theoretical: the framework estimates release velocity
    /// over a trailing window, so a drag that stops before the pointer lifts produces a near-zero
    /// velocity.
    ///
    /// This covers the floor INSIDE `fling_with_boundary`, which `ScrollState::fling` enters directly.
    /// The other floor — 50 px/s, at the call site in `app.rs` — is not reachable from here and is not
    /// claimed by any assertion below; it is guarded by
    /// `app::release_velocity_floor_tests::a_paged_list_flings_below_the_decay_floor`, with
    /// `an_ordinary_container_keeps_the_decay_floor` as its control, and driven end to end on the demo.
    #[test]
    fn a_paged_list_settles_even_when_it_is_released_at_rest() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        const STEP: f32 = 336.0;
        let snap = SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() };
        let settle = |offset: f32, velocity: f32| {
            let scroll = ScrollState::new();
            scroll.fling_limit.set(STEP * 10.0);
            scroll.snap.set(Some(snap.clone()));
            scroll.offset.set(offset);
            scroll.fling(velocity);
            let mut frames = 0;
            loop {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(20));
                frames += 1;
                if !crate::animation::has_animation_for_state(scroll.offset.state_id()) || frames > 400 {
                    break;
                }
            }
            scroll.offset.get()
        };

        assert_eq!(
            settle(200.0, 0.0),
            STEP,
            "a drag past halfway released at rest still advances to the next page"
        );
        assert_eq!(
            settle(100.0, 0.0),
            0.0,
            "and one short of halfway returns to the page it started on"
        );
        assert_eq!(
            settle(200.0, 12.0),
            STEP,
            "a gentle release still snaps rather than decaying across pages"
        );

        // And the floor still guards what it was written for: without a snap spec, a release below
        // 1 px/s must start no animation at all.
        crate::animation::clear_all_animations();
        let plain = ScrollState::new();
        plain.fling_limit.set(STEP * 10.0);
        plain.offset.set(200.0);
        plain.fling(0.5);
        crate::animation::update_animations();
        assert!(
            !crate::animation::has_animation_for_state(plain.offset.state_id()),
            "an unsnapped container still ignores a sub-1 px/s release instead of decaying from it"
        );
    }

    /// The 1 px/s floor belongs to the DECAY, so it is applied to the plan: a custom strategy on a
    /// container with no snap spec still sees a release that carries almost no momentum.
    #[test]
    fn a_slow_release_still_reaches_a_custom_strategy() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct LandOnFourHundred;
        impl crate::fling::FlingBehavior for LandOnFourHundred {
            fn plan(&self, _fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::spring(
                    400.0,
                    crate::animation::SpringSpec {
                        stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_DEFAULT,
                        ..Default::default()
                    },
                )
            }
        }

        let mut scroll = ScrollState::new();
        scroll.fling_limit.set(1000.0);

        // Control: with the built-in strategy and no snap spec, 0.5 px/s is the tail of a drag.
        scroll.fling(0.5);
        assert!(
            !crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "the decay floor still drops a release that carries no momentum"
        );
        assert_eq!(scroll.offset.get(), 0.0);

        scroll.fling_behavior = Some(std::sync::Arc::new(LandOnFourHundred));
        scroll.fling(0.5);
        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "the strategy decides what a slow release means, not the floor"
        );
        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !scroll.is_scroll_in_progress.peek() {
                break;
            }
        }
        assert!(
            (scroll.offset.get() - 400.0).abs() < 0.5,
            "and it ran the plan, got {}",
            scroll.offset.get()
        );
        crate::animation::clear_all_animations();
    }

    /// A plan that is not finite never reaches the offset: the engine's overrun guard would write it
    /// verbatim after five seconds, and a NaN scroll offset is something a container cannot recover from.
    #[test]
    fn a_non_finite_plan_is_refused() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct NanTarget;
        impl crate::fling::FlingBehavior for NanTarget {
            fn plan(&self, _fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::spring(f32::NAN, crate::animation::SpringSpec::default())
            }
        }
        #[derive(Debug)]
        struct NanSpring;
        impl crate::fling::FlingBehavior for NanSpring {
            fn plan(&self, _fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::spring(
                    100.0,
                    crate::animation::SpringSpec { stiffness: f32::NAN, ..Default::default() },
                )
            }
        }

        for behavior in [
            std::sync::Arc::new(NanTarget) as std::sync::Arc<dyn crate::fling::FlingBehavior>,
            std::sync::Arc::new(NanSpring),
        ] {
            let mut scroll = ScrollState::new();
            scroll.fling_limit.set(1000.0);
            scroll.fling_behavior = Some(behavior);
            scroll.fling(500.0);
            assert!(
                !crate::animation::has_animation_for_state(scroll.offset.state_id()),
                "a plan the engine cannot run must not start"
            );
            assert_eq!(scroll.offset.get(), 0.0, "and the offset stays where it was");
            assert!(!scroll.is_scroll_in_progress.peek(), "nor does it claim to be scrolling");
        }
    }

    /// Two releases landing on the SAME target: the second must not report a finish while the first
    /// spring is still moving. The engine's same-target dedup would call `done` immediately.
    #[test]
    fn a_second_fling_to_the_same_target_keeps_the_scroll_in_progress_flag() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let mut scroll = ScrollState::new();
        scroll.fling_limit.set(1000.0);
        scroll.snap.set(Some(SnapInfo { step: 200.0, ..Default::default() }));
        scroll.offset.set(50.0);
        scroll.fling(4000.0);
        assert!(scroll.is_scroll_in_progress.peek(), "the first release settled onto page 1");
        // The same flick again, before the first spring has moved: same target, same state.
        scroll.fling(4000.0);
        assert!(
            scroll.is_scroll_in_progress.peek(),
            "the running settle still owns the flag - the second release must not report a finish"
        );
        crate::animation::clear_all_animations();
    }

    /// A plan that asks to go past the container's limit is clamped, and the boundary is reported when
    /// the spring lands: a post-fling connection has to learn that the child hit its end, even though a
    /// spring has no leftover velocity the way a decay does.
    #[test]
    fn a_clamped_spring_plan_reports_the_boundary_when_it_lands() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct Overshoot;
        impl crate::fling::FlingBehavior for Overshoot {
            fn plan(&self, fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::spring(
                    fling.limit + 500.0,
                    crate::animation::SpringSpec {
                        stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_DEFAULT,
                        ..Default::default()
                    },
                )
            }
        }

        let mut scroll = ScrollState::new();
        scroll.fling_limit.set(300.0);
        scroll.fling_behavior = Some(std::sync::Arc::new(Overshoot));
        let reports = std::sync::Arc::new(AtomicUsize::new(0));
        let r = reports.clone();
        scroll.fling_with_boundary(800.0, move |remaining| {
            assert_eq!(remaining, 0.0, "a clamped spring has no velocity left to hand on");
            r.fetch_add(1, Ordering::Relaxed);
        });
        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !scroll.is_scroll_in_progress.peek() {
                break;
            }
        }
        assert_eq!(
            reports.load(Ordering::Relaxed),
            1,
            "a plan clamped by the limit reports the boundary once it lands"
        );
        assert!(
            (scroll.offset.get() - 300.0).abs() < 0.5,
            "and it stops at the limit, got {}",
            scroll.offset.get()
        );

        // The control: a settle that lands inside the content reports nothing (this is every pager).
        let mut inside = ScrollState::new();
        inside.fling_limit.set(300.0);
        inside.snap.set(Some(SnapInfo { step: 200.0, ..Default::default() }));
        inside.offset.set(50.0);
        let quiet = std::sync::Arc::new(AtomicUsize::new(0));
        let q = quiet.clone();
        inside.fling_with_boundary(12.0, move |_| {
            q.fetch_add(1, Ordering::Relaxed);
        });
        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !inside.is_scroll_in_progress.peek() {
                break;
            }
        }
        assert_eq!(
            quiet.load(Ordering::Relaxed),
            0,
            "a settle inside the content reports no boundary"
        );
        crate::animation::clear_all_animations();
    }

    /// A container with a custom strategy runs ITS plan, not the built-in one: the release lands
    /// wherever the strategy says, with the spring it named.
    #[test]
    fn a_custom_fling_behavior_decides_where_the_release_lands() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct LandOnLast;
        impl crate::fling::FlingBehavior for LandOnLast {
            fn plan(&self, fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                // A stiff spring, so the test measures the TARGET rather than how long a soft one
                // takes to crawl its last unit (the default is stiffness 200).
                crate::fling::FlingPlan::spring(
                    fling.limit,
                    crate::animation::SpringSpec {
                        stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_DEFAULT,
                        ..Default::default()
                    },
                )
            }
        }

        let mut scroll = ScrollState::new();
        scroll.fling_limit.set(600.0);
        scroll.fling_behavior = Some(std::sync::Arc::new(LandOnLast));
        scroll.fling(120.0);
        // Break on the flag the assertion reads, not on the animation table: a spring one threshold
        // away from its target is still running for a while, and the two conditions are not the same
        // instant.
        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !scroll.is_scroll_in_progress.peek() {
                break;
            }
        }
        assert!(
            !scroll.is_scroll_in_progress.peek(),
            "the strategy's motion must report finished (offset {})",
            scroll.offset.get()
        );
        assert!(
            (scroll.offset.get() - 600.0).abs() < 0.5,
            "the strategy chose the end of the list; the built-in would have decayed ~29 px, got {}",
            scroll.offset.get()
        );
        crate::animation::clear_all_animations();
    }

    /// A strategy may also decline: `FlingPlan::None` starts no animation and leaves the flag down, so
    /// the container does not sit there claiming to be scrolling.
    #[test]
    fn a_strategy_that_declines_starts_no_animation() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct Nothing;
        impl crate::fling::FlingBehavior for Nothing {
            fn plan(&self, _fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::None
            }
        }

        let mut scroll = ScrollState::new();
        scroll.fling_limit.set(600.0);
        scroll.fling_behavior = Some(std::sync::Arc::new(Nothing));
        scroll.fling(900.0);
        assert!(!crate::animation::has_animation_for_state(scroll.offset.state_id()));
        assert_eq!(scroll.offset.get(), 0.0, "nothing moved");
        assert!(!scroll.is_scroll_in_progress.peek(), "and nothing claims to be moving");
    }

    /// A strategy can hand the release back to momentum with its OWN decay: the plan's spec is what
    /// runs, not the built-in friction.
    #[test]
    fn a_strategy_can_choose_the_momentum_instead_of_a_snap() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct SlowMomentum;
        impl crate::fling::FlingBehavior for SlowMomentum {
            fn plan(&self, _fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::decay(crate::animation::DecaySpec::new(10.0, 0.1))
            }
        }

        let mut scroll = ScrollState::new();
        scroll.fling_limit.set(10_000.0);
        scroll.fling_behavior = Some(std::sync::Arc::new(SlowMomentum));
        scroll.fling(500.0);
        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !scroll.is_scroll_in_progress.peek() {
                break;
            }
        }
        // v0 / friction = 500 / 10 = 50 px, against ~119 px at the tree's own 4.2.
        assert!(
            (scroll.offset.get() - 50.0).abs() < 5.0,
            "the plan's decay carried it ~50 px, got {}",
            scroll.offset.get()
        );
        crate::animation::clear_all_animations();
    }

    /// Without a snap spec the fling is the plain decay it always was — the snap is opt-in, so every
    /// existing scroll container keeps its behaviour.
    #[test]
    fn a_fling_without_a_snap_keeps_stopping_where_the_decay_ends() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let scroll = ScrollState::new();
        scroll.fling_limit.set(4000.0);
        scroll.snap.set(None);
        scroll.offset.set(100.0);

        scroll.fling(5200.0);
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            frames += 1;
            if !crate::animation::has_animation_for_state(scroll.offset.state_id()) || frames > 400 {
                break;
            }
        }

        let end = scroll.offset.get();
        assert!(
            end > 1000.0,
            "an unsnapped fling keeps its decay distance, got {end}"
        );
        assert!(
            (end / 400.0 - (end / 400.0).round()).abs() >= 0.02,
            "and must NOT land on a boundary, got {end}"
        );
    }

    #[test]
    fn test_focus_requests_are_window_scoped() {
        let request_a = {
            let _window = focus_window(11);
            let requester = FocusRequester::new();
            requester.request_focus();
            requester
        };
        let request_b = {
            let _window = focus_window(22);
            let requester = FocusRequester::new();
            requester.request_focus();
            requester
        };

        let _window_a = focus_window(11);
        assert_eq!(take_focus_requests(), vec![request_a.id]);
        let _window_b = focus_window(22);
        assert_eq!(take_focus_requests(), vec![request_b.id]);
    }

    #[test]
    fn test_focus_requester_restores_window_context() {
        let requester = {
            let _window = focus_window(33);
            FocusRequester::new()
        };
        requester.request_focus();
        // Asserted about THIS request, not about the queue being empty: `FOCUS_REQUESTS` is a global
        // and another test's leftover is not this test's business. It used to assert emptiness and
        // passed only because of where it fell in the single-threaded order — renaming `ui` to
        // `components` moved it and it started failing with an unrelated request in the queue.
        assert!(
            !take_focus_requests().contains(&requester.id),
            "this request must not leak to the unbound window context"
        );
        let _window = focus_window(33);
        assert!(
            take_focus_requests().contains(&requester.id),
            "the bound window context must receive it"
        );
    }

    #[test]
    fn test_empty_modifier() {
        let m = Modifier::new();
        assert_eq!(m.elements().len(), 0);
    }

    #[test]
    fn test_chain_syntax() {
        let m = Modifier::new()
            .size(100.0, 50.0)
            .padding(8.0)
            .background(Color::RED, Shape::rounded(4.0))
            .clickable(|| println!("clicked"));

        assert_eq!(m.elements().len(), 4);
    }

    #[test]
    fn test_padding_sides_queries() {
        // 非对称 padding：四边独立
        let m = Modifier::new().padding_sides(2.0, 4.0, 6.0, 8.0);
        assert_eq!(m.get_padding_sides(), (2.0, 4.0, 6.0, 8.0));
        assert_eq!(m.get_padding_horizontal(), (2.0, 6.0));
        assert_eq!(m.get_padding_vertical(), (4.0, 8.0));
        assert_eq!(m.get_padding_values(), (8.0, 12.0));

        // 与等距/对称 padding 叠加（累积）
        let m = Modifier::new()
            .padding(1.0)
            .padding_horizontal(10.0)
            .padding_vertical(20.0)
            .padding_sides(2.0, 4.0, 6.0, 8.0);
        assert_eq!(m.get_padding_sides(), (13.0, 25.0, 17.0, 29.0));
    }

    #[test]
    fn test_padding_single_sides() {
        // 单边方法：只影响对应边
        let m = Modifier::new()
            .padding_start(3.0)
            .padding_end(5.0)
            .padding_top(7.0)
            .padding_bottom(9.0);
        assert_eq!(m.get_padding_sides(), (3.0, 7.0, 5.0, 9.0));
    }

    #[test]
    fn test_padding_dynamic_value() {
        // 动态 padding：State 驱动（动画作用于 padding 的机制）
        let s = crate::runtime::state::State::new(4.0f32);
        let m = Modifier::new().padding_start(s.clone());
        let (start, _, _, _) = m.get_padding_sides();
        assert_eq!(start, 4.0);
        s.set(20.0);
        let (start, _, _, _) = m.get_padding_sides();
        assert_eq!(start, 20.0, "动态 padding 重新求值");
    }

    #[test]
    fn test_offset_basic_and_single_axis() {
        // 全参 + 单轴
        let m = Modifier::new().offset(10.0, 20.0);
        assert_eq!(m.get_offset(), Some((10.0, 20.0)));
        let m = Modifier::new().offset_x(5.0);
        assert_eq!(m.get_offset(), Some((5.0, 0.0)));
        let m = Modifier::new().offset_y(7.0);
        assert_eq!(m.get_offset(), Some((0.0, 7.0)));
        // 无 offset → None
        assert_eq!(Modifier::new().get_offset(), None);
    }

    #[test]
    fn test_offset_axis_chained_merges_into_single_element() {
        // ⚠ 回归保护：offset_x/offset_y 连用必须**合并**为单元素——
        // get_offset 只取第一个 Offset，若各自 push 会静默丢 y（y=0 bug，
        // 曾致 BottomSheetScaffold 片贴顶）。
        let m = Modifier::new().offset_x(5.0).offset_y(7.0);
        assert_eq!(m.get_offset(), Some((5.0, 7.0)), "offset_x+offset_y 连用应得 (5,7)");
        // 与全参 offset 连用：后者合并进已有 Offset
        let m = Modifier::new().offset_y(7.0).offset(5.0, 8.0);
        assert_eq!(m.get_offset(), Some((5.0, 8.0)), "offset 应覆盖 y 分量");
        // 链上只应有一个 Offset 元素
        assert_eq!(
            m.elements.iter().filter(|e| matches!(e, ModifierElement::Offset { .. })).count(),
            1,
            "offset_x/offset_y/offset 连用后只应有一个 Offset 元素"
        );
    }

    #[test]
    fn test_offset_dynamic_animation() {
        // 动态 offset：State 驱动（动画作用于 offset——Compose offset 动画语义）
        let s = crate::runtime::state::State::new(0.0f32);
        let m = Modifier::new().offset(s.clone(), 10.0);
        assert_eq!(m.get_offset(), Some((0.0, 10.0)));
        s.set(50.0);
        assert_eq!(m.get_offset(), Some((50.0, 10.0)), "动态 offset 重新求值");
    }

    #[test]
    fn test_absolute_offset() {
        // absolute_offset：单独查询，与普通 offset 互不影响
        let m = Modifier::new().absolute_offset(3.0, 4.0);
        assert_eq!(m.get_offset(), None, "absolute 不进普通 offset 查询");
        assert_eq!(m.get_absolute_offset(), Some((3.0, 4.0)));
        let m = Modifier::new().absolute_offset_x(9.0);
        assert_eq!(m.get_absolute_offset(), Some((9.0, 0.0)));
        // 两者共存：各查各的
        let m = Modifier::new().offset(1.0, 2.0).absolute_offset(3.0, 4.0);
        assert_eq!(m.get_offset(), Some((1.0, 2.0)));
        assert_eq!(m.get_absolute_offset(), Some((3.0, 4.0)));
    }


    /// Padding and offset are LENGTHS: they accept a state (resolve per call, register a layout
    /// dependency when read during layout) and they can no longer be handed a layout MODE — a
    /// `Fill` has no meaning for an offset, and this is where the old `Dimension::Fill` silently
    /// resolved to 0.

    /// A border WIDTH is a length: a literal, a `Dp`, or a state resolved at paint time. The width is
    /// read while the border is drawn, so a dynamic one animates without re-measuring the node.
    #[test]
    fn a_border_width_can_be_a_length_or_a_state() {
        use crate::runtime::state::State;
        let color = Color::from_argb(255, 0, 0, 0);
        let width_of = |m: &Modifier| -> f32 {
            m.elements()
                .iter()
                .find_map(|el| match el {
                    ModifierElement::Border { width, .. } => Some(width.to_logical_px()),
                    _ => None,
                })
                .expect("the chain has a border")
        };

        let w = State::new(1.0f32);
        let m = Modifier::new().border(w.clone(), color, Shape::Rectangle);
        assert_eq!(width_of(&m), 1.0);
        w.set(3.0);
        assert_eq!(width_of(&m), 3.0, "a dynamic width resolves the current value");

        // Two dynamic widths are the same parameter: the value is read at paint time, and the
        // dependency belongs to the state, not to the element.
        let a = Modifier::new().border(State::new(1.0f32), color, Shape::Rectangle);
        let b = Modifier::new().border(State::new(2.0f32), color, Shape::Rectangle);
        assert!(a.param_eq(&b), "a dynamic width is a parameter, not a value");
    }

    #[test]
    fn padding_and_offset_take_lengths_and_track_their_state() {
        use crate::runtime::state::State;
        let pad = State::new(4.0f32);
        let off = State::new(1.0f32);
        let m = Modifier::new().padding(pad.clone()).offset(off.clone(), 0.0);

        assert_eq!(m.get_padding_sides(), (4.0, 4.0, 4.0, 4.0));
        assert_eq!(m.get_offset(), Some((1.0, 0.0)));

        pad.set(9.0);
        off.set(-3.0);
        assert_eq!(
            m.get_padding_sides(),
            (9.0, 9.0, 9.0, 9.0),
            "a padding resolves the value it was given, not a copy taken at build time"
        );
        assert_eq!(m.get_offset(), Some((-3.0, 0.0)));
    }

    #[test]
    fn test_dimension_conversions() {
        use crate::layout::SizeValue;
        let m = Modifier::new()
            .size(100.0, 20.0)      // f32 -> Length(Fixed)
            .width(SizeValue::Auto);

        let elements = m.elements();
        assert_eq!(elements.len(), 2);
        match &elements[0] {
            ModifierElement::Size { width, height } => {
                assert!(width.same_parameter(&SizeValue::from(100.0f32)));
                assert!(height.same_parameter(&SizeValue::from(20.0f32)));
            }
            _ => panic!("expected Size"),
        }
        // `width()` leaves the other axis on `Auto`; there is no `Fill` size mode any more (a fill is
        // `fill_max_width`, which the measure implements).
        match &elements[1] {
            ModifierElement::Size { width, .. } => {
                assert!(matches!(width, SizeValue::Auto), "width(Auto) is the Auto mode");
            }
            _ => panic!("expected a second Size element"),
        }
    }

    #[test]
    fn test_fill_max() {
        let m = Modifier::new()
            .fill_max_width()
            .fill_max_height()
            .fill_max_size();

        assert_eq!(m.elements().len(), 3);
    }

    #[test]
    fn test_modifier_immutable() {
        let a = Modifier::new().size(100.0, 50.0);
        let b = a.clone().padding(8.0);

        // a 不变
        assert_eq!(a.elements().len(), 1);
        // b 追加了
        assert_eq!(b.elements().len(), 2);
    }

    #[test]
    fn test_border_and_focusable() {
        let m = Modifier::new()
            .border(2.0, Color::BLUE, Shape::rounded(8.0))
            .focusable();

        assert_eq!(m.elements().len(), 2);
    }
