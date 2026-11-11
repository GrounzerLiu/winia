//! Button — a clickable button.
//!
//! Usage:
//! ```ignore
//! Button::new()
//!     .on_click(|| println!("clicked"))
//!     .modifier(Modifier::new().size(200.0, 48.0).background(Color::BLUE))
//!     .build(ctx, |ctx| {
//!         Text::new("Click me").color(Color::WHITE).build(ctx);
//!     });
//! ```

use crate::composable;
use crate::graphics::Shape;
use crate::input::interaction::{ComponentState, MutableInteractionSource};
use crate::layout::BoxLayout;
use crate::layout::Dimension;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
use std::fmt;
use std::sync::Arc;

/// The button variants — M3's five composables, differing only in defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonStyle {
    /// Filled: the high-emphasis default (`Button`).
    #[default]
    Filled,
    /// Elevated: `SurfaceContainerLow` container with `primary` content (`ElevatedButton`).
    Elevated,
    /// Outlined: transparent with a 1 dp border (`OutlinedButton`).
    Outlined,
    /// Text: transparent, no border, tight padding (`TextButton`).
    Text,
    /// Tonal: `SecondaryContainer` fill (`FilledTonalButton`).
    Tonal,
}

/// The size variants — material3's `ButtonTokens` family (v0_11_0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    XSmall,
    Small,
    Medium,
    Large,
    XLarge,
}

impl ButtonSize {
    /// `ContainerHeight`: 32/40/56/96/136.
    pub fn container_height(self) -> f32 {
        match self {
            ButtonSize::XSmall => 32.0,
            ButtonSize::Small => 40.0,
            ButtonSize::Medium => 56.0,
            ButtonSize::Large => 96.0,
            ButtonSize::XLarge => 136.0,
        }
    }

