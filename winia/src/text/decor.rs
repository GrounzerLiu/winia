//! Text decoration and font rendering options, and the resolved per-span style rich text
//! carries.

use crate::graphics::Color;

/// How a decoration line is drawn (Skia's `TextDecorationStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoStyle { Solid, Double, Dotted, Dashed, Wavy }

/// Whether a decoration goes through the glyph gaps or straight across (Skia's
/// `TextDecorationMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoMode { Gaps, Through }

/// Glyph edge treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontEdge { Alias, AntiAlias, SubpixelAntiAlias }

/// Font hinting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontHint { None, Slight, Normal, Full }

/// One resolved run of rich text, with the range it covers.
///
/// A `RichText` resolves its scopes into these while it builds; the measure and render paths read
/// them straight out of the `RichTextContent` element, so nothing re-resolves a style per frame.
#[derive(Debug, Clone, PartialEq)]
pub struct RichSpanStyle {
    /// Range start, a CHARACTER index (inclusive).
    pub start: usize,
    /// Range end, a CHARACTER index (exclusive).
    pub end: usize,
    pub font_size: f32,
    pub color: Color,
    pub font_weight: crate::text::FontWeight,
    pub font_style: crate::text::FontSlant,
    // ── Decorations ──
    pub underline: bool,
    pub overline: bool,
    pub strikethrough: bool,
    pub decoration_color: Option<Color>,
    pub decoration_style: Option<DecoStyle>,
    pub decoration_mode: Option<DecoMode>,
    // ── Baseline ──
    pub baseline_shift: f32,
    // ── Spacing ──
    /// Resolved letter spacing in logical pixels: the resolver turns the caller's `TextUnit`
    /// (`sp`, or an `em` multiple of THIS span's own size) into pixels, because Skia wants pixels
    /// and a span's size is what the `em` is relative to.
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub height_multiple: f32,
    pub half_leading: bool,
    // ── Typeface ──
    pub font_families: Vec<String>,
    pub font_width: i32,
    pub font_edging: Option<FontEdge>,
    pub font_hinting: Option<FontHint>,
    pub subpixel: bool,
    // ── Foreground / background ──
    pub foreground_color: Option<Color>,
    pub background: Option<Color>,
    // ── Miscellaneous ──
    pub locale: Option<String>,
}
