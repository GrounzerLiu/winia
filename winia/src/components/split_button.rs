//! Split button (material3 `SplitButtonLayout`, an M3 Expressive component) — a primary action
//! button joined to a trailing menu button by a small gap, with a container shape that morphs on
//! interaction.
//!
//! Ground truth, in the order the numbers were taken:
//! - `target/compose-src/commonMain/androidx/compose/material3/tokens/SplitButton{XSmall,Small,
//!   Medium,Large,XLarge}Tokens.kt` — container height (32/40/56/96/136), the 2 dp gap, the inner
//!   corner sizes and their pressed variants, the two buttons' content paddings, the trailing icon
//!   size (22/22/26/38/50) and the outer corner percent (50).
//! - `target/compose-src/commonMain/androidx/compose/material3/HorizontalCenterOptically.kt` — the
//!   content correction `CenterOpticallyCoefficient * (avgStart - avgEnd)` with the coefficient
//!   `0.11f` (:61, :89), clamped into the content padding.
//! - `target/compose-src/commonMain/androidx/compose/material3/internal/AnimatedShape.kt` — the
//!   shape morph: the corner radii animate between the shapes (each corner is its own `Animatable`
//!   driven by the same spec), and the optical offset is read from the ANIMATED radii (:98-105).
//! - `SplitButton.kt` from `androidx-main` (fetched through the network relay; the extraction under
//!   `target/compose-src` carries the tokens and the internals but not this file): the layout's
//!   measure policy (trailing button measured FIRST for width, the leading one given what is left,
//!   both forced to the same height), `SplitButtonDefaults`' per-size constants, and the shape set
//!   (`SplitButtonShapes { shape, pressedShape, checkedShape }`, the leading button's checked shape
//!   being `null` while the trailing button's is `CircleShape`).
//!
//! One deliberate deviation, recorded in `docs/split-button.md`:
//! - Compose picks the shape from `pressed` and `checked` only (`shapeByInteraction`); the M3 spec
//!   page also lists hovered and focused as shape-changing states, and the tokens carry a
//!   `InnerHoveredCornerCornerSize`. winia follows the CODE, like every other component here.
//!
//! The morph used to run on a 180 ms tween of winia's own, because the motion token's numbers were
//! not in the extracted sources — `MotionSchemeKeyTokens.kt` lists the keys only. They ARE in
//! `tokens/ExpressiveMotionTokens.kt` (`SpringDefaultEffectsDamping = 1.0`,
//! `SpringDefaultEffectsStiffness = 1600.0`), and the theme's `default_effects` already holds
//! exactly that in BOTH schemes, so the morph now runs on upstream's own
//! `MotionSchemeKeyTokens.DefaultEffects` (`SplitButton.kt:695`, `:775`, `:887`).

use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::layout::constraints::Constraints;
use crate::unit::{Offset, Size};
use crate::layout::node::{
    intrinsic_size_of, measure_node, IntrinsicQuery, LayoutNode, MeasurePolicy, Placement,
};
use crate::layout::LayoutDirection;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::components::button::{Button, ButtonColors, ButtonElevation, ButtonSize, ButtonStyle};
use crate::interaction::MutableInteractionSource;
use crate::theme::WiniaTheme;
use std::sync::Arc;

/// The shapes a split button morphs between (material3 `SplitButtonShapes`).
///
/// `checked_shape` is `Some` for the trailing button only — material3 morphs that one to
/// `CircleShape` when the menu it opens is showing, and leaves the leading button's `null`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitButtonShapes {
    /// The resting shape.
    pub shape: Shape,
    /// The shape while the button is pressed.
    pub pressed_shape: Shape,
    /// The shape while the button is checked (the trailing button's menu-open state).
    pub checked_shape: Option<Shape>,
}

impl SplitButtonShapes {
    /// A shape set that does not morph: resting and pressed are the same and there is no checked
    /// shape (material3's `SplitButtonShapes(shape, shape, null)`).
    pub const fn flat(shape: Shape) -> Self {
        Self { shape, pressed_shape: shape, checked_shape: None }
    }
}

/// Defaults for the split button (material3 `SplitButtonDefaults`), keyed by the button size tier —
/// material3 keys the same table by button HEIGHT and picks a tier from it, which is exactly what
/// [`ButtonSize`] already is in winia.
pub struct SplitButtonDefaults;

impl SplitButtonDefaults {
    /// Gap between the two buttons (`SplitButtonSmallTokens.BetweenSpace`, and the M3 spec repeats
    /// "the space should always be 2dp").
    pub const SPACING: f32 = 2.0;

