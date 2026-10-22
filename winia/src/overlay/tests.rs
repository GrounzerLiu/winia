    use super::*;

    /// A menu item must carry the ripple AND the "no focus ring" marker.
    ///
    /// material3's item is `clickable(..., indication = ripple(true))` and marks focus with a state layer,
    /// not with an outline. winia draws a focus ring around any focused node by default (`render.rs`, gated
    /// on `ModifierElement::NoFocusRing`), which across a menu reads as a divider between rows — so the item
    /// opts out, and the highlight it keeps comes from the ripple element's state layer (`hover + focus`).
    ///
    /// Teeth: dropping `no_focus_ring()` from the item fails the second assertion. That one is structural,
    /// not pixel-based, on purpose: the ring is a ~1px band just OUTSIDE the item's rect (measured:
    /// physical x=23 against the item's edge at 24 is `(197,193,199)` while everything inside is the
    /// `(217,211,219)` state layer), so a pixel probe of it is a rounding artefact away from lying.

    /// The reveal clip height must keep "settled, no clip" distinct from "clipped to zero height".
    ///
    /// Teeth: returning `Some(0.0)` instead of `None` at `reveal == 1.0` (the mistake this function was
    /// extracted from — a fully open docked dropdown reported `painted_h=0` in the render trace while its
    /// layout height was 280) fails the settled case below.
    #[test]
    fn reveal_clip_height_distinguishes_no_clip_from_zero() {
        // Settled: fully revealed and not sliding -> NO clip.
        assert_eq!(
            overlay_reveal_clip_height(280.0, 0.0, 1.0),
            None,
            "a settled overlay is not clipped at all (0 here would read as an invisible panel)"
        );
        // Revealing: proportional height.
        assert_eq!(overlay_reveal_clip_height(280.0, 0.0, 0.5), Some(140.0));
        // Just started: effectively zero height, but it is a real clip.
        assert_eq!(overlay_reveal_clip_height(280.0, 0.0, 0.0), Some(0.0));
        // Sliding: the translation moves the panel, so the clip is the FULL height regardless of reveal.
        assert_eq!(
            overlay_reveal_clip_height(280.0, -0.5, 0.2),
            Some(280.0),
            "a slide clips to its full height (the offset, not a reveal, is what animates)"
        );
        // Overshoot (a spring past 1.0) must not produce a clip taller than the panel.
        assert_eq!(overlay_reveal_clip_height(280.0, 0.0, 1.2), None);
    }

    /// A panel that grows out of an anchor runs `lerp(anchor, 0, progress)` on BOTH axes, so it starts at the
    /// anchor's corner and reaches the window's corner exactly when the animation finishes.
    ///
    /// Teeth: dropping the second axis (the first version slid only Y) or using the wrong endpoint fails the
    /// assertions below — the earlier Y-only version left a permanent gap equal to the anchor's own top.
    #[test]
    fn anchor_slide_interpolates_both_axes_from_the_anchor_to_the_window_corner() {
        // An anchor 240 wide at x=40, and 56 tall at y=69 (the demo's bar).
        let (ax, ay) = (40.0f32, 69.0f32);

        // progress 0: exactly at the anchor.
        assert_eq!(anchor_slide_lerp(ax, 0.0), ax, "starts at the anchor's x");
        assert_eq!(anchor_slide_lerp(ay, 0.0), ay, "starts at the anchor's y");

        // progress 1: at the window corner, on BOTH axes.
        assert_eq!(anchor_slide_lerp(ax, 1.0), 0.0, "ends at the window's left edge");
        assert_eq!(anchor_slide_lerp(ay, 1.0), 0.0, "ends at the window's top edge");

        // Half way on each axis, and the Y axis really does move (the earlier Y-only version is what this
        // catches: it must not stay at the anchor's top).
        assert_eq!(anchor_slide_lerp(ax, 0.5), ax / 2.0, "half way in x");
        assert_eq!(anchor_slide_lerp(ay, 0.5), ay / 2.0, "half way in y");

        // Out-of-range progress is clamped, so a spring overshoot cannot push the panel past the window.
        assert_eq!(anchor_slide_lerp(ay, 1.5), 0.0, "overshoot clamps to the window edge");
        assert_eq!(anchor_slide_lerp(ay, -0.5), ay, "undershoot clamps to the anchor");
    }

    /// `AnchorSlide` exposes the clamped progress the layout pass interpolates with.
    #[test]
    fn anchor_slide_clamps_its_progress() {
        let over = AnchorSlide::new(std::sync::Arc::new(|| 1.4));
        assert_eq!(over.progress(), 1.0);
        let under = AnchorSlide::new(std::sync::Arc::new(|| -0.2));
        assert_eq!(under.progress(), 0.0);
        let mid = AnchorSlide::new(std::sync::Arc::new(|| 0.25));
        assert_eq!(mid.progress(), 0.25);
    }

    /// The four direction-resolved positions, with the absolute ones as their control: `*Start`/`*End` follow
    /// the layout direction, and a variant named `Left` stays left in BOTH directions, so migrating a caller is
    /// an explicit choice rather than a silent behaviour change.
    ///
    /// Teeth: ignoring `direction` (returning the LTR result in both) fails every RTL assertion below.
    #[test]
    fn start_and_end_positions_resolve_against_the_layout_direction() {
        use crate::layout::LayoutDirection::{Ltr, Rtl};
        use PopupPosition as P;
        let size = (80.0, 30.0);
        let window = (400.0, 400.0);
        let anchor = Some((100.0, 50.0, 60.0, 40.0));

        // Anchored: the popup's start edge sits on the anchor's start edge.
        assert_eq!(
            resolve_popup_position(P::BottomStart, Ltr, anchor, size, window),
            (100.0, 90.0),
            "LTR: left edges meet, below the anchor"
        );
        assert_eq!(
            resolve_popup_position(P::BottomStart, Rtl, anchor, size, window),
            (80.0, 90.0),
            "RTL: right edges meet (100 + 60 - 80), below the anchor"
        );
        assert_eq!(
            resolve_popup_position(P::BottomEnd, Rtl, anchor, size, window),
            (100.0, 90.0),
            "and `End` in RTL is the mirrored candidate"
        );
        assert_eq!(
            resolve_popup_position(P::TopStart, Rtl, anchor, size, window),
            (80.0, 20.0),
            "`Top*` keeps the vertical side while mirroring horizontally"
        );

        // Window-aligned: the same question one level up.
        assert_eq!(resolve_popup_position(P::BottomStart, Ltr, None, size, window), (0.0, 370.0));
        assert_eq!(resolve_popup_position(P::BottomStart, Rtl, None, size, window), (320.0, 370.0));
        assert_eq!(resolve_popup_position(P::TopStart, Rtl, None, size, window), (320.0, 0.0));

        // The absolute corners are the control: they do not mirror, which is the whole reason the resolved
        // variants exist instead of making `Left` direction-dependent.
        assert_eq!(
            resolve_popup_position(P::BottomLeft, Rtl, anchor, size, window),
            (100.0, 90.0),
            "a corner named Left stays left under RTL"
        );
        assert_eq!(
            resolve_popup_position(P::BottomRight, Rtl, anchor, size, window),
            (80.0, 90.0),
            "and Right stays right"
        );
        assert_eq!(resolve_popup_position(P::TopLeft, Rtl, None, size, window), (0.0, 0.0));
        assert_eq!(resolve_popup_position(P::Center, Rtl, anchor, size, window), (160.0, 185.0));

        // No anchor at all falls back to window alignment for every variant, including the resolved ones.
        assert_eq!(resolve_popup_position(P::BottomLeft, Ltr, None, size, window), (0.0, 370.0));
        assert_eq!(resolve_popup_position(P::Center, Ltr, None, size, window), (160.0, 185.0));
    }

