use crate::text::text_style::TextStyle;
use crate::text::{Paragraph, ParagraphBuilder, StyleType, TextLayout, Typeface};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use skia_safe::textlayout::{
    FontCollection, ParagraphStyle, TextAlign, TextDecoration, TextStyle as SkiaTextStyle,
    TypefaceFontProvider,
};
use skia_safe::{FontMgr, FontStyle, Paint};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::ops::{Add, Deref, Index, Range};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
use crate::shared::SharedDrawable;

lazy_static!(
    /// (The path to the font file, The family name)
    static ref TYPEFACE_CACHE: Arc<Mutex<HashMap<PathBuf, String>>> = Arc::new(Mutex::new(HashMap::new()));
);

thread_local! {

    static TYPEFACE_FONT_PROVIDER: TypefaceFontProvider = TypefaceFontProvider::new();

    static FONT_MANAGER: FontMgr = FontMgr::default();

    /// The font collection used to create paragraphs.
    /// Creating a font collection is expensive so it is created once and shared across threads.
    static FONT_COLLECTION: FontCollection = {
        let mut font_collection = FontCollection::new();
        let font_manager = FONT_MANAGER.with(|fm| fm.clone());
        font_collection.set_default_font_manager(font_manager, None);
        let typeface_font_provider = typeface_font_provider();
        font_collection.set_asset_font_manager(typeface_font_provider.deref().clone());
        font_collection
    }
}

pub fn typeface_font_provider() -> TypefaceFontProvider {
    TYPEFACE_FONT_PROVIDER.with(|t| t.clone())
}

pub fn font_manager() -> FontMgr {
    FONT_MANAGER.with(|fm| fm.clone())
}

pub fn font_collection() -> FontCollection {
    FONT_COLLECTION.with(|fc| fc.clone())
}

/// Loads a typeface from the specified path and registers it in the font system
///
/// # Parameters
/// * `path` - Path to the font file, convertible to PathBuf
///
/// # Returns
/// * `Some(String)` - The font family name when successful
/// * `None` - When loading or registering the font fails
pub fn load_typeface_from_path(path: impl Into<PathBuf>) -> Option<String> {
    let mut typeface_cache = TYPEFACE_CACHE.lock();
    let path = path.into();
    if let Some(typeface) = typeface_cache.get(&path) {
        Some(typeface.clone())
    } else {
        let data = std::fs::read(path.clone()).ok()?;
        if let Some(typeface) = font_manager().new_from_data(&data, None) {
            let family_name = typeface.family_name();
            typeface_font_provider().register_typeface(typeface.clone(), None);
            typeface_cache.insert(path.clone(), family_name.clone());
            Some(family_name)
        } else {
            None
        }
    }
}

pub(crate) fn create_segments<'text>(
    text: &'text StyledText,
    range: &Range<usize>,
    text_style: &SkiaTextStyle,
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
                    if text_segment.range.start >= range.end {// The segment has no intersection with the range
                        break;
                    }
                    if range.start <= text_segment.range.start
                        && range.end >= text_segment.range.end
                    {// The segment is completely inside the range, so we can apply the style
                        text_segment.apply_style(style);
                        index += 1;
                    } else if range.start > text_segment.range.start
                        && range.start < text_segment.range.end
                        && range.end > text_segment.range.start
                        && range.end < text_segment.range.end
                        && text_segment.image.is_none()
                    {// The segment is inside the range, but not completely
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
                        text_segments.insert(index, left_segment);
                        text_segments.insert(index + 1, middle_segment);
                        text_segments.insert(index + 2, right_segment);
                    } else if range.start > text_segment.range.start
                        && range.start < text_segment.range.end
                        && text_segment.image.is_none()
                    {// The right side of the segment is inside the range
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
                        text_segments.insert(index, left_segment);
                        text_segments.insert(index + 1, right_segment);
                    } else if range.end > text_segment.range.start
                        && range.end < text_segment.range.end
                        && text_segment.image.is_none()
                    {// The left side of the segment is inside the range
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
                        text_segments.insert(index, left_segment);
                        text_segments.insert(index + 1, right_segment);
                    } else {// The segment is completely outside the range
                        index += 1;
                    }
                }
            }
        });
    text_segments
}

/// A segment of text with a specific style.
pub(crate) struct StyleSegment<'text> {
    text: &'text str,
    range: Range<usize>,
    text_style: SkiaTextStyle,
    image: Option<SharedDrawable>,
}

impl Debug for StyleSegment<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StyleSegment")
            .field("text", &&self.text[self.range.clone()])
            .finish()
    }
}

impl<'text> StyleSegment<'text> {
    pub fn new(
        text: &'text StyledText,
        range: &Range<usize>,
        def_text_style: &SkiaTextStyle,
    ) -> StyleSegment<'text> {
        let text_style = def_text_style.clone();
        StyleSegment {
            text: text.as_str(),
            range: range.clone(),
            text_style,
            image: None,
        }
    }

    pub fn apply_style(&mut self, style: impl AsRef<TextStyle>) {
        let style = style.as_ref();
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
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::UNDERLINE);
                self.text_style.set_decoration(&decoration);
            }
            TextStyle::Strikethrough => {
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::LINE_THROUGH);
                self.text_style.set_decoration(&decoration);
            }
            TextStyle::FontSize(font_size) => {
                self.text_style.set_font_size(*font_size);
            }
            TextStyle::BackgroundColor(color) => {
                self.text_style
                    .set_background_paint(Paint::default().set_color(*color));
            }
            TextStyle::TextColor(color) => {
                self.text_style.set_color(*color);
            }
            TextStyle::Weight(weight) => {
                let font_style = self.text_style.font_style();
                self.text_style.set_font_style(FontStyle::new(
                    *weight,
                    font_style.width(),
                    font_style.slant(),
                ));
            }
            TextStyle::Tracking(tracking) => {
                self.text_style.set_letter_spacing(*tracking);
            }
            TextStyle::Typeface(typeface) => match typeface {
                Typeface::Family(family) => {
                    self.text_style.set_font_families(&[family]);
                }
                Typeface::FontFile(path) => {
                    if let Some(family) = load_typeface_from_path(path) {
                        self.text_style.set_font_families(&[family]);
                    }
                }
            },
            TextStyle::Subscript => {
                let font_size = self.text_style.font_size();
                self.text_style.set_font_size(font_size * 0.58);
                self.text_style.set_baseline_shift(font_size * 0.15);
            }
            TextStyle::Superscript => {
                let font_size = self.text_style.font_size();
                self.text_style.set_font_size(font_size * 0.58);
                self.text_style.set_baseline_shift(font_size * -0.30);
            }
            TextStyle::Image(image) => {
                // self.text_style.set_placeholder();
                self.image = Some(image.clone());
            }
        }
    }
}

