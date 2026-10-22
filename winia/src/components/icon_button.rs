//! IconButton 组件 — 对标 material3 `IconButton` / `FilledIconButton` /
//! `FilledTonalIconButton` / `OutlinedIconButton`

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::components::button::ButtonBorder;
use crate::input::interaction::MutableInteractionSource;
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

    /// `SelectedContainerShapeRound` / `SelectedContainerShapeSquare` — the shape a TOGGLE's
    /// container takes while CHECKED.
    ///
    /// This is the M3 Expressive swap and it INVERTS the family. The tokens say so directly: every
    /// size has `ContainerShapeRound = CornerFull` and `SelectedContainerShapeRound` = this size's
    /// rounded rectangle (CornerMedium 12 for XS/S, CornerLarge 16 for M, CornerExtraLarge 28 for
    /// L/XL), while `ContainerShapeSquare` is that same rectangle and `SelectedContainerShapeSquare`
    /// is `CornerFull`. So a round toggle rests as a circle and becomes a rounded rectangle when
    /// selected, and a square one does the opposite.
    /// The corner radius the SELECTED container paints, as a number — with `CornerFull` expressed
    /// through `round`, half the container's shorter side, exactly as `container_radius` does.
    ///
    /// (There was a `selected_container_shape` beside this returning a `Shape`; it had no caller —
    /// the toggle resolves this per size and the shape family is `Round` there — so it went rather
    /// than sitting in the API waiting for one.)
    pub fn selected_container_radius(self, family: IconButtonShape, round: f32) -> f32 {
        match family {
            IconButtonShape::Round => match self {
                IconButtonSize::XSmall | IconButtonSize::Small => 12.0,
                IconButtonSize::Medium => 16.0,
                IconButtonSize::Large | IconButtonSize::XLarge => 28.0,
            },
            IconButtonShape::Square => round,
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
            // `IconButtonDefaults.outlinedIconButtonColors` hands the icon `LocalContentColor.current`
            // too, exactly like the standard style (`material3/IconButtonDefaults.kt:505-513`). The
            // token's `OnSurfaceVariant` is what the VIBRANT set pins, and using it here made the plain
            // and the vibrant outlined button indistinguishable.
            IconButtonStyle::Outlined => {
                let content = crate::theme::WiniaTheme::content_color();
                Self::new(transparent, content, transparent, alpha(content, 0.38))
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
    /// `IconButtonDefaults.outlinedIconButtonBorder` (`material3/IconButtonDefaults.kt:819-826`): the
    /// plain border takes `LocalContentColor`, NOT the token's `OutlineVariant` — that one belongs to
    /// the vibrant border, and using it here made the plain and vibrant outlines the same colour.
    pub fn outlined_border(_theme: &ThemeColors, enabled: bool, width: f32) -> ButtonBorder {
        let c = crate::theme::WiniaTheme::content_color();
        let color = if enabled { c } else { Self::faded(c, 0.38) };
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
                // The paint is inset back to the token container (see `inset` below), so CornerFull is
                // half of that — not half the 48 dp touch target the node is measured at.
                let round = width.min(height) / 2.0;
                // The spec is `MotionSchemeKeyTokens.DefaultEffects`, NOT a spatial spring:
                // upstream's `shapeForInteraction` says the choice is intentional "to prevent any
                // bounce in this component" (`material3/IconButton.kt:1558-1568`, and the same note
                // at `:1576-1586` for the toggle). A spatial spring overshoots by design — at the
                // expressive scheme's `fast_spatial` (damping 0.6) visibly.
                let morph = ctx.animate_float_as_state(
                    if interaction.state(self.enabled).pressed { 1.0 } else { 0.0 },
                    shape_morph_spec(),
                );
                let rest = self.size_variant.container_radius(self.shape_family, round);
                let pressed = self.size_variant.pressed_container_radius();
                Shape::rounded(rest + (pressed - rest) * morph.get())
            }
        };

        // The node is grown to the 48 dp touch target; the renderer insets the paint back to the content
        // box using the offset this publishes, so the container keeps its token size.
        // `.clip(shape)` sits before the background, as in compose (`IconButton.kt:180`). The renderer clips
        // at `paint_rect` — the token container the background paints, not the 48 dp touch target the node is
        // measured at — so this clips the container's own shape.
        let mut modifier = Modifier::new()
            .minimum_interactive_component_size()
            .size(width, height)
            .clip(shape)
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

/// The spec a container's SHAPE morph runs on: `MotionSchemeKeyTokens.DefaultEffects`.
///
/// Upstream picks it deliberately rather than a spatial spring — "TODO Load the motionScheme tokens
/// from the component tokens file / MotionSchemeKeyTokens.DefaultEffects is intentional here to
/// prevent any bounce in this component" (`material3/IconButton.kt:1558-1568` for `IconButton`,
/// the same note at `:1576-1586` for `IconToggleButton`). A spatial spring overshoots by design,
/// and at the expressive scheme's `fast_spatial` (damping 0.6, stiffness 800) that overshoot is
/// large. `IconToggleButton` shares this function because upstream uses the same token for both.
pub(crate) fn shape_morph_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default())
}


#[cfg(test)]
mod tests;
