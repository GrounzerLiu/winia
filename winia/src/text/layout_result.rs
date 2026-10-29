//! The result of one text layout pass — Compose's `TextLayoutResult`, as far as winia surfaces it.

use crate::unit::Size;

/// What the text leaf measured and how it got there. Handed to `Text::on_text_layout` after every
/// measure (Compose's `onTextLayout` callback).
///
/// This is a `Copy` snapshot on purpose: the callback fires while the paragraph cache is the
/// caller's to inspect, and handing out a borrow of the `Paragraph` would tie the callback to the
/// measure pass. Everything a layout consumer needs to place a caret, a selection handle or a
/// popup — the box, the baselines, the overflow flags — is therefore computed here, once, and
/// copied out.
///
/// Compose's per-line queries (`getLineForOffset`, `getBoundingBox`, `getCursorRect`, …) need the
/// paragraph itself and are answered by [`crate::text::TextLayout`] instead. They cannot live on
/// this snapshot: `Paragraph` wraps a `RefHandle` around Skia's native paragraph, which is NOT
/// `Send` (checked by compiling `assert_send::<Paragraph>()`), so a value that outlives the measure
/// pass cannot carry one. `Text::on_text_layout` still hands the snapshot out; a consumer that
/// needs the queries reads them from the paragraph while it is borrowed, through `TextLayout`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextLayoutResult {
    /// The measured box: the paragraph's size, grown to `min_lines` when the content is shorter.
    pub size: Size,
    /// The paragraph's own size, before `min_lines` grew the box.
    pub text_size: Size,
    /// How many lines the paragraph laid out.
    pub line_count: usize,
    /// Whether `max_lines` cut the text off.
    pub did_exceed_max_lines: bool,
    /// Distance from the box top to the FIRST line's alphabetic baseline (Compose's
    /// `firstBaseline`), which is what a `Modifier.alignByBaseline` row places against.
    pub first_baseline: f32,
    /// Distance from the box top to the LAST line's alphabetic baseline (Compose's
    /// `lastBaseline`).
    pub last_baseline: f32,
    /// The layout is wider than the box it was given (Compose's `didOverflowWidth`).
    pub did_overflow_width: bool,
    /// The layout is taller than the box it was given (Compose's `didOverflowHeight`).
    pub did_overflow_height: bool,
}

impl TextLayoutResult {
    /// Compose's `hasVisualOverflow`: the text was cut off in either direction.
    pub fn has_visual_overflow(&self) -> bool {
        self.did_overflow_width || self.did_overflow_height || self.did_exceed_max_lines
    }
}
