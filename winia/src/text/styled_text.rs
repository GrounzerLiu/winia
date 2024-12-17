use crate::text::style::Style;
use crate::text::{create_segments, font_collection, AddStyleSegment, EdgeBehavior, StyleType, TextLayout};
use bimap::BiBTreeMap;
use skia_safe::textlayout::{Paragraph, ParagraphBuilder, ParagraphStyle, TextAlign, TextRange, TextStyle};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::ops::{Add, Index, Range};
use std::str::FromStr;
use skia_safe::Color;
use unicode_segmentation::UnicodeSegmentation;

pub struct StyledText {
    string: String,
    styles: Vec<(Style, Range<usize>, EdgeBehavior)>,
    line_breaks: HashSet<Range<usize>>,
    byte_to_utf16_indices: BiBTreeMap<usize, usize>,
    byte_to_glyph_indices: BiBTreeMap<usize, usize>,
    paragraph: Option<Paragraph>,
}

impl StyledText {
    fn new(string: String) -> Self {
        let mut s = StyledText {
            string,
            styles: Vec::new(),
            line_breaks: HashSet::new(),
            byte_to_utf16_indices: BiBTreeMap::new(),
            byte_to_glyph_indices: BiBTreeMap::new(),
            paragraph: None,
        };
        s.generate_indices();
        s
    }

    fn generate_indices(&mut self) {
        self.byte_to_utf16_indices.clear();
        self.byte_to_glyph_indices.clear();
        self.line_breaks.clear();

        let mut last_utf16_index = 0;
        let mut last_glyph_index = 0;

        self.string.grapheme_indices(false).enumerate().for_each(
            |(glyph_index, (byte_index, str))| {
                self.byte_to_utf16_indices
                    .insert(byte_index, last_utf16_index);
                self.byte_to_glyph_indices.insert(byte_index, glyph_index);

                let utf16_length = str.encode_utf16().count();
                last_utf16_index += utf16_length;

                if str == "\r\n" || str == "\n" || str == "\r" {
                    self.line_breaks.insert(byte_index..byte_index + str.len());
                }
                last_glyph_index = glyph_index + 1;
            },
        );

        self.byte_to_utf16_indices
            .insert(self.string.len(), last_utf16_index);
        self.byte_to_glyph_indices
            .insert(self.string.len(), last_glyph_index);
    }

    pub fn byte_index_to_glyph_index(&self, byte_index: usize) -> usize {
        *self.byte_to_glyph_indices.get_by_left(&byte_index).unwrap()
    }

    pub fn glyph_index_to_byte_index(&self, glyph_index: usize) -> usize {
        *self.byte_to_glyph_indices.get_by_right(&glyph_index).unwrap()
    }

    pub fn byte_index_to_utf16_index(&self, byte_index: usize) -> usize {
        *self.byte_to_utf16_indices.get_by_left(&byte_index).unwrap()
    }

    pub fn utf16_index_to_byte_index(&self, utf16_index: usize) -> usize {
        *self.byte_to_utf16_indices.get_by_right(&utf16_index).unwrap()
    }


    pub fn create_text_layout(&mut self, max_width: f32, text_align: TextAlign) {
        let mut text_style = TextStyle::default();
        text_style.set_font_size(16.0);
        text_style.set_color(Color::BLACK);

        let mut paragraph_style = ParagraphStyle::default();
        paragraph_style.set_text_align(text_align);

        let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection());

        if self.string.is_empty() {
            let mut text = self.clone();
            text.push(' ');
            // for (style, range, _) in self.styles.iter() {
            //     if range.start == 0 && range.end == 0 {
            //         if let Style::FontSize(size) = style {
            //             text.set_style(style.clone(), 0..1, EdgeBehavior::ExcludeAndExclude);
            //         }
            //     }
            // }
            create_segments(&text, &(0..text.len()), text_style)
                .iter()
                .for_each(|style_segment| {
                    paragraph_builder.add_style_segment(style_segment);
                });
        } else {
            create_segments(self, &(0..self.len()), text_style)
                .iter()
                .for_each(|style_segment| {
                    paragraph_builder.add_style_segment(style_segment);
                });
        };

