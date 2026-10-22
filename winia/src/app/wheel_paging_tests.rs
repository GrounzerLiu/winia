    use super::{
        apply_wheel_to_target, wheel_kind_for_lines, wheel_page_step, wheel_scroll_target,
        WheelDelta, WheelKind, WheelPage, WheelPager, MAIN_TREE_WHEEL_SPACE,
    };
    use crate::layout::node::LayoutNode;
    use crate::fling::SnapInfo;
    use crate::modifier::{Modifier, ScrollState};
    use std::time::{Duration, Instant};

    /// One page of a pager, in logical px.
    const PAGE: f32 = 300.0;

    /// A scroll state that pages, with a limit far past the positions these tests touch.
    fn paging_state() -> ScrollState {
        let state = ScrollState::new();
        state.fling_limit.set(PAGE * 9.0);
        state.snap.set(Some(SnapInfo { step: PAGE, min_fling_velocity: 400.0, ..Default::default() }));
        state
    }

    fn vertical(state: ScrollState) -> LayoutNode {
        LayoutNode::leaf(Modifier::new().vertical_scroll(state).size(400.0, PAGE))
    }

    fn horizontal(state: ScrollState) -> LayoutNode {
        LayoutNode::leaf(Modifier::new().horizontal_scroll(state).size(PAGE, 400.0))
    }

    /// A notch is one deliberate page step, in either direction.
    #[test]
    fn a_notch_pages_on_its_own_delta() {
        let mut pager = WheelPager::default();
        let now = Instant::now();
        let key = (MAIN_TREE_WHEEL_SPACE, 7, true);
        assert_eq!(pager.page_step(key, -20.0, WheelKind::Notch, now), Some(true), "an upward notch steps forward");
        assert_eq!(pager.page_step(key, 20.0, WheelKind::Notch, now), Some(false), "a downward one steps back");
    }

    /// A precision touchpad's 3 px events must not each turn a page: twenty of them bank exactly one.
    #[test]
    fn tiny_pixel_events_bank_a_notch_before_they_page() {
        let mut pager = WheelPager::default();
        let now = Instant::now();
        let key = (MAIN_TREE_WHEEL_SPACE, 7, true);
        for event in 1..20 {
            assert_eq!(
                pager.page_step(key, -3.0, WheelKind::Pixel, now),
                None,
                "touchpad event {event} carries {} px, short of the {} px a page costs",
                event * 3,
                WheelPager::PIXEL_PAGE_TRAVEL
            );
        }
        assert_eq!(
            pager.page_step(key, -3.0, WheelKind::Pixel, now),
            Some(true),
            "the twentieth 3 px event completes a notch's worth of travel"
        );
        assert_eq!(
            pager.page_step(key, -3.0, WheelKind::Pixel, now),
            None,
            "and the bank starts over after a step"
        );
    }

    /// A notch arriving in the middle of a touchpad gesture pages at once and takes no banked travel
    /// with it, so the following touchpad events still have to earn their own page.
    #[test]
    fn a_notch_drops_travel_a_touchpad_banked() {
        let mut pager = WheelPager::default();
        let now = Instant::now();
        let key = (MAIN_TREE_WHEEL_SPACE, 7, true);
        assert_eq!(pager.page_step(key, -40.0, WheelKind::Pixel, now), None, "40 px is short of a page");
        assert_eq!(pager.page_step(key, -20.0, WheelKind::Notch, now), Some(true), "the notch itself pages");
        assert_eq!(
            pager.page_step(key, -20.0, WheelKind::Pixel, now),
            None,
            "the touchpad travel banked before the notch did not ride along with it"
        );
    }

    /// Half a notch one way and half a notch back is a scrub, not a page step.
    #[test]
    fn a_reversal_starts_the_travel_over() {
        let mut pager = WheelPager::default();
        let now = Instant::now();
        let key = (MAIN_TREE_WHEEL_SPACE, 7, true);
        assert_eq!(pager.page_step(key, -40.0, WheelKind::Pixel, now), None);
        assert_eq!(pager.page_step(key, 40.0, WheelKind::Pixel, now), None, "40 px back is still short of a page");
        assert_eq!(
            pager.page_step(key, 20.0, WheelKind::Pixel, now),
            Some(false),
            "and only the reversed travel counts toward the page"
        );
    }

    /// Travel banked before a pause is stale: a gesture that stopped is over.
    #[test]
    fn a_pause_ends_the_session() {
        let mut pager = WheelPager::default();
        let start = Instant::now();
        let key = (MAIN_TREE_WHEEL_SPACE, 7, true);
        assert_eq!(pager.page_step(key, -40.0, WheelKind::Pixel, start), None);
        let later = start + WheelPager::SESSION_GAP + Duration::from_millis(50);
        assert_eq!(
            pager.page_step(key, -40.0, WheelKind::Pixel, later),
            None,
            "the earlier 40 px does not count after a pause"
        );
        assert_eq!(
            pager.page_step(key, -40.0, WheelKind::Pixel, later + Duration::from_millis(10)),
            Some(true),
            "but the second gesture's own travel does"
        );
    }

    /// One event turns at most one page, even when the platform reports hundreds of pixels at once.
    #[test]
    fn one_oversized_event_turns_one_page_and_carries_nothing() {
        let mut pager = WheelPager::default();
        let now = Instant::now();
        let key = (MAIN_TREE_WHEEL_SPACE, 7, true);
        assert_eq!(pager.page_step(key, -900.0, WheelKind::Pixel, now), Some(true), "a flick still turns a page");
        assert_eq!(
            pager.page_step(key, -5.0, WheelKind::Pixel, now),
            None,
            "the ~840 px left over must not cascade more pages out of one event"
        );
    }

    /// A whole number of lines is a notch; a fractional one is a continuous stream and has to bank.
    #[test]
    fn fractional_lines_bank_instead_of_paging() {
        assert_eq!(wheel_kind_for_lines(0.0, 3.0), WheelKind::Notch);
        assert_eq!(wheel_kind_for_lines(0.0, -1.0), WheelKind::Notch);
        assert_eq!(
            wheel_kind_for_lines(0.0, 0.5),
            WheelKind::Pixel,
            "half a line is not a notch"
        );
        assert_eq!(wheel_kind_for_lines(0.25, 0.0), WheelKind::Pixel);
    }

    /// Two containers never share one gesture's travel, so a half-notch on one cannot page the other.
    #[test]
    fn another_container_banks_its_own_travel() {
        let mut pager = WheelPager::default();
        let now = Instant::now();
        assert_eq!(pager.page_step((MAIN_TREE_WHEEL_SPACE, 7, true), -40.0, WheelKind::Pixel, now), None);
        assert_eq!(
            pager.page_step((MAIN_TREE_WHEEL_SPACE, 8, true), -40.0, WheelKind::Pixel, now),
            None,
            "travel banked on one pager must not page another"
        );
        assert_eq!(pager.page_step((MAIN_TREE_WHEEL_SPACE, 8, true), -20.0, WheelKind::Pixel, now), Some(true));
    }

    /// A vertical wheel reaches a HorizontalPager (winia's deliberate deviation from Compose), and a
    /// plain horizontal row does not steal it from the vertical list around it.
    #[test]
    fn a_vertical_wheel_reaches_a_pager_but_not_a_plain_row() {
        let nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            vertical(paging_state()),
            horizontal(paging_state()),
        ];
        assert_eq!(
            wheel_scroll_target(&nodes, &[0, 1, 2], 0.0, -20.0),
            Some(2),
            "the horizontal pager is the innermost node that takes a plain wheel"
        );

        let nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            vertical(ScrollState::new()),
            horizontal(ScrollState::new()),
        ];
        assert_eq!(
            wheel_scroll_target(&nodes, &[0, 1, 2], 0.0, -20.0),
            Some(1),
            "a horizontal row that does not page leaves the vertical wheel to the list around it"
        );
    }

    /// The page step itself: a pager reached by a vertical wheel steps one page, and a container without
    /// a snap spec is left to the caller's pixel scroll.
    #[test]
    fn a_page_step_is_taken_only_by_a_paging_container() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let state = paging_state();
        let nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            horizontal(state.clone()),
        ];
        let mut pager = WheelPager::default();
        let wheel = WheelDelta { dx: 0.0, dy: -20.0, kind: WheelKind::Notch };
        assert_eq!(
            wheel_page_step(&nodes, 1, MAIN_TREE_WHEEL_SPACE, wheel, &mut pager, Instant::now()),
            WheelPage::Stepped,
            "a horizontal pager reached by a vertical wheel takes one page"
        );
        assert_eq!(
            *state.step_goal.lock().unwrap_or_else(|e| e.into_inner()),
            Some(PAGE),
            "and it steps forward by exactly one page"
        );

        let plain = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            vertical(ScrollState::new()),
        ];
        assert_eq!(
            wheel_page_step(&plain, 1, MAIN_TREE_WHEEL_SPACE, wheel, &mut pager, Instant::now()),
            WheelPage::NotPaged,
            "a list without a snap spec keeps pixel scrolling"
        );

        crate::animation::clear_all_animations();
    }

    /// A notch with no page left in that direction is swallowed, not pixel-scrolled: the pixel path
    /// cancels the settle in flight and nudges the offset off the page, which is the "stuck between two
    /// pages during a fast scroll" state (the next notch re-derives a page and appears to fix it).
    #[test]
    fn a_refused_page_step_neither_nudges_nor_cancels_the_settle() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        // Between the last two pages, with a step to the last one in flight.
        let state = paging_state();
        state.offset.set(PAGE * 8.5);
        assert!(state.snap_page(true, false), "a step to the last page starts");
        assert!(
            crate::animation::has_animation_for_state(state.offset.state_id()),
            "the settle is running"
        );

        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            vertical(state.clone()),
        ];
        let mut pager = WheelPager::default();
        let wheel = WheelDelta { dx: 0.0, dy: -20.0, kind: WheelKind::Notch };
        let before = state.offset.peek();
        let outcome = apply_wheel_to_target(
            &mut nodes,
            0,
            1,
            MAIN_TREE_WHEEL_SPACE,
            wheel,
            &mut pager,
            crate::unit::Density::from_density(1.0),
            Instant::now(),
        );

        assert!(
            !outcome.paged,
            "the pager refused the page, so it did not claim the event (an ancestor may)"
        );
        assert_eq!(
            state.offset.peek(),
            before,
            "a refused page step must not move the offset - that is the pixel path's job, and it would \
             strand the pager between two pages"
        );
        assert!(
            crate::animation::has_animation_for_state(state.offset.state_id()),
            "nor may it cancel the settle that is already running"
        );

        // Let it land, so the rest of the suite starts from a clean animation table.
        for _ in 0..400 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !state.is_scroll_in_progress.peek() {
                break;
            }
        }
        assert!(
            (state.offset.peek() - PAGE * 9.0).abs() < 0.5,
            "and the settle still lands on the last page, got {}",
            state.offset.peek()
        );
        crate::animation::clear_all_animations();
    }

    /// A refused page step must still reach the NESTED chain: a carousel inside a list cannot stop that
    /// list from scrolling just because it ran out of pages.
    #[test]
    fn a_refused_page_step_still_reaches_an_ancestors_nested_chain() {
        use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource, ScrollDelta};

        #[derive(Debug)]
        struct Recording {
            seen: std::sync::Arc<std::sync::Mutex<Vec<f32>>>,
        }
        impl NestedScrollConnection for Recording {
            fn on_pre_scroll(&self, available: ScrollDelta, _source: NestedScrollSource) -> ScrollDelta {
                self.seen.lock().unwrap().push(available.y);
                ScrollDelta::ZERO
            }
        }

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let state = paging_state();
        state.offset.set(PAGE * 9.0);
        let mut nodes = vec![
            LayoutNode::leaf(
                Modifier::new()
                    .size(400.0, 600.0)
                    .nested_scroll(Recording { seen: seen.clone() }),
            ),
            vertical(state.clone()),
        ];
        // The dispatch walks from the root to the target, so the two have to be linked.
        nodes[0].children.push(1);
        let mut pager = WheelPager::default();
        let wheel = WheelDelta { dx: 0.0, dy: -20.0, kind: WheelKind::Notch };
        apply_wheel_to_target(
            &mut nodes,
            0,
            1,
            MAIN_TREE_WHEEL_SPACE,
            wheel,
            &mut pager,
            crate::unit::Density::from_density(1.0),
            Instant::now(),
        );

        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[-20.0],
            "the ancestor's pre-scroll still sees the wheel the pager could not use"
        );
        assert_eq!(state.offset.peek(), PAGE * 9.0, "and the pager itself did not move");
    }

    /// A touchpad event on a pager is swallowed while it banks: a page either turns or nothing moves,
    /// and pixel-scrolling a pager would leave it between two pages.
    #[test]
    fn a_banking_touchpad_event_is_held_not_pixel_scrolled() {
        let mut pager = WheelPager::default();
        let nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            vertical(paging_state()),
        ];
        let wheel = WheelDelta { dx: 0.0, dy: -3.0, kind: WheelKind::Pixel };
        assert_eq!(
            wheel_page_step(&nodes, 1, MAIN_TREE_WHEEL_SPACE, wheel, &mut pager, Instant::now()),
            WheelPage::Held
        );
    }
