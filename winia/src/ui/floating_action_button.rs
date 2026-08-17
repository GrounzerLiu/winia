//! Material 3 Floating Action Button components.
//!
//! This module implements the icon-only FAB family. Extended FAB is intentionally
//! left for a separate component because its text layout and expand/collapse
//! semantics are different from a fixed-size FAB.

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::interaction::{ComponentState, MutableInteractionSource};
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// FAB size variants from the Material 3 token families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingActionButtonSize {
    /// 40dp. Supported for compatibility, but no longer the recommended size.
    Small,
    /// 56dp regular FAB.
    Regular,
    /// 80dp expressive FAB.
    Medium,
    /// 96dp large FAB.
    Large,
}

impl FloatingActionButtonSize {
    /// The outer container size in dp.
    pub fn container_size(self) -> f32 {
        match self {
            Self::Small => FAB_SMALL_SIZE,
            Self::Regular => FAB_REGULAR_SIZE,
            Self::Medium => FAB_MEDIUM_SIZE,
            Self::Large => FAB_LARGE_SIZE,
        }
    }

    /// Recommended icon size in dp.
    pub fn icon_size(self) -> f32 {
        match self {
            Self::Small | Self::Regular => FAB_ICON_SIZE,
            Self::Medium => FAB_MEDIUM_ICON_SIZE,
            Self::Large => FAB_LARGE_ICON_SIZE,
        }
    }
}

pub const FAB_SMALL_SIZE: f32 = 40.0;
pub const FAB_REGULAR_SIZE: f32 = 56.0;
pub const FAB_MEDIUM_SIZE: f32 = 80.0;
pub const FAB_LARGE_SIZE: f32 = 96.0;
pub const FAB_ICON_SIZE: f32 = 24.0;
pub const FAB_MEDIUM_ICON_SIZE: f32 = 28.0;
pub const FAB_LARGE_ICON_SIZE: f32 = 32.0;

/// Container/content colors for a FAB, including disabled colors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatingActionButtonColors {
    pub container: Color,
    pub content: Color,
    pub disabled_container: Color,
    pub disabled_content: Color,
}

impl FloatingActionButtonColors {
    pub fn new(
        container: Color,
        content: Color,
        disabled_container: Color,
        disabled_content: Color,
    ) -> Self {
        Self {
            container,
            content,
            disabled_container,
            disabled_content,
        }
    }

    pub fn container_color(&self, enabled: bool) -> Color {
        if enabled {
            self.container
        } else {
            self.disabled_container
        }
    }

    pub fn content_color(&self, enabled: bool) -> Color {
        if enabled {
            self.content
        } else {
            self.disabled_content
        }
    }

    /// Material 3 uses the icon/content color for the FAB state layer and Ripple.
    pub fn ripple_color(&self, enabled: bool) -> Color {
        self.content_color(enabled)
    }

    /// Builds a color set from a container/content pair using FAB disabled tokens.
    pub fn from_pair(theme: &ThemeColors, container: Color, content: Color) -> Self {
        let disabled_container = Color::from_argb(
            (theme.on_surface.a as f32 * 0.12) as u8,
            theme.on_surface.r,
            theme.on_surface.g,
            theme.on_surface.b,
        );
        let disabled_content = Color::from_argb(
            (theme.on_surface.a as f32 * 0.38) as u8,
            theme.on_surface.r,
            theme.on_surface.g,
            theme.on_surface.b,
        );
        Self::new(container, content, disabled_container, disabled_content)
    }

    pub fn from_theme(theme: &ThemeColors) -> Self {
        FloatingActionButtonDefaults::colors(theme)
    }
}

/// Elevation values selected from the current interaction state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatingActionButtonElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub disabled: f32,
}

impl FloatingActionButtonElevation {
    pub fn new(default: f32, pressed: f32, focused: f32, hovered: f32, disabled: f32) -> Self {
        Self {
            default,
            pressed,
            focused,
            hovered,
            disabled,
        }
    }

    /// Material 3 FAB elevation: 6dp at rest, 12dp while pressed, and 8dp
    /// while focused or hovered.
    pub fn default_elevation() -> Self {
        Self::new(6.0, 12.0, 8.0, 8.0, 0.0)
    }

    /// Lowered elevation used when a FAB is attached to another surface.
    pub fn lowered() -> Self {
        Self::new(1.0, 1.0, 1.0, 2.0, 0.0)
    }

