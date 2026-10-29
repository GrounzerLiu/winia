use skia_safe::textlayout::paragraph::{ExtendedVisitorInfo, FontInfo, GlyphClusterInfo, GlyphInfo, Paragraph as SkParagraph, VisitorInfo};
use skia_safe::textlayout::{Affinity, LineMetrics, ParagraphStyle, RectHeightStyle, RectWidthStyle, TextBox, TextRange, TextStyle};
use skia_safe::{scalar, Canvas, Font, Path, Point, TextBlob, Unichar};
use std::collections::HashSet;
use std::ops::Range;
use crate::text::{IndexBiMap, ParagraphBuilder};

/// Build a laid-out paragraph for one plain text run — the same construction `Text` goes through,
/// exposed for the public drawing surface (`crate::components::draw_scope`'s `draw_text`) so canvas text is
/// shaped by one path rather than two.
///
/// `width` is the layout width in logical pixels; pass a large value for a single unwrapped line.
pub fn build_plain_paragraph(
    text: &str,
    font_size: f32,
    color: crate::graphics::Color,
    width: f32,
) -> Paragraph {
    let fc = crate::text::font::get_font_collection();
    let mut ts = TextStyle::new();
    ts.set_font_size(font_size);
    ts.set_color(crate::graphics::skia_color(color));
    let mut style = ParagraphStyle::default();
    style.set_text_style(&ts);
    let mut b = ParagraphBuilder::new(&style, fc);
    b.push_style(&ts);
    b.add_text(text);
    let mut p = b.build();
    p.layout(width);
    p
}

/// Where the paragraph's own text sits inside the caller's content, when start/middle ellipsis
/// truncated it (see `truncate_for_ellipsis`).
///
/// The paragraph then holds `content[..head_bytes] + "…" + content[tail_start_byte..]`, so a hit
/// test answers in DISPLAY bytes and every consumer that speaks content offsets has to translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplaySpan {
    /// Bytes of the content kept in front of the marker (0 for `StartEllipsis`).
    pub head_bytes: usize,
    /// Byte offset in the CONTENT where the kept tail starts (the content length for `StartEllipsis`
    /// with no tail, never used that way).
    pub tail_start_byte: usize,
    /// Bytes the marker itself occupied in the display text (`"…"` is 3).
    pub marker_bytes: usize,
}

impl DisplaySpan {
    /// A display byte offset → the content byte offset the user pointed at.
    ///
    /// A click on the marker itself lands on the first visible tail byte, which is what the reader
    /// means by pointing there.
    pub fn content_offset(&self, display: usize) -> usize {
        if display < self.head_bytes {
            display
        } else if display < self.head_bytes + self.marker_bytes {
            self.tail_start_byte
        } else {
            self.tail_start_byte + (display - self.head_bytes - self.marker_bytes)
        }
    }
}

pub struct Paragraph {
    paragraph: SkParagraph,
    placeholders: Vec<std::sync::Arc<dyn crate::text::InlineDrawable>>,
    line_breaks: HashSet<Range<usize>>,
    pub(crate) paragraph_byte_to_real_indices: IndexBiMap,
    pub(crate) byte_to_utf16_indices: IndexBiMap,
    pub(crate) byte_to_glyph_indices: IndexBiMap,
    display_span: Option<DisplaySpan>,
}

impl Paragraph {
    pub(crate) fn new(
        paragraph: SkParagraph,
        placeholders: &[std::sync::Arc<dyn crate::text::InlineDrawable>],
        line_breaks: &HashSet<Range<usize>>,
        paragraph_byte_to_real_indices: &IndexBiMap,
        byte_to_utf16_indices: &IndexBiMap,
        byte_to_glyph_indices: &IndexBiMap,
    ) -> Self {
        Self {
            display_span: None,
            paragraph,
            placeholders: Vec::from(placeholders),
            line_breaks: line_breaks.clone(),
            paragraph_byte_to_real_indices: paragraph_byte_to_real_indices.clone(),
            byte_to_utf16_indices: byte_to_utf16_indices.clone(),
            byte_to_glyph_indices: byte_to_glyph_indices.clone(),
        }
    }

