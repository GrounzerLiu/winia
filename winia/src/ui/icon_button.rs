//! IconButton 组件 — 对标 material3 `IconButton` / `FilledIconButton` /
//! `FilledTonalIconButton` / `OutlinedIconButton`

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::button::ButtonBorder;
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// IconButton 变体（对标 material3 各独立 composable）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconButtonStyle {
    Standard,
    Filled,
    FilledTonal,
    Outlined,
}

/// IconButton 尺寸变体（对标 material3 IconButtonTokens 系列）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconButtonSize {
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
}

impl IconButtonSize {
    /// 容器边长（Small = 48：本框架把 M3 的 40dp 视觉容器与 48dp 触摸目标
    /// 合并为一个容器；其余按 token：32/56/96/136）
    pub fn container_size(self) -> f32 {
        match self {
            IconButtonSize::XSmall => 32.0,
            IconButtonSize::Small => 48.0,
            IconButtonSize::Medium => 56.0,
            IconButtonSize::Large => 96.0,
            IconButtonSize::XLarge => 136.0,
        }
    }

    /// 建议图标尺寸（token IconSize：20/24/24/32/40）
    pub fn icon_size(self) -> f32 {
        match self {
            IconButtonSize::XSmall => 20.0,
            IconButtonSize::Small | IconButtonSize::Medium => 24.0,
            IconButtonSize::Large => 32.0,
            IconButtonSize::XLarge => 40.0,
        }
    }

    /// Outlined 边框宽度（token OutlinedOutlineWidth：1/1/1/2/3）
    pub fn outline_width(self) -> f32 {
        match self {
            IconButtonSize::XSmall | IconButtonSize::Small | IconButtonSize::Medium => 1.0,
            IconButtonSize::Large => 2.0,
            IconButtonSize::XLarge => 3.0,
        }
    }
}

/// 图标按钮颜色集——容器/内容各含 enabled/disabled 变体
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconButtonColors {
    pub container: Color,
    pub content: Color,
    pub disabled_container: Color,
    pub disabled_content: Color,
}

impl IconButtonColors {
    pub fn new(
        container: Color,
        content: Color,
        disabled_container: Color,
        disabled_content: Color,
    ) -> Self {
        Self { container, content, disabled_container, disabled_content }
    }

    pub fn container_color(&self, enabled: bool) -> Color {
        if enabled { self.container } else { self.disabled_container }
    }

    pub fn content_color(&self, enabled: bool) -> Color {
        if enabled { self.content } else { self.disabled_content }
    }

    /// 从主题按变体生成默认色（对标 IconButtonDefaults.*Colors）
    pub fn from_theme(theme: &ThemeColors, style: IconButtonStyle) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        // M3 token：disabled 容器 = OnSurface @ 0.10、disabled 内容 = OnSurface @ 0.38
        // （不是容器色/内容色的 alpha——Filled 禁用是灰而非半透明紫）
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        let disabled_container = alpha(theme.on_surface, 0.10);
        let disabled_content = alpha(theme.on_surface, 0.38);
        match style {
            IconButtonStyle::Standard => {
                Self::new(transparent, theme.on_surface, transparent, disabled_content)
            }
            IconButtonStyle::Outlined => {
                Self::new(
                    transparent,
                    theme.on_surface_variant,
                    transparent,
                    disabled_content,
                )
            }
            IconButtonStyle::Filled => {
                Self::new(theme.primary, theme.on_primary, disabled_container, disabled_content)
            }
            IconButtonStyle::FilledTonal => Self::new(
                theme.secondary_container,
                theme.on_secondary_container,
                disabled_container,
                disabled_content,
            ),
        }
    }
}

/// IconButton 默认值（对标 material3 `IconButtonDefaults`）
pub struct IconButtonDefaults;

impl IconButtonDefaults {
    /// 容器尺寸：M3 为 40dp 容器 + 48dp 最小触摸目标；本框架直接用 48
    pub fn container_size() -> f32 {
        IconButtonSize::Small.container_size()
    }

    /// 默认形状：圆形（对标 ContainerShapeRound）
    pub fn shape() -> Shape {
        Shape::Circle
    }

    pub fn icon_button_colors(theme: &ThemeColors) -> IconButtonColors {
        IconButtonColors::from_theme(theme, IconButtonStyle::Standard)
    }

    pub fn filled_icon_button_colors(theme: &ThemeColors) -> IconButtonColors {
        IconButtonColors::from_theme(theme, IconButtonStyle::Filled)
    }

    pub fn filled_tonal_icon_button_colors(theme: &ThemeColors) -> IconButtonColors {
        IconButtonColors::from_theme(theme, IconButtonStyle::FilledTonal)
    }

