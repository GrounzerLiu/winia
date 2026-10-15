//! Material 3 Floating Action Button components.
//!
//! This module implements the icon-only FAB family. Extended FAB is intentionally
//! left for a separate component because its text layout and expand/collapse
//! semantics are different from a fixed-size FAB.

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::{Backchannel, State};
use crate::layout::constraints::Constraints;
use crate::layout::LayoutDirection;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::unit::{Offset, Size};
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::interaction::{ComponentState, MutableInteractionSource};
use crate::theme::{ThemeColors, WiniaTheme};
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
/// `FloatingActionButtonDefaults.LargeIconSize` (`material3/FloatingActionButton.kt:509-510`) is
/// 36 dp: the source marks `FabLargeTokens.IconSize` (32) as incorrect.
pub const FAB_LARGE_ICON_SIZE: f32 = 36.0;

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

    /// Builds a pair from a container colour alone, deriving the content colour the way Compose's
    /// `contentColor: Color = contentColorFor(containerColor)` default does
    /// (`material3/FloatingActionButton.kt:114`): the theme maps every scheme role that has a paired
    /// "on" colour, and a colour with no pair — a caller's own — falls back to `on_surface`, which
    /// is what the ambient content colour carries for the default theme.
    pub fn from_container(theme: &ThemeColors, container: Color) -> Self {
        let content = theme.content_color_for(container).unwrap_or(theme.on_surface);
        Self::from_pair(theme, container, content)
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

    /// `FabPrimaryContainerTokens` / `FabSecondaryContainerTokens`: `ContainerElevation`,
    /// `PressedContainerElevation` and `FocusedContainerElevation` are all `ElevationTokens.Level3` (6 dp) and
    /// only `HoveredContainerElevation` is `Level4` (8 dp) — pressing a FAB does NOT lift it higher, and
    /// focusing does not either. winia carried 12 dp for pressed and 8 dp for focused, i.e. a press grew the
    /// shadow to `Level5`, which no M3 token asks for.
    pub fn default_elevation() -> Self {
        Self::new(6.0, 6.0, 6.0, 8.0, 0.0)
    }

    /// `FloatingActionButtonDefaults.loweredElevation` (`material3/FloatingActionButton.kt:568-575`):
    /// Level1 (1 dp) at rest, pressed and focused, `Level2` (3 dp) while hovered.
    ///
    /// winia had 2 dp for the hover, which is not a token level at all (`Level2` is 3 dp).
    pub fn lowered() -> Self {
        Self::new(1.0, 1.0, 1.0, 3.0, 0.0)
    }

    /// Zero elevation used by bottom app bars.
    pub fn bottom_app_bar() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0)
    }

    /// The elevation for a state snapshot, by Compose's "latest interaction wins" order: disabled,
    /// then a dragged FAB (which takes the DEFAULT — Compose's elevation animatable matches only
    /// `PressInteraction.Press`, `HoverInteraction.Enter` and `FocusInteraction.Focus` and falls
    /// through to `defaultElevation` for anything else, `FloatingActionButton.kt:713-719`; the
    /// Button follows the same rule), then a press, then hover, then focus.
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

    /// Whether every state's elevation is 0 — nothing to animate, and no layer to pay for. A FAB
    /// whose every entry is 0 (`bottom_app_bar`) skips the animated layer entirely.
    pub fn is_shadow_free(&self) -> bool {
        self.default <= 0.0
            && self.pressed <= 0.0
            && self.focused <= 0.0
            && self.hovered <= 0.0
            && self.disabled <= 0.0
    }

    /// `material3/FloatingActionButton.kt:624-626`: a FAB's tonal elevation IS its default one.
    /// Compose paints the FAB through a `Surface(tonalElevation = ...)` (`:155`), so a container
    /// that resolves to `surface` is tinted by it.
    pub fn tonal_elevation(&self) -> f32 {
        self.default
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

    /// `FloatingActionButtonDefaults.extendedFabShape` (`material3/FloatingActionButton.kt:525-526`):
    /// `ExtendedFabPrimaryTokens.ContainerShape` = CornerLarge (16 dp). Both extended-FAB forms
    /// default to it, so a caller can name the shape the component would pick for itself.
    pub fn extended_fab_shape() -> Shape {
        Shape::rounded(16.0)
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
        // Compose paints the FAB through a `Surface(onClick = ...)`, so its container goes through
        // the tonal overlay: `Surface(tonalElevation = elevation.tonalElevation())`
        // (`FloatingActionButton.kt:155`), where `tonalElevation()` is the DEFAULT elevation
        // (`:624-626`). Only a container that resolves to `surface` is tinted, so the usual
        // `primaryContainer` FAB is unaffected.
        let tonal_elevation = self
            .elevation
            .map(|e| e.tonal_elevation())
            .unwrap_or(0.0);
        let container = crate::components::surface::tonal_background(
            theme,
            colors.container_color(self.enabled),
            tonal_elevation,
        );
        let content_color = colors.content_color(self.enabled);
        let ripple_color = colors.ripple_color(self.enabled);
        let shape = self
            .shape
            .unwrap_or_else(|| FloatingActionButtonDefaults::shape(self.size));
        let size = self.size.container_size();

        // The spec is Compose's, keyed on the interaction PAIR being left and entered
        // (`animation::elevation_spec_for_pair`, from `material3/internal/Elevation.kt:50-61`):
        // 120 ms `FastOutSlowIn` towards an interaction, 150 ms of the exit curve away from one
        // (120 ms out of HOVER), and a snap between two idle frames. winia used one 180 ms
        // `EaseOutCubic` for every direction. The interaction the last target came from is kept in
        // a backchannel — reading it registers no dependency.
        let elevation_anim = self.elevation.and_then(|elevation| {
            if elevation.is_shadow_free() {
                None // every state is 0: no shadow to animate, and no layer to pay for
            } else {
                let last_kind: Backchannel<Option<crate::animation::ElevationInteraction>> =
                    ctx.remember_backchannel(|| None);
                let to = state.elevation_interaction();
                let from = if state.enabled { last_kind.peek() } else { None };
                let spec = crate::animation::elevation_spec_for_pair(from, to)
                    .unwrap_or(crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()));
                last_kind.set(to);
                Some(ctx.animate_float_as_state(elevation.for_state(&state), spec))
            }
        });

        let mut modifier = Modifier::new()
            .size(size, size)
            .background(container, shape);
        if let Some(anim) = elevation_anim {
            modifier = modifier.graphics_layer(move || crate::graphics::GraphicsLayerParams {
                shadow_elevation: anim.get(),
                shadow_shape: Some(shape),
                ..Default::default()
            });
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
    use crate::runtime::composer::Composer;
    use crate::components::Text;

    /// The extended FAB's size animation is M3's `FastSpatial` spring (0.9/1400), not the no-bouncy
    /// 400 the comment used to call an approximation.
    #[test]
    fn the_size_spec_is_m3s_fast_spatial_spring() {
        match fab_size_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("the size spec must be a spring, got {other:?}"),
        }
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, index: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn size_tokens_match_m3_fab_variants() {
        assert_eq!(FloatingActionButtonSize::Small.container_size(), 40.0);
        assert_eq!(FloatingActionButtonSize::Regular.container_size(), 56.0);
        assert_eq!(FloatingActionButtonSize::Medium.container_size(), 80.0);
        assert_eq!(FloatingActionButtonSize::Large.container_size(), 96.0);
        assert_eq!(FloatingActionButtonSize::Regular.icon_size(), 24.0);
        assert_eq!(FloatingActionButtonSize::Medium.icon_size(), 28.0);
        assert_eq!(FloatingActionButtonSize::Large.icon_size(), 36.0);
    }

    #[test]
    fn default_elevation_matches_material3_tokens() {
        // `FabPrimaryContainerTokens`: `ContainerElevation`, `PressedContainerElevation` and
        // `FocusedContainerElevation` are all `ElevationTokens.Level3` (6 dp); only the hovered one is
        // `Level4` (8 dp). (This assertion used to read `(6, 12, 8, 8, 0)`, which the tokens never said.)
        assert_eq!(
            FloatingActionButtonElevation::default_elevation(),
            FloatingActionButtonElevation::new(6.0, 6.0, 6.0, 8.0, 0.0)
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

    /// `FloatingActionButtonDefaults.extendedFabShape` (`material3/FloatingActionButton.kt:525-526`)
    /// is CornerLarge, and it is the shape an extended FAB with no explicit shape resolves to.
    #[test]
    fn the_extended_fab_default_shape_is_corner_large() {
        let def = FloatingActionButtonDefaults::extended_fab_shape();
        assert_eq!(
            def,
            Shape::rounded(16.0),
            "ExtendedFabPrimaryTokens.ContainerShape"
        );

        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::content(|ctx| Text::new("Create").build(ctx))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let painted_shape = nodes[root]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { shape, .. } => Some(*shape),
                _ => None,
            })
            .expect("the extended FAB paints a background");
        assert_eq!(
            painted_shape, def,
            "the component defaults to the shared accessor, not a second literal"
        );
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

    /// `contentColorFor(containerColor)` is Compose's default (`FloatingActionButton.kt:114`): a
    /// paired role derives its "on" colour, and anything else falls back to `on_surface`.
    #[test]
    fn from_container_derives_the_content_colour() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let paired = FloatingActionButtonColors::from_container(&theme, theme.secondary_container);
        assert_eq!(paired.container, theme.secondary_container);
        assert_eq!(paired.content, theme.on_secondary_container);

        let custom = Color::from_argb(255, 1, 2, 3);
        let unpaired = FloatingActionButtonColors::from_container(&theme, custom);
        assert_eq!(unpaired.container, custom);
        assert_eq!(
            unpaired.content, theme.on_surface,
            "a colour with no paired \"on\" role falls back to on_surface"
        );
        // The disabled pair still comes from the theme, exactly as `from_pair` builds it.
        assert_eq!(
            unpaired.disabled_container.a,
            (theme.on_surface.a as f32 * 0.12) as u8
        );
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

    /// A DRAGGED FAB takes the default — Compose's `FloatingActionButtonElevationAnimatable`
    /// matches only Press/Hover/Focus and falls through to the default for a drag
    /// (`FloatingActionButton.kt:713-719`), and the Button follows the same rule. A drag always
    /// holds a press, so without the dragged branch the value would be the pressed one.
    #[test]
    fn a_dragged_fab_takes_the_default_elevation() {
        let elevation = FloatingActionButtonElevation::new(1.0, 3.0, 2.0, 4.0, 0.0);
        let mut state = ComponentState::idle();
        state.dragged = true;
        state.pressed = true;
        assert_eq!(elevation.for_state(&state), 1.0);
        assert_eq!(
            state.elevation_interaction(),
            Some(crate::animation::ElevationInteraction::Drag)
        );
    }

    /// The short-circuit both FABs share: a token set whose every state is 0 gets no animated
    /// layer at all (`FloatingActionButtonElevation::bottom_app_bar`).
    #[test]
    fn a_shadow_free_elevation_is_recognised() {
        assert!(FloatingActionButtonElevation::bottom_app_bar().is_shadow_free());
        assert!(!FloatingActionButtonElevation::new(1.0, 3.0, 2.0, 4.0, 0.0).is_shadow_free());
        assert!(!FloatingActionButtonElevation::new(0.0, 0.0, 0.0, 0.0, 1.0).is_shadow_free());
    }

    /// Compose paints the FAB through a `Surface(tonalElevation = elevation.tonalElevation())`
    /// (`FloatingActionButton.kt:155`), and `tonalElevation()` IS the default elevation
    /// (`:624-626`). So a container that resolves to `surface` is tinted by that value — and only
    /// a `surface` container is.
    #[test]
    fn a_surface_coloured_fab_is_tinted_by_its_tonal_elevation() {
        let theme = ThemeColors::default_light();
        let painted = |colors: FloatingActionButtonColors| -> Color {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    FloatingActionButton::new().colors(colors).build(ctx, |_| {});
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
            composer
                .arena_nodes()
                .iter()
                .find_map(|node| {
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { color_fn, .. } => {
                            Some(color_fn())
                        }
                        _ => None,
                    })
                })
                .expect("the FAB paints a background")
        };

        // The token set's default is Level3 = 6 dp, so the tint's alpha is (4.5·ln 7 + 2)/100.
        let alpha = ((4.5 * (6.0f32 + 1.0).ln()) + 2.0) / 100.0;
        let on_surface =
            FloatingActionButtonColors::from_pair(&theme, theme.surface, theme.on_surface);
        assert_eq!(
            painted(on_surface),
            theme.surface.overlay(theme.surface_tint, alpha),
            "a surface-coloured FAB is tinted by its default elevation"
        );

        // Any other container is left alone — the gate `applyTonalElevation` has, and the reason
        // the usual `primaryContainer` FAB never shows a tint.
        assert_eq!(
            painted(FloatingActionButtonDefaults::primary_colors(&theme)),
            theme.primary,
            "a non-surface container is untouched"
        );
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
    // ── ExtendedFloatingActionButton ──

    fn icon_leaf(ctx: &mut ComposeCtx, size: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(size, size));
        ctx.end_node();
    }

    #[test]
    fn extended_fab_tokens_match_androidx_main() {
        assert_eq!(EXTENDED_FAB_HEIGHT, 56.0, "ExtendedFabPrimaryTokens.ContainerHeight");
        assert_eq!(EXTENDED_FAB_COLLAPSED_WIDTH, 56.0, "FabBaselineTokens.ContainerWidth");
        assert_eq!(EXTENDED_FAB_MIN_EXPANDED_WIDTH, 80.0, "ExtendedFabMinimumWidth");
    }

    /// Each tier's metric set, against `*ExtendedFabTokens` — including the three values upstream
    /// does NOT take from the token file. The Medium and Large icon-label gaps are overridden in
    /// the source (its own TODO says the tokens are wrong: 16 and 20 against the code's 12 and 16),
    /// the medium shape is `ShapeDefaults.LargeIncreased` because that token line is commented out
    /// upstream, and the collapsed width is the tier's `ContainerHeight`, not a shared constant.
    #[test]
    fn the_extended_fab_size_tiers_are_the_tokens() {
        for (size, height, padding, gap, radius, icon) in [
            (ExtendedFabSize::Small, 56.0, 16.0, 8.0, 16.0, 24.0),
            (ExtendedFabSize::Medium, 80.0, 26.0, 12.0, 20.0, 28.0),
            (ExtendedFabSize::Large, 96.0, 28.0, 16.0, 28.0, 32.0),
        ] {
            assert_eq!(size.container_height(), height, "{size:?} ContainerHeight");
            assert_eq!(size.horizontal_padding(), padding, "{size:?} Leading/TrailingSpace");
            assert_eq!(size.icon_label_space(), gap, "{size:?} IconLabelSpace");
            assert_eq!(size.shape(), Shape::rounded(radius), "{size:?} extendedFabShape");
            assert_eq!(size.icon_size(), icon, "{size:?} IconSize");
            assert_eq!(
                size.metrics().collapsed_width,
                height,
                "{size:?}: *ExtendedFabMinimumWidth IS the tier's ContainerHeight"
            );
        }
        // The three tiers are three distinct heights — nothing is shared between them.
        assert_ne!(
            ExtendedFabSize::Small.container_height(),
            ExtendedFabSize::Medium.container_height()
        );
        assert_ne!(
            ExtendedFabSize::Medium.container_height(),
            ExtendedFabSize::Large.container_height()
        );
    }

    /// A named tier moves the whole metric set at once. The collapsed box is the tier's own square,
    /// and the expanded one is `start + icon + gap + text + end` with the icon at the start padding
    /// and the label one icon-plus-gap further in (upstream's private overload, which takes all four
    /// numbers as parameters: `FloatingActionButton.kt:917-986`).
    #[test]
    fn a_named_tier_moves_the_collapsed_and_expanded_geometry() {
        let geom = |size: ExtendedFabSize, expanded: bool| {
            let mut composer = Composer::new();
            // `move` because the icon slot captures `size`, and a composable closure has to own
            // what it captures.
            composer.compose(move |ctx| {
                ExtendedFloatingActionButton::new(
                    |ctx| Text::new("Create").build(ctx),
                    move |ctx| icon_leaf(ctx, size.icon_size()),
                    State::new(expanded),
                )
                .size(size)
                .on_click(|| {})
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 600.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let icon = child(&nodes, root, 0);
            let text = child(&nodes, root, 1);
            (
                nodes[root].measured_size,
                icon.position.x,
                text.position.x,
                text.measured_size.width,
            )
        };

        for size in [
            ExtendedFabSize::Small,
            ExtendedFabSize::Medium,
            ExtendedFabSize::Large,
        ] {
            let h = size.container_height();
            let (collapsed, _, _, _) = geom(size, false);
            assert_eq!(
                (collapsed.width, collapsed.height),
                (h, h),
                "{size:?} collapses to its own square"
            );

            let (expanded, icon_x, text_x, text_w) = geom(size, true);
            let padding = size.horizontal_padding();
            assert_eq!(expanded.height, h, "{size:?} height");
            assert_eq!(
                expanded.width,
                padding + size.icon_size() + size.icon_label_space() + text_w + padding,
                "{size:?}: start + icon + gap + text + end"
            );
            assert_eq!(icon_x, padding, "{size:?} icon sits at the start padding");
            assert_eq!(
                text_x,
                padding + size.icon_size() + size.icon_label_space(),
                "{size:?} label sits one icon plus the gap further in"
            );
        }
    }

    #[test]
    fn extended_fab_collapsed_is_56_square_icon_centered() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(false),
            )
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // 收起态：56×56（同普通 FAB），图标居中
        assert_eq!(nodes[root].measured_size.width, EXTENDED_FAB_COLLAPSED_WIDTH);
        assert_eq!(nodes[root].measured_size.height, EXTENDED_FAB_HEIGHT);
        let icon = child(&nodes, root, 0);
        assert_eq!(
            icon.position,
            Offset::new(
                (EXTENDED_FAB_COLLAPSED_WIDTH - 24.0) / 2.0,
                (EXTENDED_FAB_HEIGHT - 24.0) / 2.0,
            ),
            "收起态图标居中"
        );
    }

    #[test]
    fn extended_fab_expanded_shows_icon_then_text_with_spec_paddings() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(true),
            )
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let icon = child(&nodes, root, 0);
        let text_slot = child(&nodes, root, 1);
        // 展开态：宽 = 16 + 24 + 12 + textW + 20（≥80）；高 56
        let expected_w = EXT_START_ICON_PADDING
            + 24.0
            + EXT_END_ICON_PADDING
            + text_slot.measured_size.width
            + EXT_TEXT_PADDING;
        assert_eq!(nodes[root].measured_size.height, EXTENDED_FAB_HEIGHT);
        assert_eq!(nodes[root].measured_size.width, expected_w);
        // 图标 x=16（start padding）垂直居中
        assert_eq!(icon.position.x, EXT_START_ICON_PADDING);
        assert_eq!(icon.position.y, (EXTENDED_FAB_HEIGHT - 24.0) / 2.0);
        // 文本槽紧跟 icon+12dp 间距，垂直居中
        assert_eq!(
            text_slot.position.x,
            EXT_START_ICON_PADDING + 24.0 + EXT_END_ICON_PADDING,
        );
        assert_eq!(
            text_slot.position.y,
            (EXTENDED_FAB_HEIGHT - text_slot.measured_size.height) / 2.0,
        );
    }

    #[test]
    fn extended_fab_rtl_mirrors_icon_and_text_positions() {
        // RTL 展开态：图标贴右缘（width-16-iconW），文本在图标左侧（gap 12）
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(true),
            )
            .modifier(Modifier::new().layout_direction(LayoutDirection::Rtl))
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let width = nodes[root].measured_size.width;
        let icon = child(&nodes, root, 0);
        let text_slot = child(&nodes, root, 1);
        // 图标右缘距容器右缘 16（LTR 时是左缘距左缘 16）
        assert_eq!(
            icon.position.x + icon.measured_size.width,
            width - EXT_START_ICON_PADDING,
            "RTL 下图标应镜像到右侧"
        );
        // 文本右缘 = 图标左缘 - 12dp 间距
        assert_eq!(
            text_slot.position.x + text_slot.measured_size.width,
            icon.position.x - EXT_END_ICON_PADDING,
        );
        // LTR 对照：同结构方向相反
        let mut ltr = Composer::new();
        ltr.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(true),
            )
            .on_click(|| {})
            .build(ctx);
        });
        ltr.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes2 = ltr.arena_nodes();
        let icon_ltr = child(&nodes2, ltr.layout_root_idx().unwrap(), 0);
        assert_eq!(icon_ltr.position.x, EXT_START_ICON_PADDING, "LTR 图标在左侧");
    }

    /// The content-only overload (`FloatingActionButton.kt:312`): one row, 20 dp horizontal
    /// padding, an 80 dp floor, 56 dp tall, with the content centred.
    #[test]
    fn extended_fab_content_only_pads_20_and_floors_at_80() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::content(|ctx| Text::new("Create").build(ctx))
                .on_click(|| {})
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let content = child(&nodes, root, 0);
        let size = nodes[root].measured_size;
        assert_eq!(size.height, EXTENDED_FAB_HEIGHT);
        assert!(
            size.width >= EXTENDED_FAB_MIN_EXPANDED_WIDTH,
            "a short label still floors at 80 dp, got {size:?}"
        );
        assert_eq!(content.position.x, (size.width - content.measured_size.width) / 2.0);
        assert_eq!(
            content.position.y,
            (size.height - content.measured_size.height) / 2.0
        );
    }

    /// Content wider than the floor widens the bar: width = 20 + content + 20.
    #[test]
    fn extended_fab_content_only_widens_for_wide_content() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::content(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().size(200.0, 24.0));
                ctx.end_node();
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(
            nodes[root].measured_size.width,
            200.0 + 2.0 * EXT_TEXT_PADDING,
        );
        assert_eq!(nodes[root].measured_size.height, EXTENDED_FAB_HEIGHT);
    }
}

