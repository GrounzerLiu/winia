    use super::*;
    use std::sync::Arc;
    use crate::runtime::composer::Composer;

    /// Repeated animation cycles: push targets and step updates repeatedly, converging to the latest target.
    /// Verifies the engine stays correct under repeated triggers (click loops), with no double driving.
    #[test]
    fn repeated_animation_cycles_converge() {
        let (_guard, _clock) = super::tests::serial_clock();
        let state = crate::runtime::state::State::new(40.0f32);

        // Each round: push a target, advance past the duration, run one update with the real dt, then check.
        for (i, target) in [(1usize, 200.0f32), (2, 40.0), (3, 200.0), (4, 40.0)] {
            push_animatable(state.clone(), target, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(300), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
            }));
            _clock.advance(std::time::Duration::from_millis(400));
            update_animations();
            let v = state.peek();
            eprintln!("[repeat-anim] round {} target={} val={:.1}", i, target, v);
            assert!((v - target).abs() < 1.0,
                "repeated animation round {} should converge to {}, got {:.1}", i, target, v);
        }
    }

    /// Pushing again before the animation finishes (retargeting mid-flight) switches to the new target.
    #[test]
    fn mid_flight_retarget_switches() {
        let (_guard, _clock) = super::tests::serial_clock();
        let state = crate::runtime::state::State::new(40.0f32);
        push_animatable(state.clone(), 200.0, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(1000), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
        }));
        // Retarget to 40 mid-flight (after 10 frames); this should switch over.
        _clock.advance(std::time::Duration::from_millis(50));
        update_animations();
        let mid = state.peek();
        assert!(mid > 40.0 && mid < 200.0, "should be mid-animation, got {}", mid);
        push_animatable(state.clone(), 90.0, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(200), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
        }));
        _clock.advance(std::time::Duration::from_millis(300));
        update_animations();
        // Completion can be off by one frame (dt offset); advance and update once more to settle.
        _clock.advance(std::time::Duration::from_millis(250));
        update_animations();
        let v = state.peek();
        eprintln!("[retarget] mid={:.1} final={:.1}", mid, v);
        assert!((v - 90.0).abs() < 1.0, "mid-flight retarget should converge to 90, got {:.1}", v);
    }

    /// i32 interpolation (the basis of animate_int_as_state): rounding + spring support.
    #[test]
    fn int_value_lerp_rounds() {
        let (_guard, _clock) = super::tests::serial_clock();
        use crate::animation::AnimatableValue;
        assert_eq!(<i32 as AnimatableValue>::lerp(&0, &10, 0.5), 5);
        assert_eq!(<i32 as AnimatableValue>::lerp(&0, &10, 0.51), 5);
        assert_eq!(<i32 as AnimatableValue>::lerp(&0, &10, 0.55), 6);
        assert_eq!(<i32 as AnimatableValue>::from_f32(3.7), 4);
        assert!(<i32 as AnimatableValue>::supports_spring(), "i32 scalars should support Spring");
        // Animation convergence: 0 -> 100 (Tween).
        let state = crate::runtime::state::State::new(0i32);
        push_animatable(state.clone(), 100, AnimationSpec::Tween(TweenSpec { duration: std::time::Duration::from_millis(200), delay: Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::Linear::new()),
        }));
        _clock.advance(std::time::Duration::from_millis(300));
        update_animations();
        assert_eq!(state.peek(), 100, "i32 animation should converge to 100");
    }

    /// Generic Transition::animate (the core of animate_value): a Color value is
    /// mapped through target_fn and driven to convergence by push_animatable.
    #[test]
    fn transition_animate_generic_value() {
        let (_guard, _clock) = super::tests::serial_clock();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut captured = None;
        composer.compose(|ctx| {
            let mut t = ctx.update_transition(3.0f32, AnimationSpec::Tween(TweenSpec::default()), "t");
            let s = t.animate(ctx, |v| *v * 2.0, "v");
            captured = Some(s);
        });
        assert_eq!(captured.unwrap().peek(), 6.0, "target_fn mapping applies immediately (same value skips)");
    }

    #[test]
    fn transition_animate_value_named_wrapper() {
        // animate_value is the Compose-named twin of the generic animate<U>.
        let (_guard, _clock) = super::tests::serial_clock();
        crate::animation::clear_all_animations();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut captured = None;
        composer.compose(|ctx| {
            let mut t = ctx.update_transition(10i32, AnimationSpec::Tween(TweenSpec::default()), "t");
            let s = t.animate_value(ctx, |v| *v + 5, "v");
            captured = Some(s);
        });
        assert_eq!(captured.unwrap().peek(), 15, "animate_value maps target_fn immediately");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn transition_child_follows_parent_target() {
        // create_child_transition: child target derives from parent target on every
        // call — flip the parent, the child re-maps and animates to the new value.
        let (_guard, _clock) = super::tests::serial_clock();
        crate::animation::clear_all_animations();
        let mut composer = crate::runtime::composer::Composer::new();
        let parent = crate::runtime::state::State::new(1i32);
        let child_val = std::cell::RefCell::new(None);
        let build = |composer: &mut crate::runtime::composer::Composer,
                     child_val: &std::cell::RefCell<
            Option<crate::runtime::state::State<f32>>,
        >| {
            let p = parent.clone();
            composer.compose(|ctx| {
                let t = ctx.update_transition(p.get(), AnimationSpec::Tween(TweenSpec::default()), "p");
                let child = t.create_child_transition(|v| *v * 100);
                let mut child = child;
                let s = child.animate_float(ctx, |v| *v as f32, "c");
                *child_val.borrow_mut() = Some(s.clone());
            });
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer, &child_val);
        assert_eq!(child_val.borrow().clone().unwrap().peek(), 100.0, "child maps parent 1 → 100");
        // Flip parent 1 → 2: child target becomes 200 and animates there.
        parent.set(2);
        build(&mut composer, &child_val);
        for _ in 0..10 {
            crate::animation::update_animations();
            _clock.advance(std::time::Duration::from_millis(50));
            build(&mut composer, &child_val);
        }
        let v = child_val.borrow().clone().unwrap().peek();
        assert!(
            (v - 200.0).abs() < 5.0,
            "child should converge 100 → 200 after parent flip, got {v}"
        );
        crate::animation::clear_all_animations();
    }
