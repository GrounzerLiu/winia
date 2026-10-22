//! Button 组件 — 可点击按钮
//!
//! 用法:
//! ```ignore
//! Button::new()
//!     .on_click(|| println!("clicked"))
//!     .modifier(Modifier::new().size(200.0, 48.0).background(Color::BLUE))
//!     .build(ctx, |ctx| {
//!         Text::new("Click me").color(Color::WHITE).build(ctx);
//!     });
//! ```

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::layout::Dimension;
use crate::graphics::{Shape};
use crate::input::interaction::{ComponentState, MutableInteractionSource};
use std::sync::Arc;
use std::fmt;

/// Button 变体风格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// 实心填充按钮（主要操作）
    Filled,
    /// 悬浮按钮（对标 `ElevatedButton`）——SurfaceContainerLow 容器 + primary 内容
    Elevated,
    /// 轮廓按钮（次要操作）
    Outlined,
    /// 纯文本按钮（最低强调）
    Text,
    /// 带浅色背景的填充按钮
    Tonal,
}

/// Button 尺寸变体（对标 material3 ButtonTokens 系列，v0_11_0）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
}

impl ButtonSize {
    /// 容器高度（ContainerHeight：32/40/56/96/136）
    pub fn container_height(self) -> f32 {
        match self {
            ButtonSize::XSmall => 32.0,
            ButtonSize::Small => 40.0,
            ButtonSize::Medium => 56.0,
            ButtonSize::Large => 96.0,
            ButtonSize::XLarge => 136.0,
        }
    }

    /// 建议图标尺寸（IconSize：20/20/24/32/40）
    pub fn icon_size(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small => 20.0,
            ButtonSize::Medium => 24.0,
            ButtonSize::Large => 32.0,
            ButtonSize::XLarge => 40.0,
        }
    }

    /// 图标/文字间距（IconLabelSpace：8/8/8/12/16）
    pub fn icon_label_space(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small | ButtonSize::Medium => 8.0,
            ButtonSize::Large => 12.0,
            ButtonSize::XLarge => 16.0,
        }
    }

    /// Horizontal padding, `LeadingSpace`/`TrailingSpace` per size: 16/16/24/48/64.
    pub fn horizontal_padding(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small => 16.0,
            ButtonSize::Medium => 24.0,
            ButtonSize::Large => 48.0,
            ButtonSize::XLarge => 64.0,
        }
    }

    /// `PressedContainerShape` as a corner radius — the shape the container morphs to while it is
    /// pressed. `ButtonXSmallTokens`/`ButtonSmallTokens` say CornerSmall (8), `ButtonMediumTokens`
    /// CornerMedium (12), `ButtonLargeTokens`/`ButtonXLargeTokens` CornerLarge (16).
    pub fn pressed_container_radius(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small => 8.0,
            ButtonSize::Medium => 12.0,
            ButtonSize::Large | ButtonSize::XLarge => 16.0,
        }
    }

    /// Outlined 边框宽度（OutlinedOutlineWidth：1/1/1/2/3）
    pub fn outline_width(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small | ButtonSize::Medium => 1.0,
            ButtonSize::Large => 2.0,
            ButtonSize::XLarge => 3.0,
        }
    }
}

/// 按钮颜色集（对标 material3 `ButtonColors`）——container/content 各含
/// enabled/disabled 变体；`container_color(enabled)` / `content_color(enabled)`
/// 按状态取色（禁用：M3 token——容器 OnSurface@10/12%、内容 OnSurface(Variant)@38%）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonColors {
    pub container: crate::graphics::Color,
    pub content: crate::graphics::Color,
    pub disabled_container: crate::graphics::Color,
    pub disabled_content: crate::graphics::Color,
}

impl ButtonColors {
    pub fn new(
        container: crate::graphics::Color,
        content: crate::graphics::Color,
        disabled_container: crate::graphics::Color,
        disabled_content: crate::graphics::Color,
    ) -> Self {
        Self { container, content, disabled_container, disabled_content }
    }

    /// 按启用状态取容器色
    pub fn container_color(&self, enabled: bool) -> crate::graphics::Color {
        self.container_color_for(&ComponentState { enabled, ..ComponentState::idle() })
    }

    /// 按启用状态取内容（文字）色
    pub fn content_color(&self, enabled: bool) -> crate::graphics::Color {
        self.content_color_for(&ComponentState { enabled, ..ComponentState::idle() })
    }

    /// 状态化容器色——仅区分 enabled/disabled（与 material3 ButtonColors 一致；
    /// hover/focus/press 视觉由 indication（ripple）状态层绘制，避免双重叠加）。
    pub fn container_color_for(&self, state: &ComponentState) -> crate::graphics::Color {
        if !state.enabled {
            self.disabled_container
        } else {
            self.container
        }
    }

    /// 状态化内容色（内容色仅区分 enabled/disabled——与 material3 ButtonColors 一致）
    pub fn content_color_for(&self, state: &ComponentState) -> crate::graphics::Color {
        if state.enabled { self.content } else { self.disabled_content }
    }

