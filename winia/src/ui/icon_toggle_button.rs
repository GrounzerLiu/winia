//! IconToggleButton 组件 — 对标 material3 `IconToggleButton` /
//! `FilledIconToggleButton` / `FilledTonalIconToggleButton` /
//! `OutlinedIconToggleButton`

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::button::ButtonBorder;
use crate::ui::icon_button::{IconButtonDefaults, IconButtonSize, IconButtonStyle};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 图标切换按钮颜色集——enabled/disabled × checked/unchecked
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconToggleButtonColors {
    pub container: Color,
    pub content: Color,
    pub disabled_container: Color,
    pub disabled_content: Color,
    pub checked_container: Color,
    pub checked_content: Color,
}

impl IconToggleButtonColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        container: Color,
        content: Color,
        disabled_container: Color,
        disabled_content: Color,
        checked_container: Color,
        checked_content: Color,
    ) -> Self {
        Self {
            container,
            content,
            disabled_container,
            disabled_content,
            checked_container,
            checked_content,
        }
    }

    pub fn container_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            self.disabled_container
        } else if checked {
            self.checked_container
        } else {
            self.container
        }
    }

    pub fn content_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            self.disabled_content
        } else if checked {
            self.checked_content
        } else {
            self.content
        }
    }

    /// 从主题按变体生成默认色（对标 IconButtonDefaults.*IconToggleButtonColors；
    /// token 版本 14_1_0）
    pub fn from_theme(theme: &ThemeColors, style: IconButtonStyle) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        // DisabledContainerColor = OnSurface @ 0.10、DisabledColor = OnSurface @ 0.38
        let disabled_container = alpha(theme.on_surface, 0.10);
        let disabled_content = alpha(theme.on_surface, 0.38);
        match style {
            IconButtonStyle::Standard => Self::new(
                transparent,
                theme.on_surface,
                transparent,
                disabled_content,
                transparent,
                theme.primary,
            ),
            // UnselectedContainerColor = SurfaceContainer / UnselectedColor = OnSurfaceVariant
            IconButtonStyle::Filled => Self::new(
                theme.surface_container,
                theme.on_surface_variant,
                disabled_container,
                disabled_content,
                theme.primary,
                theme.on_primary,
            ),
            // Unselected = SecondaryContainer/OnSecondaryContainer；
            // Selected = Secondary/OnSecondary（14_1_0 token）
            IconButtonStyle::FilledTonal => Self::new(
                theme.secondary_container,
                theme.on_secondary_container,
                disabled_container,
                disabled_content,
                theme.secondary,
                theme.on_secondary,
            ),
            // Selected = InverseSurface/InverseOnSurface（14_1_0 token）
            IconButtonStyle::Outlined => Self::new(
                transparent,
                theme.on_surface_variant,
                transparent,
                disabled_content,
                theme.inverse_surface,
                theme.inverse_on_surface,
            ),
        }
    }
}

/// IconToggleButton 默认值（对标 material3 `IconButtonDefaults.*IconToggleButton*`）
pub struct IconToggleButtonDefaults;

impl IconToggleButtonDefaults {
    pub fn icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::Standard)
    }

    pub fn filled_icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::Filled)
    }

    pub fn filled_tonal_icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::FilledTonal)
    }

    pub fn outlined_icon_toggle_button_colors(theme: &ThemeColors) -> IconToggleButtonColors {
        IconToggleButtonColors::from_theme(theme, IconButtonStyle::Outlined)
    }

    pub fn colors_for(theme: &ThemeColors, style: IconButtonStyle) -> IconToggleButtonColors {
        match style {
            IconButtonStyle::Standard => Self::icon_toggle_button_colors(theme),
            IconButtonStyle::Filled => Self::filled_icon_toggle_button_colors(theme),
            IconButtonStyle::FilledTonal => Self::filled_tonal_icon_toggle_button_colors(theme),
            IconButtonStyle::Outlined => Self::outlined_icon_toggle_button_colors(theme),
        }
    }

    /// Outlined 边框：仅未选中时绘制（对标
    /// `outlinedIconToggleButtonBorder`——checked 返回 null）；
    /// 1dp + OutlineVariant，disabled 38% alpha
    pub fn outlined_border(
        theme: &ThemeColors,
        enabled: bool,
        checked: bool,
        width: f32,
    ) -> Option<ButtonBorder> {
        if checked {
            return None;
        }
        // UnselectedOutlineColor = OutlineVariant（非 outline）
        let c = theme.outline_variant;
        let color = if enabled {
            c
        } else {
            Color::from_argb((c.a as f32 * 0.38) as u8, c.r, c.g, c.b)
        };
        Some(ButtonBorder::new(width, color))
    }
}

