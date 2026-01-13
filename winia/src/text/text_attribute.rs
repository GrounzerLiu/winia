use std::fmt::Debug;
use crate::shared::SharedDrawable;
use crate::ui::{Color, SetColor};
use proc_macro::AsRef;
use skia_safe::font::Edging;
use skia_safe::font_style::{Slant, Weight, Width};
use skia_safe::textlayout::{PlaceholderAlignment, TextBaseline, TextDecorationMode, TextDecorationStyle, TextShadow as SkiaTextShadow};
use skia_safe::{FontHinting, Paint, Point};
use strum_macros::{AsRefStr, Display};

#[derive(Copy, Clone, Debug, PartialEq, AsRefStr, Display, AsRef)]
pub enum AttributeType {
    Background,
    BaselineShift,
    Color,
    Underline,
    Overline,
    Strikethrough,
    DecorationColor,
    DecorationStyle,
    DecorationMode,
    Font,
    FontEdging,
    FontFeature,
    FontHinting,
    FontSize,
    FontWeight,
    FontWidth,
    FontSlant,
    Foreground,
    HaftLeading,
    HeightMultiple,
    Placeholder,
    PlaceholderStyle,
    LetterSpacing,
    Locale,
    Shadow,
    Subpixel,
    TextBaseline,
    WordSpacing,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FontSource {
    Family(String),
    File(String),
}

#[derive(Copy, Clone, Debug, PartialEq, Display)]
pub enum BaselineShift {
    Subscript,
    Superscript,
    Custom(f32),
}

#[derive(Copy, Clone, Debug)]
pub struct TextShadow {
    pub color: Color,
    pub offset: Point,
    pub blur_sigma: f64,
}

impl Default for TextShadow {
    fn default() -> Self {
        let skia_shadow = SkiaTextShadow::default();
        Self {
            color: Color::from(skia_shadow.color),
            offset: skia_shadow.offset,
            blur_sigma: skia_shadow.blur_sigma
        }
    }
}

impl PartialEq for TextShadow {
    fn eq(&self, other: &Self) -> bool {
        self.color == other.color &&
        self.offset == other.offset &&
        self.blur_sigma == other.blur_sigma
    }
}

impl TextShadow {
    pub fn new(color: impl Into<Color>, offset: impl Into<Point>, blur_sigma: f64) -> Self {
        Self {
            color: color.into(),
            offset: offset.into(),
            blur_sigma,
        }
    }
}

impl From<&TextShadow> for SkiaTextShadow {
    fn from(value: &TextShadow) -> Self {
        SkiaTextShadow {
            color: value.color.to_skia_color(),
            offset: value.offset,
            blur_sigma: value.blur_sigma,
        }
    }
}

impl From<TextShadow> for SkiaTextShadow {
    fn from(value: TextShadow) -> Self {
        SkiaTextShadow::from(&value)
    }
}

/// The style of the text.
#[derive(Clone, AsRef)]
pub enum TextAttribute {
    Background(Paint),
    BaselineShift(BaselineShift),
    Color(Color),
    Underline,
    Overline,
    Strikethrough,
    DecorationColor(Color),
    DecorationStyle(TextDecorationStyle),
    DecorationMode(TextDecorationMode),
    Font(FontSource),
    FontEdging(Edging),
    FontFeature(String, i32),
    FontHinting(FontHinting),
    FontSize(f32),
    FontWeight(Weight),
    FontWidth(Width),
    FontSlant(Slant),
    Foreground(Paint),
    HalfLeading(bool),
    HeightMultiple(f32, bool),
    Placeholder(SharedDrawable),
    PlaceholderStyle(PlaceholderAlignment, TextBaseline, f32),
    LetterSpacing(f32),
    Locale(String),
    Shadow(TextShadow),
    Subpixel(bool),
    TextBaseline(TextBaseline),
    WordSpacing(f32),
}

impl TextAttribute {
    pub fn name(&self) -> String {
        self.attr_type().to_string()
    }