    /// 从主题按 style 生成默认色（Compose ButtonDefaults.buttonColors 对标）
    pub fn from_theme(theme: &crate::theme::ThemeColors, style: ButtonStyle) -> Self {
        use crate::graphics::Color;
        let (container, content) = match style {
            ButtonStyle::Filled => (theme.primary, theme.on_primary),
            ButtonStyle::Elevated => (theme.surface_container_low, theme.primary),
            ButtonStyle::Tonal => (theme.secondary_container, theme.on_secondary_container),
            ButtonStyle::Outlined => (Color::from_argb(0, 0, 0, 0), theme.on_surface_variant),
            ButtonStyle::Text => (Color::from_argb(0, 0, 0, 0), theme.primary),
        };
        // M3 token：Filled/Elevated 禁用容器 OnSurface@10%、FilledTonal 12%、
        // Outlined/Text 容器透明；禁用内容 OnSurface(Variant)@38%
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        let (disabled_container, disabled_content) = match style {
            ButtonStyle::Filled | ButtonStyle::Elevated => (
                alpha(theme.on_surface, 0.10),
                alpha(theme.on_surface_variant, 0.38),
            ),
            ButtonStyle::Tonal => (
                alpha(theme.on_surface, 0.12),
                alpha(theme.on_surface, 0.38),
            ),
            ButtonStyle::Outlined => (
                transparent,
                alpha(theme.on_surface_variant, 0.38),
            ),
            ButtonStyle::Text => (
                transparent,
                alpha(theme.on_surface_variant, 0.38),
            ),
        };
        Self::new(container, content, disabled_container, disabled_content)
    }
}

/// Button shadow elevation (the material3 `ButtonElevation` counterpart): a per-state elevation, applied as a
/// `Modifier.shadow` by `Button::build`. The Filled and Tonal defaults are `ElevationTokens.Level0` in every
/// state except HOVER, which is `Level1` (1 dp) — not all-zero; `ButtonElevation::elevated()` is
/// `ElevatedButtonTokens` (1/1/1/3/0 dp); Outlined and Text have no elevation entry at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub disabled: f32,
}

impl ButtonElevation {
    pub fn new(default: f32, pressed: f32, focused: f32, hovered: f32, disabled: f32) -> Self {
        Self { default, pressed, focused, hovered, disabled }
    }

    /// M3 `FilledButtonTokens` / `FilledTonalButtonTokens`: rest 0 / pressed 0 / focused 0 / **hovered 1** /
    /// disabled 0. The hover value is the one that is easy to miss — winia carried all zeros here and then
    /// skipped the elevation layer for a Filled button, so hovering one could never raise the 1 dp shadow M3
    /// gives it (the same shape of bug the Card had for its dragged elevation).
    pub fn default_elevation() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0, 0.0)
    }

    /// A button style that carries no elevation in any state: `OutlinedButtonTokens` and `TextButtonTokens`
    /// declare no elevation entries at all, so their shadow is always the ambient one.
    pub fn none() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0)
    }

    /// Whether every state's elevation is 0 — nothing to animate, no layer to pay for.
    pub fn is_shadow_free(&self) -> bool {
        self.default <= 0.0
            && self.pressed <= 0.0
            && self.focused <= 0.0
            && self.hovered <= 0.0
            && self.disabled <= 0.0
    }

    /// `ElevatedButtonTokens`, in dp: `ContainerElevation`/`PressedContainerElevation`/`FocusedContainerElevation`
    /// are `ElevationTokens.Level1` (1), `HoveredContainerElevation` is `Level2` — which is **3** dp, not the
    /// level index 2 — and `DisabledContainerElevation` is `Level0`.
    pub fn elevated() -> Self {
        Self::new(1.0, 1.0, 1.0, 3.0, 0.0)
    }

    /// The spec for a change of elevation — Compose's rule, shared with every other component that animates
    /// one ([`crate::animation::elevation_spec_for_pair`], from `material3/internal/Elevation.kt:50-61`,
    /// `:113-117`): 120 ms `FastOutSlowIn` towards an interaction, 150 ms of
    /// `CubicBezierEasing(0.40, 0.00, 0.60, 1.00)` away from one, 120 ms of the same curve out of HOVER.
    /// winia used one 180 ms `EaseOutCubic` for every direction.
    ///
    /// `from` and `to` are the interactions being left and entered; `(None, None)` means Compose snaps, which
    /// the callers express as an explicit `Snap`.
    pub fn spec_for(
        from: Option<crate::animation::ElevationInteraction>,
        to: Option<crate::animation::ElevationInteraction>,
    ) -> Option<crate::animation::AnimationSpec> {
        crate::animation::elevation_spec_for_pair(from, to)
    }

    /// The elevation for a state snapshot, by Compose's "latest interaction wins" order
    /// (`material3/Card.kt:705-719` is the same rule spelled out; the Button resolves it through the shared
    /// `animateElevation`): disabled, then a drag, then a press, then hover, then focus, then the default. A
    /// press flattening below the hover it followed is deliberate — `PressedContainerElevation` sits below
    /// `HoverContainerElevation` in every token set, which is M3's press feedback.
    ///
    /// A DRAGGED button takes the default elevation, because Compose's `ButtonElevation` matches only
    /// `PressInteraction.Press`, `HoverInteraction.Enter` and `FocusInteraction.Focus` and falls through to
    /// `defaultElevation` for anything else (`material3/Button.kt:978-988`) — and a drag is the latest
    /// interaction while it lasts. winia reported the pressed value here, which its own `pressed` guard made
    /// unreachable anyway: every drag holds a press (`MutableInteractionSource` has four independent flags).
    /// The `Card`, whose elevation DOES have a dragged entry, takes the drag's own value.
    pub fn for_state(&self, state: &ComponentState) -> f32 {
        if !state.enabled {
            self.disabled
        } else if state.dragged {
            self.default
        } else if state.pressed {
            self.pressed
        } else if state.hovered {
            self.hovered
        } else if state.focused {
            self.focused
        } else {
            self.default
        }
    }
}