// ── ExtendedFloatingActionButton（M3 扩展 FAB）──
//
// 对齐 androidx-main ExtendedFloatingActionButton：
// - 高 56dp；收起态 56×56（同 FAB，仅图标居中）；展开态 min 宽 80dp，
///  内容 [icon 前 16dp | icon | 图标-文本 12dp | text | 后 20dp]
// - 形状 CornerLarge（16dp 圆角）；容器 PrimaryContainer、内容 OnPrimaryContainer
// - 展开/收起：宽度与内容位置按进度插值（FastSpatial 近似 stiffness400），
///  文本透明度随进度淡入淡出（FastEffects）
// - 高程默认 Level3 / hover Level4（同 FAB）——复用 FloatingActionButtonElevation

/// 高度 = ExtendedFabPrimaryTokens.ContainerHeight
pub const EXTENDED_FAB_HEIGHT: f32 = 56.0;
/// 收起态宽 = FabBaselineTokens.ContainerWidth（同普通 FAB）
pub const EXTENDED_FAB_COLLAPSED_WIDTH: f32 = 56.0;
/// 展开态最小宽
pub const EXTENDED_FAB_MIN_EXPANDED_WIDTH: f32 = 80.0;
const EXT_START_ICON_PADDING: f32 = 16.0;
const EXT_END_ICON_PADDING: f32 = 12.0;
const EXT_TEXT_PADDING: f32 = 20.0;

