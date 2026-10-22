#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
    /// Below the anchor, aligned to the anchor's **start** edge — the leading edge in the ambient layout
    /// direction, so `BottomStart` is the left edge in LTR and the right edge in RTL. material3 anchors with
    /// `Alignment.TopStart`-style values, which resolve against the direction; the absolute corners here cannot
    /// express that, and making a variant named `Left` mirror would be a lie, so the resolved forms are their
    /// own variants.
    BottomStart,
    /// Below the anchor, aligned to its **end** edge (trailing edge: right in LTR, left in RTL).
    BottomEnd,
    /// Above the anchor, aligned to its start edge.
    TopStart,
    /// Above the anchor, aligned to its end edge.
    TopEnd,
}

/// Where a popup or overlay goes, resolved from its [`PopupPosition`] — the one place this arithmetic lives.
///
/// `anchor` is the anchor's rect `(x, y, width, height)` when the popup is anchored and the anchor was found;
/// `None` means the popup is window-aligned. `direction` is the direction the anchor resolved to, which is
/// what the four `*Start`/`*End` variants consult; the seven absolute variants ignore it, because a name that
/// says `Left` has to keep meaning left in both directions.
///
/// The anchored `fit_around_anchor` candidate walk is deliberately NOT here: it needs material3's candidate
/// lists (`internal/MenuPosition.kt`) and lives with the menu (`dropdown_menu::dropdown_menu_position`).
pub(crate) fn resolve_popup_position(
    position: PopupPosition,
    direction: crate::layout::LayoutDirection,
    anchor: Option<(f32, f32, f32, f32)>,
    size: (f32, f32),
    window: (f32, f32),
) -> (f32, f32) {
    let (w, h) = window;
    let rtl = direction == crate::layout::LayoutDirection::Rtl;
    let window_aligned = |p: PopupPosition| -> (f32, f32) {
        match p {
            PopupPosition::Center => ((w - size.0) / 2.0, (h - size.1) / 2.0),
            PopupPosition::TopLeft => (0.0, 0.0),
            PopupPosition::TopCenter => ((w - size.0) / 2.0, 0.0),
            PopupPosition::TopRight => (w - size.0, 0.0),
            PopupPosition::BottomLeft => (0.0, h - size.1),
            PopupPosition::BottomCenter => ((w - size.0) / 2.0, h - size.1),
            PopupPosition::BottomRight => (w - size.0, h - size.1),
            // Start is the leading edge: the window's left in LTR, its right in RTL.
            PopupPosition::TopStart => (if rtl { w - size.0 } else { 0.0 }, 0.0),
            PopupPosition::TopEnd => (if rtl { 0.0 } else { w - size.0 }, 0.0),
            PopupPosition::BottomStart => (if rtl { w - size.0 } else { 0.0 }, h - size.1),
            PopupPosition::BottomEnd => (if rtl { 0.0 } else { w - size.0 }, h - size.1),
        }
    };
    let Some((ax, ay, aw, ah)) = anchor else {
        return window_aligned(position);
    };
    match position {
        PopupPosition::BottomLeft => (ax, ay + ah),
        PopupPosition::BottomCenter => (ax + (aw - size.0) / 2.0, ay + ah),
        PopupPosition::BottomRight => (ax + aw - size.0, ay + ah),
        PopupPosition::TopLeft => (ax, ay - size.1),
        PopupPosition::TopCenter => (ax + (aw - size.0) / 2.0, ay - size.1),
        PopupPosition::TopRight => (ax + aw - size.0, ay - size.1),
        PopupPosition::Center => window_aligned(PopupPosition::Center),
        // Start/end put the popup's matching edge on the anchor's: in RTL the popup's right edge sits on the
        // anchor's right edge, which is what "below the field, flush with its start" means there.
        PopupPosition::BottomStart => (if rtl { ax + aw - size.0 } else { ax }, ay + ah),
        PopupPosition::BottomEnd => (if rtl { ax } else { ax + aw - size.0 }, ay + ah),
        PopupPosition::TopStart => (if rtl { ax + aw - size.0 } else { ax }, ay - size.1),
        PopupPosition::TopEnd => (if rtl { ax } else { ax + aw - size.0 }, ay - size.1),
    }
}