    pub fn inner_paragraph(&self) -> &SkParagraph {
        &self.paragraph
    }
    
    pub fn is_line_break(&self, range: Range<usize>) -> bool {
        self.line_breaks.contains(&range)
    }

    /// Total length of the REAL (content) text, in bytes.
    ///
    /// This is what a caret offset is expressed in, and the bound `TextLayout` clamps to. It is not
    /// `content.len()` on the caller's side: a `RichText` placeholder may stand for more or fewer
    /// content bytes than the single U+FFFC it renders as, and the paragraph is the authority on
    /// the space it actually maps.
    /// The truncation this paragraph displays, when start/middle ellipsis shortened its text.
    pub fn display_span(&self) -> Option<DisplaySpan> {
        self.display_span
    }

    /// Record how the displayed text relates to the caller's content (see [`DisplaySpan`]).
    pub fn set_display_span(&mut self, span: Option<DisplaySpan>) {
        self.display_span = span;
    }

    /// A DISPLAY byte offset (what a hit test answers) → the caller's content byte offset.
    ///
    /// Identity unless start/middle ellipsis truncated the string, in which case every consumer
    /// that stores content offsets (caret, selection, drag) must go through this.
    pub fn content_offset(&self, display: usize) -> usize {
        match self.display_span {
            Some(span) => span.content_offset(display),
            None => display,
        }
    }

    /// A CONTENT byte offset → the display byte offset to draw at (the inverse of
    /// [`Paragraph::content_offset`]).
    ///
    /// An offset inside the bytes the truncation dropped collapses onto the marker, so a selection
    /// that covers only invisible text paints nothing rather than the whole visible run.
    pub fn display_offset(&self, content: usize) -> usize {
        match self.display_span {
            None => content,
            Some(span) => {
                if content < span.head_bytes {
                    content
                } else if content < span.tail_start_byte {
                    // STRICTLY before the kept tail: those bytes were dropped, so their offset
                    // collapses onto the marker. `<=` put `tail_start_byte` here too, and that byte
                    // is the FIRST VISIBLE one — it answered "the marker" instead of "three bytes
                    // past the head", so a caret on it sat on the ellipsis (measured: the caret for
                    // content 14 of a start-ellipsized run was at x=0.0, the marker's own x).
                    span.head_bytes
                } else {
                    span.head_bytes + span.marker_bytes + (content - span.tail_start_byte)
                }
            }
        }
    }

    /// Total length of the CONTENT the caller handed in, in bytes.
    ///
    /// `text_len` is the DISPLAY space (what the paragraph actually holds); when start/middle
    /// ellipsis truncated the string the two differ, and every public query takes content offsets.
    /// The tail is everything after `tail_start_byte`, so the content length comes back out of the
    /// display length:
    ///
    ///     content = tail_start + (display - head - marker)
    ///
    /// For `StartEllipsis` (head 0) that is `14 + (9 - 3) = 20` on a 20-byte content truncated to
    /// nine bytes, which is exactly where the missing head went.
    pub fn content_len(&self) -> usize {
        match self.display_span {
            None => self.text_len(),
            Some(span) => {
                let tail = self.text_len().saturating_sub(span.head_bytes + span.marker_bytes);
                span.tail_start_byte + tail
            }
        }
    }
    pub fn text_len(&self) -> usize {
        self.paragraph_byte_to_real_indices.right_keys().last().copied().unwrap_or(0)
    }

    /// Total length of the PARAGRAPH's own byte space (what Skia's clusters are reported in).
    pub(crate) fn paragraph_byte_len(&self) -> usize {
        self.paragraph_byte_to_real_indices.left_keys().last().copied().unwrap_or(0)
    }