/// The extended FAB's three size tiers — material3's `SmallExtendedFloatingActionButton`,
/// `MediumExtendedFloatingActionButton` and `LargeExtendedFloatingActionButton`
/// (`FloatingActionButton.kt:396`, `:461`, `:526`, and the icon/text overloads at `:659`,
/// `:726`, `:793`).
///
/// Each tier is a full metric set, not a scale factor: height, shape, both horizontal paddings,
/// the icon-label gap, the icon size and the label text style all move together, and two of the
/// gaps do NOT come from the token file — upstream overrides them, see `icon_label_space`.
///
/// The DEFAULT extended FAB (no tier named) stays on its own metric set: upstream's un-suffixed
/// `ExtendedFloatingActionButton` is a separate composable with its own constants, not one of
/// these three. See `EXTENDED_FAB_BASELINE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendedFabSize {
    Small,
    Medium,
    Large,
}

impl ExtendedFabSize {
    /// `*ExtendedFabTokens.ContainerHeight` — also the tier's MINIMUM width, so a collapsed sized
    /// FAB is square (`SmallExtendedFabMinimumWidth = ExtendedFabSmallTokens.ContainerHeight`,
    /// `FloatingActionButton.kt:1420-1423`).
    pub fn container_height(self) -> f32 {
        match self {
            Self::Small => 56.0,
            Self::Medium => 80.0,
            Self::Large => 96.0,
        }
    }