    /// Zero elevation used by bottom app bars.
    pub fn bottom_app_bar() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0)
    }

    pub fn for_state(&self, state: &ComponentState) -> f32 {
        if !state.enabled {
            self.disabled
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

impl Default for FloatingActionButtonElevation {
    fn default() -> Self {
        Self::default_elevation()
    }
}

/// FAB defaults and Material 3 color mappings.
pub struct FloatingActionButtonDefaults;

impl FloatingActionButtonDefaults {
    /// Default rounded-rectangle shape approximations for the M3 shape tokens:
    /// CornerMedium, CornerLarge, LargeIncreased, and CornerExtraLarge.
    pub fn shape(size: FloatingActionButtonSize) -> Shape {
        match size {
            FloatingActionButtonSize::Small => Shape::rounded(12.0),
            FloatingActionButtonSize::Regular => Shape::rounded(16.0),
            FloatingActionButtonSize::Medium => Shape::rounded(20.0),
            FloatingActionButtonSize::Large => Shape::rounded(28.0),
        }
    }

    /// Primary-container FAB colors, the default mapping.
    pub fn colors(theme: &ThemeColors) -> FloatingActionButtonColors {
        Self::from_pair(theme, theme.primary_container, theme.on_primary_container)
    }

    pub fn primary_colors(theme: &ThemeColors) -> FloatingActionButtonColors {
        Self::from_pair(theme, theme.primary, theme.on_primary)
    }

    pub fn secondary_colors(theme: &ThemeColors) -> FloatingActionButtonColors {
        Self::from_pair(theme, theme.secondary, theme.on_secondary)
    }

    pub fn tertiary_colors(theme: &ThemeColors) -> FloatingActionButtonColors {
        Self::from_pair(theme, theme.tertiary, theme.on_tertiary)
    }

    pub fn from_pair(
        theme: &ThemeColors,
        container: Color,
        content: Color,
    ) -> FloatingActionButtonColors {
        FloatingActionButtonColors::from_pair(theme, container, content)
    }

    pub fn elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation::default_elevation()
    }

    pub fn lowered_elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation::lowered()
    }

    pub fn bottom_app_bar_elevation() -> FloatingActionButtonElevation {
        FloatingActionButtonElevation::bottom_app_bar()
    }
}

/// A fixed-size, icon-oriented Material 3 floating action button.
#[derive(Clone)]
pub struct FloatingActionButton {
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<FloatingActionButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    elevation: Option<FloatingActionButtonElevation>,
    size: FloatingActionButtonSize,
    shape: Option<Shape>,
    modifier: Modifier,
}

impl FloatingActionButton {
    /// Creates a regular 56dp FAB using primary-container colors.
    pub fn new() -> Self {
        Self {
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            elevation: Some(FloatingActionButtonDefaults::elevation()),
            size: FloatingActionButtonSize::Regular,
            shape: None,
            modifier: Modifier::new(),
        }
    }

    pub fn small() -> Self {
        Self::new().size(FloatingActionButtonSize::Small)
    }
    pub fn medium() -> Self {
        Self::new().size(FloatingActionButtonSize::Medium)
    }
    pub fn large() -> Self {
        Self::new().size(FloatingActionButtonSize::Large)
    }

    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: FloatingActionButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn elevation(mut self, elevation: FloatingActionButtonElevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    pub fn size(mut self, size: FloatingActionButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.size);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        ctx.changed(&self.shape);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| FloatingActionButtonDefaults::colors(&theme));
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let state = interaction.state(self.enabled);
        let container = colors.container_color(self.enabled);
        let content_color = colors.content_color(self.enabled);
        let ripple_color = colors.ripple_color(self.enabled);
        let shape = self
            .shape
            .unwrap_or_else(|| FloatingActionButtonDefaults::shape(self.size));
        let size = self.size.container_size();