    /// Minimum width of either button (`SplitButtonDefaults.LeadingButtonMinWidth`).
    pub const MIN_BUTTON_WIDTH: f32 = 48.0;

    /// State-layer alpha the trailing button paints while checked
    /// (`StateTokens.PressedStateLayerOpacity`).
    pub const CHECKED_STATE_LAYER_ALPHA: f32 = 0.1;

    /// `CenterOpticallyCoefficient` (`HorizontalCenterOptically.kt:89`).
    pub const OPTICAL_COEFFICIENT: f32 = 0.11;

    /// Container height of a size tier (`*ContainerHeight`).
    pub fn container_height(size: ButtonSize) -> f32 {
        size.container_height()
    }

    /// Inner corner size — the leading button's END corners and the trailing button's START corners
    /// (`InnerCornerCornerSize`): 4/4/4/8/12 dp.
    pub fn inner_corner_size(size: ButtonSize) -> f32 {
        match size {
            ButtonSize::XSmall | ButtonSize::Small | ButtonSize::Medium => 4.0,
            ButtonSize::Large => 8.0,
            ButtonSize::XLarge => 12.0,
        }
    }

    /// The same corners while pressed (`InnerPressedCornerCornerSize`): 8/12/12/20/20 dp.
    pub fn inner_corner_size_pressed(size: ButtonSize) -> f32 {
        match size {
            ButtonSize::XSmall => 8.0,
            ButtonSize::Small | ButtonSize::Medium => 12.0,
            ButtonSize::Large | ButtonSize::XLarge => 20.0,
        }
    }

    /// The outer corners (`OuterCornerCornerSizePercent` = 50%): half the SHORT side, which for a
    /// button is its height.
    pub fn outer_corner_size(container_height: f32) -> f32 {
        container_height / 2.0
    }

    /// Content padding of the leading button as `(start, end)`
    /// (`LeadingButtonLeadingSpace`, `LeadingButtonTrailingSpace`).
    pub fn leading_content_padding(size: ButtonSize) -> (f32, f32) {
        match size {
            ButtonSize::XSmall => (12.0, 10.0),
            ButtonSize::Small => (16.0, 12.0),
            ButtonSize::Medium => (24.0, 24.0),
            ButtonSize::Large => (48.0, 48.0),
            ButtonSize::XLarge => (64.0, 64.0),
        }
    }

    /// Content padding of the trailing button as `(start, end)`
    /// (`TrailingButtonLeadingSpace`, `TrailingButtonTrailingSpace`).
    pub fn trailing_content_padding(size: ButtonSize) -> (f32, f32) {
        match size {
            ButtonSize::XSmall | ButtonSize::Small => (13.0, 13.0),
            ButtonSize::Medium => (15.0, 15.0),
            ButtonSize::Large => (29.0, 29.0),
            ButtonSize::XLarge => (43.0, 43.0),
        }
    }

    /// Icon size for the leading button. material3 forwards `ButtonDefaults.iconSizeFor(height)`,
    /// which is the plain button's per-tier icon size here.
    pub fn leading_icon_size(size: ButtonSize) -> f32 {
        size.icon_size()
    }

    /// Icon size for the trailing button (`*TrailingButtonIconSize`): 22/22/26/38/50 dp.
    pub fn trailing_icon_size(size: ButtonSize) -> f32 {
        match size {
            ButtonSize::XSmall | ButtonSize::Small => 22.0,
            ButtonSize::Medium => 26.0,
            ButtonSize::Large => 38.0,
            ButtonSize::XLarge => 50.0,
        }
    }

    /// The leading button's shapes for a size tier: full corners on the outer (leading) side, the
    /// inner corner facing the gap, and no checked shape.
    ///
    /// The direction is read from the theme, the way [`crate::components::SegmentedButtonDefaults::item_shape`]
    /// reads it: a `Shape` carries no direction, so "start" has to be resolved by the caller.
    pub fn leading_shapes(size: ButtonSize) -> SplitButtonShapes {
        let outer = Self::outer_corner_size(Self::container_height(size));
        let rtl = crate::layout::direction::current() == LayoutDirection::Rtl;
        let make = |inner: f32| {
            if rtl {
                // start is the right side: outer corners there, inner ones on the left.
                Shape::corners(inner, outer, outer, inner)
            } else {
                Shape::corners(outer, inner, inner, outer)
            }
        };
        SplitButtonShapes {
            shape: make(Self::inner_corner_size(size)),
            pressed_shape: make(Self::inner_corner_size_pressed(size)),
            checked_shape: None,
        }
    }