    pub fn attr_type(&self) -> AttributeType {
        match self {
            TextAttribute::Background(_) => AttributeType::Background,
            TextAttribute::BaselineShift(_) => AttributeType::BaselineShift,
            TextAttribute::Color(_) => AttributeType::Color,
            TextAttribute::Underline => AttributeType::Underline,
            TextAttribute::Overline => AttributeType::Overline,
            TextAttribute::Strikethrough => AttributeType::Strikethrough,
            TextAttribute::DecorationColor(_) => AttributeType::DecorationColor,
            TextAttribute::DecorationStyle(_) => AttributeType::DecorationStyle,
            TextAttribute::DecorationMode(_) => AttributeType::DecorationMode,
            TextAttribute::Font(_) => AttributeType::Font,
            TextAttribute::FontEdging(_) => AttributeType::FontEdging,
            TextAttribute::FontFeature(_, _) => AttributeType::FontFeature,
            TextAttribute::FontHinting(_) => AttributeType::FontHinting,
            TextAttribute::FontSize(_) => AttributeType::FontSize,
            TextAttribute::FontWeight(_) => AttributeType::FontWeight,
            TextAttribute::FontWidth(_) => AttributeType::FontWidth,
            TextAttribute::FontSlant(_) => AttributeType::FontSlant,
            TextAttribute::Foreground(_) => AttributeType::Foreground,
            TextAttribute::HalfLeading(_) => AttributeType::HaftLeading,
            TextAttribute::HeightMultiple(_, _) => AttributeType::HeightMultiple,
            TextAttribute::Placeholder(_) => AttributeType::Placeholder,
            TextAttribute::PlaceholderStyle(_, _, _) => AttributeType::PlaceholderStyle,
            TextAttribute::LetterSpacing(_) => AttributeType::LetterSpacing,
            TextAttribute::Locale(_) => AttributeType::Locale,
            TextAttribute::Shadow(_) => AttributeType::Shadow,
            TextAttribute::Subpixel(_) => AttributeType::Subpixel,
            TextAttribute::TextBaseline(_) => AttributeType::TextBaseline,
            TextAttribute::WordSpacing(_) => AttributeType::WordSpacing,
        }
    }
}

impl Debug for TextAttribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextAttribute::{:?}", self.attr_type())
    }
}

pub trait AddTextAttribute {
    fn background(self, paint: Paint) -> Self;
    fn background_color(self, color: Color) -> Self;
    fn baseline_shift(self, baseline_shift: BaselineShift) -> Self;
    fn subscript(self) -> Self;
    fn superscript(self) -> Self;
    fn color(self, color: Color) -> Self;
    fn underline(self) -> Self;
    fn overline(self) -> Self;
    fn strikethrough(self) -> Self;
    fn decoration_color(self, color: Color) -> Self;
    fn decoration_style(self, style: TextDecorationStyle) -> Self;
    fn decoration_mode(self, mode: TextDecorationMode) -> Self;
    fn font(self, font: FontSource) -> Self;
    fn font_family(self, family: impl Into<String>) -> Self;
    fn font_file(self, file: impl Into<String>) -> Self;
    fn font_edging(self, edging: Edging) -> Self;
    fn font_feature(self, feature: impl Into<String>, value: i32) -> Self;
    fn font_hinting(self, hinting: FontHinting) -> Self;
    fn font_size(self, size: f32) -> Self;
    fn font_weight(self, weight: Weight) -> Self;
    fn font_width(self, width: Width) -> Self;
    fn font_slant(self, slant: Slant) -> Self;
    fn bold(self) -> Self;
    fn italic(self) -> Self;
    fn foreground(self, paint: Paint) -> Self;
    fn foreground_color(self, color: Color) -> Self;
    fn half_leading(self, half: bool) -> Self;
    fn height_multiple(self, multiple: f32, include_line_spacing: bool) -> Self;
    fn placeholder(self, drawable: SharedDrawable) -> Self;
    fn placeholder_style(
        self,
        alignment: PlaceholderAlignment,
        baseline: TextBaseline,
        offset: f32,
    ) -> Self;
    fn letter_spacing(self, spacing: f32) -> Self;
    fn locale(self, locale: impl Into<String>) -> Self;
    fn shadow(self, shadow: TextShadow) -> Self;
    fn subpixel(self, subpixel: bool) -> Self;
    fn text_baseline(self, baseline: TextBaseline) -> Self;
    fn word_spacing(self, spacing: f32) -> Self;
}

impl AddTextAttribute for Vec<TextAttribute> {
    fn background(mut self, paint: Paint) -> Self {
        self.push(TextAttribute::Background(paint));
        self
    }

    fn background_color(mut self, color: Color) -> Self {
        let mut paint = Paint::default();
        paint.set_any_color(color);
        paint.set_style(skia_safe::paint::Style::Fill);
        self.push(TextAttribute::Background(paint));
        self
    }
    fn baseline_shift(mut self, baseline_shift: BaselineShift) -> Self {
        self.push(TextAttribute::BaselineShift(baseline_shift));
        self
    }
    fn subscript(mut self) -> Self {
        self.push(TextAttribute::BaselineShift(BaselineShift::Subscript));
        self
    }
    fn superscript(mut self) -> Self {
        self.push(TextAttribute::BaselineShift(BaselineShift::Superscript));
        self
    }
    fn color(mut self, color: Color) -> Self {
        self.push(TextAttribute::Color(color));
        self
    }