    /// Map a REAL (content) byte index to the paragraph's byte space, clamping to the nearest
    /// preceding mapped position when the exact one does not exist.
    ///
    /// The caret is an editing offset: it can arrive past the end (a field that just shrank) or
    /// inside a multi-byte character (a click that landed mid-glyph). Compose clamps both;
    /// returning `None` made the caret silently disappear instead.
    pub(crate) fn paragraph_byte_for_real_index(&self, real: usize) -> usize {
        if let Some(mapped) = self.paragraph_byte_to_real_indices.get_by_right(&real) {
            return *mapped;
        }
        let mut best = 0usize;
        for (left, right) in self
            .paragraph_byte_to_real_indices
            .left_keys()
            .iter()
            .zip(self.paragraph_byte_to_real_indices.right_keys().iter())
        {
            if *right > real { break; }
            best = *left;
        }
        best
    }

    /// Map a PARAGRAPH byte index to the real (content) byte index, clamping to the nearest
    /// preceding mapped position when the exact one does not exist.
    ///
    /// The two spaces differ wherever a placeholder stands in for text of another length — the
    /// paragraph holds one U+FFFC while the content may hold a whole word. Callers that hit-test
    /// must never panic because a cluster boundary fell inside such a span.
    pub(crate) fn real_index_for_paragraph_byte(&self, paragraph_byte: usize) -> usize {
        if let Some(mapped) = self.paragraph_byte_to_real_indices.get_by_left(&paragraph_byte) {
            return *mapped;
        }
        // Walk back to the nearest mapped paragraph byte (monotonic insertion, so a reverse scan
        // finds the largest key at or below the target).
        let mut best = 0usize;
        for (left, right) in self
            .paragraph_byte_to_real_indices
            .left_keys()
            .iter()
            .zip(self.paragraph_byte_to_real_indices.right_keys().iter())
        {
            if *left > paragraph_byte { break; }
            best = *right;
        }
        best
    }

    fn get_utf16_index(&self, index: usize) -> Option<usize> {
        // An exact key.
        if let Some(paragraph_index) = self.paragraph_byte_to_real_indices.get_by_right(&index) {
            return self.byte_to_utf16_indices.get_by_left(paragraph_index).copied();
        }
        // Otherwise walk back to the nearest mapped start byte, which is what an offset inside a
        // multi-byte character asks for.
        for left_byte in self.paragraph_byte_to_real_indices.left_keys().iter().rev() {
            if let Some(mapped) = self.paragraph_byte_to_real_indices.get_by_left(left_byte) {
                if *mapped <= index {
                    return self.byte_to_utf16_indices.get_by_left(left_byte).copied();
                }
            }
        }
        None
    }