    /// The trailing button's shapes for a size tier: the inner corner faces the gap, the outer
    /// corners are full, and the checked shape is a stadium — material3's `TrailingCheckedShape`.
    pub fn trailing_shapes(size: ButtonSize) -> SplitButtonShapes {
        let outer = Self::outer_corner_size(Self::container_height(size));
        let rtl = crate::layout::direction::current() == LayoutDirection::Rtl;
        let make = |inner: f32| {
            if rtl {
                Shape::corners(outer, inner, inner, outer)
            } else {
                Shape::corners(inner, outer, outer, inner)
            }
        };
        SplitButtonShapes {
            shape: make(Self::inner_corner_size(size)),
            pressed_shape: make(Self::inner_corner_size_pressed(size)),
            checked_shape: Some(Shape::Pill),
        }
    }

    /// How far the content moves toward the shared gap, in dp, for a pair of radii — material3's
    /// `CenterOpticallyCoefficient * (avgStart - avgEnd)`, written as the difference that actually
    /// matters here: the outer side is rounder than the gap side, so the content moves toward the
    /// gap.
    ///
    /// `gap_padding` is the padding the content has on that side, and it clamps the shift exactly
    /// as material3 clamps it into the content padding (`HorizontalCenterOptically.kt:63`): the
    /// content can never be pushed past the room it has.
    pub fn optical_shift(outer_radius: f32, inner_radius: f32, gap_padding: f32) -> f32 {
        (Self::OPTICAL_COEFFICIENT * (outer_radius - inner_radius)).clamp(0.0, gap_padding)
    }

    /// The shape to draw for a state — material3's `shapeByInteraction`: pressed wins over checked,
    /// checked falls back to the resting shape when the set has no checked shape (the leading button
    /// never has one), and otherwise the resting shape it is.
    pub fn shape_for_state(shapes: &SplitButtonShapes, pressed: bool, checked: bool) -> Shape {
        if pressed {
            shapes.pressed_shape
        } else if checked {
            shapes.checked_shape.unwrap_or(shapes.shape)
        } else {
            shapes.shape
        }
    }
}

/// The layout: a leading button, a gap, a trailing button (material3 `SplitButtonLayout`).
///
/// The measure policy is material3's, and it is what being able to answer intrinsic questions buys
/// here: the trailing button is asked how wide it wants to be, the leading one is given the rest of
/// the width, and both are then forced to the same height. Before the intrinsic protocol existed
/// this component could not have been written as material3 writes it.
pub struct SplitButtonLayout {
    spacing: f32,
    modifier: Modifier,
}

impl SplitButtonLayout {
    pub fn new() -> Self {
        Self { spacing: SplitButtonDefaults::SPACING, modifier: Modifier::new() }
    }