impl Default for ButtonElevation {
    fn default() -> Self {
        Self::default_elevation()
    }
}

impl Default for ButtonStyle {
    fn default() -> Self {
        ButtonStyle::Filled
    }
}

/// 按钮边框（对标 material3 `BorderStroke`）——宽度 + 颜色；
/// 形状由 [`Button::shape`] 决定（画在容器形状边缘）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonBorder {
    pub width: f32,
    pub color: crate::graphics::Color,
}

impl ButtonBorder {
    pub fn new(width: f32, color: crate::graphics::Color) -> Self {
        Self { width, color }
    }

    /// OutlinedButton 默认边框（对标 material3：1dp + `OutlineVariant`；
    /// disabled 为 OutlineVariant @ DisabledContainerOpacity(0.1)）
    fn outlined(theme: &crate::theme::ThemeColors, enabled: bool) -> Self {
        // OutlinedButtonTokens.OutlineColor = OutlineVariant
        let c = theme.outline_variant;
        let color = if enabled {
            c
        } else {
            crate::graphics::Color::from_argb(
                (c.a as f32 * 0.10) as u8,
                c.r,
                c.g,
                c.b,
            )
        };
        Self::new(1.0, color)
    }
}

/// Button 默认值（对标 material3 `ButtonDefaults`——M3 small button tokens）
pub struct ButtonDefaults;


/// The mutable view a [`ButtonStyling`] writes into.
///
/// It starts from what the button would have used anyway (`colors` is already the variant default for
/// the current theme), so a styling that sets one field leaves the rest exactly as the roles left them.
/// `theme` is exposed so a styling can derive from roles instead of hard-coding a colour.
pub struct ButtonStylingScope {
    pub theme: crate::theme::ThemeColors,
    /// Whether the palette in scope is dark - the usual reason to branch inside a styling
    /// (`if scope.is_dark { ... } else { ... }`). Decided from the palette, so a subtree pinned to a
    /// light theme styles as light even when the system is dark.
    pub is_dark: bool,
    pub colors: ButtonColors,
    /// The Outlined variant's border colour. `None` keeps the theme's `outline_variant`; the other
    /// variants have no default border, so this only matters when the caller adds one.
    pub border: Option<crate::graphics::Color>,
}

impl ButtonStylingScope {
    /// The container colour for the resting, enabled state.
    pub fn container(&mut self, color: crate::graphics::Color) {
        self.colors.container = color;
    }

    /// The label/icon colour for the resting, enabled state.
    pub fn content(&mut self, color: crate::graphics::Color) {
        self.colors.content = color;
    }

    /// The disabled pair: the container is `on_surface` at 10% (Filled/Elevated) or 12% (Tonal) and
    /// transparent for Outlined/Text; the content is `on_surface_variant` at 38% throughout.
    pub fn disabled(&mut self, container: crate::graphics::Color, content: crate::graphics::Color) {
        self.colors.disabled_container = container;
        self.colors.disabled_content = content;
    }

    /// The Outlined variant's border colour.
    pub fn border(&mut self, color: crate::graphics::Color) {
        self.border = Some(color);
    }

    /// Replace the whole set at once.
    pub fn colors(&mut self, colors: ButtonColors) {
        self.colors = colors;
    }
}

/// Everything a button resolves from the theme plus the styling: its colour set and, when a styling
/// asked for one, the outlined border colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonVisual {
    pub colors: ButtonColors,
    pub border: Option<crate::graphics::Color>,
}

/// One button styling, registered through [`crate::components::component_styles::ComponentStyles::with`].
pub type ButtonStyling = crate::components::component_styles::Styling<ButtonStylingScope>;

/// What a button should look like: the variant default for `theme`, with the styling in scope applied
/// on top when there is one.
///
/// This is the ONE place the precedence (explicit parameter > styling > theme) is decided, so
/// `Button::build` must resolve through it - calling `ButtonColors::from_theme` directly would silently
/// ignore every styling.
pub fn button_visual_with_styles(
    theme: &crate::theme::ThemeColors,
    variant: ButtonStyle,
) -> ButtonVisual {
    let mut visual = ButtonVisual {
        colors: ButtonColors::from_theme(theme, variant),
        border: None,
    };
    let styles = crate::components::component_styles::current_component_styles();
    if let Some(styling) = styles.get::<ButtonStyling>() {
        let mut scope = ButtonStylingScope {
            theme: theme.clone(),
            is_dark: crate::components::component_styles::palette_is_dark(theme),
            colors: visual.colors,
            border: None,
        };
        styling.apply(&mut scope);
        visual.colors = scope.colors;
        visual.border = scope.border;
    }
    visual
}