    fn underline(mut self) -> Self {
        self.push(TextAttribute::Underline);
        self
    }
    fn overline(mut self) -> Self {
        self.push(TextAttribute::Overline);
        self
    }
    fn strikethrough(mut self) -> Self {
        self.push(TextAttribute::Strikethrough);
        self
    }
    fn decoration_color(mut self, color: Color) -> Self {
        self.push(TextAttribute::DecorationColor(color));
        self
    }
    fn decoration_style(mut self, style: TextDecorationStyle) -> Self {
        self.push(TextAttribute::DecorationStyle(style));
        self
    }
    fn decoration_mode(mut self, mode: TextDecorationMode) -> Self {
        self.push(TextAttribute::DecorationMode(mode));
        self
    }
    fn font(mut self, font: FontSource) -> Self {
        self.push(TextAttribute::Font(font));
        self
    }
    fn font_family(mut self, family: impl Into<String>) -> Self {
        self.push(TextAttribute::Font(FontSource::Family(family.into())));
        self
    }
    fn font_file(mut self, file: impl Into<String>) -> Self {
        self.push(TextAttribute::Font(FontSource::File(file.into())));
        self
    }
    fn font_edging(mut self, edging: Edging) -> Self {
        self.push(TextAttribute::FontEdging(edging));
        self
    }
    fn font_feature(mut self, feature: impl Into<String>, value: i32) -> Self {
        self.push(TextAttribute::FontFeature(feature.into(), value));
        self
    }
    fn font_hinting(mut self, hinting: FontHinting) -> Self {
        self.push(TextAttribute::FontHinting(hinting));
        self
    }
    fn font_size(mut self, size: f32) -> Self {
        self.push(TextAttribute::FontSize(size));
        self
    }
    fn font_weight(mut self, weight: Weight) -> Self {
        self.push(TextAttribute::FontWeight(weight));
        self
    }
    fn font_width(mut self, width: Width) -> Self {
        self.push(TextAttribute::FontWidth(width));
        self
    }
    fn font_slant(mut self, slant: Slant) -> Self {
        self.push(TextAttribute::FontSlant(slant));
        self
    }
    fn bold(mut self) -> Self {
        self.push(TextAttribute::FontWeight(Weight::BOLD));
        self
    }
    fn italic(mut self) -> Self {
        self.push(TextAttribute::FontSlant(Slant::Italic));
        self
    }
    fn foreground(mut self, paint: Paint) -> Self {
        self.push(TextAttribute::Foreground(paint));
        self
    }
    fn foreground_color(mut self, color: Color) -> Self {
        let mut paint = Paint::default();
        paint.set_any_color(color);
        paint.set_style(skia_safe::paint::Style::Fill);
        self.push(TextAttribute::Foreground(paint));
        self
    }
    fn half_leading(mut self, half: bool) -> Self {
        self.push(TextAttribute::HalfLeading(half));
        self
    }
    fn height_multiple(mut self, multiple: f32, include_line_spacing: bool) -> Self {
        self.push(TextAttribute::HeightMultiple(multiple, include_line_spacing));
        self
    }
    fn placeholder(mut self, drawable: SharedDrawable) -> Self {
        self.push(TextAttribute::Placeholder(drawable));
        self
    }

    fn placeholder_style(mut self, alignment: PlaceholderAlignment, baseline: TextBaseline, offset: f32) -> Self {
        self.push(TextAttribute::PlaceholderStyle(alignment, baseline, offset));
        self
    }

    fn letter_spacing(mut self, spacing: f32) -> Self {
        self.push(TextAttribute::LetterSpacing(spacing));
        self
    }
    fn locale(mut self, locale: impl Into<String>) -> Self {
        self.push(TextAttribute::Locale(locale.into()));
        self
    }
    fn shadow(mut self, shadow: TextShadow) -> Self {
        self.push(TextAttribute::Shadow(shadow));
        self
    }
    fn subpixel(mut self, subpixel: bool) -> Self {
        self.push(TextAttribute::Subpixel(subpixel));
        self
    }
    fn text_baseline(mut self, baseline: TextBaseline) -> Self {
        self.push(TextAttribute::TextBaseline(baseline));
        self
    }
    fn word_spacing(mut self, spacing: f32) -> Self {
        self.push(TextAttribute::WordSpacing(spacing));
        self
    }
}