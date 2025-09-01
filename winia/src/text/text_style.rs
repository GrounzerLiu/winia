use skia_safe::Color;
use skia_safe::font_style::Weight;
use strum_macros::{AsRefStr, Display};
use proc_macro::AsRef;
use crate::shared::SharedDrawable;

#[derive(Copy, Clone, Debug, PartialEq, AsRefStr, Display, AsRef)]
pub enum StyleType {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    FontSize,
    BackgroundColor,
    TextColor,
    Weight,
    Tracking,
    Typeface,
    Subscript,
    Superscript,
    Image,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Typeface {
    Family(String),
    FontFile(String),
}

/// The style of the text.
#[derive(Clone, AsRef)]
pub enum TextStyle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    FontSize(f32),
    BackgroundColor(Color),
    TextColor(Color),
    Weight(Weight),
    Tracking(f32),
    Typeface(Typeface),
    Subscript,
    Superscript,
    Image(SharedDrawable)
}

impl TextStyle {
    pub fn name(&self) -> String {
        self.style_type().to_string()
    }

    pub fn style_type(&self) -> StyleType {
        match self {
            TextStyle::Bold => StyleType::Bold,
            TextStyle::Italic => StyleType::Italic,
            TextStyle::Underline => StyleType::Underline,
            TextStyle::Strikethrough => StyleType::Strikethrough,
            TextStyle::FontSize(_) => StyleType::FontSize,
            TextStyle::BackgroundColor(_) => StyleType::BackgroundColor,
            TextStyle::TextColor(_) => StyleType::TextColor,
            TextStyle::Weight(_) => StyleType::Weight,
            TextStyle::Tracking(_) => StyleType::Tracking,
            TextStyle::Typeface(_) => StyleType::Typeface,
            TextStyle::Subscript => StyleType::Subscript,
            TextStyle::Superscript => StyleType::Superscript,
            TextStyle::Image(_) => StyleType::Typeface,
        }
    }
}