pub(crate) trait AddStyleSegment {
    fn add_style_segment(&mut self, style_segment: &StyleSegment);
}

impl AddStyleSegment for ParagraphBuilder {
    fn add_style_segment(&mut self, style_segment: &StyleSegment) {
        if let Some(image) = &style_segment.image {
            self.push_style(&style_segment.text_style);
            self.add_placeholder(&style_segment.text[style_segment.range.clone()], image.clone());
            self.pop();
        } else {
            self.push_style(&style_segment.text_style);
            self.add_text(&style_segment.text[style_segment.range.clone()]);
            self.pop();
        }
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
}

impl StyledText {
    fn new(string: String) -> Self {
        StyledText {
            string,
            styles: Vec::new(),
            changed: true,
        }
    }
    
    pub fn prev_glyph_index(&self, index: usize) -> Option<usize> {
        if index == 0 {
            return None;
        }
        if let Some(str) = self.string[..index].graphemes(false).next_back() {
            index.checked_sub(str.len())
        }else {
            None
        }
    }
    pub fn next_glyph_index(&self, index: usize) -> Option<usize> {
        if index >= self.string.len() {
            return None;
        }
        if let Some(str) = self.string[index..].graphemes(false).next() {
            index.checked_add(str.len())
        } else {
            None
        }
    }

    pub fn create_paragraph(
        &mut self,
        default_text_style: &skia_safe::textlayout::TextStyle,
        max_width: f32,
        text_align: TextAlign,
    ) -> Paragraph {
        let mut paragraph_style = ParagraphStyle::default();
        paragraph_style.set_text_align(text_align);
        // let mut text_style = default_text_style.clone();
        // text_style.set_font_families(&["CodeNewRoman Nerd Font"]);
        // paragraph_style.set_text_style(&text_style);

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
            self.changed = false;
        }
        TextLayout::new(paragraph, self.len())
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
                styles.push((style.clone(), new_start..new_end, edge_behavior.clone()));
            } else if style_range.end >= range.start && style_range.end <= range.end {
                let new_start = if style_range.start < range.start {
                    // The start of the style range is outside the substring
                    0
                } else {
                    style_range.start - range.start
                };
                let new_end = style_range.end - range.start;
                styles.push((style.clone(), new_start..new_end, *edge_behavior));
            }
        }

        StyledText {
            string,
            styles,
            changed: true,
        }
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
    
    pub fn append_str_with_style(
        &mut self,
        string: &str,
        style: TextStyle,
        expanded: bool,
    ) {
        let range = self.string.len()..(self.string.len() + string.len());
        self.insert_str(self.string.len(), string);
        self.set_style(style, range, expanded);
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
                styles.push((style.clone(), style_range.clone(), *expanded));
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
                            segmented_styles.push((
                                style.clone(),
                                range.end..style_range.end,
                                *expanded,
                            ));
                        }
                        false
                    } else {
                        true
                    };
                } else if range.start == style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push((
                            style.clone(),
                            range.end..style_range.end,
                            *expanded,
                        ));
                        return false;
                    } else if range.end >= style_range.end {
                        return false;
                    }
                } else if range.start > style_range.start {
                    if range.end < style_range.end {
                        segmented_styles.push((
                            style.clone(),
                            style_range.start..range.start,
                            expanded.clone(),
                        ));
                        segmented_styles.push((
                            style.clone(),
                            range.end..style_range.end,
                            expanded.clone(),
                        ));
                        return false;
                    } else if range.end >= style_range.end {
                        segmented_styles.push((
                            style.clone(),
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
        let mut output = self;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::text_style::TextStyle;
    use crate::text::Typeface;
    use skia_safe::textlayout::TextAlign;
    use crate::shared::SharedText;

    #[test]
    fn test_styled_text() {
        let text = SharedText::from("A simple i text");
        text.lock().set_style(TextStyle::Bold, 2..8, true);
        // text.lock().set_style(TextStyle::Italic, 0..2, true);
        // text.lock().set_style(TextStyle::Underline, 0..2, true);
        // text.lock().set_style(TextStyle::TextColor(Color::from_rgb(255, 0, 0)), 0..2, true);
        // // text.lock().set_style(TextStyle::Typeface(Typeface::Family("CodeNewRoman Nerd Font".to_string())), 2..8, true);
        let index_of_i = text.lock().as_str().find('i').unwrap();
        text.lock().set_style(
            TextStyle::Image(SharedDrawable::from_file("/home/grounzer/Downloads/check_box_selected.svg").unwrap()),
            index_of_i..index_of_i + 1,
            true,
        );
        let paragraph = text.lock().create_paragraph(
            &skia_safe::textlayout::TextStyle::default(),
            800.0,
            TextAlign::Left,
        );
    }
}