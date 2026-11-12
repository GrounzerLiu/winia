//! The Card component — aligned with Compose material3 1.4.0's Card family
//!
//! The three variants of the M3 Card family (Card(filled) / ElevatedCard / OutlinedCard) are
//! constructor variants of the one `Card` builder, as Button's filled/elevated/outlined are:
//! - `Card::new()` / `Card::filled()` — `Card`'s counterpart (SurfaceContainerHighest container)
//! - `Card::elevated()` — `ElevatedCard`'s counterpart (SurfaceContainerLow + default shadow)
//! - `Card::outlined()` — `OutlinedCard`'s counterpart (Surface container + 1dp outline border)
//!
//! Content = a top-aligned Column (the counterpart of M3's Surface { Column(content) }); the
//! content colour is passed down through `WiniaTheme::with_content_color` (an Icon with tint
//! Auto then picks up the card's own content colour).

use crate::composable;
use crate::graphics::{Color, Shape};
use crate::input::interaction::{ComponentState, MutableInteractionSource};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════
// CardStyle / CardColors / CardElevation / CardBorder
// ═══════════════════════════════════════════════════════════

/// Card style variants (the counterpart of M3's Card / ElevatedCard / OutlinedCard)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardStyle {
    /// Filled card (the counterpart of `Card`) — SurfaceContainerHighest container
    #[default]
    Filled,
    /// Elevated card (the counterpart of `ElevatedCard`) — SurfaceContainerLow + default shadow
    Elevated,
    /// Outlined card (the counterpart of `OutlinedCard`) — Surface container + 1dp outline border
    Outlined,
}

/// The card colour set (the material3 `CardColors` counterpart) — container/content each carry an
/// enabled/disabled variant; disabled: the container follows the M3 token (Filled composites
/// SurfaceVariant@38% underneath, the Elevated/Outlined container stays as it is) and the content
/// colour sits at 38% (M3's DisabledAlpha).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardColors {
    pub container: Color,
    pub content: Color,
    pub disabled_container: Color,
    pub disabled_content: Color,
}

impl CardColors {
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

    /// The state-aware container colour — it tells enabled and disabled apart only (as material3's
    /// CardColors does; the hover/focus/press visuals are drawn by the indication (ripple) state
    /// layer, so the two never stack).
    pub fn container_color_for(&self, state: &ComponentState) -> Color {
        if !state.enabled {
            self.disabled_container
        } else {
            self.container
        }
    }

    /// The state-aware content colour (enabled/disabled only — as material3's CardColors does)
    pub fn content_color_for(&self, state: &ComponentState) -> Color {
        if state.enabled {
            self.content
        } else {
            self.disabled_content
        }
    }

    /// Builds the style's default colours from the theme (the counterpart of material3's
    /// CardDefaults.cardColors/elevatedCardColors/outlinedCardColors — M3 1.4.0 tokens).
    ///
    /// The disabled container is evaluated with Compose's own expression, not approximated by
    /// multiplying `alpha` directly: M3 composites
    /// `DisabledContainerColor @ DisabledContainerOpacity compositeOver(ContainerColor)`
    /// (`Card.kt:507-511`, `:559-563`) onto the opaque container, so the result is **opaque**.
    /// Multiplying the alpha directly yields a translucent card that lets whatever sits behind it
    /// show through — and two of the three styles hit that: Filled's base is
    /// `SurfaceContainerHighest`, Elevated's base is `Surface` itself. (Outlined is not affected:
    /// its `disabledContainerColor` IS its `ContainerColor`.)
    ///
    /// `Color::overlay` is exactly that src-over composite; `surface.rs:29` uses the same form.
    pub fn from_theme(theme: &crate::theme::ThemeColors, style: CardStyle) -> Self {
        use crate::graphics::Color;
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        match style {
            // FilledCardTokens: Container = SurfaceContainerHighest,
            // Disabled = SurfaceVariant @0.38 compositeOver SurfaceContainerHighest.
            CardStyle::Filled => Self::new(
                theme.surface_container_highest,
                theme.on_surface,
                theme
                    .surface_container_highest
                    .overlay(theme.surface_variant, 0.38),
                alpha(theme.on_surface, 0.38),
            ),
            // ElevatedCardTokens: Container = SurfaceContainerLow.
            // Disabled = DisabledContainerColor(Surface) @0.38 compositeOver(Surface), and compositing
            // a surface tint over an opaque surface of the SAME colour returns that colour — so the
            // disabled container is the plain, fully opaque `Surface`. That is a real change from
            // SurfaceContainerLow, not "unchanged": the two are different scheme roles.
            //
            // Written as the exact result rather than the repo's usual `alpha(.., 0.38)` shortcut
            // (FilledCard above uses it): the shortcut would leave a TRANSLUCENT card that shows
            // whatever sits behind it, where Compose gets an opaque one.
            CardStyle::Elevated => Self::new(
                theme.surface_container_low,
                theme.on_surface,
                theme.surface,
                alpha(theme.on_surface, 0.38),
            ),
            // OutlinedCardTokens: Container=Surface, and the disabled container stays unchanged
            CardStyle::Outlined => Self::new(
                theme.surface,
                theme.on_surface,
                theme.surface,
                alpha(theme.on_surface, 0.38),
            ),
        }
    }
}

/// Card shadow elevation (the material3 `CardElevation` counterpart — six states: default/pressed/
/// focused/hovered/dragged/disabled), in DP, resolved from the token LEVELS in
/// `material3/tokens/ElevationTokens.kt:24-29`: Level0 0, Level1 1, Level2 3, Level3 6, Level4 8 dp. winia
/// used to carry the level INDEX here, which is the same number only for Level0 and Level1.
/// - `card_elevation()` (Filled): 0/0/0/1/6/0
/// - `elevated_card_elevation()` (Elevated): 1/1/1/3/8/1
/// - `outlined_card_elevation()` (Outlined): 0/0/0/1/6/0
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub dragged: f32,
    pub disabled: f32,
}