/// The colour half of [`button_visual_with_styles`], for callers that only need the colours.
pub fn button_colors_with_styles(
    theme: &crate::theme::ThemeColors,
    variant: ButtonStyle,
) -> ButtonColors {
    button_visual_with_styles(theme, variant).colors
}

impl ButtonDefaults {
    /// 默认形状：胶囊（对标 `ButtonSmallTokens.ContainerShapeRound` = CornerFull）
    pub fn shape() -> Shape {
        Shape::pill()
    }

    /// 默认按钮颜色（对标 material3 `ButtonDefaults.buttonColors()`——
    /// 从主题色板按 style 推导容器/内容色）
    pub fn button_colors(
        theme: &crate::theme::ThemeColors,
        style: ButtonStyle,
    ) -> ButtonColors {
        ButtonColors::from_theme(theme, style)
    }

    /// `ButtonDefaults.buttonElevation()` — `FilledButtonTokens`: flat at rest and while pressed or focused,
    /// `Level1` (1 dp) while hovered.
    pub fn button_elevation() -> ButtonElevation {
        ButtonElevation::default_elevation()
    }

    /// ElevatedButton 默认阴影（对标 `ButtonDefaults.elevatedButtonElevation()`）
    pub fn elevated_button_elevation() -> ButtonElevation {
        ButtonElevation::elevated()
    }

    /// `ButtonDefaults.filledTonalButtonElevation()` (`material3/Button.kt:862-875`), whose defaults are
    /// `FilledTonalButtonTokens`: `ContainerElevation`, `PressedContainerElevation` and
    /// `FocusContainerElevation` are `ElevationTokens.Level0`, `HoverContainerElevation` is `Level1` (1 dp) and
    /// the disabled elevation is 0. A FilledTonal button therefore has NO shadow at rest — only a hover lifts
    /// it 1 dp — the same numbers as `button_elevation()`, but from its own token set.
    pub fn filled_tonal_button_elevation() -> ButtonElevation {
        ButtonElevation::new(0.0, 0.0, 0.0, 1.0, 0.0)
    }

    /// 默认最小宽度（对标 `ButtonDefaults.MinWidth` 58.dp）
    pub fn min_width() -> f32 {
        58.0
    }

    /// 默认最小高度（对标 `ButtonDefaults.MinHeight` 40.dp——ContainerHeight）
    pub fn min_height() -> f32 {
        ButtonSize::Small.container_height()
    }

    pub fn min_height_for(size: ButtonSize) -> f32 {
        size.container_height()
    }

    /// 默认内容 padding `(start, top, end, bottom)`——Button 系 24/8/24/8，
    /// Text 按钮 12/8/12/8（对标 `ButtonDefaults.ContentPadding` /
    /// `TextButtonContentPadding`）；`SizeValue`——支持动画
    pub fn content_padding(style: ButtonStyle) -> (Dimension, Dimension, Dimension, Dimension) {
        match style {
            ButtonStyle::Text => (12.0.into(), 8.0.into(), 12.0.into(), 8.0.into()),
            _ => (24.0.into(), 8.0.into(), 24.0.into(), 8.0.into()),
        }
    }

    /// 图标与文字间距（对标 `ButtonDefaults.IconSpacing` =
    /// ButtonSmallTokens.IconLabelSpace，8dp）——M3 不自动应用，
    /// 本框架在内容 Row 内自动应用；内部 Row 的 spacing 无法从外部覆盖，
    /// 自定义间距时给 Icon/Text 加 padding 或自排内容
    pub fn icon_spacing() -> f32 {
        ButtonSize::Small.icon_label_space()
    }

    pub fn icon_spacing_for(size: ButtonSize) -> f32 {
        size.icon_label_space()
    }

    /// 带图标的 Button 内容 padding（对标 `ButtonDefaults.ButtonWithIconContentPadding`：
    /// 左 16 / 右 24——图标贴近边缘，文字保持标准右边距）
    pub fn button_with_icon_content_padding() -> (Dimension, Dimension, Dimension, Dimension) {
        (16.0.into(), 8.0.into(), 24.0.into(), 8.0.into())
    }

    /// 带图标的 Text 按钮内容 padding（对标
    /// `ButtonDefaults.TextButtonWithIconContentPadding`：左 12 / 右 16）
    pub fn text_button_with_icon_content_padding() -> (Dimension, Dimension, Dimension, Dimension) {
        (12.0.into(), 8.0.into(), 16.0.into(), 8.0.into())
    }

