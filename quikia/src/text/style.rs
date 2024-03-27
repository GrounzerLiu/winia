use skia_safe::Color;

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
            Style::TextColor(_) => {
                "TextColor"
            }
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