    /// The gap between the two buttons (`SplitButtonLayout(spacing = …)`).
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Put a leading and a trailing button side by side. Build the two with
    /// [`SplitButtonDefaults::leading_button`] / [`SplitButtonDefaults::trailing_button`], or
    /// compose anything else you like — material3 takes arbitrary content too.
    pub fn build(
        self,
        ctx: &mut ComposeCtx,
        leading: impl FnOnce(&mut ComposeCtx),
        trailing: impl FnOnce(&mut ComposeCtx),
    ) {
        // ONE source of direction for the whole component. The policy needs it to mirror the two buttons,
        // and each half needs the same one to resolve its shape (`SplitButtonDefaults::*_shapes` reads the
        // theme) — so the resolution made here is handed down through the theme scope instead of letting the
        // halves resolve a second, possibly different, one. A node-level `layout_direction` on this layout
        // used to place the pair one way and shape it the other.
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        let key = ctx.next_key();
        let policy = SplitButtonPolicy { spacing: self.spacing, direction };
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::runtime::composer::GroupStatus::Skip => {}
            crate::runtime::composer::GroupStatus::Enter => {
                // Colors and typography are passed through untouched: only the direction is pinned.
                WiniaTheme::with_theme_typography_and_direction(
                    WiniaTheme::colors(),
                    WiniaTheme::typography(),
                    direction,
                    ctx,
                    |ctx| {
                        leading(ctx);
                        trailing(ctx);
                    },
                );
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for SplitButtonLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// material3's `SplitButtonLayout` measure policy, ported step for step.
#[derive(Debug)]
struct SplitButtonPolicy {
    spacing: f32,
    direction: LayoutDirection,
}

impl SplitButtonPolicy {
    /// The fallback for a composition that is not the pair material3 describes — reachable in release
    /// builds only, because the count is asserted in debug ones. Every child is measured at the incoming
    /// constraints and placed in a row `spacing` apart, the leading edge following the direction, so a
    /// mis-composed pair is visible instead of being skipped in silence.
    fn measure_row(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let mut sizes = Vec::with_capacity(children.len());
        let mut tallest: f32 = 0.0;
        for &child in children {
            let (size, _) = measure_node(nodes, policies, child, constraints);
            tallest = tallest.max(size.height);
            sizes.push(size);
        }
        let height = constraints.constrain_height(tallest);
        let width = constraints.constrain_width(
            sizes.iter().map(|size| size.width).sum::<f32>()
                + self.spacing * children.len().saturating_sub(1) as f32,
        );
        let mut placements = Vec::with_capacity(children.len());
        let mut cursor = 0.0;
        for size in &sizes {
            let x = if self.direction == LayoutDirection::Rtl {
                width - cursor - size.width
            } else {
                cursor
            };
            placements.push(Placement {
                size: Size::new(size.width, height),
                position: Offset::new(x, (height - size.height) / 2.0),
            });
            cursor += size.width + self.spacing;
        }
        (Size::new(width, height), placements)
    }
}

impl MeasurePolicy for SplitButtonPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // material3's `SplitButtonLayout` takes exactly two composables (leading, then trailing) and
        // indexes them, and `SplitButtonLayout::build` composes exactly those two — so the count is an
        // invariant rather than a case to handle: assert it, and if a caller breaks it anyway lay the
        // children out as a row (measured and placed) instead of collapsing into an empty layout.
        debug_assert_eq!(
            children.len(),
            2,
            "a split button arranges exactly two buttons (leading, then trailing)"
        );
        if children.len() != 2 {
            return self.measure_row(nodes, policies, children, constraints);
        }
        let leading = children[0];
        let trailing = children[1];

        // 1. How wide the trailing button wants to be, from its own content.
        let trailing_intrinsic_width = intrinsic_size_of(
            nodes,
            policies,
            trailing,
            IntrinsicQuery::MaxWidth,
            constraints.max_height,
        );
        // 2. What is left for the leading button (`subtractConstraintSafely`: a negative result
        //    clamps to zero rather than flipping the constraint).
        let available_leading_width =
            (constraints.max_width - (trailing_intrinsic_width + self.spacing)).max(0.0);
        // 3. The height both buttons want at the widths they will actually get.
        let leading_height = intrinsic_size_of(
            nodes,
            policies,
            leading,
            IntrinsicQuery::MaxHeight,
            available_leading_width,
        );
        let trailing_height = intrinsic_size_of(
            nodes,
            policies,
            trailing,
            IntrinsicQuery::MaxHeight,
            trailing_intrinsic_width,
        );
        let height = constraints.constrain_height(leading_height.max(trailing_height));

        // 4. The trailing button is measured FIRST: it has priority for width, and both are forced
        //    to the same height so the pair reads as one control.
        let (trailing_size, _) = measure_node(
            nodes,
            policies,
            trailing,
            Constraints::new(0.0, constraints.max_width, height, height),
        );
        // 5. The leading button gets the remaining width.
        let remaining =
            (constraints.max_width - (trailing_size.width + self.spacing)).max(0.0);
        let (leading_size, _) = measure_node(
            nodes,
            policies,
            leading,
            Constraints::new(0.0, remaining, height, height),
        );

        let width = constraints.constrain_width(
            leading_size.width + trailing_size.width + self.spacing,
        );
        // material3 places the two side by side and centres each vertically; under RTL it is
        // `placeRelative`, which mirrors the pair, so the LEADING button ends up on the right.
        let leading_x = if self.direction == LayoutDirection::Rtl {
            width - leading_size.width
        } else {
            0.0
        };
        let trailing_x = if self.direction == LayoutDirection::Rtl {
            leading_x - self.spacing - trailing_size.width
        } else {
            leading_size.width + self.spacing
        };
        let placements = vec![
            Placement {
                size: Size::new(leading_size.width, height),
                position: Offset::new(leading_x, (height - leading_size.height) / 2.0),
            },
            Placement {
                size: Size::new(trailing_size.width, height),
                position: Offset::new(trailing_x, (height - trailing_size.height) / 2.0),
            },
        ];
        (Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            if let Some(p) = placements.get(index) {
                nodes[child].position = p.position;
                nodes[child].measured_size = p.size;
            }
        }
    }
}

impl SplitButtonDefaults {
    /// A `filled` leading button, the M3 default emphasis (material3's
    /// `SplitButtonDefaults.LeadingButton`). Pass a different [`ButtonStyle`] for tonal, elevated or
    /// outlined, exactly as material3 says to do.
    pub fn leading_button(on_click: impl Fn() + Send + Sync + 'static) -> LeadingButton {
        LeadingButton::new(on_click)
    }

