    use super::*;
    use crate::layout::constraints::Constraints;
    use crate::layout::node::LayoutNode;
    use crate::runtime::composer::Composer;
    use crate::runtime::state::State;

    /// Test leaf: its width reflects the target (page=0 → 50, anything else → 200).
    struct SizedLeaf {
        w: f32,
    }
    impl SizedLeaf {
        fn build(&self, ctx: &mut ComposeCtx) {
            let key = ctx.next_key();
            ctx.start_restartable_group(
                key,
                Modifier::new().size(self.w, 30.0),
                crate::layout::column::ColumnLayout::default(),
            );
            ctx.end_restartable_group();
        }
    }

    /// Two generations on screen means the container has two children; at rest it has one.
    ///
    /// A shape heuristic, with two blind spots worth knowing before reusing it: a generation whose
    /// content composes NO node (legal — `if flag { Text(..) }` with `flag` false) leaves that
    /// wrapper childless, and a tree whose root is not the container (every demo wraps it in a
    /// `Column`) sends the `children[0]` descent past it. Both hold for the trees in this module.
    fn container_child_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize) -> usize {
            let n = &nodes[idx];
            if n.children.is_empty() {
                return 0;
            }
            // The container: 1..=2 children, each of which has children of its own (the wrapper
            // around the content).
            if n.children.len() <= 2 && n.children.iter().all(|&c| !nodes[c].children.is_empty()) {
                return n.children.len();
            }
            walk(nodes, n.children[0])
        }
        walk(nodes, root)
    }

    /// Every leaf's width, ascending — two generations on screen show up as two entries.
    fn leaf_widths(composer: &Composer) -> Vec<f32> {
        let Some(root) = composer.layout_root_idx() else { return Vec::new() };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize, out: &mut Vec<f32>) {
            if nodes[idx].children.is_empty() {
                out.push(nodes[idx].measured_size.width);
                return;
            }
            for &c in &nodes[idx].children {
                walk(nodes, c, out);
            }
        }
        let mut out = Vec::new();
        walk(nodes, root, &mut out);
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }

    /// Integration: a target change puts both generations on screen (old 50 fading out, new 200
    /// fading in) while the container animates from the old size to the new one.
    #[test]
    fn animated_content_switches_with_size_transform() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(VisibilityTransition::fade_in(crate::animation::TweenSpec::default()))
                    .exit(VisibilityTransition::fade_out(crate::animation::TweenSpec::default()))
                    .size_animation(crate::animation::TweenSpec::default())
                    .build(ctx, |ctx, page| {
                        let w = if page == 0 { 50.0 } else { 200.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance_one = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(composer);
        };

        // First frame: target 0 is showing (container 50 wide, one generation)
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initially target 0");
        assert_eq!(container_child_count(&composer), 1, "one generation at rest");

        // Switch: both generations up, the container interpolating 50 → 200 with the size progress
        target.set(7);
        recompose(&mut composer);
        let mut saw_both = false;
        let mut mid_w = -1.0;
        for _ in 0..80 {
            advance_one(&mut composer);
            let w = leaf_widths(&composer);
            if w.contains(&50.0) && w.contains(&200.0) {
                saw_both = true;
            }
            let cw = container_width_of(&composer);
            if cw > 50.0 && cw < 200.0 {
                mid_w = cw;
                break;
            }
        }
        assert!(saw_both, "both generations are on screen during the transition");
        assert!(
            mid_w > 50.0 && mid_w < 200.0,
            "the container should be mid-way through 50..200 while the size animates (got {mid_w})"
        );

        // Advance to completion — one generation, container width = the new content's 200
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![200.0], "the outgoing generation is gone");
        assert_eq!(container_width_of(&composer), 200.0, "the container ends at the new content's width");

        // Switch back to target 0
        target.set(0);
        recompose(&mut composer);
        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![50.0], "switching back leaves one generation");
        assert_eq!(container_width_of(&composer), 50.0, "the container width is back to 50");
    }

    /// Every leaf's placement, as (x, width) — the alignment test needs WHERE a generation sits, not
    /// just how wide it is.
    fn leaf_placements(composer: &Composer) -> Vec<(f32, f32)> {
        let Some(root) = composer.layout_root_idx() else { return Vec::new() };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize, x: f32, out: &mut Vec<(f32, f32)>) {
            let here = x + nodes[idx].position.x;
            if nodes[idx].children.is_empty() {
                out.push((here, nodes[idx].measured_size.width));
                return;
            }
            for &c in &nodes[idx].children {
                walk(nodes, c, here, out);
            }
        }
        let mut out = Vec::new();
        walk(nodes, root, 0.0, &mut out);
        out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        out
    }

    /// The container's own measured width: it is the interpolated value while two generations are
    /// up.
    fn container_width_of(composer: &Composer) -> f32 {
        let Some(root) = composer.layout_root_idx() else { return -1.0 };
        let nodes = composer.arena_nodes();
        fn walk(nodes: &[LayoutNode], idx: usize) -> f32 {
            let n = &nodes[idx];
            if n.children.is_empty() {
                return -1.0;
            }
            if n.children.len() <= 2 && n.children.iter().all(|&c| !nodes[c].children.is_empty()) {
                return n.measured_size.width;
            }
            walk(nodes, n.children[0])
        }
        walk(nodes, root)
    }

    /// sizeTransform is independent (regression: sharing the progress with the fade made
    /// `same_target` dedupe skip it, so `size_spec` never took effect). `size_animation` is Snap
    /// here, so the container reaches the new width the moment of the switch while the fade is
    /// still running.
    #[test]
    fn size_animation_independent_of_fade() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let slow = || {
            VisibilityTransition::fade_in(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(600),
                crate::animation::interpolator::Linear::new(),
            ))
        };

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(slow())
                    .exit(VisibilityTransition::fade_out(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .size_animation(crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()))
                    .build(ctx, |ctx, page| {
                        let w = if page == 0 { 50.0 } else { 200.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance_one = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(composer);
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initially target 0");

        target.set(7);
        recompose(&mut composer);
        for _ in 0..20 {
            advance_one(&mut composer);
        }
        // The fade is only ~320ms into its 600ms — with an independent size (Snap) the container
        // should already be 200; sharing the progress (the old bug) would leave it mid-interpolation.
        let w = container_width_of(&composer);
        assert!(
            (w - 200.0).abs() < 0.5,
            "sizeTransform(Snap) reaches 200 mid-fade (got {w})"
        );
    }

    /// Compose's `AnimatedContent` keeps the OUTGOING generation composed while the incoming one
    /// enters (`currentlyVisible`) and draws the incoming on top (`AnimatedContent.kt:189`). The
    /// old implementation faded the outgoing to nothing, swapped, then faded the incoming in — one
    /// generation at a time.
    #[test]
    fn both_generations_are_composed_during_a_transition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone()).build(ctx, |ctx, page| {
                    let w = if page == 0 { 50.0 } else { 200.0 };
                    SizedLeaf { w }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance_one = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(composer);
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "at rest: one generation");
        assert_eq!(container_child_count(&composer), 1);

        target.set(7);
        recompose(&mut composer);
        let mut both = false;
        for _ in 0..40 {
            advance_one(&mut composer);
            let widths = leaf_widths(&composer);
            if widths.contains(&50.0) && widths.contains(&200.0) {
                both = true;
                assert_eq!(container_child_count(&composer), 2, "two children: outgoing + incoming");
                break;
            }
        }
        assert!(both, "both generations are on screen mid-transition");

        for _ in 0..90 {
            advance_one(&mut composer);
        }
        assert_eq!(leaf_widths(&composer), vec![200.0], "the outgoing is removed");
        assert_eq!(container_child_count(&composer), 1);
    }

    /// `contentKey`: two states with the same key are the same content — the value is swapped in
    /// without a transition (Compose's `contentKey`), so no second generation appears.
    #[test]
    fn the_same_content_key_updates_in_place_without_a_transition() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();

        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    // 0..=9 is one piece of content (two renders of the same page); 10 is a new one.
                    .content_key(|p| if *p < 10 { 0 } else { 1 })
                    .build(ctx, |ctx, page| {
                        // The width must follow the VALUE, not the key: with a key-derived width this
                        // test could not tell "the content re-ran with the new value" from "the content
                        // never re-ran" — both give the same leaf. Review finding.
                        let w = 50.0 + page as f32 * 10.0;
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![50.0], "initial");

        // Same key: the value is swapped and the content re-runs with it (the width 80 is the
        // proof it re-ran), but no second generation enters
        target.set(3);
        recompose(&mut composer);
        assert_eq!(container_child_count(&composer), 1, "no transition for the same key");
        assert_eq!(leaf_widths(&composer), vec![80.0], "the content re-ran with the new value");

        // A new key: a normal transition with both generations up
        target.set(10);
        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![80.0, 150.0], "a new key animates");
    }


    /// `contentAlignment` puts each generation where Compose puts it: the child is aligned inside the
    /// container, not at its origin (`AnimatedContent.kt:692-694`).
    ///
    /// The case that shows it is a small page in a container that is still the large page's size —
    /// which is what the size transform spends its time doing. The size spec is slow ON PURPOSE: with
    /// `Snap` the container is already the new size on the frame of the switch, so the child and the
    /// box agree and a broken alignment looks correct (measured — that version of this test passed
    /// with the alignment removed).
    #[test]
    fn animated_content_aligns_each_generation_inside_the_container() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .content_alignment(ContentAlignment::CENTER)
                    .size_animation(AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(5000),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .build(ctx, |ctx, page| {
                        let w = if page == 0 { 200.0 } else { 60.0 };
                        SizedLeaf { w }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        // The switch: the small page arrives while the container is still the large one's size.
        target.set(1);
        recompose(&mut composer);
        let box_w = container_width_of(&composer);
        let leaves = leaf_placements(&composer);
        let small = leaves
            .iter()
            .find(|(_, w)| (*w - 60.0).abs() < 0.01)
            .map(|(x, w)| (*x, *w))
            .expect("the small generation is on screen");
        assert!(
            (small.0 - (box_w - small.1) / 2.0).abs() < 0.51,
            "the child is centred in the container, not at its origin (x={}, box_w={box_w})",
            small.0
        );
    }

    /// A switch made while a transition is STILL RUNNING keeps the state it interrupted, so three
    /// states are on screen at once.
    ///
    /// Compose's `currentlyVisible` is a list and only empties when the transition settles, so the
    /// state that was leaving, the one that was arriving and the new target are all composed
    /// together. The implementation this replaced held ONE outgoing generation: when the third
    /// target arrived it overwrote the first outgoing state, dropping a state that was still
    /// visible. The specs here widen that window (a 600 ms exit against an 80 ms enter) so the
    /// difference is not a matter of sampling luck.
    #[test]
    fn a_switch_during_a_transition_keeps_the_state_it_interrupted() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(VisibilityTransition::fade_in(TweenSpec::new(
                        std::time::Duration::from_millis(80),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .exit(VisibilityTransition::fade_out(TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .size_animation(AnimationSpec::Snap(crate::animation::SnapSpec::default()))
                    .build(ctx, |ctx, page| {
                        SizedLeaf { w: 50.0 + page as f32 * 10.0 }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer, n: usize| {
            for _ in 0..n {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(16));
                recompose(composer);
            }
        };

        recompose(&mut composer);
        target.set(1);
        recompose(&mut composer);
        advance(&mut composer, 10);
        assert_eq!(
            leaf_widths(&composer),
            vec![50.0, 60.0],
            "the second state is in while the first is still leaving"
        );

        // …switch to a third while both are still on screen.
        target.set(2);
        recompose(&mut composer);
        advance(&mut composer, 1);
        assert_eq!(
            leaf_widths(&composer),
            vec![50.0, 60.0, 70.0],
            "the target is on screen at once and neither state it interrupted was dropped"
        );

        advance(&mut composer, 90);
        assert_eq!(leaf_widths(&composer), vec![70.0], "and it settles on the target alone");
    }

    /// How many content slots have ever been created, to tell "the entry kept its slot" from "the
    /// entry was thrown away and rebuilt".
    static SLOT_MARKERS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// Compose replaces an entry that is still on screen IN PLACE, so a state that comes back keeps
    /// its own alpha AND its own slots — it reverses instead of restarting from nothing. The width
    /// here encodes which slot the content lives in (`remember` runs once per slot), which is what
    /// makes the difference visible: the single-outgoing implementation dropped the leaving entry
    /// and rebuilt the returning one, and that shows up as a third slot.
    #[test]
    fn a_state_that_comes_back_reuses_its_own_slot() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        SLOT_MARKERS.store(0, std::sync::atomic::Ordering::SeqCst);
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone())
                    .enter(VisibilityTransition::fade_in(TweenSpec::new(
                        std::time::Duration::from_millis(80),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .exit(VisibilityTransition::fade_out(TweenSpec::new(
                        std::time::Duration::from_millis(600),
                        crate::animation::interpolator::Linear::new(),
                    )))
                    .size_animation(AnimationSpec::Snap(crate::animation::SnapSpec::default()))
                    .build(ctx, |ctx, _page| {
                        let slot = ctx
                            .remember(|| SLOT_MARKERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
                            .peek();
                        SizedLeaf { w: 10.0 + slot as f32 * 100.0 }.build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer, n: usize| {
            for _ in 0..n {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(16));
                recompose(composer);
            }
        };

        recompose(&mut composer);
        assert_eq!(leaf_widths(&composer), vec![10.0], "the first state lives in slot 0");
        target.set(1);
        recompose(&mut composer);
        advance(&mut composer, 6);
        assert_eq!(leaf_widths(&composer), vec![10.0, 110.0], "the second state gets its own slot");

        // …back to 0 while 1 is still leaving: 0 is on screen, so it is replaced in place.
        target.set(0);
        recompose(&mut composer);
        advance(&mut composer, 1);
        assert_eq!(
            leaf_widths(&composer),
            vec![10.0, 110.0],
            "the state that came back kept its own slot, and no third one was created"
        );
        assert_eq!(
            SLOT_MARKERS.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "exactly two slots were ever created"
        );

        advance(&mut composer, 90);
        assert_eq!(leaf_widths(&composer), vec![10.0], "and it settles on the state that came back");
    }

    /// A transition leaves nothing running behind it: once only the target is on screen — which also
    /// means the container's size animation has settled — the engine is idle again.
    #[test]
    fn nothing_is_still_animating_once_a_switch_settles() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = Composer::new();
        let target = State::new(0u32);
        let t = target.clone();
        let mut recompose = |composer: &mut Composer| {
            composer.compose(|ctx| {
                AnimatedContent::new(t.clone()).build(ctx, |ctx, page| {
                    let w = if page == 0 { 50.0 } else { 200.0 };
                    SizedLeaf { w }.build(ctx);
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        recompose(&mut composer);
        target.set(1);
        recompose(&mut composer);
        let mut settled = false;
        for _ in 0..120 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(16));
            recompose(&mut composer);
            if !crate::animation::is_animating() && leaf_widths(&composer) == vec![200.0] {
                settled = true;
                break;
            }
        }
        assert!(settled, "the transition finishes and the engine goes idle");
        assert!(!crate::animation::is_animating(), "nothing is left animating");
    }
