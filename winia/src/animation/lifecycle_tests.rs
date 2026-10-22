use super::*;

/// The imperative handle re-targets; the per-frame path keeps deduping.
///
/// `animate*AsState` calls its push on every recomposition, so a same-target motion there must be
/// left alone (otherwise nothing would ever finish), and that guard used to swallow a deliberate
/// re-target too: `Animatable::animate_to(same_value, new_spec)` was silently ignored. Compose's
/// `animateTo` always runs what it is given.
#[test]
fn the_imperative_handle_retargets_while_the_per_frame_path_dedups() {
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    clear_all_animations();

    let state: State<f32> = State::new(0.0);
    let sid = state.state_id();
    let anim = Animatable {
        state: state.clone(),
        bounds: crate::runtime::state::Backchannel::new(None),
    };
    let fast = || AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(5), interpolator::Linear::new()));
    // Only the lines that describe THIS state: another test's motion (the registries are global) must
    // neither satisfy these assertions nor make them fail.
    let mine = |want: &str| -> bool {
        let needle = format!("state={sid:?}");
        active_animations_report()
            .into_iter()
            .any(|l| l.contains(&needle) && l.contains(want))
    };

    // A soft spring: the test needs it still running after a few frames, with ~2 s of margin.
    anim.animate_to(100.0, SpringSpec { stiffness: 20.0, ..SpringSpec::default() });
    for _ in 0..3 {
        update_animations();
        _clock.advance(Duration::from_millis(2));
    }
    assert!(has_animation_for_state(sid), "the spring is running");
    assert!(mine("spring("), "a spring is registered before the re-target");

    // Per-frame path (what `animate*AsState` calls): same target, new spec - ignored on purpose.
    push_animatable(state.clone(), 100.0, fast());
    assert!(
        mine("spring("),
        "the per-frame path must NOT restart a motion that is already heading there"
    );

    // Imperative path: same target, new spec - the new motion runs.
    anim.animate_to(100.0, fast());
    assert!(
        mine("tween("),
        "animate_to must adopt the new spec, got {:?}",
        active_animations_report()
    );

    for _ in 0..200 {
        update_animations();
        _clock.advance(Duration::from_millis(1));
        if !has_animation_for_state(sid) {
            break;
        }
    }
    assert!(
        (state.peek() - 100.0).abs() < 0.01,
        "the re-targeted motion reaches the same value, got {}",
        state.peek()
    );
    clear_all_animations();
}

/// A real `#[composable]` branch for the slot-ownership test: the macro injects
/// `start_scope`/`end_scope` around the body, which is what makes the slots (and their forget
/// callbacks) belong to the call site instead of the parent slot.
#[crate::composable]
fn slot_owned_branch(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    target: f32,
    sid: std::rc::Rc<std::cell::RefCell<Option<StateId>>>,
) {
    let value = ctx.animate_float_as_state(target, AnimationSpec::Spring(SpringSpec::default()));
    *sid.borrow_mut() = Some(value.state_id());
}

/// A slot-owned motion dies with its slot. `animate_float_as_state` remembers a state in the slot it
/// was called from; when the branch that composed it goes away the engine must stop ticking it
/// (Compose's `DisposableEffect` equivalent), instead of writing to a state nobody reads until the
/// whole Composer is dropped.
#[test]
fn a_slot_owned_motion_dies_with_its_slot() {
    use crate::runtime::composer::Composer;
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    clear_all_animations();

    let show = State::new(true);
    let target = State::new(1.0f32);
    let sid = std::rc::Rc::new(std::cell::RefCell::new(None::<StateId>));
    let mut composer = Composer::new();

    let frame = |composer: &mut Composer| {
        let show = show.clone();
        let target = target.clone();
        let sid = sid.clone();
        composer.compose(move |ctx| {
            if show.get() {
                // A composable call, not a bare `if`: the macro's scope is what makes these slots
                // (and their forget callbacks) belong to the branch instead of the parent slot.
                slot_owned_branch(ctx, target.get(), sid.clone());
            }
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));
    };

    frame(&mut composer); // the state is created ON its target: nothing to move yet
    let state_id = sid.borrow().expect("the branch composed");
    assert!(
        !has_animation_for_state(state_id),
        "a value that starts on its target has no motion"
    );

    target.set(0.0);
    frame(&mut composer); // now a spring runs
    assert!(
        has_animation_for_state(state_id),
        "the spring registered for the slot's state"
    );

    show.set(false);
    frame(&mut composer); // the branch is gone
    assert!(
        !has_animation_for_state(state_id),
        "the slot was forgotten, so its motion must be removed from the engine"
    );
    clear_all_animations();
}