    /// Recommended icon size, `IconSize`: 20/20/24/32/40.
    pub fn icon_size(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small => 20.0,
            ButtonSize::Medium => 24.0,
            ButtonSize::Large => 32.0,
            ButtonSize::XLarge => 40.0,
        }
    }

    /// Gap between an icon and its label, `IconLabelSpace`: 8/8/8/12/16.
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

    /// Vertical padding per size: 6/10/16/32/48.
    ///
    /// Compose derives it from the height rather than storing one number
    /// (`smallVerticalPadding` 10, `MediumVerticalPadding` 16, `LargeVerticalPadding` 32,
    /// `ExtraSmallContentPadding` 6, `ExtraLargeContentPadding` 48 — `Button.kt:958-1010`,
    /// `:1652-1663`), and each value leaves exactly that size's icon height in the box:
    /// 6+6+20=32, 10+10+20=40, 16+16+24=56, 32+32+32=96, 48+48+40=136.
    ///
    /// One shared 8 could not work for the smallest size: XSmall's 32 dp container had only 16 dp
    /// left after 8+8, less than the 20 dp a label or an icon needs, so the box grew to 36 and the
    /// token stopped being the height. Measured before this existed, at 400x400: XSmall 36.0 against
    /// `ButtonXSmallTokens.ContainerHeight` 32.0, every other size exact.
    pub fn vertical_padding(self) -> f32 {
        match self {
            ButtonSize::XSmall => 6.0,
            ButtonSize::Small => 10.0,
            ButtonSize::Medium => 16.0,
            ButtonSize::Large => 32.0,
            ButtonSize::XLarge => 48.0,
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

    /// Outlined border width, `OutlinedOutlineWidth`: 1/1/1/2/3.
    pub fn outline_width(self) -> f32 {
        match self {
            ButtonSize::XSmall | ButtonSize::Small | ButtonSize::Medium => 1.0,
            ButtonSize::Large => 2.0,
            ButtonSize::XLarge => 3.0,
        }
    }
}

/// A button's colour set — material3's `ButtonColors`. Container and content each carry an
/// enabled and a disabled value; `container_color(enabled)` / `content_color(enabled)` resolve by
/// state. Disabled follows the M3 tokens: container `OnSurface` at 10% (12% for Tonal), content
/// `OnSurfaceVariant` at 38%.
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
        Self {
            container,
            content,
            disabled_container,
            disabled_content,
        }
    }

    /// The container colour for an enabled/disabled pair.
    pub fn container_color(&self, enabled: bool) -> crate::graphics::Color {
        self.container_color_for(&ComponentState {
            enabled,
            ..ComponentState::idle()
        })
    }

    /// The content (label) colour for an enabled/disabled pair.
    pub fn content_color(&self, enabled: bool) -> crate::graphics::Color {
        self.content_color_for(&ComponentState {
            enabled,
            ..ComponentState::idle()
        })
    }

    /// The state-aware container colour. It distinguishes enabled from disabled ONLY, which is
    /// material3's `ButtonColors` contract: hover, focus and press are drawn by the ripple state
    /// layer instead, so the two never stack.
    pub fn container_color_for(&self, state: &ComponentState) -> crate::graphics::Color {
        if !state.enabled {
            self.disabled_container
        } else {
            self.container
        }
    }

    /// The state-aware content colour — enabled/disabled only, as above.
    pub fn content_color_for(&self, state: &ComponentState) -> crate::graphics::Color {
        if state.enabled {
            self.content
        } else {
            self.disabled_content
        }
    }

    /// The default colours for a style, derived from the theme (material3's
    /// `ButtonDefaults.buttonColors()`).
    pub fn from_theme(theme: &crate::theme::ThemeColors, style: ButtonStyle) -> Self {
        use crate::graphics::Color;
        let (container, content) = match style {
            ButtonStyle::Filled => (theme.primary, theme.on_primary),
            ButtonStyle::Elevated => (theme.surface_container_low, theme.primary),
            ButtonStyle::Tonal => (theme.secondary_container, theme.on_secondary_container),
            ButtonStyle::Outlined => (Color::from_argb(0, 0, 0, 0), theme.on_surface_variant),
            ButtonStyle::Text => (Color::from_argb(0, 0, 0, 0), theme.primary),
        };
        // The M3 tokens: Filled/Elevated disable the container to OnSurface at 10%, Tonal at 12%,
        // Outlined/Text keep it transparent; every disabled label is OnSurfaceVariant at 38%.
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        let (disabled_container, disabled_content) = match style {
            ButtonStyle::Filled | ButtonStyle::Elevated => (
                alpha(theme.on_surface, 0.10),
                alpha(theme.on_surface_variant, 0.38),
            ),
            ButtonStyle::Tonal => (alpha(theme.on_surface, 0.12), alpha(theme.on_surface, 0.38)),
            ButtonStyle::Outlined => (transparent, alpha(theme.on_surface_variant, 0.38)),
            ButtonStyle::Text => (transparent, alpha(theme.on_surface_variant, 0.38)),
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
        Self {
            default,
            pressed,
            focused,
            hovered,
            disabled,
        }
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

/// A button's border — material3's `BorderStroke`: width plus colour. The SHAPE comes from
/// [`Button::shape`], because the border is drawn on the container's edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonBorder {
    pub width: f32,
    pub color: crate::graphics::Color,
}

impl ButtonBorder {
    pub fn new(width: f32, color: crate::graphics::Color) -> Self {
        Self { width, color }
    }

    /// The OutlinedButton default border: 1 dp of `OutlineVariant`, or `OutlineVariant` at
    /// `DisabledContainerOpacity` (0.1) when disabled.
    fn outlined(theme: &crate::theme::ThemeColors, enabled: bool) -> Self {
        // OutlinedButtonTokens.OutlineColor = OutlineVariant
        let c = theme.outline_variant;
        let color = if enabled {
            c
        } else {
            crate::graphics::Color::from_argb((c.a as f32 * 0.10) as u8, c.r, c.g, c.b)
        };
        Self::new(1.0, color)
    }
}

/// The button defaults — material3's `ButtonDefaults`, over the M3 small-button tokens.
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
            theme: *theme,
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
    /// The default shape: the pill (`ButtonSmallTokens.ContainerShapeRound` = CornerFull).
    pub fn shape() -> Shape {
        Shape::pill()
    }

    /// The default button colours (material3's `ButtonDefaults.buttonColors()`): container and
    /// content derived from the theme's roles for this style.
    pub fn button_colors(theme: &crate::theme::ThemeColors, style: ButtonStyle) -> ButtonColors {
        ButtonColors::from_theme(theme, style)
    }

    /// `ButtonDefaults.buttonElevation()` — `FilledButtonTokens`: flat at rest and while pressed or focused,
    /// `Level1` (1 dp) while hovered.
    pub fn button_elevation() -> ButtonElevation {
        ButtonElevation::default_elevation()
    }

    /// The ElevatedButton default shadow (`ButtonDefaults.elevatedButtonElevation()`).
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

    /// The default minimum width (`ButtonDefaults.MinWidth`, 58 dp).
    pub fn min_width() -> f32 {
        58.0
    }

    /// The default minimum height (`ButtonDefaults.MinHeight`, 40 dp — `ContainerHeight`).
    pub fn min_height() -> f32 {
        ButtonSize::Small.container_height()
    }

    pub fn min_height_for(size: ButtonSize) -> f32 {
        size.container_height()
    }

    /// The default content padding `(start, top, end, bottom)`: 24/8/24/8 for the button family,
    /// 12/8/12/8 for Text (`ButtonDefaults.ContentPadding` / `TextButtonContentPadding`). Every
    /// edge is a `Dimension`, so it can animate.
    pub fn content_padding(style: ButtonStyle) -> (Dimension, Dimension, Dimension, Dimension) {
        match style {
            ButtonStyle::Text => (12.0.into(), 8.0.into(), 12.0.into(), 8.0.into()),
            _ => (24.0.into(), 8.0.into(), 24.0.into(), 8.0.into()),
        }
    }

    /// The icon/label gap (`ButtonDefaults.IconSpacing` = `ButtonSmallTokens.IconLabelSpace`,
    /// 8 dp). M3 leaves this to the caller's Row; winia applies it inside the content Row. That
    /// spacing is not reachable from outside, so a caller wanting a different gap pads the Icon/Text
    /// or lays the content out itself.
    pub fn icon_spacing() -> f32 {
        ButtonSize::Small.icon_label_space()
    }

    pub fn icon_spacing_for(size: ButtonSize) -> f32 {
        size.icon_label_space()
    }

    /// The content padding for a button WITH an icon (`ButtonDefaults.ButtonWithIconContentPadding`):
    /// 16 start, 24 end — the icon sits tight against the edge while the label keeps its 24 dp.
    pub fn button_with_icon_content_padding() -> (Dimension, Dimension, Dimension, Dimension) {
        (16.0.into(), 8.0.into(), 24.0.into(), 8.0.into())
    }

    /// The content padding for a Text button WITH an icon
    /// (`ButtonDefaults.TextButtonWithIconContentPadding`): 12 start, 16 end.
    pub fn text_button_with_icon_content_padding() -> (Dimension, Dimension, Dimension, Dimension) {
        (12.0.into(), 8.0.into(), 16.0.into(), 8.0.into())
    }

    /// The default content padding for a size: horizontal from that size's `Leading`/`Trailing`
    /// token, vertical from its own token; Text keeps a fixed 12/8/12/8.
    ///
    /// The vertical value is per size (`ButtonSize::vertical_padding`) because it is what keeps the
    /// box at `ContainerHeight`: one shared 8 gave XSmall's 32 dp container only 16 dp of content
    /// room, less than the 20 dp a label needs, and the box measured 36 instead.
    pub fn content_padding_for(
        size: ButtonSize,
        style: ButtonStyle,
    ) -> (Dimension, Dimension, Dimension, Dimension) {
        if style == ButtonStyle::Text {
            return (12.0.into(), 8.0.into(), 12.0.into(), 8.0.into());
        }
        let h = size.horizontal_padding();
        let v = size.vertical_padding();
        (h.into(), v.into(), h.into(), v.into())
    }
}