/// IconToggleButton 组件 Builder
pub struct IconToggleButton {
    style: IconButtonStyle,
    checked: bool,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    colors: Option<IconToggleButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    shape: Shape,
    border: Option<ButtonBorder>,
    size_variant: IconButtonSize,
    modifier: Modifier,
}

impl IconToggleButton {
    pub fn new(checked: bool) -> Self {
        Self {
            style: IconButtonStyle::Standard,
            checked,
            on_checked_change: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            shape: IconButtonDefaults::shape(),
            border: None,
            size_variant: IconButtonSize::Small,
            modifier: Modifier::new(),
        }
    }

    pub fn filled(checked: bool) -> Self {
        Self::new(checked).style(IconButtonStyle::Filled)
    }

    pub fn filled_tonal(checked: bool) -> Self {
        Self::new(checked).style(IconButtonStyle::FilledTonal)
    }

    pub fn outlined(checked: bool) -> Self {
        Self::new(checked).style(IconButtonStyle::Outlined)
    }

    pub fn style(mut self, style: IconButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_checked_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_checked_change = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: IconToggleButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    pub fn border(mut self, border: ButtonBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 尺寸变体（XSmall/Small/Medium/Large/XLarge，默认 Small）
    pub fn size(mut self, size: IconButtonSize) -> Self {
        self.size_variant = size;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        ctx.changed(&self.checked);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| IconToggleButtonDefaults::colors_for(&theme, self.style));
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let container = colors.container_color(self.enabled, self.checked);
        let content_color = colors.content_color(self.enabled, self.checked);
        let shape = self.shape;

        let size = self.size_variant.container_size();
        let mut modifier = Modifier::new().size(size, size).background(container, shape);
        let border = self.border.or_else(|| {
            if self.style == IconButtonStyle::Outlined {
                IconToggleButtonDefaults::outlined_border(
                    &theme,
                    self.enabled,
                    self.checked,
                    self.size_variant.outline_width(),
                )
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape);
        }
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_checked_change) = &self.on_checked_change {
                let cb = on_checked_change.clone();
                let checked = self.checked;
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb(!checked))
                    .ripple_with_shape(&interaction, content_color, true, shape);
            }
        }