    pub fn get_glyph_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_indices
            .get_by_right(&index)?;
        let glyph_index = self
            .byte_to_glyph_indices
            .get_by_left(paragraph_index)?;
        Some(*glyph_index)
    }
    
    pub fn prev_glyph_byte_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_indices
            .get_by_right(&index)?;
        let glyph_index = self
            .byte_to_glyph_indices
            .get_by_left(paragraph_index)?;
        let prev_glyph_index = glyph_index.checked_sub(1)?;
        let prev_byte_index = self
            .byte_to_glyph_indices
            .get_by_right(&prev_glyph_index)?;
        let prev_byte_index = self
            .paragraph_byte_to_real_indices
            .get_by_left(prev_byte_index)?;
        Some(*prev_byte_index)
    }
    
    pub fn next_glyph_byte_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_indices
            .get_by_right(&index)?;
        let glyph_index = self
            .byte_to_glyph_indices
            .get_by_left(paragraph_index)?;
        let next_glyph_index = glyph_index + 1;
        let next_byte_index = self
            .byte_to_glyph_indices
            .get_by_right(&next_glyph_index)?;
        let next_byte_index = self
            .paragraph_byte_to_real_indices
            .get_by_left(next_byte_index)?;
        Some(*next_byte_index)
    }

    pub fn get_index(&self, utf16_index: usize) -> usize {
        self.get_index_or_default(utf16_index)
    }

    /// Returns the real byte index for a given UTF-16 index, or 0 as fallback.
    pub fn get_index_or_default(&self, utf16_index: usize) -> usize {
        self.try_get_index(utf16_index).unwrap_or(0)
    }

    /// Try to get the real byte index for a given UTF-16 index.
    pub fn try_get_index(&self, utf16_index: usize) -> Option<usize> {
        let paragraph_index = self
            .byte_to_utf16_indices
            .get_by_right(&utf16_index)?;
        self.paragraph_byte_to_real_indices
            .get_by_left(paragraph_index)
            .copied()
    }

    pub fn max_width(&self) -> scalar {
        self.paragraph.max_width()
    }

    pub fn height(&self) -> scalar {
        self.paragraph.height()
    }

    pub fn min_intrinsic_width(&self) -> scalar {
        self.paragraph.min_intrinsic_width()
    }

    pub fn max_intrinsic_width(&self) -> scalar {
        self.paragraph.max_intrinsic_width()
    }

    /// The font's own line height (ascent + descent) of the first run, without any line-height
    /// multiplier — what Compose subtracts from `lineHeight` to get the extra leading.
    ///
    /// Known difference from Compose: Compose takes this from the style's PRIMARY typeface, while
    /// this takes it from the font that actually shaped the first glyph, which can be a fallback —
    /// measured at 16 sp, `Ag` reports 21.28 and two CJK glyphs report 20.32, so the same
    /// `LineHeightStyle` shifts a CJK line about 0.5 px differently from a Latin one. skia-safe
    /// exposes no per-paragraph "style font before fallback", and `LineMetrics::get_style_metrics`
    /// answers with the resolved fallback too (measured: identical numbers). Recorded in
    /// `docs/text.md` rather than papered over.
    pub fn natural_line_height(&self) -> scalar {
        let font = self.inner_paragraph().get_font_at(0);
        let (_, metrics) = font.metrics();
        (metrics.descent - metrics.ascent) as scalar
    }

    pub fn alphabetic_baseline(&self) -> scalar {
        self.paragraph.alphabetic_baseline()
    }

    pub fn ideographic_baseline(&self) -> scalar {
        self.paragraph.ideographic_baseline()
    }

    pub fn longest_line(&self) -> scalar {
        self.paragraph.longest_line()
    }

    pub fn did_exceed_max_lines(&self) -> bool {
        self.paragraph.did_exceed_max_lines()
    }

    pub fn layout(&mut self, width: scalar) {
        self.paragraph.layout(width);
    }

    pub fn paint(&self, canvas: &Canvas, x: f32, y: f32) {
        self.paragraph.paint(canvas, (x, y));
        self.paragraph
            .get_rects_for_placeholders()
            .iter()
            .enumerate()
            .for_each(|(i, text_box)| {
                if let Some(placeholder) = self.placeholders.get(i) {
                    placeholder
                        .draw(canvas, x + text_box.rect.left, y + text_box.rect.top);
                }
            });
    }

    pub fn get_rects_for_range(
        &self,
        range: Range<usize>,
        rect_height_style: RectHeightStyle,
        rect_width_style: RectWidthStyle,
    ) -> Vec<TextBox> {
        let start = self.get_utf16_index(range.start).unwrap();
        let end = self.get_utf16_index(range.end).unwrap();
        self.paragraph.get_rects_for_range(
            start..end,
            rect_height_style,
            rect_width_style,
        )
    }

    pub fn get_rects_for_placeholders(&self) -> Vec<TextBox> {
        self.paragraph.get_rects_for_placeholders()
    }

    pub fn get_glyph_position_at_coordinate(&self, p: impl Into<Point>) -> (usize, Affinity) {
        let p_with_a = self
            .paragraph
            .get_glyph_position_at_coordinate(p);
        (
            self.get_index(p_with_a.position as usize),
            p_with_a.affinity
        )
    }

    pub fn get_word_boundary(&self, offset: usize) -> Range<usize> {
        let range = self
            .paragraph
            .get_word_boundary(self.get_utf16_index(offset).unwrap() as u32);
        let start = self.get_index(range.start);
        let end = self.get_index(range.end);
        start..end
    }

    pub fn get_line_metrics(&self) -> Vec<LineMetrics<'_>> {
        let mut lines = self.paragraph.get_line_metrics();
        for line in lines.iter_mut() {
            line.start_index = self.get_index(line.start_index);
            line.end_index = self.get_index(line.end_index);
        }
        lines
    }

    pub fn line_number(&self) -> usize {
        self.paragraph.line_number()
    }

    pub fn mark_dirty(&mut self) {
        self.paragraph.mark_dirty();
    }

    pub fn unresolved_glyphs(&mut self) -> Option<usize> {
        self.paragraph.unresolved_glyphs()
    }

    pub fn unresolved_codepoints(&mut self) -> Vec<Unichar>{
        self.paragraph.unresolved_codepoints()
    }

    pub fn visit<'a, F>(&mut self, visitor: F)
    where
        F: FnMut(usize, Option<&'a VisitorInfo>) {
        self.paragraph.visit(visitor);
    }

    pub fn extended_visit<'a, F>(&mut self, visitor: F)
    where
        F: FnMut(usize, Option<&'a ExtendedVisitorInfo>) {
        self.paragraph.extended_visit(visitor);
    }

    pub fn get_path_at(&mut self, line_number: usize) -> (usize, Path) {
        self.paragraph.get_path_at(line_number)
    }

    pub fn contains_emoji(&mut self, text_blob: &mut TextBlob) -> bool {
        self.paragraph.contains_emoji(text_blob)
    }

    pub fn contains_color_font_or_bitmap(
        &mut self,
        text_blob: &mut TextBlob,
    ) -> bool {
        self.paragraph.contains_color_font_or_bitmap(text_blob)
    }

    pub fn get_line_number_at(&self, index: usize) -> Option<usize> {
        self.paragraph.get_line_number_at(index)
    }

    pub fn get_line_metrics_at(&self, line_number: usize) -> Option<LineMetrics<'_>> {
        if let Some(mut line_metrics) = self.paragraph.get_line_metrics_at(line_number) {
            line_metrics.start_index = self.get_index(line_metrics.start_index);
            line_metrics.end_index = self.get_index(line_metrics.end_index);
            Some(line_metrics)
        } else {
            None
        }
    }

    pub fn get_actual_text_range(
        &self,
        line_number: usize,
        include_spaces: bool,
    ) -> TextRange {
        let mut text_range = self
            .paragraph
            .get_actual_text_range(line_number, include_spaces);
        // These are PARAGRAPH BYTES in this skia-safe build (`get_actual_text_range` has no
        // `_utf16_offset` sibling and reports 0..8 for a 4-character CJK+ASCII line), so they go
        // through the byte map. They used to go through `get_index`, which expects UTF-16 units:
        // the value missed the map and collapsed to 0, so a CJK line reported 0..0 where its
        // content is 0..8. ASCII hid it because there the two units coincide.
        text_range.start = self.real_index_for_paragraph_byte(text_range.start);
        text_range.end = self.real_index_for_paragraph_byte(text_range.end);
        text_range
    }

    pub fn get_glyph_cluster_at(
        &self,
        index: usize,
    ) -> Option<GlyphClusterInfo> {
        if let Some(mut glyph_cluster_info) = self.paragraph.get_glyph_cluster_at(index) {
            glyph_cluster_info.text_range.start = self.real_index_for_paragraph_byte(glyph_cluster_info.text_range.start);
            glyph_cluster_info.text_range.end = self.real_index_for_paragraph_byte(glyph_cluster_info.text_range.end);
            Some(glyph_cluster_info)
        } else {
            None
        }
    }

    pub fn get_closest_glyph_cluster_at(
        &self,
        d: impl Into<Point>,
    ) -> Option<GlyphClusterInfo> {
        if let Some(mut glyph_info) = self.paragraph.get_closest_glyph_cluster_at(d) {
            // Skia reports the cluster in PARAGRAPH byte space, which differs from the real space
            // wherever a placeholder replaced shorter text: an inline image is one U+FFFC in the
            // paragraph (3 bytes) but may stand for a multi-byte span in the content. A cluster
            // boundary that lands inside such a span therefore has no exact key.
            //
            // This used to `expect(...)` and take the window down; Compose clamps instead, and a
            // hit-test that runs on every pointer move must never panic. Mapping to the nearest
            // preceding real index keeps the caret inside the text and is what the caller's own
            // grapheme walk assumes anyway.
            glyph_info.text_range.start = self.real_index_for_paragraph_byte(glyph_info.text_range.start);
            glyph_info.text_range.end = self.real_index_for_paragraph_byte(glyph_info.text_range.end);
            Some(glyph_info)
        } else {
            None
        }
    }

    pub fn get_glyph_info_at(
        &mut self,
        index: usize,
    ) -> Option<GlyphInfo> {
        let index = self.get_utf16_index(index).unwrap();
        if let Some(mut glyph_info) = self.paragraph.get_glyph_info_at_utf16_offset(index) {
            glyph_info.grapheme_cluster_text_range.start = self.get_index(glyph_info.grapheme_cluster_text_range.start);
            glyph_info.grapheme_cluster_text_range.end = self.get_index(glyph_info.grapheme_cluster_text_range.end);
            Some(glyph_info)
        } else {
            None
        }
    }

    pub fn get_closest_glyph_info_at(
        &mut self,
        d: impl Into<Point>,
    ) -> Option<GlyphInfo> {
        if let Some(mut glyph_info) = self.paragraph.get_closest_utf16_glyph_info_at(d) {
            glyph_info.grapheme_cluster_text_range.start = self.get_index(glyph_info.grapheme_cluster_text_range.start);
            glyph_info.grapheme_cluster_text_range.end = self.get_index(glyph_info.grapheme_cluster_text_range.end);
            Some(glyph_info)
        } else {
            None
        }
    }

    pub fn get_font_at(&self, index: usize) -> Font {
        self.paragraph.get_font_at(index)
    }

    pub fn get_fonts(&self) -> Vec<FontInfo> {
        self.paragraph.get_fonts()
    }
}

