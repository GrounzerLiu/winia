//! IconButton 组件 — 对标 material3 `IconButton` / `FilledIconButton` /
//! `FilledTonalIconButton` / `OutlinedIconButton`

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::components::button::ButtonBorder;
use crate::interaction::MutableInteractionSource;
use crate::theme::{ThemeColors, WiniaTheme};
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
    /// Container height in dp — the `ContainerHeight` token for this size (32/40/56/96/136). Small
    /// used to read 48, merging M3's 40 dp container with the 48 dp touch target; the two are
    /// separate now, and `build` reserves the target with `minimum_interactive_component_size`.
    pub fn container_size(self) -> f32 {
        match self {
            IconButtonSize::XSmall => 32.0,
            IconButtonSize::Small => 40.0,
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

    /// Leading + trailing space this size leaves around the icon for a width option —
    /// `{Narrow,Default,Wide}LeadingSpace` + the matching `...TrailingSpace` in the size's token set.
    pub fn horizontal_space(self, option: IconButtonWidthOption) -> f32 {
        let (narrow, uniform, wide) = match self {
            IconButtonSize::XSmall => (8.0, 12.0, 20.0),
            IconButtonSize::Small => (8.0, 16.0, 28.0),
            IconButtonSize::Medium => (24.0, 32.0, 48.0),
            IconButtonSize::Large => (32.0, 64.0, 96.0),
            IconButtonSize::XLarge => (64.0, 96.0, 144.0),
        };
        match option {
            IconButtonWidthOption::Narrow => narrow,
            IconButtonWidthOption::Uniform => uniform,
            IconButtonWidthOption::Wide => wide,
        }
    }

    /// Container width: `IconSize + horizontal space`, the arithmetic `smallContainerSize` performs
    /// (`material3/IconButtonDefaults.kt:880-883`). `Uniform` reproduces the square container, so it
    /// equals that size's `ContainerHeight`.
    pub fn container_width(self, option: IconButtonWidthOption) -> f32 {
        self.icon_size() + self.horizontal_space(option)
    }

    /// `ContainerShapeRound` / `ContainerShapeSquare` for this size. `Round` is `CornerFull` at every
    /// size; `Square` is the size's rounded-rectangle token (CornerMedium / CornerLarge /
    /// CornerExtraLarge).
    pub fn container_shape(self, family: IconButtonShape) -> Shape {
        match family {
            IconButtonShape::Round => Shape::Circle,
            IconButtonShape::Square => match self {
                IconButtonSize::XSmall | IconButtonSize::Small => Shape::rounded(12.0),
                IconButtonSize::Medium => Shape::rounded(16.0),
                IconButtonSize::Large | IconButtonSize::XLarge => Shape::rounded(28.0),
            },
        }
    }

    /// `PressedContainerShape` for this size — the shape the container morphs to while pressed
    /// (CornerSmall / CornerMedium / CornerLarge).
    ///
    /// Note: these tokens exist in the material3 token set but nothing in the compose snapshot reads
    /// them yet, so this shape and the morph below are ours until upstream wires them up.
    pub fn pressed_container_shape(self) -> Shape {
        match self {
            IconButtonSize::XSmall | IconButtonSize::Small => Shape::rounded(8.0),
            IconButtonSize::Medium => Shape::rounded(12.0),
            IconButtonSize::Large | IconButtonSize::XLarge => Shape::rounded(16.0),
        }
    }

    /// The corner radius `container_shape` paints, as a number — with `CornerFull` expressed through
    /// `round`, half the container's shorter side, which is what `Shape::Circle` resolves to at paint
    /// time. The press morph interpolates these, and a `Circle` carries no radius to interpolate.
    pub fn container_radius(self, family: IconButtonShape, round: f32) -> f32 {
        match family {
            IconButtonShape::Round => round,
            IconButtonShape::Square => match self {
                IconButtonSize::XSmall | IconButtonSize::Small => 12.0,
                IconButtonSize::Medium => 16.0,
                IconButtonSize::Large | IconButtonSize::XLarge => 28.0,
            },
        }
    }

    /// `PressedContainerShape`'s corner radius (CornerSmall / CornerMedium / CornerLarge).
    pub fn pressed_container_radius(self) -> f32 {
        match self {
            IconButtonSize::XSmall | IconButtonSize::Small => 8.0,
            IconButtonSize::Medium => 12.0,
            IconButtonSize::Large | IconButtonSize::XLarge => 16.0,
        }
    }
}

/// Which of a size's two container shapes to use — `ContainerShapeRound` or `ContainerShapeSquare`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconButtonShape {
    /// `CornerFull`: a circle, for all sizes.
    Round,
    /// The size's rounded-rectangle token.
    Square,
}

