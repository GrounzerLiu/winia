//! The result of one text layout pass — Compose's `TextLayoutResult`, as far as winia surfaces it.

use crate::unit::Size;

/// What the text leaf measured and how it got there. Handed to `Text::on_text_layout` after every
/// measure (Compose's `onTextLayout` callback).
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
}

impl TextLayoutResult {
    /// Compose's `hasVisualOverflow`: the text was cut off. Only the height side has a signal here —
    /// a width overflow is resolved by the paragraph itself (ellipsis or clip).
    pub fn has_visual_overflow(&self) -> bool {
        self.did_exceed_max_lines
    }
}