    /// A `filled` trailing button (material3's `SplitButtonDefaults.TrailingButton`).
    pub fn trailing_button() -> TrailingButton {
        TrailingButton::new()
    }
}

/// The button running along the leading edge of a [`SplitButtonLayout`].
pub struct LeadingButton {
    part: SplitButtonPart,
}

impl LeadingButton {
    pub fn new(on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self { part: SplitButtonPart::new(SplitButtonRole::Leading, Some(Arc::new(on_click))) }
    }

    /// Size tier (material3 keys its defaults by button height; [`ButtonSize`] is that table here).
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.part.size = size;
        self
    }

    /// Emphasis: `Filled` (default), `Tonal`, `Elevated`, `Outlined`.
    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.part.style = style;
        self
    }

    /// Shape set to morph between; defaults to [`SplitButtonDefaults::leading_shapes`] for the size.
    pub fn shapes(mut self, shapes: SplitButtonShapes) -> Self {
        self.part.shapes = Some(shapes);
        self
    }

    pub fn colors(mut self, colors: ButtonColors) -> Self {
        self.part.colors = Some(colors);
        self
    }

    pub fn elevation(mut self, elevation: ButtonElevation) -> Self {
        self.part.elevation = Some(elevation);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.part.enabled = enabled;
        self
    }

    /// Hoisted interaction source, for a caller that wants to observe or replay the button's states
    /// (material3 exposes the same parameter).
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.part.interaction_source = Some(source);
        self
    }

    /// Content padding as `(start, end)`; defaults to
    /// [`SplitButtonDefaults::leading_content_padding`] for the size.
    pub fn content_padding(mut self, start: f32, end: f32) -> Self {
        self.part.content_padding = Some((start, end));
        self
    }

    /// Turn off the optical content shift (it is what the M3 spec describes as the content being
    /// optically centred in an asymmetric shape).
    pub fn without_optical_shift(mut self) -> Self {
        self.part.optical_shift = false;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.part.modifier = self.part.modifier.then(modifier);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        self.part.build(ctx, content);
    }
}

/// The menu button on the trailing edge of a [`SplitButtonLayout`].
///
/// Two shapes of the API, like material3's two overloads: a plain action ([`TrailingButton::new`] +
/// [`TrailingButton::on_click`]) or a menu trigger that owns a checked state
/// ([`TrailingButton::checked`]) and therefore morphs to a stadium while the menu is showing.
pub struct TrailingButton {
    part: SplitButtonPart,
}

impl TrailingButton {
    /// A trailing button that runs an action.
    pub fn new() -> Self {
        Self { part: SplitButtonPart::new(SplitButtonRole::Trailing, None) }
    }

    /// A trailing button that reflects a menu's open state. material3's `checked` overload: while
    /// checked the button morphs to `full` corners and paints a state layer.
    pub fn checked(expanded: State<bool>) -> Self {
        Self {
            part: SplitButtonPart {
                checked: Some(expanded),
                ..SplitButtonPart::new(SplitButtonRole::Trailing, None)
            },
        }
    }

    /// The action to run (the plain-action form). On the checked form it runs AFTER the toggle, and the
    /// sibling `on_checked_change` callback receives the new value first; with no such callback winia
    /// writes the toggled value into the state itself. material3's checked button only takes
    /// `onCheckedChange`, so a plain action there is a winia extension — and one that now runs rather
    /// than being dropped in silence.
    pub fn on_click(mut self, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        self.part.on_click = Some(Arc::new(on_click));
        self
    }

    /// The checked form's callback: material3's `onCheckedChange`.
    pub fn on_checked_change(mut self, on_checked_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.part.on_checked_change = Some(Arc::new(on_checked_change));
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.part.size = size;
        self
    }

    pub fn style(mut self, style: ButtonStyle) -> Self {
        self.part.style = style;
        self
    }

    pub fn shapes(mut self, shapes: SplitButtonShapes) -> Self {
        self.part.shapes = Some(shapes);
        self
    }

    pub fn colors(mut self, colors: ButtonColors) -> Self {
        self.part.colors = Some(colors);
        self
    }

    pub fn elevation(mut self, elevation: ButtonElevation) -> Self {
        self.part.elevation = Some(elevation);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.part.enabled = enabled;
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.part.interaction_source = Some(source);
        self
    }

    /// Content padding as `(start, end)`; defaults to
    /// [`SplitButtonDefaults::trailing_content_padding`] for the size.
    pub fn content_padding(mut self, start: f32, end: f32) -> Self {
        self.part.content_padding = Some((start, end));
        self
    }

    pub fn without_optical_shift(mut self) -> Self {
        self.part.optical_shift = false;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.part.modifier = self.part.modifier.then(modifier);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        self.part.build(ctx, content);
    }
}