    /// `*ExtendedFabTokens.LeadingSpace` / `TrailingSpace` — the two are equal at every tier.
    pub fn horizontal_padding(self) -> f32 {
        match self {
            Self::Small => 16.0,
            Self::Medium => 26.0,
            Self::Large => 28.0,
        }
    }

    /// `*ExtendedFabTokens.IconLabelSpace` — except that upstream overrides two of them and says
    /// why (`FloatingActionButton.kt:1440`, `:1452`):
    ///
    ///     // TODO: ExtendedFabMediumTokens.IconLabelSpace is incorrect
    ///     private val MediumExtendedFabIconPadding get() = 12.dp
    ///     // TODO: ExtendedFabLargeTokens.IconLabelSpace is incorrect
    ///     private val LargeExtendedFabIconPadding get() = 16.dp
    ///
    /// The tokens say 16 and 20. winia follows the CODE, like every other component here.
    pub fn icon_label_space(self) -> f32 {
        match self {
            Self::Small => 8.0,
            Self::Medium => 12.0,
            Self::Large => 16.0,
        }
    }

    /// `FloatingActionButtonDefaults.small/medium/largeExtendedFabShape`
    /// (`FloatingActionButton.kt:1020-1032`): CornerLarge (16), then `ShapeDefaults.LargeIncreased`
    /// (20) — the medium token is commented out upstream behind a TODO — then CornerExtraLarge (28).
    pub fn shape(self) -> Shape {
        Shape::rounded(match self {
            Self::Small => 16.0,
            Self::Medium => 20.0,
            Self::Large => 28.0,
        })
    }

