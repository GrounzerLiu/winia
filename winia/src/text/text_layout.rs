use crate::text::Paragraph;
use skia_safe::textlayout::TextDirection;

/// Caret and hit-test geometry over a laid-out [`Paragraph`].
///
/// The paragraph is the only source of truth: it knows how many content bytes it maps, so there is
/// no caller-supplied length to disagree with. It used to take one — `render` passed
/// `content.len()`, the pointer and IME paths passed `0` and the IME path passed a map-entry count —
/// and a `0` turned every caret query into "the first cluster", silently parking the caret at the
/// start of a non-empty field (measured: a 3-byte "abc" reported x=0.0 for indices 0, 1, 2 and 3,
/// where the correct positions were 0.0 / 8.14 / 17.55 / 24.94).
pub struct TextLayout<'a> {
    paragraph: &'a Paragraph,
}

impl<'a> TextLayout<'a> {
    pub(crate) fn new(paragraph: &'a Paragraph) -> TextLayout<'a> {
        TextLayout { paragraph }
    }

    /// get the cursor position and height of the line at the index
    /// * return (x,y,height)
    pub fn get_cursor_position(&self, index: usize) -> Option<(f32, f32, f32)> {
        // An empty paragraph has no cluster to measure, so the caret sits at the origin with one
        // line of height. This is the ONLY case that answers without a cluster — a non-empty
        // paragraph answers for the index it was asked about.
        if self.paragraph.text_len() == 0 {
            return Some(self.empty_caret());
        }
        // Clamp like Compose: an offset past the end (a field that just shrank) parks the caret at
        // the end, and one that landed mid-character parks it at that character's start.
        let index = index.min(self.paragraph.text_len());
        let paragraph_index = self.paragraph.paragraph_byte_for_real_index(index);
        let glyph_index = self.paragraph.byte_to_glyph_indices.get_by_left(&paragraph_index)?;
        // The FIRST CLUSTER, not "index 0": an offset clamped into the first character has the
        // same caret as that character's start, and the previous-cluster branch below would ask for
        // glyph index -1 and give up (this is what made a mid-character caret return None).
        if paragraph_index == 0 || {
            let prev_byte_index = self.paragraph.prev_glyph_byte_index(index);
            if let Some(prev_byte_index) = prev_byte_index {
                self.paragraph.is_line_break(prev_byte_index..index)
            } else {
                false
            }
        } {
            if let Some(gc) = self.paragraph.inner_paragraph().get_glyph_cluster_at(paragraph_index) {
                if gc.position == TextDirection::LTR {
                    Some((gc.bounds.left, gc.bounds.top, gc.bounds.height()))
                } else {
                    Some((gc.bounds.right, gc.bounds.top, gc.bounds.height()))
                }
            } else {
                None
            }
        } else {
            let prev_glyph_index = glyph_index.checked_sub(1)?;
            let prev_byte_index = self.paragraph.byte_to_glyph_indices
                .get_by_right(&prev_glyph_index)?;

            if let Some(gc) = self.paragraph.inner_paragraph().get_glyph_cluster_at(*prev_byte_index) {
                if gc.position == TextDirection::LTR {
                    Some((gc.bounds.right, gc.bounds.top, gc.bounds.height()))
                } else {
                    Some((gc.bounds.left, gc.bounds.top, gc.bounds.height()))
                }
            } else {
                None
            }
        }
    }

    /// The caret height and origin for an empty paragraph: origin, one line tall.
    ///
    /// Skia reports no line metrics for a paragraph with no text, so the height falls back to the
    /// alphabetic baseline (ascent + descent for a single empty line), which is what the one-line
    /// case reports too.
    fn empty_caret(&self) -> (f32, f32, f32) {
        let height = self
            .paragraph
            .get_line_metrics()
            .first()
            .map(|m| m.height as f32)
            .unwrap_or_else(|| self.paragraph.alphabetic_baseline());
        (0.0, 0.0, height)
    }

    /// The real (content) byte offset of the grapheme cluster nearest to `point`.
    ///
    /// Named after Compose's `getOffsetForPosition` neighbour rather than the old
    /// `…cluster_cluster_at`, which doubled the word.
    pub fn get_closest_grapheme_cluster_at(&self, point: impl Into<skia_safe::Point>) -> usize {
        let point = point.into();
        let point_clone = point.clone();
        let glyph_info = self.paragraph.inner_paragraph().get_closest_glyph_cluster_at(point);
        if let Some(glyph_info) = glyph_info {
            let bounds = glyph_info.bounds;
            let center_x = (bounds.left + bounds.right) / 2.0;
            if self.paragraph.is_line_break(glyph_info.text_range.clone()) {
                return glyph_info.text_range.start;
            }

            let start = {
                let mut start = glyph_info.text_range.start;
                while !self.paragraph.byte_to_glyph_indices.contains_left(&start) {
                    if start == 0 {
                        break;
                    }
                    start -= 1;
                }
                start
            };
            let end = {
                let mut end = glyph_info.text_range.end;
                // Bound the walk by the PARAGRAPH's own byte space (what Skia's ranges are in),
                // not by a caller-supplied length that used to be 0 at every hit-test call site.
                let limit = self.paragraph.paragraph_byte_len();
                while !self.paragraph.byte_to_glyph_indices.contains_left(&end) && end < limit {
                    end += 1;
                }
                end
            };

            // Clamped, not unwrapped: a cluster boundary can fall inside a placeholder span that
            // has no exact key (see `Paragraph::real_index_for_paragraph_byte`).
            let start = self.paragraph.real_index_for_paragraph_byte(start);
            let end = self.paragraph.real_index_for_paragraph_byte(end);

            return if point_clone.x < center_x {
                if glyph_info.position == TextDirection::LTR {
                    start
                } else {
                    end
                }
            } else if glyph_info.position == TextDirection::LTR {
                end
            } else {
                start
            };
        }
        0
    }

    pub fn inner_paragraph(&self) -> &Paragraph {
        self.paragraph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::Color;

    fn paragraph(text: &str) -> Paragraph {
        let mut p = crate::text::build_plain_paragraph(text, 16.0, Color::BLACK, 400.0);
        p.layout(400.0);
        p
    }

    /// Every index of a non-empty paragraph must report ITS OWN caret.
    ///
    /// The old constructor took a `length` and every caller that passed 0 (all three hit-test and
    /// IME sites) made this method answer with cluster 0 for every index — measured on "abc":
    /// x=0.0 for indices 0, 1, 2 and 3, where the positions are 0.0 / 8.14 / 17.55 / 24.94.
    #[test]
    fn a_caret_position_belongs_to_the_index_that_was_asked_for() {
        let p = paragraph("abc");
        assert_eq!(p.text_len(), 3, "three ASCII bytes");
        let layout = TextLayout::new(&p);
        let xs: Vec<f32> = (0..=3)
            .map(|i| layout.get_cursor_position(i).expect("a caret for every index").0)
            .collect();
        assert!(
            xs.windows(2).all(|w| w[1] > w[0] + 1.0),
            "each index must advance the caret: {xs:?}"
        );
        assert!(xs[3] > 20.0, "the end caret sits past the two glyphs: {xs:?}");
    }

    /// An offset past the end (a field that just shrank) parks at the end, like Compose.
    #[test]
    fn a_caret_past_the_end_parks_at_the_end() {
        let p = paragraph("abc");
        let layout = TextLayout::new(&p);
        let end = layout.get_cursor_position(3).unwrap();
        assert_eq!(layout.get_cursor_position(99), Some(end));
    }

    /// An offset that lands inside a multi-byte character parks at that character's start.
    #[test]
    fn a_caret_mid_character_parks_at_the_character_start() {
        // "\u{4e2d}a": the first character is 3 bytes, so indices 1 and 2 are inside it.
        let p = paragraph("\u{4e2d}a");
        assert_eq!(p.text_len(), 4);
        let layout = TextLayout::new(&p);
        let start = layout.get_cursor_position(0).unwrap();
        assert_eq!(layout.get_cursor_position(1), Some(start));
        assert_eq!(layout.get_cursor_position(2), Some(start));
        assert!(layout.get_cursor_position(3).unwrap().0 > start.0);
    }

    /// An empty paragraph answers with the origin and one line of height, and does so for any index.
    #[test]
    fn an_empty_paragraph_puts_the_caret_at_the_origin() {
        let p = paragraph("");
        assert_eq!(p.text_len(), 0);
        let layout = TextLayout::new(&p);
        let caret = layout.get_cursor_position(0).expect("an empty field still has a caret");
        assert_eq!((caret.0, caret.1), (0.0, 0.0));
        assert!(caret.2 > 0.0, "the caret needs a line height: {caret:?}");
    }

    /// Hit-testing never leaves the text, for any x.
    #[test]
    fn a_hit_test_stays_inside_the_text() {
        let p = paragraph("abc");
        let layout = TextLayout::new(&p);
        for x in 0..80 {
            let hit = layout.get_closest_grapheme_cluster_at(skia_safe::Point::new(x as f32, 8.0));
            assert!(hit <= p.text_len(), "hit {hit} out of range at x={x}");
        }
    }
}