impl Default for TrailingButton {
    fn default() -> Self {
        Self::new()
    }
}

/// Which half of the pair a [`SplitButtonPart`] is: it decides which side of the shape is round and
/// which side the content is optically nudged toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitButtonRole {
    Leading,
    Trailing,
}

struct SplitButtonPart {
    role: SplitButtonRole,
    size: ButtonSize,
    style: ButtonStyle,
    shapes: Option<SplitButtonShapes>,
    colors: Option<ButtonColors>,
    elevation: Option<ButtonElevation>,
    enabled: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    checked: Option<State<bool>>,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
    content_padding: Option<(f32, f32)>,
    optical_shift: bool,
}

impl SplitButtonPart {
    fn new(role: SplitButtonRole, on_click: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        Self {
            role,
            size: ButtonSize::Small,
            style: ButtonStyle::Filled,
            shapes: None,
            colors: None,
            elevation: None,
            enabled: true,
            on_click,
            checked: None,
            on_checked_change: None,
            interaction_source: None,
            modifier: Modifier::new(),
            content_padding: None,
            optical_shift: true,
        }
    }

    fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let rtl = crate::layout::direction::current() == LayoutDirection::Rtl;
        let height = SplitButtonDefaults::container_height(self.size);
        let interaction = self
            .interaction_source
            .clone()
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        // Reading the interaction state registers the dependency, so a press re-composes the button
        // and the shape it picks (material3 gets the same recomposition from
        // `collectIsPressedAsState`).
        let state = interaction.state(self.enabled);
        let checked = self.checked.as_ref().is_some_and(|state| state.get());

        let shapes = self.shapes.unwrap_or(match self.role {
            SplitButtonRole::Leading => SplitButtonDefaults::leading_shapes(self.size),
            SplitButtonRole::Trailing => SplitButtonDefaults::trailing_shapes(self.size),
        });
        // material3's `shapeByInteraction`: pressed wins over checked, then the resting shape.
        let target_shape = SplitButtonDefaults::shape_for_state(&shapes, state.pressed, checked);
        // `AnimatedShape.kt` animates each of the four corner radii and rebuilds the shape from what the
        // animation currently holds. Taking the radii out of the shape `shapeByInteraction` resolved — a
        // caller's own set included, not only the tokens — is what makes a custom set morph too: it used
        // to be drawn at its resolved state throughout, so it snapped.
        let target_radii = corner_radii(&target_shape, height);
        let radii = [
            ctx.animate_float_as_state(target_radii.0, shape_morph_spec()),
            ctx.animate_float_as_state(target_radii.1, shape_morph_spec()),
            ctx.animate_float_as_state(target_radii.2, shape_morph_spec()),
            ctx.animate_float_as_state(target_radii.3, shape_morph_spec()),
        ];
        let drawn_radii = (radii[0].get(), radii[1].get(), radii[2].get(), radii[3].get());
        let shape = drawn_shape(&target_shape, target_radii, drawn_radii);

        // The content's optical offset follows the SETTLED radii — where the button rests, or the stadium
        // it becomes when checked — on an animation of its own, not the one the pressed radii feed.
        // material3 derives the offset from the animated shape (`SplitButton.kt:807-813`), which slides the
        // content ~1 dp along the press morph and back out again on release; the spec only ever tabulates
        // the offset for the two SETTLED states ("menu icon offset when unselected", "the icon becomes
        // centered when selected"). This keeps both of those, drops the press slide, and still slides the
        // icon to its centred position while the menu opens. Recorded in `docs/split-button.md`.
        let settled_radii =
            corner_radii(&SplitButtonDefaults::shape_for_state(&shapes, false, checked), height);
        let settled = [
            ctx.animate_float_as_state(settled_radii.0, shape_morph_spec()),
            ctx.animate_float_as_state(settled_radii.1, shape_morph_spec()),
            ctx.animate_float_as_state(settled_radii.2, shape_morph_spec()),
            ctx.animate_float_as_state(settled_radii.3, shape_morph_spec()),
        ];