    /// `*ExtendedFabTokens.IconSize`. The component does not force it — upstream's private
    /// overload takes no icon size and the icon slot is the caller's — so this is the value to
    /// hand to the icon, not a value this component applies.
    pub fn icon_size(self) -> f32 {
        match self {
            Self::Small => 24.0,
            Self::Medium => 28.0,
            Self::Large => 32.0,
        }
    }

    /// `Small/Medium/LargeExtendedFabTextStyle` (`FloatingActionButton.kt:1429`, `:1443`,
    /// `:1455`): TitleMedium, TitleLarge, HeadlineSmall.
    pub fn text_style(self, typography: &crate::theme::Typography) -> crate::text::TextStyle {
        match self {
            Self::Small => typography.title_medium.clone(),
            Self::Medium => typography.title_large.clone(),
            Self::Large => typography.headline_small.clone(),
        }
    }
}

/// The metric set one extended FAB measures with — the tier's, or the un-suffixed default's.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ExtendedFabMetrics {
    height: f32,
    /// Width while collapsed. Upstream's `lerp(minWidth, expandedWidth, progress)` starts here
    /// (`FloatingActionButton.kt:960`).
    collapsed_width: f32,
    /// The gap before the icon, and the one after the label.
    start_padding: f32,
    end_padding: f32,
    /// Between the icon and the label.
    icon_label_space: f32,
    /// Floor for the EXPANDED width. Upstream has no floor on the icon/text overload — the width
    /// is the measured intrinsic — but the un-suffixed form ships `ExtendedFabMinimumWidth` 80.
    min_expanded_width: f32,
    /// The content-only (Row) form's two paddings and its own minimum width. They differ from the
    /// icon/text pair above at the un-suffixed tier: that overload pads BOTH sides by
    /// `ExtendedFabTextPadding` (20) inside a `sizeIn(minWidth = 80)`, while the icon/text one
    /// splits 16 / 20 (`FloatingActionButton.kt:611-615` against `:955-965`).
    row_start_padding: f32,
    row_end_padding: f32,
    row_min_width: f32,
}