/// `IconButtonWidthOption` (`material3/IconButtonDefaults.kt:886-905`): how much horizontal room the
/// container leaves around the icon. Upstream names these the narrow, uniform and wide
/// configurations, recommending them for small, medium and wide screens respectively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconButtonWidthOption {
    /// The dense configuration: `NarrowLeadingSpace` + `NarrowTrailingSpace`.
    Narrow,
    /// `DefaultLeadingSpace` + `DefaultTrailingSpace` — the square container, and the default.
    Uniform,
    /// The roomy configuration: `WideLeadingSpace` + `WideTrailingSpace`.
    Wide,
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
            // `IconButtonDefaults.iconButtonColors` hands the icon `LocalContentColor.current`
            // (`material3/IconButtonDefaults.kt:43-46`), so a standard icon button follows the
            // surface it sits on rather than a fixed role. `theme.on_surface` was neither that nor
            // the token (`StandardIconButtonTokens.Color` is `OnSurfaceVariant`).
            IconButtonStyle::Standard => {
                let content = crate::theme::WiniaTheme::content_color();
                Self::new(transparent, content, transparent, alpha(content, 0.38))
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
    /// The small container's height — M3's 40 dp container, inside the 48 dp touch target that
    /// `build` reserves via `minimum_interactive_component_size`.
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

    /// `color` at `factor` of its own alpha — the token sets express a disabled state as the enabled
    /// colour with `DisabledOpacity` (0.38) applied.
    fn faded(color: Color, factor: f32) -> Color {
        Color::from_argb((color.a as f32 * factor) as u8, color.r, color.g, color.b)
    }

    /// `IconButtonDefaults.iconButtonVibrantColors` (`material3/IconButtonDefaults.kt:110-155`): the
    /// standard style's high-contrast set. Where the plain standard colours hand the icon
    /// `LocalContentColor`, the vibrant set pins the token's role — `StandardIconButtonTokens.Color`,
    /// which is `OnSurfaceVariant`.
    pub fn icon_button_vibrant_colors(theme: &ThemeColors) -> IconButtonColors {
        let transparent = Color::from_argb(0, 0, 0, 0);
        IconButtonColors::new(
            transparent,
            theme.on_surface_variant,
            transparent,
            Self::faded(theme.on_surface, 0.38),
        )
    }

    /// `IconButtonDefaults.outlinedIconButtonVibrantColors` (`:581-628`) — the same shape for the
    /// outlined style, whose token `Color` is `OnSurfaceVariant` as well.
    pub fn outlined_icon_button_vibrant_colors(theme: &ThemeColors) -> IconButtonColors {
        let transparent = Color::from_argb(0, 0, 0, 0);
        IconButtonColors::new(
            transparent,
            theme.on_surface_variant,
            transparent,
            Self::faded(theme.on_surface, 0.38),
        )
    }

    /// `IconButtonDefaults.outlinedIconButtonVibrantBorder` (`:837-848`): the vibrant border takes the
    /// token's `OutlineColor` (`OutlineVariant`), whereas the plain one takes `LocalContentColor`.
    /// Both fade by `DisabledOpacity` (0.38) when disabled.
    pub fn outlined_icon_button_vibrant_border(
        theme: &ThemeColors,
        enabled: bool,
        width: f32,
    ) -> ButtonBorder {
        let c = theme.outline_variant;
        let color = if enabled { c } else { Self::faded(c, 0.38) };
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
    shape: Option<Shape>,
    shape_family: IconButtonShape,
    border: Option<ButtonBorder>,
    size_variant: IconButtonSize,
    width_option: IconButtonWidthOption,
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
            shape: None,
            shape_family: IconButtonShape::Round,
            border: None,
            size_variant: IconButtonSize::Small,
            width_option: IconButtonWidthOption::Uniform,
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
        self.shape = Some(shape);
        self
    }

    /// The size's shape family — `Round` (the default) or `Square`. An explicit `shape()` wins.
    pub fn shape_family(mut self, family: IconButtonShape) -> Self {
        self.shape_family = family;
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

    /// How much horizontal room the container leaves around the icon (default `Uniform`, the square
    /// container). See [`IconButtonSize::container_width`].
    pub fn width_option(mut self, option: IconButtonWidthOption) -> Self {
        self.width_option = option;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 注册到组合树并执行子内容（内容通常是一个 `Icon`——tint Auto 会取
    /// IconButton 提供的内容色）
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        // Declared because `build` reads them below: the Skip contract is a whitelist, so a
        // parameter that changes without being declared here leaves a Clean slot skipping straight
        // past the new value (the container size, the background/border shape, the border).
        ctx.changed(&self.size_variant);
        ctx.changed(&self.shape);
        ctx.changed(&self.border);
        ctx.changed(&self.width_option);
        ctx.changed(&self.shape_family);
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
        // Compose: `.minimumInteractiveComponentSize().size(smallContainerSize())`
        // (`material3/IconButton.kt:178-179`) — the token container (40 dp tall for Small) sits
        // inside a 48 dp touch target, and the width option sets the horizontal space. The morph
        // below needs the container's own size, because `CornerFull` is half the shorter side.
        let width = self.size_variant.container_width(self.width_option);
        let height = self.size_variant.container_size();

        // The container's shape. An explicit `shape()` wins outright, press included; otherwise the
        // size's shape family with a press travelling to that size's `PressedContainerShape` — an
        // animated morph rather than a switch, so the corners move. (`Shape::Circle` has no radius to
        // interpolate, hence the arithmetic.) The animation runs through recomposition: reading the
        // animated value here re-runs this build every frame, so the painted corner follows it.
        let shape = match self.shape {
            Some(explicit) => explicit,
            None => {
                let round = width.min(height) / 2.0;
                let morph = ctx.animate_float_as_state(
                    if interaction.state(self.enabled).pressed { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Spring(
                        crate::animation::SpringSpec::spatial_fast(),
                    ),
                );
                let rest = self.size_variant.container_radius(self.shape_family, round);
                let pressed = self.size_variant.pressed_container_radius();
                Shape::rounded(rest + (pressed - rest) * morph.get())
            }
        };

        let mut modifier = Modifier::new()
            .minimum_interactive_component_size()
            .size(width, height)
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
        modifier = modifier.semantics(
            crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::Button)
                .merge_descendants(true)
                .state(crate::semantics::SemanticsState::new().enabled(self.enabled)),
        );
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
                    crate::components::text::ProvideTextStyle(
                        crate::text::TextStyle::new().color(content_color),
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
            .unwrap_or_else(|| self.size_variant.container_shape(self.shape_family))
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
        // The standard style hands the icon `LocalContentColor.current`, exactly as
        // `IconButtonDefaults.iconButtonColors` does, so it has to resolve while composing. The
        // filled/tonal/outlined styles use fixed roles and need no composition.
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |_ctx| {
                let s = IconButtonDefaults::icon_button_colors(&theme);
                assert_eq!(s.container.a, 0, "standard container is transparent");
                let local = WiniaTheme::content_color();
                assert_eq!(s.content, local, "standard content follows LocalContentColor");
                assert_eq!(s.disabled_content.a, (local.a as f32 * 0.38) as u8);
            });
        });
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
    fn a_size_change_reaches_the_restartable_group() {
        // `size_variant` is read by `build` (container size and outlined border width), so it must be
        // declared with `ctx.changed`: the Skip contract is a whitelist, and an undeclared parameter
        // that changes leaves a Clean slot skipping straight past the new value.
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let size = crate::runtime::state::State::new(IconButtonSize::Small);
        let mut composer = crate::runtime::composer::Composer::new();
        // `build` reserves a 48 dp touch target around the token container, so the widest node is the
        // target: 48 for Small (whose container is 40), and the container itself for Large (96).
        let widest = |composer: &crate::runtime::composer::Composer| -> f32 {
            composer
                .arena_nodes()
                .iter()
                .map(|node| node.measured_size.width)
                .fold(0.0f32, f32::max)
        };
        let mut compose = |composer: &mut crate::runtime::composer::Composer| {
            let chosen = size.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconButton::new()
                        .size(chosen)
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
        };
        let constraints = crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0);
        compose(&mut composer);
        composer.layout(constraints);
        assert_eq!(
            widest(&composer),
            48.0,
            "Small's 40 dp container sits inside a 48 dp touch target"
        );

        size.update(|s| *s = IconButtonSize::Large);
        compose(&mut composer);
        composer.layout(constraints);
        assert_eq!(
            widest(&composer),
            96.0,
            "the new size reaches the slot instead of being skipped"
        );
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
    fn vibrant_colors_use_the_token_roles() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        // Standard: the vibrant set pins `StandardIconButtonTokens.Color` (OnSurfaceVariant), where the
        // plain set hands the icon `LocalContentColor`.
        let standard = IconButtonDefaults::icon_button_vibrant_colors(&theme);
        assert_eq!(
            standard.container.a, 0,
            "vibrant standard keeps a transparent container"
        );
        assert_eq!(standard.content, theme.on_surface_variant);
        assert_eq!(
            standard.disabled_content.a,
            (theme.on_surface.a as f32 * 0.38) as u8,
            "disabled content is OnSurface at DisabledOpacity"
        );

        let outlined = IconButtonDefaults::outlined_icon_button_vibrant_colors(&theme);
        assert_eq!(outlined.container.a, 0);
        assert_eq!(outlined.content, theme.on_surface_variant);

        // Vibrant border: the token's OutlineColor, not the LocalContentColor the plain border uses.
        let on = IconButtonDefaults::outlined_icon_button_vibrant_border(&theme, true, 1.0);
        assert_eq!(on.color, theme.outline_variant);
        assert_eq!(on.width, 1.0);
        let off = IconButtonDefaults::outlined_icon_button_vibrant_border(&theme, false, 1.0);
        assert_eq!(off.color.a, (theme.outline_variant.a as f32 * 0.38) as u8);
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
        assert_eq!(IconButtonSize::Small.container_size(), 40.0);
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
    fn width_options_match_the_tokens() {
        use IconButtonWidthOption::{Narrow, Uniform, Wide};
        // Uniform is the square container, so its width IS that size's `ContainerHeight` token.
        for (size, height) in [
            (IconButtonSize::XSmall, 32.0),
            (IconButtonSize::Small, 40.0),
            (IconButtonSize::Medium, 56.0),
            (IconButtonSize::Large, 96.0),
            (IconButtonSize::XLarge, 136.0),
        ] {
            assert_eq!(
                size.container_width(Uniform),
                height,
                "Uniform reproduces the square container"
            );
        }
        // Narrow and Wide are the token spaces around the icon: Small 4+4 / 14+14, Medium 12+12 /
        // 24+24, XLarge 32+32 / 72+72.
        assert_eq!(IconButtonSize::Small.container_width(Narrow), 32.0);
        assert_eq!(IconButtonSize::Small.container_width(Wide), 52.0);
        assert_eq!(IconButtonSize::Medium.container_width(Narrow), 48.0);
        assert_eq!(IconButtonSize::Medium.container_width(Wide), 72.0);
        assert_eq!(IconButtonSize::XLarge.container_width(Wide), 184.0);
        assert_eq!(IconButtonSize::Small.horizontal_space(Narrow), 8.0);
        assert_eq!(IconButtonSize::Large.horizontal_space(Uniform), 64.0);
    }

    #[test]
    fn shape_families_and_pressed_shapes_match_the_tokens() {
        for size in [
            IconButtonSize::XSmall,
            IconButtonSize::Small,
            IconButtonSize::Medium,
            IconButtonSize::Large,
            IconButtonSize::XLarge,
        ] {
            assert_eq!(
                size.container_shape(IconButtonShape::Round),
                Shape::Circle,
                "ContainerShapeRound is CornerFull at every size"
            );
        }
        assert_eq!(
            IconButtonSize::Small.container_shape(IconButtonShape::Square),
            Shape::rounded(12.0),
            "ContainerShapeSquare is CornerMedium for Small"
        );
        assert_eq!(
            IconButtonSize::Medium.container_shape(IconButtonShape::Square),
            Shape::rounded(16.0)
        );
        assert_eq!(
            IconButtonSize::XLarge.container_shape(IconButtonShape::Square),
            Shape::rounded(28.0)
        );
        // PressedContainerShape: CornerSmall / CornerMedium / CornerLarge.
        assert_eq!(
            IconButtonSize::Small.pressed_container_shape(),
            Shape::rounded(8.0)
        );
        assert_eq!(
            IconButtonSize::Medium.pressed_container_shape(),
            Shape::rounded(12.0)
        );
        assert_eq!(
            IconButtonSize::XLarge.pressed_container_shape(),
            Shape::rounded(16.0)
        );
    }

    #[test]
    fn a_press_morphs_the_container_shape_over_time() {
        // The container paints through `background(color, shape)`, so the shape its node carries is
        // wherever the morph has reached. The animation lives in the composer's slot, so this reuses
        // one composer instead of rebuilding per sample.
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconButton::new()
                        .on_click(|| {})
                        .interaction_source(src.clone())
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let radius = |composer: &crate::runtime::composer::Composer| -> f32 {
            let painted = composer.arena_nodes().iter().find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { shape, .. } => Some(*shape),
                    _ => None,
                })
            });
            match painted {
                Some(Shape::RoundedRect { corner_radius }) => corner_radius,
                other => panic!("the container should be a rounded rect, got {other:?}"),
            }
        };

        build(&mut composer);
        // Round is CornerFull — half the 40 dp container, which is what `Shape::Circle` paints.
        assert_eq!(radius(&composer), 20.0, "at rest the container is Round");

        source.emit_press();
        build(&mut composer);
        assert_ne!(
            radius(&composer),
            8.0,
            "a press starts a morph instead of switching the shape"
        );

        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            (radius(&composer) - 8.0).abs() < 0.5,
            "and it arrives at Small's PressedContainerShape, got {}",
            radius(&composer)
        );
    }

    #[test]
    fn content_color_propagates_to_subtree() {
        // with_content_color 覆盖子树内容色（Icon tint Auto 的取值来源）
        let mut composer = crate::runtime::composer::Composer::new();
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
        let mut composer = crate::runtime::composer::Composer::new();
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
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconButton::filled()
                    .on_click(|| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
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
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            IconButton::new()
                .enabled(false)
                .on_click(|| {})
                .build(ctx, |ctx| {
                    crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
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