    /// 按尺寸取默认内容 padding：水平 = 尺寸 Leading/Trailing，垂直 8；
    /// Text 按钮固定 12/8/12/8（紧凑）
    pub fn content_padding_for(
        size: ButtonSize,
        style: ButtonStyle,
    ) -> (Dimension, Dimension, Dimension, Dimension) {
        if style == ButtonStyle::Text {
            return (12.0.into(), 8.0.into(), 12.0.into(), 8.0.into());
        }
        let h = size.horizontal_padding();
        (h.into(), 8.0.into(), h.into(), 8.0.into())
    }
}

/// Button 组件 Builder
///
/// 声明式按钮组件。通过链式方法配置点击行为、样式、状态等。
/// 子内容通过 `build(ctx, |ctx| { ... })` 传入（通常是 Text + Icon）。
///
/// # 示例
/// ```ignore
/// Button::new()
///     .style(ButtonStyle::Filled)
///     .on_click(|| count.update(|v| *v += 1))
///     .enabled(true)
///     .modifier(Modifier::new()
///         .size(200.0, 48.0)
///         .background(Color::BLUE, Shape::rounded(8.0))
///     )
///     .build(ctx, |ctx| {
///         Text::new("Increment").color(Color::WHITE).build(ctx);
///     });
/// ```
#[derive(Clone)]
pub struct Button {
    /// 点击回调
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// 是否启用
    enabled: bool,
    /// 按钮风格
    style: ButtonStyle,
    /// 颜色集（None = 从主题按 style 默认）
    colors: Option<ButtonColors>,
    /// 交互源（None = build 时内部 remember——对标 Compose 可选注入）
    interaction_source: Option<MutableInteractionSource>,
    /// Shadow elevation (`None` = the style's token default — see `Button::build`)
    elevation: Option<ButtonElevation>,
    /// 容器/边框/阴影形状（对标 material3 `Button(shape = ...)`——默认胶囊）
    shape: Shape,
    /// 内容 padding (start, top, end, bottom)——None = 按 style 默认；
    /// `SizeValue` 支持动态（动画 State/闭包——measure 期求值，只重测不重组）
    content_padding: Option<(Dimension, Dimension, Dimension, Dimension)>,
    /// 最小尺寸覆盖（None = ButtonDefaults MinWidth/MinHeight；支持动画）
    min_width: Option<Dimension>,
    min_height: Option<Dimension>,
    /// 边框（None = 按 style 默认——Outlined 有 1px 主题色边框，其余无）
    border: Option<ButtonBorder>,
    /// 尺寸变体（默认 Small）
    size_variant: ButtonSize,
    /// `true` when the caller named a size — only then do that size's tokens decide the horizontal padding.
    size_named: bool,
    /// 修饰符链（尺寸、颜色、形状等）
    modifier: Modifier,
}

impl Button {
    /// 创建新的 Button 组件
    pub fn new() -> Self {
        Button {
            on_click: None,
            enabled: true,
            style: ButtonStyle::default(),
            colors: None,
            interaction_source: None,
            elevation: None,
            shape: ButtonDefaults::shape(),
            content_padding: None,
            min_width: None,
            min_height: None,
            border: None,
            size_variant: ButtonSize::Small,
            size_named: false,
            modifier: Modifier::new(),
        }
    }

    /// 实心按钮（对标 material3 `Button`）——默认 Filled 样式
    pub fn filled() -> Self {
        Self::new()
    }

    /// 悬浮按钮（对标 material3 `ElevatedButton`）——Elevated 样式（
    /// SurfaceContainerLow 容器 + primary 内容）+ 默认阴影
    pub fn elevated() -> Self {
        Self::new()
            .style(ButtonStyle::Elevated)
            .elevation(ButtonElevation::elevated())
    }

    /// 柔和按钮（对标 material3 `FilledTonalButton`）——SecondaryContainer 色
    pub fn filled_tonal() -> Self {
        Self::new().style(ButtonStyle::Tonal)
    }

    /// 轮廓按钮（对标 material3 `OutlinedButton`）——透明底 + 1px 主题色边框
    pub fn outlined() -> Self {
        Self::new().style(ButtonStyle::Outlined)
    }

    /// 文本按钮（对标 material3 `TextButton`）——透明底、紧凑内边距
    pub fn text() -> Self {
        Self::new().style(ButtonStyle::Text)
    }

