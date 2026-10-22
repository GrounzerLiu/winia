    use super::*;
    use crate::runtime::composer::Composer;
    use crate::runtime::state::State;
    use crate::layout::constraints::Constraints;
    use crate::layout::components::Column;

    /// 集成：visible 切换 → 内容进入/退出组合树（exit 完成才移除）。
    /// 通过多次 compose + 手动推进动画模拟可见性生命周期。
    #[test]
    fn visibility_toggle_enters_and_exits() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v1 = visible.clone();
        let mut present = Vec::new();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedVisibility::new(v1.clone())
                    .build(ctx, |ctx| {
                        TextLeaf::new().build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // Advance animation frames (real time drives them). The default spec is Compose's
        // spring(1.0, 400), whose tail is longer than the 300 ms tween this test used to budget for, so
        // the loop runs 20 frames × 60 ms ≈ 1.2 s.
        let mut advance = |composer: &mut Composer| {
            for _ in 0..20 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                recompose(composer);
            }
        };

        // 首帧：visible=false → 内容不在组合
        recompose(&mut composer);
        present.push(count_text(&composer));

        // visible=true → 内容进入组合（enter 动画 0→1）
        visible.set(true);
        recompose(&mut composer);
        present.push(count_text(&composer));
        advance(&mut composer); // progress → 1.0

        // visible=false → exit 动画中：内容保留（progress 1→0 途中）
        visible.set(false);
        recompose(&mut composer);
        present.push(count_text(&composer));

        // 推进到 exit 完成（progress → 0 → removed 标记 → 下帧移除）
        advance(&mut composer);
        present.push(count_text(&composer));

        assert_eq!(present[0], 0, "初始不可见：内容不在组合");
        assert!(present[1] > 0, "visible=true：内容进入组合");
        assert!(present[2] > 0, "exit 动画中：内容保留");
        assert_eq!(present[3], 0, "exit 完成：内容移除");
    }

    /// Compose never runs a transition for its INITIAL state: `AnimatedVisibility(visible = true)` on the
    /// first composition shows its content at full size with no enter animation (the rule
    /// `MutableTransitionState`'s initial `targetState` follows too — the transition's first value is its
    /// target, not an animation away from it). winia started `progress` at 0 and pushed the enter, so the
    /// content wiped in from nothing on the frame it first appeared.
    #[test]
    fn an_initially_visible_content_does_not_play_an_enter() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(true);
        let v = visible.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .enter(VisibilityTransition::expand_in(
                            crate::animation::TweenSpec::default(),
                        ))
                        .build(ctx, |ctx| {
                            TextLeaf::new().build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        let root = composer.layout_root_idx().expect("root");
        let av = composer.arena_nodes()[root].children[0];
        let size = composer.arena_nodes()[av].measured_size;
        assert_eq!(
            (size.width, size.height),
            (100.0, 50.0),
            "the first layout is already the full content size, not a 0-progress container"
        );
        assert!(
            !crate::animation::update_animations(),
            "no enter animation may be running on the first composition"
        );
    }

    /// Compose's `MutableTransitionState(false)` with `targetState = true` animates content in as it
    /// FIRST appears (`Transition.kt:796-800` points at `InitialStateSample`); a plain `State<bool>` that
    /// is already true does not (the test above). `animate_on_appear()` is how a winia caller asks for
    /// that initial state.
    #[test]
    fn animate_on_appear_plays_the_enter_on_the_first_composition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(true);
        let v = visible.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .animate_on_appear()
                        .enter(VisibilityTransition::expand_in(
                            crate::animation::TweenSpec::default(),
                        ))
                        .build(ctx, |ctx| {
                            TextLeaf::new().build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let height = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let av = composer.arena_nodes()[root].children[0];
            composer.arena_nodes()[av].measured_size.height
        };
        build(&mut composer);
        let start = height(&composer);
        assert!(
            start < 50.0,
            "the opted-in enter starts collapsed on the first composition (got {start})"
        );
        for _ in 0..40 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            build(&mut composer);
        }
        assert_eq!(height(&composer), 50.0, "and it reaches the content's own height");
    }

    /// Per-channel specs: Compose's `fadeIn(tween) + expandIn(spring)` runs the two channels on their
    /// own clocks, because every channel is its own animatable carrying its own spec
    /// (`EnterExitTransition.kt`'s `Fade`, `ChangeSize`, `Slide` and `Scale` each take one). The
    /// discriminator is a RATIO, so no timing budget can mask it: with a fast size channel and a slow
    /// fade, the container must be nearly full (`height > 30`) while the fade is still below half
    /// (`alpha < 0.5`). A single shared clock drives both from one progress, so the height would be
    /// `50 × alpha ≤ 25` — the predicate could not hold.
    #[test]
    fn a_slow_fade_runs_out_of_step_with_the_expand() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        // The size channel is a fast spring; the fade takes 600 ms.
        let slow_fade = crate::animation::TweenSpec::new(
            std::time::Duration::from_millis(600),
            crate::animation::interpolator::Linear::new(),
        );
        let quick_size = crate::animation::SpringSpec {
            stiffness: 1400.0,
            ..crate::animation::SpringSpec::default()
        };
        let enter = VisibilityTransition::fade_in(slow_fade)
            + VisibilityTransition::expand_in(quick_size);
        let held: std::rc::Rc<std::cell::RefCell<Option<VisibilityScope>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h2 = held.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    let h2 = h2.clone();
                    AnimatedVisibility::new(v.clone())
                        .enter(enter.clone())
                        .build_with_scope(ctx, move |scope, ctx| {
                            // Hold the scope: its channel States are live, so the loop can peek them in
                            // the SAME frame it measures the container in (the content closure itself
                            // only runs when the container Enters).
                            if h2.borrow().is_none() {
                                *h2.borrow_mut() = Some(scope.clone());
                            }
                            TextLeaf::new().build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let height = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let av = composer.arena_nodes()[root].children[0];
            composer.arena_nodes()[av].measured_size.height
        };
        build(&mut composer); // hidden
        visible.set(true);
        let mut out_of_step = false;
        for _ in 0..60 {
            build(&mut composer);
            let h = height(&composer);
            let alpha_p = held
                .borrow()
                .as_ref()
                .map(|s| s.alpha.peek())
                .expect("the content ran and held the scope");
            if h > 30.0 && alpha_p < 0.5 {
                out_of_step = true;
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let alpha_p = held.borrow().as_ref().map(|s| s.alpha.peek()).unwrap_or(-1.0);
        assert!(
            out_of_step,
            "the size channel must be nearly full while the slow fade is still below half — one shared \
             clock would pin the height to 50 × alpha (last alpha {alpha_p}, height {})",
            height(&composer)
        );
        assert_eq!(height(&composer), 50.0, "and the container still lands on the content's height");
    }

    /// Compose's `EnterTransition.None` + `ExitTransition.None`: a transition that enables nothing must
    /// switch instantly. winia used to drive all four channels regardless, so the invisible ones ran on
    /// the shared spec and held the removal open — measured before the fix: the content appeared at once
    /// but the container stayed ~320 ms (16 frames of 20 ms) because `still_animating` was true for a
    /// channel nobody had enabled.
    #[test]
    fn a_transition_with_nothing_enabled_removes_the_content_at_once() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let empty = VisibilityTransition::empty();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .enter(empty.clone())
                        .exit(empty.clone())
                        .build(ctx, |ctx| {
                            TextLeaf::new().build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let nodes = |composer: &Composer| node_count(composer);
        build(&mut composer);
        let hidden = nodes(&composer);
        visible.set(true);
        build(&mut composer);
        assert!(
            nodes(&composer) > hidden,
            "an empty enter shows the content at once (was {hidden}, now {})",
            nodes(&composer)
        );
        visible.set(false);
        build(&mut composer);
        assert_eq!(
            nodes(&composer),
            hidden,
            "an empty exit removes the content on the next build, not after an invisible animation"
        );
        assert!(
            !crate::animation::update_animations(),
            "and nothing was left animating"
        );
    }

    /// Only the channels the transition enables are driven. Compose builds `TransitionData(fade = …,
    /// changeSize = …)` and leaves the rest `null`, so no animation exists for them; winia used to push
    /// every channel, which registered four animations for a two-channel transition and made the
    /// disabled channels track the enabled ones (measured: `(size, alpha, slide, scale)` all at 1.67e-7
    /// on the first enter frame of a `fade_in + expand_in` pair).
    #[test]
    fn a_disabled_channel_is_not_driven() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let enter = VisibilityTransition::fade_in(crate::animation::TweenSpec::new(
            std::time::Duration::from_millis(400),
            crate::animation::interpolator::Linear::new(),
        )) + VisibilityTransition::expand_in(crate::animation::SpringSpec {
            stiffness: 1400.0,
            ..crate::animation::SpringSpec::default()
        });
        let held: std::rc::Rc<std::cell::RefCell<Option<VisibilityScope>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let h2 = held.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    let h2 = h2.clone();
                    AnimatedVisibility::new(v.clone())
                        .enter(enter.clone())
                        .build_with_scope(ctx, move |scope, ctx| {
                            if h2.borrow().is_none() {
                                *h2.borrow_mut() = Some(scope.clone());
                            }
                            TextLeaf::new().build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        visible.set(true);
        let mut seen = false;
        let mut checked_the_start = false;
        for _ in 0..12 {
            build(&mut composer);
            if let Some(scope) = held.borrow().as_ref() {
                seen = true;
                let animating =
                    |s: &State<f32>| crate::animation::has_animation_for_state(s.state_id());
                // The enabled channels are checked as the enter STARTS: a stiffness-1400 spring settles in
                // about 0.14 s (Compose's own duration estimate), so it has legitimately left the registry
                // long before this loop's 240 ms are up. Which channels are driven is the question — not
                // how long a spring lasts.
                if !checked_the_start {
                    checked_the_start = true;
                    assert!(
                        animating(&scope.progress) && animating(&scope.alpha),
                        "the enabled size and fade channels are driven"
                    );
                }
                assert!(
                    !animating(&scope.slide) && !animating(&scope.scale),
                    "a channel no builder enabled must not be driven, at any point"
                );
                assert_eq!(
                    (scope.slide.peek(), scope.scale.peek()),
                    (0.0, 0.0),
                    "and its value stays where the initial state put it"
                );
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen, "the content ran and held the scope");
    }

    /// A transition with no SIZE channel — `slide_in`/`slide_out` and `scale`, which is panel B of the
    /// demo — leaves the layout alone: the container is the content's own size from the frame the content
    /// appears, so the panels below it jump by that height instead of following an animation, and the
    /// jump happens again (upward) when the exit finishes and the node is removed. That is Compose's
    /// behaviour too, and its KDoc says both halves: only a size-changing enter/exit animates the parent
    /// and siblings ("The expanding and shrinking will likely also animate the parent and siblings in the
    /// row since they rely on the size of appearing/disappearing content", `AnimatedVisibility.kt:157-158`)
    /// and "Once the exit transition is finished, the [content] composable will be removed from the tree,
    /// and disposed" (`:172-173`). Compose's own defaults therefore pair the fade with a size change, and
    /// the demo's panel C is the contrast.
    #[test]
    fn a_slide_only_transition_keeps_the_container_size_and_the_space_collapses_at_removal() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .enter(VisibilityTransition::slide_in(
                            SlideDirection::Right,
                            crate::animation::TweenSpec::default(),
                        ))
                        .exit(VisibilityTransition::slide_out(
                            SlideDirection::Right,
                            crate::animation::TweenSpec::default(),
                        ))
                        .build(ctx, |ctx| {
                            LeafBox::new(80.0, 40.0).build(ctx);
                        });
                    // The witness: the panel below the animated one.
                    LeafBox::new(20.0, 20.0).build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let below_y = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let below = *nodes[root].children.last().expect("the witness leaf");
            nodes[below].position.y
        };
        build(&mut composer);
        let hidden_y = below_y(&composer);
        visible.set(true);
        build(&mut composer);
        // The content appears at its FULL height on the first frame of a slide-only enter: the witness
        // has already moved down by 40, even though the content is still off to the right.
        assert_eq!(
            below_y(&composer) - hidden_y,
            40.0,
            "a slide-only enter moves the content below at once (no size channel to animate)"
        );
        for _ in 0..40 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(15));
            build(&mut composer);
        }
        assert_eq!(below_y(&composer) - hidden_y, 40.0, "and it stays there while the content is shown");
        visible.set(false);
        // While the exit plays the space is still taken.
        build(&mut composer);
        assert_eq!(
            below_y(&composer) - hidden_y,
            40.0,
            "the container keeps its size for the whole exit"
        );
        for _ in 0..60 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(15));
            build(&mut composer);
        }
        // Removal collapses the space in one frame — the jump the demo shows.
        let collapsed = below_y(&composer) - hidden_y;
        assert!(
            collapsed.abs() < 0.001,
            "and the space collapses only when the content is removed (still {collapsed} above)"
        );
    }

    /// The content can ride the same clock the container does — Compose's `AnimatedVisibilityScope`,
    /// whose `transition: Transition<EnterExitState>` exists for exactly this ("Custom enter/exit
    /// animations can be created using the `Transition<EnterExitState>` object from the
    /// `AnimatedVisibilityScope`", `AnimatedVisibility.kt:160-163`). winia hands the progress itself
    /// through `build_with_scope`; reading it must track the container's own size, which is
    /// `content × progress` while a size-changing transition plays.
    #[test]
    fn the_content_can_read_the_transition_progress() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let sampled = std::rc::Rc::new(std::cell::Cell::new(-1.0f32));
        let s = sampled.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    let s = s.clone();
                    AnimatedVisibility::new(v.clone()).build_with_scope(
                        ctx,
                        move |scope, ctx| {
                            s.set(scope.value());
                            TextLeaf::new().build(ctx);
                        },
                    );
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer); // hidden: no container, no content
        visible.set(true);
        let mut mid_flight = false;
        for _ in 0..40 {
            build(&mut composer);
            let p = sampled.get();
            if p > 0.05 && p < 0.95 {
                mid_flight = true;
                let root = composer.layout_root_idx().expect("root");
                let av = composer.arena_nodes()[root].children[0];
                let h = composer.arena_nodes()[av].measured_size.height;
                assert!(
                    (h - 50.0 * p).abs() < 0.5,
                    "the container is sized by the same progress the content reads: h={h} p={p}"
                );
            }
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(mid_flight, "the content observed a mid-flight progress");
    }

    /// A plain `AnimatedVisibility::new(v)` (no `.enter`/`.exit`) must fade AND change the container's
    /// size: every Compose overload's default pair is `fadeIn() + expand*`/`fadeOut() + shrink*`
    /// (`AnimatedVisibility.kt:125-129` top level, `:199-203` RowScope, `:272-276` ColumnScope). winia's
    /// default faded only, so the container kept the full content height for the whole transition and the
    /// siblings below never moved.
    #[test]
    fn the_default_transition_expands_as_well_as_fades() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone()).build(ctx, |ctx| {
                        TextLeaf::new().build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let height = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let av = composer.arena_nodes()[root].children[0];
            composer.arena_nodes()[av].measured_size.height
        };
        build(&mut composer); // hidden: no container
        visible.set(true);
        build(&mut composer); // the frame the enter starts on
        let start = height(&composer);
        assert!(
            start < 50.0,
            "the default enter starts from a collapsed container (got {start}) — a fade-only default \
             leaves it at the full content height"
        );
        for _ in 0..40 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
            build(&mut composer);
        }
        assert_eq!(height(&composer), 50.0, "and it reaches the content's own height");
    }

    /// 复现 demo（animated_visibility_demo panel_c）回归：Column 内
    /// [兄弟A, AnimatedVisibility, 兄弟B]——toggle AV 后兄弟 A（按钮位置）必须
    /// 保留。用户反馈：点第三个按钮后按钮本身消失。
    /// ⚠ visible 必须用 remember 创建（绑定 owner queue——State::new 外部
    /// 创建的 notify 不入队，不会触发重组——测试环境的既有陷阱）。
    #[test]
    fn siblings_survive_visibility_toggle() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let visible_holder = std::cell::RefCell::new(None::<State<bool>>);
        let mut counts = Vec::new();
        // AV content 执行计数——区分「Column Skip 冻结 AV」vs「动画未推进」
        let content_runs = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let cr = content_runs.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(crate::compose!(|ctx| {
                Column::new().build(ctx, |ctx| {
                    // 兄弟 A（模拟 toggle 按钮——AV 的兄弟，应始终保留）
                    LeafBox::new(40.0, 24.0).build(ctx);
                    // AV：show 变化 → exit 动画 → removed → 子树回收
                    let visible = ctx.remember(|| true); // remember 绑定 owner queue
                    *visible_holder.borrow_mut() = Some(visible.clone());
                    AnimatedVisibility::new(visible.clone())
                        .enter(VisibilityTransition::fade_in(crate::animation::TweenSpec::default()))
                        .exit(VisibilityTransition::fade_out(crate::animation::TweenSpec::default()))
                        .build(ctx, |ctx| {
                            cr.set(cr.get() + 1);
                            LeafBox::new(80.0, 40.0).build(ctx);
                        });
                    // 兄弟 B（固定锚点——AV 收缩后应保留）
                    LeafBox::new(20.0, 20.0).build(ctx);
                });
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        // 推进动画帧
        let mut advance = |composer: &mut Composer| {
            for _ in 0..8 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                recompose(composer);
            }
        };
        // 单次 toggle：翻转 visible → 逐帧推进 → 断言 AV 子树被回收（A/B 保留）
        let mut toggle = |composer: &mut Composer| {
            let v = visible_holder.borrow().clone().unwrap();
            v.set(!v.get()); // notify → 下帧重组（remember 创建的 State 绑定队列）
            recompose(composer);
            advance(composer);
            node_count(composer)
        };

        // 首帧：全显示 + enter 动画完成（progress 0→1——用户场景 toggle 时动画已就绪）
        recompose(&mut composer);
        advance(&mut composer);
        let n0 = node_count(&composer);
        counts.push(n0);
        assert!(n0 >= 4, "首帧应含 Column+A+B+AV内容（实际 {n0}）");

        // toggle(false)：exit 动画 → removed → AV 子树回收（Column+A+B=3；A 不应消失）
        let n1 = toggle(&mut composer);
        counts.push(n1);
        assert_eq!(n1, 3, "AV 移除后应剩 Column+A+B（实际 {n1}，序列 {counts:?}）——兄弟 A 不应消失");

        // toggle(true)：enter 动画 → AV 内容恢复（A/B 仍保留）
        let n2 = toggle(&mut composer);
        counts.push(n2);
        assert!(n2 >= 4, "AV 恢复后应含 Column+A+B+AV内容（实际 {n2}，序列 {counts:?}）");
    }

    /// 测试用固定尺寸叶子（Text 需字体环境——用固定尺寸盒子代替）
    struct TextLeaf;
    impl TextLeaf {
        const fn new() -> Self {
            Self
        }
        fn build(&self, ctx: &mut ComposeCtx) {
            let key = ctx.next_key();
            ctx.start_restartable_group(
                key,
                Modifier::new().size(100.0, 50.0),
                crate::layout::column::ColumnLayout::default(),
            );
            ctx.end_restartable_group();
        }
    }

    /// 指定尺寸的叶子（siblings_survive_visibility_toggle 用）
    struct LeafBox {
        w: f32,
        h: f32,
    }
    impl LeafBox {
        fn new(w: f32, h: f32) -> Self {
            Self { w, h }
        }
        fn build(&self, ctx: &mut ComposeCtx) {
            let key = ctx.next_key();
            ctx.start_restartable_group(
                key,
                Modifier::new().size(self.w, self.h),
                crate::layout::column::ColumnLayout::default(),
            );
            ctx.end_restartable_group();
        }
    }

    fn count_text(composer: &Composer) -> usize {
        node_count(composer)
    }

    /// Measure the AV policy math at a fixed progress (pure, no animation timing).
    /// Builds a minimal arena with one 120x60 leaf child and runs policy.measure.
    /// Returns (container size, child placements).
    fn measure_policy_at(
        expand: bool,
        expand_from: ExpandFrom,
        expand_h: bool,
        expand_from_h: ExpandFromH,
        progress: f32,
    ) -> (Size, Vec<Placement>) {
        let policy = VisibilityPolicy {
            progress: State::new(progress),
            expand,
            expand_from,
            expand_h,
            expand_from_h,
            content_size: crate::runtime::state::Backchannel::new((0.0, 0.0)),
        };
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(120.0, 60.0))];
        let policies: Vec<Box<dyn MeasurePolicy>> = Vec::new();
        let (size, placements) =
            policy.measure(&mut nodes, &policies, &[0], Constraints::new(0.0, 400.0, 0.0, 400.0));
        policy.place(&mut nodes, &[0], &placements);
        (size, placements)
    }

    #[test]
    fn expand_vertical_scales_height_by_progress() {
        // Vertical expand at p=0.5 on 120x60 content → 120x30, Top anchor keeps origin.
        let (size, placements) =
            measure_policy_at(true, ExpandFrom::Top, false, ExpandFromH::Start, 0.5);
        assert!((size.width - 120.0).abs() < 0.5, "width untouched, got {}", size.width);
        assert!((size.height - 30.0).abs() < 0.5, "height = 60*0.5 at p=0.5, got {}", size.height);
        assert!((placements[0].position.y - 0.0).abs() < 0.5, "Top anchor keeps y=0");
    }

    #[test]
    fn expand_horizontal_scales_width_by_progress() {
        // NEW horizontal expand at p=0.5 on 120x60 content → 60x60.
        let (size, _) = measure_policy_at(false, ExpandFrom::Top, true, ExpandFromH::Start, 0.5);
        assert!((size.width - 60.0).abs() < 0.5, "width = 120*0.5 at p=0.5, got {}", size.width);
        assert!((size.height - 60.0).abs() < 0.5, "height untouched, got {}", size.height);
    }

    #[test]
    fn expand_bottom_anchor_shifts_child_up() {
        // Bottom anchor at p=0.5: container 120x30, child y = 60-30 = 30 (bottom fixed).
        let (_, placements) =
            measure_policy_at(true, ExpandFrom::Bottom, false, ExpandFromH::Start, 0.5);
        assert!(
            (placements[0].position.y - 30.0).abs() < 0.5,
            "Bottom anchor child y should be 30, got {}",
            placements[0].position.y
        );
    }

    #[test]
    fn expand_end_anchor_shifts_child_left() {
        // End anchor at p=0.5: container 60x60, child x = 120-60 = 60 (right fixed).
        let (_, placements) =
            measure_policy_at(false, ExpandFrom::Top, true, ExpandFromH::End, 0.5);
        assert!(
            (placements[0].position.x - 60.0).abs() < 0.5,
            "End anchor child x should be 60, got {}",
            placements[0].position.x
        );
    }

    #[test]
    fn expand_h_demo_replica_grows_monotonically() {
        // Exact demo-D replica: Column > AV(expand_in_h) > Text-like sized leaf.
        // Step real animation frames; container width must grow monotonically 0→300.
        use crate::layout::components::Column;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(false);
        let build = |composer: &mut Composer, v: &State<bool>| {
            composer.compose(|ctx| {
                Column::new().build(ctx, |ctx| {
                    AnimatedVisibility::new(v.clone())
                        .enter(VisibilityTransition::expand_in_h(
                            crate::animation::TweenSpec::default(),
                        ))
                        .exit(VisibilityTransition::shrink_out_h(
                            crate::animation::TweenSpec::default(),
                        ))
                        .build(ctx, |ctx| {
                            LeafBox::new(300.0, 44.0).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer, &v);
        v.set(true);
        let mut widths = Vec::new();
        // Read AV container width: Column root child[0].
        let read_w = |composer: &Composer| -> f32 {
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let av = nodes[root].children[0];
            nodes[av].measured_size.width
        };
        for _ in 0..10 {
            build(&mut composer, &v);
            widths.push(read_w(&composer));
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(widths[0] < 100.0, "first frame should be narrow, widths={widths:?}");
        assert!(
            (widths[widths.len() - 1] - 300.0).abs() < 5.0,
            "last frame should settle at 300, widths={widths:?}"
        );
        for pair in widths.windows(2) {
            assert!(
                pair[1] + 1.0 >= pair[0],
                "width must grow monotonically, widths={widths:?}"
            );
        }
        crate::animation::clear_all_animations();
    }
    #[test]
    fn expand_h_first_frame_starts_collapsed() {
        // First frame after toggle from removed: progress ~0 so the container must be
        // ~0 wide — full width here means the animation never played (progress jumped
        // to 1 or expand_h miswired in build).
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(false);
        let v1 = v.clone();
        composer.compose(|ctx| {
            AnimatedVisibility::new(v1.clone())
                .enter(VisibilityTransition::expand_in_h(
                    crate::animation::TweenSpec::default(),
                ))
                .exit(VisibilityTransition::shrink_out_h(
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(300.0, 44.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        // Frame 1: removed, nothing composed (layout_root None → count 0).
        assert_eq!(node_count(&composer), 0, "removed AV leaves nothing composed");
        // Toggle visible, single frame, no animation stepping.
        v.set(true);
        composer.compose(|ctx| {
            AnimatedVisibility::new(v.clone())
                .enter(VisibilityTransition::expand_in_h(
                    crate::animation::TweenSpec::default(),
                ))
                .exit(VisibilityTransition::shrink_out_h(
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(300.0, 44.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // AV container IS the root here (top-level compose) — its own size, not the child's.
        let w = nodes[root].measured_size.width;
        assert!(
            w < 290.0,
            "frame 1 after toggle should be mid-expand (w<290), got {w} — animation never played"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn mid_flight_toggle_keeps_latched_anchor() {
        // P1-1: anchors latch at animation start — a mid-flight toggle must NOT jump
        // the anchor. Enter Top + exit Bottom: toggle visible mid-enter, the latched
        // anchor stays Top (child y=0) until the animation settles.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(false);
        let build = |composer: &mut Composer, v: &State<bool>| {
            composer.compose(|ctx| {
                AnimatedVisibility::new(v.clone())
                    .enter(
                        VisibilityTransition::expand_in(crate::animation::TweenSpec::default())
                            .with_expand_from(ExpandFrom::Top),
                    )
                    .exit(
                        VisibilityTransition::shrink_out(crate::animation::TweenSpec::default())
                            .with_expand_from(ExpandFrom::Bottom),
                    )
                    .build(ctx, |ctx| {
                        LeafBox::new(120.0, 60.0).build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer, &v);
        v.set(true);
        build(&mut composer, &v);
        // Step the enter forward to mid-flight (progress ~0.5): each cycle advances
        // the animation clock and rebuilds so progress leaves ~0 (otherwise the exit
        // below would start from ~0 and complete instantly — nothing to latch).
        for _ in 0..3 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer, &v);
        }
        v.set(false);
        build(&mut composer, &v);
        // After toggle mid-flight, child y must still be 0 (Top latched), not 30 (Bottom).
        // (Top-level compose: the AV container IS the root.)
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let child = nodes[root].children[0];
        let y = nodes[child].position.y;
        assert!(
            y.abs() < 1.0,
            "latched Top anchor must survive mid-flight toggle (child y=0), got {y}"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn slide_only_exit_is_not_clipped() {
        // P1-2: enter-expand + exit-slide combo — the exit slide must NOT be clipped.
        // Policy expand flag follows the active direction: exit slide → expand=false,
        // so measure peeks (no layout_dep) and build skips clip.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let v = State::new(true);
        composer.compose(|ctx| {
            AnimatedVisibility::new(v.clone())
                .enter(VisibilityTransition::expand_in(crate::animation::TweenSpec::default()))
                .exit(VisibilityTransition::slide_out(
                    SlideDirection::Right,
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(120.0, 60.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        // Settle enter, then toggle to exit-slide and check the container has no clip.
        for _ in 0..8 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            composer.compose(|ctx| {
                AnimatedVisibility::new(v.clone())
                    .enter(VisibilityTransition::expand_in(
                        crate::animation::TweenSpec::default(),
                    ))
                    .exit(VisibilityTransition::slide_out(
                        SlideDirection::Right,
                        crate::animation::TweenSpec::default(),
                    ))
                    .build(ctx, |ctx| {
                        LeafBox::new(120.0, 60.0).build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        }
        v.set(false);
        composer.compose(|ctx| {
            AnimatedVisibility::new(v.clone())
                .enter(VisibilityTransition::expand_in(crate::animation::TweenSpec::default()))
                .exit(VisibilityTransition::slide_out(
                    SlideDirection::Right,
                    crate::animation::TweenSpec::default(),
                ))
                .build(ctx, |ctx| {
                    LeafBox::new(120.0, 60.0).build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let av = nodes[root].children[0];
        let has_clip = nodes[av]
            .modifier
            .elements()
            .iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::Clip { .. }));
        assert!(!has_clip, "exit-slide direction must not clip the container");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn slide_offset_fraction_resolves_against_content_size() {
        // Fraction(1.0) on 120-wide content = 120px full slide; Fixed(48) stays 48.
        // Resolve through the same match arms the gfx closure uses.
        let resolve = |offset: SlideOffset, dir: SlideDirection, w: f32, h: f32| -> f32 {
            let extent = match dir {
                SlideDirection::Left | SlideDirection::Right => w,
                SlideDirection::Up | SlideDirection::Down => h,
            };
            offset.resolve(extent)
        };
        assert!((resolve(SlideOffset::Fixed(48.0), SlideDirection::Right, 120.0, 60.0) - 48.0).abs() < 0.01);
        assert!((resolve(SlideOffset::Fraction(1.0), SlideDirection::Right, 120.0, 60.0) - 120.0).abs() < 0.01);
        assert!((resolve(SlideOffset::Fraction(0.5), SlideDirection::Down, 120.0, 60.0) - 30.0).abs() < 0.01);
    }

    #[test]
    fn scale_from_and_origin_defaults_match_status_quo() {
        // Default transition = old behavior: scale_from 0.8, center pivot.
        // Old gfx math s = 0.8 + 0.2*p must equal new math with defaults.
        let t = VisibilityTransition::scale_in(crate::animation::TweenSpec::default());
        assert!((t.scale_from - 0.8).abs() < 1e-6);
        assert_eq!(t.transform_origin, (0.5, 0.5));
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let old = 0.8 + 0.2 * p;
            let new = t.scale_from + (1.0 - t.scale_from) * p;
            assert!((old - new).abs() < 1e-6, "scale math unchanged at p={p}");
        }
    }

    fn node_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        let mut count = 0;
        fn walk(nodes: &[LayoutNode], idx: usize, count: &mut usize) {
            *count += 1;
            for &c in &nodes[idx].children {
                walk(nodes, c, count);
            }
        }
        walk(nodes, root, &mut count);
        count
    }

    /// Every node's paint-time alpha, in tree order — the only way to tell a container's fade from a
    /// child's, since both are just graphics layers.
    fn layer_alphas(composer: &Composer) -> Vec<f32> {
        let Some(root) = composer.layout_root_idx() else { return Vec::new() };
        let nodes = composer.arena_nodes();
        let mut out = Vec::new();
        fn walk(nodes: &[LayoutNode], idx: usize, out: &mut Vec<f32>) {
            if let Some(p) = nodes[idx].modifier.graphics_layer_params() {
                out.push(p.alpha);
            }
            for &c in &nodes[idx].children {
                walk(nodes, c, out);
            }
        }
        walk(nodes, root, &mut out);
        out
    }

    /// Compose's `Modifier.animateEnterExit`: a child runs its OWN enter on the container's visibility
    /// target. The container fades over 100 ms and the child over 800 ms, so a quarter of a second after
    /// the flip the container is essentially opaque while the child has barely started — which a child
    /// sharing the container's clock cannot produce.
    #[test]
    fn a_child_can_run_its_own_enter() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = Composer::new();
        let visible = State::new(false);
        let v = visible.clone();
        let mut build = move |composer: &mut Composer| {
            let v = v.clone();
            composer.compose(|ctx| {
                AnimatedVisibility::new(v)
                    .enter(VisibilityTransition::fade_in(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(100),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .build_with_scope(ctx, |scope, ctx| {
                        let layer = scope.animate_enter_exit(
                            ctx,
                            VisibilityTransition::fade_in(crate::animation::TweenSpec::new(
                                std::time::Duration::from_millis(800),
                                crate::animation::interpolator::Linear::new(),
                            )),
                            VisibilityTransition::fade_out(crate::animation::TweenSpec::new(
                                std::time::Duration::from_millis(800),
                                crate::animation::interpolator::Linear::new(),
                            )),
                            (40.0, 20.0),
                        );
                        Column::new()
                            .modifier(layer.size(40.0, 20.0))
                            .build(ctx, |_| {});
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        build(&mut composer);
        visible.set(true);
        for _ in 0..13 {
            build(&mut composer);
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let alphas = layer_alphas(&composer);
        assert!(
            alphas.len() >= 2,
            "the container and the child both paint through a layer ({alphas:?})"
        );
        let container = alphas[0];
        let child = alphas[1];
        assert!(
            container > 0.9,
            "the container's 100 ms fade is done after ~260 ms (at {container})"
        );
        assert!(
            child > 0.0 && child < 0.5,
            "the child is on its own 800 ms clock (at {child}, container {container})"
        );
        crate::animation::clear_all_animations();
    }
