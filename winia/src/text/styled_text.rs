use crate::text::text_style::TextStyle;
use crate::text::{StyleType, TextLayout};
use bimap::BiBTreeMap;
use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextAlign, TextDecoration,
    TextStyle as SkiaTextStyle,
};
use skia_safe::{FontMgr, FontStyle, Paint};
use std::collections::HashSet;
use std::fmt::Display;
use std::ops::{Add, Index, Range};
use std::str::FromStr;
use unicode_segmentation::UnicodeSegmentation;

thread_local! {
    /// The font collection used to create paragraphs.
    /// Creating a font collection is expensive so it is created once and shared across threads.
    static FONT_COLLECTION: FontCollection = {
        let mut font_collection = FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), None);
        font_collection
    }
}

pub(crate) fn font_collection() -> FontCollection {
    FONT_COLLECTION.with(|fc| fc.clone())
}

pub(crate) fn create_segments<'text>(
    text: &'text StyledText,
    range: &Range<usize>,
    text_style: SkiaTextStyle,
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

/// A segment of text with a specific style.
#[derive(Debug)]
struct StyleSegment<'text> {
    text: &'text str,
    range: Range<usize>,
    text_style: SkiaTextStyle,
}

impl<'text> StyleSegment<'text> {
    pub fn new(
        text: &'text StyledText,
        range: &Range<usize>,
        def_text_style: &SkiaTextStyle,
    ) -> StyleSegment<'text> {
        StyleSegment {
            text: text.as_str(),
            range: range.clone(),
            text_style: def_text_style.clone(),
        }
    }

    pub fn apply_style(&mut self, style: TextStyle) {
        match style {
            TextStyle::Bold => {
                let font_style = self.text_style.font_style();

                if font_style == FontStyle::italic() {
                    self.text_style.set_font_style(FontStyle::bold_italic());
                } else if font_style != FontStyle::bold() {
                    self.text_style.set_font_style(FontStyle::bold());
                }
            }
            TextStyle::Italic => {
                let font_style = self.text_style.font_style();

                if font_style == FontStyle::bold() {
                    self.text_style.set_font_style(FontStyle::bold_italic());
                } else if font_style != FontStyle::italic() {
                    self.text_style.set_font_style(FontStyle::italic());
                }
            }
            TextStyle::Underline => {
                let mut decoration = self.text_style.decoration().clone();
                decoration.ty.insert(TextDecoration::UNDERLINE);
                self.text_style.set_decoration(&decoration);
            }
            TextStyle::Strikethrough => {
                let mut decoration = self.text_style.decoration().clone();
                decoration.ty.insert(TextDecoration::LINE_THROUGH);
                self.text_style.set_decoration(&decoration);
            }
            TextStyle::FontSize(font_size) => {
                self.text_style.set_font_size(font_size);
            }
            TextStyle::BackgroundColor(color) => {
                self.text_style
                    .set_background_paint(Paint::default().set_color(color));
            }
            TextStyle::TextColor(color) => {
                self.text_style.set_color(color);
            }
        }
    }
}

trait AddStyleSegment {
    fn add_style_segment(&mut self, style_segment: &StyleSegment);
}

impl AddStyleSegment for ParagraphBuilder {
    fn add_style_segment(&mut self, style_segment: &StyleSegment) {
        // Apply the style to the paragraph
        self.push_style(&style_segment.text_style);
        self.add_text(&style_segment.text[style_segment.range.clone()]);
        self.pop();
    }
}

pub struct StyledText {
    string: String,
    /// Style,
    /// The range of the style,
    /// Should the range expand to include the text inserted at the end of the range or not
    styles: Vec<(TextStyle, Range<usize>, bool)>,
    /// Generating the indices is expensive so we only do it when using the indices.
    changed: bool,
    line_breaks: HashSet<Range<usize>>,
    byte_to_utf16_indices: BiBTreeMap<usize, usize>,
    byte_to_glyph_indices: BiBTreeMap<usize, usize>,
}