    /// 设置点击回调
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// 设置启用状态
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 设置按钮风格
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// 设置颜色集（覆盖主题默认——对标 material3 `Button(colors = ...)`）
    pub fn colors(mut self, colors: ButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 注入交互源（hoist——Button 的 press/hover/focus 状态发射到此源；
    /// 不传则内部 remember 一个）
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// 设置阴影高度（各状态取值见 [`ButtonElevation::for_state`]）
    pub fn elevation(mut self, elevation: ButtonElevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    /// 设置容器/边框/阴影形状（对标 material3 `Button(shape = ...)`——默认胶囊）
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// 设置内容 padding (start, top, end, bottom)——不设置则按 style 取
    /// [`ButtonDefaults::content_padding`]；每边支持动态 `SizeValue`
    /// （`&State<f32>` / `State<f32>` / 闭包——动画可作用于内边距）
    pub fn content_padding<A: Into<Dimension>, B: Into<Dimension>, C: Into<Dimension>, D: Into<Dimension>>(
        mut self,
        padding: (A, B, C, D),
    ) -> Self {
        self.content_padding = Some((
            padding.0.into(),
            padding.1.into(),
            padding.2.into(),
            padding.3.into(),
        ));
        self
    }

    /// 覆盖默认最小宽度（对标 material3 内部 `defaultMinSize(MinWidth, MinHeight)`——
    /// 默认 58/40，用户可经此或 `Modifier::min_width` 覆盖；支持动态 `SizeValue`）
    pub fn min_size<A: Into<Dimension>, B: Into<Dimension>>(
        mut self,
        min_width: A,
        min_height: B,
    ) -> Self {
        self.min_width = Some(min_width.into());
        self.min_height = Some(min_height.into());
        self
    }

    /// 设置边框（对标 material3 `Button(border = BorderStroke(...))`）——
    /// 显式传入后覆盖 style 默认（Outlined 的 1px 主题色边框），形状跟随 shape
    pub fn border(mut self, border: ButtonBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// 尺寸变体（XSmall/Small/Medium/Large/XLarge，默认 Small）
    ///
    /// Naming a size switches the horizontal padding to that size's token: `ButtonSmallTokens.LeadingSpace`
    /// is 16 dp, while the DEFAULT button (no `.size()` call) keeps `ButtonDefaults.ContentPadding`'s 24 dp,
    /// which comes from `BaselineButtonTokens.LeadingSpace` and is what M3's own default button uses. The two
    /// are different numbers for the same height, so the builder has to remember which one was asked for.
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size_variant = size;
        self.size_named = true;
        self
    }

    /// 设置修饰符链（追加到已有 modifier）
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 注册到组合树并执行子内容。
    /// 根据 style 自动从 WiniaTheme 读取默认颜色（用户 modifier 可覆盖）。
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let theme = crate::theme::WiniaTheme::colors();
        // Resolve BEFORE declaring parameters: the resolved set is itself a parameter. A styling that
        // changes at runtime must re-enter the group, or a Clean slot keeps the label style and the
        // border it built on the previous pass while only the background (a live closure) updates.
        let visual = button_visual_with_styles(&theme, self.style);
        let colors = self.colors.unwrap_or(visual.colors);
        // 参数暂存（参数相等跳过——style/enabled/解析出的颜色未变 → 容器 Skip）
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        ctx.changed(&colors);
        ctx.changed(&visual.border);
        // 交互源：外部注入或内部 remember（对标 Compose Button 的 interactionSource 参数）
        let interaction = self.interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        // 状态化取色（读取注册依赖——press/hover/focus 变化自动重组）
        let state = interaction.state(self.enabled);
        let container = colors.container_color_for(&state);
        let text_color = colors.content_color_for(&state);
        // 阴影高度：状态变化（hover/focus/press 进入与离开）用动画 State
        // 平滑过渡——悬停时阴影逐渐升高、移出后逐渐回落，不跳变。
        //
        // The spec is Compose's, and Compose keys it on the interaction PAIR (`ButtonElevation::spec_for`), so
        // the build remembers the interaction kind the last target came from in a backchannel — reading it
        // registers no dependency, and `(None, None)` (idle to idle) is Compose's snap.
        let last_kind: crate::runtime::state::Backchannel<
            Option<crate::animation::ElevationInteraction>,
        > = ctx.remember_backchannel(|| None);
        // The elevation is the caller's when there is one, otherwise the style's own token set — Compose always
        // has one (`ButtonDefaults.buttonElevation()` is 0/0/0/1/0 for a Filled button: flat at rest, 1 dp while
        // hovered), and `OutlinedButtonTokens`/`TextButtonTokens` declare none at all. winia stored `None` and
        // skipped the layer, so a Filled button's hover shadow never existed.
        let elevation = self.elevation.unwrap_or_else(|| match self.style {
            ButtonStyle::Filled => ButtonDefaults::button_elevation(),
            ButtonStyle::Tonal => ButtonDefaults::filled_tonal_button_elevation(),
            ButtonStyle::Elevated => ButtonDefaults::elevated_button_elevation(),
            ButtonStyle::Outlined | ButtonStyle::Text => ButtonElevation::none(),
        });
        let elevation_anim = if elevation.is_shadow_free() {
            None // every state is 0: no shadow to animate, and no layer to pay for
        } else {
            let target = elevation.for_state(&state);
            let to = state.elevation_interaction();
            let from = if state.enabled { last_kind.peek() } else { None };
            let spec = ButtonElevation::spec_for(from, to)
                .unwrap_or(crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()));
            last_kind.set(to);
            Some(ctx.animate_float_as_state(target, spec))
        };

        // 对标 material3 Button：内部 Row = defaultMinSize(58,40) + contentPadding，
        // 容器/边框/阴影统一使用 shape 参数（默认胶囊 CornerFull）。
        //
        // The shape MORPHS on press, as upstream's does: `ButtonShapes(shape, pressedShape)`
        // through `shapeByInteraction(shapes, pressed, spec)` (`material3/Button.kt:1922-1939`),
        // defaulting to the size's `ContainerShapeRound` (CornerFull) and its
        // `PressedContainerShape`. The spec is `MotionSchemeKeyTokens.DefaultEffects`, which
        // upstream comments is "intentional here to prevent any bounce in this component"
        // (`:248-252`) — the same rule the icon buttons follow, and the reason this is not a
        // spatial spring.
        let press_progress = ctx.animate_float_as_state(
            if state.pressed { 1.0 } else { 0.0 },
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default()),
        );
        let pressed_radius = self.size_variant.pressed_container_radius();
        // The press morph is a PAINT-time value: the closures read the animated progress with
        // `peek()`, so pressing no longer recomposes the button every frame. ONE carrier is handed to
        // the background, the border, the ripple and the shadow, so they all read the same t.
        let shape: crate::graphics::Dyn<Shape> = match self.shape {
            // A percent-50 corner can only be resolved against the box it paints into, so the
            // transition itself carries both ends: `PillToRounded { t: 0.0 }` IS the pill. The
            // two ENDS still return the plain shapes — a button that never pressed should carry
            // the very shape the caller asked for, not the transition resting at its start.
            Shape::Pill | Shape::Circle => {
                let base = self.shape;
                let p = press_progress.clone();
                crate::graphics::Dyn::Dynamic(std::sync::Arc::new(move || {
                    let t = p.peek();
                    if t <= 0.0 {
                        base
                    } else if t >= 1.0 {
                        Shape::rounded(pressed_radius)
                    } else {
                        Shape::PillToRounded {
                            corner_radius: pressed_radius,
                            t,
                        }
                    }
                }))
            }
            // A fixed radius interpolates directly.
            Shape::RoundedRect { corner_radius } => {
                let p = press_progress.clone();
                crate::graphics::Dyn::Dynamic(std::sync::Arc::new(move || {
                    Shape::rounded(corner_radius + (pressed_radius - corner_radius) * p.peek())
                }))
            }
            // Per-corner, top-only and rectangle shapes have nothing to interpolate.
            other => crate::graphics::Dyn::Static(other),
        };
        let (pad_s, pad_t, pad_e, pad_b) = self.content_padding.unwrap_or_else(|| {
            if self.size_named {
                // A named size takes that size's own tokens (`ButtonSmallTokens.LeadingSpace` = 16 for Small).
                ButtonDefaults::content_padding_for(self.size_variant, self.style)
            } else {
                // No size named: M3's default button, whose padding is the BASELINE 24 dp
                // (`BaselineButtonTokens.LeadingSpace`) and not the small size's 16.
                ButtonDefaults::content_padding(self.style)
            }
        });
        let min_w = self
            .min_width
            .clone()
            .unwrap_or_else(|| ButtonDefaults::min_width().into());
        let min_h = self
            .min_height
            .clone()
            .unwrap_or_else(|| ButtonDefaults::min_height_for(self.size_variant).into());