        match ctx.start_restartable_group(
            key,
            modifier,
            BoxLayout::new().alignment(crate::layout::Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 内容色下传（含 checked 态内容色）——Icon tint Auto 自动跟随
                WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    crate::ui::text::ProvideTextStyle(
                        crate::ui::text::TextStyle::new().color(content_color),
                        ctx,
                        content,
                    );
                });
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }

    pub fn get_checked(&self) -> bool {
        self.checked
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_style(&self) -> IconButtonStyle {
        self.style
    }
    pub fn get_colors(&self) -> Option<IconToggleButtonColors> {
        self.colors
    }
    pub fn get_shape(&self) -> Shape {
        self.shape
    }
    pub fn get_border(&self) -> Option<ButtonBorder> {
        self.border
    }
    pub fn get_size(&self) -> IconButtonSize {
        self.size_variant
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(colors.container_color(true, false), colors.container);
        assert_eq!(colors.content_color(true, false), colors.content);
        assert_eq!(colors.container_color(true, true), theme.primary, "checked 容器 = primary");
        assert_eq!(colors.content_color(true, true), theme.on_primary);
        // disabled 优先于 checked
        assert_eq!(colors.container_color(false, true), colors.disabled_container);
        assert_eq!(colors.content_color(false, true), colors.disabled_content);
        // 标准版 checked 内容 = primary（M3 SelectedColor）
        let s = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Standard);
        assert_eq!(s.content_color(true, true), theme.primary);
        // disabled 色值本身：OnSurface 10%/38%（防与 IconButton 漂移）
        assert_eq!(s.disabled_content.a, (theme.on_surface.a as f32 * 0.38) as u8);
        let f2 = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(f2.disabled_container.a, (theme.on_surface.a as f32 * 0.10) as u8);
        // Filled 未选 = surfaceContainer/onSurfaceVariant
        let f = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(f.container, theme.surface_container);
        assert_eq!(f.content, theme.on_surface_variant);
        // Tonal 选中 = secondary/onSecondary（14_1_0 token）
        let t = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::FilledTonal);
        assert_eq!(t.checked_container, theme.secondary);
        assert_eq!(t.checked_content, theme.on_secondary);
        // Outlined 选中 = inverseSurface/inverseOnSurface
        let o = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Outlined);
        assert_eq!(o.checked_container, theme.inverse_surface);
        assert_eq!(o.checked_content, theme.inverse_on_surface);
    }

    #[test]
    fn outlined_border_hidden_when_checked() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        assert!(IconToggleButtonDefaults::outlined_border(&theme, true, false, 1.0).is_some());
        assert_eq!(
            IconToggleButtonDefaults::outlined_border(&theme, true, false, 1.0).unwrap().color,
            theme.outline_variant
        );
        assert!(IconToggleButtonDefaults::outlined_border(&theme, true, true, 1.0).is_none(), "checked 无边框");
        let disabled = IconToggleButtonDefaults::outlined_border(&theme, false, false, 1.0).unwrap();
        assert_eq!(disabled.color.a, (theme.outline_variant.a as f32 * 0.38) as u8);
        assert_eq!(
            IconToggleButtonDefaults::outlined_border(&theme, true, false, IconButtonSize::Large.outline_width())
                .unwrap()
                .width,
            2.0
        );
    }

    #[test]
    fn constructors() {
        assert!(IconToggleButton::new(true).get_checked());
        assert!(!IconToggleButton::new(false).get_checked());
        assert_eq!(IconToggleButton::filled(true).get_style(), IconButtonStyle::Filled);
        assert_eq!(IconToggleButton::filled_tonal(true).get_style(), IconButtonStyle::FilledTonal);
        assert_eq!(IconToggleButton::outlined(true).get_style(), IconButtonStyle::Outlined);
        assert_eq!(IconToggleButton::new(true).checked(false).get_checked(), false);
        assert_eq!(IconToggleButton::new(true).get_size(), IconButtonSize::Small);
        assert_eq!(
            IconToggleButton::new(true).size(IconButtonSize::XLarge).get_size(),
            IconButtonSize::XLarge
        );
        // 尺寸表与 IconButton 共用同一 token 表
        assert_eq!(IconToggleButton::new(true).size(IconButtonSize::XSmall).get_size().container_size(), 32.0);
        assert_eq!(IconToggleButton::new(true).size(IconButtonSize::Large).get_size().icon_size(), 32.0);
    }

    #[test]
    fn checked_content_color_propagates_to_icon() {
        // 集成：Filled checked 内 Icon（tint Auto）解析为 on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::filled(true)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut tint = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    tint = spec.tint;
                }
            }
        }
        assert_eq!(tint, Some(theme.on_primary), "checked 内容色下传 Icon tint");
    }

    #[test]
    fn disabled_toggle_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            IconToggleButton::new(false)
                .enabled(false)
                .on_checked_change(|_| {})
                .build(ctx, |ctx| {
                    crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Ripple { .. })
                        && !matches!(el, ModifierElement::Focusable { .. }),
                    "禁用 Toggle 不应有波纹/可聚焦元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn outlined_checked_has_no_border_element() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            IconToggleButton::outlined(true)
                .on_checked_change(|_| {})
                .build(ctx, |ctx| {
                    crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Border { .. }),
                    "Outlined checked 节点不应有 Border 元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn toggle_callback_receives_inverted_checked() {
        use crate::modifier::ModifierElement;
        use std::sync::atomic::{AtomicBool, Ordering};
        // 未选中 → 点击回调应收到 true（取反）
        for (checked, expect) in [(false, true), (true, false)] {
            let received = Arc::new(AtomicBool::new(false));
            let cb = received.clone();
            let mut composer = crate::core::composer::Composer::new();
            composer.compose(|ctx| {
                IconToggleButton::new(checked)
                    .on_checked_change(move |v| cb.store(v, Ordering::Relaxed))
                    .build(ctx, |ctx| {
                        crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
            let mut clicked = false;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    if let ModifierElement::Clickable { on_click, .. } = el {
                        on_click();
                        clicked = true;
                    }
                }
            }
            assert!(clicked, "存在 Clickable");
            assert_eq!(received.load(Ordering::Relaxed), expect, "回调收到 !checked");
        }
    }
}