impl StyledText {
    fn new(string: String) -> Self {
        StyledText {
            string,
            styles: Vec::new(),
            changed: true,
            line_breaks: HashSet::new(),
            byte_to_utf16_indices: BiBTreeMap::new(),
            byte_to_glyph_indices: BiBTreeMap::new(),
        }
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

    pub fn create_paragraph(
        &mut self,
        default_text_style: skia_safe::textlayout::TextStyle,
        max_width: f32,
        text_align: TextAlign,
    ) -> Paragraph {
        let mut paragraph_style = ParagraphStyle::default();
        paragraph_style.set_text_align(text_align);

        let mut paragraph_builder = ParagraphBuilder::new(&paragraph_style, font_collection());

        if self.string.is_empty() {
            let mut text = self.clone();
            text.push(' ');
            create_segments(&text, &(0..text.len()), default_text_style)
                .iter()
                .for_each(|style_segment| {
                    paragraph_builder.add_style_segment(style_segment);
                });
        } else {
            create_segments(self, &(0..self.len()), default_text_style)
                .iter()
                .for_each(|style_segment| {
                    paragraph_builder.add_style_segment(style_segment);
                });
        };

        let mut paragraph = paragraph_builder.build();
        paragraph.layout(max_width);
        paragraph
    }

    pub fn get_text_layout<'a>(&'a mut self, paragraph: &'a Paragraph) -> TextLayout<'a> {
        if self.changed {
            self.generate_indices();
            self.changed = false;
        }
        TextLayout::new(
            paragraph,
            &self.line_breaks,
            &self.byte_to_utf16_indices,
            &self.byte_to_glyph_indices,
            self.len(),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }

    pub fn substring(&self, range: Range<usize>) -> StyledText {
        self.assert_in_range(&range);
        let string = self.string[range.clone()].to_string();
        let mut styles: Vec<(TextStyle, Range<usize>, bool)> = Vec::new();
        for (style, style_range, edge_behavior) in self.styles.iter() {
            // The start of the style range is in the substring
            if style_range.start >= range.start && style_range.start <= range.end {
                let new_start = style_range.start - range.start;
                let new_end = if style_range.end > range.end {
                    // The end of the style range is outside the substring
                    range.end - range.start
                } else {
                    style_range.end - range.start
                };
                styles.push((*style, new_start..new_end, edge_behavior.clone()));
            } else if style_range.end >= range.start && style_range.end <= range.end {
                let new_start = if style_range.start < range.start {
                    // The start of the style range is outside the substring
                    0
                } else {
                    style_range.start - range.start
                };
                let new_end = style_range.end - range.start;
                styles.push((*style, new_start..new_end, *edge_behavior));
            }
        }

        StyledText {
            string,
            styles,
            changed: true,
            line_breaks: HashSet::new(),
            byte_to_utf16_indices: BiBTreeMap::new(),
            byte_to_glyph_indices: BiBTreeMap::new(),
        }
    }

    pub fn byte_index_to_glyph_index(&mut self, byte_index: usize) -> usize {
        if self.changed {
            self.generate_indices();
            self.changed = false;
        }
        *self.byte_to_glyph_indices.get_by_left(&byte_index).unwrap()
    }

    pub fn glyph_index_to_byte_index(&mut self, glyph_index: usize) -> usize {
        if self.changed {
            self.generate_indices();
            self.changed = false;
        }
        *self
            .byte_to_glyph_indices
            .get_by_right(&glyph_index)
            .unwrap()
    }

    pub fn byte_index_to_utf16_index(&mut self, byte_index: usize) -> usize {
        if self.changed {
            self.generate_indices();
            self.changed = false;
        }
        *self.byte_to_utf16_indices.get_by_left(&byte_index).unwrap()
    }

    pub fn utf16_index_to_byte_index(&mut self, utf16_index: usize) -> usize {
        if self.changed {
            self.generate_indices();
            self.changed = false;
        }
        *self
            .byte_to_utf16_indices
            .get_by_right(&utf16_index)
            .unwrap()
    }

    pub fn len(&self) -> usize {
        self.string.len()
    }

    pub fn is_empty(&self) -> bool {
        self.string.is_empty()
    }

    pub fn insert_str(&mut self, index: usize, string: &str) {
        self.string.insert_str(index, string);
        self.styles.iter_mut().for_each(|(_, range, expanded)| {
            if index == range.end {
                // Inserted at the end of the range
                if *expanded {
                    range.end += string.len();
                }
            } else if index > range.start && index < range.end {
                // Inserted in the range
                range.end += string.len();
            } else if index <= range.start {
                // Inserted before the range
                range.start += string.len();
                range.end += string.len();
            }
        });
        self.changed = true;
    }

    pub fn insert(&mut self, index: usize, text: &StyledText) {
        self.insert_str(index, &text.string);
        text.styles.iter().for_each(|(style, range, expanded)| {
            self.set_style(
                style.clone(),
                (range.start + index)..(range.end + index),
                *expanded,
            );
        });
        self.changed = true;
    }

    pub fn remove(&mut self, range: Range<usize>) {
        self.string.drain(range.clone());
        self.styles.retain(|(_, style_range, _)| {
            // Remove the style if the range is inside the style range
            if style_range.start >= range.start && style_range.end <= range.end {
                return false;
            }
            true
        });
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
        self.changed = true;
    }

    pub fn append_str(&mut self, string: &str) {
        self.insert_str(self.string.len(), string);
        self.changed = true;
    }

    pub fn append(&mut self, text: &StyledText) {
        self.insert(self.string.len(), text);
        self.changed = true;
    }

    pub fn push(&mut self, c: char) {
        self.insert_str(self.string.len(), &c.to_string());
        self.changed = true;
    }

    pub fn clear(&mut self) {
        self.string.clear();
        self.styles.clear();
        self.changed = true;
    }

    fn assert_in_range(&self, range: &Range<usize>) {
        if range.start > self.string.len() || range.end > self.string.len() {
            panic!(
                "Range out of bounds. Range: {:?}, String length: {}",
                range,
                self.string.len()
            );
        }
    }

    pub fn set_style(&mut self, style: TextStyle, range: Range<usize>, expanded: bool) {
        self.assert_in_range(&range);

        self.remove_style(style.style_type(), range.clone());
        self.styles.push((style, range, expanded));
    }

    pub fn get_styles(&self, range: Range<usize>) -> Vec<(TextStyle, Range<usize>, bool)> {
        self.assert_in_range(&range);
        let mut styles: Vec<(TextStyle, Range<usize>, bool)> = Vec::new();
        for (style, style_range, expanded) in self.styles.iter() {
            if style_range.start >= range.start && style_range.end <= range.end {
                styles.push((*style, style_range.clone(), *expanded));
            }
        }
        styles
    }

    pub fn retain_styles(&mut self, f: impl Fn(&TextStyle, &Range<usize>, &bool) -> bool) {
        self.styles
            .retain(|(style, range, expanded)| f(style, range, expanded));
    }

    pub fn remove_style(&mut self, style_type: StyleType, range: Range<usize>) {
        self.assert_in_range(&range);

        let mut segmented_styles: Vec<(TextStyle, Range<usize>, bool)> = Vec::new();

        self.styles.retain(|(style, style_range, expanded)| {
            if style.style_type() == style_type {
                if range.start <= style_range.start {
                    return if range.end > style_range.start {
                        if range.end < style_range.end {
                            segmented_styles.push((*style, range.end..style_range.end, *expanded));
                        }
                        false
                    } else {
                        true
                    };
                } else if range.start == style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push((*style, range.end..style_range.end, *expanded));
                        return false;
                    } else if range.end >= style_range.end {
                        return false;
                    }
                } else if range.start > style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push((
                            *style,
                            style_range.start..range.start,
                            expanded.clone(),
                        ));
                        segmented_styles.push((
                            *style,
                            range.end..style_range.end,
                            expanded.clone(),
                        ));
                        return false;
                    } else if range.end >= style_range.end {
                        segmented_styles.push((
                            *style,
                            style_range.start..range.start,
                            expanded.clone(),
                        ));
                        return false;
                    }
                }
                return false;
            }
            true
        });

        self.styles.append(&mut segmented_styles);
    }

    pub fn clear_styles(&mut self) {
        self.styles.clear();
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
            changed: self.changed,
            line_breaks: self.line_breaks.clone(),
            byte_to_utf16_indices: self.byte_to_utf16_indices.clone(),
            byte_to_glyph_indices: self.byte_to_glyph_indices.clone(),
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
        let mut output = self as StyledText;
        output.append(rhs.as_ref());
        output
    }
}

