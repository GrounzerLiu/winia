use crate::text::Paragraph;
use skia_safe::textlayout::{RectHeightStyle, RectWidthStyle, TextBox, TextDirection};
use skia_safe::{Canvas, Point};
use std::ops::Range;

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

    pub fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        self.paragraph.paint(canvas, x, y);
    }

    pub fn width(&self) -> f32 {
        self.paragraph.max_intrinsic_width()
    }

    pub fn height(&self) -> f32 {
        self.paragraph.height()
    }

    pub fn base_line(&self) -> f32 {
        if let Some(line_metrics) = self.paragraph.get_line_metrics_at(0) {
            line_metrics.baseline as f32
        } else {
            0.0
        }
    }

    /// get the cursor position and height of the line at the index
    /// * return (x,y,height)
    pub fn get_cursor_position(&self, index: usize) -> Option<(f32, f32, f32)> {
        if self.length == 0 {
            let boxes = self.paragraph.get_rects_for_range(
                0..1,
                RectHeightStyle::Max,
                RectWidthStyle::Tight,
            );
            let box0 = boxes[0];
            return Some((box0.rect.left, box0.rect.top, box0.rect.height()));
        }

        if index == 0 || {
            let prev_byte_index = self.paragraph.prev_glyph_byte_index(index)?;
            self.paragraph.is_line_break(prev_byte_index..index)
        } {
            let next_byte_index = self.paragraph.next_glyph_byte_index(index)?;
            let boxes = self.paragraph.get_rects_for_range(
                index..next_byte_index,
                RectHeightStyle::Max,
                RectWidthStyle::Tight,
            );
            let box0 = boxes[0];
            if box0.direct == TextDirection::LTR {
                Some((box0.rect.left, box0.rect.top, box0.rect.height()))
            } else {
                Some((box0.rect.right, box0.rect.top, box0.rect.height()))
            }
        } else {
            let prev_byte_index = self.paragraph.prev_glyph_byte_index(index)?;
            let boxes = self.paragraph.get_rects_for_range(
                prev_byte_index..index,
                RectHeightStyle::Max,
                RectWidthStyle::Tight,
            );
            let box0 = boxes[0];
            if box0.direct == TextDirection::LTR {
                Some((box0.rect.right, box0.rect.top, box0.rect.height()))
            } else {
                Some((box0.rect.left, box0.rect.top, box0.rect.height()))
            }
        }
    }

    pub fn get_rects_for_range(&self, range: Range<usize>) -> Vec<TextBox> {
        self.paragraph.get_rects_for_range(
            range,
            RectHeightStyle::Max,
            RectWidthStyle::Tight,
        )
    }

    pub fn get_closest_glyph_cluster_at(&self, point: impl Into<Point>) -> usize {
        let point = point.into();
        let point_clone = point.clone();
        let glyph_info = self.paragraph.get_closest_glyph_cluster_at(point);
        if let Some(glyph_info) = glyph_info {
            let bounds = glyph_info.bounds;
            let center_x = (bounds.left + bounds.right) / 2.0;
            if self.paragraph.is_line_break(glyph_info.text_range.clone()) {
                return glyph_info.text_range.start;
            }

            return if point_clone.x < center_x {
                if glyph_info.position == TextDirection::LTR {
                    glyph_info.text_range.start
                } else {
                    glyph_info.text_range.end
                }
            } else if glyph_info.position == TextDirection::LTR {
                glyph_info.text_range.end
            } else {
                glyph_info.text_range.start
            };
        }
        0
    }

    pub fn inner_paragraph(&self) -> &Paragraph {
        self.paragraph
    }
}
