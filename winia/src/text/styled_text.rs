use crate::shared::SharedDrawable;
use crate::text::text_attribute::TextAttribute;
use crate::text::{AddTextAttribute, AttributeType, BaselineShift, FontSource, Paragraph, ParagraphBuilder, TextLayout, TextShadow};
use crate::ui::{Color, SetColor};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use skia_safe::font::Edging;
use skia_safe::font_style::{Slant, Weight, Width};
use skia_safe::textlayout::{FontCollection, ParagraphStyle, PlaceholderAlignment, TextBaseline, TextDecoration, TextDecorationMode, TextDecorationStyle, TextStyle, TypefaceFontProvider};
use skia_safe::{FontHinting, FontMgr, FontStyle, Paint};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::ops::{Add, Deref, DerefMut, Index, Range};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

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
    text_style: &TextStyle,
) -> Vec<StyleSegment<'text>> {
    let mut text_segments = Vec::new();

    let first_segment = StyleSegment::new(text, range, &text_style);
    text_segments.push(first_segment);
    text.get_attrs(range.clone())
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
                        text_segment.apply_attr(style);
                        index += 1;
                    } else if range.start > text_segment.range.start
                        && range.start < text_segment.range.end
                        && range.end > text_segment.range.start
                        && range.end < text_segment.range.end
                        && text_segment.placeholder.is_none()
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
                        && text_segment.placeholder.is_none()
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
                        && text_segment.placeholder.is_none()
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
    text_style: TextStyle,
    placeholder: Option<SharedDrawable>,
    placeholder_style: Option<(PlaceholderAlignment, TextBaseline, f32)>,
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
        def_text_style: &TextStyle,
    ) -> StyleSegment<'text> {
        let text_style = def_text_style.clone();
        StyleSegment {
            text: text.as_str(),
            range: range.clone(),
            text_style,
            placeholder: None,
            placeholder_style: None,
        }
    }

    pub fn apply_attr(&mut self, attr: impl AsRef<TextAttribute>) {
        let attr = attr.as_ref();
        /*match style {
            TextAttribute::Bold => {
                let font_style = self.text_style.font_style();

                if font_style == FontStyle::italic() {
                    self.text_style.set_font_style(FontStyle::bold_italic());
                } else if font_style != FontStyle::bold() {
                    self.text_style.set_font_style(FontStyle::bold());
                }
            }
            TextAttribute::Italic => {
                let font_style = self.text_style.font_style();
                if font_style == FontStyle::bold() {
                    self.text_style.set_font_style(FontStyle::bold_italic());
                } else if font_style != FontStyle::italic() {
                    self.text_style.set_font_style(FontStyle::italic());
                }
            }
            TextAttribute::FontFeature(feature, value) => {
                self.text_style.add_font_feature(feature, *value);
            }
            TextAttribute::LetterSpacing(tracking) => {
                self.text_style.set_letter_spacing(*tracking);
            }
            TextAttribute::BaselineShift(baselineShift) => match baselineShift {
                BaselineShift::Subscript => { self.text_style.set_baseline_shift(-0.5); }
                BaselineShift::Superscript => { self.text_style.set_baseline_shift(0.5); }
                BaselineShift::Custom(shift) => { self.text_style.set_baseline_shift(*shift); }
            }
            TextAttribute::Underline => {
                self.text_style.font_s
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::UNDERLINE);
                self.text_style.set_decoration(&decoration);
            }
            TextAttribute::Strikethrough => {
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::LINE_THROUGH);
                self.text_style.set_decoration(&decoration);
            }
            TextAttribute::FontSize(font_size) => {
                self.text_style.set_font_size(*font_size);
            }
            TextAttribute::Background(color) => {
                self.text_style
                    .set_background_paint(Paint::default().set_color(*color));
            }
            TextAttribute::Color(color) => {
                self.text_style.set_color(*color);
            }
            TextAttribute::FontWeight(weight) => {
                let font_style = self.text_style.font_style();
                self.text_style.set_font_style(FontStyle::new(
                    *weight,
                    font_style.width(),
                    font_style.slant(),
                ));
            }
            TextAttribute::Font(typeface) => match typeface {
                FontSource::Family(family) => {
                    self.text_style.set_font_families(&[family]);
                }
                FontSource::File(path) => {
                    if let Some(family) = load_typeface_from_path(path) {
                        self.text_style.set_font_families(&[family]);
                    }
                }
            },
/*            TextAttribute::Subscript => {
                let font_size = self.text_style.font_size();
                self.text_style.set_font_size(font_size * 0.58);
                self.text_style.set_baseline_shift(font_size * 0.15);
            }
            TextAttribute::Superscript => {
                let font_size = self.text_style.font_size();
                self.text_style.set_font_size(font_size * 0.58);
                self.text_style.set_baseline_shift(font_size * -0.30);
            }*/
            TextAttribute::Image(image) => {
                // self.text_style.set_placeholder();
                self.image = Some(image.clone());
            }
        }*/
        match attr {
            TextAttribute::Background(paint) => {
                self.text_style.set_background_paint(&paint);
            }
            TextAttribute::BaselineShift(baseline_shift) => match baseline_shift {
                BaselineShift::Subscript => {
                    // self.text_style.set_baseline_shift(-0.5);
                    let font_size = self.text_style.font_size();
                    self.text_style.set_font_size(font_size * 0.58);
                    self.text_style.set_baseline_shift(font_size * 0.15);
                }
                BaselineShift::Superscript => {
                    // self.text_style.set_baseline_shift(0.5);
                    let font_size = self.text_style.font_size();
                    self.text_style.set_font_size(font_size * 0.58);
                    self.text_style.set_baseline_shift(font_size * -0.30);
                }
                BaselineShift::Custom(shift) => { self.text_style.set_baseline_shift(*shift); }
            }
            TextAttribute::Color(color) => {
                self.text_style.set_color(color.to_skia_color());
            }
            TextAttribute::Underline => {
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::UNDERLINE);
                self.text_style.set_decoration(&decoration);
            }
            TextAttribute::Overline => {
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::OVERLINE);
                self.text_style.set_decoration(&decoration);
            }
            TextAttribute::Strikethrough => {
                let mut decoration = *self.text_style.decoration();
                decoration.ty.insert(TextDecoration::LINE_THROUGH);
                self.text_style.set_decoration(&decoration);
            }
            TextAttribute::DecorationColor(color) => {
                self.text_style.set_decoration_color(color.to_skia_color());
            }
            TextAttribute::DecorationStyle(style) => {
                self.text_style.set_decoration_style(*style);
            }
            TextAttribute::DecorationMode(mode) => {
                self.text_style.set_decoration_mode(*mode);
            }
            TextAttribute::Font(typeface) => match typeface {
                FontSource::Family(family) => {
                    self.text_style.set_font_families(&[family]);
                }
                FontSource::File(path) => {
                    if let Some(family) = load_typeface_from_path(path) {
                        self.text_style.set_font_families(&[family]);
                    }
                }
            },
            TextAttribute::FontEdging(edging) => {
                self.text_style.set_font_edging(*edging);
            }
            TextAttribute::FontFeature(feature, value) => {
                self.text_style.add_font_feature(feature, *value);
            }
            TextAttribute::FontHinting(hinting) => {
                self.text_style.set_font_hinting(*hinting);
            }
            TextAttribute::FontSize(font_size) => {
                self.text_style.set_font_size(*font_size);
            }
            TextAttribute::FontWeight(weight) => {
                let font_style = self.text_style.font_style();
                self.text_style.set_font_style(FontStyle::new(
                    *weight,
                    font_style.width(),
                    font_style.slant(),
                ));
            }
            TextAttribute::FontWidth(width) => {
                let font_style = self.text_style.font_style();
                self.text_style.set_font_style(FontStyle::new(
                    font_style.weight(),
                    *width,
                    font_style.slant(),
                ));
            }
            TextAttribute::FontSlant(slant) => {
                let font_style = self.text_style.font_style();
                self.text_style.set_font_style(FontStyle::new(
                    font_style.weight(),
                    font_style.width(),
                    *slant,
                ));
            }
            TextAttribute::Foreground(paint) => {
                self.text_style.set_foreground_paint(&paint);
            }
            TextAttribute::HalfLeading(half_leading) => {
                self.text_style.set_half_leading(*half_leading);
            }
            TextAttribute::HeightMultiple(height_multiple, height_override) => {
                self.text_style.set_height(*height_multiple);
                self.text_style.set_height_override(*height_override);
            }
            TextAttribute::Placeholder(placeholder) => {
                self.placeholder = Some(placeholder.clone());
            }
            TextAttribute::PlaceholderStyle(alignment, baseline, offset) => {
                self.placeholder_style = Some((*alignment, *baseline, *offset));
            }
            TextAttribute::LetterSpacing(tracking) => {
                self.text_style.set_letter_spacing(*tracking);
            }
            TextAttribute::Locale(locale) => {
                self.text_style.set_locale(locale);
            }
            TextAttribute::Shadow(shadow) => {
                self.text_style.reset_shadows().add_shadow(shadow.into());
            }
            TextAttribute::Subpixel(subpixel) => {
                self.text_style.set_subpixel(*subpixel);
            }
            TextAttribute::TextBaseline(text_baseline) => {
                self.text_style.set_text_baseline(*text_baseline);
            }
            TextAttribute::WordSpacing(word_spacing) => {
                self.text_style.set_word_spacing(*word_spacing);
            }
        }
    }
}