impl<T: AsRef<StyledText> + 'static> Add<T> for &StyledText {
    type Output = StyledText;

    fn add(self, rhs: T) -> Self::Output {
        let mut output = self.clone();
        output.append(rhs.as_ref());
        output
    }
}

impl Add<&str> for StyledText {
    type Output = StyledText;

    fn add(self, rhs: &str) -> Self::Output {
        let mut output = self.clone();
        output.append_str(rhs);
        output
    }
}

impl Add<&str> for &StyledText {
    type Output = StyledText;

    fn add(self, rhs: &str) -> Self::Output {
        let mut output = self.clone();
        output.append_str(rhs);
        output
    }
}

impl Add<String> for StyledText {
    type Output = StyledText;

    fn add(self, rhs: String) -> Self::Output {
        let mut output = self.clone();
        output.append_str(&rhs);
        output
    }
}

impl Add<String> for &StyledText {
    type Output = StyledText;

    fn add(self, rhs: String) -> Self::Output {
        let mut output = self.clone();
        output.append_str(&rhs);
        output
    }
}

impl Add<&String> for StyledText {
    type Output = StyledText;

    fn add(self, rhs: &String) -> Self::Output {
        let mut output = self.clone();
        output.append_str(rhs);
        output
    }
}

impl Add<&String> for &StyledText {
    type Output = StyledText;

    fn add(self, rhs: &String) -> Self::Output {
        let mut output = self.clone();
        output.append_str(rhs);
        output
    }
}

impl Add<char> for StyledText {
    type Output = StyledText;

    fn add(self, rhs: char) -> Self::Output {
        let mut output = self.clone();
        output.push(rhs);
        output
    }
}

impl Add<char> for &StyledText {
    type Output = StyledText;

    fn add(self, rhs: char) -> Self::Output {
        let mut output = self.clone();
        output.push(rhs);
        output
    }
}
