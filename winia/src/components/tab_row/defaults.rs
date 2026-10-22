use super::*;

// ── TabRowDefaults ──

/// TabRow 默认值与颜色。
pub struct TabRowDefaults;

impl TabRowDefaults {
    /// 容器色（PrimaryNavigationTabTokens.ContainerColor = Surface）
    pub fn container_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.surface
    }

    /// Primary 内容色（PrimaryNavigationTabTokens.ActiveLabelTextColor = Primary）
    pub fn primary_content_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Secondary 内容色（SecondaryNavigationTabTokens.ActiveLabelTextColor = OnSurface）
    pub fn secondary_content_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.on_surface
    }

    /// 分隔线色（DividerTokens.Color = OutlineVariant）
    pub fn divider_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.outline_variant
    }

    /// Primary 指示条色（PrimaryNavigationTabTokens.ActiveIndicatorColor = Primary）
    pub fn primary_indicator_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Secondary 指示条色（Same as Primary 但无 shape）
    pub fn secondary_indicator_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.primary
    }

    /// Primary 指示条形状（ActiveIndicatorShape = RoundedCornerShape(3dp)）
    pub fn primary_indicator_shape() -> Shape {
        Shape::RoundedRect { corner_radius: 3.0 }
    }

    /// Secondary 指示条形状（直角全宽）
    pub fn secondary_indicator_shape() -> Shape {
        Shape::Rectangle
    }

    /// 选中 Tab 内容色（Primary 选中 = Primary，Secondary 选中 = OnSurface）
    /// ⚠ 当前 Tab::build 的选中色走 `self.selected_content_color.unwrap_or(content_color)`
    ///（TabRow 注入的 contentColor），不经由此函数——保留供未来 scrollable/自定义使用。
    pub fn selected_content_color(theme: &crate::theme::ThemeColors, is_primary: bool) -> Color {
        if is_primary { theme.primary } else { theme.on_surface }
    }

    /// 未选中 Tab 内容色（InactiveLabelTextColor = OnSurfaceVariant）
    pub fn unselected_content_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.on_surface_variant
    }

    /// Tab 文本样式（LabelTextFont = TitleSmall = 14.0/20.0）
    pub fn label_text_style() -> crate::text::TextStyle {
        let mut s = WiniaTheme::typography().title_small;
        s.text_align = Some(TextAlign::Center);
        s
    }
}