        let mut modifier = Modifier::new()
            .min_width(min_w)
            .min_height(min_h)
            .padding_sides(pad_s, pad_t, pad_e, pad_b);
        // 背景按 style（Filled/Tonal 有容器色，Outlined 无）；
        // The Text style's shape, carried without a clip:
        //
        // The Text style has neither a background nor a border to carry its shape, and the ripple plus the
        // focus ring both infer that shape from the chain (`render_pass1`'s focus-shape inference). It used a `clip` for that —
        // but a clip also CUTS everything drawn around the box, and the ring is stroked OUTSIDE the node's rect
        // (`render::draw_focus`). So a Text button lost its ring while the styles whose shape comes from a
        // Background or a Border kept theirs — visible side by side on the demo, where `Button::outlined()`
        // shows a ring and `Button::text()` showed none. A zero-width transparent border states the same shape
        // and clips nothing.
        modifier = match self.style {
            ButtonStyle::Filled | ButtonStyle::Elevated | ButtonStyle::Tonal => {
                modifier.background(container, shape.clone())
            }
            ButtonStyle::Outlined => modifier,
            ButtonStyle::Text => modifier.border(
                0.0,
                crate::graphics::Color::from_argb(0, 0, 0, 0),
                shape.clone(),
            ),
        };
        // 边框：显式 border 优先，否则 Outlined 默认 1px outline 色
        // （对标 material3 OutlinedButton = Button(border = BorderStroke(1.dp, OutlineVariant))）
        let border = self.border.or_else(|| {
            if self.style == ButtonStyle::Outlined {
                let mut b = ButtonBorder::outlined(&theme, self.enabled);
                // A styling may restyle the border too; without one this is the theme's colour.
                if let Some(color) = visual.border {
                    b.color = color;
                }
                b.width = self.size_variant.outline_width();
                Some(b)
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape.clone());
        }

        // 阴影渲染走 graphics_layer 动态闭包（shadow_elevation 每帧读取动画值——
        // 渲染期求值不触发重组；对标 Compose 层阴影语义；形状跟随 Button shape）
        if let Some(anim) = elevation_anim {
            let shape = shape.clone();
            modifier = modifier.graphics_layer(move || crate::graphics::GraphicsLayerParams {
                shadow_elevation: anim.peek(),
                shadow_shape: Some(shape.eval()),
                ..Default::default()
            });
        }

        // Accessibility: this is a button, and whether it is enabled is part of what it reports.
        // Declared before the caller's modifier is appended, so a caller's own declaration merges
        // over this one per field.
        modifier = modifier.semantics(
            crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::Button)
                .merge_descendants(true)
                .state(crate::semantics::SemanticsState::new().enabled(self.enabled)),
        );

