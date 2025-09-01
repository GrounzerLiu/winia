use crate::shared::SharedDrawable;
use bimap::BiBTreeMap;
use skia_safe::textlayout::paragraph::{ExtendedVisitorInfo, FontInfo, GlyphClusterInfo, GlyphInfo, Paragraph as SkParagraph, VisitorInfo};
use skia_safe::textlayout::{Affinity, LineMetrics, RectHeightStyle, RectWidthStyle, TextBox, TextRange};
use skia_safe::{scalar, Canvas, Font, Path, Point, TextBlob, Unichar};
use std::collections::HashSet;
use std::ops::Range;

pub struct Paragraph {
    paragraph: SkParagraph,
    placeholders: Vec<SharedDrawable>,
    line_breaks: HashSet<Range<usize>>,
    paragraph_byte_to_real_index: BiBTreeMap<usize, usize>,
    byte_to_utf16_indices: BiBTreeMap<usize, usize>,
    byte_to_glyph_indices: BiBTreeMap<usize, usize>,
}

impl Paragraph {
    pub(crate) fn new(
        paragraph: SkParagraph,
        placeholders: &Vec<SharedDrawable>,
        line_breaks: &HashSet<Range<usize>>,
        paragraph_byte_to_real_index: &BiBTreeMap<usize, usize>,
        byte_to_utf16_indices: &BiBTreeMap<usize, usize>,
        byte_to_glyph_indices: &BiBTreeMap<usize, usize>,
    ) -> Self {
        Self {
            paragraph,
            placeholders: placeholders.clone(),
            line_breaks: line_breaks.clone(),
            paragraph_byte_to_real_index: paragraph_byte_to_real_index.clone(),
            byte_to_utf16_indices: byte_to_utf16_indices.clone(),
            byte_to_glyph_indices: byte_to_glyph_indices.clone(),
        }
    }
    
    pub fn is_line_break(&self, range: Range<usize>) -> bool {
        self.line_breaks.contains(&range)
    }