        let mut paragraph = paragraph_builder.build();
        paragraph.layout(max_width);
        self.paragraph = Some(paragraph);
    }

    pub fn get_text_layout(&self) -> Option<TextLayout>{
        if let Some(paragraph) = &self.paragraph {
            Some(TextLayout::new(paragraph, &self.line_breaks, &self.byte_to_utf16_indices, &self.byte_to_glyph_indices, self.len()))
        } else {
            None
        }
    }

    pub fn has_text_layout(&self) -> bool {
        self.paragraph.is_some()
    }

    pub fn reset_text_layout_width(&mut self, max_width: f32) {
        if let Some(paragraph) = &mut self.paragraph {
            paragraph.layout(max_width);
        }
    }

    fn update(&mut self) {
        self.generate_indices();
        self.paragraph.take();
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn substring(&self, range: Range<usize>) -> StyledText {
        self.assert_in_range(&range);
        let string = self.string[range.clone()].to_string();
        let mut styles: Vec<(Style, Range<usize>, EdgeBehavior)> = Vec::new();
        for (style, style_range, edge_behavior) in self.styles.iter() {
            if style_range.start < range.start {
                if style_range.end > range.start {
                    styles.push((
                        style.clone(),
                        style_range.start - range.start..style_range.end - range.start,
                        edge_behavior.clone(),
                    ));
                }
            } else if style_range.start >= range.start && style_range.end < range.end {
                styles.push((
                    style.clone(),
                    style_range.start - range.start..style_range.end - range.start,
                    edge_behavior.clone(),
                ));
            }
        }
        let mut styled_text = StyledText{
            string,
            styles,
            line_breaks: HashSet::new(),
            byte_to_utf16_indices: BiBTreeMap::new(),
            byte_to_glyph_indices: BiBTreeMap::new(),
            paragraph: None,
        };
        styled_text.generate_indices();
        styled_text
    }

    pub fn len(&self) -> usize {
        self.string.len()
    }

    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    pub fn insert(&mut self, index: usize, string: &str) {
        self.string.insert_str(index, string);
        self.styles.iter_mut().for_each(|(_, range, _)| {
            if range.start == range.end && range.start == index {
                range.end += string.len();
            } else {
                if range.start >= index {
                    range.start += string.len();
                }
                if range.end >= index {
                    range.end += string.len();
                }
            }
        });
        self.update();
    }

    pub fn remove(&mut self, range: Range<usize>) {
        self.string.drain(range.clone());
        self.styles.iter_mut().for_each(|(_, style_range, _)| {
            if style_range.start >= range.end {
                style_range.start -= range.end - range.start;
            } else if style_range.start >= range.start {
                style_range.start = range.start;
            }
            if style_range.end >= range.end {
                style_range.end -= range.end - range.start;
            } else if style_range.end >= range.start {
                style_range.end = range.start;
            }
        });
        self.update();
    }

    pub fn append(&mut self, string: &str) {
        self.insert(self.string.len(), string);
    }

    pub fn push(&mut self, c: char) {
        self.insert(self.string.len(), &c.to_string());
    }

    pub fn clear(&mut self) {
        self.string.clear();
        self.styles.clear();
        self.update();
    }

    fn assert_in_range(&self, range: &Range<usize>) {
        if range.start > self.string.len() || range.end > self.string.len() {
            panic!("Range out of bounds. Range: {:?}, String length: {}", range, self.string.len());
        }
    }

    pub fn set_style(&mut self, style: Style, range: Range<usize>, boundary_type: EdgeBehavior) {
        self.assert_in_range(&range);

        let style_clone = style.clone();

        self.remove_style(style, range.clone());
        self.styles.push((style_clone, range, boundary_type));
        self.paragraph.take();
    }

    pub fn get_styles(&self, range: Range<usize>) -> Vec<(Style, Range<usize>, EdgeBehavior)> {
        self.assert_in_range(&range);
        let mut styles: Vec<(Style, Range<usize>, EdgeBehavior)> = Vec::new();
        for (style, style_range, edge_behavior) in self.styles.iter() {
            if style_range.start >= range.start && style_range.end <= range.end {
                styles.push((*style, style_range.clone(), *edge_behavior));
            }
        }
        styles
    }

    pub fn retain_styles(&mut self, f: impl Fn(&Style, &Range<usize>, &EdgeBehavior) -> bool) {
        self.styles.retain(|(style, range, edge_behavior)| f(style, range, edge_behavior));
        self.paragraph.take();
    }

    pub fn remove_style(&mut self, style: Style, range: Range<usize>) {
        self.assert_in_range(&range);

        let mut segmented_styles: Vec<(Style, Range<usize>, EdgeBehavior)> = Vec::new();

        self.styles.retain(|(s, style_range, boundary_type)| {
            if s.name() == style.name() {
                if range.start <= style_range.start {
                    if range.end > style_range.start {
                        if range.end < style_range.end {
                            segmented_styles.push((
                                *s,
                                range.end..style_range.end,
                                boundary_type.clone(),
                            ));
                        }
                        return false;
                    } else {
                        return true;
                    }
                } else if range.start == style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push((
                            *s,
                            range.end..style_range.end,
                            boundary_type.clone(),
                        ));
                        return false;
                    } else if range.end >= style_range.end {
                        return false;
                    }
                } else if range.start > style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push((
                            *s,
                            style_range.start..range.start,
                            boundary_type.clone(),
                        ));
                        segmented_styles.push((
                            *s,
                            range.end..style_range.end,
                            boundary_type.clone(),
                        ));
                        return false;
                    } else if range.end >= style_range.end {
                        segmented_styles.push((
                            *s,
                            style_range.start..range.start,
                            boundary_type.clone(),
                        ));
                        return false;
                    }
                }
                return false;
            }
            true
        });

        self.styles.append(&mut segmented_styles);
        self.paragraph.take();
    }

    pub fn clear_styles(&mut self) {
        self.styles.clear();
        self.paragraph.take();
    }
}