        // 追加用户 modifier（在外层，可覆盖默认样式）
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_click) = &self.on_click {
                let cb = on_click.clone();
                // clickable（press/focus/hover 交互）+ 水波纹（对标 Compose
                // clickable 默认 indication=ripple；颜色用内容色）
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb())
                    // 显式传容器 shape——Outlined/Text 无 Background 元素，
                    // 若让波纹自行推断会回退成矩形裁剪（超出胶囊范围）
                    .ripple_with_shape(&interaction, text_color, true, shape);
            }
        }

        // The touch target, as its OWN node around the visual box: Compose gets it from the clickable
        // `Surface`'s `Box`, which applies `Modifier.minimumInteractiveComponentSize()`
        // (`material3/Surface.kt:219`; the node is at `material3/InteractiveComponentSize.kt:98-140`), so every
        // M3 button reserves at least 48 dp on both axes with its 40 dp visual box centred inside — which is
        // also why a row of buttons sits 48 apart. In ONE chain winia decides the size across the whole chain,
        // so `min_height(40)` clamps the 48 away (measured: 40 with the modifier on either side of it), hence
        // the wrapper. It wraps the button's own scope so the visual chain keeps its box.
        //
        // Deviation, recorded: the wrapper sits OUTSIDE the caller's modifier, so a
        // `Button::modifier(Modifier::new().size(40.0, 40.0))` still measures 48. In Compose the caller's
        // modifier is the outer one (`Surface(modifier = modifier…)`), so its explicit size wins and the touch
        // target cannot grow past it. winia's modifier chain cannot express "the caller's size clamps the
        // smaller node inside", which is why the two Node trees would be needed to match exactly.
        // `propagate_min_constraints`: `Surface` is a `Box(propagateMinConstraints = true)`
        // (`material3/Surface.kt:219`), so a parent that FORCES a width on the button gets a
        // painted box that width — the touch target is not the only thing that grows. Without it
        // a ButtonGroup item's press growth widened this 48 dp box while the visual box inside
        // stayed at its content width, and the difference showed up as a gap on each side.
        crate::layout::components::build_container(
            ctx,
            Modifier::new().minimum_interactive_component_size(),
            BoxLayout::new()
                .alignment(crate::layout::Alignment::Center)
                .propagate_min_constraints(true),
            |ctx| {
        // content 闭包自动成为组合 scope（与 Column 一致）
        match ctx.start_restartable_group(key, modifier, BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
            crate::runtime::composer::GroupStatus::Skip => {}
            crate::runtime::composer::GroupStatus::Enter => {
                // 对标 M3：Button 内容 = 居中 Row（图标/文字并排）；
                // 内容色下传（LocalContentColor 等价物）——Icon tint Auto
                // 取按钮内容色（如 Filled 内图标自动 on_primary）
                crate::theme::WiniaTheme::with_content_color(text_color, ctx, |ctx| {
                    let mut text_style = crate::theme::WiniaTheme::typography().label_large;
                    text_style.color = Some(text_color);
                    crate::components::text::ProvideTextStyle(
                        text_style,
                        ctx,
                        |ctx| {
                            crate::layout::Row::new()
                                .alignment(crate::layout::Alignment::Center)
                                .spacing(self.size_variant.icon_label_space())
                                .build(ctx, content);
                        },
                    );
                });
            }
        }
        // 焦点环颜色：主题 primary（组合期捕获——渲染期 CompositionLocal 已退出）
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
            },
        );
    }

    // ── Getters（测试用）──
    pub fn get_enabled(&self) -> bool { self.enabled }
    pub fn get_style(&self) -> ButtonStyle { self.style }
    pub fn get_colors(&self) -> Option<ButtonColors> { self.colors }
    pub fn get_elevation(&self) -> Option<ButtonElevation> { self.elevation }
    pub fn get_shape(&self) -> Shape { self.shape }
    pub fn get_content_padding(&self) -> Option<&(Dimension, Dimension, Dimension, Dimension)> {
        self.content_padding.as_ref()
    }
    pub fn get_min_size(&self) -> (Option<Dimension>, Option<Dimension>) {
        (self.min_width.clone(), self.min_height.clone())
    }
    pub fn get_border(&self) -> Option<ButtonBorder> { self.border }
    pub fn get_size(&self) -> ButtonSize { self.size_variant }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Button")
            .field("enabled", &self.enabled)
            .field("style", &self.style)
            .field("modifier", &self.modifier)
            .field("on_click", &self.on_click.as_ref().map(|_| "<fn>"))
            .finish()
    }
}


#[cfg(test)]
mod tests;