    fn get_utf16_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_index
            .get_by_right(&index)?;
        let utf16_index = self
            .byte_to_utf16_indices
            .get_by_left(paragraph_index)?;
        Some(*utf16_index)
    }

    pub fn get_glyph_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_index
            .get_by_right(&index)?;
        let glyph_index = self
            .byte_to_glyph_indices
            .get_by_left(paragraph_index)?;
        Some(*glyph_index)
    }
    
    pub fn prev_glyph_byte_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_index
            .get_by_right(&index)?;
        let glyph_index = self
            .byte_to_glyph_indices
            .get_by_left(paragraph_index)?;
        let prev_glyph_index = glyph_index.checked_sub(1)?;
        let prev_byte_index = self
            .byte_to_glyph_indices
            .get_by_right(&prev_glyph_index)?;
        let prev_byte_index = self
            .paragraph_byte_to_real_index
            .get_by_left(prev_byte_index)?;
        Some(*prev_byte_index)
    }
    
    pub fn next_glyph_byte_index(&self, index: usize) -> Option<usize> {
        let paragraph_index = self
            .paragraph_byte_to_real_index
            .get_by_right(&index)?;
        let glyph_index = self
            .byte_to_glyph_indices
            .get_by_left(paragraph_index)?;
        let next_glyph_index = glyph_index + 1;
        let next_byte_index = self
            .byte_to_glyph_indices
            .get_by_right(&next_glyph_index)?;
        let next_byte_index = self
            .paragraph_byte_to_real_index
            .get_by_left(next_byte_index)?;
        Some(*next_byte_index)
    }

    pub fn get_index(&self, utf16_index: usize) -> usize {
        let paragraph_index = self
            .byte_to_utf16_indices
            .get_by_right(&utf16_index)
            .unwrap_or_else(|| panic!("index {} not found", utf16_index));
        let index = self
            .paragraph_byte_to_real_index
            .get_by_left(paragraph_index)
            .unwrap_or_else(|| panic!("index {} not found", paragraph_index));
        *index
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
                        .lock()
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
        text_range.start = self.get_index(text_range.start);
        text_range.end = self.get_index(text_range.end);
        text_range
    }

    pub fn get_glyph_cluster_at(
        &self,
        index: usize,
    ) -> Option<GlyphClusterInfo> {
        if let Some(mut glyph_cluster_info) = self.paragraph.get_glyph_cluster_at(index) {
            glyph_cluster_info.text_range.start = *self.paragraph_byte_to_real_index.get_by_left(&glyph_cluster_info.text_range.start).unwrap();
            glyph_cluster_info.text_range.end = *self.paragraph_byte_to_real_index.get_by_left(&glyph_cluster_info.text_range.end).unwrap();
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
            glyph_info.text_range.start = *self.paragraph_byte_to_real_index.get_by_left(&glyph_info.text_range.start).unwrap();
            glyph_info.text_range.end = *self.paragraph_byte_to_real_index.get_by_left(&glyph_info.text_range.end).unwrap();
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
    use skia_safe::FontMgr;
    use skia_safe::textlayout::{FontCollection, ParagraphStyle, RectHeightStyle, RectWidthStyle, TextStyle};
    use unicode_segmentation::UnicodeSegmentation;
    use crate::shared::SharedDrawable;
    use crate::text::ParagraphBuilder;

    //#[test]
    // fn test_paragraph() {
    //     let text = "eat d🤗eeeeee 你好世界👩🏽‍🦰三";
    //     let mut font_collection = FontCollection::new();
    //     font_collection.set_default_font_manager(FontMgr::default(), None);
    //     let paragraph_style = ParagraphStyle::default();
    //     let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection);
    //     let mut text_style = TextStyle::new();
    //     text_style.set_font_size(30.0);
    // 
    //     paragraph_builder.push_style(&text_style);
    //     paragraph_builder.add_text("eat ");
    //     paragraph_builder.pop();
    // 
    //     paragraph_builder.push_style(&text_style);
    //     paragraph_builder.add_placeholder("d", SharedDrawable::from_file("/home/grounzer/Downloads/check_box_selected.svg").unwrap());
    //     paragraph_builder.pop();
    // 
    //     paragraph_builder.push_style(&text_style);
    //     paragraph_builder.add_text("🤗");
    //     paragraph_builder.pop();
    // 
    //     paragraph_builder.push_style(&text_style);
    //     paragraph_builder.add_placeholder("eeeeee", SharedDrawable::from_file("/home/grounzer/Downloads/check_box_selected.svg").unwrap());
    //     paragraph_builder.pop();
    // 
    //     paragraph_builder.push_style(&text_style);
    //     paragraph_builder.add_text(" 你好世界👩🏽‍🦰三");
    //     paragraph_builder.pop();
    // 
    //     // println!("text: {}", text);
    //     // println!("{}", paragraph_builder.get_text());
    //     // let mut last_utf16_index = 0;
    //     // paragraph_builder.get_text().grapheme_indices(false).for_each(|(index, str)| {
    //     //     println!("start: {}, end: {}, str: {}", index, index + str.len(), str);
    //     //     println!("utf16_index: {}", last_utf16_index);
    //     //     last_utf16_index += str.encode_utf16().count();
    //     //     println!("utf16_index: {}", last_utf16_index);
    //     // });
    // 
    //     let mut paragraph = paragraph_builder.build();
    //     paragraph.layout(90.0);
    // 
    //     println!("paragraph_byte_to_real_index: {:?}", paragraph.paragraph_byte_to_real_index);
    //     println!("byte_to_utf16_indices: {:?}", paragraph.byte_to_utf16_indices);
    //     println!("byte_to_glyph_indices: {:?}", paragraph.byte_to_glyph_indices);
    //     
    //     println!("width: {}", paragraph.max_width());
    //     println!("height: {}", paragraph.height());
    //     paragraph.get_line_metrics().iter().for_each(|line| {
    //         println!("line: start: {}, end: {}", line.start_index, line.end_index);
    //     });
    //     // paragraph.get_rects_for_range(0..18, RectHeightStyle::Max, RectWidthStyle::Tight).iter().for_each(|rect| {
    //     //     println!("rect: {:?}", rect);
    //     // });
    // 
    //     println!();
    // 
    //     // for (index, str) in text.grapheme_indices(false) {
    //     //     let rect = paragraph.get_rects_for_range(index..index + str.len(), RectHeightStyle::Max, RectWidthStyle::Tight);
    //     //     let center = rect[0].rect.center();
    //     //     let p = paragraph.get_glyph_position_at_coordinate(center);
    //     //     println!("index: {}, str: {}, rect: {:?}, p: {:?}", index, str, rect[0], p);
    //     // }
    //     println!();
    //     let mut last_index = 0;
    //     for (byte_index, glyph) in paragraph.byte_to_glyph_indices.clone().iter() {
    //         if *byte_index == 0 {
    //             continue;
    //         }
    //         let start = *paragraph.paragraph_byte_to_real_index.get_by_left(&last_index).unwrap();
    //         let end = *paragraph.paragraph_byte_to_real_index.get_by_left(byte_index).unwrap();
    //         println!("start: {}, end: {}", start, end);
    //         let rect = paragraph.get_rects_for_range(start..end, RectHeightStyle::Max, RectWidthStyle::Tight);
    //         let mut center = rect[0].rect.center();
    //         center.x -= 5.0;
    //         let p = paragraph.get_glyph_position_at_coordinate(center);
    //         println!("get_rects_for_range:");
    //         println!("\tx: {}, y: {}, width: {}, height: {}", rect[0].rect.left, rect[0].rect.top, rect[0].rect.width(), rect[0].rect.height());
    //         println!("get_glyph_position_at_coordinate:");
    //         println!("\tp: {:?}", p);
    //         println!("get_word_boundary:");
    //         let range = paragraph.get_word_boundary(start);
    //         println!("\trange: {:?}", range);
    //         println!("get_line_number_at:");
    //         println!("\tline {:?}", paragraph.get_line_number_at(start));
    //         println!("get_glyph_cluster_at:");
    //         println!("\t{:?}", paragraph.get_glyph_cluster_at(start));
    //         println!("get_closest_glyph_cluster_at:");
    //         println!("\t{:?}", paragraph.get_closest_glyph_cluster_at(center));
    //         println!("get_font_at:");
    //         println!("\t{:?}", paragraph.get_font_at(start));
    //         
    //         println!();
    //         
    //         
    //         last_index = *byte_index;
    //     }
    // 
    // }
}

/*
line: start: 0, end: 5
line: start: 5, end: 16
line: start: 16, end: 25
line: start: 25, end: 43
line: start: 43, end: 46
*/