use crate::text::{Style, StyledText};
use bimap::BiBTreeMap;
use skia_safe::textlayout::{
    Affinity, FontCollection, Paragraph, ParagraphBuilder, RectHeightStyle,
    RectWidthStyle, TextBox, TextDecoration, TextDirection, TextRange, TextStyle,
};
use skia_safe::{Canvas, FontMgr, FontStyle, Paint, Point};
use std::collections::HashSet;
use std::ops::Range;

thread_local! {
    static FONT_COLLECTION: FontCollection = {
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), None);
        font_collection
    }
}

pub(crate) fn font_collection() -> FontCollection {
    FONT_COLLECTION.with(|fc| fc.clone())
}

pub struct TextLayout<'a> {
    paragraph: &'a Paragraph,
    line_breaks: &'a HashSet<TextRange>,
    byte_to_utf16_indices: &'a BiBTreeMap<usize, usize>,
    byte_to_glyph_indices: &'a BiBTreeMap<usize, usize>,
    length: usize,
}

impl<'a> TextLayout<'a> {
    pub(crate) fn new(
        paragraph: &'a Paragraph,
        line_breaks: &'a HashSet<TextRange>,
        byte_to_utf16_indices: &'a BiBTreeMap<usize, usize>,
        byte_to_glyph_indices: &'a BiBTreeMap<usize, usize>,
        length: usize,
    ) -> TextLayout<'a> {
        TextLayout {
            paragraph,
            byte_to_utf16_indices,
            byte_to_glyph_indices,
            line_breaks,
            length,
        }
    }

    // pub fn re_layout(&mut self, max_width: f32) {
    //     self.paragraph.layout(max_width);
    // }

    pub fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        self.paragraph.paint(canvas, (x, y));
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
        let is_start = index == 0;

        let utf16_index = *self.byte_to_utf16_indices.get_by_left(&index)?;
        let glyph_index = *self.byte_to_glyph_indices.get_by_left(&index)?;

        if is_start || {
            let prev_byte_index = *self
                .byte_to_glyph_indices
                .get_by_right(&(glyph_index - 1))?;
            self.line_breaks.contains(&(prev_byte_index..index))
        } {
            let next_byte_index = *self
                .byte_to_glyph_indices
                .get_by_right(&(glyph_index + 1))?;
            let next_utf16_index = *self.byte_to_utf16_indices.get_by_left(&next_byte_index)?;
            let boxes = self.paragraph.get_rects_for_range(
                utf16_index..next_utf16_index,
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
            let prev_byte_index = *self
                .byte_to_glyph_indices
                .get_by_right(&(glyph_index - 1))?;
            let prev_utf16_index = *self.byte_to_utf16_indices.get_by_left(&prev_byte_index)?;
            let boxes = self.paragraph.get_rects_for_range(
                prev_utf16_index..utf16_index,
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
        let utf16_start_index = *self
            .byte_to_utf16_indices
            .get_by_left(&range.start)
            .unwrap();
        let utf16_end_index = *self.byte_to_utf16_indices.get_by_left(&range.end).unwrap();
        self.paragraph.get_rects_for_range(
            utf16_start_index..utf16_end_index,
            RectHeightStyle::Max,
            RectWidthStyle::Tight,
        )
    }

    pub fn glyph_index_to_byte_index(&self, glyph_index: usize) -> usize {
        if let Some(byte_index) = self.byte_to_glyph_indices.get_by_right(&glyph_index) {
            *byte_index
        } else {
            panic!(
                "glyph_index_to_byte_index: glyph_index not found {}",
                glyph_index
            );
        }
    }

    pub fn byte_index_to_glyph_index(&self, byte_index: usize) -> usize {
        if let Some(glyph_index) = self.byte_to_glyph_indices.get_by_left(&byte_index) {
            *glyph_index
        } else {
            panic!(
                "the index of {} is not a glyph cluster boundary",
                byte_index
            );
        }
    }

    pub fn utf16_index_to_byte_index(&self, utf16_index: usize) -> usize {
        if let Some(byte_index) = self.byte_to_utf16_indices.get_by_right(&utf16_index) {
            *byte_index
        } else {
            panic!(
                "the index of {} is not a glyph cluster boundary",
                utf16_index
            );
        }
    }

    pub fn get_closest_glyph_cluster_at(&self, point: impl Into<Point>) -> usize {
        let point = point.into();
        let point_clone = point.clone();
        let glyph_info = self.paragraph.get_closest_glyph_cluster_at(point);
        if let Some(glyph_info) = glyph_info {
            let bounds = glyph_info.bounds;
            let center_x = (bounds.left + bounds.right) / 2.0;
            if self.line_breaks.contains(&glyph_info.text_range) {
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

    pub fn get_glyph_position_at_coordinate(&self, x: f32, y: f32) -> Option<(usize, Affinity)> {
        let p_with_a = self
            .paragraph
            .get_glyph_position_at_coordinate(Point::new(x, y));
        Some((
            *self
                .byte_to_utf16_indices
                .get_by_right(&(p_with_a.position as usize))?,
            p_with_a.affinity,
        ))
    }

    pub fn inner_paragraph(&self) -> &Paragraph {
        self.paragraph
    }

    // pub fn inner_paragraph_mut(&mut self) -> &mut Paragraph {
    //     &mut self.paragraph
    // }
}

pub(crate) fn create_segments<'text>(
    text: &'text StyledText,
    range: &Range<usize>,
    text_style: TextStyle,
) -> Vec<StyleSegment<'text>> {
    let mut text_segments = Vec::new();

    let first_segment = StyleSegment::new(text, range, &text_style);
    text_segments.push(first_segment);
    text.get_styles(range.clone())
        .iter()
        .for_each(|(style, range, _)| {
            let mut index = 0;
            while index < text_segments.len() {
                if let Some(text_segment) = text_segments.get_mut(index) {
                    if text_segment.range.start >= range.end {
                        break;
                    }
                    if range.start <= text_segment.range.start
                        && range.end >= text_segment.range.end
                    {
                        text_segment.apply_style(*style);
                        index += 1;
                    } else if range.start > text_segment.range.start
                        && range.start < text_segment.range.end
                        && range.end > text_segment.range.start
                        && range.end < text_segment.range.end
                    {
                        let left_segment = StyleSegment::new(
                            text,
                            &(text_segment.range.start..range.start),
                            &text_segment.text_style,
                        );
                        let middle_segment = StyleSegment::new(
                            text,
                            &(range.start..range.end),
                            &text_segment.text_style,
                        );
                        let right_segment = StyleSegment::new(
                            text,
                            &(range.end..text_segment.range.end),
                            &text_segment.text_style,
                        );
                        text_segments.remove(index);
                        text_segments.push(left_segment);
                        text_segments.push(middle_segment);
                        text_segments.push(right_segment);
                    } else if range.start > text_segment.range.start
                        && range.start < text_segment.range.end
                    {
                        let left_segment = StyleSegment::new(
                            text,
                            &(text_segment.range.start..range.start),
                            &text_segment.text_style,
                        );
                        let right_segment = StyleSegment::new(
                            text,
                            &(range.start..text_segment.range.end),
                            &text_segment.text_style,
                        );
                        text_segments.remove(index);
                        text_segments.push(left_segment);
                        text_segments.push(right_segment);
                    } else if range.end > text_segment.range.start
                        && range.end < text_segment.range.end
                    {
                        let left_segment = StyleSegment::new(
                            text,
                            &(text_segment.range.start..range.end),
                            &text_segment.text_style,
                        );
                        let right_segment = StyleSegment::new(
                            text,
                            &(range.end..text_segment.range.end),
                            &text_segment.text_style,
                        );
                        text_segments.remove(index);
                        text_segments.push(left_segment);
                        text_segments.push(right_segment);
                    } else {
                        index += 1;
                    }
                }
            }
        });

    text_segments
}

#[derive(Debug)]
pub(crate) struct StyleSegment<'text> {
    text: &'text str,
    range: Range<usize>,
    text_style: TextStyle,
}

impl<'text> StyleSegment<'text> {
    pub fn new(
        text: &'text StyledText,
        range: &Range<usize>,
        def_text_style: &TextStyle,
    ) -> StyleSegment<'text> {
        StyleSegment {
            text: text.as_str(),
            range: range.clone(),
            text_style: def_text_style.clone(),
        }
    }

    pub fn apply_style(&mut self, style: Style) {
        match style {
            Style::Bold => {
                let font_style = self.text_style.font_style();

                if font_style == FontStyle::italic() {
                    self.text_style.set_font_style(FontStyle::bold_italic());
                } else if font_style != FontStyle::bold() {
                    self.text_style.set_font_style(FontStyle::bold());
                }
            }
            Style::Italic => {
                let font_style = self.text_style.font_style();

                if font_style == FontStyle::bold() {
                    self.text_style.set_font_style(FontStyle::bold_italic());
                } else if font_style != FontStyle::italic() {
                    self.text_style.set_font_style(FontStyle::italic());
                }
            }
            Style::Underline => {
                let mut ty = self.text_style.decoration().clone();
                ty.ty.insert(TextDecoration::UNDERLINE);
                self.text_style.set_decoration(&ty);
            }
            Style::Strikethrough => {
                let mut ty = self.text_style.decoration().clone();
                ty.ty.insert(TextDecoration::LINE_THROUGH);
                self.text_style.set_decoration(&ty);
            }
            Style::FontSize(font_size) => {
                self.text_style.set_font_size(font_size);
            }
            Style::BackgroundColor(color) => {
                self.text_style
                    .set_background_paint(Paint::default().set_color(color));
            }
            Style::TextColor(color) => {
                self.text_style.set_color(color);
            }
        }
    }
}

pub(crate) trait AddStyleSegment {
    fn add_style_segment(&mut self, style_segment: &StyleSegment);
}

impl AddStyleSegment for ParagraphBuilder {
    fn add_style_segment(&mut self, style_segment: &StyleSegment) {
        self.push_style(&style_segment.text_style);
        self.add_text(&style_segment.text[style_segment.range.clone()]);
        self.pop();
    }
}