impl CardElevation {
    pub fn new(
        default: f32,
        pressed: f32,
        focused: f32,
        hovered: f32,
        dragged: f32,
        disabled: f32,
    ) -> Self {
        Self {
            default,
            pressed,
            focused,
            hovered,
            dragged,
            disabled,
        }
    }

    /// FilledCard defaults from `FilledCardTokens`, in dp: `ContainerElevation` / `PressedContainerElevation`
    /// / `FocusContainerElevation` / `DisabledContainerElevation` are `ElevationTokens.Level0` (0),
    /// `HoverContainerElevation` is `Level1` (1) and `DraggedContainerElevation` is `Level3` — 6 dp, not 3.
    /// winia stored the level index, so a dragged Filled card lifted half as far as M3's.
    pub fn default_elevation() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0, 6.0, 0.0)
    }

    /// ElevatedCard defaults from `ElevatedCardTokens`, in dp: `ContainerElevation`, `PressedContainerElevation`,
    /// `FocusContainerElevation` and `DisabledContainerElevation` are `ElevationTokens.Level1` (1),
    /// `HoverContainerElevation` is `Level2` (**3**) and `DraggedContainerElevation` is `Level4` (**8**).
    pub fn elevated() -> Self {
        Self::new(1.0, 1.0, 1.0, 3.0, 8.0, 1.0)
    }

    /// An OutlinedCard is flat at rest and lifts 1 dp on HOVER and 6 dp while DRAGGED — `OutlinedCardTokens`:
    /// `ContainerElevation`/`PressedContainerElevation`/`FocusContainerElevation`/`DisabledContainerElevation`
    /// are `ElevationTokens.Level0`, `HoverContainerElevation` is `Level1` (1) and `DraggedContainerElevation`
    /// is `Level3` (6). winia carried hovered 0 here, which read as "an outlined card only ever moves when
    /// dragged", and the level index (3) for the drag.
    pub fn outlined() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0, 6.0, 0.0)
    }

    /// Whether every state's elevation is 0 — the card then has nothing to animate and no shadow layer to pay
    /// for. Note that a style's REST elevation being 0 does NOT make it shadow-free: Filled is 0 at rest and
    /// still 1 on hover and 3 while dragged.
    pub fn is_shadow_free(&self) -> bool {
        self.default <= 0.0
            && self.pressed <= 0.0
            && self.focused <= 0.0
            && self.hovered <= 0.0
            && self.dragged <= 0.0
            && self.disabled <= 0.0
    }

    /// The elevation for a state snapshot.
    ///
    /// Compose resolves it from the LAST interaction the card received (`material3/Card.kt:705-719`:
    /// `val interaction = interactions.lastOrNull()` then a `when` over it) — not from a fixed priority, and
    /// not from a maximum. So a drag wins over the press that started it (which is why dragging shows the
    /// `dragged` elevation at all), and a press wins over the hover it followed, which is M3's deliberate
    /// "an M3 button and card FLATTEN when pressed": `PressedContainerElevation` is below
    /// `HoverContainerElevation` in every token set (Filled card: 0 against 1; Elevated card: 1 against 2).
    ///
    /// This used to rank `pressed` above `dragged` and to take `max(pressed, hovered)`, which made the
    /// `dragged` elevation unreachable with any real pointer (a drag always holds a press) and made a pressed
    /// card keep its hover height instead of flattening. The state order below is the same "latest wins"
    /// ordering Compose gets from its interaction list.
    pub fn for_state(&self, state: &ComponentState) -> f32 {
        if !state.enabled {
            self.disabled
        } else if state.dragged {
            // A drag is always the latest interaction: it began with a press and outlives it.
            self.dragged
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

impl Default for CardElevation {
    fn default() -> Self {
        Self::default_elevation()
    }
}

/// A card border (the material3 `BorderStroke` counterpart) — a width plus a colour; the shape
/// comes from [`Card::shape`] (the border is drawn along the container's shape edge).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardBorder {
    pub width: f32,
    pub color: Color,
}

impl CardBorder {
    pub fn new(width: f32, color: Color) -> Self {
        Self { width, color }
    }

    /// The OutlinedCard default border (the material3 counterpart: 1dp + `OutlineVariant`; when
    /// disabled, Outline @ DisabledOutlineOpacity(0.12) over SurfaceContainerLow)
    fn outlined(theme: &crate::theme::ThemeColors, enabled: bool) -> Self {
        let color = if enabled {
            theme.outline_variant
        } else {
            // OutlinedCardTokens.DisabledOutlineColor = Outline @ 12% over the container
            let c = theme.outline;
            Color::from_argb((c.a as f32 * 0.12) as u8, c.r, c.g, c.b)
        };
        Self::new(1.0, color)
    }
}

/// The Card defaults (the material3 `CardDefaults` counterpart)
pub struct CardDefaults;

impl CardDefaults {
    /// The default shape: `CardTokens.ContainerShape` = `CornerMedium` = 12dp (`ShapeTokens.kt:70`:
    /// `val CornerMedium = RoundedCornerShape(12.0.dp)`).
    ///
    /// This said 8 dp for a long time, and the comment claimed `CornerMedium` while the number was
    /// a different token's — the same 8/12 mix-up `icon_button`'s `CornerSmall` (8) and
    /// `CornerMedium` (12) make easy to hit.
    pub fn shape() -> Shape {
        Shape::rounded(12.0)
    }

    /// The default colours (the material3 CardDefaults.cardColors counterpart — derived per style)
    pub fn card_colors(theme: &crate::theme::ThemeColors, style: CardStyle) -> CardColors {
        CardColors::from_theme(theme, style)
    }

    /// FilledCard defaults (`FilledCardTokens`): 0/0/0/1/6/0 dp.
    pub fn card_elevation() -> CardElevation {
        CardElevation::default_elevation()
    }

    /// ElevatedCard defaults (`ElevatedCardTokens`): 1/1/1/3/8/1 dp.
    pub fn elevated_card_elevation() -> CardElevation {
        CardElevation::elevated()
    }

    /// OutlinedCard defaults (`OutlinedCardTokens`): 0/0/0/1/6/0 dp.
    pub fn outlined_card_elevation() -> CardElevation {
        CardElevation::outlined()
    }

    /// The OutlinedCard default border (M3 OutlinedCardTokens: 1dp OutlineVariant)
    pub fn outlined_border(theme: &crate::theme::ThemeColors, enabled: bool) -> CardBorder {
        CardBorder::outlined(theme, enabled)
    }
}

// ═══════════════════════════════════════════════════════════
// The Card component
// ═══════════════════════════════════════════════════════════

/// The card component (the counterpart of material3's Card family).
///
/// Without `on_click` it is a display-only card (not interactive, no ripple); with `on_click`
/// set it is clickable and gets the ripple (the counterpart of M3's clickable Card).
pub struct Card {
    /// The click callback (None = a display-only card)
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Whether the card is enabled
    enabled: bool,
    /// The card style (Filled / Elevated / Outlined)
    style: CardStyle,
    /// The colour set (None = the style's default, derived from the theme)
    colors: Option<CardColors>,
    /// The interaction source (None = remembered internally in `build` — the Compose counterpart
    /// of an optional injection)
    interaction_source: Option<MutableInteractionSource>,
    /// The shadow elevation (None = the style's default)
    elevation: Option<CardElevation>,
    /// The container/border/shadow shape (the material3 `Card(shape = ...)` counterpart — 12dp
    /// rounded by default)
    shape: Shape,
    /// The border (None = the style's default — Outlined has a 1dp theme-coloured border, the
    /// other styles have none)
    border: Option<CardBorder>,
    /// The modifier chain
    modifier: Modifier,
}

impl Card {
    /// Creates a new Card component (Filled style by default)
    pub fn new() -> Self {
        Card {
            on_click: None,
            enabled: true,
            style: CardStyle::default(),
            colors: None,
            interaction_source: None,
            elevation: None,
            shape: CardDefaults::shape(),
            border: None,
            modifier: Modifier::new(),
        }
    }

    /// A filled card (the material3 `Card` counterpart) — the default, Filled style
    pub fn filled() -> Self {
        Self::new()
    }

    /// An elevated card (the material3 `ElevatedCard` counterpart) — Elevated style + the default
    /// shadow
    pub fn elevated() -> Self {
        Self::new()
            .style(CardStyle::Elevated)
            .elevation(CardElevation::elevated())
    }

    /// An outlined card (the material3 `OutlinedCard` counterpart) — Surface container + 1dp
    /// outline border
    pub fn outlined() -> Self {
        Self::new().style(CardStyle::Outlined)
    }

    /// Sets the click callback (None = display-only; once set the card is clickable and ripples)
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    /// Sets the enabled state
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the card style
    pub fn style(mut self, style: CardStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the colour set (overriding the theme default — the material3 `Card(colors = ...)`
    /// counterpart)
    pub fn colors(mut self, colors: CardColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Injects an interaction source (a hoist — the card's press/hover/focus states are emitted
    /// into this source; when not passed, one is remembered internally)
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// Sets the shadow elevation (see [`CardElevation::for_state`] for the per-state values)
    pub fn elevation(mut self, elevation: CardElevation) -> Self {
        self.elevation = Some(elevation);
        self
    }

    /// Sets the container/border/shadow shape (the material3 `Card(shape = ...)` counterpart —
    /// 12dp rounded by default)
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    /// Sets the border (overriding the style default — Outlined's default is 1dp outline colour)
    pub fn border(mut self, border: CardBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// The modifier chain (size, padding, … — appended on the outside)
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Builds the card. The content is wrapped in a top-aligned Column (the M3
    /// Surface { Column } counterpart).
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // Stage the parameters (equal params skip — style/enabled unchanged → the container Skips)
        ctx.changed(&self.style);
        ctx.changed(&self.enabled);
        let key = ctx.next_key();
        let theme = crate::theme::WiniaTheme::colors();
        // The default colours all come from CardDefaults (the material3
        // CardDefaults.cardColors counterpart)
        let colors = self
            .colors
            .unwrap_or_else(|| CardDefaults::card_colors(&theme, self.style));
        // The interaction source: injected from outside or remembered internally (the counterpart
        // of the Compose Card's interactionSource parameter)
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(MutableInteractionSource::new).get());
        // State-aware colour lookup (registering the dependency — press/hover/focus changes
        // recompose on their own)
        let state = interaction.state(self.enabled);
        let container = colors.container_color_for(&state);
        let content_color = colors.content_color_for(&state);
        // The shadow elevation: state changes (a hover/focus/press entering and leaving) go
        // through an animated State, so the shadow rises gradually while hovering/pressing and
        // settles back after the pointer leaves, instead of jumping.
        // The spec is Compose's, and Compose keys it on the interaction PAIR
        // (`animation::elevation_spec_for_pair`, from `material3/internal/Elevation.kt:50-61`), so the build
        // remembers the interaction kind the last target came from in a backchannel — reading it registers no
        // dependency, and `(None, None)` (idle to idle) is Compose's snap. This used one 180 ms `EaseOutCubic`
        // for every direction.
        let last_kind: crate::runtime::state::Backchannel<
            Option<crate::animation::ElevationInteraction>,
        > = ctx.remember_backchannel(|| None);
        // The elevation is the caller's when there is one, otherwise the style's own token set. Compose
        // ALWAYS has an elevation — `CardDefaults.cardElevation()` for a Filled card is
        // `0 / 0 / 0 / 1 / 6 / 0` dp, not "nothing" — so a style whose REST elevation happens to be 0 still
        // needs the animated layer: hovering raises it to 1 and dragging to 6 (`FilledCardTokens`). winia stored
        // `None` for Filled and Outlined and then skipped the layer entirely, so those cards could never show a
        // shadow at all — a dragged Filled card moved and nothing else happened.
        let elevation = self.elevation.unwrap_or_else(|| match self.style {
            CardStyle::Filled => CardDefaults::card_elevation(),
            CardStyle::Elevated => CardDefaults::elevated_card_elevation(),
            CardStyle::Outlined => CardDefaults::outlined_card_elevation(),
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
            let spec = crate::animation::elevation_spec_for_pair(from, to).unwrap_or(
                crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()),
            );
            last_kind.set(to);
            Some(ctx.animate_float_as_state(target, spec))
        };

        let shape = self.shape;
        // The container: all three styles have an opaque container colour
        // (Filled=SurfaceContainerHighest, Elevated=SurfaceContainerLow, Outlined=Surface — M3 has
        // no card variant with a transparent background)
        let mut modifier = Modifier::new().background(container, shape);
        // The border: an explicit border wins, otherwise Outlined's default 1dp outline colour
        // (the counterpart of material3's OutlinedCard =
        // Card(border = BorderStroke(1.dp, OutlineVariant)))
        let border = self.border.or_else(|| {
            if self.style == CardStyle::Outlined {
                Some(CardBorder::outlined(&theme, self.enabled))
            } else {
                None
            }
        });
        if let Some(b) = border {
            modifier = modifier.border(b.width, b.color, shape);
        }

        // The shadow is rendered through a dynamic graphics_layer closure (shadow_elevation reads
        // the animated value every frame, so evaluating it at render time triggers no
        // recomposition; the counterpart of Compose's layer shadow semantics; the shape follows
        // the Card shape)
        if let Some(anim) = elevation_anim {
            modifier = modifier.graphics_layer(move || crate::graphics::GraphicsLayerParams {
                shadow_elevation: anim.peek(),
                shadow_shape: Some(shape),
                ..Default::default()
            });
        }

        // Accessibility: a card that takes clicks is a BUTTON to a screen reader, and whether it is
        // enabled is part of what it reports. Declared BEFORE the caller's modifier is appended, so
        // a caller's own `semantics(..)` merges over this one per field while keeping the role
        // (`docs/component-authoring.md` §9).
        //
        // Only the clickable form declares anything: Compose's non-clickable `Card` is a plain
        // `Surface`, and §9's second rule is that a node declaring nothing stays transparent in the
        // tree. Supplying a role there would put a nameless button in front of every static card.
        //
        // `merge_descendants(true)` for the same reason `Button` sets it: the card IS one control,
        // so its label must be read as this node's name rather than as a separate child.
        if self.on_click.is_some() {
            modifier = modifier.semantics(
                crate::semantics::SemanticsConfig::new()
                    .role(crate::semantics::SemanticsRole::Button)
                    .merge_descendants(true)
                    .state(crate::semantics::SemanticsState::new().enabled(self.enabled)),
            );
        }

        // Append the caller's modifier (on the outside, so it can override the default styling)
        modifier = modifier.then(self.modifier);

        if self.enabled
            && let Some(on_click) = &self.on_click
        {
            let cb = on_click.clone();
            // clickable (press/focus/hover interaction) + the ripple (a Compose clickable's
            // default `indication=ripple`; the colour is the content colour)
            modifier = modifier
                .clickable_with_source(&interaction, move || cb())
                // Pass the container shape explicitly — the ripple is clipped to the container
                // shape
                .ripple_with_shape(&interaction, content_color, true, shape);
        }

        // The content closure becomes a composition scope of its own (as it does with Column); the
        // layout strategy = ColumnLayout (the M3 counterpart: the content is wrapped in a Column,
        // top-aligned)
        //
        // A CLICKABLE card gets Compose's 48 dp touch target: its `onClick` overload goes through
        // `Surface(onClick = …)` (`material3/Card.kt:146-156`), whose Box applies
        // `Modifier.minimumInteractiveComponentSize()` (`material3/Surface.kt:219`). A card that takes no
        // clicks has no such minimum in Compose, and does not get one here either. The wrapper is a node of
        // its own for the reason the Button's is: winia decides a chain's size across the whole chain, so a
        // minimum inside the card's chain would fight its own background and padding. It also carries the
        // ring's shape and colour, since it is the node that takes focus.
        let build_card = |ctx: &mut ComposeCtx, modifier: Modifier| {
            let dir = modifier
                .get_layout_direction()
                .unwrap_or(crate::layout::direction::current());
            match ctx.start_restartable_group(
                key,
                modifier,
                crate::layout::ColumnLayout::new().direction(dir),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // The content colour is passed down (the `LocalContentColor` equivalent) so an `Icon`
                    // with tint Auto picks up the card's own content colour: `onSurface` for all three
                    // styles.
                    crate::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                        content(ctx);
                    });
                }
            }
            // The focus ring's colour is the theme's `secondary` — M3's `CardTokens.FocusIndicatorColor`,
            // deliberately different from Button's `primary`. Captured at compose time into the node's desc,
            // because the theme's CompositionLocal is gone by render time.
            ctx.set_current_node_focus_color(theme.secondary);
            ctx.end_restartable_group();
        };
        if self.on_click.is_some() {
            // The wrapper IS the node the parent measures, so it takes the parent-data half of the
            // caller's chain (`weight`, `align`, …) while the visual half stays on the inner
            // surface. Compose gets this for free: `Modifier.weight` is a `ParentDataModifierNode`
            // that writes `RowColumnParentData` onto the resulting `Placeable`
            // (`foundation-layout/RowColumnImpl.kt:485-492`), so the Row sees it through any number
            // of nested nodes. winia asks the DIRECT child's modifier instead, which is why a
            // weighted clickable card measured 48 dp rather than sharing its row (measured).
            let (parent_data, visual) = modifier.split_parent_data();
            let wrapper = parent_data.then(
                Modifier::new().minimum_interactive_component_size().border(
                    0.0,
                    crate::graphics::Color::from_argb(0, 0, 0, 0),
                    shape,
                ),
            );
            crate::layout::components::build_container(
                ctx,
                wrapper,
                // `propagate_min_constraints`: the same reason the Button's wrapper sets it — a
                // parent that FORCES a width on the card gets a card that width. Without it the
                // wrapper reports the forced size while the surface inside it keeps hugging its own
                // content (a BoxLayout loosens its children otherwise), so the ring, the ripple and
                // the surface no longer agree on how wide the card is.
                crate::layout::BoxLayout::new()
                    .alignment(crate::layout::Alignment::Center)
                    .propagate_min_constraints(true),
                |ctx| {
                    build_card(ctx, visual);
                    // The wrapper is the focus node, so the ring's colour has to reach it too.
                    ctx.set_current_node_focus_color(theme.secondary);
                },
            );
        } else {
            // No wrapper: the card's own node IS the direct child, so its chain stays whole.
            build_card(ctx, modifier);
        }
    }

    // ── Getters (tests) ──
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_style(&self) -> CardStyle {
        self.style
    }
    pub fn get_colors(&self) -> Option<CardColors> {
        self.colors
    }
    pub fn get_elevation(&self) -> Option<CardElevation> {
        self.elevation
    }
    pub fn get_shape(&self) -> Shape {
        self.shape
    }
    pub fn get_border(&self) -> Option<CardBorder> {
        self.border
    }
    pub fn get_modifier(&self) -> &Modifier {
        &self.modifier
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// A clickable card honours `RowScope::weight`, exactly as a plain one does.
    ///
    /// `weight` is Compose's `ParentDataModifierNode`: `LayoutWeightNode` writes
    /// `RowColumnParentData` onto the resulting `Placeable`
    /// (`foundation-layout/RowColumnImpl.kt:485-492`), so a `Row` reads it through any number of
    /// nodes nested in between. winia asks the DIRECT child's modifier, and a clickable card inserts
    /// its own 48 dp wrapper, so the caller's chain has to be split with
    /// `Modifier::split_parent_data` or the weight lands one level too deep and the card falls back
    /// to its touch target (measured: three weighted cards at 48 dp each instead of sharing 600).
    #[test]
    fn a_clickable_card_honours_a_row_weight() {
        let widths = |clickable: bool| -> Vec<f32> {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                crate::layout::components::Row::new()
                    .spacing(12.0)
                    .build(ctx, |row| {
                        for _ in 0..3 {
                            // `weight` is a `RowScope` method: it hands back the modifier this child
                            // needs, and the scope derefs to the ctx the card builds with.
                            let share = row.weight(1.0);
                            let card = Card::filled();
                            let card = if clickable {
                                card.on_click(|| {})
                            } else {
                                card
                            };
                            card.modifier(share).build(row, |ctx| {
                                crate::components::Text::new("Card")
                                    .modifier(Modifier::new().padding(16.0))
                                    .build(ctx);
                            });
                        }
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 600.0, 0.0, 400.0));
            composer
                .arena_nodes()
                .iter()
                .map(|n| n.measured_size.width)
                .collect()
        };

        // Three equal shares of 600 minus two 12 dp gaps: each is 192.
        let expected = (600.0 - 24.0) / 3.0;
        let shares = |clickable: bool| -> usize {
            let widths = widths(clickable);
            let n = widths.iter().filter(|w| (**w - expected).abs() < 0.5).count();
            assert!(
                widths.iter().all(|w| (*w - 48.0).abs() > 0.5 || expected <= 48.0),
                "no card may fall back to its 48 dp touch target when it has a weight \
                 (clickable: {clickable}, all widths: {widths:?})"
            );
            n
        };
        // A plain card contributes one node at its share; a clickable one contributes two — the
        // wrapper and the surface inside it, which `propagate_min_constraints` widens to match.
        assert_eq!(
            shares(false), 3,
            "a plain weighted card is one node, at its share"
        );
        assert_eq!(
            shares(true), 6,
            "a clickable weighted card is the wrapper AND the surface inside it, both at its share \
             — one node here means the wrapper took the weight and the surface kept its own width"
        );
    }

    /// Compose's CLICKABLE card carries a 48 dp touch target, because the `onClick` overload is built on
    /// `Surface(onClick = …)` (`material3/Card.kt:146-156`) whose Box applies
    /// `Modifier.minimumInteractiveComponentSize()` (`material3/Surface.kt:219`, the node at
    /// `material3/InteractiveComponentSize.kt:98-140`): the box becomes `max(content, 48)` with the content
    /// centred. A card with no `on_click` has no such minimum in Compose — it is asserted here as the control.
    #[test]
    fn a_clickable_card_reserves_a_48_dp_touch_target() {
        let heights = |clickable: bool| -> Vec<f32> {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        let card = Card::filled();
                        let card = if clickable {
                            card.on_click(|| {})
                        } else {
                            card
                        };
                        card.build(ctx, |ctx| {
                            crate::components::Text::new("Card")
                                .modifier(Modifier::new().padding(8.0))
                                .build(ctx);
                        });
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let nodes = composer.arena_nodes();
            nodes.iter().map(|n| n.measured_size.height).collect()
        };
        let clickable = heights(true);
        assert!(
            clickable.contains(&48.0),
            "the clickable card reserves the 48 dp touch target ({clickable:?})"
        );
        let plain = heights(false);
        assert!(
            !plain.contains(&48.0),
            "and a card that takes no clicks does not, as in Compose ({plain:?})"
        );
    }

    /// A card that takes clicks announces itself as a BUTTON and reports whether it is enabled;
    /// a card that takes none declares nothing at all.
    ///
    /// Compose's clickable `Card` is a `Surface(onClick = …)`, whose semantics come from the
    /// `clickable` modifier; the non-clickable `Card` is a plain `Surface` and declares nothing.
    /// `docs/component-authoring.md` §9 wants the role and the state declared BEFORE the caller's
    /// modifier is appended, and a node that declares nothing to stay transparent in the tree —
    /// both halves are asserted here, because either one alone passes for the wrong reason.
    #[test]
    fn a_clickable_card_declares_the_button_role_and_its_enabled_state() {
        let declare = |clickable: bool, enabled: bool| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                let card = Card::filled().enabled(enabled);
                let card = if clickable {
                    card.on_click(|| {})
                } else {
                    card
                };
                card.build(ctx, |ctx| {
                    crate::components::Text::new("Card").build(ctx);
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            composer
                .arena_nodes()
                .iter()
                .map(|node| node.modifier.semantics_config())
                .find(|config| config.role_value().is_some())
        };

        let clickable = declare(true, true).expect("the clickable card declares a role");
        assert_eq!(
            clickable.role_value(),
            Some(crate::semantics::SemanticsRole::Button),
            "a card that takes clicks is a button to a screen reader"
        );
        assert_eq!(
            clickable.state_value().enabled_value(),
            Some(true),
            "and it reports that it is enabled"
        );
        assert!(
            clickable.merges_descendants(),
            "the card is ONE control, so its label must be read as this node's name rather than as \
             a separate child"
        );

        let disabled = declare(true, false).expect("a disabled clickable card keeps its role");
        assert_eq!(
            disabled.state_value().enabled_value(),
            Some(false),
            "a disabled card keeps its role AND reports the disabled state — dropping either would \
             leave a nameless, unreachable control"
        );

        assert!(
            declare(false, true).is_none(),
            "a card that takes no clicks declares nothing, so it stays transparent in the tree"
        );
    }

    /// The elevation spec keys on the PAIR of interactions, as Compose's `ElevationDefaults` does
    /// (`material3/internal/Elevation.kt:50-61`, `:113-117`) — the same rule the Button uses. Comparing VALUES
    /// instead (window's previous form) got two reachable cases wrong: a hover followed by a press took the
    /// hovered curve where Compose takes `FastOutSlowIn`, and a release that leaves the pointer hovering was
    /// read as an outgoing change although `HoverInteraction.Enter` is still the latest interaction.
    #[test]
    fn the_card_elevation_spec_keys_on_the_interaction_pair() {
        use crate::animation::ElevationInteraction as I;
        let millis =
            |from: Option<I>, to: Option<I>| match crate::animation::elevation_spec_for_pair(
                from, to,
            ) {
                Some(crate::animation::AnimationSpec::Tween(t)) => t.duration.as_millis(),
                Some(other) => panic!("an elevation animation is a tween, got {other:?}"),
                None => 0,
            };
        assert_eq!(
            millis(None, Some(I::Hover)),
            120,
            "entering hover is the incoming 120 ms"
        );
        assert_eq!(
            millis(Some(I::Hover), Some(I::Press)),
            120,
            "hover to press is incoming too"
        );
        assert_eq!(
            millis(Some(I::Press), Some(I::Hover)),
            120,
            "releasing into a hover is still an incoming interaction"
        );
        assert_eq!(
            millis(Some(I::Press), None),
            150,
            "back to rest is the outgoing 150 ms"
        );
        assert_eq!(millis(Some(I::Focus), None), 150, "and so is leaving focus");
        assert_eq!(
            millis(Some(I::Hover), None),
            120,
            "leaving hover is shorter"
        );
        assert_eq!(millis(None, None), 0, "idle to idle snaps, as Compose does");
    }

    #[test]
    fn test_card_variant_constructors() {
        let filled = Card::new();
        assert_eq!(filled.get_style(), CardStyle::Filled);
        assert_eq!(filled.get_elevation(), None, "Filled has no default shadow");

        let elevated = Card::elevated();
        assert_eq!(elevated.get_style(), CardStyle::Elevated);
        assert_eq!(elevated.get_elevation(), Some(CardElevation::elevated()));

        let outlined = Card::outlined();
        assert_eq!(outlined.get_style(), CardStyle::Outlined);
        assert_eq!(
            outlined.get_elevation(),
            None,
            "None means \"use the style's token default\", which for Outlined is 0/0/0/1/6/0 dp"
        );
    }

    #[test]
    fn test_card_default_shape_is_corner_medium() {
        // `CornerMedium` is 12 dp (`ShapeTokens.kt:70`), not 8: the 8 dp in this assertion was the
        // `CornerSmall` token, which is what `IconButton`'s smallest `Square` uses.
        assert_eq!(CardDefaults::shape(), Shape::rounded(12.0));
    }

    #[test]
    fn test_card_colors_match_m3_tokens() {
        let theme = crate::theme::ThemeColors::default_light();
        // Filled: SurfaceContainerHighest container + OnSurface content
        let filled = CardDefaults::card_colors(&theme, CardStyle::Filled);
        assert_eq!(filled.container, theme.surface_container_highest);
        assert_eq!(filled.content, theme.on_surface);
        // `SurfaceVariant @0.38 compositeOver SurfaceContainerHighest` (`Card.kt:507-511`). The
        // point of computing it rather than multiplying the alpha: the result is OPAQUE, because
        // the enabled container it composites onto is opaque. The old assertion here pinned
        // `a == surface_variant.a * 0.38`, which is what a translucent card looks like — it passed
        // precisely because it encoded the shortcut instead of the formula.
        assert_eq!(
            filled.disabled_container,
            theme
                .surface_container_highest
                .overlay(theme.surface_variant, 0.38),
            "the Filled disabled container = SurfaceVariant@38% compositeOver \
             SurfaceContainerHighest"
        );
        assert_eq!(
            filled.disabled_container.a, 255,
            "the composite lands on an opaque container, so the disabled card is opaque — an alpha \
             shortcut would let whatever sits behind it show through"
        );
        assert_ne!(
            filled.disabled_container, filled.container,
            "a disabled Filled card does change colour"
        );
        assert_eq!(
            filled.disabled_content.a,
            (theme.on_surface.a as f32 * 0.38) as u8
        );
        // Elevated: SurfaceContainerLow container; the disabled container is an **opaque Surface**
        //
        // Compose computes it as `Surface @0.38 compositeOver Surface`, which returns the plain
        // opaque `Surface` — a DIFFERENT scheme role from the enabled container, so the card really
        // does change colour when disabled. This used to assert `disabled_container == container`,
        // which is the reading that made the mismatch invisible.
        let elevated = CardDefaults::card_colors(&theme, CardStyle::Elevated);
        assert_eq!(elevated.container, theme.surface_container_low);
        assert_eq!(elevated.disabled_container, theme.surface);
        assert_ne!(
            theme.surface, theme.surface_container_low,
            "the assertion above only means something while these two roles differ"
        );
        assert_eq!(
            elevated.disabled_container.a, 255,
            "compositing the tint over an opaque surface leaves it opaque — the alpha shortcut would \
             make the disabled card show whatever is behind it"
        );
        // Outlined: Surface container, and the disabled container stays as it is
        let outlined = CardDefaults::card_colors(&theme, CardStyle::Outlined);
        assert_eq!(outlined.container, theme.surface);
        assert_eq!(outlined.disabled_container, outlined.container);
    }

    #[test]
    fn test_card_colors_state_resolution() {
        let colors = CardColors::new(
            Color::RED,
            Color::WHITE,
            Color::from_argb(100, 100, 100, 100),
            Color::from_argb(80, 200, 200, 200),
        );
        // disabled wins
        assert_eq!(
            colors.container_color_for(&ComponentState::disabled()),
            colors.disabled_container
        );
        // The container colour tells enabled and disabled apart only (as material3's CardColors
        // does — the hover/focus/press state layer is drawn by the ripple indication and never
        // stacks onto the container colour)
        assert_eq!(
            colors.container_color_for(&ComponentState::idle()),
            colors.container
        );
        assert_eq!(
            colors.container_color_for(&ComponentState {
                hovered: true,
                pressed: true,
                ..ComponentState::idle()
            }),
            colors.container,
        );
        // The content colour tells enabled and disabled apart only
        assert_eq!(
            colors.content_color_for(&ComponentState::idle()),
            colors.content
        );
        assert_eq!(
            colors.content_color_for(&ComponentState::disabled()),
            colors.disabled_content
        );
    }

    /// A Filled card has no shadow at REST — and Compose still gives it an elevation, so hovering raises it to 1
    /// and dragging to 3 (`FilledCardTokens.HoverContainerElevation` / `DraggedContainerElevation`). winia stored
    /// `None` for Filled and Outlined and then skipped the layer, so those two styles could never show a shadow
    /// at all: on the demo a dragged Filled card moved and nothing else happened, which is what the user hit.
    ///
    /// This drives the card's own interaction source (`DragInteraction.Start`) and reads the elevation the
    /// graphics layer would paint. The control is the same card at rest.
    #[test]
    fn a_dragged_filled_card_raises_the_shadow_it_paints() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let source = MutableInteractionSource::new();
        let mut compose_card = |composer: &mut crate::runtime::composer::Composer| {
            let src = source.clone();
            composer.compose(|ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Card::filled()
                            .interaction_source(src.clone())
                            .build(ctx, |ctx| {
                                crate::components::Text::new("card")
                                    .modifier(Modifier::new().padding(8.0))
                                    .build(ctx);
                            });
                    });
            });
        };
        let painted_shadow = |composer: &crate::runtime::composer::Composer| -> Option<f32> {
            composer.arena_nodes().iter().find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::GraphicsLayer { params } => {
                        Some(params.eval().shadow_elevation)
                    }
                    _ => None,
                })
            })
        };
        let constraints = crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0);
        let mut composer = crate::runtime::composer::Composer::new();

        compose_card(&mut composer);
        composer.layout(constraints);
        let at_rest = painted_shadow(&composer)
            .expect("a Filled card carries an elevation layer even though its rest elevation is 0");
        assert_eq!(at_rest, 0.0, "and it paints no shadow at rest");

        source.emit_drag_start();
        compose_card(&mut composer);
        composer.layout(constraints);
        // Real frames, not a tight loop: the animation integrates over the time since the last update, so a
        // tight loop would step it by ~0.
        // Assert that the animation is registered rather than counting real frames: under load a step
        // can take long enough to reach the target in a frame or two, which is what made
        // `frames > 5` fail intermittently in a full run while passing when run alone.
        assert!(
            crate::animation::is_animating(),
            "dragging registers an elevation animation"
        );
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let dragged = painted_shadow(&composer).expect("still there");
        assert!(
            (dragged - 6.0).abs() < 0.01,
            "dragging raises the shadow to `DraggedContainerElevation` (Level3 = 6 dp), got {dragged}"
        );
    }

    #[test]
    fn test_card_elevation_defaults() {
        // M3 1.4.0 tokens, in DP: `ElevationTokens` maps Level1 -> 1, Level2 -> 3, Level3 -> 6, Level4 -> 8.
        // Carrying the LEVEL INDEX here is what made a dragged card lift 3 dp instead of 6 on a Filled style
        // and 4 instead of 8 on an Elevated one.
        assert_eq!(
            CardElevation::default_elevation(),
            CardElevation::new(0.0, 0.0, 0.0, 1.0, 6.0, 0.0)
        );
        assert_eq!(
            CardElevation::elevated(),
            CardElevation::new(1.0, 1.0, 1.0, 3.0, 8.0, 1.0)
        );
        // `OutlinedCardTokens.HoverContainerElevation = Level1` — an outlined card lifts on hover too.
        assert_eq!(
            CardElevation::outlined(),
            CardElevation::new(0.0, 0.0, 0.0, 1.0, 6.0, 0.0)
        );
        assert_eq!(
            CardDefaults::card_elevation(),
            CardElevation::default_elevation()
        );
        assert_eq!(
            CardDefaults::elevated_card_elevation(),
            CardElevation::elevated()
        );
        assert_eq!(
            CardDefaults::outlined_card_elevation(),
            CardElevation::outlined()
        );
    }

    #[test]
    fn test_card_elevation_priority() {
        let e = CardElevation::new(1.0, 8.0, 2.0, 2.0, 5.0, 0.0);
        assert_eq!(e.for_state(&ComponentState::disabled()), 0.0);
        assert_eq!(e.for_state(&ComponentState::idle()), 1.0);
        assert_eq!(
            e.for_state(&ComponentState {
                pressed: true,
                ..ComponentState::idle()
            }),
            8.0
        );
        assert_eq!(
            e.for_state(&ComponentState {
                hovered: true,
                ..ComponentState::idle()
            }),
            2.0
        );
        assert_eq!(
            e.for_state(&ComponentState {
                focused: true,
                ..ComponentState::idle()
            }),
            2.0
        );
        // Order: disabled > dragged > pressed > hovered > focused > default (latest interaction wins)
        assert_eq!(
            e.for_state(&ComponentState {
                pressed: true,
                hovered: true,
                ..ComponentState::idle()
            }),
            8.0
        );
        // Compose takes the elevation of the LATEST interaction (`Card.kt:705-719`), so a drag outranks the
        // press that started it …
        assert_eq!(
            e.for_state(&ComponentState {
                dragged: true,
                ..ComponentState::idle()
            }),
            5.0
        );
        assert_eq!(
            e.for_state(&ComponentState {
                dragged: true,
                pressed: true,
                ..ComponentState::idle()
            }),
            5.0,
            "a drag is always later than its own press, and it is what the `dragged` token is for"
        );
        // … and a press outranks the hover it followed, because M3 flattens on press (`PressedContainerElevation`
        // sits below `HoverContainerElevation` in every token set).
        let low_press = CardElevation::new(1.0, 1.0, 1.0, 3.0, 5.0, 0.0);
        assert_eq!(
            low_press.for_state(&ComponentState {
                pressed: true,
                hovered: true,
                ..ComponentState::idle()
            }),
            1.0,
            "pressed, not the hover it followed — this used to take max(pressed, hovered)"
        );
        // The real Elevated tokens (1/1/1/2/4/1): a press flattens to 1 where the hover above it was 2 — the
        // M3 press feedback — and a drag carries the card up to 4.
        assert_eq!(
            CardElevation::elevated().for_state(&ComponentState {
                pressed: true,
                ..ComponentState::idle()
            }),
            1.0,
            "pressed flattens below hover, as the tokens say"
        );
        assert_eq!(
            CardElevation::elevated().for_state(&ComponentState {
                pressed: true,
                hovered: true,
                ..ComponentState::idle()
            }),
            1.0,
            "and hovering before the press does not keep it up"
        );
        assert_eq!(
            CardElevation::elevated().for_state(&ComponentState {
                dragged: true,
                ..ComponentState::idle()
            }),
            8.0,
            "`ElevatedCardTokens.DraggedContainerElevation` is Level4 = 8 dp"
        );
    }

    #[test]
    fn test_outlined_border_uses_theme_outline_variant() {
        let theme = crate::theme::ThemeColors::default_light();
        let enabled = CardBorder::outlined(&theme, true);
        assert_eq!(enabled.width, 1.0);
        assert_eq!(enabled.color, theme.outline_variant);
        let disabled = CardBorder::outlined(&theme, false);
        assert_eq!(
            disabled.color.a,
            (theme.outline.a as f32 * 0.12) as u8,
            "the disabled border = Outline @ 12%"
        );
        // Outlined carries a border by default, Filled/Elevated do not
        assert_eq!(
            Card::outlined().get_border(),
            None,
            "the border is resolved by style at build time"
        );
    }

    #[test]
    fn test_card_builder() {
        let card = Card::elevated()
            .on_click(|| {})
            .modifier(Modifier::new().padding(8.0));
        assert!(card.get_enabled());
        assert_eq!(card.get_style(), CardStyle::Elevated);
        assert_eq!(card.get_shape(), Shape::rounded(12.0));
        assert!(card.get_modifier().elements().len() >= 1);
    }
}
