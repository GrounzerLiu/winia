use super::*;

// ═══════════════════════════════════════════════════════════
// Material Typography
// ═══════════════════════════════════════════════════════════

/// Material 3 type scale. Styles intentionally omit color and layout semantics;
/// components provide those through their own content-color and slot rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    pub display_large: TextStyle,
    pub display_medium: TextStyle,
    pub display_small: TextStyle,
    pub headline_large: TextStyle,
    pub headline_medium: TextStyle,
    pub headline_small: TextStyle,
    pub title_large: TextStyle,
    pub title_medium: TextStyle,
    pub title_small: TextStyle,
    pub body_large: TextStyle,
    pub body_medium: TextStyle,
    pub body_small: TextStyle,
    pub label_large: TextStyle,
    pub label_medium: TextStyle,
    pub label_small: TextStyle,
}

impl Typography {
    pub(crate) fn style(size: f32, line_height: f32, letter_spacing: f32, weight: FontWeight) -> TextStyle {
        TextStyle::new()
            .font_size(TextUnit::Sp(Sp(size)))
            .line_height(line_height)
            .letter_spacing(letter_spacing)
            .font_weight(weight)
    }

    /// Standard Material 3 `TypeScaleTokens` values.
    pub fn material3_default() -> Self {
        Self {
            display_large: Self::style(57.0, 64.0, -0.2, FontWeight::NORMAL),
            display_medium: Self::style(45.0, 52.0, 0.0, FontWeight::NORMAL),
            display_small: Self::style(36.0, 44.0, 0.0, FontWeight::NORMAL),
            headline_large: Self::style(32.0, 40.0, 0.0, FontWeight::NORMAL),
            headline_medium: Self::style(28.0, 36.0, 0.0, FontWeight::NORMAL),
            headline_small: Self::style(24.0, 32.0, 0.0, FontWeight::NORMAL),
            title_large: Self::style(22.0, 28.0, 0.0, FontWeight::NORMAL),
            title_medium: Self::style(16.0, 24.0, 0.2, FontWeight::MEDIUM),
            title_small: Self::style(14.0, 20.0, 0.1, FontWeight::MEDIUM),
            body_large: Self::style(16.0, 24.0, 0.5, FontWeight::NORMAL),
            body_medium: Self::style(14.0, 20.0, 0.2, FontWeight::NORMAL),
            body_small: Self::style(12.0, 16.0, 0.4, FontWeight::NORMAL),
            label_large: Self::style(14.0, 20.0, 0.1, FontWeight::MEDIUM),
            label_medium: Self::style(12.0, 16.0, 0.5, FontWeight::MEDIUM),
            label_small: Self::style(11.0, 16.0, 0.5, FontWeight::MEDIUM),
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::material3_default()
    }
}