/// The un-suffixed `ExtendedFloatingActionButton`'s metrics — `ExtendedFabPrimaryTokens` plus the
/// four private constants next to it (`FloatingActionButton.kt:1462-1478`):
/// `ExtendedFabStartIconPadding` 16, `ExtendedFabEndIconPadding` 12, `ExtendedFabTextPadding` 20,
/// `ExtendedFabMinimumWidth` 80; height `ContainerHeight` 56 and shape CornerLarge 16.
///
/// `collapsed_width` is winia's OWN value and a recorded deviation: upstream collapses this form to
/// `ExtendedFabMinimumWidth` (80), while this component collapses to the square
/// `FabBaselineTokens.ContainerWidth` (56) — the shape the wide navigation rail's header FAB has
/// while the rail is collapsed.
const EXTENDED_FAB_BASELINE: ExtendedFabMetrics = ExtendedFabMetrics {
    height: EXTENDED_FAB_HEIGHT,
    collapsed_width: EXTENDED_FAB_COLLAPSED_WIDTH,
    start_padding: EXT_START_ICON_PADDING,
    end_padding: EXT_TEXT_PADDING,
    icon_label_space: EXT_END_ICON_PADDING,
    min_expanded_width: EXTENDED_FAB_MIN_EXPANDED_WIDTH,
    row_start_padding: EXT_TEXT_PADDING,
    row_end_padding: EXT_TEXT_PADDING,
    row_min_width: EXTENDED_FAB_MIN_EXPANDED_WIDTH,
};

impl ExtendedFabSize {
    fn metrics(self) -> ExtendedFabMetrics {
        let height = self.container_height();
        let padding = self.horizontal_padding();
        ExtendedFabMetrics {
            height,
            // Upstream: `*ExtendedFabMinimumWidth` IS the tier's `ContainerHeight`.
            collapsed_width: height,
            start_padding: padding,
            end_padding: padding,
            icon_label_space: self.icon_label_space(),
            // Upstream leaves the expanded width to the measured intrinsic; flooring it at the
            // collapsed width keeps the box from ever being narrower than the spring's start.
            min_expanded_width: height,
            // The sized Row overload pads by `LeadingSpace` / `TrailingSpace` and floors the
            // container at the tier's `ContainerHeight` (`FloatingActionButton.kt:409-420`).
            row_start_padding: padding,
            row_end_padding: padding,
            row_min_width: height,
        }
    }
}

pub struct ExtendedFloatingActionButton {
    content: ExtendedFabContent,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<FloatingActionButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    elevation: Option<FloatingActionButtonElevation>,
    shape: Option<Shape>,
    /// The size tier, when the caller named one. `None` keeps the un-suffixed metric set.
    size: Option<ExtendedFabSize>,
    modifier: Modifier,
}

/// The extended FAB's two content forms, one per material3 overload.
enum ExtendedFabContent {
    /// `FloatingActionButton.kt:381`: separate icon/text slots whose width, positions and label
    /// alpha all follow `expanded`.
    IconText {
        text: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
        icon: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
        expanded: State<bool>,
    },
    /// `FloatingActionButton.kt:312`: one caller-composed row instead of the icon/text split —
    /// no expand/collapse, 20 dp horizontal padding, 80 dp minimum width.
    Row(Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>),
}

impl ExtendedFloatingActionButton {
    /// text/icon 为内容槽；expanded 绑定开合状态（State 驱动动画）
    pub fn new(
        text: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        expanded: State<bool>,
    ) -> Self {
        Self {
            content: ExtendedFabContent::IconText {
                text: Box::new(text),
                icon: Box::new(icon),
                expanded,
            },
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            elevation: Some(FloatingActionButtonDefaults::elevation()),
            shape: None,
            size: None,
            modifier: Modifier::new(),
        }
    }