        let (start_pad, end_pad) = self.content_padding.unwrap_or(match self.role {
            SplitButtonRole::Leading => SplitButtonDefaults::leading_content_padding(self.size),
            SplitButtonRole::Trailing => SplitButtonDefaults::trailing_content_padding(self.size),
        });
        // Optically centred in an asymmetric shape: the content moves toward the shared gap, by the
        // amount material3 computes from the average corner radii on each side, clamped into the
        // padding it has on that side.
        let gap_padding = match self.role {
            SplitButtonRole::Leading => end_pad,
            SplitButtonRole::Trailing => start_pad,
        };
        // A DYNAMIC value, not a number. `Modifier::offset` is a layout input, and winia's contract for an
        // animated layout value is a closure the LAYOUT phase evaluates (`Dimension::Dynamic`,
        // `modifier.rs:3548-3557`: "动态尺寸（动画 State/闭包）视为相同——布局期 layout_dep 已覆盖"): the read
        // inside it registers a layout dependency, so the node is re-measured on every animation tick
        // without being recomposed. Handing over a static number reads the animation during COMPOSITION,
        // which only a recomposition refreshes — and the animation's own ticks bring none here, so the
        // content kept whatever offset it had until some unrelated event forced a frame. Measured on the
        // live fixture: -2 while unselected, -2 four hundred milliseconds after the menu opened, 0 only
        // after a pointer move (the report "the icon only moves when the mouse moves over it").
        let optical_shift = self.optical_shift;
        let shift_value = crate::layout::Dimension::Dynamic(Arc::new(move || {
            if !optical_shift {
                return 0.0;
            }
            optical_shift_of(
                (settled[0].get(), settled[1].get(), settled[2].get(), settled[3].get()),
                rtl,
                gap_padding,
            )
        }));

        let colors = self
            .colors
            .unwrap_or_else(|| ButtonColors::from_theme(&WiniaTheme::colors(), self.style));
        let content_color = colors.content_color_for(&state);

        let mut button = Button::new()
            .style(self.style)
            .size(self.size)
            .shape(shape)
            .colors(colors)
            .enabled(self.enabled)
            .interaction_source(interaction)
            .min_size(SplitButtonDefaults::MIN_BUTTON_WIDTH, height)
            .content_padding((start_pad, 0.0, end_pad, 0.0));
        if let Some(elevation) = self.elevation {
            button = button.elevation(elevation);
        }
        // The checked state layer: material3 paints it with `drawWithContent { drawContent(); drawOutline(
        // shape, contentColor, PressedStateLayerOpacity) }` — OVER the content, not behind it under the
        // container. The after-content slot is what `DrawWrapNode::draw_after` runs in.
        let layer_alpha = SplitButtonDefaults::CHECKED_STATE_LAYER_ALPHA;
        if checked {
            let layer = Color::from_argb(
                (255.0 * layer_alpha).round() as u8,
                content_color.r,
                content_color.g,
                content_color.b,
            );
            button = button.modifier(Modifier::new().draw_wrap_node(StateLayer {
                color: layer,
                shape: shape.clone(),
            }));
        }
        let click = self.click_action();
        button = button.modifier(self.modifier);

        if let Some(click) = click {
            button = button.on_click(move || click());
        }

        button.build(ctx, |ctx| {
            // The offset wrapper is unconditional, even when the offset is zero. A conditional wrapper
            // changes the shape of the composition the moment the morph settles on its target, and a
            // rebuilt content subtree is not the same node as the one the layout had — the icon stays
            // where it was instead of sliding to the position the new offset asks for.
            // `absolute_offset`, not `offset`: the sign above is already the geometric direction, and a
            // plain `offset` has its x mirrored a second time by the parent's direction in RTL — which
            // turned the correction away from the gap in both halves there. The absolute form is the one
            // the layout does not mirror (`layout/node.rs`, the two branches side by side).
            crate::layout::Row::new()
                .modifier(Modifier::new().absolute_offset(shift_value, 0.0))
                .build(ctx, |ctx| content(ctx));
        });
    }

    /// The click handler, resolving material3's two overloads: a plain action, or a menu trigger
    /// that flips the checked state (`onCheckedChange(!checked)`).
    ///
    /// On the checked form the toggle runs first, and a plain action the caller set runs after it.
    /// material3's checked button takes only `onCheckedChange`, so an action here is winia's extension —
    /// and running it late beats the alternative this used to do, which was dropping it with no error.
    fn click_action(&self) -> Option<Arc<dyn Fn() + Send + Sync>> {
        if let Some(checked) = self.checked.clone() {
            let callback = self.on_checked_change.clone();
            let action = self.on_click.clone();
            return Some(Arc::new(move || {
                let next = !checked.get();
                match &callback {
                    Some(callback) => callback(next),
                    // winia's convenience: the state is already the caller's, so a checked trailing
                    // button works without a callback at all (material3 requires `onCheckedChange`).
                    None => checked.set(next),
                }
                if let Some(action) = &action {
                    action();
                }
            }));
        }
        self.on_click.clone()
    }
}

