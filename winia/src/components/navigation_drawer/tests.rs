    use super::*;
    use crate::runtime::composer::Composer;

    fn comp() -> Composer {
        Composer::new()
    }

    /// Holds the global animation lock for the test's lifetime and, on drop, clears the
    /// animations of every state the test registered with [`AnimGuard::track`]. AGENTS.md
    /// requires the isolation of any test that pushes animations (`settle`/`open`/`close` all
    /// do), because the table is process-wide and an entry outliving the test keeps an `Arc`
    /// to its state alive.
    ///
    /// The cleanup is targeted rather than `clear_all_animations()`: that call is global, and
    /// not every animation test takes the lock (the existing `sheet_state` and
    /// `anchored_draggable` tests push without it), so a blanket clear would pull an
    /// animation out from under a test running on another thread.
    struct AnimGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        states: Vec<crate::runtime::state::StateId>,
    }

    impl AnimGuard {
        /// Clean up this state's animations when the guard drops.
        fn track(&mut self, state: &DrawerState) {
            self.states.push(state.offset_state().state_id());
        }
    }

    impl Drop for AnimGuard {
        fn drop(&mut self) {
            crate::animation::clear_animations_for_states(&self.states);
        }
    }

    fn anim_guard() -> AnimGuard {
        AnimGuard {
            _lock: crate::animation::tests::TEST_SERIAL
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
            states: Vec::new(),
        }
    }

    // ── anchors ──

    #[test]
    fn anchors_are_open_at_zero_and_closed_off_the_leading_edge() {
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Open), 0.0);
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Closed), -360.0);
        // RTL docks at the right, so the closed anchor flips sign.
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, true);
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Closed), 360.0);
    }

    #[test]
    fn first_layout_parks_the_closed_drawer_at_its_anchor() {
        let s = DrawerState::new(DrawerValue::Closed);
        assert!(s.current_offset().is_nan(), "uninitialized before the first anchors");
        s.update_anchors(360.0, false);
        assert_eq!(s.current_offset(), -360.0);
        assert!(s.is_closed());
        assert_eq!(s.progress(), 0.0, "parked closed reads progress 0");
    }

    #[test]
    fn open_close_move_the_target_and_progress() {
        let mut anim = anim_guard();
        let s = DrawerState::new(DrawerValue::Closed);
        anim.track(&s);
        s.update_anchors(360.0, false);
        s.open();
        assert_eq!(s.target_value(), DrawerValue::Open);
        assert_eq!(s.current_value(), DrawerValue::Open, "settle is eager; the tween follows");
        s.close();
        assert_eq!(s.target_value(), DrawerValue::Closed);
        assert!(s.is_closed());
    }

    #[test]
    fn progress_is_zero_closed_and_one_open() {
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        assert_eq!(s.progress(), 0.0);
        s.snap_to(DrawerValue::Open);
        assert_eq!(s.current_offset(), 0.0);
        assert_eq!(s.progress(), 1.0);
        s.drag_delta(-180.0);
        assert!((s.progress() - 0.5).abs() < 1e-3, "halfway out reads 0.5");
        assert!((s.peek_progress() - 0.5).abs() < 1e-3, "peek agrees with get");
    }

    #[test]
    fn progress_is_direction_agnostic() {
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, true);
        assert_eq!(s.progress(), 0.0, "RTL parked closed still reads 0");
        s.snap_to(DrawerValue::Open);
        assert_eq!(s.progress(), 1.0);
    }

    // ── gestures ──

    #[test]
    fn drag_toward_the_edge_closes_in_both_directions() {
        let mut anim = anim_guard();
        // LTR: the drawer lives at the left, so dragging LEFT closes it. The release
        // has no velocity, so the target is the nearest anchor — past the midpoint.
        let s = DrawerState::new(DrawerValue::Open);
        anim.track(&s);
        s.update_anchors(360.0, false);
        s.drag_delta(-200.0);
        assert_eq!(s.current_offset(), -200.0);
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Closed);
        // RTL: mirrored — dragging RIGHT closes it, with no sign flip at the call site.
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, true);
        s.drag_delta(200.0);
        assert_eq!(s.current_offset(), 200.0);
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Closed);
    }

    #[test]
    fn drag_cannot_overshoot_either_anchor() {
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, false);
        s.drag_delta(-9000.0);
        assert_eq!(s.current_offset(), -360.0, "clamped at the closed anchor");
        s.drag_delta(9000.0);
        assert_eq!(s.current_offset(), 0.0, "clamped at the open anchor");
    }

    #[test]
    fn a_slow_drag_settles_by_position_and_a_fast_one_by_direction() {
        let mut anim = anim_guard();
        // Slow release short of the midpoint → back to the anchor it came from.
        let s = DrawerState::new(DrawerValue::Open);
        anim.track(&s);
        s.update_anchors(360.0, false);
        s.drag_delta(-100.0); // 100 of 360 — nearer Open
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Open);
        // A deliberate flick short of the midpoint still closes: that is the whole point of a
        // velocity rule. Read at 1.5x density with an UNSCALED logical velocity, so a
        // threshold that converted the dp token to physical px/s (600 here instead of 400)
        // would spring back and fail.
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.5), || {
            let s = DrawerState::new(DrawerValue::Open);
            anim.track(&s);
            s.update_anchors(360.0, false);
            s.drag_delta(-40.0);
            assert_eq!(s.settle_with_velocity(-500.0), DrawerValue::Closed);
        });
    }

    #[test]
    fn the_drawer_threshold_is_higher_than_the_anchored_draggable_default() {
        let mut anim = anim_guard();
        // 200 logical px/s clears the 125dp/s `AnchoredDraggable` default but NOT the drawer's
        // own 400dp/s token, so a lazy drag that stops just short of the midpoint springs back
        // instead of flinging the drawer shut. Read at 1.5x density with an unscaled velocity:
        // the earlier version scaled the velocity by the density exactly as the buggy
        // threshold did, so the two cancelled out and the test could not see the bug.
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.5), || {
            let s = DrawerState::new(DrawerValue::Closed);
            anim.track(&s);
            s.update_anchors(360.0, false);
            s.snap_to(DrawerValue::Open);
            s.drag_delta(-170.0); // just short of the 180 midpoint
            assert_eq!(s.settle_with_velocity(-200.0), DrawerValue::Open);
        });
    }

    #[test]
    fn confirm_value_change_vetoes_a_gesture() {
        let mut anim = anim_guard();
        // The same gesture twice, once with a veto. Without it the drawer closes —
        // which is what makes the vetoed run's outcome meaningful.
        let mut outcomes = Vec::new();
        for veto in [false, true] {
            let mut s = DrawerState::new(DrawerValue::Open);
            anim.track(&s);
            s.update_anchors(360.0, false);
            if veto {
                s.set_confirm_value_change(|v| v != DrawerValue::Closed);
            }
            s.drag_delta(-300.0);
            let target = s.settle_with_velocity(0.0);
            outcomes.push((target, s.is_open(), s.is_animation_running()));
        }
        let (target, open, _) = outcomes[0];
        assert_eq!(target, DrawerValue::Closed);
        assert!(!open, "without a veto the flick closes the drawer");

        let (target, open, animating) = outcomes[1];
        assert_eq!(target, DrawerValue::Closed, "the gesture still computes Closed");
        assert!(open, "but the veto leaves the drawer parked Open");
        assert!(animating, "and the offset is driven back to the Open anchor");
    }

    // ── layout / presence ──

    #[test]
    fn width_is_the_token_clipped_to_the_window() {
        assert_eq!(resolve_sheet_width(360.0, 240.0, 800.0), 360.0);
        assert_eq!(resolve_sheet_width(360.0, 240.0, 300.0), 300.0, "narrow window");
        assert_eq!(
            resolve_sheet_width(200.0, 240.0, 800.0),
            240.0,
            "minimum wins over a small maximum (sizeIn semantics)"
        );
        assert_eq!(resolve_sheet_width(360.0, 240.0, 100.0), 100.0, "min clamped too");
    }

    #[test]
    fn resizing_keeps_the_open_drawer_flush_and_the_closed_one_off_screen() {
        // The window has to be NARROWER than the 360dp token for the sheet — and so the
        // closed anchor — to move at all: on a wider window the token is the cap.
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        drive_drawer_sized(&mut c, &state, 300.0, 600.0);
        assert_eq!(state.current_offset(), -300.0);
        assert_eq!(sheet_x(&c), -300.0, "closed on a 300-wide window");

        // Widen it: the closed drawer follows its anchor so it stays off-screen.
        drive_drawer_sized(&mut c, &state, 520.0, 600.0);
        assert_eq!(
            state.anchored_draggable().position_of(&DrawerValue::Closed),
            -360.0,
            "the anchor follows the new width (the token, now that it fits)"
        );
        assert_eq!(state.current_offset(), -360.0, "and the offset is re-aligned to it");
        assert_eq!(sheet_x(&c), -360.0, "so nothing of the closed drawer is on screen");

        // Open on the narrow window, then widen: it must stay flush with the edge.
        state.snap_to(DrawerValue::Open);
        drive_drawer_sized(&mut c, &state, 300.0, 600.0);
        assert_eq!(sheet_x(&c), 0.0, "open on the narrow window");
        drive_drawer_sized(&mut c, &state, 520.0, 600.0);
        assert_eq!(sheet_x(&c), 0.0, "still flush after the window widened");
    }

    #[test]
    fn a_resize_mid_drag_does_not_cancel_the_gesture() {
        // The offset belongs to the finger while a drag lasts, so a geometry change must
        // not re-align it — that is the P0's failure mode with a narrower trigger.
        crate::layout::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Open);
        let mut c = comp();
        drive_drawer(&mut c, &state);
        state.drag_delta(-120.0);
        assert!(state.anchored_draggable().is_dragging());
        drive_drawer_sized(&mut c, &state, 300.0, 600.0); // narrow enough to move the anchors
        assert_eq!(
            state.current_offset(),
            -120.0,
            "the drag keeps its offset; the resize must not teleport it to the anchor"
        );
        assert_eq!(sheet_x(&c), -120.0, "and the sheet stays under the finger");
    }

    #[test]
    fn a_resize_mid_settle_is_repaired_once_the_tween_finishes() {
        let mut anim = anim_guard();
        // The other half of the same ownership question: while a tween runs it owns the
        // offset, and the re-align must not fight it — but once it has finished (here at a
        // target captured before the resize) the settled drawer is put back on its anchor.
        // Start on a window narrower than the token, so the closed anchor really moves when
        // the window widens (300 -> 800 puts it at -300 -> -360).
        let state = DrawerState::new(DrawerValue::Open);
        anim.track(&state);
        let mut c = comp();
        drive_drawer_sized(&mut c, &state, 300.0, 600.0);
        assert_eq!(sheet_x(&c), 0.0, "open");
        assert_eq!(
            state.anchored_draggable().position_of(&DrawerValue::Closed),
            -300.0
        );

        state.close(); // tween 0 -> -300
        assert!(state.is_animation_running());
        let before = state.current_offset();
        drive_drawer_sized(&mut c, &state, 800.0, 600.0); // widens while the tween runs
        assert!(
            (state.current_offset() - before).abs() < 1.0,
            "a running tween owns the offset; composing must not snap it (was {before}, now {})",
            state.current_offset()
        );

        // Let the tween finish at its stale target, then compose at the same window: the
        // drawer is settled Closed but sits where the old geometry put it (-300 against an
        // anchor of -360), so the invariant repair puts it back on the anchor.
        std::thread::sleep(std::time::Duration::from_millis(400));
        while crate::animation::update_animations() {}
        drive_drawer_sized(&mut c, &state, 800.0, 600.0);
        assert_eq!(
            state.current_offset(),
            -360.0,
            "after the tween ends the closed drawer is re-aligned to the current anchor"
        );
        assert_eq!(sheet_x(&c), -360.0, "so it is fully off-screen again");
    }

    #[test]
    fn defaults_match_the_material_tokens() {
        assert_eq!(DrawerDefaults::MAXIMUM_DRAWER_WIDTH, Dp(360.0));
        assert_eq!(DrawerDefaults::MINIMUM_DRAWER_WIDTH, Dp(240.0));
        assert_eq!(DRAWER_ITEM_HEIGHT, 56.0, "ActiveIndicatorHeight");
        assert_eq!(DRAWER_ITEM_ICON_SIZE, 24.0, "IconSize");
        assert_eq!(DRAWER_CORNER_RADIUS, 16.0, "CornerLarge");
        // The sheet rounds the side facing the content and squares the window edge.
        assert_eq!(
            DrawerDefaults::shape(LayoutDirection::Ltr),
            Shape::RightRoundedRect { radius: DRAWER_CORNER_RADIUS },
            "LTR drawer docks left → rounded on the right"
        );
        assert_eq!(
            DrawerDefaults::shape(LayoutDirection::Rtl),
            Shape::LeftRoundedRect { radius: DRAWER_CORNER_RADIUS },
        );
    }

    // ── composition ──

    #[test]
    fn drawer_builds_content_sheet_and_a_live_anchor() {
        let mut c = comp();
        c.compose(|ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_content(|ctx| {
                ModalDrawerSheet::new(|ctx| {
                    NavigationDrawerItem::text_label("Inbox", true).build(ctx);
                })
                .build(ctx);
            })
            .build(ctx);
        });
        assert!(c.layout_root().is_some(), "the drawer composes a tree");
    }

    #[test]
    fn scrim_is_present_only_while_the_drawer_is_out() {
        // A closed drawer must compose NO scrim: a full-window click-catcher left in
        // the tree would eat every click on the page behind it.
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
            .build(ctx);
        });
        let closed_nodes = count_nodes(&c);
        assert!(c.layout_root().is_some());
        // Open it and compose again: the scrim adds a node.
        state.snap_to(DrawerValue::Open);
        let st2 = state.clone();
        c.compose(move |ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_state(st2.clone())
            .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
            .build(ctx);
        });
        let open_nodes = count_nodes(&c);
        assert_eq!(
            open_nodes,
            closed_nodes + 1,
            "exactly one node — the scrim — appears when the drawer opens \
             (closed {closed_nodes}, open {open_nodes})"
        );
    }

    fn count_nodes(c: &Composer) -> usize {
        c.arena_nodes().len()
    }

    /// Tag the frame helpers put on the drawer sheet, so tests find it by identity.
    const SHEET_TAG: &str = "drawer-sheet";

    /// Drawer content the frame helpers compose: a tagged sheet whose content fills it, so
    /// the surface is exactly as wide as the element the drawer positions (an empty sheet
    /// would be only its own padding wide, or the 240dp minimum).
    fn tagged_sheet() -> impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static {
        |ctx| {
            ModalDrawerSheet::new(|ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |_| {});
            })
            .modifier(Modifier::new().test_tag(SHEET_TAG))
            .build(ctx);
        }
    }

    /// The drawer sheet's place on screen and its width: `(x, w)` of the tagged node, with
    /// its ancestors' offsets summed. Identified by tag rather than by width — a width match
    /// also hits the app content, which is just as wide, and stops matching at all once the
    /// window is narrower than the drawer token. A node's own `position` is parent-relative,
    /// so the walk accumulates from the root.
    fn sheet_x_and_width(c: &Composer) -> (f32, f32) {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
        ) -> Option<(f32, f32)> {
            let node = &nodes[idx];
            let abs_x = x + node.position.x;
            if node.modifier.get_test_tag() == Some(SHEET_TAG) {
                return Some((abs_x, node.measured_size.width));
            }
            node.children
                .iter()
                .find_map(|&child| walk(nodes, child, abs_x))
        }
        let root = c.layout_root_idx().expect("the drawer was laid out");
        walk(c.arena_nodes(), root, 0.0)
            .expect("the drawer sheet is laid out somewhere in the tree")
    }

    fn sheet_x(c: &Composer) -> f32 {
        sheet_x_and_width(c).0
    }

    // The placement is the whole design: a layout offset (so hit testing follows the
    // slide) driven by the anchors (so one convention covers LTR and RTL), applied to
    // a leading-edge-aligned sheet. It is also the part a unit test of the state alone
    // cannot see, so it is exercised through a real compose + layout.
    fn drive_drawer(c: &mut Composer, state: &DrawerState) {
        drive_drawer_sized(c, state, 800.0, 600.0)
    }

    /// Compose + lay out one frame of the drawer with the window it reads set to `w`x`h`.
    /// The size has to be set *inside* the compose: a running Composer reads its own
    /// adaptive context, which was seeded from the fallback when the Composer was created,
    /// so writing the fallback afterwards would not reach it.
    fn drive_drawer_sized(c: &mut Composer, state: &DrawerState, w: f32, h: f32) {
        let st = state.clone();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(w, h);
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(tagged_sheet())
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, w, 0.0, h));
    }

    #[test]
    fn the_sheet_parks_off_the_edge_and_lands_flush_when_opened() {
        crate::layout::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        drive_drawer(&mut c, &state);
        assert_eq!(
            sheet_x(&c),
            -DRAWER_MAX_WIDTH,
            "closed: the sheet sits entirely off the leading edge, so it cannot be hit"
        );
        state.snap_to(DrawerValue::Open);
        drive_drawer(&mut c, &state);
        assert_eq!(sheet_x(&c), 0.0, "open: flush with the leading edge");
        state.snap_to(DrawerValue::Closed);
        drive_drawer(&mut c, &state);
        assert_eq!(sheet_x(&c), -DRAWER_MAX_WIDTH, "and closed again");
    }

    #[test]
    fn a_drag_survives_the_next_compose() {
        // Regression, and the one only an end-to-end frame can prove: `update_anchors`
        // runs on every compose, and it used to re-align the offset to the parked anchor
        // whenever no animation was running — and a live drag is not an animation — so
        // every drag frame was undone by the next compose and the drawer could not be
        // dragged at all. Every state-level test passed with that bug in place.
        crate::layout::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Open);
        let mut c = comp();
        drive_drawer(&mut c, &state);
        assert_eq!(sheet_x(&c), 0.0, "open");

        state.drag_delta(-200.0);
        assert_eq!(state.current_offset(), -200.0);
        drive_drawer(&mut c, &state); // the compose + layout that follows the move
        assert_eq!(
            state.current_offset(),
            -200.0,
            "the drag must survive the compose, not snap back to its anchor"
        );
        assert_eq!(
            sheet_x(&c),
            -200.0,
            "and the sheet is painted where the finger left it"
        );

        assert_eq!(state.settle_with_velocity(0.0), DrawerValue::Closed);
        assert!(state.is_closed(), "released past the midpoint: parked at Closed");
        drive_drawer(&mut c, &state);
        assert!(
            state.is_animation_running(),
            "and the offset is tweening to the closed anchor (it does not teleport there)"
        );
        assert_eq!(state.target_value(), DrawerValue::Closed);
    }

    #[test]
    fn the_sheet_width_is_logical_not_physical() {
        // A HiDPI window is a trap: the width token is compared against the window's
        // LOGICAL width, so converting it with the density (physical px) would make a
        // 360dp drawer 540 "logical" px and silently fill a 520-wide window. Measured
        // on the running demo at 1.5x before this was pinned.
        crate::layout::adaptive::set_window_size(520.0, 620.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.5), || {
            c.compose(move |ctx| {
                ModalNavigationDrawer::new(|ctx| {
                    crate::components::Text::new("page").build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(tagged_sheet())
                .build(ctx);
            });
            // The layout runs under the same density, as it does in the app.
            c.layout(crate::layout::Constraints::new(0.0, 520.0, 0.0, 620.0));
        });
        // Identity-based, not a width filter: a width match also hits the app content
        // (just as wide) and stops matching once the window is narrower than the token.
        let (x, w) = sheet_x_and_width(&c);
        assert_eq!(
            w, DRAWER_MAX_WIDTH,
            "the sheet is the token in LOGICAL px at 1.5x density, not the 540 the physical \
             value would give (which the window would clamp to its own 520)"
        );
        assert_eq!(
            x, -DRAWER_MAX_WIDTH,
            "and it is parked off the leading edge, at the closed anchor"
        );
        // `reset_window_size_state` clears the State but not the fallback cell that
        // `set_window_size` wrote outside a composer, so restore the size as well —
        // otherwise this thread reports a 520-wide window to whatever test runs next on it.
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(800.0, 600.0);
    }

    #[test]
    fn a_narrow_sheet_keeps_the_minimum_width() {
        // androidx's `DrawerSheet` carries `sizeIn(minWidth = MinimumDrawerWidth)`, so a
        // drawer whose content does not fill it is 240dp wide rather than collapsing to its
        // own padding — which is what an empty sheet measured (24px: the 12dp sides) before
        // the minimum was applied.
        let state = DrawerState::new(DrawerValue::Open);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(800.0, 600.0);
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(|ctx| {
                ModalDrawerSheet::new(|ctx| {
                    crate::components::Text::new("hi").build(ctx);
                })
                .modifier(Modifier::new().test_tag(SHEET_TAG))
                .build(ctx);
            })
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        let (x, w) = sheet_x_and_width(&c);
        assert_eq!(x, 0.0, "open");
        assert_eq!(
            w, DRAWER_MIN_WIDTH,
            "a sheet narrower than the minimum is {DRAWER_MIN_WIDTH}, not its own padding"
        );
    }

    #[test]
    fn the_rtl_drawer_parks_off_the_trailing_edge() {
        crate::layout::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            crate::theme::WiniaTheme::with_theme_and_direction(
                WiniaTheme::colors(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    ModalNavigationDrawer::new(|ctx| {
                        crate::components::Text::new("page").build(ctx);
                    })
                    .drawer_state(st.clone())
                    .drawer_content(tagged_sheet())
                    .build(ctx);
                },
            );
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        assert_eq!(
            sheet_x(&c),
            800.0,
            "closed in RTL: docked right, parked past the trailing edge"
        );
        state.snap_to(DrawerValue::Open);
        let st = state.clone();
        c.compose(move |ctx| {
            crate::theme::WiniaTheme::with_theme_and_direction(
                WiniaTheme::colors(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    ModalNavigationDrawer::new(|ctx| {
                        crate::components::Text::new("page").build(ctx);
                    })
                    .drawer_state(st.clone())
                    .drawer_content(tagged_sheet())
                    .build(ctx);
                },
            );
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        assert_eq!(sheet_x(&c), 440.0, "open in RTL: flush with the right edge");
    }

    /// Walks to a tagged node and answers its ABSOLUTE x (accumulating the parent chain, which is
    /// what `sheet_x_and_width` does for the sheet alone).
    fn x_of(c: &Composer, tag: &str) -> Option<f32> {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            tag: &str,
        ) -> Option<f32> {
            let node = &nodes[idx];
            let abs_x = x + node.position.x;
            if node.modifier.get_test_tag() == Some(tag) {
                return Some(abs_x);
            }
            node.children
                .iter()
                .find_map(|&child| walk(nodes, child, abs_x, tag))
        }
        let root = c.layout_root_idx()?;
        walk(c.arena_nodes(), root, 0.0, tag)
    }

    const PAGE_TAG: &str = "drawer-page";

    /// The three sheets differ ONLY in the defaults their kind resolves: `ModalContainerColor`
    /// (`SurfaceContainerLow`) with the rounded `ContainerShape`, against `StandardContainerColor`
    /// (`Surface`) with a RECTANGLE for both standard sheets (`NavigationDrawer.kt:646-650`,
    /// `:736-737`, `:821-822`).
    #[test]
    fn the_sheet_kind_resolves_the_upstream_defaults() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        assert_eq!(
            DrawerSheetKind::Modal.container_color(&theme),
            theme.surface_container_low,
            "ModalContainerColor"
        );
        assert_eq!(
            DrawerSheetKind::Standard.container_color(&theme),
            theme.surface,
            "StandardContainerColor"
        );
        assert_eq!(
            DrawerSheetKind::Modal.shape(LayoutDirection::Ltr),
            Shape::RightRoundedRect { radius: DRAWER_CORNER_RADIUS },
            "ContainerShape = CornerLargeEnd"
        );
        assert_eq!(
            DrawerSheetKind::Standard.shape(LayoutDirection::Ltr),
            Shape::Rectangle,
            "drawerShape = RectangleShape for BOTH standard sheets"
        );
        assert_eq!(
            NavigationDrawerItemDefaults::ITEM_PADDING,
            DRAWER_SHEET_HORIZONTAL_PADDING,
            "ItemPadding = 12dp"
        );
    }

    /// The badge's DEFAULT is the label colour — upstream's `colors()` has `selectedBadgeColor` /
    /// `unselectedBadgeColor` defaulting to the text colours (`NavigationDrawer.kt:1218-1219`), while
    /// winia used to hard-wire the ICON colour. The default is a struct question; that the item PAINTS
    /// it is the test below.
    #[test]
    fn the_default_badge_color_is_the_label_color() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let colors = NavigationDrawerItemDefaults::colors(&theme);
        assert_eq!(colors.badge(true), colors.text(true), "selected: the label color");
        assert_eq!(colors.badge(false), colors.text(false), "unselected: the label color");
    }

    /// The badge is painted in its OWN colour, not the icon's.
    ///
    /// This reads the colour THE RENDERER WILL PAINT (`ModifierElement::TextContent.color`), not the
    /// colour struct: comparing structs passed even while `build` painted the badge with
    /// `colors.icon(selected)` — which is the exact bug this replaced, since the two defaults agree.
    /// So the assertion overrides ONE of the two colours and looks at the pixels' source.
    #[test]
    fn the_badge_is_painted_in_the_badge_color() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let badge_color = Color::from_argb(255, 1, 2, 3);
        let mut colors = NavigationDrawerItemDefaults::colors(&theme);
        colors.selected_badge = badge_color;
        let mut c = comp();
        c.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme, ctx, |ctx| {
                NavigationDrawerItem::text_label("Inbox", true)
                    .colors(colors)
                    .badge(|ctx| {
                        crate::components::Text::new("7").build(ctx);
                    })
                    .build(ctx);
            });
        });
        c.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes = c.arena_nodes();
        let painted = |want: &str| -> Option<Color> {
            nodes.iter().find_map(|n| {
                n.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::TextContent { content, color, .. }
                        if content.as_str() == want =>
                    {
                        Some(*color)
                    }
                    _ => None,
                })
            })
        };
        assert_eq!(
            painted("7"),
            Some(badge_color),
            "the badge slot paints the BADGE colour"
        );
        assert_eq!(
            painted("Inbox"),
            Some(colors.text(true)),
            "and the label still paints its own"
        );
    }

    /// A DISMISSIBLE drawer pushes the content instead of covering it: upstream places the content at
    /// `sheetWidth + offset` and the sheet at `offset` (`NavigationDrawer.kt:578-587`), so an open
    /// drawer costs the content exactly the sheet's width and a closed one costs it nothing.
    #[test]
    fn a_dismissible_drawer_pushes_its_content_instead_of_covering_it() {
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let mut drive = |c: &mut Composer| {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(800.0, 600.0);
                DismissibleNavigationDrawer::new(|ctx| {
                    crate::components::Text::new("page")
                        .modifier(Modifier::new().test_tag(PAGE_TAG))
                        .build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(|ctx| {
                    DrawerSheet::dismissible(|_| {})
                        .modifier(Modifier::new().test_tag(SHEET_TAG))
                        .build(ctx);
                })
                .build(ctx);
            });
            c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        };
        drive(&mut c);
        assert_eq!(x_of(&c, SHEET_TAG), Some(-360.0), "closed: the sheet is off the leading edge");
        assert_eq!(x_of(&c, PAGE_TAG), Some(0.0), "and the content is NOT pushed");

        state.snap_to(DrawerValue::Open);
        drive(&mut c);
        assert_eq!(x_of(&c, SHEET_TAG), Some(0.0), "open: the sheet is flush with the edge");
        assert_eq!(
            x_of(&c, PAGE_TAG),
            Some(360.0),
            "and the content is pushed out by exactly the sheet's width, not covered"
        );
    }

    /// The same two endpoints under RTL, where the CLOSED anchor is `+width` instead of `-width`
    /// (`DrawerState::update_anchors`). The content's pushed distance is therefore a different
    /// expression per direction; the first version negated the LTR one, which parked the page at
    /// `-(360 + 360) = -720` while the drawer was closed — every element of the page off the left
    /// edge. Reported from the demo: Dismissible + RTL showed an empty screen.
    #[test]
    fn a_dismissible_drawer_pushes_its_content_the_right_way_under_rtl() {
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let mut drive = |c: &mut Composer| {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(800.0, 600.0);
                DismissibleNavigationDrawer::new(|ctx| {
                    crate::components::Text::new("page")
                        .modifier(Modifier::new().test_tag(PAGE_TAG))
                        .build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(|ctx| {
                    DrawerSheet::dismissible(|_| {})
                        .modifier(Modifier::new().test_tag(SHEET_TAG))
                        .build(ctx);
                })
                .build(ctx);
            });
            c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        };
        // The page is a HUGGING label, so under RTL it right-aligns inside its box: `x_of` reports the
        // label's absolute x, which is not 0 even when the box is. What matters is that the box is
        // inside the window when closed, and that opening moves it LEFT by the sheet's width.
        crate::layout::direction::provides(LayoutDirection::Rtl, || {
            drive(&mut c);
            let closed_page = x_of(&c, PAGE_TAG).expect("the page is laid out");
            // A LITERAL, not a range: the page is a 31dp label right-aligned in its 800dp box, so
            // 800 - 31 = 769. A range would pass a page parked at, say, 400 — which is what a wrong
            // push would produce — and this test exists to catch exactly that.
            assert!(
                (closed_page - (800.0 - 31.0)).abs() < 0.5,
                "closed under RTL the page sits at its right-aligned position, got x={closed_page}"
            );
            let closed_sheet = x_of(&c, SHEET_TAG).expect("the sheet is laid out");
            assert!(
                closed_sheet >= 800.0,
                "and the sheet parks off the trailing edge, got x={closed_sheet}"
            );

            state.snap_to(DrawerValue::Open);
            drive(&mut c);
            let open_page = x_of(&c, PAGE_TAG).expect("the page is laid out");
            assert!(
                (closed_page - open_page - 360.0).abs() < 0.5,
                "opening pushes the page LEFT by the sheet's width: closed {closed_page}, open {open_page}"
            );
            let open_sheet = x_of(&c, SHEET_TAG).expect("the sheet is laid out");
            assert!(
                open_sheet < closed_sheet,
                "and the sheet slides in from the right: closed {closed_sheet}, open {open_sheet}"
            );
        });
    }

    /// A PERMANENT drawer has no state at all: the sheet is always on screen and the content follows
    /// it (`Row { drawerContent(); Box { content() } }`, `NavigationDrawer.kt:618-623`).
    #[test]
    fn a_permanent_drawer_keeps_the_sheet_beside_the_content() {
        let mut c = comp();
        c.compose(|ctx| {
            crate::layout::adaptive::set_window_size(800.0, 600.0);
            PermanentNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page")
                    .modifier(Modifier::new().test_tag(PAGE_TAG))
                    .build(ctx);
            })
            .drawer_content(|ctx| {
                DrawerSheet::permanent(|_| {})
                    .modifier(Modifier::new().test_tag(SHEET_TAG))
                    .build(ctx);
            })
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        let (sheet_x, sheet_w) = sheet_x_and_width(&c);
        assert_eq!(sheet_x, 0.0, "the sheet is docked at the leading edge");
        assert!(
            (sheet_w - 240.0).abs() < 0.5,
            "and sits at MinimumDrawerWidth: the sheet sizes to its content, clamped to [240, 360] \
             (NavigationDrawer.kt:866), and this one's content is empty, got {sheet_w}"
        );
        assert!(x_of(&c, PAGE_TAG).unwrap_or(-1.0) >= sheet_w, "the content starts after it");
    }

    /// The sheet's own clamp is TWO-sided: `sizeIn(240, ContainerWidth)` on the sheet
    /// (`NavigationDrawer.kt:854`, `:866`). winia used to apply only the minimum, so a sheet whose
    /// content wanted more than 360dp simply grew — nothing else in the permanent path bounds it.
    #[test]
    fn the_sheet_stops_at_the_container_width_token() {
        let mut c = comp();
        c.compose(|ctx| {
            crate::layout::adaptive::set_window_size(900.0, 600.0);
            DrawerSheet::permanent(|ctx| {
                crate::components::Text::new("x".repeat(200))
                    .modifier(Modifier::new().test_tag(PAGE_TAG))
                    .build(ctx);
            })
            .modifier(Modifier::new().test_tag(SHEET_TAG))
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 900.0, 0.0, 600.0));
        let (_x, w) = sheet_x_and_width(&c);
        assert!(
            (w - 360.0).abs() < 0.5,
            "a wide sheet stops at ContainerWidth = 360, got {w}"
        );
    }
