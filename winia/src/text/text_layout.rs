use crate::text::Paragraph;
use skia_safe::textlayout::TextDirection;
use skia_safe::Point;

pub struct TextLayout<'a> {
    paragraph: &'a Paragraph,
    length: usize,
}

impl<'a> TextLayout<'a> {
    pub(crate) fn new(
        paragraph: &'a Paragraph,
        length: usize,
    ) -> TextLayout<'a> {
        TextLayout {
            paragraph,
            length,
        }
    }

    /// get the cursor position and height of the line at the index
    /// * return (x,y,height)
    pub fn get_cursor_position(&self, index: usize) -> Option<(f32, f32, f32)> {
        if self.length == 0 {
            if let Some(gc) = self.paragraph.get_glyph_cluster_at(0) {
                return Some((gc.bounds.left, gc.bounds.top, gc.bounds.height()));
            }
        }

        let paragraph_index = self.paragraph
            .paragraph_byte_to_real_indices
            .get_by_right(&index)?;
        let glyph_index = self.paragraph.byte_to_glyph_indices.get_by_left(paragraph_index)?;
        if index == 0 || {
            let prev_byte_index = self.paragraph.prev_glyph_byte_index(index);
            if let Some(prev_byte_index) = prev_byte_index {
                self.paragraph.is_line_break(prev_byte_index..index)
            } else {
                false
            }
        } {
            if let Some(gc) = self.paragraph.inner_paragraph().get_glyph_cluster_at(*paragraph_index) {
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

    pub fn get_closest_grapheme_cluster_cluster_at(&self, point: impl Into<Point>) -> usize {
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
                while !self.paragraph.byte_to_glyph_indices.contains_left(&end) {
                    if end >= self.length {
                        break;
                    }
                    end += 1;
                }
                end
            };

            let start = self.paragraph.paragraph_byte_to_real_indices.get_by_left(&start).cloned().unwrap();
            let end = self.paragraph.paragraph_byte_to_real_indices.get_by_left(&end).cloned().unwrap();

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
