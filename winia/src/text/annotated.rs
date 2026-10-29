//! An `AnnotatedString` built from text plus style ranges — Compose's data model.
//!
//! Compose keeps styled text as a value (`AnnotatedString`) that can be sliced, merged and queried
//! (`getSpanStyles`, `substring`), and turns it into a paragraph at the last moment. winia had only
//! the resolved product (`RichSpanStyle`), produced while a `RichText` scope is open, so nothing
//! could be stored and reused. This is the missing value type.
//!
//! Offsets are CHARACTER indices, like `RichSpanStyle` and Compose's own char offsets — not the
//! UTF-8 byte offsets the text payload uses for cursor and selection work.

use std::ops::Range;

use crate::graphics::Color;
use crate::text::{FontSlant, FontWeight, TextDecoration};
use crate::unit::TextUnit;

/// The style one range of an [`AnnotatedString`] carries — a subset of Compose's `SpanStyle`.
///
/// Every field is optional: a span says only what it changes, and [`AnnotatedString::style_at`]
/// merges the spans that cover an offset in the order they were pushed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpanStyle {
    pub color: Option<Color>,
    pub font_size: Option<TextUnit>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontSlant>,
    /// Compose's `SpanStyle.letterSpacing`, a `TextUnit` (`sp`, or an `em` of THIS span's size).
    pub letter_spacing: Option<TextUnit>,
    pub background: Option<Color>,
    pub decoration: Option<TextDecoration>,
}

impl SpanStyle {
    pub fn new() -> Self { Self::default() }
    pub fn color(mut self, color: Color) -> Self { self.color = Some(color); self }
    pub fn font_size(mut self, size: impl Into<TextUnit>) -> Self { self.font_size = Some(size.into()); self }
    pub fn font_weight(mut self, weight: FontWeight) -> Self { self.font_weight = Some(weight); self }
    pub fn bold(self) -> Self { self.font_weight(FontWeight::BOLD) }
    pub fn font_style(mut self, style: FontSlant) -> Self { self.font_style = Some(style); self }
    pub fn italic(self) -> Self { self.font_style(FontSlant::Italic) }
    pub fn letter_spacing(mut self, spacing: impl Into<TextUnit>) -> Self {
        self.letter_spacing = Some(spacing.into());
        self
    }
    pub fn background(mut self, color: Color) -> Self { self.background = Some(color); self }
    pub fn decoration(mut self, decoration: TextDecoration) -> Self { self.decoration = Some(decoration); self }
    pub fn underline(self) -> Self { self.decoration(TextDecoration::UNDERLINE) }

    /// `other`'s fields over this one — Compose's `SpanStyle.merge`.
    pub fn merge(&self, other: &SpanStyle) -> SpanStyle {
        SpanStyle {
            color: other.color.or(self.color),
            font_size: other.font_size.or(self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            font_style: other.font_style.or(self.font_style),
            letter_spacing: other.letter_spacing.or(self.letter_spacing),
            background: other.background.or(self.background),
            decoration: other.decoration.or(self.decoration),
        }
    }
}

/// Text plus the style ranges over it — Compose's `AnnotatedString`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotatedString {
    text: String,
    spans: Vec<(Range<usize>, SpanStyle)>,
}

impl AnnotatedString {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), spans: Vec::new() }
    }

    /// Text and spans together; a span outside the text is dropped, one that runs past the end is
    /// clamped (Compose throws; a stored value that cannot panic when it is later sliced is worth
    /// more here).
    pub fn from_parts(text: impl Into<String>, spans: Vec<(Range<usize>, SpanStyle)>) -> Self {
        let text = text.into();
        let len = text.chars().count();
        let spans = spans
            .into_iter()
            .filter_map(|(range, style)| {
                let start = range.start.min(len);
                let end = range.end.min(len);
                if start < end { Some((start..end, style)) } else { None }
            })
            .collect();
        Self { text, spans }
    }

    pub fn text(&self) -> &str { &self.text }
    /// Length in CHARACTERS (Compose's `length`).
    pub fn len(&self) -> usize { self.text.chars().count() }
    pub fn is_empty(&self) -> bool { self.text.is_empty() }
    pub fn span_styles(&self) -> &[(Range<usize>, SpanStyle)] { &self.spans }

    /// Add a style range. Pushed order is merge order: the later span wins field by field.
    pub fn with_style(mut self, range: Range<usize>, style: SpanStyle) -> Self {
        self.push_style(range, style);
        self
    }

    pub fn push_style(&mut self, range: Range<usize>, style: SpanStyle) {
        let len = self.len();
        let start = range.start.min(len);
        let end = range.end.min(len);
        if start < end {
            self.spans.push((start..end, style));
        }
    }

    /// The merged style at one character offset (Compose's `getSpanStyleAt`).
    pub fn style_at(&self, offset: usize) -> SpanStyle {
        let mut style = SpanStyle::default();
        for (range, span) in &self.spans {
            if range.contains(&offset) {
                style = style.merge(span);
            }
        }
        style
    }

    /// A slice, with the spans that survive it rebased onto the new start (Compose's `substring`).
    pub fn substring(&self, range: Range<usize>) -> AnnotatedString {
        let len = self.len();
        let start = range.start.min(len);
        let end = range.end.min(len);
        let text: String = self.text.chars().skip(start).take(end.saturating_sub(start)).collect();
        let mut spans = Vec::new();
        for (span_range, style) in &self.spans {
            let from = span_range.start.max(start);
            let to = span_range.end.min(end);
            if from < to {
                spans.push((from - start..to - start, style.clone()));
            }
        }
        AnnotatedString { text, spans }
    }
}

#[cfg(test)]
mod annotated_tests {
    use super::*;

    #[test]
    fn spans_merge_in_the_order_they_were_pushed() {
        let text = AnnotatedString::new("hello world")
            .with_style(0..5, SpanStyle::new().bold().color(Color::RED))
            .with_style(3..8, SpanStyle::new().color(Color::BLUE));
        assert_eq!(text.len(), 11);
        assert_eq!(text.style_at(1).color, Some(Color::RED));
        assert_eq!(text.style_at(1).font_weight, Some(FontWeight::BOLD));
        // The later span wins the field it sets and leaves the rest alone.
        let both = text.style_at(4);
        assert_eq!(both.color, Some(Color::BLUE));
        assert_eq!(both.font_weight, Some(FontWeight::BOLD));
        assert_eq!(text.style_at(9), SpanStyle::default());
    }

    #[test]
    fn a_substring_rebases_the_spans_that_survive() {
        let text = AnnotatedString::new("abcdef");
        let text = text.with_style(1..4, SpanStyle::new().bold());
        let slice = text.substring(2..5);
        assert_eq!(slice.text(), "cde");
        assert_eq!(slice.span_styles().len(), 1);
        assert_eq!(slice.span_styles()[0].0, 0..2);
        assert_eq!(slice.style_at(0).font_weight, Some(FontWeight::BOLD));
        assert_eq!(slice.style_at(2).font_weight, None);
    }

    #[test]
    fn a_range_outside_the_text_is_clamped_or_dropped() {
        let text = AnnotatedString::from_parts(
            "abc",
            vec![(0..99, SpanStyle::new().bold()), (5..9, SpanStyle::new().italic())],
        );
        assert_eq!(text.span_styles().len(), 1, "the empty span is dropped: {:?}", text.span_styles());
        assert_eq!(text.span_styles()[0].0, 0..3);
    }
}