/// T5: an infinite-animation scope is disposed automatically when its composition point goes
/// away, so the global animation table is emptied and `update_animations()` no longer spins.
#[test]
fn test_infinite_transition_auto_dispose() {
    // The global animation table is shared, hence the serial lock (parallel clears would wipe it).
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    use crate::runtime::composer::{Composer, GroupStatus};
    use crate::layout::constraints::Constraints;
    use crate::layout::BoxLayout;
    use crate::modifier::Modifier;
    use crate::runtime::state::State;
    use std::time::Duration;

    // Empty the global animation table (isolation from parallel tests).
    ACTIVE_ANIMATIONS.lock().unwrap().clear();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().clear();
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);
    // Remember our own animation's state_id: assert only on our own state.
    let sid_holder = std::cell::RefCell::new(None::<StateId>);

    let build = |composer: &mut Composer,
                 holder: &std::cell::RefCell<Option<State<bool>>>,
                 sid_holder: &std::cell::RefCell<Option<StateId>>| {
        composer.compose(|ctx| {
            let show = ctx.remember(|| true);
            *holder.borrow_mut() = Some(show.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if show.get() {
                        // Infinite-animation scope (it registers on_remove to dispose itself).
                        let mut inf = ctx.remember_infinite_transition();
                        let s = inf.animate_float(
                            ctx,
                            0.0,
                            1.0,
                            InfiniteRepeatableSpec::restart(Duration::from_millis(100)),
                        );
                        *sid_holder.borrow_mut() = Some(s.state_id());
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    // Frame 1: the animation is registered and live.
    build(&mut composer, &holder, &sid_holder);
    let sid = sid_holder.borrow().unwrap();
    assert!(has_animation_for_state(sid), "frame 1 should have a live infinite animation");

    // Frame 2: show=false removes the composition point, on_remove disposes, the table empties.
    holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer, &holder, &sid_holder);
    assert!(!has_animation_for_state(sid), "the infinite animation is disposed when its composition point goes away");
}

/// T6: an explicit dispose plus the on_remove firing is idempotent: no panic, table unchanged.
#[test]
fn test_infinite_transition_manual_dispose_idempotent() {
    // The global animation table is shared, hence the serial lock (parallel clears would wipe it).
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    use crate::runtime::composer::{Composer, GroupStatus};
    use crate::layout::constraints::Constraints;
    use crate::layout::BoxLayout;
    use crate::modifier::Modifier;
    use crate::runtime::state::State;
    use std::time::Duration;

    // Empty the global animation table (isolation from parallel tests).
    ACTIVE_ANIMATIONS.lock().unwrap().clear();
    ACTIVE_COLOR_ANIMATIONS.lock().unwrap().clear();
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);
    // Keep a reference to the InfiniteTransition (the user holds one; explicit-dispose path).
    let inf_holder = std::cell::RefCell::new(None::<InfiniteTransition>);

    let build = |composer: &mut Composer,
                 holder: &std::cell::RefCell<Option<State<bool>>>,
                 inf_holder: &std::cell::RefCell<Option<InfiniteTransition>>| {
        composer.compose(|ctx| {
            let show = ctx.remember(|| true);
            *holder.borrow_mut() = Some(show.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if show.get() {
                        let mut inf = ctx.remember_infinite_transition();
                        let _s = inf.animate_float(
                            ctx,
                            0.0,
                            1.0,
                            InfiniteRepeatableSpec::restart(Duration::from_millis(100)),
                        );
                        *inf_holder.borrow_mut() = Some(inf);
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    // Frame 1: register.
    build(&mut composer, &holder, &inf_holder);
    let sid = inf_holder.borrow().as_ref().unwrap().ids.lock().unwrap()[0];
    assert!(has_animation_for_state(sid), "frame 1 should have an infinite animation");

    // Explicit dispose (the user path) empties the table.
    inf_holder.borrow().as_ref().unwrap().dispose();
    assert!(!has_animation_for_state(sid), "the table should be empty after an explicit dispose");

    // The composition point goes away, so on_remove fires again (ids are drained: idempotent).
    holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer, &holder, &inf_holder);
    assert!(!has_animation_for_state(sid), "the table is still empty after both triggers (idempotent)");
}

    /// Wiring test for the table-driven interpolators (v1's 29 curves plus `Linear`).
    #[test]
    fn tween_uses_table_interpolator() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        use crate::animation::interpolator::{EaseInQuad, Interpolator, Linear};
        // 1. The default is Compose's `tween()` easing, `FastOutSlowIn` — not a linear ramp. Its
        //    midpoint sits at 0.776 (see `interpolator::tests` for the solved reference values).
        let t = TweenSpec::default();
        assert!(
            (t.interpolator.interpolate(0.5) - 0.7756).abs() < 0.01,
            "the default easing is FastOutSlowIn, midpoint = {}",
            t.interpolator.interpolate(0.5)
        );
        // 1b. An explicitly linear ramp still is one — `TweenSpec::new(dur, Linear::new())`.
        let linear = TweenSpec::new(Duration::from_millis(300), Linear::new());
        assert_eq!(linear.interpolator.interpolate(0.5), 0.5, "an explicit Linear midpoint = 0.5");
        // 2. A table-driven curve takes effect (EaseInQuad's midpoint is below 0.5 — it eases in).
        let ease = Arc::new(EaseInQuad::new());
        assert!(ease.interpolate(0.5) < 0.5, "EaseInQuad's midpoint must be below 0.5");
        assert!(ease.interpolate(0.0) == 0.0 && ease.interpolate(1.0) == 1.0, "the ends are clamped");
        // 3. Engine wiring: an Animatable animated by a table interpolator. After 60 ms (t ≈ 0.2)
        //    EaseInQuad(0.2) = 0.04 → about 4, while linear(0.2) = 0.2 → about 20: five times apart,
        //    so a ±10 ms jitter in the sleep cannot blur the two.
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Tween(TweenSpec { duration: Duration::from_millis(300), delay: Duration::ZERO, interpolator: Arc::new(EaseInQuad::new()),
            }),
        );
        _clock.advance(Duration::from_millis(60));
        anim.update();
        let v = st.peek();
        assert!(
            v > 0.0 && v < 10.0,
            "EaseInQuad after 60 ms must be far below linear (got {}, linear is about 20)",
            v
        );
        // Control: the same 300 ms tween through its own declared interpolator reaches about 20.
        let st2 = State::new(0.0f32);
        let mut anim2 = Motion::new(st2.clone());
        anim2.animate_to(
            100.0,
            AnimationSpec::Tween(TweenSpec::new(
                Duration::from_millis(300),
                crate::animation::interpolator::Linear::new(),
            )),
        );
        _clock.advance(Duration::from_millis(60));
        anim2.update();
        assert!((st2.peek() - 20.0).abs() < 5.0, "linear after 60 ms should be about 20 (got {})", st2.peek());
    }

    /// Regression: after an overshooting interpolator (Elastic/Back) finishes, the animation must
    /// settle exactly on the target. Previously completion used eased >= 1.0, so the overshoot ended
    /// the animation early at an out-of-range value.
    #[test]
    fn tween_overshoot_interpolator_settles_at_target() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        use crate::animation::interpolator::{EaseOutElastic, Interpolator};
        // The curve itself already has eased > 1.0 at t ~ 0.98 (the overshoot).
        let ease = EaseOutElastic::new();
        assert!(ease.interpolate(0.98) > 1.0, "EaseOutElastic should overshoot (> 1.0) near the end");
        // Advance the full duration: it must settle exactly on 100 (before the fix it stopped at the overshoot).
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        anim.animate_to(
            100.0,
            AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(200), EaseOutElastic::new())),
        );
        let mut frames = 0;
        while anim.update() && frames < 30 {
            _clock.advance(Duration::from_millis(40));
            frames += 1;
        }
        assert!(frames < 30, "the animation should converge");
        assert_eq!(st.peek(), 100.0, "an overshooting interpolator must settle exactly on the target");
    }

    /// The Keyframes done frame must settle exactly on the target too, like Tween and Spring: with a
    /// final progress < 1.0 the interpolated value is not `to`.
    #[test]
    fn keyframes_settles_at_target() {
        let (_guard, _clock) = crate::animation::tests::serial_clock();
        let st = State::new(0.0f32);
        let mut anim = Motion::new(st.clone());
        // The last frame has progress 0.8 (< 1.0): using the interpolated value at done would not equal to.
        anim.animate_to(
            100.0,
            AnimationSpec::Keyframes(KeyframesSpec {
                duration: Duration::from_millis(200),
                frames: vec![
                    (0.0, 0.0, Arc::new(crate::animation::interpolator::Linear::new())),
                    (0.8, 50.0, Arc::new(crate::animation::interpolator::Linear::new())),
                ],
            }),
        );
        let mut frames = 0;
        while anim.update() && frames < 20 {
            _clock.advance(Duration::from_millis(40));
            frames += 1;
        }
        assert!(frames < 20, "keyframes should converge");
        assert_eq!(st.peek(), 100.0, "keyframes must settle exactly on the target even when the last progress < 1.0");
    }

/// With a frozen clock the engine's trajectory is a function of time, so a test can assert WHERE the
/// value is at each timestamp instead of sleeping and sampling whenever the machine wakes up.
#[test]
fn a_spring_follows_the_clock_frame_by_frame() {
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    clear_all_animations();
    let st = State::new(0.0f32);
    push_animatable(
        st.clone(),
        100.0,
        AnimationSpec::Spring(SpringSpec {
            stiffness: 400.0,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            threshold: 0.1,
            mass: 1.0,
        }),
    );

    // 8 ms frames. A critically damped spring approaches from one side, so the samples must be
    // monotone; the first frame has to move, and the motion has to have ENDED inside the window.
    let mut samples = Vec::new();
    let mut alive = true;
    for _ in 0..200 {
        _clock.advance(Duration::from_millis(8));
        alive = update_animations();
        samples.push(st.peek());
        if !alive {
            break;
        }
    }
    assert!(!alive, "the spring finished inside the 1.6 s window");
    assert!(
        samples[0] > 1.0 && samples[0] < 100.0,
        "the first frame moved without arriving (at {})",
        samples[0]
    );
    for pair in samples.windows(2) {
        assert!(
            pair[1] >= pair[0] - 1e-3,
            "a critically damped spring does not go back: {:?}",
            pair
        );
    }
    assert_eq!(st.peek(), 100.0, "and it lands exactly on the target");
    clear_all_animations();
}

/// A motion that finishes on its first update is not left in the registry: its callbacks already ran,
/// and an entry would keep `is_animating()` / `has_animation_for_state` true for a frame.
#[test]
fn a_motion_that_finishes_at_once_is_not_registered() {
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    clear_all_animations();
    let st = State::new(0.0f32);
    push_animatable(st.clone(), 1.0, AnimationSpec::Snap(SnapSpec::default()));
    assert_eq!(st.peek(), 1.0, "the jump happened during the push");
    assert!(
        !has_animation_for_state(st.state_id()),
        "nothing is left in the registry"
    );
    assert!(!is_animating(), "and the engine is idle");
}

/// The 5 s guard must not write past a value bound: a bounded motion that overruns ends AT the bound
/// with `BoundReached`, not forced onto a target outside the range.
#[test]
fn the_overrun_guard_honours_a_value_bound() {
    let (_guard, _clock) = crate::animation::tests::serial_clock();
    clear_all_animations();
    let st = State::new(0.0f32);
    let ends = std::sync::Arc::new(std::sync::Mutex::new(Vec::<MotionEnd>::new()));
    let rec = ends.clone();
    // Slow enough that the value is still inside the bound when the guard fires (~5.9/ω s in, the
    // displacement is under 2 % of the travel), with a threshold that puts the estimated clock at
    // ~15 s — well past the guard.
    push_animatable_with_result(
        st.clone(),
        100.0,
        AnimationSpec::Spring(SpringSpec {
            stiffness: 1.5,
            damping_ratio: SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            mass: 1.0,
            threshold: 0.0001,
        }),
        move |end| rec.lock().unwrap().push(end),
    );
    set_animation_bounds(st.state_id(), Some((0.0, 10.0)));

    for _ in 0..220 {
        update_animations();
        _clock.advance(Duration::from_millis(30));
    }

    let ends = ends.lock().unwrap();
    assert_eq!(ends.len(), 1, "the motion ended once");
    assert_eq!(
        ends[0].reason,
        MotionEndReason::BoundReached,
        "the bound ends it, not the guard"
    );
    assert_eq!(
        st.peek(),
        10.0,
        "and the value stops at the bound instead of the target outside it"
    );
    clear_all_animations();
}