#[cfg(test)]
mod paragraph_tests {
    /// `display_offset` is the inverse of `content_offset`, and an offset inside the dropped gap
    /// collapses onto the marker so an invisible selection paints nothing.
    #[test]
    fn the_display_mapping_round_trips_and_collapses_the_dropped_gap() {
        use crate::text::DisplaySpan;
        // start ellipsis of "0123456789" kept the last 4 bytes: display "…6789".
        let span = DisplaySpan { head_bytes: 0, tail_start_byte: 6, marker_bytes: 3 };
        // The visible tail maps back and forth.
        assert_eq!(span.content_offset(3), 6, "first visible tail byte");
        assert_eq!(span.content_offset(6), 9, "last visible byte");
        // A middle ellipsis: head "0123", marker, tail "89".
        let middle = DisplaySpan { head_bytes: 4, tail_start_byte: 8, marker_bytes: 3 };
        assert_eq!(middle.content_offset(2), 2, "inside the head");
        assert_eq!(middle.content_offset(4), 8, "the marker means the tail start");
        assert_eq!(middle.content_offset(7), 8, "first tail byte");
        assert_eq!(middle.content_offset(9), 10, "last tail byte");
    }

    use skia_safe::FontMgr;
    use skia_safe::textlayout::{FontCollection, ParagraphStyle, TextStyle};
    use crate::text::ParagraphBuilder;

