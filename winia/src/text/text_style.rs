use skia_safe::Color;
use std::fmt::Display;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum StyleType {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    FontSize,
    BackgroundColor,
    TextColor,
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
        }
    }
}

/// The style of the text.
#[derive(Copy, Clone, Debug)]
pub enum TextStyle {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    FontSize(f32),
    BackgroundColor(Color),
    TextColor(Color),
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
        }
    }
}
