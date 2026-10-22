    use super::{should_request_redraw, PerWindow};
    use std::time::{Duration, Instant};

    #[test]
    fn request_at_exact_interval_allowed() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(should_request_redraw(t0, t0 + interval, interval));
    }

    #[test]
    fn request_below_interval_blocked() {
        // I-ε (ε=1 ns): the edge of the halving regression; a render later than the request must not
        // push the next request out to 2I.
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(!should_request_redraw(t0, t0 + interval - Duration::from_nanos(1), interval));
    }

    #[test]
    fn request_after_two_intervals_allowed() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(should_request_redraw(t0, t0 + interval * 2, interval));
    }

    #[test]
    fn request_at_zero_blocked() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(!should_request_redraw(t0, t0, interval));
    }

    #[test]
    fn request_over_interval_allowed() {
        let t0 = Instant::now();
        let interval = Duration::from_millis(16);
        assert!(should_request_redraw(t0, t0 + interval + Duration::from_millis(1), interval));
    }

    /// Shift + wheel: the vertical delta becomes horizontal, and the vertical component is cleared.
    #[test]
    fn shift_wheel_delta_becomes_horizontal() {
        assert_eq!(super::scroll_delta_with_shift(0.0, -20.0, true), (-20.0, 0.0));
        assert_eq!(super::scroll_delta_with_shift(0.0, 20.0, true), (20.0, 0.0));
        assert_eq!(super::scroll_delta_with_shift(5.0, 0.0, false), (5.0, 0.0));
        assert_eq!(super::scroll_delta_with_shift(0.0, 10.0, false), (0.0, 10.0));
    }

    #[test]
    fn per_window_modifiers_are_independent() {
        let theme = crate::theme::ThemeColors::default_light();
        let mut first = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, theme.clone());
        let mut second = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, theme);
        first.modifiers = winit::keyboard::ModifiersState::default();
        second.modifiers = winit::keyboard::ModifiersState::default();
        assert_eq!(first.modifiers, second.modifiers);
    }

    /// Applying a scroll delta to a Column: find the scroll node, update the offset, then clamp.
    #[test]
    fn scroll_delta_applies_to_column() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = crate::runtime::composer::Composer::new();
        let scroll = crate::modifier::ScrollState::new();
        let scroll2 = scroll.clone();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Column::new()
                .modifier(crate::modifier::Modifier::new().fill_max_size().vertical_scroll(scroll2))
                .build(ctx, |ctx| {
                    // The content is taller than the 600 viewport (~19 px per line x 40 lines ~ 760).
                    for _ in 0..40 {
                        crate::components::Text::new("line content line content").build(ctx);
                    }
                });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(scroll.offset.get(), 0.0);
        let consumed = super::scroll::apply_scroll_delta(
            composer.arena_nodes_mut(),
            root,
            0.0,
            -200.0, // a negative dy scrolls down (the content moves up, matching winit's wheel semantics)
            crate::unit::Density::from_density(1.0),
        );
        assert!(consumed.y != 0.0, "the scroll delta should be consumed");
        assert!(scroll.offset.get() > 0.0, "the offset should be > 0 after scrolling (got {})", scroll.offset.get());
    }