/// The `Button` builder.
///
/// A declarative button: the callback, style and enabled state are chainable methods, and the
/// content is passed to `build(ctx, |ctx| { ... })` (usually a Text plus an Icon).
///
/// # Example
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
    /// The click callback.
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Whether the button responds to input.
    enabled: bool,
    /// The visual variant.
    style: ButtonStyle,
    /// The colour set; `None` derives it from the theme for this style.
    colors: Option<ButtonColors>,
    /// The interaction source; `None` remembers one inside `build` — Compose's optional injection.
    interaction_source: Option<MutableInteractionSource>,
    /// Shadow elevation (`None` = the style's token default — see `Button::build`)
    elevation: Option<ButtonElevation>,
    /// The container/border/shadow shape (material3's `Button(shape = ...)`; the pill by default).
    shape: Shape,
    /// Content padding `(start, top, end, bottom)`; `None` takes the style's default. Each edge is a
    /// `Dimension`, so an animating value is evaluated at measure time — a re-measure, never a
    /// recompose.
    content_padding: Option<(Dimension, Dimension, Dimension, Dimension)>,
    /// Minimum-size overrides; `None` takes `ButtonDefaults`' `MinWidth`/`MinHeight`. Both animate.
    min_width: Option<Dimension>,
    min_height: Option<Dimension>,
    /// The border; `None` takes the style's default — Outlined has a 1 dp border, the rest none.
    border: Option<ButtonBorder>,
    /// The size variant (Small by default).
    size_variant: ButtonSize,
    /// `true` when the caller named a size — only then do that size's tokens decide the horizontal padding.
    size_named: bool,
    /// The caller's own modifier chain (size, colour, shape and so on).
    modifier: Modifier,
}

