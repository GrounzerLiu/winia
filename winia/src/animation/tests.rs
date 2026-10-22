    // Test serial lock: the engine uses the global ACTIVE_ANIMATIONS, so parallel tests interfere (push/update races).
    pub(crate) static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The suite's shared guard: serialises the global registries AND installs a frozen clock, so a
    /// test advances time explicitly instead of sleeping. Bind the tuple for the whole test — dropping
    /// it restores real time and releases the lock (in that order, on panic too).
    pub(crate) fn serial_clock() -> (std::sync::MutexGuard<'static, ()>, super::TestClock) {
        let guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        (guard, super::TestClock::install())
    }

    use super::*;
    use std::sync::Arc;
    use crate::runtime::composer::Composer;
    use std::time::Duration;

    /// Simulates 16.7 ms frames, stepping n frames to advance the spring.
    fn step_spring(
        stiffness: f32, damping: f32, threshold: f32,
        start: f32, target: f32,
        frames: usize,
    ) -> (f32, bool) {
        let mut disp = start - target;
        let mut vel = 0.0f32;
        for _ in 0..frames {
            disp = compute_spring_displacement(
                stiffness, damping, 1.0, disp, &mut vel,
                Duration::from_millis(17), threshold,
            );
        }
        let val = target + disp;
        // The production test, not a copy of it: a settle assertion has to measure the same criterion
        // the engine does, or it pins a duration nobody runs.
        let done = spring_at_rest(disp, vel, threshold);
        (val, done)
    }

    /// The rest test's second half is what keeps a bouncy spring's overshoot: passing through the
    /// threshold band while still moving fast is NOT rest. Pinned because "the velocity and the
    /// displacement are compared in different units" looks like a bug and has already been
    /// "fixed" once — that attempt cut the bounce and turned
    /// `components::shared_transition::tier0_tests::tier0_bouncy_spring_overshoot_renders_then_settles` red
    /// (see `spring_at_rest`).
    #[test]
    fn a_spring_crossing_its_target_is_not_at_rest() {
        // `SpringSpec::bouncy()`'s threshold on a 0..1 progress.
        let threshold = 0.1f32;
        assert!(
            spring_at_rest(0.05, 0.02, threshold),
            "under the threshold and nearly still: that is rest, and a bouncy spring has already settled"
        );
        assert!(
            !spring_at_rest(0.05, 3.0, threshold),
            "inside the band but moving 3 units/s: the bounce is still on its way out"
        );
        assert!(
            !spring_at_rest(1.0, 0.0, threshold),
            "a whole unit from the target is not rest however still it is"
        );
    }

    /// A spring ends on its own criterion, not on the 5 s extreme-parameter protection: from 0 to 123
    /// with `components::tab_row`'s indicator spring (damping 0.6 / stiffness 700 / threshold 0.01) the physics
    /// gives `ln(123 / 0.01) / (0.6 * sqrt(700)) ≈ 0.59 s`, and the value lands EXACTLY on the target.
    #[test]
    fn a_spring_finishes_without_waiting_for_the_extreme_parameter_protection() {
        let (_guard, _clock) = super::tests::serial_clock();
        clear_all_animations();
        let state = State::new(0.0f32);
        let spec = AnimationSpec::Spring(SpringSpec {
            damping_ratio: 0.6,
            stiffness: 700.0,
            mass: 1.0,
            threshold: 0.01,
        });
        push_animatable(state.clone(), 123.0, spec);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while is_animating() && std::time::Instant::now() < deadline {
            update_animations();
            _clock.advance(Duration::from_millis(8));
        }
        let finished = !is_animating();
        clear_all_animations();
        assert!(
            finished,
            "the spring must settle on its own criterion well inside 3 s (the extreme-parameter \
             protection only fires at 5 s); value={}",
            state.peek()
        );
        assert_eq!(state.peek(), 123.0, "a finished spring lands exactly on its target");
    }

    #[test]
    fn spring_converges_to_target() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Critically damped, 50 -> 300.
        let (val, done) = step_spring(1500.0, 1.0, 0.1, 50.0, 300.0, 300);
        assert!(done, "spring should settle within 300 frames");
        assert!((val - 300.0).abs() < 1.0, "val={} should be near 300", val);
    }

    #[test]
    fn spring_bouncy_overshoots_then_converges() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Underdamped bouncy spring: it should overshoot and then converge.
        let (mut val, mut done) = step_spring(1500.0, 0.6, 0.1, 50.0, 300.0, 5);
        // Early on it is well below the target (not there yet) or already overshooting.
        let (final_val, final_done) = step_spring(1500.0, 0.6, 0.1, 50.0, 300.0, 300);
        assert!(final_done, "bouncy should settle");
        assert!((final_val - 300.0).abs() < 1.0, "final_val={}", final_val);
        // Record whether any intermediate frame overshot (val > 300 was seen).
        let mut overshot = false;
        let mut disp = -250.0f32;
        let mut vel = 0.0f32;
        for _ in 0..60 {
            disp = compute_spring_displacement(1500.0, 0.6, 0.1, disp, &mut vel, Duration::from_millis(17), 0.1);
            if 300.0 + disp > 300.0 { overshot = true; }
        }
        assert!(overshot, "underdamped spring should overshoot");
        let _ = (val, done);
    }

    #[test]
    fn spring_reverse_animation() {
        let (_guard, _clock) = super::tests::serial_clock();
        // The reverse 300 -> 50 direction must converge too (an earlier bug pinned the denominator to max(EPSILON)).
        let (val, done) = step_spring(1500.0, 1.0, 0.1, 300.0, 50.0, 300);
        assert!(done);
        assert!((val - 50.0).abs() < 1.0, "val={} should be near 50", val);
    }

    #[test]
    fn spring_dt_zero_is_safe() {
        let (_guard, _clock) = super::tests::serial_clock();
        // dt=0 must not panic or produce NaN.
        let mut disp = -250.0f32;
        let mut vel = 0.0f32;
        let d = compute_spring_displacement(1500.0, 1.0, 0.1, disp, &mut vel, Duration::ZERO, 0.1);
        assert!(d.is_finite());
        let _ = disp;
    }

    #[test]
    fn tween_completes_within_duration() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut anim = Motion::<f32>::new(State::new(0.0));
        anim.animate_to(100.0, AnimationSpec::Tween(TweenSpec::default()));
        // Simulate 400 ms (10 ms per frame): it should finish past the 300 ms duration.
        let mut frames = 0;
        while anim.update() && frames < 60 {
            _clock.advance(Duration::from_millis(10));
            frames += 1;
        }
        assert!(frames < 60, "tween should finish within 600ms, took {} frames", frames);
        assert_eq!(anim.state.get(), 100.0);
    }

    /// `TweenSpec::delay` (Compose's `tween(delayMillis)`): the value holds `from` until the delay
    /// has passed, then the tween runs its full duration from there.
    #[test]
    fn tween_holds_from_until_its_delay_elapses() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut anim = Motion::<f32>::new(State::new(0.0));
        anim.animate_to(
            100.0,
            AnimationSpec::Tween(
                TweenSpec::new(Duration::from_millis(60), interpolator::Linear::new())
                    .delay(Duration::from_millis(120)),
            ),
        );
        // Well inside the delay: still exactly `from`, and still running.
        let mut held_at = 0.0f32;
        for _ in 0..6 {
            assert!(anim.update(), "a delayed tween is still running");
            _clock.advance(Duration::from_millis(10));
            held_at = anim.state.get();
        }
        assert_eq!(held_at, 0.0, "the value has not moved while the delay runs");

        // …and once the delay is past, it completes the 60 ms tween (180 ms of delay + tween).
        let mut frames = 6;
        while anim.update() && frames < 60 {
            _clock.advance(Duration::from_millis(10));
            frames += 1;
        }
        assert!(frames < 60, "the delayed tween finishes (took {frames} frames)");
        assert_eq!(anim.state.get(), 100.0);
    }

    #[test]
    fn infinite_float_restart_loops() {
        let (_guard, _clock) = super::tests::serial_clock();
        let state = State::new(0.0);
        let mut inf = Infinite {
            state: state.clone().into_visual(),
            from: 0.0, to: 10.0,
            spec: InfiniteRepeatableSpec::restart(Duration::from_millis(50)),
            start: now(),
        };
        // 6 cycles of 50 ms. Sampled at 1 ms, not 10 ms: the triangle hits its peak only inside a
        // ~2.5 ms window per half-cycle, so 10 ms sampling misses it depending on machine load and the
        // assertion below flaked once in a full run (the value seen depends on when the scheduler runs
        // this thread). Same wall time, deterministic phase coverage.
        let mut max_seen = 0.0f32;
        let mut min_seen = 10.0f32;
        for _ in 0..300 {
            inf.update();
            _clock.advance(Duration::from_millis(1));
            let v = state.get();
            max_seen = max_seen.max(v);
            min_seen = min_seen.min(v);
        }
        assert!(min_seen <= 0.5, "restart should return near from, min={}", min_seen);
        assert!(max_seen >= 9.5, "restart should reach near to, max={}", max_seen);
        // An infinite animation never completes.
        assert!(inf.update(), "infinite should never complete");
    }

    #[test]
    fn infinite_float_reverse_oscillates() {
        let (_guard, _clock) = super::tests::serial_clock();
        let state = State::new(0.4);
        let mut inf = Infinite {
            state: state.clone().into_visual(),
            from: 0.4, to: 1.0,
            spec: InfiniteRepeatableSpec::reverse(Duration::from_millis(50)),
            start: now(),
        };
        let mut saw_high = false;
        let mut saw_low = false;
        for _ in 0..30 {
            inf.update();
            _clock.advance(Duration::from_millis(10));
            let v = state.get();
            if v > 0.95 { saw_high = true; }
            if v < 0.45 { saw_low = true; }
        }
        assert!(saw_high, "reverse should reach near to=1.0");
        assert!(saw_low, "reverse should return near from=0.4");
    }

    /// M3's motion-scheme effect springs, pinned to the token values they mirror
    /// (`tokens/StandardMotionTokens.kt:22-27`): both critically damped, 1600 for an arriving state
    /// and 3800 for a leaving one.
    #[test]
    fn the_effects_springs_are_m3s_tokens() {
        let arriving = SpringSpec::effects_default();
        assert_eq!(arriving.damping_ratio, 1.0);
        assert_eq!(arriving.stiffness, 1600.0, "SpringDefaultEffectsStiffness");
        let leaving = SpringSpec::effects_fast();
        assert_eq!(leaving.damping_ratio, 1.0);
        assert_eq!(leaving.stiffness, 3800.0, "SpringFastEffectsStiffness");
        assert!(leaving.stiffness > arriving.stiffness, "leaving is quicker than arriving");
        assert_eq!(arriving.threshold, 0.01, "the default visibility threshold");
    }

    /// A `Decay` on a colour used to write an opaque grey through the scalar fallback (the fallback's
    /// `from_f32` is a grey at the old Oklab lightness) on the first frame, because the push path
    /// supplies a zero initial velocity and the decay then reports itself finished at once. It is now
    /// replaced by a tween, which mixes the two colours through `lerp` and lands on the target.
    #[test]
    fn a_decay_on_a_colour_fades_instead_of_painting_a_grey() {
        let (_guard, _clock) = super::tests::serial_clock();
        super::tests::clear_all_animations();
        let from = crate::graphics::Color::from_argb(255, 33, 150, 243);
        let to = crate::graphics::Color::from_argb(255, 244, 67, 54);
        let state = State::new(from);
        push_animatable_color(state.clone(), to, AnimationSpec::Decay(DecaySpec::default()));
        assert!(
            super::active_animations_report().iter().any(|l| l.contains("tween(")),
            "a target-based decay is a tween, got {:?}",
            super::active_animations_report()
        );
        let grey = crate::graphics::Color::from_oklab([1.0, from.to_oklab()[1], 0.0, 0.0]);
        _clock.advance(Duration::from_millis(120));
        super::update_animations();
        let mid = state.peek();
        assert_ne!(mid, grey, "the fade must never paint the placeholder grey");
        for _ in 0..400 {
            super::update_animations();
            _clock.advance(Duration::from_millis(2));
            if !super::has_animation_for_state(state.state_id()) {
                break;
            }
        }
        assert_eq!(state.peek(), to, "and it lands exactly on the target");
    }

    /// The same rule for the other placeholder types: a `Size` decay used to travel along the area
    /// norm through `from_f32` (a square at the origin). It now interpolates each axis and lands on
    /// the target.
    #[test]
    fn a_decay_on_a_size_interpolates_both_axes() {
        let (_guard, _clock) = super::tests::serial_clock();
        super::tests::clear_all_animations();
        let from = crate::unit::Size::new(10.0, 10.0);
        let target = crate::unit::Size::new(80.0, 20.0);
        let state = State::new(from);
        push_animatable(state.clone(), target, AnimationSpec::Decay(DecaySpec::default()));
        assert!(
            super::active_animations_report().iter().any(|l| l.contains("tween(")),
            "a target-based decay is a tween, got {:?}",
            super::active_animations_report()
        );
        // Every frame must stay inside the from..to box: the bug this covers wrote the PLACEHOLDER
        // value (a square at the origin, outside the box) on the first frame. Sampling the box rather
        // than one fixed moment keeps the test meaningful on a machine that stalls past the tween's
        // 300 ms — a late frame is the target, which is still inside the box.
        for _ in 0..400 {
            let v = state.peek();
            assert!(
                v.width >= from.width && v.width <= target.width
                    && v.height >= from.height && v.height <= target.height,
                "a frame left the from..to box: {v:?} (a placeholder writes the origin)"
            );
            super::update_animations();
            _clock.advance(Duration::from_millis(2));
            if !super::has_animation_for_state(state.state_id()) {
                break;
            }
        }
        assert_eq!(state.peek(), target, "and it lands on the target");
    }

    /// M3's three spatial tokens, pinned: one shared damping (0.9 — the slight overshoot is the point)
    /// and the three stiffnesses the components name.
    #[test]
    fn the_spatial_springs_are_m3s_tokens() {
        for (spec, stiffness, name) in [
            (SpringSpec::spatial_slow(), 300.0, "SpringSlowSpatialStiffness"),
            (SpringSpec::spatial_default(), 700.0, "SpringDefaultSpatialStiffness"),
            (SpringSpec::spatial_fast(), 1400.0, "SpringFastSpatialStiffness"),
        ] {
            assert_eq!(spec.damping_ratio, 0.9, "{name}: SpringDefaultSpatialDamping");
            assert_eq!(spec.stiffness, stiffness, "{name}");
            assert_eq!(spec.mass, 1.0);
        }
        assert!(SpringSpec::spatial_fast().stiffness > SpringSpec::spatial_default().stiffness);
        assert_eq!(
            SpringSpec::spatial_default().damping_ratio,
            SpringSpec::DAMPING_RATIO_SPATIAL
        );
        assert_ne!(
            SpringSpec::spatial_default().damping_ratio,
            SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            "the spatial springs overshoot; the effect springs do not"
        );
    }

    /// Compose interpolates a colour in Oklab — `ColorVectorConverter.convertToVector` hands the
    /// animation `AnimationVector4D(alpha, L, a, b)` (`animation/ColorVectorConverter.kt:33-47`), so a
    /// Tween's fraction is applied to those four components rather than to the sRGB bytes or to a
    /// CAM16 blend.
    #[test]
    fn color_lerp_interpolates_in_oklab() {
        let (_guard, _clock) = super::tests::serial_clock();
        use crate::graphics::Color;
        let from = Color::from_argb(128, 33, 150, 243);
        let to = Color::from_argb(255, 255, 82, 82);
        let mid = from.lerp(&to, 0.5);

        let (a, b) = (from.to_oklab(), to.to_oklab());
        let mut half = [0.0f32; 4];
        for i in 0..4 {
            half[i] = a[i] + (b[i] - a[i]) * 0.5;
        }
        assert_eq!(mid, Color::from_oklab(half), "the midpoint is the component-wise Oklab midpoint");
        assert!((mid.a as i32 - 191).abs() <= 1, "alpha={} should be ~191", mid.a);
        assert!(mid.r > 100 || mid.b > 100, "mid={:?} should not be grayish", mid);
        assert_eq!(from.lerp(&to, 0.0), from);
        assert_eq!(from.lerp(&to, 1.0), to);

        // The perceptual midpoint, which the encoded-byte blend does not produce.
        assert_eq!(
            Color::BLACK.lerp(&Color::WHITE, 0.5),
            Color::from_argb(255, 99, 99, 99),
            "black -> white at 50% is Oklab L 0.5 (#636363), not #7F7F7F"
        );
    }

    /// A colour spring is a spring: one per Oklab component, so the lightness follows the same curve a
    /// scalar `f32` spring does under the same spec — and not the 300 ms linear tween the engine used
    /// to substitute for it (the property this test was written to pin).
    ///
    /// The endpoints are mid-tones on purpose: winia stores a colour as eight-bit sRGB, so the visible
    /// value moves in byte steps (near black one step is 0.067 of Oklab L; at these greys it is 0.004),
    /// which is the recorded deviation from Compose's float components.
    #[test]
    fn a_colour_spring_runs_on_every_component() {
        use std::time::Duration;
        let (_guard, _clock) = super::tests::serial_clock();
        super::tests::clear_all_animations();

        let from = crate::graphics::Color::from_argb(255, 0x40, 0x40, 0x40);
        let to = crate::graphics::Color::from_argb(255, 0xC8, 0xC8, 0xC8);
        let (from_l, to_l) = (from.to_oklab()[1], to.to_oklab()[1]);
        let color = State::new(from);
        let scalar: State<f32> = State::new(from_l);
        let tweened = State::new(from);
        let spec = AnimationSpec::Spring(SpringSpec::default());
        push_animatable_color(color.clone(), to, spec.clone());
        push_animatable(scalar.clone(), to_l, spec);
        push_animatable_color(
            tweened.clone(),
            to,
            AnimationSpec::Tween(TweenSpec::default()),
        );

        let mut frames = 0;
        let mut max_spring_gap = 0.0f32;
        let mut max_tween_gap = 0.0f32;
        for _ in 0..2_000 {
            if !update_animations() {
                break;
            }
            _clock.advance(Duration::from_millis(8));
            let lightness = color.peek().to_oklab()[1];
            // The first two frames are skipped: the three states are separate pushes, so one can start a frame
            // later than another, and under load that phase offset showed up as a gap three times the budget
            // while the spring was still moving fastest. This guard is about the CURVE the colour follows, so it
            // starts once all three are running.
            if frames >= 2 {
                max_spring_gap = max_spring_gap.max((lightness - scalar.peek()).abs());
                max_tween_gap = max_tween_gap.max((lightness - tweened.peek().to_oklab()[1]).abs());
            }
            frames += 1;
        }
        let total = color.peek();
        // A floor, not a frame budget: under load the loop legitimately finishes in fewer, longer
        // steps, so demanding 20 frames was load-dependent.
        assert!(frames > 3, "the loop sampled the motion (frames={frames})");
        assert!(
            max_spring_gap <= 0.006,
            "the colour's lightness follows the scalar spring under the same spec, one byte step \
             apart at most (largest gap={max_spring_gap})"
        );
        assert!(
            max_tween_gap > 0.03,
            "…and is NOT the linear tween the engine used to substitute for a colour spring \
             (largest gap={max_tween_gap})"
        );
        assert_eq!(total, to, "the spring lands exactly on target");
        assert!(tweened.peek() == to, "the tween does too, and in its own time");
    }

    #[test]
    fn infinite_color_reverse_cycles() {
        let (_guard, _clock) = super::tests::serial_clock();
        use crate::graphics::Color;
        let state = State::new(Color::RED);
        let mut inf = Infinite {
            state: state.clone().into_visual(),
            from: Color::RED, to: Color::BLUE,
            spec: InfiniteRepeatableSpec::reverse(Duration::from_millis(50)),
            start: now(),
        };
        let mut saw_blue = false;
        let mut saw_red = false;
        // Deterministic phase stepping (instead of the old sleep sampling): a Reverse cycle is
        // 2 x duration (100 ms); the first half (phase < 1) goes red -> blue and the second half
        // blue -> red. Move inf.start to each phase and update, sampling the endpoints directly.
        let set_phase = |inf: &mut Infinite<Color>, phase_ms: u64| {
            inf.start = now() - std::time::Duration::from_millis(phase_ms);
        };
        // Blue end: phase ~ 25 ms (t -> 1); red end: phase ~ 0/50/75 ms (t -> 0).
        for phase_ms in [0u64, 24, 25, 26, 49, 50, 51, 74, 75, 76] {
            set_phase(&mut inf, phase_ms);
            inf.update();
            let c = state.get();
            if c.b > 200 && c.r < 50 { saw_blue = true; }
            if c.r > 200 && c.b < 50 { saw_red = true; }
        }
        assert!(saw_blue, "should reach blue (phase 25ms: t→1)");
        assert!(saw_red, "should return to red (phase 0/50/75ms: t→0)");
    }

    #[test]
    fn composer_drop_cleans_owned_animation_without_touching_other_composer() {
        let (_guard, _clock) = super::tests::serial_clock();
        clear_all_animations();
        let owned = State::new(0.0f32);
        let foreign = State::new(0.0f32);
        push_animatable(owned.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));
        push_animatable(foreign.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));

        let composer = {
            let mut composer = Composer::new();
            composer.animation_state_ids.insert(owned.state_id());
            composer
        };
        drop(composer);
        assert!(!has_animation_for_state(owned.state_id()));
        assert!(has_animation_for_state(foreign.state_id()));
        remove_animation_by_state(foreign.state_id());
    }

    #[test]
    fn remove_animation_by_state_cleans_lists() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Push three animations: f32, Color and infinite Color.
        let s1 = State::new(0.0f32);
        let s2 = State::new(crate::graphics::Color::RED);
        let s3 = State::new(crate::graphics::Color::BLUE);
        push_animatable(s1.clone(), 10.0, AnimationSpec::Tween(TweenSpec::default()));
        push_animatable_color(s2.clone(), crate::graphics::Color::GREEN, AnimationSpec::Tween(TweenSpec::default()));
        push_infinite(s3.clone(), crate::graphics::Color::BLUE, crate::graphics::Color::RED,
            InfiniteRepeatableSpec::restart(Duration::from_millis(50)));
        assert!(is_animating(), "animations should be registered");

        // Remove the animations for s1 and s3.
        remove_animation_by_state(s1.state_id());
        remove_animation_by_state(s3.state_id());
        // s2 is still there.
        assert!(has_animation_for_state(s2.state_id()), "s2 color animation should remain");
        remove_animation_by_state(s2.state_id());
        assert!(!has_animation_for_state(s2.state_id()), "s2 should be removed");
    }

    #[test]
    fn keyframes_interpolate_segments() {
        let (_guard, _clock) = super::tests::serial_clock();
        use crate::unit::DpExt;
        // 0% -> 0, 50% -> 50, 100% -> 100, linear.
        let spec = KeyframesSpec::new(Duration::from_millis(100), vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.0), 0.0);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.25), 0.25);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.5), 0.5);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.75), 0.75);
        assert_eq!(interpolate_keyframes(&spec.frames, 1.0), 1.0);
        assert_eq!(interpolate_keyframes(&spec.frames, 2.0), 1.0); // out of range clamps
    }

    #[test]
    fn repeatable_runs_iterations() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut anim = Motion::<f32>::new(State::new(0.0));
        anim.animate_to(100.0, AnimationSpec::Repeatable(
            RepeatableSpec::new(3, RepeatMode::Restart,
                TweenSpec { duration: Duration::from_millis(40), delay: Duration::ZERO, interpolator: std::sync::Arc::new(interpolator::Linear::new()) })));
        let mut frames = 0;
        while anim.update() && frames < 100 {
            _clock.advance(Duration::from_millis(10));
            frames += 1;
        }
        // 3 x 40 ms = 120 ms: it should finish.
        assert!(frames < 100, "repeatable should finish within 100 frames");
        assert_eq!(anim.state.get(), 100.0);
    }

    #[test]
    fn repeatable_reverse_even_ends_at_from() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Blocking bug: Reverse with an even count ends the last cycle at from, not at to.
        let mut anim = Motion::<f32>::new(State::new(0.0));
        anim.animate_to(100.0, AnimationSpec::Repeatable(
            RepeatableSpec::new(2, RepeatMode::Reverse,
                TweenSpec { duration: Duration::from_millis(40), delay: Duration::ZERO, interpolator: std::sync::Arc::new(interpolator::Linear::new()) })));
        while anim.update() {
            _clock.advance(Duration::from_millis(10));
        }
        // Two reverse iterations: 0 -> 100, then 100 -> 0, ending at 0 (from).
        let final_val = anim.state.peek();
        assert!((final_val - 0.0).abs() < 1.0, "should end at from=0, got {}", final_val);
    }

    #[test]
    fn keyframes_first_frame_offset() {
        let (_guard, _clock) = super::tests::serial_clock();
        // With a first frame at progress > 0, t below it takes the first frame's value.
        let spec = KeyframesSpec::new(Duration::from_millis(100), vec![(0.5, 0.7), (1.0, 1.0)]);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.1), 0.7);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.5), 0.7);
        assert_eq!(interpolate_keyframes(&spec.frames, 0.75), 0.85);
        assert_eq!(interpolate_keyframes(&spec.frames, 1.0), 1.0);
    }

    #[test]
    fn snap_jumps_immediately() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut anim = Motion::<f32>::new(State::new(0.0));
        anim.animate_to(42.0, AnimationSpec::Snap(SnapSpec::default()));
        assert!(!anim.update(), "snap completes in one update");
        assert_eq!(anim.state.get(), 42.0);
    }

    #[test]
    fn offset_dedup_is_exact_not_norm() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Blocking bug: two Offsets with the same norm must not be mistaken for the same target.
        use crate::unit::Offset;
        let s = State::new(Offset::new(0.0, 0.0));
        push_animatable(s.clone(), Offset::new(10.0, 0.0), AnimationSpec::Tween(TweenSpec::default()));
        // Remove the first animation (so it cannot intercept), then register the same-norm target.
        remove_animation_by_state(s.state_id());
        push_animatable(s.clone(), Offset::new(0.0, 10.0), AnimationSpec::Tween(TweenSpec::default()));
        assert!(is_animating(), "new offset animation should be registered (norm collision must not block)");
        // The target is exactly (0,10), not (10,0).
        let list = ACTIVE_ANIMATIONS.lock().unwrap();
        let target_ok = list.iter().any(|a| a.same_target(&Offset::new(0.0, 10.0)));
        assert!(target_ok, "animation target should be (0,10)");
        let wrong_ok = list.iter().any(|a| a.same_target(&Offset::new(10.0, 0.0)));
        assert!(!wrong_ok, "animation should NOT match (10,0)");
    }

    #[test]
    fn offset_spring_no_longer_downgraded_to_tween() {
        let (_guard, _clock) = super::tests::serial_clock();
        // A former blocking bug: an Offset driven by Spring converged along the norm to (norm,norm)
        // instead of the target, so it was forced down to Tween. Offset now declares
        // `write_components` (one spring per axis) and each lands on its own target, so the
        // downgrade is gone; this test pins that it stays gone.
        use crate::unit::Offset;
        let s = State::new(Offset::new(0.0, 0.0));
        push_animatable(s.clone(), Offset::new(3.0, 4.0), AnimationSpec::Spring(SpringSpec::default()));
        assert!(has_animation_for_state(s.state_id()), "Offset animation should be registered");
        // Step with real frame intervals (update() uses the real clock; back-to-back calls never advance).
        for _ in 0..400 {
            if !update_animations() {
                break;
            }
            _clock.advance(std::time::Duration::from_millis(8));
        }
        assert_eq!(
            s.peek(),
            Offset::new(3.0, 4.0),
            "each component spring writes its own exact target (Tween or a norm-following run cannot reach (3,4))"
        );
        remove_animation_by_state(s.state_id());
        assert!(!has_animation_for_state(s.state_id()), "own animation should be removed");
    }

    #[test]
    fn size_spring_lands_each_axis_on_its_own_target() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Compose's `IntSize.VectorConverter`: one spring each for width and height, and the
        // animation ends only when both arrive (`AnimationModifier.kt:70-74`). The two axes travel
        // very different distances (width +10, height -1), so a single scalar displacement would
        // drag both along the norm and neither would land on its own target.
        use crate::unit::Size;
        let s = State::new(Size::new(50.0, 10.0));
        push_animatable(
            s.clone(),
            Size::new(60.0, 9.0),
            AnimationSpec::Spring(SpringSpec::default()),
        );
        let mut frames = 0;
        for _ in 0..400 {
            if !update_animations() {
                break;
            }
            _clock.advance(std::time::Duration::from_millis(8));
            frames += 1;
        }
        assert!(frames > 1, "two component springs take several frames, not one Snap (frames={frames})");
        assert_eq!(s.peek(), Size::new(60.0, 9.0), "width and height each write their own exact target");
    }

    #[test]
    fn a_retarget_carries_each_axis_its_own_velocity() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Compose's `Animatable.animateTo` hands the running velocity to the replacement animation
        // PER COMPONENT (`IntSize`'s converter is two-dimensional): interrupting while the width is
        // travelling fast must not give the height a velocity it never had. One scalar velocity
        // (the fastest axis) drags the height along by tens of pixels.
        use crate::unit::Size;
        let s = State::new(Size::new(0.0, 100.0));
        let spec = AnimationSpec::Spring(SpringSpec::default());
        push_animatable(s.clone(), Size::new(100.0, 100.0), spec.clone());
        for _ in 0..4 {
            _clock.advance(std::time::Duration::from_millis(8));
            update_animations();
            assert_eq!(s.peek().height, 100.0, "the height's target never moved, so it must not move");
        }
        // The width is mid-flight (and fast); retarget it. Only the width carries a velocity on.
        push_animatable(s.clone(), Size::new(20.0, 100.0), spec);
        let mut frames = 0;
        for _ in 0..400 {
            if !update_animations() {
                break;
            }
            _clock.advance(std::time::Duration::from_millis(8));
            frames += 1;
            assert_eq!(
                s.peek().height, 100.0,
                "the height inherited the width's velocity (height={}, frames={frames})",
                s.peek().height
            );
        }
        assert!(frames > 1, "component springs take several frames (frames={frames})");
        assert_eq!(s.peek(), Size::new(20.0, 100.0));
    }

    #[test]
    fn spring_done_returns_exact_target() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Regression: Spring converges asymptotically, so on done the Animatable must write the
        // exact target value. Before the fix it returned to + residual displacement, and
        // AnimatedVisibility's exit check (progress < 0.001) then stalled or misfired.
        use crate::runtime::state::State;
        let st = State::new(1.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_to(0.0, AnimationSpec::Spring(SpringSpec::default()));
        // Step until done: advance the frozen clock 16 ms per frame (60 fps) and let update() read
        // the injected time instead of the wall clock. Cap at 100 frames to avoid an infinite loop.
        let mut frames = 0;
        while anim.update() && frames < 100 {
            _clock.advance(Duration::from_millis(16));
            frames += 1;
        }
        assert!(frames < 100, "the spring should converge (stiffness=200 takes about 300-400 ms)");
        assert_eq!(st.peek(), 0.0, "on done the value must equal the target exactly (it used to be the residual)");
    }

    /// `target_value` is the asymptote the decay itself lands on, `v0 / friction` — the number a pager
    /// estimates "how many pages would this flick carry" from, so it must agree with the motion.
    #[test]
    fn a_decay_target_value_is_the_asymptote() {
        let spec = exponential_decay(4.2);
        assert!((spec.target_value(840.0) - 200.0).abs() < 1e-3, "840 / 4.2");
        assert!(
            (spec.target_value(-840.0) + 200.0).abs() < 1e-3,
            "signed, so a backward flick estimates backwards"
        );
        assert_eq!(spec.target_value(0.0), 0.0);
    }

    /// Decay (exponential): v0=1000, friction=4.2, so the limit is 1000/4.2 ~ 238.1. Verifies that the
    /// motion is monotonically increasing and settles exactly on the limit at done.
    #[test]
    fn decay_moves_and_stops_at_limit() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_decay(1000.0, DecaySpec::new(4.2, 0.1));
        let mut prev = 0.0f32;
        let mut frames = 0;
        while anim.update() && frames < 300 {
            let v = st.peek();
            assert!(v >= prev - 0.01, "decay must be monotonic (frame {}: {prev} -> {v})", frames);
            prev = v;
            _clock.advance(Duration::from_millis(20));
            frames += 1;
        }
        assert!(frames < 300, "decay should converge");
        let limit = 1000.0 / 4.2;
        assert!(
            (st.peek() - limit).abs() < 0.5,
            "on done it must settle exactly on the limit {} (got {})",
            limit,
            st.peek()
        );
    }

    /// P2-10: Repeatable supports a Keyframes base, running the keyframe curve each cycle. Before the
    /// fix it fell back to a 300 ms linear ramp (3 iterations = 900 ms); the Keyframes base is 50 ms x 3 = 150 ms.
    #[test]
    fn repeatable_keyframes_base() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Repeatable(RepeatableSpec::new(
                3,
                RepeatMode::Restart,
                KeyframesSpec::new(
                    Duration::from_millis(50),
                    vec![(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)],
                ),
            )),
        );
        let mut frames = 0;
        while anim.update() && frames < 30 {
            _clock.advance(Duration::from_millis(16));
            frames += 1;
        }
        assert!(frames < 20, "a Keyframes base of 3 x 50 ms should finish in ~10 frames (got {frames}; a 300 ms linear fallback would exceed 20)");
        assert_eq!(st.peek(), 100.0, "it must settle exactly on to when done");
    }

    /// P2-10: Repeatable supports a Snap base, which completes at once.
    #[test]
    fn repeatable_snap_base() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Repeatable(RepeatableSpec::new(3, RepeatMode::Restart, SnapSpec::default())),
        );
        assert!(!anim.update(), "a Snap base completes on the first update");
        assert_eq!(st.peek(), 100.0, "a Snap base completes at to");
    }

    /// Decay with an initial velocity of 0 completes at once and leaves the value unchanged.
    #[test]
    fn decay_zero_velocity_done_immediately() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(42.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_decay(0.0, DecaySpec::default());
        assert!(!anim.update(), "v0=0 completes at once");
        assert_eq!(st.peek(), 42.0, "v0=0 leaves the value unchanged");
    }

    /// animateIntAsState: a target change starts the animation, the value approaches monotonically and
    /// finally arrives exactly.
    #[test]
    fn animate_int_as_state_animates_to_target() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut composer = Composer::new();
        let value: std::cell::RefCell<Option<State<i32>>> = std::cell::RefCell::new(None);
        let mut recompose = |composer: &mut Composer, t: i32| {
            composer.compose(|ctx| {
                let v = crate::animation::animate_int_as_state(
                    ctx,
                    t,
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(300),
                        crate::animation::interpolator::Linear::new(),
                    ),
                );
                let _ = v.get(); // register the dependency (a target change re-runs this)
                *value.borrow_mut() = Some(v.clone());
            });
        };
        recompose(&mut composer, 0);
        assert_eq!(value.borrow().as_ref().unwrap().peek(), 0, "initially equal to target");

        // Target becomes 100, the animation starts, and mid-flight values are in 0..100 and rising.
        recompose(&mut composer, 100);
        let mut prev = 0i32;
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            _clock.advance(std::time::Duration::from_millis(20));
            let v = value.borrow().as_ref().unwrap().peek();
            assert!(v >= prev, "an i32 animation must rise monotonically ({prev} -> {v})");
            prev = v;
            frames += 1;
            if v >= 100 || frames > 60 {
                break;
            }
        }
        assert_eq!(prev, 100, "the i32 animation finally arrives exactly at target ({frames} frames)");
    }

    /// Generic animateValueAsState: a Color target animates to the destination colour.
    #[test]
    fn animate_value_as_state_color_reaches_target() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut composer = Composer::new();
        let from = crate::graphics::Color::from_argb(255, 0, 0, 0);
        let to = crate::graphics::Color::from_argb(255, 255, 255, 255);
        let value: std::cell::RefCell<Option<State<crate::graphics::Color>>> = std::cell::RefCell::new(None);
        let mut recompose = |composer: &mut Composer, t: crate::graphics::Color| {
            composer.compose(|ctx| {
                let v = crate::animation::animate_value_as_state(
                    ctx,
                    t,
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(200),
                        crate::animation::interpolator::Linear::new(),
                    ),
                );
                let _ = v.get();
                *value.borrow_mut() = Some(v.clone());
            });
        };
        recompose(&mut composer, from.clone());
        assert_eq!(value.borrow().as_ref().unwrap().peek(), from, "initially equal to target");

        recompose(&mut composer, to.clone());
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            _clock.advance(std::time::Duration::from_millis(20));
            let v = value.borrow().as_ref().unwrap().peek();
            frames += 1;
            if v == to || frames > 60 {
                break;
            }
        }
        assert_eq!(value.borrow().as_ref().unwrap().peek(), to, "the Color reaches target ({frames} frames)");
    }

    /// P2-9 velocity continuity (instance level): animate_to(Spring) mid-Decay, where the new
    /// animation inherits the old velocity (physical continuity, Compose's interruption semantics).
    #[test]
    fn retarget_inherits_velocity() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_decay(1000.0, DecaySpec::default());
        // Step 5 frames (20 ms): the velocity decays but is still > 0.
        for _ in 0..5 {
            anim.update();
            _clock.advance(Duration::from_millis(20));
        }
        let old_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert!(old_vel > 100.0, "the velocity should still be significant after 5 frames (got {old_vel})");
        // Interrupt with a new Spring toward 200: it inherits the old velocity.
        anim.animate_to(200.0, AnimationSpec::Spring(SpringSpec::default()));
        let new_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert_eq!(new_vel, old_vel, "a retarget must inherit the old velocity (it used to restart at 0)");
        // And it starts from the current value, with no jump.
        let from = anim.anim_state.as_ref().unwrap().from;
        assert!((from - st.peek()).abs() < 0.01, "from must be the current value (no jump)");
    }

    /// cancel_animation: after cancelling mid-flight the value is no longer overwritten by the
    /// animation. Regression: after a Reset button set(0), Decay wrote the old value back on the next
    /// frame, so the reset did nothing.
    #[test]
    fn cancel_animation_stops_decay() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        push_decay(st.clone(), 1000.0, DecaySpec::default());
        // Advance 5 frames: the value moves noticeably.
        for _ in 0..5 {
            super::update_animations();
            _clock.advance(Duration::from_millis(20));
        }
        let mid = st.peek();
        assert!(mid > 50.0, "the value should have moved noticeably under Decay (got {mid})");
        // Cancel the animation and set 0 (the Reset semantics).
        cancel_animation(&st);
        st.set(0.0);
        // Advance 5 more frames: the value must stay 0 (the animation must not write it back).
        for _ in 0..5 {
            super::update_animations();
            _clock.advance(Duration::from_millis(20));
        }
        assert_eq!(st.peek(), 0.0, "the value must stay put after cancelling (Decay used to write it back)");
    }

    /// Review fix: velocity residue. A second interruption (Spring -> Tween -> Spring) must restart
    /// from rest, because last_velocity does not update during a Tween and inheriting it would carry a
    /// stale physical velocity.
    #[test]
    fn retarget_after_tween_restarts_from_rest() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        // 1) Get the Spring running (a significant velocity).
        anim.animate_to(100.0, AnimationSpec::Spring(SpringSpec::default()));
        for _ in 0..5 {
            anim.update();
            _clock.advance(Duration::from_millis(20));
        }
        let spring_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert!(spring_vel > 10.0, "the velocity should be significant after the Spring runs (got {spring_vel})");
        // 2) Interrupt with a Tween (review fix: non-physical specs do not inherit velocity; it zeroes).
        anim.animate_to(200.0, AnimationSpec::Tween(TweenSpec::default()));
        for _ in 0..3 {
            anim.update();
            _clock.advance(Duration::from_millis(16));
        }
        let tween_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert_eq!(tween_vel, 0.0, "a Tween must start from rest (review fix: it does not inherit the Spring velocity)");
        // 3) Interrupt with a Spring again: it must restart from rest (it used to inherit the stale spring_vel).
        anim.animate_to(50.0, AnimationSpec::Spring(SpringSpec::default()));
        let final_vel = anim.anim_state.as_ref().unwrap().last_velocity;
        assert_eq!(final_vel, 0.0, "a Spring after a Tween interruption must restart from rest (it used to inherit the stale velocity {spring_vel})");
    }

    /// fling: exponential decay plus a boundary clamp. Hitting the boundary writes the boundary value
    /// and completes at once (matching Compose, where a fling stops once it is spent). v0=10000 gives a
    /// limit of 2380, clamped to 100.
    #[test]
    fn fling_clamps_at_boundary_and_stops() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.set_clamp(|o| o.min(100.0));
        anim.animate_decay(10000.0, DecaySpec::new(4.2, 0.1));
        let mut frames = 0;
        while anim.update() && frames < 300 {
            _clock.advance(Duration::from_millis(20));
            frames += 1;
        }
        assert!(frames < 300, "a fling hitting the boundary should converge ({frames} frames)");
        assert_eq!(st.peek(), 100.0, "it clamps to the boundary value and stops");
    }

    #[test]
    fn fling_boundary_callback_reports_remaining_velocity_once() {
        let (_guard, _clock) = super::tests::serial_clock();
        clear_all_animations();
        let state = State::new(95.0f32);
        let boundary_velocity = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let reported = boundary_velocity.clone();
        push_fling_with_boundary(
            state.clone(),
            10_000.0,
            DecaySpec::new(4.2, 0.1),
            |offset| offset.min(100.0),
            move |velocity| reported.lock().unwrap().push(velocity),
            || {},
        );
        for _ in 0..20 {
            if !update_animations() { break; }
            _clock.advance(Duration::from_millis(20));
        }
        let velocities = boundary_velocity.lock().unwrap();
        assert_eq!(state.peek(), 100.0);
        assert_eq!(velocities.len(), 1);
        assert!(velocities[0] > 0.0);
    }

    #[test]
    fn fling_clamps_below_zero() {
        let (_guard, _clock) = super::tests::serial_clock();
        let st = State::new(50.0f32);
        let mut anim = Motion::new(st.clone());
        anim.set_clamp(|o| o.max(0.0));
        anim.animate_decay(-500.0, DecaySpec::new(4.2, 0.1));
        let mut frames = 0;
        while anim.update() && frames < 300 {
            _clock.advance(Duration::from_millis(20));
            frames += 1;
        }
        assert!(frames < 300, "the backward fling should converge ({frames} frames)");
        assert_eq!(st.peek(), 0.0, "the lower bound clamps to 0");
    }

    /// Review fix: DecaySpec fails fast on friction <= 0, so a misconfiguration surfaces at once.
    #[test]
    #[should_panic(expected = "friction must be > 0")]
    fn decay_spec_rejects_nonpositive_friction() {
        let (_guard, _clock) = super::tests::serial_clock();
        let _ = DecaySpec::new(0.0, 0.1);
    }

    /// Limitation #6: a Tween/Keyframes with duration=0 completes at once with the value exactly = to
    /// (no division-by-zero NaN).
    #[test]
    fn zero_duration_tween_and_keyframes_complete_immediately() {
        let (_guard, _clock) = super::tests::serial_clock();
        // Tween duration=0
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_to(42.0, AnimationSpec::Tween(TweenSpec::new(Duration::ZERO, interpolator::Linear::new())));
        assert!(!anim.update(), "a duration=0 Tween completes on the first update");
        assert_eq!(st.peek(), 42.0, "a duration=0 Tween completes at to");
        // Keyframes duration=0
        let st2 = State::new(0.0f32);
        let mut anim2 = Motion::new(st2.clone());
        anim2.animate_to(
            42.0,
            AnimationSpec::Keyframes(KeyframesSpec::new(Duration::ZERO, vec![(0.0, 0.0), (1.0, 1.0)])),
        );
        assert!(!anim2.update(), "duration=0 Keyframes completes on the first update");
        assert_eq!(st2.peek(), 42.0, "duration=0 Keyframes completes at to");
    }

    /// P2-9 velocity continuity (push path): push_decay then a push_animatable retarget, where the new
    /// animation in the global table inherits the old velocity.
    #[test]
    fn push_retarget_inherits_velocity() {
        let (_guard, _clock) = super::tests::serial_clock();
        use crate::runtime::state::State;
        let st = State::new(0.0f32);
        push_decay(st.clone(), 1000.0, DecaySpec::default());
        // Advance 5 frames (update_animations drives the global table).
        for _ in 0..5 {
            super::update_animations();
            _clock.advance(Duration::from_millis(20));
        }
        let vel_before = {
            let list = ACTIVE_ANIMATIONS.lock().unwrap();
            list.iter().find(|a| a.state_id() == st.state_id()).map(|a| a.last_velocity()).unwrap_or(0.0)
        };
        assert!(vel_before > 100.0, "the velocity should be significant after Decay runs (got {vel_before})");
        // Retarget: push a new target (Spring) for the same state.
        push_animatable(st.clone(), 200.0, AnimationSpec::Spring(SpringSpec::default()));
        let vel_after = {
            let list = ACTIVE_ANIMATIONS.lock().unwrap();
            list.iter().find(|a| a.state_id() == st.state_id()).map(|a| a.last_velocity()).unwrap_or(0.0)
        };
        assert!(
            (vel_after - vel_before).abs() < 1.0,
            "a push retarget must inherit the old velocity (before={vel_before} after={vel_after}; the Spring's first step has dt ~ 0, hence the small difference)"
        );
        // Clean up: let the animation finish so no residue leaks into other tests.
        for _ in 0..60 {
            if !super::update_animations() { break; }
            _clock.advance(Duration::from_millis(16));
        }
    }

    #[test]
    fn on_finish_fires_when_animation_completes() {
        let (_guard, _clock) = super::tests::serial_clock();
        // finishedListener (Compose parity): fires once on the frame the animation completes.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use crate::runtime::state::State;
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = fired.clone();
        anim.on_finish(move || {
            f2.store(true, Ordering::SeqCst);
        });
        anim.animate_to(1.0, AnimationSpec::Snap(SnapSpec::default()));
        anim.update(); // a Snap completes in one frame, firing the callback
        assert!(fired.load(Ordering::SeqCst), "completion should fire on_finish");
        assert_eq!(st.peek(), 1.0, "the animated value should reach the target");
    }
    /// Reproduces the animation_demo scroll freeze (dup-key): Column + vertical_scroll + Crossfade +
    /// several sections. Scrolling alternates Skip/Enter and the animations notify, so each frame
    /// composes twice (a recompose loop); nodes must not leak or duplicate their keys.
    #[test]
    fn scroll_with_animation_no_dup_key() {
        use std::cell::RefCell;
        let (_guard, _clock) = super::tests::serial_clock();
        let mut composer = crate::runtime::composer::Composer::new();
        let scroll = RefCell::new(None::<crate::modifier::ScrollState>);
        let page = RefCell::new(None::<crate::runtime::state::State<u32>>);
        let frame_parity = RefCell::new(std::rc::Rc::new(std::cell::Cell::new(false)));
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);

        let scene = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(crate::compose!(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::Column::new()
                        .modifier(crate::modifier::Modifier::new()
                            .fill_max_size()
                            .vertical_scroll({
                                // Clone first, then decide: borrow_mut would clash with the outer borrow.
                                let cur = scroll.borrow().clone();
                                if let Some(s) = cur {
                                    s
                                } else {
                                    let s = ctx.remember(|| crate::modifier::ScrollState::new()).get();
                                    *scroll.borrow_mut() = Some(s.clone());
                                    s
                                }
                            }))
                        .build(ctx, |ctx| {
                            for i in 0..10 {
                                crate::components::Text::new(format!("section {i}")).font_size(14.0).build(ctx);
                                crate::layout::Row::new().build(ctx, |ctx| {
                                    crate::components::Text::new(format!("content {i} - tall content")).build(ctx);
                                });
                            }
                            // The animated section (notifies every frame, as in demos 8a/8b).
                            let a = ctx.animate_float_as_state(
                                if frame_parity.borrow().get() { 200.0 } else { 40.0 },
                                crate::animation::TweenSpec::default().into(),
                            );
                            crate::components::Text::new(format!("w={}", a.peek())).build(ctx);
                            let p = ctx.remember(|| 0u32);
                            *page.borrow_mut() = Some(p.clone());
                            crate::components::crossfade::Crossfade::new(p.clone())
                                .animation(crate::animation::TweenSpec::default())
                                .build(ctx, |ctx, pg| {
                                    crate::components::Text::new(format!("page {pg}")).build(ctx);
                                });
                        });
                });
            }));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        };

        scene(&mut composer);
        let nodes1 = composer.arena.nodes.len();
        for frame in 0..30 {
            if let Some(s) = scroll.borrow().clone() {
                s.scroll_to((frame % 5) as f32 * 150.0, 2000.0);
            }
            if frame % 10 == 0 {
                if let Some(p) = page.borrow().clone() {
                    p.set((p.get() + 1) % 3);
                }
            }
            frame_parity.borrow().set(!frame_parity.borrow().get());
            for _ in 0..5 {
                crate::animation::update_animations();
                _clock.advance(std::time::Duration::from_millis(8));
            }
            scene(&mut composer);
            scene(&mut composer);
            let n = composer.arena.nodes.len();
            assert!(n <= nodes1 + 20,
                "frame {frame}: the node count must not keep growing ({n} vs initial {nodes1}), a leak or duplicate");
        }
    }

    /// `push_animatable_with_done` has to cancel a conflicting animation exactly as
    /// `push_animatable_handle` does (`animation.rs:294-309`). The value's CURRENT value can equal the
    /// NEW target while an animation toward a DIFFERENT target is still running — the value passes
    /// through it mid-flight — and returning early there leaves the stale animation pulling the value
    /// to its old target. Measured before the fix: an animation 0.2 → 1.0 was pushed, the value was
    /// read at ~0.62 and that same value pushed back as the target, and the state then sailed on to
    /// 1.0 instead of staying put.
    #[test]
    fn push_animatable_with_done_cancels_a_conflicting_animation() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let state = crate::runtime::state::State::new(0.2f32);
        let slow = AnimationSpec::Spring(SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        });
        push_animatable_with_done(state.clone(), 1.0, slow.clone(), || {});
        // Let it get well clear of 0.2 but nowhere near 1.0.
        for _ in 0..12 {
            update_animations();
            _clock.advance(std::time::Duration::from_millis(16));
        }
        let mid = state.peek();
        assert!(mid > 0.3 && mid < 0.9, "the first animation is mid-flight (value {mid})");
        // Push the value it is AT right now as the new target: that is the `peek == target` branch.
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d = done.clone();
        push_animatable_with_done(state.clone(), mid, slow, move || {
            d.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        for _ in 0..30 {
            update_animations();
            _clock.advance(std::time::Duration::from_millis(16));
        }
        let end = state.peek();
        assert!(
            (end - mid).abs() < 0.05,
            "the conflicting animation must be cancelled, so the value stays at {mid} (ended at {end})"
        );
        assert!(
            done.load(std::sync::atomic::Ordering::SeqCst),
            "and the zero-displacement animation completes, so its callback runs"
        );
    }

    /// The velocity variant had the same hole from the other side: on `peek() == target` it reported
    /// completion and returned, without cancelling a conflicting motion — so the callback announced
    /// "finished" while the stale animation was still pulling the value (before this fix: it reported
    /// done at 0.55 and the value ended at 0.97).
    #[test]
    fn push_animatable_with_velocity_and_done_cancels_a_conflicting_animation_too() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let state = crate::runtime::state::State::new(0.2f32);
        let slow = AnimationSpec::Spring(SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        });
        push_animatable(state.clone(), 1.0, slow.clone());
        for _ in 0..12 {
            update_animations();
            _clock.advance(std::time::Duration::from_millis(16));
        }
        let mid = state.peek();
        assert!(mid > 0.3 && mid < 0.9, "the first animation is mid-flight (value {mid})");
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let d = done.clone();
        push_animatable_with_velocity_and_done(state.clone(), mid, slow, 0.0, move || {
            d.store(true, std::sync::atomic::Ordering::SeqCst)
        });
        for _ in 0..30 {
            update_animations();
            _clock.advance(std::time::Duration::from_millis(16));
        }
        let end = state.peek();
        assert!(
            (end - mid).abs() < 0.05,
            "the conflicting animation must be cancelled, so the value stays at {mid} (ended at {end})"
        );
        assert!(
            done.load(std::sync::atomic::Ordering::SeqCst),
            "and the callback fires once the zero-displacement motion lands"
        );
    }

    /// A loop honours its `initial_start_offset` (Compose's `initialStartOffset`): `Delay` holds the
    /// loop's first value for the offset, `FastForward` begins that far into the first cycle.
    #[test]
    fn an_infinite_loop_honours_its_start_offset() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        // FastForward: a 400 ms cycle entered 300 ms in → the first step is already three quarters along.
        let fast = crate::runtime::state::State::new(0.0f32);
        push_infinite(
            fast.clone(),
            0.0,
            1.0,
            InfiniteRepeatableSpec::restart(Duration::from_millis(400))
                .with_start_offset(Duration::from_millis(300), StartOffsetType::FastForward),
        );
        update_animations();
        let v = fast.peek();
        assert!(
            (v - 0.75).abs() < 0.1,
            "FastForward starts three quarters into the cycle (value {v})"
        );
        clear_all_animations();
        // Delay: the same cycle, but held for 300 ms before it begins.
        let slow = crate::runtime::state::State::new(0.0f32);
        push_infinite(
            slow.clone(),
            0.0,
            1.0,
            InfiniteRepeatableSpec::restart(Duration::from_millis(400))
                .with_start_offset(Duration::from_millis(300), StartOffsetType::Delay),
        );
        update_animations();
        let held = slow.peek();
        assert!(held < 0.02, "Delay holds the first value for the offset (value {held})");
        _clock.advance(Duration::from_millis(430)); // 300 ms of waiting + ~130 ms into the cycle
        update_animations();
        let moving = slow.peek();
        assert!(
            moving > 0.1 && moving < 0.6,
            "then the first cycle runs from its beginning (value {moving})"
        );
        clear_all_animations();
    }

    /// `ComposeCtx::animate_float_as_state_with_done` is Compose's
    /// `animateFloatAsState(targetValue, animationSpec, label, finishedListener)`: the slot owns the
    /// state, so nothing else can drive it, and the callback fires once per motion that finishes.
    #[test]
    fn the_ctx_animate_state_with_done_reports_each_finished_motion() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let visible = crate::runtime::state::State::new(false);
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut composer = Composer::new();
        let spec = AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(120),
            std::sync::Arc::new(interpolator::Linear::new())
                as std::sync::Arc<dyn interpolator::Interpolator>,
        ));
        let mut frame = |composer: &mut Composer| {
            let on = visible.get();
            let c = counter.clone();
            let spec = spec.clone();
            composer.compose(|ctx| {
                let value = ctx.animate_float_as_state_with_done(if on { 100.0 } else { 0.0 }, spec, move || {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
                let _ = value.get();
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut run = |composer: &mut Composer, frames: usize, frame: &mut dyn FnMut(&mut Composer)| {
            for _ in 0..frames {
                frame(composer);
                update_animations();
                _clock.advance(std::time::Duration::from_millis(16));
            }
        };
        frame(&mut composer); // the state is created AT the target: nothing runs, nothing reports
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0);
        visible.set(true);
        run(&mut composer, 20, &mut frame);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the grow reports exactly once"
        );
        // Interrupt it: the abandoned motion does not report, the replacement does.
        visible.set(false);
        run(&mut composer, 5, &mut frame);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a motion that was replaced must not report"
        );
        run(&mut composer, 20, &mut frame);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "the shrink reports once when it lands"
        );
        clear_all_animations();
    }

    /// Compose's `Animatable<T>.animateDecay(initialVelocity: T, decayAnimationSpec)` runs one float
    /// decay per component (`DecayAnimationSpec<T>`, vectorized by the type's converter). Before this,
    /// a Decay reaching a multi-component value was replaced by Snap
    /// (`normalise_target_spec`), so a fling on an `Offset` never moved at all.
    #[test]
    fn a_vector_fling_decays_each_component_on_its_own() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let start = crate::unit::Offset::new(10.0, 20.0);
        let velocity = crate::unit::Offset::new(100.0, -60.0);
        let friction = 4.2f32;
        let spec = DecaySpec::new(friction, 0.1);
        let state = crate::runtime::state::State::new(start);
        push_decay_vector(state.clone(), velocity, spec);
        _clock.advance(Duration::from_millis(16));
        update_animations();
        let mid = state.peek();
        let (limit_x, limit_y) = (10.0 + 100.0 / friction, 20.0 - 60.0 / friction);
        assert!(
            mid.x > start.x && mid.x < limit_x,
            "x travels toward its own limit (at {})",
            mid.x
        );
        assert!(
            mid.y < start.y && mid.y > limit_y,
            "y travels the other way, on its own limit (at {})",
            mid.y
        );
        let mut frames = 0;
        while update_animations() && frames < 400 {
            _clock.advance(Duration::from_millis(8));
            frames += 1;
        }
        let end = state.peek();
        assert!(
            (end.x - limit_x).abs() < 0.01 && (end.y - limit_y).abs() < 0.01,
            "each component parks on its exact limit (ended at {end:?}, expected {limit_x}x{limit_y})"
        );
        assert!(frames < 400, "the fling ends on its own ({frames} frames)");
        clear_all_animations();
    }

    /// Compose's keyframe shape — absolute milliseconds and values in the type's units — is normalised
    /// into the engine's fraction band by `KeyframesOf`, and the resulting motion is the one Compose
    /// describes: a 100 → 300 flight through `340f at 200ms` overshoots to 340 and lands exactly on 300.
    #[test]
    fn composes_keyframe_shape_drives_the_same_motion() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let spec = KeyframesOf::between(100.0, 300.0)
            .at(0, 100.0)
            .at(200, 340.0)
            .at(400, 300.0)
            .build();
        assert_eq!(spec.duration, Duration::from_millis(400));
        assert_eq!(spec.frames.len(), 3);
        // 200 ms of a 400 ms flight is progress 0.5, and 340 is 1.2 bands up from 100.
        assert!((spec.frames[1].0 - 0.5).abs() < 1e-6, "the middle keyframe sits at half the duration");
        assert!((spec.frames[1].1 - 1.2).abs() < 1e-6, "and 340 is 1.2 bands above the 100 start");
        assert_eq!(interpolate_keyframes(&spec.frames, 0.0), 0.0);
        assert!((interpolate_keyframes(&spec.frames, 0.5) - 1.2).abs() < 1e-6);
        assert_eq!(interpolate_keyframes(&spec.frames, 1.0), 1.0);
        // End to end through the engine: 100 → 300 overshoots at half time and lands on the target.
        let state = crate::runtime::state::State::new(100.0f32);
        push_animatable(state.clone(), 300.0, AnimationSpec::Keyframes(spec.clone()));
        update_animations();
        let mut mid = state.peek();
        let mut frames = 0;
        while update_animations() && frames < 200 {
            _clock.advance(Duration::from_millis(4));
            frames += 1;
            let v = state.peek();
            if v > mid {
                mid = v;
            }
        }
        assert!(
            mid > 330.0,
            "the flight overshoots toward 340 the way the keyframes say (peak {mid})"
        );
        assert_eq!(state.peek(), 300.0, "and it lands exactly on the target");
        // A keyframe list without a 0 ms entry starts at the `from` value (Compose's rule).
        let no_first = KeyframesOf::between(0.0, 1.0).at(100, 0.5).at(200, 1.0).build();
        assert_eq!(no_first.frames.first().map(|f| f.0), Some(0.0));
        assert_eq!(no_first.frames.first().map(|f| f.1), Some(0.0));
        clear_all_animations();
    }

    /// Compose's `snap(delayMillis)` holds the value for the delay and only then jumps, and Compose's
    /// `repeatable(..., initialStartOffset = StartOffset(…))` shifts the first cycle of a finite repeat
    /// the same way the infinite spec does. Both were missing: winia's `Snap` had no delay at all, and a
    /// repeat always began at its first frame.
    #[test]
    fn a_delayed_snap_and_a_delayed_repeat_both_hold_first() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        // snap(delayMillis = 250): nothing moves for 250 ms, then the target appears in one frame.
        let state = crate::runtime::state::State::new(0.0f32);
        push_animatable(state.clone(), 1.0, AnimationSpec::Snap(SnapSpec::new(Duration::from_millis(250))));
        update_animations();
        _clock.advance(Duration::from_millis(120));
        assert!(update_animations(), "the animation is still running during the hold");
        assert_eq!(state.peek(), 0.0, "and the value has not moved yet");
        _clock.advance(Duration::from_millis(180));
        assert!(!update_animations(), "after the delay the snap completes");
        assert_eq!(state.peek(), 1.0, "writing the target exactly");
        clear_all_animations();
        // repeatable(2, tween(200, linear), FastForward 150): the first frame is already 3/4 in.
        let fast = crate::runtime::state::State::new(0.0f32);
        push_animatable(
            fast.clone(),
            1.0,
            AnimationSpec::Repeatable(
                RepeatableSpec::new(
                    2,
                    RepeatMode::Restart,
                    TweenSpec::new(Duration::from_millis(200), linear()),
                )
                .with_start_offset(Duration::from_millis(150), StartOffsetType::FastForward),
            ),
        );
        update_animations();
        let started = fast.peek();
        assert!(
            (started - 0.75).abs() < 0.1,
            "a fast-forwarded repeat starts three quarters into its first cycle (at {started})"
        );
        clear_all_animations();
        // The same repeat with Delay 300 ms still sits at the first value 120 ms in.
        let slow = crate::runtime::state::State::new(0.0f32);
        push_animatable(
            slow.clone(),
            1.0,
            AnimationSpec::Repeatable(
                RepeatableSpec::new(
                    2,
                    RepeatMode::Restart,
                    TweenSpec::new(Duration::from_millis(200), linear()),
                )
                .with_start_offset(Duration::from_millis(300), StartOffsetType::Delay),
            ),
        );
        update_animations();
        _clock.advance(Duration::from_millis(120));
        update_animations();
        assert_eq!(slow.peek(), 0.0, "a delayed repeat holds its first value");
        _clock.advance(Duration::from_millis(300));
        update_animations();
        let moving = slow.peek();
        assert!(
            moving > 0.05 && moving < 0.95,
            "and only then does its first cycle run (at {moving})"
        );
        clear_all_animations();
    }

    /// Compose's `Animatable` handle — `rememberAnimatable` with `animateTo` / `snapTo` / `stop` /
    /// `isRunning` / `targetValue`. Three of those had no equivalent at all: nothing could tell a caller
    /// what a running motion was heading for, whether one was running, or cancel it and keep the
    /// velocity. `snapTo` cancelling is the part that is easy to get wrong: leaving the motion
    /// registered lets it pull the value straight back off what the caller just wrote.
    #[test]
    fn the_animatable_handle_reports_and_controls_its_motion() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");
        assert_eq!(a.value(), 0.0);
        assert!(!a.is_running(), "nothing runs before the first animateTo");
        assert_eq!(a.target(), None);

        let slow = AnimationSpec::Spring(SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        });
        a.animate_to(100.0, slow.clone());
        assert!(a.is_running());
        assert_eq!(a.target(), Some(100.0), "the handle reports what it is heading for");
        let mut frames = 0;
        while frames < 10 {
            update_animations();
            _clock.advance(Duration::from_millis(16));
            frames += 1;
        }
        let mid = a.value();
        assert!(mid > 0.0 && mid < 100.0, "the motion is on its way (at {mid})");

        let velocity = a.stop();
        assert!(!a.is_running(), "stop() cancels the motion");
        assert_eq!(a.target(), None, "and there is no target left");
        assert!(velocity.abs() > 1.0, "stop() hands back the velocity it had ({velocity})");
        let stopped = a.value();
        for _ in 0..5 {
            update_animations();
            _clock.advance(Duration::from_millis(16));
        }
        assert_eq!(a.value(), stopped, "and nothing moves it afterwards");

        a.animate_to(500.0, slow.clone());
        for _ in 0..4 {
            update_animations();
            _clock.advance(Duration::from_millis(16));
        }
        assert!(a.value() > stopped, "the second motion is running");
        a.snap_to(0.0);
        assert_eq!(a.value(), 0.0);
        assert!(!a.is_running(), "snapTo cancels too");
        for _ in 0..6 {
            update_animations();
            _clock.advance(Duration::from_millis(16));
        }
        assert_eq!(a.value(), 0.0, "the cancelled motion does not pull the value back");
        assert_eq!(a.as_state().peek(), 0.0, "as_state() is the same value the frame paints");
        clear_all_animations();
    }

    /// Compose's `visibilityThreshold` argument (`animateFloatAsState(..., visibilityThreshold = …)`):
    /// winia's springs carry the threshold on the spec, so this is the per-call override — a loose
    /// threshold arrives sooner, a tight one keeps travelling.
    #[test]
    fn a_looser_visibility_threshold_arrives_sooner() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let soft = SpringSpec {
            stiffness: 60.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        };
        let frames_until_done = |threshold: f32| -> u32 {
            let state = crate::runtime::state::State::new(0.0f32);
            push_animatable(
                state.clone(),
                100.0,
                AnimationSpec::Spring(soft.clone()).with_visibility_threshold(threshold),
            );
            let mut frames = 0;
            while update_animations() && frames < 600 {
                _clock.advance(Duration::from_millis(8));
                frames += 1;
            }
            assert_eq!(state.peek(), 100.0, "a spring still lands on its exact target");
            frames
        };
        let tight = frames_until_done(0.1);
        clear_all_animations();
        let loose = frames_until_done(40.0);
        clear_all_animations();
        assert!(
            loose < tight,
            "a 40-unit threshold settles before a 0.1-unit one (loose {loose} frames, tight {tight})"
        );
        // And the threshold really is inside the spec, where Compose puts it: nothing else changes.
        let spec = AnimationSpec::Spring(soft.clone()).with_visibility_threshold(7.5);
        match spec {
            AnimationSpec::Spring(s) => {
                assert_eq!(s.threshold, 7.5);
                assert_eq!(s.stiffness, soft.stiffness, "the rest of the spring is untouched");
            }
            other => panic!("expected a spring, got {other:?}"),
        }
    }

    /// `active_animations_report` is what the debug server's `al` command prints: it has to name the
    /// animation that owns a moving value, with the numbers needed to tell a stuck motion from a healthy
    /// one. Before it there was no way to ask — the registry was write-only from outside.
    #[test]
    fn the_active_animation_report_names_what_is_running() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        assert!(active_animations_report().is_empty(), "nothing runs before anything is pushed");

        let state = crate::runtime::state::State::new(40.0f32);
        push_animatable(state.clone(), 200.0, AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(300), Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>)));
        update_animations();
        let lines = active_animations_report();
        assert_eq!(lines.len(), 1, "one motion, one line ({lines:?})");
        let line = &lines[0];
        assert!(line.contains(&format!("state={:?}", state.state_id())), "names the state ({line})");
        assert!(line.contains("spec=tween(300ms)"), "names the spec and its duration ({line})");
        assert!(line.contains("from=40.000") && line.contains("target=200.000"), "names the ends ({line})");

        // A fling has no target: the line says decay and carries the friction instead.
        let fling = crate::runtime::state::State::new(0.0f32);
        push_decay(fling.clone(), 900.0, exponential_decay(4.2));
        update_animations();
        let lines = active_animations_report();
        assert_eq!(lines.len(), 2, "the tween and the fling ({lines:?})");
        assert!(
            lines.iter().any(|l| l.contains("spec=decay(friction=4.20)")),
            "the fling is named as a decay ({lines:?})"
        );

        // A vector motion says how many components it is driving.
        let vector = crate::runtime::state::State::new(crate::unit::Offset::new(0.0, 0.0));
        push_decay_vector(vector.clone(), crate::unit::Offset::new(100.0, -60.0), exponential_decay(4.2));
        update_animations();
        assert!(
            active_animations_report()
                .iter()
                .any(|l| l.contains("decay_components=2")),
            "the diagonal fling reports its two components"
        );

        // Nothing is running once every animation has been dropped.
        clear_all_animations();
        assert!(active_animations_report().is_empty());
    }

    /// A frame longer than `FIXED_DT * MAX_STEPS` advances a spring by only that much, while a Tween reads
    /// the wall clock and arrives — so one stall makes the two disagree and the spring quietly takes
    /// longer than the real time it was given. Compose's springs are functions of elapsed time, so they
    /// are unaffected. Measured here so the deviation is a number, not a suspicion.
    #[test]
    fn a_long_frame_does_not_leave_a_spring_behind_the_clock() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let spring = crate::runtime::state::State::new(0.0f32);
        let tween = crate::runtime::state::State::new(0.0f32);
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        let soft = SpringSpec {
            stiffness: 200.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            ..SpringSpec::default()
        };
        push_animatable(spring.clone(), 100.0, AnimationSpec::Spring(soft));
        push_animatable(
            tween.clone(),
            100.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), linear())),
        );
        update_animations();
        // One 400 ms frame: no update runs in between, exactly what a stalled loop looks like.
        _clock.advance(Duration::from_millis(400));
        update_animations();
        let (s, t) = (spring.peek(), tween.peek());
        assert_eq!(t, 100.0, "the tween reads the clock, so it arrives ({t})");
        // A stiffness-200 spring needs ~4/omega0 = 280 ms to settle; 400 ms of real time is past that, so
        // anything under ~96 means the frame was truncated rather than integrated. (The analytic value at
        // 400 ms is 97.68; six 1/60 s semi-implicit steps land at 96.88, the integrator's own error, which
        // is why the bar is a range and not the analytic number.)
        assert!(
            s > 96.0,
            "the spring integrates the whole 400 ms it was given (at {s} of 100; 71.8 means the frame was cut at 10 x 1/60 s)"
        );
        clear_all_animations();
    }

    /// Compose's `Animatable.updateBounds`: the value never leaves its range and a motion that reaches a
    /// bound ends exactly there. Without the range the spring would settle at 5.0 and the fling would
    /// travel on to 900/4.2 = 214.3.
    #[test]
    fn the_handle_keeps_its_value_inside_its_bounds() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");

        a.set_bounds(0.0, 1.0);
        assert_eq!(a.bounds(), Some((0.0, 1.0)));
        a.animate_to(
            5.0,
            AnimationSpec::Spring(SpringSpec {
                stiffness: 300.0,
                ..SpringSpec::default()
            }),
        );
        for _ in 0..60 {
            update_animations();
            _clock.advance(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 1.0, "the value stops exactly at the upper bound");
        assert!(!a.is_running(), "and the motion ends there rather than pushing on");

        // The same for a fling: 900 units/s of decay would travel 214.3, but the range ends it at 100.
        a.set_bounds(0.0, 100.0);
        a.snap_to(0.0);
        a.animate_decay(900.0, exponential_decay(4.2));
        for _ in 0..60 {
            update_animations();
            _clock.advance(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 100.0, "the fling parks on the bound");
        assert!(!a.is_running());

        // Dropping the range lets it travel anywhere again.
        a.clear_bounds();
        assert_eq!(a.bounds(), None);
        a.animate_to(
            200.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(60), Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>)),
        );
        for _ in 0..40 {
            update_animations();
            _clock.advance(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 200.0, "with no bounds the target is reached");
        clear_all_animations();
    }

    /// `animate_decay` on the handle is the vector form: an `Offset` fling decays each axis on its own
    /// limit. (Bounds are a scalar concept — see the module note on `set_bounds`.)
    #[test]
    fn the_handle_can_fling_a_vector_value() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<crate::unit::Offset>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(crate::unit::Offset::new(0.0, 0.0));
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let o = held.borrow().clone().expect("the slot remembered the animatable");
        o.animate_decay(crate::unit::Offset::new(100.0, -60.0), exponential_decay(4.2));
        let mut frames = 0;
        while update_animations() && frames < 400 {
            _clock.advance(Duration::from_millis(8));
            frames += 1;
        }
        let v = o.value();
        assert!(
            (v.x - 100.0 / 4.2).abs() < 0.01 && (v.y + 60.0 / 4.2).abs() < 0.01,
            "each axis parks on 100/4.2 and -60/4.2 (at {v:?})"
        );
        assert!(!o.is_running());
        clear_all_animations();
    }

    /// Compose's `AnimationResult`: the three ways a motion can end were indistinguishable through a
    /// zero-argument callback — a motion cut short by the overrun guard reported "finished" like any other,
    /// and one stopped by a value range looked the same. Measured here for all three.
    #[test]
    fn a_completion_callback_is_told_why_the_motion_ended() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<MotionEnd>::new()));

        let record = {
            let seen = seen.clone();
            move |end: MotionEnd| seen.lock().unwrap().push(end)
        };
        let quick = || AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(60), Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>));
        let step = |frames: u32| {
            for _ in 0..frames {
                update_animations();
                _clock.advance(Duration::from_millis(6));
            }
        };

        // 1. It arrived.
        a.animate_to_with_result(100.0, quick(), record.clone());
        step(30);
        assert_eq!(seen.lock().unwrap().len(), 1, "one motion, one report");
        assert_eq!(seen.lock().unwrap()[0].reason, MotionEndReason::Finished);

        // 2. A value range stopped it: 5.0 is outside (0, 1), so the value is written at the bound.
        a.set_bounds(0.0, 1.0);
        a.animate_to_with_result(5.0, quick(), record.clone());
        step(30);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[1].reason, MotionEndReason::BoundReached, "the bound is named");
        assert_eq!(a.value(), 1.0);
        a.clear_bounds();
        a.snap_to(0.0);

        // 3. The overrun guard cut it short: a spring too soft to settle inside five seconds. This is the
        // one reason with no Compose counterpart, and the only way to reach it is to wait it out.
        a.animate_to_with_result(
            100.0,
            AnimationSpec::Spring(SpringSpec {
                stiffness: 0.5,
                damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
                threshold: 0.001,
                mass: 1.0,
            }),
            record.clone(),
        );
        // 400 frames of 30 ms = 12 s of engine time: past the 5 s guard, and no wall-clock wait.
        for _ in 0..400 {
            update_animations();
            _clock.advance(Duration::from_millis(30));
        }
        assert_eq!(seen.lock().unwrap().len(), 3, "the guard reports instead of vanishing");
        assert_eq!(
            seen.lock().unwrap()[2].reason,
            MotionEndReason::Forced,
            "and it says it was cut short rather than finished"
        );
        clear_all_animations();
    }

    /// The estimator is Compose's own code, so it is checked against a method that is not: for the
    /// critically damped case the solution of a step from rest is `x(t) = D·(1 + ω t)·e^(-ω t)`, and the
    /// crossing of `|x| = delta` is single and monotone, so a bisection finds it independently.
    #[test]
    fn the_spring_duration_estimate_matches_a_bisection() {
        for (stiffness, displacement) in [(400.0f32, 100.0f32), (200.0, 100.0), (1000.0, 20.0)] {
            let omega = (stiffness as f64).sqrt();
            let x = |t: f64| displacement as f64 * (1.0 + omega * t) * (-omega * t).exp();
            let (mut lo, mut hi) = (0.0f64, 5.0f64);
            assert!(x(hi).abs() < 1.0, "the bracket has to contain the crossing");
            for _ in 0..90 {
                let mid = 0.5 * (lo + hi);
                if x(mid).abs() > 1.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let reference = 0.5 * (lo + hi);
            let estimate =
                estimate_spring_duration_seconds(stiffness, 1.0, 0.0, displacement, 1.0) as f64;
            assert!(
                (estimate - reference).abs() <= 0.01 + reference * 0.02,
                "the port agrees with the analytic crossing ({stiffness} over {displacement}: estimate {estimate:.4}s against {reference:.4}s)"
            );
        }
        let base = estimate_spring_duration_seconds(400.0, 1.0, 0.0, 100.0, 1.0);
        assert!(
            estimate_spring_duration_seconds(400.0, 1.0, 0.0, 100.0, 0.5) > base,
            "a tighter threshold takes longer"
        );
        assert!(
            estimate_spring_duration_seconds(1600.0, 1.0, 0.0, 100.0, 1.0) < base,
            "a stiffer spring takes less"
        );
        assert!(
            estimate_spring_duration_seconds(400.0, 0.0, 0.0, 100.0, 1.0).is_infinite(),
            "an undamped spring never settles"
        );
    }

    /// A spring now ENDS on Compose's clock rather than waiting for its velocity to fall under the
    /// threshold. Measured here: the finish time tracks the estimate, and the gap between the frame before
    /// the end and the exact target it writes is small (the integrator's own tracking error, not a jump).
    /// With only the velocity gate — the rule this replaced — the same spring ran about 0.15 s longer.
    #[test]
    fn a_spring_ends_on_the_estimated_clock() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let spec = SpringSpec {
            stiffness: 400.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            mass: 1.0,
            threshold: 1.0,
        };
        let estimate = estimate_spring_duration_seconds(400.0, 1.0, 0.0, -100.0, 1.0);
        let state = crate::runtime::state::State::new(0.0f32);
        push_animatable(state.clone(), 100.0, AnimationSpec::Spring(spec));
        let mut frames = 0u32;
        let mut before_end = 0.0f32;
        while update_animations() && frames < 600 {
            before_end = state.peek();
            _clock.advance(Duration::from_millis(8));
            frames += 1;
        }
        // Eight milliseconds per frame is the timeline the motion ran on.
        let took = frames as f32 * 0.008;
        assert_eq!(state.peek(), 100.0, "the motion ends exactly on the target");
        assert!(
            took - estimate < 0.08,
            "it ends on the estimate rather than waiting out the velocity gate (took {took:.3}s against an estimate of {estimate:.3}s)"
        );
        let gap = 100.0 - before_end;
        assert!(
            gap < 5.0,
            "the exact target only replaces a value the integrator had already brought to within a few pixels (gap {gap:.3})"
        );
        clear_all_animations();
    }

    /// Compose sequences two motions with a coroutine — `a.animateTo(one); a.animateTo(two)` inside a
    /// `LaunchedEffect` — so the second starts when the first has finished suspending. The queued version
    /// has to do the same: step two starts only after step one ends. Measured against the alternative it
    /// replaces (pushing both targets in turn, where the second simply replaces the first): after the first
    /// leg's 150 ms the value must still be on its way BACK, not already at the second target.
    #[test]
    fn a_sequence_runs_its_steps_one_after_another() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let mut composer = Composer::new();
        let held: std::rc::Rc<std::cell::RefCell<Option<Animatable<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h = held.clone();
        composer.compose(|ctx| {
            let handle = ctx.remember_animatable(0.0f32);
            if h.borrow().is_none() {
                *h.borrow_mut() = Some(handle.clone());
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let a = held.borrow().clone().expect("the slot remembered the animatable");
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;

        a.animate_sequence([
            (100.0, AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(150), linear()))),
            (0.0, AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(150), linear()))),
        ]);

        // Midway through the first leg: up, and nowhere near the second target.
        for _ in 0..10 {
            update_animations();
            _clock.advance(Duration::from_millis(8));
        }
        let rising = a.value();
        assert!(
            rising > 20.0 && rising < 95.0,
            "the first leg is still running ({rising}), so the second has not started"
        );

        // Once both legs are done the value is at the second target and the queue is empty.
        for _ in 0..60 {
            update_animations();
            _clock.advance(Duration::from_millis(8));
        }
        assert_eq!(a.value(), 0.0, "the sequence ended on its last step");
        assert!(!a.is_running());
        clear_all_animations();
    }

    /// `TimedAnimation` is the engine's own evaluation exposed as a function of time, so what it reports has
    /// to match what a running motion shows at the same moment — otherwise a hand-off would jump. Measured
    /// against a real registered animation, sampled every frame, for a spring, a tween and a fling.
    #[test]
    fn a_timed_animation_matches_the_running_one() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        let specs: Vec<(&str, AnimationSpec)> = vec![
            (
                "spring",
                AnimationSpec::Spring(SpringSpec {
                    stiffness: 200.0,
                    ..SpringSpec::default()
                }),
            ),
            (
                "tween",
                AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(300), linear())),
            ),
            ("decay", AnimationSpec::Decay(exponential_decay(4.2))),
        ];
        for (name, spec) in specs {
            clear_all_animations();
            let state = crate::runtime::state::State::new(0.0f32);
            let timed = match spec.clone() {
                AnimationSpec::Decay(decay) => TimedAnimation::fling(0.0f32, 900.0, decay),
                other => TimedAnimation::target(0.0f32, 100.0, other),
            };
            match spec.clone() {
                AnimationSpec::Decay(decay) => push_decay(state.clone(), 900.0, decay),
                other => push_animatable(state.clone(), 100.0, other),
            }
            // Advance FIRST, then update: the running motion's own elapsed time then equals `elapsed`,
            // so the comparison is between two evaluations of the same instant, not one 16 ms behind.
            let mut elapsed = Duration::ZERO;
            let mut worst = 0.0f32;
            // 8 ms frames (120 Hz): the comparison is between the query and a frame-driven motion, and
            // a coarser step would make the running spring's own integrator error the thing measured.
            while elapsed < Duration::from_millis(280) {
                _clock.advance(Duration::from_millis(8));
                elapsed += Duration::from_millis(8);
                update_animations();
                let gap = (state.peek() - timed.value_at(elapsed)).abs();
                worst = worst.max(gap);
            }
            // 5 % of the travel, not a pixel: the RUNNING motion integrates with whatever step its frames
            // give it (a long frame means coarse steps and an error of its own), so the two curves differ by
            // that error rather than by a disagreement about what is being computed. Under a tight loop the
            // measured worst gap is well under this; the deterministic pins are the analytic comparison and
            // the hand-off below.
            assert!(
                worst < 5.0,
                "the timed {name} tracks the running one (worst gap {worst} over 100 units)"
            );
        }
        clear_all_animations();
    }

    /// The query's own accuracy, frame rate aside: for a critically damped spring the exact solution of a
    /// step from rest is `x(t) = D(1 + ωt)e^(-ωt)`, and the query must reproduce it — this is what the
    /// shorter integration step buys, and it is deterministic (no clock involved). The residual is the
    /// integrator's own error at 1/480 s: measured 0.7 px at the fastest point of a 100-unit move.
    #[test]
    fn a_timed_spring_matches_the_analytic_solution() {
        let stiffness = 200.0f32;
        let omega = (stiffness as f64).sqrt();
        let whole = TimedAnimation::target(
            0.0f32,
            100.0,
            AnimationSpec::Spring(SpringSpec {
                stiffness,
                damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
                mass: 1.0,
                threshold: 1.0,
            }),
        );
        for millis in [40u64, 90, 150, 210] {
            let t = Duration::from_millis(millis);
            let seconds = t.as_secs_f64();
            let analytic = 100.0 * (1.0 - (1.0 + omega * seconds) * (-omega * seconds).exp());
            let measured = whole.value_at(t);
            assert!(
                (measured as f64 - analytic).abs() < 1.0,
                "the query reproduces the analytic spring within a pixel at {millis} ms ({measured:.3} against {analytic:.3})"
            );
        }
    }

    /// What the type exists for: freeze a spring mid-flight and continue it from where it was. A linear
    /// spring resumed with the position AND velocity it had keeps the trajectory it would have followed, so
    /// a gesture hand-off does not jump — measured against the uninterrupted motion.
    #[test]
    fn a_mid_flight_hand_off_keeps_the_trajectory() {
        let spec = SpringSpec {
            stiffness: 200.0,
            ..SpringSpec::default()
        };
        let whole = TimedAnimation::target(0.0f32, 100.0, AnimationSpec::Spring(spec.clone()));
        let at = Duration::from_millis(150);
        let frozen_value = whole.value_at(at);
        let frozen_velocity = whole.velocity_at(at);
        let resumed = TimedAnimation::with_velocity(
            frozen_value,
            100.0,
            AnimationSpec::Spring(spec),
            frozen_velocity,
        );
        let after = resumed.value_at(Duration::from_millis(100));
        let reference = whole.value_at(at + Duration::from_millis(100));
        assert!(
            (after - reference).abs() < 0.5,
            "the resumed motion continues the same trajectory (resumed {after:.3}, uninterrupted {reference:.3}, frozen at {frozen_value:.3} moving {frozen_velocity:.1}/s)"
        );
    }

    /// Compose's `RepeatMode.Reverse` mirrors the iteration TIME and evaluates the base curve there, so the
    /// return leg is the same curve running backwards. Interpolating the return leg linearly — what this
    /// engine used to do — is a different motion for any eased base, and a loop builder could not even name one
    /// (`restart_tween`/`restart_keyframes` were the only curve constructors, and both imply Restart).
    #[test]
    fn a_reverse_loop_runs_its_curve_backwards() {
        use crate::animation::interpolator::Interpolator;
        let curve = |t: f32| interpolator::fast_out_slow_in().interpolate(t);
        let tween = DurationSpec::Tween(TweenSpec::new(
            Duration::from_millis(400),
            Arc::new(interpolator::fast_out_slow_in()) as Arc<dyn Interpolator>,
        ));
        let forward = loop_factor(Some(&tween), RepeatMode::Reverse, 0, 0.25);
        let backward = loop_factor(Some(&tween), RepeatMode::Reverse, 1, 0.25);
        assert!(
            (forward - curve(0.25)).abs() < 1e-6,
            "the first leg is the curve itself ({forward} against {})",
            curve(0.25)
        );
        assert!(
            (backward - curve(0.75)).abs() < 1e-6,
            "the return leg evaluates the curve at the mirrored time ({backward} against {})",
            curve(0.75)
        );
        assert!(
            (backward - 0.75).abs() > 0.1,
            "which is not the linear mirror (linear reads 0.75, the curve reads {backward})"
        );

        // Then through the engine: a reverse loop carrying that curve, sampled on its return leg.
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let state = crate::runtime::state::State::new(0.0f32);
        push_infinite(
            state.clone(),
            0.0f32,
            1.0f32,
            InfiniteRepeatableSpec::reverse(Duration::from_millis(400)).with_base(tween),
        );
        let mut elapsed = Duration::ZERO;
        let mut on_the_return_leg = None;
        while elapsed < Duration::from_millis(560) {
            update_animations();
            let t = elapsed.as_secs_f32();
            if (0.48..0.52).contains(&t) {
                on_the_return_leg = Some(state.peek());
            }
            _clock.advance(Duration::from_millis(8));
            elapsed += Duration::from_millis(8);
        }
        let value = on_the_return_leg.expect("the loop was sampled on its return leg");
        assert!(
            value > 0.85,
            "half a second in, the backward leg is already near the loop's start value ({value}; a linear return would read about 0.75)"
        );
        clear_all_animations();
    }

    #[derive(Clone, PartialEq, Debug)]
    enum Leg {
        Idle,
        Busy,
    }

    /// Compose's `transitionSpec = { initial, target -> ... }` chooses a spec from the PAIR of states
    /// (`Transition.kt`'s `TransitionSpec`), so the leg leaving `Idle` and the leg returning to it can
    /// run different curves. Before this, a `Transition` carried one spec and used it for every
    /// animation: this test fails on a single spec whichever one is chosen (a quick one finishes the
    /// slow leg too early, a slow one leaves the quick leg mid-flight).
    #[test]
    fn a_transition_chooses_a_spec_for_each_pair() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        clear_all_animations();
        let linear = || Arc::new(interpolator::Linear::new()) as Arc<dyn interpolator::Interpolator>;
        let slow = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(700), linear()));
        let quick = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(60), linear()));
        let leg = crate::runtime::state::State::new(Leg::Idle);
        let held: std::rc::Rc<std::cell::RefCell<Option<crate::runtime::state::State<f32>>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            let current = leg.get();
            let slow = slow.clone();
            let quick = quick.clone();
            let held = held.clone();
            composer.compose(|ctx| {
                let mut t = ctx.update_transition(current, quick.clone(), "leg");
                let value = t.animate_float_with_spec(
                    ctx,
                    |l| match l {
                        Leg::Idle => 0.0,
                        Leg::Busy => 100.0,
                    },
                    "position",
                    move |from, to| {
                        if *from == Leg::Idle && *to == Leg::Busy {
                            slow.clone()
                        } else {
                            quick.clone()
                        }
                    },
                );
                let _ = value.get();
                if held.borrow().is_none() {
                    *held.borrow_mut() = Some(value);
                }
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut run = |composer: &mut Composer, ms: u64, frame: &mut dyn FnMut(&mut Composer)| {
            let frames = (ms / 16).max(1);
            for _ in 0..frames {
                frame(composer);
                update_animations();
                _clock.advance(Duration::from_millis(16));
            }
        };
        run(&mut composer, 40, &mut frame);
        let value = held.borrow().clone().expect("the animation ran and held its state");
        assert_eq!(value.peek(), 0.0, "the idle state starts where it is");
        leg.set(Leg::Busy);
        run(&mut composer, 260, &mut frame);
        let outbound = value.peek();
        assert!(
            outbound > 15.0 && outbound < 85.0,
            "the Idle -> Busy leg runs the SLOW spec (value {outbound} after ~260 ms of 700)"
        );
        leg.set(Leg::Idle);
        run(&mut composer, 260, &mut frame);
        let back = value.peek();
        assert!(
            back < 0.001,
            "the Busy -> Idle leg runs the QUICK spec and is already home (value {back})"
        );
        clear_all_animations();
    }

