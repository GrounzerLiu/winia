    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::constraints::Constraints;

    /// Compose needs a tokio runtime (TextField coroutine scope) — same helper
    /// pattern as text_field.rs `build_field`.
    fn with_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn node_count(composer: &Composer) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        let mut count = 0;
        fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, count: &mut usize) {
            *count += 1;
            for &c in &nodes[idx].children {
                walk(nodes, c, count);
            }
        }
        walk(nodes, root, &mut count);
        count
    }

    /// Count Text leaves in the tree (results content marker).
    fn text_leaves(composer: &Composer, needle: &str) -> usize {
        let Some(root) = composer.layout_root_idx() else { return 0 };
        let nodes = composer.arena_nodes();
        let mut count = 0;
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            needle: &str,
            count: &mut usize,
        ) {
            let n = &nodes[idx];
            if n.modifier.elements().iter().any(|el| {
                matches!(el, crate::modifier::ModifierElement::TextContent { content, .. } if content.contains(needle))
            }) {
                *count += 1;
            }
            for &c in &n.children {
                walk(nodes, c, needle, count);
            }
        }
        walk(nodes, root, needle, &mut count);
        count
    }

    #[test]
    fn inactive_hides_results_active_registers_overlay() {
        // Core state machine: inactive → no overlay, no results; open() → fullscreen
        // Dialog overlay registered (results live there, NOT in-tree — in-tree
        // expansion would push siblings off screen); close() → overlay gone.
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        let build = |composer: &mut Composer, state: &SearchBarState| {
            composer.compose(|ctx| {
                SearchBar::new()
                    .state(state.clone())
                    .placeholder(|ctx| {
                        crate::components::text::Text::new("Search").build(ctx);
                    })
                    .build(ctx, |ctx| {
                        crate::components::text::Text::new("RESULT_MARKER").build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "inactive: no overlay");
        assert_eq!(text_leaves(&composer, "RESULT_MARKER"), 0, "inactive: no results");
        assert!(node_count(&composer) > 0, "inactive: collapsed bar exists");
        state.open();
        build(&mut composer, &state);
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "active: fullscreen Dialog overlay registered");
        assert!(overlays[0].modal, "expanded search is modal (outside tap dismisses)");
        assert!(node_count(&composer) > 0, "active: pill persists under overlay");
        state.close();
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "closed: overlay gone");
        assert_eq!(text_leaves(&composer, "RESULT_MARKER"), 0, "closed: no results");
    }

    #[test]
    fn query_state_round_trips() {
        let state = SearchBarState::new();
        assert_eq!(state.query_text(), "");
        state.set_query("hello");
        assert_eq!(state.query_text(), "hello");
        assert_eq!(state.query.get().text, "hello");
    }

    /// The expansion geometry mirrors Compose `FullScreenSearchBarLayout`: one progress value interpolates
    /// the container size, the corner radius and the vertical padding.
    ///
    /// Teeth: drop the `(1 - progress)` factor from `expansion_corner_radius` (a plausible slip that turns
    /// the morph into a pill that grows BY getting rounder) and the radius assertions fail.
    #[test]
    fn expansion_geometry_interpolates_with_progress() {
        // Size grows from the collapsed bar to the full-screen box.
        let collapsed = (240.0, SEARCH_BAR_HEIGHT);
        let full = (720.0, 560.0);
        assert_eq!(expansion_size(collapsed, full, 0.0), collapsed, "progress 0 = the bar");
        assert_eq!(expansion_size(collapsed, full, 1.0), full, "progress 1 = the panel");
        let mid = expansion_size(collapsed, full, 0.5);
        assert_eq!(mid, (480.0, (SEARCH_BAR_HEIGHT + 560.0) / 2.0), "half way is the midpoint");

        // Corner radius is the pill's radius times (1 - progress): round at rest, rect when open.
        assert_eq!(
            expansion_corner_radius(0.0),
            SEARCH_BAR_HEIGHT / 2.0,
            "collapsed: a full pill"
        );
        assert_eq!(expansion_corner_radius(1.0), 0.0, "expanded: square corners");
        assert!(
            expansion_corner_radius(0.5) < expansion_corner_radius(0.0),
            "the radius must SHRINK as it opens"
        );

        // …and the shape degenerates to a plain rect once the radius rounds away (Compose `radius < 1e-3`).
        assert!(matches!(expansion_shape(0.0), Shape::RoundedRect { .. }), "collapsed is rounded");
        assert!(matches!(expansion_shape(1.0), Shape::Rectangle), "expanded is a rect");

        // Vertical padding grows from zero.
        assert_eq!(expansion_vertical_padding(0.0), 0.0);
        assert_eq!(expansion_vertical_padding(1.0), SEARCH_BAR_VERTICAL_PADDING);
    }

    /// Compose's timings, as `TweenSpec` fields: expand 600ms of motion after a 100ms delay, collapse
    /// 350ms after the same delay. This used to be a `KeyframesSpec` whose duration was the sum, with the
    /// delay expressed as a held head of the polyline; `TweenSpec::delay` carries it directly now, so the
    /// duration here is the MOTION, exactly like Compose's `durationMillis`.
    #[test]
    fn expansion_specs_match_composes_timings() {
        let timing = |spec: &crate::animation::AnimationSpec| match spec {
            crate::animation::AnimationSpec::Tween(t) => {
                (t.duration.as_millis() as u64, t.delay.as_millis() as u64)
            }
            other => panic!("expected a tween spec, got {other:?}"),
        };
        assert_eq!(
            timing(&expand_spec()),
            (SEARCH_BAR_EXPAND_MS, SEARCH_BAR_ANIMATION_DELAY_MS),
            "expand = 600ms of motion, 100ms late"
        );
        assert_eq!(
            timing(&collapse_spec()),
            (SEARCH_BAR_COLLAPSE_MS, SEARCH_BAR_ANIMATION_DELAY_MS),
            "collapse = 350ms of motion, 100ms late"
        );

        // …and the curve is still the emphasized-decelerate one (fast early).
        let quarter = expand_interpolator().interpolate(0.25);
        assert!(quarter > 0.25, "emphasized-decelerate runs ahead of linear, got {quarter}");
    }


    /// The results subtree actually carries the faded graphics layer: measured on a real composition, the
    /// node wrapping the caller's content reports `alpha` from `content_progress` and the input field above
    /// it does not.
    ///
    /// Teeth: point the layer at `progress` (the geometry clock) and the mid-fade assertion fails.
    #[test]
    fn results_content_carries_the_content_fade_layer() {
        // Same guard as `expanded_container_shape_tracks_progress`: composing a SearchBar drives the
        // expansion animation onto the global `ACTIVE_ANIMATIONS` table.
        let _serial = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        state.open();
        // Compose, then take the overlay body and compose THAT too (the panel lives in its own composer).
        composer.compose(|ctx| {
            SearchBar::new()
                .state(state.clone())
                .build(ctx, |ctx| {
                    crate::components::text::Text::new("RESULT_MARKER").build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 420.0, 0.0, 700.0));
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "expanded search registers its panel");
        let mut panel = Composer::new();
        panel.compose(|ctx| (overlays[0].content)(ctx));
        panel.layout(Constraints::new(0.0, 420.0, 0.0, 700.0));

        // Nothing faded yet: content_progress is 0, so the results subtree's layer alpha is 0.
        let alpha_of = |c: &Composer| -> Option<f32> {
            let root = c.layout_root_idx()?;
            let nodes = c.arena_nodes();
            let mut found = None;
            let mut stack = vec![root];
            while let Some(i) = stack.pop() {
                if let Some(p) = nodes[i].modifier.graphics_layer_params() {
                    if (p.alpha - 1.0).abs() > 1e-6 {
                        found = Some(p.alpha);
                    }
                }
                stack.extend(nodes[i].children.iter().copied());
            }
            found
        };
        assert_eq!(
            alpha_of(&panel),
            Some(0.0),
            "the results layer starts fully transparent (content_progress 0)"
        );
    }

    /// The docked dropdown uses Compose's `DockedEnter/ExitTransition`: fade + VERTICAL reveal (never a
    /// translation), with the container durations (600ms in, 350ms out, 100ms delay on both).
    ///
    /// Teeth: switch either spec back to `slide_down` (the previous behaviour) and the "no translation"
    /// assertion fails — the dropdown is anchored under the bar, so a slide moved it, which is what
    /// DropdownMenu does and SearchBar does not.
    #[test]
    fn docked_transition_is_fade_plus_vertical_reveal() {
        for (name, spec) in [("enter", docked_enter_spec()), ("exit", docked_exit_spec())] {
            assert!(spec.fade, "{name}: Compose's docked transition fades");
            assert!(spec.reveal_top, "{name}: …and grows/shrinks VERTICALLY (expandVertically)");
            assert_eq!(spec.slide_from_y, 0.0, "{name}: no translation (that is DropdownMenu's motion)");
            assert_eq!(spec.scale_from, 1.0, "{name}: no scale either");
        }
        assert_eq!(
            docked_enter_spec().duration.as_millis() as u64,
            SEARCH_BAR_EXPAND_MS,
            "docked expand = 600ms of motion"
        );
        assert_eq!(
            docked_enter_spec().delay.as_millis() as u64,
            SEARCH_BAR_ANIMATION_DELAY_MS,
            "…starting after the 100ms delay (a separate hold, NOT folded into the duration: a 700ms \
             duration would begin moving during the delay)"
        );
        assert_eq!(
            docked_exit_spec().duration.as_millis() as u64,
            SEARCH_BAR_COLLAPSE_MS,
            "docked collapse = 350ms of motion"
        );
        assert_eq!(
            docked_exit_spec().delay.as_millis() as u64,
            SEARCH_BAR_ANIMATION_DELAY_MS,
            "…also after the 100ms delay"
        );
        // The spec the overlay layer actually runs must encode the hold, not just carry the duration.
        let crate::animation::AnimationSpec::Keyframes(k) = docked_enter_spec().animation_spec() else {
            panic!("a delayed overlay spec must compile to a hold-then-curve keyframe spec");
        };
        assert_eq!(
            k.duration.as_millis() as u64,
            SEARCH_BAR_EXPAND_MS + SEARCH_BAR_ANIMATION_DELAY_MS,
            "the runnable spec covers hold + motion"
        );
        let held = SEARCH_BAR_ANIMATION_DELAY_MS as f32 / k.duration.as_millis() as f32;
        let at_delay = k.frames.iter().find(|(x, _, _)| (*x - held).abs() < 1e-6);
        assert_eq!(
            at_delay.map(|(_, v, _)| *v),
            Some(0.0),
            "the value is still at its start when the delay elapses"
        );
    }

    /// The expanded container's SHAPE must follow `progress` during the animation, not only the size.
    ///
    /// The size is safe by construction — it is a `Dimension::Dynamic` closure that the layout pass calls
    /// every frame. The shape is not: `expansion_shape(progress.get())` reads the state during COMPOSITION,
    /// and the panel body sits in a `start_restartable_group` that only re-enters when its declared
    /// parameters change. If the body does not re-run while the animation advances, the radius stays at the
    /// value from the last re-entry and the pill never squares off.
    ///
    /// Teeth: this fails if the shape is captured once (the group skips on a progress-only change).
    #[test]
    fn expanded_container_shape_tracks_progress() {
        // `SearchBar::build` drives the expansion animation, which pushes onto the GLOBAL
        // `ACTIVE_ANIMATIONS` table. Without the serial lock a concurrent test can advance or clear it and
        // overwrite the progress this test sets (measured: this test failed intermittently in a full run
        // and passed when run alone). Same rule as every other test that pushes animations.
        let _serial = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let state = SearchBarState::new();
        state.open();

        let mut main = Composer::new();
        let mut panel = Composer::new();
        let mut radius = |main: &mut Composer, panel: &mut Composer, state: &SearchBarState| -> Option<f32> {
            let s = state.clone();
            main.compose(|ctx| {
                SearchBar::new().state(s).build(ctx, |ctx| {
                    crate::components::text::Text::new("row").build(ctx);
                });
            });
            main.layout(Constraints::new(0.0, 420.0, 0.0, 700.0));
            let overlays = main.take_overlays();
            panel.compose(|ctx| (overlays[0].content)(ctx));
            panel.layout(Constraints::new(0.0, 420.0, 0.0, 700.0));
            // Breadth-first, and take the first node carrying a `Clip` — REGARDLESS of the shape it clips.
            // The container encloses the IconButton, so the shallowest clipping node is the container.
            // Requiring the clip to be ROUNDED was the bug: at the expanded end the container clips
            // `Shape::Rectangle`, so the search slid past it onto the IconButton's 20 dp rounded clip and the
            // test read that instead. Several earlier attempts patched the ordering; the identifier was wrong.
            let root = panel.layout_root_idx()?;
            let nodes = panel.arena_nodes();
            let mut found = None;
            let mut queue = std::collections::VecDeque::from([root]);
            'walk: while let Some(i) = queue.pop_front() {
                for el in nodes[i].modifier.elements() {
                    if let crate::modifier::ModifierElement::Clip { shape } = el {
                        found = match shape.eval() {
                            crate::graphics::Shape::RoundedRect { corner_radius }
                                if corner_radius > 0.5 =>
                            {
                                Some(corner_radius)
                            }
                            _ => None,
                        };
                        break 'walk;
                    }
                }
                queue.extend(nodes[i].children.iter().copied());
            }
            found
        };

        // Collapsed end of the animation: the container is a full pill.
        state.progress.set(0.0);
        let at_zero = radius(&mut main, &mut panel, &state);

        // Half way: the radius must have halved (progress 0.5 -> radius * 0.5).
        state.progress.set(0.5);
        let at_half = radius(&mut main, &mut panel, &state);

        // Fully open: square corners, so no rounded shape at all.
        state.progress.set(1.0);
        let at_one = radius(&mut main, &mut panel, &state);

        assert!(
            at_zero.is_some(),
            "collapsed: a rounded container is expected (got {at_zero:?})"
        );
        let expected_half = at_zero.unwrap() * 0.5;
        assert!(
            matches!(at_half, Some(r) if (r - expected_half).abs() < 1.0),
            "the radius must track progress: expected ~{expected_half} at p=0.5, got {at_half:?} \
             (a shape captured at composition time would still read {:?})",
            at_zero
        );
        assert_eq!(at_one, None, "fully expanded: square corners (no rounded clip left)");
    }

    /// The content fade is its own channel (Compose `contentAnimatable`), so the results must NOT be locked
    /// to the container's timeline: fading in is a short, delayed spec, fading out is short and immediate.
    ///
    /// Teeth: make the fade-in reuse `expand_spec()` (the container's 600ms spec) and the duration assertion
    /// fails — that is the coupling Compose specifically avoids.
    #[test]
    fn content_fade_is_separate_from_the_geometry_clock() {
        let timing = |spec: &crate::animation::AnimationSpec| match spec {
            crate::animation::AnimationSpec::Tween(t) => {
                (t.duration.as_millis() as u64, t.delay.as_millis() as u64)
            }
            other => panic!("unexpected spec {other:?}"),
        };
        assert_eq!(
            timing(&content_fade_in_spec()),
            (SEARCH_BAR_CONTENT_FADE_MS, SEARCH_BAR_CONTENT_FADE_DELAY_MS),
            "content fades in on the short spec + the short delay, not on the 600ms container clock"
        );
        assert_eq!(
            timing(&content_fade_out_spec()),
            (SEARCH_BAR_CONTENT_FADE_MS, 0),
            "content fades out immediately (no delay)"
        );
        // …and it is genuinely shorter than the container's motion, which is the point of the split.
        assert!(
            timing(&content_fade_in_spec()).0 < timing(&expand_spec()).0,
            "the content clock must be shorter than the geometry clock"
        );
    }

    #[test]
    fn docked_inactive_hides_results() {
        // Docked collapsed: no results, no popup. (Popup overlay needs app loop;
        // compose-level assert covers the collapsed half; expanded popup covered by
        // docked demo + Popup's own overlay tests.)
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        composer.compose(|ctx| {
            DockedSearchBar::new()
                .state(state.clone())
                .build(ctx, |ctx| {
                    crate::components::text::Text::new("DOCKED_RESULT").build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        assert_eq!(text_leaves(&composer, "DOCKED_RESULT"), 0, "docked inactive: no results");
        assert!(node_count(&composer) > 0, "docked inactive: bar exists");
    }

    #[test]
    fn docked_active_registers_anchored_popup() {
        // Active docked: exactly one non-modal Popup overlay WITH an anchor slot
        // (explicit wrapper-group key — prev_sibling capture cannot work: statement
        // keys and node keys live in different key spaces). The anchor must
        // resolve to a laid-out node (the pill box), otherwise the dropdown
        // falls back to window alignment (lands at screen bottom).
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        let build = |composer: &mut Composer, state: &SearchBarState| {
            composer.compose(|ctx| {
                DockedSearchBar::new()
                    .state(state.clone())
                    .build(ctx, |ctx| {
                        crate::components::text::Text::new("DOCKED_RESULT").build(ctx);
                    });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "docked inactive: no overlay");
        state.open();
        build(&mut composer, &state);
        let overlays = composer.take_overlays();
        assert_eq!(overlays.len(), 1, "docked active: popup overlay registered");
        assert!(!overlays[0].modal, "docked dropdown is non-modal");
        let anchor = overlays[0].anchor_slot.expect("docked popup must carry an anchor slot");
        let root = composer.layout_root_idx().expect("layout root exists");
        let nodes = composer.arena_nodes();
        let nid = crate::layout::node::find_node_id_by_slot_key(nodes, root, anchor);
        assert!(nid.is_some(), "anchor slot must resolve to a laid-out node (pill box)");
        state.close();
        build(&mut composer, &state);
        assert!(composer.take_overlays().is_empty(), "docked closed: overlay gone");
    }

    #[test]
    fn defaults_shapes_are_distinct() {
        // Pill (collapsed) vs Rectangle (fullscreen) vs rounded-12 (docked).
        assert_eq!(SearchBarDefaults::input_field_shape(), Shape::Pill);
        assert_eq!(SearchBarDefaults::full_screen_shape(), Shape::Rectangle);
        assert_ne!(
            SearchBarDefaults::docked_shape(),
            Shape::Rectangle,
            "docked keeps rounded corners"
        );
    }

    /// The input field opts out of the framework's focus ring.
    ///
    /// The render-side exclusion keys on `TextFieldVisual`, and this field is `no_container()` — so
    /// the framework painted its blue ring around the whole input row when the field took focus (the
    /// reported blue line on the expanded panel). The bar draws the field's affordance itself, the way
    /// an M3 container does and the way `Slider` opts out for its thumb.
    ///
    /// Structural on purpose: the ring is a paint-time decision, so this pins the opt-out on the node
    /// the bar focuses (the one carrying the `FocusRequester`). The visual result was verified on a
    /// running window with a temporary probe inside the ring's draw branch: 169 draws while the panel
    /// expanded before the fix, 0 after, while a focused `Button` still drew 132.
    #[test]
    fn the_input_field_opts_out_of_the_framework_focus_ring() {
        let _rt = with_runtime();
        let _guard = _rt.enter();
        let mut composer = Composer::new();
        let state = SearchBarState::new();
        composer.compose(|ctx| {
            SearchBar::new()
                .state(state.clone())
                .build(ctx, |ctx| {
                    crate::components::text::Text::new("RESULT").build(ctx);
                });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));

        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let mut stack = vec![root];
        let mut checked = 0;
        while let Some(i) = stack.pop() {
            let els = nodes[i].modifier.elements();
            let has_requester = els.iter().any(|el| matches!(el, crate::modifier::ModifierElement::FocusRequesterId { .. }));
            if has_requester {
                checked += 1;
                assert!(
                    els.iter().any(|el| matches!(el, crate::modifier::ModifierElement::NoFocusRing)),
                    "the node carrying the FocusRequester must also opt out of the focus ring"
                );
            }
            stack.extend(nodes[i].children.iter().copied());
        }
        assert!(checked > 0, "the bar must expose a focus requester to focus its field");
    }