    /// The content-only overload (`material3/FloatingActionButton.kt:312-340`): a single
    /// caller-composed row instead of the icon/text split, with no expand/collapse to drive.
    /// The row keeps material3's geometry — 20 dp horizontal padding and a minimum width of
    /// 80 dp — inside the 56 dp-tall CornerLarge container.
    pub fn content(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            content: ExtendedFabContent::Row(Box::new(content)),
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            elevation: Some(FloatingActionButtonDefaults::elevation()),
            shape: None,
            size: None,
            modifier: Modifier::new(),
        }
    }

    /// The size tier — material3's `SmallExtendedFloatingActionButton`,
    /// `MediumExtendedFloatingActionButton` and `LargeExtendedFloatingActionButton`, which are
    /// three separate composables over one private implementation
    /// (`material3/FloatingActionButton.kt:917-986`).
    ///
    /// Naming a tier replaces the whole metric set at once: height, minimum (collapsed) width,
    /// both horizontal paddings, the icon-label gap, the label text style and the default shape.
    /// Every one of those moves between tiers, so they are not independently settable upstream
    /// either — an explicit `.shape(...)` still wins over the tier's, as it does over the default.
    ///
    /// Left unset the component keeps the un-suffixed `ExtendedFloatingActionButton`'s metrics,
    /// which is a different set from any tier even at the same height (56): the baseline pads
    /// 16 / 20 around the label with a 12 dp icon gap, while `Small` pads 16 / 16 with 8.
    pub fn size(mut self, size: ExtendedFabSize) -> Self {
        self.size = Some(size);
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

    /// 默认 CornerLarge（16dp 圆角）
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        ctx.changed(&self.shape);
        ctx.changed(&self.size);
        if let ExtendedFabContent::IconText { expanded, .. } = &self.content {
            ctx.changed(&expanded.get());
        }
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| {
            FloatingActionButtonColors::from_pair(
                &theme,
                theme.primary_container,
                theme.on_primary_container,
            )
        });
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let state = interaction.state(self.enabled);
        let tonal_elevation = self
            .elevation
            .map(|e| e.tonal_elevation())
            .unwrap_or(0.0);
        let container = crate::components::surface::tonal_background(
            theme,
            colors.container_color(self.enabled),
            tonal_elevation,
        );
        let content_color = colors.content_color(self.enabled);
        let ripple_color = colors.ripple_color(self.enabled);
        // The tier's whole metric set, or the un-suffixed default's. Naming a tier moves the
        // height, the collapsed width, both paddings, the icon gap, the text style and the shape
        // together — they are not independently settable upstream either.
        let metrics = self
            .size
            .map(ExtendedFabSize::metrics)
            .unwrap_or(EXTENDED_FAB_BASELINE);
        // The shape: an explicit caller shape wins, then the tier's, then
        // `ExtendedFabPrimaryTokens.ContainerShape` = CornerLarge (16 dp).
        let shape = self.shape.unwrap_or_else(|| {
            self.size
                .map(ExtendedFabSize::shape)
                .unwrap_or_else(FloatingActionButtonDefaults::extended_fab_shape)
        });
        // The label style: `Small/Medium/LargeExtendedFabTextStyle` per tier, else
        // `ExtendedFabPrimaryTokens.LabelTextFont` = LabelLarge.
        let label_style = match self.size {
            Some(size) => size.text_style(&WiniaTheme::typography()),
            None => WiniaTheme::typography().label_large.clone(),
        };

        // Elevation animates exactly like `FloatingActionButton::build`: same interaction-pair
        // spec rule, same backchannel. This used to take `for_state`'s instantaneous value, so
        // the shadow jumped while the plain FAB's glided.
        let elevation_anim = self.elevation.and_then(|elevation| {
            if elevation.is_shadow_free() {
                None
            } else {
                let last_kind: Backchannel<Option<crate::animation::ElevationInteraction>> =
                    ctx.remember_backchannel(|| None);
                let to = state.elevation_interaction();
                let from = if state.enabled { last_kind.peek() } else { None };
                let spec = crate::animation::elevation_spec_for_pair(from, to)
                    .unwrap_or(crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()));
                last_kind.set(to);
                Some(ctx.animate_float_as_state(elevation.for_state(&state), spec))
            }
        });

        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);

        let mut modifier = Modifier::new().background(container, shape);
        if let Some(anim) = elevation_anim {
            modifier = modifier.graphics_layer(move || crate::graphics::GraphicsLayerParams {
                shadow_elevation: anim.get(),
                shadow_shape: Some(shape),
                ..Default::default()
            });
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
                let callback = on_click.clone();
                modifier = modifier
                    .clickable_with_source(&interaction, move || callback())
                    .ripple_with_shape(&interaction, ripple_color, true, shape);
            }
        }

        match self.content {
            ExtendedFabContent::IconText { text, icon, expanded } => {
                // The expand/collapse progress — see `fab_size_spec`.
                let progress = ctx.animate_float_as_state(
                    if expanded.get() { 1.0 } else { 0.0 },
                    fab_size_spec(),
                );
                let policy = ExtendedFabLayoutPolicy {
                    progress: progress.clone(),
                    direction,
                    metrics,
                };
                match ctx.start_restartable_group(key, modifier, policy) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        // Child order [icon, text] — placement belongs to ExtendedFabLayoutPolicy.
                        wrap_fab_slot(ctx, icon);
                        // The label slot fades with the progress (M3's FastEffects fade).
                        let alpha_progress = progress.clone();
                        let text_alpha = Modifier::new().graphics_layer(move || {
                            crate::graphics::GraphicsLayerParams {
                                alpha: alpha_progress.peek(),
                                ..Default::default()
                            }
                        });
                        wrap_fab_slot_with(ctx, text_alpha, |ctx| {
                            WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                                crate::components::text::ProvideTextStyle(label_style.clone(), ctx, |ctx| {
                                    // The tier's icon-label gap, bled into the text slot so the
                                    // gap moves with the padding. The baseline's is 12 dp
                                    // (`ExtendedFabEndIconPadding`).
                                    text(ctx);
                                });
                            });
                        });
                    }
                }
            }
            ExtendedFabContent::Row(content) => {
                match ctx.start_restartable_group(key, modifier, ExtendedFabContentPolicy { metrics }) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        wrap_fab_slot(ctx, |ctx| {
                            WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                                crate::components::text::ProvideTextStyle(label_style.clone(), ctx, |ctx| {
                                    content(ctx);
                                });
                            });
                        });
                    }
                }
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