pub(crate) trait AddStyleSegment {
    fn add_style_segment(&mut self, style_segment: &StyleSegment);
}

impl AddStyleSegment for ParagraphBuilder {
    fn add_style_segment(&mut self, style_segment: &StyleSegment) {
        if let Some(placeholder) = &style_segment.placeholder {
            self.push_style(&style_segment.text_style);
            self.add_placeholder(
                &style_segment.text[style_segment.range.clone()],
                placeholder.clone(),
                &style_segment.placeholder_style
            );
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
    /// attrs,
    /// The range of the attrs,
    /// Should the range expand to include the text inserted at the end of the range or not
    attrs: Vec<(TextAttribute, Range<usize>, bool)>,
    /// Generating the indices is expensive so we only do it when using the indices.
    changed: bool,
}

impl StyledText {
    fn new(string: String) -> Self {
        StyledText {
            string,
            attrs: Vec::new(),
            changed: true,
        }
    }
    
    pub fn prev_glyph_index(&self, index: usize) -> Option<usize> {
        if index == 0 {
            return None;
        }
        if let Some(str) = self.string[..index].graphemes(true).next_back() {
            index.checked_sub(str.len())
        }else {
            None
        }
    }
    pub fn next_glyph_index(&self, index: usize) -> Option<usize> {
        if index >= self.string.len() {
            return None;
        }
        if let Some(str) = self.string[index..].graphemes(true).next() {
            index.checked_add(str.len())
        } else {
            None
        }
    }

    pub fn create_paragraph(
        &mut self,
        paragraph_style: &ParagraphStyle,
        default_text_style: &TextStyle,
        max_width: f32,
    ) -> Paragraph {
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
        let mut attrs: Vec<(TextAttribute, Range<usize>, bool)> = Vec::new();
        for (attr, attr_range, edge_behavior) in self.attrs.iter() {
            // The start of the attr range is in the substring
            if attr_range.start >= range.start && attr_range.start <= range.end {
                let new_start = attr_range.start - range.start;
                let new_end = if attr_range.end > range.end {
                    // The end of the attr range is outside the substring
                    range.end - range.start
                } else {
                    attr_range.end - range.start
                };
                attrs.push((attr.clone(), new_start..new_end, edge_behavior.clone()));
            } else if attr_range.end >= range.start && attr_range.end <= range.end {
                let new_start = attr_range.start.saturating_sub(range.start);
                let new_end = attr_range.end - range.start;
                attrs.push((attr.clone(), new_start..new_end, *edge_behavior));
            }
        }

        StyledText {
            string,
            attrs,
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
        self.attrs.iter_mut().for_each(|(_, range, expanded)| {
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
        text.attrs.iter().for_each(|(attr, range, expanded)| {
            self.set_attr(
                attr.clone(),
                (range.start + index)..(range.end + index),
                *expanded,
            );
        });
        self.changed = true;
    }

    pub fn remove(&mut self, range: Range<usize>) {
        self.string.drain(range.clone());
        self.attrs.retain(|(_, attr_range, _)| {
            if attr_range.start >= range.start && attr_range.end <= range.end {
                return false;
            }
            true
        });
        self.attrs.iter_mut().for_each(|(_, attr_range, _)| {
            if attr_range.start >= range.end {
                attr_range.start -= range.end - range.start;
            } else if attr_range.start >= range.start {
                attr_range.start = range.start;
            }
            if attr_range.end >= range.end {
                attr_range.end -= range.end - range.start;
            } else if attr_range.end >= range.start {
                attr_range.end = range.start;
            }
        });
        self.changed = true;
    }

    pub fn append_str(&mut self, string: &str) -> StyleSetter {
        let start = self.string.len();
        self.insert_str(self.string.len(), string);
        let end = self.string.len();
        self.changed = true;
        StyleSetter {
            styled_text: self,
            range: start..end,
            attrs: Vec::new(),
            is_expanded: false,
        }
    }

    pub fn style_setter(&mut self, range: Range<usize>) -> StyleSetter {
        StyleSetter {
            styled_text: self,
            range,
            attrs: Vec::new(),
            is_expanded: false,
        }
    }
    
    pub fn append_str_with_attr(
        &mut self,
        string: &str,
        attr: TextAttribute,
        expanded: bool,
    ) {
        let range = self.string.len()..(self.string.len() + string.len());
        self.insert_str(self.string.len(), string);
        self.set_attr(attr, range, expanded);
        self.changed = true;
    }

    pub fn append_with_attrs(
        &mut self,
        string: &str,
        attrs: &[TextAttribute],
        expanded: bool,
    ) {
        let range = self.string.len()..(self.string.len() + string.len());
        self.insert_str(self.string.len(), string);
        self.set_attrs(range, expanded, attrs);
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
        self.attrs.clear();
        self.changed = true;
    }

    fn validate_range(&self, range: &Range<usize>) -> Result<(), crate::error::WiniaError> {
        if range.start > self.string.len() || range.end > self.string.len() {
            let err = crate::error::WiniaError::InvalidTextRange {
                start: range.start,
                end: range.end,
                length: self.string.len(),
            };
            log::error!("{err}");
            #[cfg(debug_assertions)]
            panic!("{err}");
            return Err(err);
        }
        if range.start > range.end {
            let err = crate::error::WiniaError::InvalidTextRange {
                start: range.start,
                end: range.end,
                length: self.string.len(),
            };
            log::error!("{err}");
            #[cfg(debug_assertions)]
            panic!("{err}");
            return Err(err);
        }
        Ok(())
    }

    fn assert_in_range(&self, range: &Range<usize>) {
        // Keep backward-compatible wrapper; delegates to validate_range
        // In debug mode, validate_range will panic on error
        let _ = self.validate_range(range);
    }

    pub fn set_attr(&mut self, attr: TextAttribute, range: Range<usize>, expanded: bool) {
        self.assert_in_range(&range);

        self.remove_attr(attr.attr_type(), range.clone());
        self.attrs.push((attr, range, expanded));
    }
    
    pub fn set_attrs(
        &mut self,
        range: Range<usize>,
        expanded: bool,
        attrs: &[TextAttribute]
    ) {
        self.assert_in_range(&range);

        attrs.iter().for_each(|attr| {
            self.remove_attr(attr.attr_type(), range.clone());
            self.attrs.push((attr.clone(), range.clone(), expanded));
        });
    }

    pub fn get_attrs(&self, range: Range<usize>) -> Vec<(TextAttribute, Range<usize>, bool)> {
        self.assert_in_range(&range);
        let mut attrs: Vec<(TextAttribute, Range<usize>, bool)> = Vec::new();
        for (attr, attr_range, expanded) in self.attrs.iter() {
            if attr_range.start >= range.start && attr_range.end <= range.end {
                attrs.push((attr.clone(), attr_range.clone(), *expanded));
            }
        }
        attrs
    }

    pub fn retain_attrs(&mut self, f: impl Fn(&TextAttribute, &Range<usize>, &bool) -> bool) {
        self.attrs
            .retain(|(attr, range, expanded)| f(attr, range, expanded));
    }

    pub fn remove_attr(&mut self, attr_type: AttributeType, range: Range<usize>) {
        self.assert_in_range(&range);

        let mut segmented_attrs: Vec<(TextAttribute, Range<usize>, bool)> = Vec::new();

        let remove_range = range;
        self.attrs.retain(|(attr, attr_range, expanded)| {
            if attr.attr_type() == attr_type {
                if remove_range.start <= attr_range.start {
                    if remove_range.end <= attr_range.start {
                        // attr 的范围在 remove_range 的后面，保留 attr
                        return true;
                    } else if remove_range.end > attr_range.start && remove_range.end < attr_range.end {
                        // 保留 range.end 到 attr_range.end 的部分
                        segmented_attrs.push((
                            attr.clone(),
                            remove_range.end..attr_range.end,
                            *expanded,
                        ));
                    } else if remove_range.end >= attr_range.end {
                        // attr 完全在 remove_range 里面，删除 attr
                        return false;
                    }
                } else if remove_range.start > attr_range.start && remove_range.start < attr_range.end {
                    if remove_range.end < attr_range.end {
                        // 分割 attr
                        segmented_attrs.push((
                            attr.clone(),
                            attr_range.start..remove_range.start,
                            *expanded,
                        ));
                        segmented_attrs.push((
                            attr.clone(),
                            remove_range.end..attr_range.end,
                            *expanded,
                        ));
                    } else if remove_range.end >= attr_range.end {
                        // 保留 attr_range.start 到 remove_range.start 的部分
                        segmented_attrs.push((
                            attr.clone(),
                            attr_range.start..remove_range.start,
                            *expanded,
                        ));
                    }
                } else if remove_range.start >= attr_range.end {
                    // attr 的范围在 remove_range 的前面，保留 attr
                    return true;
                }
            }
            true
        });

        self.attrs.append(&mut segmented_attrs);
    }

    pub fn clear_attrs(&mut self) {
        self.attrs.clear();
    }


}

impl Display for StyledText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
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
            attrs: self.attrs.clone(),
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

pub struct StyleSetter<'a> {
    styled_text: &'a mut StyledText,
    range: Range<usize>,
    attrs: Vec<TextAttribute>,
    is_expanded: bool
}

impl<'a> StyleSetter<'a> {
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.is_expanded = expanded;
        self
    }

    pub fn set(self) {
        self.styled_text.set_attrs(self.range, self.is_expanded, &self.attrs);
    }
}

impl<'a> AddTextAttribute for StyleSetter<'a> {
    fn background(mut self, paint: Paint) -> Self {
        self.attrs.push(TextAttribute::Background(paint));
        self
    }

    fn background_color(mut self, color: Color) -> Self {
        let mut paint = Paint::default();
        paint.set_any_color(color);
        paint.set_style(skia_safe::paint::Style::Fill);
        self.attrs.push(TextAttribute::Background(paint));
        self
    }
    fn baseline_shift(mut self, baseline_shift: BaselineShift) -> Self {
        self.attrs.push(TextAttribute::BaselineShift(baseline_shift));
        self
    }
    fn subscript(mut self) -> Self {
        self.attrs.push(TextAttribute::BaselineShift(BaselineShift::Subscript));
        self
    }
    fn superscript(mut self) -> Self {
        self.attrs.push(TextAttribute::BaselineShift(BaselineShift::Superscript));
        self
    }
    fn color(mut self, color: Color) -> Self {
        self.attrs.push(TextAttribute::Color(color));
        self
    }

    fn underline(mut self) -> Self {
        self.attrs.push(TextAttribute::Underline);
        self
    }
    fn overline(mut self) -> Self {
        self.attrs.push(TextAttribute::Overline);
        self
    }
    fn strikethrough(mut self) -> Self {
        self.attrs.push(TextAttribute::Strikethrough);
        self
    }
    fn decoration_color(mut self, color: Color) -> Self {
        self.attrs.push(TextAttribute::DecorationColor(color));
        self
    }
    fn decoration_style(mut self, style: TextDecorationStyle) -> Self {
        self.attrs.push(TextAttribute::DecorationStyle(style));
        self
    }
    fn decoration_mode(mut self, mode: TextDecorationMode) -> Self {
        self.attrs.push(TextAttribute::DecorationMode(mode));
        self
    }
    fn font(mut self, font: FontSource) -> Self {
        self.attrs.push(TextAttribute::Font(font));
        self
    }
    fn font_family(mut self, family: impl Into<String>) -> Self {
        self.attrs.push(TextAttribute::Font(FontSource::Family(family.into())));
        self
    }
    fn font_file(mut self, file: impl Into<String>) -> Self {
        self.attrs.push(TextAttribute::Font(FontSource::File(file.into())));
        self
    }
    fn font_edging(mut self, edging: Edging) -> Self {
        self.attrs.push(TextAttribute::FontEdging(edging));
        self
    }
    fn font_feature(mut self, feature: impl Into<String>, value: i32) -> Self {
        self.attrs.push(TextAttribute::FontFeature(feature.into(), value));
        self
    }
    fn font_hinting(mut self, hinting: FontHinting) -> Self {
        self.attrs.push(TextAttribute::FontHinting(hinting));
        self
    }
    fn font_size(mut self, size: f32) -> Self {
        self.attrs.push(TextAttribute::FontSize(size));
        self
    }
    fn font_weight(mut self, weight: Weight) -> Self {
        self.attrs.push(TextAttribute::FontWeight(weight));
        self
    }
    fn font_width(mut self, width: Width) -> Self {
        self.attrs.push(TextAttribute::FontWidth(width));
        self
    }
    fn font_slant(mut self, slant: Slant) -> Self {
        self.attrs.push(TextAttribute::FontSlant(slant));
        self
    }
    fn bold(mut self) -> Self {
        self.attrs.push(TextAttribute::FontWeight(Weight::BOLD));
        self
    }
    fn italic(mut self) -> Self {
        self.attrs.push(TextAttribute::FontSlant(Slant::Italic));
        self
    }
    fn foreground(mut self, paint: Paint) -> Self {
        self.attrs.push(TextAttribute::Foreground(paint));
        self
    }
    fn foreground_color(mut self, color: Color) -> Self {
        let mut paint = Paint::default();
        paint.set_any_color(color);
        paint.set_style(skia_safe::paint::Style::Fill);
        self.attrs.push(TextAttribute::Foreground(paint));
        self
    }
    fn half_leading(mut self, half: bool) -> Self {
        self.attrs.push(TextAttribute::HalfLeading(half));
        self
    }
    fn height_multiple(mut self, multiple: f32, include_line_spacing: bool) -> Self {
        self.attrs.push(TextAttribute::HeightMultiple(multiple, include_line_spacing));
        self
    }
    fn placeholder(mut self, drawable: SharedDrawable) -> Self {
        self.attrs.push(TextAttribute::Placeholder(drawable));
        self
    }

    fn placeholder_style(mut self, alignment: PlaceholderAlignment, baseline: TextBaseline, offset: f32) -> Self {
        self.attrs.push(TextAttribute::PlaceholderStyle(alignment, baseline, offset));
        self
    }

    fn letter_spacing(mut self, spacing: f32) -> Self {
        self.attrs.push(TextAttribute::LetterSpacing(spacing));
        self
    }
    fn locale(mut self, locale: impl Into<String>) -> Self {
        self.attrs.push(TextAttribute::Locale(locale.into()));
        self
    }
    fn shadow(mut self, shadow: TextShadow) -> Self {
        self.attrs.push(TextAttribute::Shadow(shadow));
        self
    }
    fn subpixel(mut self, subpixel: bool) -> Self {
        self.attrs.push(TextAttribute::Subpixel(subpixel));
        self
    }
    fn text_baseline(mut self, baseline: TextBaseline) -> Self {
        self.attrs.push(TextAttribute::TextBaseline(baseline));
        self
    }
    fn word_spacing(mut self, spacing: f32) -> Self {
        self.attrs.push(TextAttribute::WordSpacing(spacing));
        self
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_styled_text() {
/*        let text = SharedText::from("A simple i text");
        text.lock().set_style(TextAttribute::Bold, 2..8, true);
        // text.lock().set_style(TextStyle::Italic, 0..2, true);
        // text.lock().set_style(TextStyle::Underline, 0..2, true);
        // text.lock().set_style(TextStyle::TextColor(Color::from_rgb(255, 0, 0)), 0..2, true);
        // // text.lock().set_style(TextStyle::Typeface(Typeface::Family("CodeNewRoman Nerd Font".to_string())), 2..8, true);
        let index_of_i = text.lock().as_str().find('i').unwrap();
        text.lock().set_style(
            TextAttribute::Image(SharedDrawable::from_file("/home/grounzer/Downloads/check_box_selected.svg").unwrap()),
            index_of_i..index_of_i + 1,
            true,
        );
        let paragraph = text.lock().create_paragraph(
            &skia_safe::textlayout::TextStyle::default(),
            800.0,
            TextAlign::Left,
        );*/
    }
}