        let elevation_anim = self.elevation.and_then(|elevation| {
            if elevation.default <= 0.0
                && elevation.pressed <= 0.0
                && elevation.focused <= 0.0
                && elevation.hovered <= 0.0
                && elevation.disabled <= 0.0
            {
                None
            } else {
                Some(ctx.animate_float_as_state(
                    elevation.for_state(&state),
                    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(180),
                        crate::animation::interpolator::EaseOutCubic::new(),
                    )),
                ))
            }
        });

        let mut modifier = Modifier::new()
            .size(size, size)
            .background(container, shape);
        if let Some(anim) = elevation_anim {
            modifier = modifier.graphics_layer(move || crate::modifier::GraphicsLayerParams {
                shadow_elevation: anim.get(),
                shadow_shape: Some(shape),
                ..Default::default()
            });
        }
        modifier = modifier.then(self.modifier);

        if self.enabled {
            if let Some(on_click) = &self.on_click {
                let callback = on_click.clone();
                modifier = modifier
                    .clickable_with_source(&interaction, move || callback())
                    .ripple_with_shape(&interaction, ripple_color, true, shape);
            }
        }

        match ctx.start_restartable_group(
            key,
            modifier,
            BoxLayout::new().alignment(crate::layout::Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
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
    pub fn get_colors(&self) -> Option<FloatingActionButtonColors> {
        self.colors
    }
    pub fn get_elevation(&self) -> Option<FloatingActionButtonElevation> {
        self.elevation
    }
    pub fn get_size(&self) -> FloatingActionButtonSize {
        self.size
    }
    pub fn get_shape(&self) -> Shape {
        self.shape
            .unwrap_or_else(|| FloatingActionButtonDefaults::shape(self.size))
    }
    pub fn get_modifier(&self) -> &Modifier {
        &self.modifier
    }
}

impl Default for FloatingActionButton {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;

    #[test]
    fn size_tokens_match_m3_fab_variants() {
        assert_eq!(FloatingActionButtonSize::Small.container_size(), 40.0);
        assert_eq!(FloatingActionButtonSize::Regular.container_size(), 56.0);
        assert_eq!(FloatingActionButtonSize::Medium.container_size(), 80.0);
        assert_eq!(FloatingActionButtonSize::Large.container_size(), 96.0);
        assert_eq!(FloatingActionButtonSize::Regular.icon_size(), 24.0);
        assert_eq!(FloatingActionButtonSize::Medium.icon_size(), 28.0);
        assert_eq!(FloatingActionButtonSize::Large.icon_size(), 32.0);
    }

    #[test]
    fn default_elevation_matches_material3_tokens() {
        assert_eq!(
            FloatingActionButtonElevation::default_elevation(),
            FloatingActionButtonElevation::new(6.0, 12.0, 8.0, 8.0, 0.0)
        );
    }

    #[test]
    fn defaults_use_regular_primary_container_fab() {
        let fab = FloatingActionButton::new();
        assert_eq!(fab.get_size(), FloatingActionButtonSize::Regular);
        assert_eq!(fab.get_shape(), Shape::rounded(16.0));
        assert_eq!(
            fab.get_elevation(),
            Some(FloatingActionButtonElevation::default_elevation())
        );
        assert!(fab.get_enabled());
    }

    #[test]
    fn size_constructors_select_expected_shapes() {
        assert_eq!(
            FloatingActionButton::small().get_shape(),
            Shape::rounded(12.0)
        );
        assert_eq!(
            FloatingActionButton::medium().get_shape(),
            Shape::rounded(20.0)
        );
        assert_eq!(
            FloatingActionButton::large().get_shape(),
            Shape::rounded(28.0)
        );
        assert_eq!(
            FloatingActionButton::medium()
                .shape(Shape::Circle)
                .get_shape(),
            Shape::Circle
        );
    }

    #[test]
    fn default_colors_use_primary_container_tokens() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = FloatingActionButtonDefaults::colors(&theme);
        assert_eq!(colors.container, theme.primary_container);
        assert_eq!(colors.content, theme.on_primary_container);
        assert_eq!(
            colors.container_color(false).a,
            (theme.on_surface.a as f32 * 0.12) as u8
        );
        assert_eq!(
            colors.content_color(false).a,
            (theme.on_surface.a as f32 * 0.38) as u8
        );

        let primary = FloatingActionButtonDefaults::primary_colors(&theme);
        assert_eq!(primary.container, theme.primary);
        assert_eq!(primary.content, theme.on_primary);
    }

    #[test]
    fn ripple_color_follows_enabled_content_color() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = FloatingActionButtonDefaults::colors(&theme);
        assert_eq!(colors.ripple_color(true), theme.on_primary_container);
        assert_eq!(colors.ripple_color(false), colors.disabled_content);

        let primary = FloatingActionButtonDefaults::primary_colors(&theme);
        assert_eq!(primary.ripple_color(true), theme.on_primary);
    }

    #[test]
    fn elevation_state_priority_matches_interaction_order() {
        let elevation = FloatingActionButtonElevation::new(1.0, 3.0, 2.0, 4.0, 0.0);
        let mut state = ComponentState::idle();
        assert_eq!(elevation.for_state(&state), 1.0);
        state.hovered = true;
        assert_eq!(elevation.for_state(&state), 4.0);
        state.focused = true;
        assert_eq!(elevation.for_state(&state), 4.0);
        state.pressed = true;
        assert_eq!(elevation.for_state(&state), 3.0);
        state.enabled = false;
        assert_eq!(elevation.for_state(&state), 0.0);
    }

    #[test]
    fn content_color_is_provided_to_fab_content() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                FloatingActionButton::new().build(ctx, |_ctx| {
                    assert_eq!(WiniaTheme::content_color(), theme.on_primary_container);
                });
            });
        });
    }

    #[test]
    fn builder_configuration_is_retained() {
        let custom =
            FloatingActionButtonColors::new(Color::RED, Color::WHITE, Color::BLACK, Color::GREEN);
        let fab = FloatingActionButton::small()
            .enabled(false)
            .colors(custom)
            .shape(Shape::pill())
            .elevation(FloatingActionButtonElevation::bottom_app_bar());
        assert!(!fab.get_enabled());
        assert_eq!(fab.get_colors(), Some(custom));
        assert_eq!(fab.get_shape(), Shape::Pill);
        assert_eq!(
            fab.get_elevation(),
            Some(FloatingActionButtonElevation::bottom_app_bar())
        );
    }
}
