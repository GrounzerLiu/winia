use skia_safe::Color;
use std::fmt::Display;
use skia_safe::font_style::Weight;
use proc_macro::AsRef;

#[derive(Copy, Clone, Debug, PartialEq, AsRef)]
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
}
impl Display for StyleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleType::Bold => write!(f, "Bold"),
            StyleType::Italic => write!(f, "Italic"),
            StyleType::Underline => write!(f, "Underline"),
            StyleType::Strikethrough => write!(f, "Strikethrough"),
            StyleType::FontSize => write!(f, "FontSize"),
            StyleType::BackgroundColor => write!(f, "BackgroundColor"),
            StyleType::TextColor => write!(f, "TextColor"),
            StyleType::Weight => write!(f, "Weight"),
            StyleType::Tracking => write!(f, "Tracking"),
        }
    }
}

/// The style of the text.
#[derive(Copy, Clone, Debug, AsRef)]
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
}

impl TextStyle {
    pub fn name(&self) -> &'static str {
        match self {
            TextStyle::Bold => "Bold",
            TextStyle::Italic => "Italic",
            TextStyle::Underline => "Underline",
            TextStyle::Strikethrough => "Strikethrough",
            TextStyle::FontSize(_) => "FontSize",
            TextStyle::BackgroundColor(_) => "BackgroundColor",
            TextStyle::TextColor(_) => "TextColor",
            TextStyle::Weight(_) => "Weight",
            TextStyle::Tracking(_) => "Tracking",
        }
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
        }
    }
}