/// The checked half's state layer, painted in the wrap node's after-content slot: material3's
/// `drawWithContent { drawContent(); drawOutline(shape, contentColor, PressedStateLayerOpacity) }`.
/// The shape is the one the container draws in the same frame, so the layer follows the morph.
#[derive(Debug)]
struct StateLayer {
    color: Color,
    shape: Shape,
}

impl crate::modifier::DrawWrapNode for StateLayer {
    fn draw_after(
        &self,
        canvas: &skia_safe::Canvas,
        rect: skia_safe::Rect,
        _modifier: &Modifier,
    ) {
        crate::render::draw_background_for_node(canvas, rect, &self.color, &self.shape);
    }
}

/// The four corner radii a shape draws with, in the order `Shape::corners` takes them (top-left,
/// top-right, bottom-right, bottom-left). `Pill` and `Circle` are percent corners — half the SHORT side,
/// which Compose resolves in `createOutline` against the box it paints into — and a composition does not
/// know the box it will be measured into, so the container height resolves them, exactly as
/// `SplitButtonDefaults::outer_corner_size` does. That is exact while the half is at least as wide as it is
/// tall, and the settled checked shape is the token's `Pill`, which the renderer resolves against the real
/// box instead.
pub(crate) fn corner_radii(shape: &Shape, height: f32) -> (f32, f32, f32, f32) {
    let full = height / 2.0;
    match shape {
        Shape::RoundedRect { corner_radius } => {
            (*corner_radius, *corner_radius, *corner_radius, *corner_radius)
        }
        Shape::TopRoundedRect { radius } => (*radius, *radius, 0.0, 0.0),
        Shape::RightRoundedRect { radius } => (0.0, *radius, *radius, 0.0),
        Shape::LeftRoundedRect { radius } => (*radius, 0.0, 0.0, *radius),
        Shape::Pill | Shape::Circle => (full, full, full, full),
        // A pill part-way to a fixed radius. `corner_radii` resolves percent corners against the
        // HEIGHT (a composition does not know the box it will be measured into), so that is the
        // percent end here too.
        Shape::PillToRounded { corner_radius, t } => {
            let r = full + (corner_radius - full) * t;
            (r, r, r, r)
        }
        Shape::Corners {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        } => (*top_left, *top_right, *bottom_right, *bottom_left),
        Shape::Rectangle => (0.0, 0.0, 0.0, 0.0),
    }
}

/// The shape a half draws in this frame: the animated corners while the morph runs, and the resolved shape
/// itself once every corner has reached its target — so a settled chain reads exactly like material3's
/// `shapeByInteraction` result (the checked stadium included) while the transition approaches it instead of
/// snapping to it.
fn drawn_shape(target: &Shape, target_radii: (f32, f32, f32, f32), radii: (f32, f32, f32, f32)) -> Shape {
    let reached = |target: f32, current: f32| (target - current).abs() <= 0.01;
    let settled = reached(target_radii.0, radii.0)
        && reached(target_radii.1, radii.1)
        && reached(target_radii.2, radii.2)
        && reached(target_radii.3, radii.3);
    if settled {
        target.clone()
    } else {
        Shape::corners(radii.0, radii.1, radii.2, radii.3)
    }
}

/// material3's `HorizontalCenterOptically`: `0.11 * (average radius on the start side - average radius on
/// the end side)`, limited by the padding the content has on the gap side. Start and end follow the layout
/// direction, and material3 places the result with `placeRelative`, which mirrors x under RTL — winia hands
/// the layout an `absolute_offset`, so that mirror is applied here instead. The result is therefore a
/// geometric x delta: negative moves the content left.
pub(crate) fn optical_shift_of(radii: (f32, f32, f32, f32), rtl: bool, gap_padding: f32) -> f32 {
    let (tl, tr, br, bl) = radii;
    let (start, end) = if rtl {
        ((tr + br) / 2.0, (tl + bl) / 2.0)
    } else {
        ((tl + bl) / 2.0, (tr + br) / 2.0)
    };
    let offset = SplitButtonDefaults::OPTICAL_COEFFICIENT * (start - end);
    let physical = if rtl { -offset } else { offset };
    let limit = gap_padding.abs();
    physical.clamp(-limit, limit)
}

/// The spec every corner morph runs on: `MotionSchemeKeyTokens.DefaultEffects`
/// (`SplitButton.kt:695`, `:775`, `:887`, applied through `shapeByInteraction` at
/// `:1316-1334`) — damping 1.0, stiffness 1600, i.e. critically damped, so the corners settle
/// without overshoot. This was a 180 ms `EaseOutCubic` tween of winia's own until the token's
/// numbers turned up in `tokens/ExpressiveMotionTokens.kt`.
fn shape_morph_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::effects_default())
}


#[cfg(test)]
mod tests;