impl Button {
    /// A new button, with the M3 defaults (Filled).
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

    /// A filled button (material3's `Button`) — the Filled style.
    pub fn filled() -> Self {
        Self::new()
    }

    /// An elevated button (material3's `ElevatedButton`): the Elevated style — a
    /// `SurfaceContainerLow` container with `primary` content — plus its default shadow.
    pub fn elevated() -> Self {
        Self::new()
            .style(ButtonStyle::Elevated)
            .elevation(ButtonElevation::elevated())
    }

    /// A tonal button (material3's `FilledTonalButton`) — `SecondaryContainer`.
    pub fn filled_tonal() -> Self {
        Self::new().style(ButtonStyle::Tonal)
    }

    /// An outlined button (material3's `OutlinedButton`) — transparent, with a 1 dp border.
    pub fn outlined() -> Self {
        Self::new().style(ButtonStyle::Outlined)
    }

    /// A text button (material3's `TextButton`) — transparent and tightly padded.
    pub fn text() -> Self {
        Self::new().style(ButtonStyle::Text)
    }

    /// Set the click callback.
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// Set the caller's modifier chain, appended to the one already held.
    ///
    /// Immediately after the action, as Compose orders its parameters (`docs/component-authoring.md`
    /// section 2.1): a caller skimming the builder finds it where Compose puts it. Builder methods are
    /// chained, so the position is a reading convention rather than something callers depend on.
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Set whether the button responds to input.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the visual variant.
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    /// Set the colour set, overriding the theme's default (material3's `Button(colors = ...)`).
    pub fn colors(mut self, colors: ButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Hoist the interaction source: the button's press/hover/focus states are emitted into it. Left
    /// out, the button remembers one of its own.
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// Set the shadow elevation — see [`ButtonElevation::for_state`] for the per-state values.
    pub fn elevation(mut self, elevation: ButtonElevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    /// Set the container/border/shadow shape (material3's `Button(shape = ...)`; the pill by default).
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// Set the content padding `(start, top, end, bottom)`; [`ButtonDefaults::content_padding`] is
    /// the default. Each edge is a `Dimension`, so `&State<f32>` / `State<f32>` / a closure all work —
    /// an animation there costs a re-measure, not a recompose.
    pub fn content_padding<
        A: Into<Dimension>,
        B: Into<Dimension>,
        C: Into<Dimension>,
        D: Into<Dimension>,
    >(
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

    /// Override the default minimum size (material3's internal `defaultMinSize(MinWidth, MinHeight)`
    /// — 58/40 by default; a caller can override it here or with `Modifier::min_width`). Both edges
    /// are `Dimension`s, so they animate.
    pub fn min_size<A: Into<Dimension>, B: Into<Dimension>>(
        mut self,
        min_width: A,
        min_height: B,
    ) -> Self {
        self.min_width = Some(min_width.into());
        self.min_height = Some(min_height.into());
        self
    }

    /// Set the border (material3's `Button(border = BorderStroke(...))`). An explicit border
    /// overrides the style's default — the Outlined variant's 1 dp one — and follows `shape`.
    pub fn border(mut self, border: ButtonBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// The size variant (XSmall/Small/Medium/Large/XLarge; Small by default).
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

    /// Register the button in the composition tree and run its content.
    /// The default colours come from `WiniaTheme` by style; a caller's modifier can override them.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let key = ctx.next_key();
        let theme = crate::theme::WiniaTheme::colors();
        // Resolve BEFORE declaring parameters: the resolved set is itself a parameter. A styling that
        // changes at runtime must re-enter the group, or a Clean slot keeps the label style and the
        // border it built on the previous pass while only the background (a live closure) updates.
        let visual = button_visual_with_styles(&theme, self.style);
        let colors = self.colors.unwrap_or(visual.colors);
        // Parameter staging: equal parameters Skip the container — style, enabled and the resolved
        // colours unchanged means the content does not need to run again.
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        ctx.changed(&colors);
        ctx.changed(&visual.border);
        // `size_variant` reaches the content: the inner Row's spacing is
        // `self.size_variant.icon_label_space()` (section 3.2 - a parameter the CONTENT reads).
        //
        // Today this cannot be observed: the height is derived from the same field
        // (`min_height_for(size_variant)`, and all five heights differ), so the modifier chain always
        // changes with it and the group Enters anyway. That is an accident of the token set, not a
        // guarantee - declaring it is what makes the dependency real rather than incidental.
        ctx.changed(&self.size_variant);
        // The interaction source: injected from outside, or remembered here (Compose's
        // `interactionSource` parameter).
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(MutableInteractionSource::new).get());
        // State-aware colours: reading them registers the dependency, so a press/hover/focus change
        // recomposes the button on its own.
        let state = interaction.state(self.enabled);
        let container = colors.container_color_for(&state);
        let text_color = colors.content_color_for(&state);
        // The shadow animates on every interaction change, entering and leaving, so hovering raises it
        // and moving away lowers it rather than jumping.
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
            let from = if state.enabled {
                last_kind.peek()
            } else {
                None
            };
            let spec = ButtonElevation::spec_for(from, to).unwrap_or(
                crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()),
            );
            last_kind.set(to);
            Some(ctx.animate_float_as_state(target, spec))
        };

        // material3's Button: the inner Row is `defaultMinSize(58, 40)` plus the content padding, and
        // the container/border/shadow all take the same `shape` (the pill, CornerFull, by default).
        //
        // The shape MORPHS on press, as upstream's does: `ButtonShapes(shape, pressedShape)`
        // through `shapeByInteraction(shapes, pressed, spec)` (`material3/Button.kt:1922-1939`),
        // defaulting to the size's `ContainerShapeRound` (CornerFull) and its
        // `PressedContainerShape`. The spec is `MotionSchemeKeyTokens.DefaultEffects`, which
        // upstream comments is "intentional here to prevent any bounce in this component"
        // (`:248-252`) — the same rule the icon buttons follow, and the reason this is not a
        // spatial spring.
        let press_progress =
            ctx.animate_float_as_state(
                if state.pressed { 1.0 } else { 0.0 },
                crate::animation::AnimationSpec::Spring(
                    crate::animation::SpringSpec::effects_default(),
                ),
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
        // The background follows the style: Filled/Tonal paint a container, Outlined does not.
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
        // The border: an explicit one wins, otherwise Outlined takes the 1 dp `outline` colour
        // (material3's `OutlinedButton = Button(border = BorderStroke(1.dp, OutlineVariant))`).
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

        // The shadow renders through a `graphics_layer` closure: `shadow_elevation` is read per frame
        // without triggering a recompose — Compose's layer-shadow semantics. The shape follows
        // `Button::shape`.
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

        // Append the caller's modifier — outermost, so it can override the default styling.
        modifier = modifier.then(self.modifier);

        if self.enabled
            && let Some(on_click) = &self.on_click
        {
            let cb = on_click.clone();
            // `clickable` (press/focus/hover interaction) plus the ripple — Compose's default
            // indication, tinted with the content colour.
            modifier = modifier
                .clickable_with_source(&interaction, move || cb())
                // The container shape is passed explicitly: Outlined/Text have no Background
                // element, and letting the ripple infer one falls back to a rectangular clip.
                .ripple_with_shape(&interaction, text_color, true, shape);
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
                // The content closure becomes its own composition scope, as a Column's does.
                match ctx.start_restartable_group(
                    key,
                    modifier,
                    BoxLayout::new().alignment(crate::layout::Alignment::Center),
                ) {
                    crate::runtime::composer::GroupStatus::Skip => {}
                    crate::runtime::composer::GroupStatus::Enter => {
                        // material3's content: a centred Row (icon and label side by side), with the
                        // content colour handed down — the `LocalContentColor` equivalent, so an
                        // `Icon` with `tint(Auto)` picks up the button's colour (inside a Filled
                        // button it becomes `on_primary`).
                        crate::theme::WiniaTheme::with_content_color(text_color, ctx, |ctx| {
                            let mut text_style = crate::theme::WiniaTheme::typography().label_large;
                            text_style.color = Some(text_color);
                            crate::components::text::ProvideTextStyle(text_style, ctx, |ctx| {
                                crate::layout::Row::new()
                                    .alignment(crate::layout::Alignment::Center)
                                    .spacing(self.size_variant.icon_label_space())
                                    // The caller's content is a `FnOnce(&mut ComposeCtx)`; forward the
                                    // row's scope through its `Deref`.
                                    .build(ctx, |scope| content(scope));
                            });
                        });
                    }
                }
                // The focus ring colour: the theme's primary, captured during composition because
                // the render phase can no longer read the CompositionLocal.
                ctx.set_current_node_focus_color(theme.primary);
                ctx.end_restartable_group();
            },
        );
    }

    // ── Getters (for tests) ──
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_style(&self) -> ButtonStyle {
        self.style
    }
    pub fn get_colors(&self) -> Option<ButtonColors> {
        self.colors
    }
    pub fn get_elevation(&self) -> Option<ButtonElevation> {
        self.elevation
    }
    pub fn get_shape(&self) -> Shape {
        self.shape
    }
    pub fn get_content_padding(&self) -> Option<&(Dimension, Dimension, Dimension, Dimension)> {
        self.content_padding.as_ref()
    }
    pub fn get_min_size(&self) -> (Option<Dimension>, Option<Dimension>) {
        (self.min_width.clone(), self.min_height.clone())
    }
    pub fn get_border(&self) -> Option<ButtonBorder> {
        self.border
    }
    pub fn get_size(&self) -> ButtonSize {
        self.size_variant
    }
    pub fn get_modifier(&self) -> &Modifier {
        &self.modifier
    }
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
