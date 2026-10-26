    use super::*;
    use crate::layout::constraints::Constraints;
    use crate::runtime::composer::Composer;

    fn compose_pager(
        composer: &mut Composer,
        state: PagerState,
        composed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                let c = composed.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, move |ctx, page| {
                        use std::sync::atomic::Ordering;
                        c.fetch_add(1, Ordering::Relaxed);
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
    }

    #[test]
    fn a_pager_starts_on_page_zero() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        compose_pager(&mut composer, state.clone(), std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)));
        assert_eq!(state.current_page(), 0);
        assert_eq!(state.current_page_offset_fraction(), 0.0);
        assert_eq!(state.settled_page(), 0);
    }

    #[test]
    fn scrolling_to_a_page_moves_the_counter() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        state.scroll_to_page(3, 0.0);
        // The jump applies during LAYOUT, and the page counter is derived at the end of the next
        // build (the same eventual consistency Compose has while a scroll is animating).
        compose_pager(&mut composer, state.clone(), composed.clone());
        compose_pager(&mut composer, state.clone(), composed.clone());
        assert_eq!(state.list_state().first_visible(), 3, "the jump landed on page 3");
        assert_eq!(state.current_page(), 3, "scrollToPage(3) makes page 3 current");
        assert!(state.current_page_position() >= 2.99, "position={}", state.current_page_position());
    }

    #[test]
    fn only_a_window_of_pages_composes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let state = PagerState::new(200);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        let n = composed.load(Ordering::Relaxed);
        assert!(n < 30, "a 200-page pager must not compose every page (composed {n})");
    }

    #[test]
    fn the_fraction_is_never_half_a_page() {
        let state = PagerState::new(3);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        // Half a page into page 0: the counter is still 0 and the fraction approaches 0.5.
        state.list_state().offset.set(100.0);
        compose_pager(&mut composer, state.clone(), composed.clone());
        assert!(state.current_page_offset_fraction().abs() < 0.5);
    }

    #[test]
    fn a_fixed_page_is_the_size_it_was_given() {
        let state = PagerState::new(9);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .page_size(PageSize::Fixed(120.0))
                    .snap_position(SnapPosition::Center)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .page_size(PageSize::Fixed(120.0))
                    .snap_position(SnapPosition::Center)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        let info = state.layout_info();
        assert_eq!(info.page_size, 120.0, "a fixed page keeps its size");
        assert_eq!(info.viewport_size, 300.0);
        assert_eq!(info.page_count, 9);
        // The page counter divides by the STEP (page + spacing), not by the viewport.
        assert_eq!(state.current_page(), 0, "page 0 at rest");
        assert!(state.current_page_offset_fraction().abs() < 1e-3, "fraction 0 at rest, got {}", state.current_page_offset_fraction());
    }

    #[test]
    fn a_restricted_window_composes_fewer_pages() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let mut counts = Vec::new();
        for beyond in [0usize, 4] {
            let state = PagerState::new(200);
            let mut composer = Composer::new();
            let theme = crate::theme::ThemeColors::default_light();
            let composed = std::sync::Arc::new(AtomicUsize::new(0));
            let c = composed.clone();
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let s = state.clone();
                    HorizontalPager::new(s)
                        .beyond_viewport_page_count(beyond)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, move |ctx, page| {
                            c.fetch_add(1, Ordering::Relaxed);
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
            counts.push(composed.load(Ordering::Relaxed));
        }
        assert!(
            counts[0] < counts[1],
            "beyondViewportPageCount=0 must compose fewer pages than 4 (got {:?})",
            counts
        );
    }

    #[test]
    fn beyond_zero_keeps_paging_snap_configured() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                HorizontalPager::new(state.clone())
                    .beyond_viewport_page_count(0)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, _page| {
                        crate::components::text::Text::new("page").build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        assert!(state.list_state().snap.peek().is_some(), "a two-page pager must snap even with beyond=0");
    }

    #[test]
    fn a_centered_fixed_pager_can_reach_its_last_page() {
        let state = PagerState::new(4);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state.clone())
                        .page_size(PageSize::Fixed(120.0))
                        .snap_position(SnapPosition::Center)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        };
        build(&mut composer);
        state.scroll_to_page(3, 0.0);
        build(&mut composer);
        build(&mut composer);
        assert_eq!(state.current_page(), 3, "the last page remains reachable with Center snap");
        assert!(state.current_page_offset_fraction().abs() < 1e-3);
    }

    #[test]
    fn user_scroll_disabled_keeps_the_container_and_drops_only_the_input() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .user_scroll_enabled(false)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        // The scroll CONTAINER stays: it owns the clip and the viewport, and dropping it made the list
        // paint its items over the next card (measured in the lazy demo). What goes away is the input
        // channel — the gesture paths skip a node whose modifier says so.
        let container = composer
            .arena_nodes()
            .iter()
            .find(|node| node.modifier.horizontal_scroll_state().is_some())
            .expect("the scroll container is still there");
        assert!(
            container.modifier.is_scroll_input_disabled(),
            "userScrollEnabled=false must take the wheel/drag channel away"
        );
    }

    /// The snap positions stay on page boundaries even when the page is offset inside the viewport: the
    /// `SnapPosition` shift is applied to the page's PLACEMENT, not to the scroll target (Compose
    /// shifts the page too). A centred fixed page therefore rests at `k * step`, not at `k * step +
    /// offset`, which is what this pins.
    #[test]
    fn a_centred_fixed_page_rests_on_a_page_boundary() {
        let state = PagerState::new(9);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let mut build = |composer: &mut Composer, state: PagerState| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state)
                        .page_size(PageSize::Fixed(120.0))
                        .snap_position(SnapPosition::Center)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 400.0));
        };
        build(&mut composer, state.clone());
        state.scroll_to_page(1, 0.0);
        build(&mut composer, state.clone());
        build(&mut composer, state.clone());
        assert!(
            (state.list_state().offset() - 120.0).abs() < 0.5,
            "page 1 rests at one page boundary (120), not at boundary + snap offset: got {}",
            state.list_state().offset()
        );
        assert_eq!(state.current_page(), 1);
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "and the fraction is 0 at rest, got {}",
            state.current_page_offset_fraction()
        );
    }

    /// The user's bug: while the list scrolls by something OTHER than this call site re-running, the
    /// counter has to follow. Writing the offset directly is exactly that shape - no compose happens -
    /// so this pins the derive being a subscription and not a build-time computation.
    #[test]
    fn the_counter_follows_a_scroll_that_never_recomposes_the_pager() {
        let state = PagerState::new(9);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        // A second pass: the page size is resolved from the viewport the FIRST measure wrote.
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let s = state.clone();
                HorizontalPager::new(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .build(ctx, |ctx, page| {
                        crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        assert_eq!(state.current_page(), 0);

        // Two whole pages of scroll, written like the wheel does it: no compose call in between.
        let step = state.layout_info().page_size;
        assert!(step > 0.0, "the first measure resolved the page size");
        state.list_state().offset.set(step * 2.0);

        assert_eq!(
            state.current_page(),
            2,
            "the counter must follow the offset without this command re-composing"
        );
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "and land on the boundary exactly, got {}",
            state.current_page_offset_fraction()
        );
    }

    /// A padded pager starts AT its first snap position: the fraction is 0 on the first frame, not
    /// `-pad/step`, and the page is not clipped by the padding it sits inside.
    #[test]
    fn a_padded_pager_starts_on_its_first_snap_position() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer, state: PagerState| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state)
                        .content_padding(16.0)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        build(&mut composer, state.clone());
        build(&mut composer, state.clone());
        assert_eq!(state.current_page(), 0);
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "a padded pager rests on fraction 0, got {}",
            state.current_page_offset_fraction()
        );
        let info = state.layout_info();
        assert!(
            (info.page_size - 268.0).abs() < 0.5,
            "a Fill page is viewport - before - after (300 - 16 - 16), got {}",
            info.page_size
        );
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let first_page = nodes[root].children.iter().copied().find(|&child| {
            (nodes[child].measured_size.width - info.page_size).abs() < 0.5
        }).expect("the first page uses the resolved Fill size");
        assert!(
            (nodes[first_page].measured_size.width - 268.0).abs() < 0.5,
            "the measured page must honor content padding, got {}",
            nodes[first_page].measured_size.width
        );

        // Page 1 at rest: the counter divides by the PADDED page size (268), not by the viewport (300).
        // The derive used to fall back to the raw viewport, which only ever showed past page 0.
        state.scroll_to_page(1, 0.0);
        build(&mut composer, state.clone());
        build(&mut composer, state.clone());
        assert_eq!(state.current_page(), 1, "page 1 is the current page");
        assert!(
            state.current_page_offset_fraction().abs() < 1e-3,
            "and it rests on fraction 0, got {}",
            state.current_page_offset_fraction()
        );
    }

    /// A different `PagerState` at the same call site must be followed: the derive is keyed by the
    /// state's identity, not remembered across the swap.
    #[test]
    fn swapping_the_state_at_one_call_site_follows_the_new_state() {
        let first = PagerState::new(5);
        let second = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer, state: PagerState| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {}", page)).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        build(&mut composer, first.clone());
        build(&mut composer, first.clone());
        // Swap the state at the same call site and scroll the NEW one.
        build(&mut composer, second.clone());
        build(&mut composer, second.clone());
        let step = second.layout_info().page_size;
        assert!(step > 0.0, "the new state resolved its page size");
        second.list_state().offset.set(step * 2.0);
        assert_eq!(
            second.current_page(),
            2,
            "the derive moved to the new state"
        );
    }

    /// The `ScrollState` the last frame assembled for the pager's list - the handle a wheel notch
    /// steps (it is what carries the `snap` spec and the page goal).
    fn pager_scroll_state(composer: &Composer) -> crate::modifier::ScrollState {
        composer
            .arena_nodes()
            .iter()
            .find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::HorizontalScroll { state, .. } => {
                        Some(state.clone())
                    }
                    _ => None,
                })
            })
            .expect("the pager attaches a horizontal scroll channel")
    }

    /// The wheel's page goal lives on the LIST, not on the handle a frame assembles.
    ///
    /// A scroll tick rebuilds the lazy list, and `build` assembles a new `ScrollState` each time. With
    /// the goal owned by that handle, the fresh one saw an empty goal and computed the next page from
    /// the LIVE offset - which, mid-spring, is inside the page the first notch is already heading to,
    /// so the second notch re-targeted the same page and the pager stopped responding. The goal is a
    /// field of `LazyListState` now and every assembled handle clones it.
    #[test]
    fn a_page_step_survives_the_lists_rebuild() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state.clone())
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        // The page size resolves from the viewport the PREVIOUS measure wrote, so the first frame
        // only records the viewport and the second resolves the Fill page.
        build(&mut composer);
        build(&mut composer);
        let page = state.layout_info().page_size;
        assert!(page > 0.0, "the first measure resolved the page size");

        let first = pager_scroll_state(&composer);
        assert!(
            std::sync::Arc::ptr_eq(&first.step_goal, &state.list_state().step_goal),
            "the frame's handle shares the list's page goal - that sharing is what outlives the frame"
        );
        assert!(first.snap_page(true, false), "page 0 -> 1 starts a step");
        assert_eq!(
            *state.list_state().step_goal.lock().unwrap_or_else(|e| e.into_inner()),
            Some(page),
            "the step's goal is recorded on the list, not on the frame's handle"
        );

        // A scroll tick moves the offset, and the next frame rebuilds the list - the real path a
        // second wheel notch takes while the first spring is still moving.
        for _ in 0..4 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        build(&mut composer);
        let second = pager_scroll_state(&composer);
        assert!(second.snap_page(true, false), "page 1 -> 2 starts the next step");
        assert_eq!(
            *state.list_state().step_goal.lock().unwrap_or_else(|e| e.into_inner()),
            Some(page * 2.0),
            "the notch after the rebuild steps from the goal the first one left, not from the live offset"
        );
        crate::animation::clear_all_animations();
    }

    /// `current_page_offset_fraction` is documented as `[-0.5, 0.5)`, a HALF-OPEN interval. Past the
    /// last reachable page the counter clamps to the last page, so an unclamped fraction would report
    /// exactly `0.5` - and `current_page_position()` half a page further than the pager can be.
    #[test]
    fn the_fraction_stays_below_half_a_page_past_the_end() {
        let state = PagerState::new(3);
        let mut composer = Composer::new();
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        compose_pager(&mut composer, state.clone(), composed.clone());
        // A second frame: the page size is resolved from the viewport the first measure wrote.
        compose_pager(&mut composer, state.clone(), composed.clone());
        let step = state.layout_info().page_size;
        assert!(step > 0.0, "the first measure resolved the page size");

        // Written like a wheel or an overshooting fling leaves it, with no measure to clamp it back.
        state.list_state().offset.set(step * 5.0);
        assert_eq!(state.current_page(), 2, "the counter clamps to the last page");
        assert!(
            state.current_page_offset_fraction() < 0.5,
            "the fraction must stay inside [-0.5, 0.5), got {}",
            state.current_page_offset_fraction()
        );
    }

    /// A pager's defaults are Compose's: `snapPositionalThreshold` 0.5, and material3's `DefaultEffects`
    /// spring on the settle.
    #[test]
    fn a_pager_defaults_to_the_material_effects_settle() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state.clone())
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        build(&mut composer);
        build(&mut composer);

        let spec = state.list_state().snap.peek().expect("a pager snaps");
        assert_eq!(
            spec.positional_threshold, 0.5,
            "snapPositionalThreshold defaults to 0.5 - the nearer page wins"
        );
        assert_eq!(spec.settle.stiffness, 1600.0, "material3 DefaultEffects stiffness");
        assert_eq!(spec.settle.damping_ratio, 1.0, "and its damping ratio");
    }

    /// The two knobs reach the snap spec the measure writes back - a Clean list would otherwise keep
    /// the previous feel.
    #[test]
    fn the_fling_knobs_reach_the_snap_spec() {
        let state = PagerState::new(5);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    HorizontalPager::new(state.clone())
                        .snap_positional_threshold(0.3)
                        .snap_animation_spec(crate::animation::SpringSpec {
                            stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_FAST,
                            damping_ratio: crate::animation::SpringSpec::DAMPING_RATIO_SPATIAL,
                            ..Default::default()
                        })
                        .max_pages_per_fling(3)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        build(&mut composer);
        build(&mut composer);

        let spec = state.list_state().snap.peek().expect("a pager snaps");
        assert!((spec.positional_threshold - 0.3).abs() < 1e-6, "the threshold reaches the spec");
        assert_eq!(spec.settle.stiffness, 3800.0, "and so does the settle spring");
        assert_eq!(spec.settle.damping_ratio, 0.9);
        assert_eq!(spec.max_pages_per_fling, 3, "and the page ceiling");
    }

    /// The point of the threshold: at 40% of a page, a slow release carries on with `0.3` and goes back
    /// with the default `0.5`. Asserted on the offset a real fling settles at, not only on the spec.
    #[test]
    fn a_low_threshold_lets_a_slow_release_turn_the_page() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        // Release at 40% of page 0 and report where the page landed, in pages.
        let land = |threshold: Option<f32>| -> f32 {
            let state = PagerState::new(5);
            let mut composer = Composer::new();
            let theme = crate::theme::ThemeColors::default_light();
            let build = |composer: &mut Composer| {
                composer.compose(|ctx| {
                    crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                        let mut pager = HorizontalPager::new(state.clone());
                        if let Some(threshold) = threshold {
                            pager = pager.snap_positional_threshold(threshold);
                        }
                        pager
                            .modifier(Modifier::new().fill_max_width().fill_max_height())
                            .build(ctx, |ctx, page| {
                                crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                            });
                    });
                });
                composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
            };
            build(&mut composer);
            build(&mut composer);

            let page = state.layout_info().page_size;
            assert!(page > 0.0, "the page size resolved");
            state.list_state().offset.set(page * 0.4);
            let scroll = pager_scroll_state(&composer);
            scroll.fling(0.0);
            for _ in 0..400 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(5));
                if !crate::animation::has_animation_for_state(scroll.offset.state_id()) {
                    break;
                }
            }
            scroll.offset.peek() / page
        };

        assert!(
            (land(Some(0.3)) - 1.0).abs() < 0.01,
            "a 0.3 threshold turns the page from 40%"
        );
        assert!(land(None).abs() < 0.01, "the default 0.5 sends a 40% release back");

        crate::animation::clear_all_animations();
    }

    /// Compose's `PagerSnapDistance.atMost(n)`: a hard flick carries up to `n` pages, and exactly one
    /// unless the caller raises the ceiling.
    #[test]
    fn max_pages_per_fling_lets_a_flick_carry_more_than_one_page() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        // Fling 4000 px/s from page 0 and report the page it lands on.
        let land = |max_pages: Option<usize>| -> f32 {
            let state = PagerState::new(6);
            let mut composer = Composer::new();
            let theme = crate::theme::ThemeColors::default_light();
            let build = |composer: &mut Composer| {
                composer.compose(|ctx| {
                    crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                        let mut pager = HorizontalPager::new(state.clone());
                        if let Some(pages) = max_pages {
                            pager = pager.max_pages_per_fling(pages);
                        }
                        pager
                            .modifier(Modifier::new().fill_max_width().fill_max_height())
                            .build(ctx, |ctx, page| {
                                crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                            });
                    });
                });
                composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
            };
            build(&mut composer);
            build(&mut composer);

            let page = state.layout_info().page_size;
            assert!(page > 0.0, "the page size resolved");
            let scroll = pager_scroll_state(&composer);
            scroll.fling(4000.0);
            for _ in 0..400 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(5));
                if !crate::animation::has_animation_for_state(scroll.offset.state_id()) {
                    break;
                }
            }
            scroll.offset.peek() / page
        };

        assert!(
            (land(None) - 1.0).abs() < 0.01,
            "the default ceiling is one page, however hard the flick"
        );
        assert!(
            (land(Some(3)) - 3.0).abs() < 0.01,
            "three pages when the ceiling allows them"
        );

        crate::animation::clear_all_animations();
    }

    /// A nested-scroll connection the test can count.
    struct CountingConnection {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl crate::nested_scroll::NestedScrollConnection for CountingConnection {
        fn on_pre_scroll(
            &self,
            _available: crate::nested_scroll::ScrollDelta,
            _source: crate::nested_scroll::NestedScrollSource,
        ) -> crate::nested_scroll::ScrollDelta {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            crate::nested_scroll::ScrollDelta::ZERO
        }
    }

    /// A strategy that lands on the end of the list, whatever it was asked.
    #[derive(Debug)]
    struct LandOnLast;

    impl crate::fling::FlingBehavior for LandOnLast {
        fn plan(&self, fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
            crate::fling::FlingPlan::spring(
                fling.limit,
                crate::animation::SpringSpec {
                    stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_DEFAULT,
                    ..Default::default()
                },
            )
        }
    }

    /// `fling_behavior` reaches the scroll state the frame assembles, and the strategy decides where a
    /// release lands: a pager with a custom strategy is not bound by page boundaries at all.
    #[test]
    fn a_pager_hands_its_fling_behavior_to_the_list() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let state = PagerState::new(4);
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer, with_behavior: bool| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let mut pager = HorizontalPager::new(state.clone());
                    if with_behavior {
                        pager = pager.fling_behavior(std::sync::Arc::new(LandOnLast));
                    }
                    pager
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };

        build(&mut composer, false);
        build(&mut composer, false);
        assert!(
            pager_scroll_state(&composer).fling_behavior.is_none(),
            "the built-in strategy is the default"
        );

        build(&mut composer, true);
        build(&mut composer, true);
        let scroll = pager_scroll_state(&composer);
        assert!(
            scroll.fling_behavior.is_some(),
            "the pager handed its strategy to the list's scroll state"
        );
        let limit = state.list_state().fling_limit.peek();
        assert!(limit > 0.0, "the list measured a limit");
        scroll.fling(50.0);
        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !scroll.is_scroll_in_progress.peek() {
                break;
            }
        }
        assert!(
            (scroll.offset.peek() - limit).abs() < 1.0,
            "the custom strategy landed at the end of the list ({}), not on a page boundary",
            scroll.offset.peek()
        );
        crate::animation::clear_all_animations();
    }

    /// Compose's `pageNestedScrollConnection` is a hook: the caller's connection rides on the list's own
    /// modifier (the node the drags land on), and nothing is attached unless somebody asks.
    #[test]
    fn a_page_nested_scroll_connection_rides_on_the_lists_modifier() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let state = PagerState::new(5);
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let mut composer = Composer::new();
        let theme = crate::theme::ThemeColors::default_light();
        let build = |composer: &mut Composer, with_connection: bool| {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let mut pager = HorizontalPager::new(state.clone());
                    if with_connection {
                        pager = pager.page_nested_scroll_connection(CountingConnection {
                            calls: calls.clone(),
                        });
                    }
                    pager
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .build(ctx, |ctx, page| {
                            crate::components::text::Text::new(format!("Page {page}")).build(ctx);
                        });
                });
            });
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        };

        build(&mut composer, false);
        assert!(
            composer
                .arena_nodes()
                .iter()
                .all(|node| node.modifier.nested_scroll_connection().is_none()),
            "a pager attaches no nested-scroll connection by default"
        );

        build(&mut composer, true);
        let attached = composer
            .arena_nodes()
            .iter()
            .find_map(|node| node.modifier.nested_scroll_connection())
            .expect("the pager attaches the caller's connection to its list");
        attached.on_pre_scroll(
            crate::nested_scroll::ScrollDelta::new(0.0, 10.0),
            crate::nested_scroll::NestedScrollSource::Drag,
        );
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "the attached connection IS the caller's"
        );
    }