impl Display for StyledText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string.to_string())
    }
}

impl PartialEq for StyledText {
    fn eq(&self, other: &Self) -> bool {
        self.string == other.string
    }
}

impl PartialEq<String> for StyledText {
    fn eq(&self, other: &String) -> bool {
        self.string == *other
    }
}

impl PartialEq<StyledText> for String {
    fn eq(&self, other: &StyledText) -> bool {
        *self == other.string
    }
}

impl PartialEq<&str> for StyledText {
    fn eq(&self, other: &&str) -> bool {
        self.string == *other
    }
}

impl PartialEq<StyledText> for &str {
    fn eq(&self, other: &StyledText) -> bool {
        *self == other.string
    }
}

impl From<String> for StyledText {
    fn from(string: String) -> Self {
        StyledText::new(string)
    }
}

impl From<&str> for StyledText {
    fn from(string: &str) -> Self {
        StyledText::new(string.to_string())
    }
}

impl FromStr for StyledText {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(StyledText::new(s.to_string()))
    }
}

impl Clone for StyledText {
    /// Observers are not cloned because closures cannot be cloned. And different instances of StyledText should have different observers.
    fn clone(&self) -> Self {
        StyledText {
            string: self.string.clone(),
            styles: self.styles.clone(),
            line_breaks: self.line_breaks.clone(),
            byte_to_utf16_indices: self.byte_to_utf16_indices.clone(),
            byte_to_glyph_indices: self.byte_to_glyph_indices.clone(),
            paragraph: None,
        }
    }
}

impl Index<Range<usize>> for StyledText {
    type Output = str;

    fn index(&self, index: Range<usize>) -> &Self::Output {
        &self.string[index]
    }
}

impl AsRef<StyledText> for StyledText {
    fn as_ref(&self) -> &StyledText {
        self
    }
}

impl<T: AsRef<StyledText> + 'static> Add<T> for StyledText {
    type Output = StyledText;

    fn add(self, rhs: T) -> Self::Output {
        let mut output = StyledText::new(self.string.clone() + &rhs.as_ref().string);
        self.styles
            .iter()
            .for_each(|(style, range, edge_behavior)| {
                output.set_style(style.clone(), range.clone(), *edge_behavior);
            });

        let self_len = self.string.len();
        rhs.as_ref()
            .styles
            .iter()
            .for_each(|(style, range, edge_behavior)| {
                output.set_style(
                    style.clone(),
                    (range.start + self_len)..(range.end + self_len),
                    *edge_behavior,
                );
            });

        output
    }
}
