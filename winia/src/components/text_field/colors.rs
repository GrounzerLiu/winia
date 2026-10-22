use super::*;

// ═══════════════════════════════════════════════════════════
// TextField 容器视觉（对齐 material3 TextField/OutlinedTextField 外观）
// ═══════════════════════════════════════════════════════════


/// M3 默认禁用降级：基色 × alpha（disabled 色 = onSurface @ alpha）
pub(crate) fn alpha(c: crate::graphics::Color, a: f32) -> crate::graphics::Color {
    crate::graphics::Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b)
}

impl TextFieldColors {
    /// M3 FilledTextField 默认色（defaultTextFieldColors——1.4.0 tokens）：
    /// text onSurface、container surfaceContainerHighest、indicator/label 按状态、
    /// disabled 全系 onSurface@38%、cursor primary（error → error）
    pub fn filled_from_theme(theme: &crate::theme::ThemeColors) -> Self {
        Self {
            text: theme.on_surface,
            disabled_text: alpha(theme.on_surface, 0.38),
            container: theme.surface_container_highest,
            cursor: theme.primary,
            error_cursor: theme.error,
            indicator_focused: theme.primary,
            indicator_unfocused: theme.on_surface_variant,
            indicator_disabled: alpha(theme.on_surface, 0.38),
            indicator_error: theme.error,
            label_focused: theme.primary,
            label_unfocused: theme.on_surface_variant,
            label_disabled: alpha(theme.on_surface, 0.38),
            label_error: theme.error,
            placeholder: theme.on_surface_variant,
            disabled_placeholder: alpha(theme.on_surface, 0.38),
            supporting: theme.on_surface_variant,
            disabled_supporting: alpha(theme.on_surface, 0.38),
            error_supporting: theme.error,
            leading_icon_focused: theme.on_surface_variant,
            leading_icon_disabled: alpha(theme.on_surface, 0.38),
            trailing_icon_focused: theme.on_surface_variant,
            trailing_icon_disabled: alpha(theme.on_surface, 0.38),
            trailing_icon_error: theme.error,
            affix: theme.on_surface_variant,
            disabled_affix: alpha(theme.on_surface, 0.38),
        }
    }

    /// M3 OutlinedTextField 默认色（defaultOutlinedTextFieldColors）：
    /// 容器透明、边框 unfocused outline / disabled onSurface@12%、其余同 Filled
    pub fn outlined_from_theme(theme: &crate::theme::ThemeColors) -> Self {
        let mut c = Self::filled_from_theme(theme);
        c.container = crate::graphics::Color::TRANSPARENT;
        c.indicator_unfocused = theme.outline;
        c.indicator_disabled = alpha(theme.on_surface, 0.12);
        c
    }

    /// 状态解析（优先级 disabled > error > focused > unfocused——M3 同款）
    pub fn indicator_color(&self, enabled: bool, is_error: bool, focused: bool) -> crate::graphics::Color {
        if !enabled { self.indicator_disabled }
        else if is_error { self.indicator_error }
        else if focused { self.indicator_focused }
        else { self.indicator_unfocused }
    }

    pub fn label_color(&self, enabled: bool, is_error: bool, focused: bool) -> crate::graphics::Color {
        if !enabled { self.label_disabled }
        else if is_error { self.label_error }
        else if focused { self.label_focused }
        else { self.label_unfocused }
    }
}

