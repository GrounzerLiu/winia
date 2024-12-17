use std::fmt::Display;
use skia_safe::Color;

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

#[derive(Copy, Clone, Debug)]
pub enum Style{
    Bold,
    Italic,
    Underline,
    Strikethrough,
    FontSize(f32),
    BackgroundColor(Color),
    TextColor(Color),
}

impl Style{
    pub fn name(&self) -> &'static str{
        match self {
            Style::Bold => "Bold",
            Style::Italic => "Italic",
            Style::Underline => "Underline",
            Style::Strikethrough => "Strikethrough",
            Style::FontSize(_) => "FontSize",
            Style::BackgroundColor(_) => "BackgroundColor",
            Style::TextColor(_) => "TextColor"
        }
    }

    pub fn style_type(&self) -> StyleType{
        match self {
            Style::Bold => StyleType::Bold,
            Style::Italic => StyleType::Italic,
            Style::Underline => StyleType::Underline,
            Style::Strikethrough => StyleType::Strikethrough,
            Style::FontSize(_) => StyleType::FontSize,
            Style::BackgroundColor(_) => StyleType::BackgroundColor,
            Style::TextColor(_) => StyleType::TextColor,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum EdgeBehavior{
    IncludeAndInclude,
    IncludeAndExclude,
    ExcludeAndInclude,
    ExcludeAndExclude,
}