    pub fn outlined_icon_button_colors(theme: &ThemeColors) -> IconButtonColors {
        IconButtonColors::from_theme(theme, IconButtonStyle::Outlined)
    }

    /// Outlined 边框：OutlineVariant 色；disabled 为 38% alpha；
    /// 宽度按尺寸变体（token OutlinedOutlineWidth）
    pub fn outlined_border(theme: &ThemeColors, enabled: bool, width: f32) -> ButtonBorder {
        // OutlinedIconButtonTokens.OutlineColor = OutlineVariant
        let c = theme.outline_variant;
        let color = if enabled {
            c
        } else {
            Color::from_argb((c.a as f32 * 0.38) as u8, c.r, c.g, c.b)
        };
        ButtonBorder::new(width, color)
    }

    fn colors_for(theme: &ThemeColors, style: IconButtonStyle) -> IconButtonColors {
        match style {
            IconButtonStyle::Standard => Self::icon_button_colors(theme),
            IconButtonStyle::Filled => Self::filled_icon_button_colors(theme),
            IconButtonStyle::FilledTonal => Self::filled_tonal_icon_button_colors(theme),
            IconButtonStyle::Outlined => Self::outlined_icon_button_colors(theme),
        }
    }
}

/// IconButton 组件 Builder
pub struct IconButton {
    style: IconButtonStyle,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<IconButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    shape: Shape,
    border: Option<ButtonBorder>,
    size_variant: IconButtonSize,
    modifier: Modifier,
}

impl IconButton {
    /// 标准图标按钮（对标 material3 `IconButton`）
    pub fn new() -> Self {
        Self {
            style: IconButtonStyle::Standard,
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            shape: IconButtonDefaults::shape(),
            border: None,
            size_variant: IconButtonSize::Small,
            modifier: Modifier::new(),
        }
    }

    /// 实心图标按钮（对标 `FilledIconButton`）
    pub fn filled() -> Self {
        Self::new().style(IconButtonStyle::Filled)
    }

    /// 柔和图标按钮（对标 `FilledTonalIconButton`）
    pub fn filled_tonal() -> Self {
        Self::new().style(IconButtonStyle::FilledTonal)
    }

    /// 轮廓图标按钮（对标 `OutlinedIconButton`）
    pub fn outlined() -> Self {
        Self::new().style(IconButtonStyle::Outlined)
    }

