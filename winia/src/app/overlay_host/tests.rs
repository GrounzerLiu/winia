    use super::{closing_overlay_is_done, CLOSING_DEADLINE};
    use super::OverlayWindow;
    use crate::app::PerWindow;
    use crate::runtime::composer::Composer;
    use crate::overlay::{OverlayAnimSpec, OverlayDesc, PopupPosition};
    use crate::theme::ThemeColors;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// A closing overlay is dropped when its fade finished — and, failing that, past the deadline. The
    /// case in between is the reason the deadline exists: a tween that never reports done kept the overlay
    /// (and its modal scrim) rendering forever, which a user hit as a stuck dim layer over the page with
    /// the sheet already gone.
    #[test]
    fn a_closing_overlay_waits_for_its_fade_but_not_forever() {
        // The deadline has to stay in the same order of magnitude as the exit specs (a few hundred
        // milliseconds): the test below derives its instants from it, so a constant of minutes would pass
        // the test and still leave a stuck scrim on screen for minutes.
        assert!(
            CLOSING_DEADLINE <= Duration::from_millis(2000),
            "CLOSING_DEADLINE is a safety net, not a grace period: {CLOSING_DEADLINE:?}"
        );
        let now = Instant::now();
        assert!(closing_overlay_is_done(None, None, now), "no progress channel: nothing to wait for");
        assert!(closing_overlay_is_done(Some(0.0), None, now), "the fade reached its end");
        assert!(
            !closing_overlay_is_done(Some(1.0), Some(now), now),
            "a fade in flight is not dropped"
        );
        assert!(
            !closing_overlay_is_done(Some(1.0), Some(now - CLOSING_DEADLINE / 2), now),
            "…and not before its time"
        );
        assert!(
            closing_overlay_is_done(Some(1.0), Some(now - CLOSING_DEADLINE - Duration::from_millis(1)), now),
            "…and it is dropped once the deadline passes"
        );
    }

    /// An overlay that stops closing has to be VISIBLE again, not merely non-closing: the exit tween
    /// left its show-progress at 0, and that number is what the render path draws with (while the hit
    /// test never looks at it). Left there, the overlay is an invisible panel that still swallows
    /// clicks in its rect and still owns the keyboard — the state a close-then-reopen used to land in,
    /// and the state the deadline's "hand it back its interactivity" arm would have landed in too.
    #[test]
    fn an_overlay_that_stops_closing_is_shown_again() {
        let desc = |enter: Option<OverlayAnimSpec>, exit: Option<OverlayAnimSpec>| OverlayDesc {
            id: 7,
            anchor_slot: None,
            direction: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            dismiss_defers_to_content: false,
            click_passthrough: false,
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: None,
            enter_anim: enter,
            exit_anim: exit,
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };
        let fade = || OverlayAnimSpec::fade_only(Duration::from_millis(50));

        // Nothing to animate back up (no enter spec): the value must land on 1 outright.
        let mut ov = OverlayWindow::new_with_composer(desc(None, Some(fade())), Composer::new());
        let progress = ov.progress.clone().expect("the exit spec gives it a progress channel");
        progress.as_raw().set_backchannel(0.0);
        ov.closing = true;
        ov.closing_since = Some(Instant::now());
        ov.resume_after_close();
        assert!(!ov.closing, "resuming undoes the close");
        assert_eq!(
            ov.progress.as_ref().map(|p| p.peek()),
            Some(1.0),
            "with no enter animation the panel is shown outright, not left at the exit tween's 0"
        );

        // With one, the 0->1 enter is scheduled again — the push is the observable proof.
        let _serial = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut ov = OverlayWindow::new_with_composer(desc(Some(fade()), Some(fade())), Composer::new());
        let progress = ov.progress.clone().expect("an animated overlay has a progress channel");
        progress.as_raw().set_backchannel(0.0);
        ov.closing = true;
        ov.closing_since = Some(Instant::now());
        ov.resume_after_close();
        assert!(
            crate::animation::has_animation_for_state(progress.state_id()),
            "resuming an overlay that was fading out schedules its enter animation again"
        );
        crate::animation::clear_all_animations();
    }

    /// `DialogProperties.dismissOnBackPress = false` still SWALLOWS Escape — the page behind the scrim
    /// must not react to a key the dialog kept — but does not close the dialog. That swallow is winia's
    /// own, not Compose's: see the note on [`PerWindow::escape_key`].
    #[test]
    fn escape_respects_dismiss_on_back_press() {
        let desc = |dismiss_on_back_press: bool| OverlayDesc {
            id: 9,
            anchor_slot: None,
            direction: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press,
            dismiss_defers_to_content: false,
            click_passthrough: false,
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: Some(Arc::new(|| {})),
            enter_anim: None,
            exit_anim: Some(OverlayAnimSpec::fade_only(Duration::from_millis(50))),
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };

        // True (the default): the escape starts the close.
        let light = ThemeColors::default_light();
        let mut kept = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        kept.overlay.layers
            .push(OverlayWindow::new_with_composer(desc(true), Composer::new()));
        assert!(kept.escape_key(), "escape is consumed");
        assert!(
            kept.overlay.layers[0].closing,
            "and it closes the dialog that opted in"
        );

        // False: consumed, but nothing closes.
        let light = ThemeColors::default_light();
        let mut stubborn = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
        stubborn
            .overlay
            .layers
            .push(OverlayWindow::new_with_composer(desc(false), Composer::new()));
        assert!(
            stubborn.escape_key(),
            "escape is still SWALLOWED so the page behind does not see it"
        );
        assert!(
            !stubborn.overlay.layers[0].closing,
            "but the dialog that opted out stays open"
        );
    }

    /// A content that owns its exit gets the dismissal request and nothing else: `on_dismiss` runs, and the
    /// host does NOT start its own exit. That is the difference a bottom sheet needs — the panel slides down
    /// first and flips the caller's `visible`, which is what removes the overlay. Without the flag the host
    /// both calls the handler and fades, so the sheet fades out mid-slide.
    #[test]
    fn a_dismissal_request_that_the_content_owns_does_not_start_the_hosts_fade() {
        let desc = |defers: bool, called: Arc<std::sync::atomic::AtomicUsize>| OverlayDesc {
            id: 11,
            anchor_slot: None,
            direction: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            dismiss_defers_to_content: defers,
            click_passthrough: false,
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: Some(Arc::new(move || {
                called.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })),
            enter_anim: None,
            exit_anim: Some(OverlayAnimSpec::fade_only(Duration::from_millis(50))),
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };

        for (defers, want_closing) in [(true, false), (false, true)] {
            let called = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let light = ThemeColors::default_light();
            let mut pw = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, light);
            pw.overlay
                .layers
                .push(OverlayWindow::new_with_composer(
                    desc(defers, called.clone()),
                    Composer::new(),
                ));
            assert!(pw.escape_key(), "escape is consumed");
            assert_eq!(
                called.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "the handler runs either way (defers = {defers})"
            );
            assert_eq!(
                pw.overlay.layers[0].closing, want_closing,
                "a content that owns its exit is left open (defers = {defers})"
            );
        }
    }

    /// An overlay is its own composer, but it still belongs to the window that hosts it: a clock
    /// handle a component captures inside an overlay must wait on that window's frames.
    #[test]
    fn an_overlay_composer_inherits_its_windows_clock_identity() {
        let desc = OverlayDesc {
            id: 21,
            anchor_slot: None,
            direction: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: true,
            focus_scope: true,
            dismiss_on_outside: true,
            dismiss_on_back_press: true,
            dismiss_defers_to_content: false,
            click_passthrough: false,
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: None,
            enter_anim: None,
            exit_anim: None,
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };

        let mut pw = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, ThemeColors::default_light());
        pw.composer.set_window_id(0xD00D);
        let registry = crate::runtime::saveable::SaveableStateRegistry::new(Default::default());
        pw.composer.set_saveable_registry(registry.clone());
        pw.composer.compose(|ctx: &mut crate::runtime::composer::ComposeCtx| {
            ctx.open_overlay(desc);
        });
        super::sync_overlays(&mut pw, true);

        let overlay = pw.overlay.layers.first().expect("the overlay was created");
        assert_eq!(
            overlay.composer.window_id(),
            0xD00D,
            "an overlay follows the window that hosts it"
        );
        assert!(
            overlay.composer.saveable_registry().is_some(),
            "and saves into the same registry: remember_saveable in overlay content belongs to the page"
        );
    }