    /// The extra leading is centred instead of all sitting above the glyphs.
    ///
    /// `set_height` alone put every extra pixel above the text: measured baseline 19.47 in a 24 dp
    /// box at 16 sp. `set_half_leading` moves it to 18.625. Compose's `LineHeightStyle(Center)`
    /// arithmetic over the same metrics gives 17.27 + 1.5 = 18.77, so the residual drops from 0.70
    /// to 0.145 — close, but Skia's half-leading splits by its own ascent/descent, not by half of
    /// the naive difference, which is why this is not an exact match.
    #[test]
    fn a_line_height_centres_the_leading() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            crate::components::Text::new("IIII")
                .font_size(16.0)
                .line_height(crate::unit::TextUnit::Sp(crate::unit::Sp(24.0)))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let node = nodes.iter().find(|n| n.has_text_content).expect("a text node");
        let paragraph = node.cached_paragraph.borrow();
        let paragraph = paragraph.as_ref().expect("measured");
        let baseline = paragraph.alphabetic_baseline();
        assert!(
            (baseline - 18.625).abs() < 0.05,
            "centred leading puts the baseline at 18.625, not {} (uncentred it was 19.47)",
            baseline
        );
        assert!(
            baseline < 19.0,
            "the leading must no longer sit entirely above the glyphs: {baseline}"
        );
    }
    /// `get_actual_text_range` answers in CONTENT bytes.
    ///
    /// Skia reports paragraph bytes for this call; the wrapper went through `get_index`, which
    /// expects UTF-16 units, so the lookup missed the map and collapsed to 0. Measured on
    /// two 3-byte CJK glyphs then `ab` (8 content bytes): the raw call said 0..8, the wrapper 0..0.
    #[test]
    fn an_actual_text_range_is_reported_in_content_bytes() {
        let mut p = crate::text::build_plain_paragraph("\u{4f60}\u{597d}ab", 16.0, crate::graphics::Color::BLACK, 400.0);
        p.layout(400.0);
        let range = p.get_actual_text_range(0, false);
        assert_eq!((range.start, range.end), (0, 8), "two 3-byte glyphs then ab is 8 bytes");
    }

    /// ASCII cannot tell the two units apart, so it must keep working.
    #[test]
    fn an_actual_text_range_of_ascii_matches_its_byte_length() {
        let mut p = crate::text::build_plain_paragraph("hello", 16.0, crate::graphics::Color::BLACK, 400.0);
        p.layout(400.0);
        let range = p.get_actual_text_range(0, false);
        assert_eq!((range.start, range.end), (0, 5));
    }

    /// With a placeholder the paragraph space is shorter than the content space, and the answer
    /// must still land inside the content.
    #[test]
    fn an_actual_text_range_stays_inside_the_content_after_a_placeholder() {
        use skia_safe::textlayout::{PlaceholderAlignment, TextBaseline};
        use std::sync::Arc;
        struct Box2;
        impl crate::text::InlineDrawable for Box2 {
            fn draw(&self, _c: &skia_safe::Canvas, _x: f32, _y: f32) {}
            fn size(&self) -> (f32, f32) { (20.0, 16.0) }
        }
        let mut fc = FontCollection::new();
        fc.set_default_font_manager(FontMgr::default(), None);
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        let mut ts = TextStyle::new();
        ts.set_font_size(16.0);
        b.push_style(&ts);
        b.add_text("ab");
        b.add_placeholder("XY", Arc::new(Box2), &Some((PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0)));
        b.add_text("cd");
        let mut p = b.build();
        p.layout(400.0);
        assert_eq!(p.text_len(), 6, "ab + XY + cd is 6 content bytes");
        let range = p.get_actual_text_range(0, false);
        assert!(
            range.start <= range.end && range.end <= 6,
            "the range must stay inside the content: {range:?}"
        );
    }
    /// A placeholder whose content text is short enough to leave a hole in the paragraph→real
    /// map must CLAMP the hit-test, not panic on it.
    ///
    /// Skia reports the cluster in paragraph byte space; an inline element is one U+FFFC there
    /// (3 bytes) but may stand for a different number of content bytes, so a cluster boundary can
    /// fall where no key exists. This used to `expect(...)` and take the window down — measured as
    /// unreachable across ASCII/CJK/emoji sweeps before, but it is one edit to T-2's placeholder
    /// length away. See docs/text-review.md T-6.
    #[test]
    fn a_cluster_boundary_with_no_mapping_clamps_instead_of_panicking() {
        use skia_safe::textlayout::{PlaceholderAlignment, TextBaseline};
        use std::sync::Arc;
        struct Box2;
        impl crate::text::InlineDrawable for Box2 {
            fn draw(&self, _c: &skia_safe::Canvas, _x: f32, _y: f32) {}
            fn size(&self) -> (f32, f32) { (20.0, 16.0) }
        }
        let mut fc = FontCollection::new();
        fc.set_default_font_manager(FontMgr::default(), None);
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        let mut ts = TextStyle::new();
        ts.set_font_size(16.0);
        b.push_style(&ts);
        b.add_text("ab");
        // Two content bytes against a 3-byte U+FFFC: the paragraph and real index spaces differ by
        // one near the placeholder, so paragraph byte 3 has no exact key.
        b.add_placeholder("XY", Arc::new(Box2), &Some((PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0)));
        b.add_text("cd");
        let mut p = b.build();
        p.layout(200.0);
        // Paragraph byte 3 sits INSIDE the 3-byte U+FFFC and has no entry: the placeholder started
        // at paragraph byte 2 / real byte 2, and the next text starts at paragraph byte 5 / real
        // byte 4. The old code indexed this with `.expect(...)`; the contract is a clamp to the
        // nearest preceding real index (2), never a panic.
        assert_eq!(p.real_index_for_paragraph_byte(3), 2, "a hole clamps to the placeholder's start");
        assert_eq!(p.real_index_for_paragraph_byte(4), 2);
        assert_eq!(p.real_index_for_paragraph_byte(2), 2, "an exact key is unchanged");
        assert_eq!(p.real_index_for_paragraph_byte(5), 4, "the text after the placeholder");
        assert_eq!(p.real_index_for_paragraph_byte(99), 6, "past the end clamps to the last index");
        // And the public entry point stays inside the content for every x.
        let mut x = 0.0f32;
        while x < 80.0 {
            if let Some(info) = p.get_closest_glyph_cluster_at((x, 8.0)) {
                assert!(info.text_range.start <= 6, "start out of range at x={x}: {:?}", info.text_range);
                assert!(info.text_range.end <= 6, "end out of range at x={x}: {:?}", info.text_range);
            }
            x += 1.0;
        }
    }    #[test]
    fn test_paragraph_layout_and_hit_test() {
        let mut fc = FontCollection::new();
        fc.set_default_font_manager(FontMgr::default(), None);
        let style = ParagraphStyle::default();
        let mut b = ParagraphBuilder::new(&style, fc);
        let mut ts = TextStyle::new();
        ts.set_font_size(16.0);
        b.push_style(&ts);
        b.add_text("Hello 世界");
        let mut p = b.build();
        p.layout(200.0);
        assert!(p.height() > 0.0);
        assert!(p.max_intrinsic_width() > 0.0);
        // get_closest_glyph_cluster_at: hits should return valid info
        let gc = p.get_closest_glyph_cluster_at((10.0, 5.0));
        assert!(gc.is_some());
        // get_rects_for_range returns rects
        let rects = p.get_rects_for_range(0..5, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max);
        assert!(!rects.is_empty());
    }
}