    pub fn style(mut self, style: IconButtonStyle) -> Self {
        self.style = style;
        self
    }

    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: IconButtonColors) -> Self {
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

    /// 注册到组合树并执行子内容（内容通常是一个 `Icon`——tint Auto 会取
    /// IconButton 提供的内容色）
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| IconButtonDefaults::colors_for(&theme, self.style));
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let container = colors.container_color(self.enabled);
        let content_color = colors.content_color(self.enabled);
        let shape = self.shape;

        let size = self.size_variant.container_size();
        let mut modifier = Modifier::new()
            .size(size, size)
            .background(container, shape);
        // 边框：显式优先；Outlined 默认 OutlineVariant，宽度随尺寸变体
        let border = self.border.or_else(|| {
            if self.style == IconButtonStyle::Outlined {
                Some(IconButtonDefaults::outlined_border(
                    &theme,
                    self.enabled,
                    self.size_variant.outline_width(),
                ))
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape);
        }
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_click) = &self.on_click {
                let cb = on_click.clone();
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb())
                    // 波纹裁剪到容器形状（圆形）
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
                // 内容色下传：Icon tint Auto / Text 均取此色（对标 LocalContentColor）
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

    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_style(&self) -> IconButtonStyle {
        self.style
    }
    pub fn get_colors(&self) -> Option<IconButtonColors> {
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
    fn default_colors_per_style() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let s = IconButtonDefaults::icon_button_colors(&theme);
        assert_eq!(s.container.a, 0, "标准版容器透明");
        assert_eq!(s.content, theme.on_surface);
        let f = IconButtonDefaults::filled_icon_button_colors(&theme);
        assert_eq!(f.container, theme.primary);
        assert_eq!(f.content, theme.on_primary);
        let t = IconButtonDefaults::filled_tonal_icon_button_colors(&theme);
        assert_eq!(t.container, theme.secondary_container);
        assert_eq!(t.content, theme.on_secondary_container);
        let o = IconButtonDefaults::outlined_icon_button_colors(&theme);
        assert_eq!(o.container.a, 0);
        assert_eq!(o.content, theme.on_surface_variant);
    }

    #[test]
    fn outlined_border_disabled_alpha() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let enabled = IconButtonDefaults::outlined_border(&theme, true, 1.0);
        assert_eq!(enabled.color, theme.outline_variant, "Outlined 边框 = OutlineVariant");
        let disabled = IconButtonDefaults::outlined_border(&theme, false, 1.0);
        assert_eq!(disabled.color.a, (theme.outline_variant.a as f32 * 0.38) as u8);
        assert_eq!(
            IconButtonDefaults::outlined_border(&theme, true, IconButtonSize::Large.outline_width())
                .width,
            2.0,
            "Large 边框 2dp"
        );
    }

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = IconButtonDefaults::filled_icon_button_colors(&theme);
        assert_eq!(colors.container_color(true), theme.primary);
        assert_eq!(colors.container_color(false), colors.disabled_container);
        assert_eq!(colors.content_color(true), theme.on_primary);
        assert_eq!(colors.content_color(false), colors.disabled_content);
        // M3 token：禁用容器 OnSurface 10%、禁用内容 OnSurface 38%
        assert_eq!(colors.disabled_container.a, (theme.on_surface.a as f32 * 0.10) as u8);
        assert_eq!(colors.disabled_content.a, (theme.on_surface.a as f32 * 0.38) as u8);
        let standard = IconButtonDefaults::icon_button_colors(&theme);
        assert_eq!(standard.disabled_container.a, 0, "标准版禁用容器仍透明");
    }

    #[test]
    fn variant_constructors() {
        assert_eq!(IconButton::new().get_style(), IconButtonStyle::Standard);
        assert_eq!(IconButton::filled().get_style(), IconButtonStyle::Filled);
        assert_eq!(IconButton::filled_tonal().get_style(), IconButtonStyle::FilledTonal);
        assert_eq!(IconButton::outlined().get_style(), IconButtonStyle::Outlined);
        assert_eq!(IconButton::new().get_shape(), Shape::Circle);
        assert_eq!(IconButton::new().get_size(), IconButtonSize::Small);
        assert_eq!(
            IconButton::new().size(IconButtonSize::Large).get_size(),
            IconButtonSize::Large
        );
    }

    #[test]
    fn size_variant_dimensions() {
        assert_eq!(IconButtonSize::XSmall.container_size(), 32.0);
        assert_eq!(IconButtonSize::Small.container_size(), 48.0);
        assert_eq!(IconButtonSize::Medium.container_size(), 56.0);
        assert_eq!(IconButtonSize::Large.container_size(), 96.0);
        assert_eq!(IconButtonSize::XLarge.container_size(), 136.0);
        assert_eq!(IconButtonSize::XSmall.icon_size(), 20.0);
        assert_eq!(IconButtonSize::Small.icon_size(), 24.0);
        assert_eq!(IconButtonSize::Medium.icon_size(), 24.0);
        assert_eq!(IconButtonSize::Large.icon_size(), 32.0);
        assert_eq!(IconButtonSize::XLarge.icon_size(), 40.0);
        assert_eq!(IconButtonSize::XSmall.outline_width(), 1.0);
        assert_eq!(IconButtonSize::Medium.outline_width(), 1.0);
        assert_eq!(IconButtonSize::Large.outline_width(), 2.0);
        assert_eq!(IconButtonSize::XLarge.outline_width(), 3.0);
    }

    #[test]
    fn content_color_propagates_to_subtree() {
        // with_content_color 覆盖子树内容色（Icon tint Auto 的取值来源）
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_content_color(Color::RED, ctx, |ctx| {
                assert_eq!(WiniaTheme::content_color(), Color::RED);
                WiniaTheme::with_content_color(Color::BLUE, ctx, |ctx| {
                    assert_eq!(WiniaTheme::content_color(), Color::BLUE, "嵌套覆盖生效");
                });
                assert_eq!(WiniaTheme::content_color(), Color::RED, "闭包退出后恢复");
            });
        });
    }

    #[test]
    fn theme_default_content_color_is_on_surface() {
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| {
                let theme = WiniaTheme::colors();
                assert_eq!(WiniaTheme::content_color(), theme.on_surface);
            });
        });
    }

    #[test]
    fn filled_button_icon_tint_resolves_to_content_color() {
        // 集成：Filled IconButton 内容中的 Icon（tint Auto）应解析为 on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconButton::filled()
                    .on_click(|| {})
                    .build(ctx, |ctx| {
                        crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut resolved_tint = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    resolved_tint = spec.tint;
                }
            }
        }
        assert_eq!(resolved_tint, Some(theme.on_primary), "Filled 内容色下传 Icon tint");
    }

    #[test]
    fn disabled_icon_button_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            IconButton::new()
                .enabled(false)
                .on_click(|| {})
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
                    "禁用 IconButton 不应有波纹/可聚焦元素: {el:?}"
                );
            }
        }
    }
}