fn wrap_fab_slot(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

fn wrap_fab_slot_with(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    content: impl FnOnce(&mut ComposeCtx),
) {
    let key = ctx.next_key();
    match ctx.start_restartable_group(key, modifier, BoxLayout::new()) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

/// The content-only form's layout (`material3/FloatingActionButton.kt:611-618` for the
/// un-suffixed form, `:409-420` for a sized one): one centred row inside
/// `sizeIn(minWidth).padding(start, end)`, the container at least `height` tall. There is no
/// progress — this form does not collapse.
#[derive(Debug)]
struct ExtendedFabContentPolicy {
    metrics: ExtendedFabMetrics,
}

impl MeasurePolicy for ExtendedFabContentPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let loose = constraints.loosen();
        let (content_size, _) = measure_node(nodes, policies, children[0], loose);
        // The floor is the OUTER width; the padding sits inside it, which is why the row's
        // content is centred in the container rather than offset by the padding.
        let width = (content_size.width + self.metrics.row_start_padding + self.metrics.row_end_padding)
            .max(self.metrics.row_min_width);
        let height = content_size.height.max(self.metrics.height);
        let position = Offset::new(
            (width - content_size.width) / 2.0,
            (height - content_size.height) / 2.0,
        );
        (
            Size::new(width, height),
            vec![Placement { size: content_size, position }],
        )
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }
}

/// Extended FAB 布局：宽度在收起（56×56 仅图标居中）与展开
/// （[16 | icon | 12 | text | 20]，min 宽 80）之间按进度插值；
/// 文本透明度同步淡入淡出。measure 期读进度注册 layout_deps。
///
/// The paddings and the collapsed width come from `metrics`, so a named size tier moves all of
/// them together: `[start | icon | gap | text | end]`, collapsing to `collapsed_width`
/// (upstream's `lerp(minWidth, expandedWidth, progress)`).
#[derive(Debug)]
struct ExtendedFabLayoutPolicy {
    progress: State<f32>,
    direction: LayoutDirection,
    metrics: ExtendedFabMetrics,
}

/// The extended FAB's expand/collapse progress runs on M3's `FastSpatial` token
/// (`material3/FloatingActionButton.kt:467`: `val sizeAnimationSpec =
/// MotionSchemeKeyTokens.FastSpatial.value<Float>()`), damping 0.9 / stiffness 1400. winia used
/// damping 1.0 / stiffness 400 and called it an approximation in the comment.
fn fab_size_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast())
}

impl MeasurePolicy for ExtendedFabLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let p = self.progress.get().max(0.0).min(1.0);
        let loose = constraints.loosen();

        let (icon_size, _) = measure_node(nodes, policies, children[0], loose);
        let (text_size, _) = measure_node(nodes, policies, children[1], loose);

        // Collapsed: the tier's square (or the baseline's 56), icon centred.
        // Expanded: start + icon + gap + text + end, flored at `min_expanded_width`.
        let m = self.metrics;
        let collapsed_w = m.collapsed_width;
        let expanded_w = (m.start_padding
            + icon_size.width
            + m.icon_label_space
            + text_size.width
            + m.end_padding)
            .max(m.min_expanded_width);
        let width = collapsed_w + (expanded_w - collapsed_w) * p;
        let height = m.height.max(icon_size.height).max(text_size.height);

        // icon x: centered -> start padding; RTL mirrors to the right edge
        let icon_x_collapsed = (width - icon_size.width) / 2.0;
        let (icon_x_expanded, text_x_expanded) = if self.direction == LayoutDirection::Rtl {
            let ix = width - m.start_padding - icon_size.width;
            let tx = ix - m.icon_label_space - text_size.width;
            (ix, tx)
        } else {
            (
                m.start_padding,
                m.start_padding + icon_size.width + m.icon_label_space,
            )
        };
        let icon_x = icon_x_collapsed + (icon_x_expanded - icon_x_collapsed) * p;
        // text x: hidden centered when collapsed, hugs icon side when expanded
        let text_x_collapsed = (width - text_size.width) / 2.0;
        let text_x = text_x_collapsed + (text_x_expanded - text_x_collapsed) * p;

        let mut placements = Vec::with_capacity(children.len());
        placements.push(Placement {
            size: icon_size,
            position: Offset::new(icon_x, (height - icon_size.height) / 2.0),
        });
        placements.push(Placement {
            size: text_size,
            position: Offset::new(text_x, (height - text_size.height) / 2.0),
        });

        (Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            nodes[child].position = placements[index].position;
            nodes[child].measured_size = placements[index].size;
        }
    }

}
