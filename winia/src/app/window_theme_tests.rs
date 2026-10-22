    use super::PerWindow;
    use crate::theme::{ThemeColors, ThemeSpec, WindowTheme};

    /// Every window follows a system theme change — each from its OWN applied state.
    ///
    /// The "already applied" bookkeeping used to be a single process-wide flag, and each window's frame
    /// consumed it: whichever window rendered first flipped, and the rest kept the palette they had (on the
    /// platform path they did not even run a frame). Measured on a two-window app in the fixtures.
    #[test]
    fn a_theme_change_reaches_every_window() {
        let _serial = crate::theme::theme_mode_test_lock().lock().unwrap_or_else(|e| e.into_inner());
        let light = ThemeColors::default_light();
        let dark = ThemeColors::default_dark();

        crate::theme::set_system_dark_mode(Some(false));
        let mut first = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        let mut second = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        let mut pinned = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        // What a `Window` node passes: a cell whose intent follows the system.
        first.theme.cell = WindowTheme::new(ThemeSpec::Auto);
        second.theme.cell = WindowTheme::new(ThemeSpec::Auto);
        // A window whose application pinned a palette: its cell stays `Fixed`.
        pinned.theme.cell = WindowTheme::new(ThemeSpec::Fixed(light));
        first.refresh_theme();
        second.refresh_theme();
        assert_eq!(first.theme.applied.colors.background, light.background);
        assert_eq!(second.theme.applied.colors.background, light.background);

        crate::theme::set_system_dark_mode(Some(true));
        assert!(first.refresh_theme(), "the first window re-resolves");
        assert!(second.refresh_theme(), "so does the second — the state is per window");
        assert_eq!(first.theme.applied.colors.background, dark.background);
        assert_eq!(
            second.theme.applied.colors.background, dark.background,
            "the second window must not be left behind"
        );

        // A pinned window resolves the same palette again and has nothing to redraw.
        assert!(!pinned.refresh_theme(), "a pinned window has nothing to re-run");
        assert_eq!(pinned.theme.applied.colors.background, light.background);

        // An idle window does no work either.
        assert!(!first.refresh_theme(), "nothing changed since the last frame");

        // A published TYPE SCALE is a reason to re-run too, with no color change at all: components
        // resolved their type when they built.
        crate::theme::set_system_dark_mode(Some(false));
        let big = crate::theme::Typography { body_large: crate::text::TextStyle::new().font_size(24.0), ..Default::default() };
        assert!(first.theme.cell.publish(ThemeSpec::Auto, big, crate::layout::LayoutDirection::Ltr));
        assert!(first.refresh_theme(), "a type-scale change has to reach the tree");
        assert_eq!(first.theme.applied.typography.body_large.font_size, Some(crate::unit::TextUnit::Sp(crate::unit::Sp(24.0))));
        assert!(!first.refresh_theme(), "and then it is idle again");

        crate::theme::set_system_dark_mode(None);
    }
