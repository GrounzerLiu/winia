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
//! Two deliberate deviations, both recorded in `docs/split-button.md`:
//! - Compose picks the shape from `pressed` and `checked` only (`shapeByInteraction`); the M3 spec
//!   page also lists hovered and focused as shape-changing states, and the tokens carry a
//!   `InnerHoveredCornerCornerSize`. winia follows the CODE, like every other component here.
//! - The morph is animated with a 180 ms tween of winia's own instead of the motion scheme's
//!   `DefaultEffects` spring: that spec's numbers are not in the extracted sources (`MotionScheme
//!   KeyTokens.kt` lists the keys only), so no value is invented.

use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::layout::constraints::Constraints;
use crate::layout::node::{
    intrinsic_size_of, measure_node, IntrinsicQuery, LayoutNode, MeasurePolicy, Placement, Point,
    Size,
};
use crate::layout::LayoutDirection;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::button::{Button, ButtonColors, ButtonElevation, ButtonSize, ButtonStyle};
use crate::ui::interaction::{ComponentState, MutableInteractionSource};
use crate::ui::theme::WiniaTheme;
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

    /// Default icon size for the leading button (`ButtonSmallTokens.IconSize`).
    pub const LEADING_ICON_SIZE: f32 = 20.0;

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
    /// The direction is read from the theme, the way [`crate::ui::SegmentedButtonDefaults::item_shape`]
    /// reads it: a `Shape` carries no direction, so "start" has to be resolved by the caller.
    pub fn leading_shapes(size: ButtonSize) -> SplitButtonShapes {
        let outer = Self::outer_corner_size(Self::container_height(size));
        let rtl = WiniaTheme::direction() == LayoutDirection::Rtl;
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
        let rtl = WiniaTheme::direction() == LayoutDirection::Rtl;
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
        // The direction is resolved and declared the way `Row`/`SegmentedRow` do it: the policy has
        // to mirror the two buttons under RTL, and a policy left over from the other direction lays
        // them the wrong way round (the shapes follow the same direction, resolved by the caller's
        // `SplitButtonDefaults::*_shapes`).
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);
        let key = ctx.next_key();
        let policy = SplitButtonPolicy { spacing: self.spacing, direction };
        match ctx.start_restartable_group(key, self.modifier, policy) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                leading(ctx);
                trailing(ctx);
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

impl MeasurePolicy for SplitButtonPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        if children.len() < 2 {
            // A layout with half a split button has nothing to arrange; material3 would fail its
            // `fastFirst` lookups, winia lays out whatever is there at its own size.
            return (Size::new(0.0, 0.0), Vec::new());
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
                position: Point::new(leading_x, (height - leading_size.height) / 2.0),
            },
            Placement {
                size: Size::new(trailing_size.width, height),
                position: Point::new(trailing_x, (height - trailing_size.height) / 2.0),
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
        let rtl = WiniaTheme::direction() == LayoutDirection::Rtl;
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

        let outer = SplitButtonDefaults::outer_corner_size(height);
        let shapes = self.shapes.unwrap_or(match self.role {
            SplitButtonRole::Leading => SplitButtonDefaults::leading_shapes(self.size),
            SplitButtonRole::Trailing => SplitButtonDefaults::trailing_shapes(self.size),
        });
        let pressed_radius = SplitButtonDefaults::inner_corner_size_pressed(self.size);
        let resting_radius = SplitButtonDefaults::inner_corner_size(self.size);
        // material3's `shapeByInteraction`: pressed wins over checked, then the resting shape.
        let target_radius = if state.pressed {
            pressed_radius
        } else if checked {
            outer
        } else {
            resting_radius
        };
        // The morph animates every corner radius (`AnimatedShape.kt`); the outer corners are constant,
        // so one animated number is the whole difference.
        let radius = ctx
            .animate_float_as_state(target_radius, shape_morph_spec())
            .get();
        // The content's optical offset follows the SETTLED radius — where the button rests, or the
        // stadium it becomes when checked — on an animation of its own, not the one the pressed radius
        // feeds. material3 derives the offset from the animated shape (`SplitButton.kt:807-813`), which
        // slides the content ~1 dp along the press morph and back out again on release; the spec only
        // ever tabulates the offset for the two SETTLED states ("menu icon offset when unselected", "the
        // icon becomes centered when selected"). This keeps both of those, drops the press slide, and
        // still slides the icon to its centred position while the menu opens. Recorded in
        // `docs/split-button.md`.
        let settled_radius =
            ctx.animate_float_as_state(if checked { outer } else { resting_radius }, shape_morph_spec());

        // material3's `shapeByInteraction`, then the animation on top: a caller's own shape set is
        // drawn as given, while the default set is rebuilt from the animated radius — the same rule
        // re-evaluated at the value the animation currently holds, which is what `AnimatedShape.kt`
        // does when it animates each corner radius and rebuilds the shape.
        let target_shape = SplitButtonDefaults::shape_for_state(&shapes, state.pressed, checked);
        // The default set is rebuilt from the ANIMATED radius: that is what makes the morph into the
        // checked stadium animate rather than snap — when checked the radius animates up to `outer`, and
        // four equal radii are the stadium. Once it has settled there the token's own shape is drawn, so
        // the chain reads exactly as material3's `TrailingCheckedShape`. A caller's own set is drawn at
        // its resolved state throughout.
        let shape = if self.shapes.is_some() {
            target_shape
        } else {
            morph_or_token(self.role, outer, radius, rtl, checked, target_shape)
        };

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
        // animated layout value is a closure the LAYOUT phase evaluates (`SizeValue::Dynamic`,
        // `modifier.rs:3548-3557`: "动态尺寸（动画 State/闭包）视为相同——布局期 layout_dep 已覆盖"): the read
        // inside it registers a layout dependency, so the node is re-measured on every animation tick
        // without being recomposed. Handing over a static number reads the animation during COMPOSITION,
        // which only a recomposition refreshes — and the animation's own ticks bring none here, so the
        // content kept whatever offset it had until some unrelated event forced a frame. Measured on the
        // live fixture: -2 while unselected, -2 four hundred milliseconds after the menu opened, 0 only
        // after a pointer move (the report "the icon only moves when the mouse moves over it").
        let sign = match (self.role, rtl) {
            (SplitButtonRole::Leading, false) | (SplitButtonRole::Trailing, true) => 1.0,
            (SplitButtonRole::Leading, true) | (SplitButtonRole::Trailing, false) => -1.0,
        };
        let optical_shift = self.optical_shift;
        let shift_value = crate::modifier::SizeValue::Dynamic(Arc::new(move || {
            if !optical_shift {
                return 0.0;
            }
            sign * SplitButtonDefaults::optical_shift(outer, settled_radius.get(), gap_padding)
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
            crate::ui::Row::new()
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

/// The default shape set for a role, rebuilt from live radii so the morph is visible: the outer
/// corners are full, the two corners facing the gap carry `inner`.
fn animated_shape(role: SplitButtonRole, outer: f32, inner: f32, rtl: bool) -> Shape {
    let inner = inner.min(outer);
    match (role, rtl) {
        (SplitButtonRole::Leading, false) => Shape::corners(outer, inner, inner, outer),
        (SplitButtonRole::Leading, true) => Shape::corners(inner, outer, outer, inner),
        (SplitButtonRole::Trailing, false) => Shape::corners(inner, outer, outer, inner),
        (SplitButtonRole::Trailing, true) => Shape::corners(outer, inner, inner, outer),
    }
}

/// The shape a default-styled half draws at the morph's CURRENT radius: while the morph is running that
/// is the animated corners, and once it has settled on the checked stadium it is the token's own shape,
/// so the settled chain reads exactly as material3's `TrailingCheckedShape` does.
///
/// Keeping this a function of the RADIUS rather than of the state is what makes the checked transition
/// animate: the half approaches the stadium as the radius grows instead of snapping to it.
fn morph_or_token(
    role: SplitButtonRole,
    outer: f32,
    radius: f32,
    rtl: bool,
    checked: bool,
    token: Shape,
) -> Shape {
    if checked && radius >= outer - 0.01 {
        token
    } else {
        animated_shape(role, outer, radius, rtl)
    }
}

/// The corner morph's spec. material3 animates it with the motion scheme's `DefaultEffects`
/// (`MotionSchemeKeyTokens.DefaultEffects`), whose numbers are not in the extracted sources — so the
/// duration and curve here are winia's, like the segmented button's check-mark motion. Recorded in
/// `docs/split-button.md`.
fn shape_morph_spec() -> crate::animation::AnimationSpec {
    crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
        std::time::Duration::from_millis(180),
        crate::animation::interpolator::EaseOutCubic::new(),
    ))
}

/// Where the trailing button's content should sit, for a caller that wants to place the menu icon
/// itself. Kept for the same reason material3 exposes `trailingButtonIconSizeFor`.
pub fn trailing_icon_offset(size: ButtonSize, outer_radius: f32, inner_radius: f32) -> f32 {
    let (start, _) = SplitButtonDefaults::trailing_content_padding(size);
    SplitButtonDefaults::optical_shift(outer_radius, inner_radius, start)
}

/// The states a split button reports per button, exposed for tests and for a caller that hoists the
/// interaction source (material3's `interactionSource` parameter does the same job).
pub fn split_button_state(interaction: &MutableInteractionSource, enabled: bool) -> ComponentState {
    interaction.state(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gap_is_material3s_two_dp() {
        assert_eq!(SplitButtonDefaults::SPACING, 2.0);
    }

    #[test]
    fn inner_corners_come_from_the_token_files() {
        // SplitButton{XSmall,Small,Medium,Large,XLarge}Tokens.InnerCornerCornerSize
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::XSmall), 4.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::Small), 4.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::Medium), 4.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::Large), 8.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::XLarge), 12.0);
        // …Pressed: 8/12/12/20/20
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::XSmall), 8.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::Small), 12.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::Medium), 12.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::Large), 20.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::XLarge), 20.0);
    }

    #[test]
    fn paddings_and_icon_sizes_come_from_the_token_files() {
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::XSmall), (12.0, 10.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::Small), (16.0, 12.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::Medium), (24.0, 24.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::Large), (48.0, 48.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::XLarge), (64.0, 64.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::Small), (13.0, 13.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::Medium), (15.0, 15.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::Large), (29.0, 29.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::XLarge), (43.0, 43.0));
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::XSmall), 22.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small), 22.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::Medium), 26.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::Large), 38.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::XLarge), 50.0);
        assert_eq!(SplitButtonDefaults::LEADING_ICON_SIZE, 20.0);
    }

    #[test]
    fn the_outer_corners_are_half_the_short_side() {
        // CornerFull is a percent-50 corner; for a button the short side is the height.
        assert_eq!(SplitButtonDefaults::outer_corner_size(32.0), 16.0);
        assert_eq!(SplitButtonDefaults::outer_corner_size(136.0), 68.0);
    }

    #[test]
    fn the_optical_shift_matches_the_specs_menu_icon_offsets() {
        // The spec lists the trailing button's unselected menu-icon offset as -1/-1/-2/-3/-6 dp for
        // XS/S/M/L/XL. The shift below is Compose's own formula, and it lands on those numbers (the
        // spec's are rounded): 0.11 * (inner - outer).
        for size in [
            ButtonSize::XSmall,
            ButtonSize::Small,
            ButtonSize::Medium,
            ButtonSize::Large,
            ButtonSize::XLarge,
        ] {
            let outer = SplitButtonDefaults::outer_corner_size(
                SplitButtonDefaults::container_height(size),
            );
            let inner = SplitButtonDefaults::inner_corner_size(size);
            let (start, _) = SplitButtonDefaults::trailing_content_padding(size);
            let shift = SplitButtonDefaults::optical_shift(outer, inner, start);
            let spec = match size {
                ButtonSize::XSmall => 1.0,
                ButtonSize::Small => 1.0,
                ButtonSize::Medium => 2.0,
                ButtonSize::Large => 3.0,
                ButtonSize::XLarge => 6.0,
            };
            assert!(
                (shift - spec).abs() <= 1.5,
                "{size:?}: the optical shift {shift} should be near the spec's {spec} dp"
            );
        }
    }

    #[test]
    fn the_optical_shift_cannot_push_the_content_out_of_its_padding() {
        // A tiny padding must clamp the shift: material3 coerces the correction into the padding,
        // precisely so a narrow button's content is never clipped.
        assert_eq!(SplitButtonDefaults::optical_shift(20.0, 4.0, 1.0), 1.0);
        assert!(SplitButtonDefaults::optical_shift(4.0, 20.0, 10.0) == 0.0);
    }

    #[test]
    fn the_shapes_put_the_full_corners_outside_and_the_inner_corner_in() {
        let leading = SplitButtonDefaults::leading_shapes(ButtonSize::Small);
        assert_eq!(leading.shape, Shape::corners(20.0, 4.0, 4.0, 20.0));
        assert_eq!(leading.pressed_shape, Shape::corners(20.0, 12.0, 12.0, 20.0));
        assert_eq!(leading.checked_shape, None);

        let trailing = SplitButtonDefaults::trailing_shapes(ButtonSize::Small);
        assert_eq!(trailing.shape, Shape::corners(4.0, 20.0, 20.0, 4.0));
        assert_eq!(trailing.checked_shape, Some(Shape::Pill));
    }

    // ── Layout (the measure policy material3 writes) ──

    use crate::core::composer::Composer;
    use crate::ui::text::Text;
    use crate::ui::theme::ThemeColors;

    /// A split button of the default size (40 dp) with a leading label and a trailing "v" glyph, laid
    /// out inside a theme of the given direction. `trailing_height` grows the trailing button's
    /// content, which is how the shared-height rule gets something to equalise (a fixed height on the
    /// content leaf, not a font size: the button's own text style wins over the content's font size).
    fn compose_split(
        label: &str,
        trailing_height: Option<f32>,
        dir: LayoutDirection,
    ) -> Composer {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let label = label.to_string();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                SplitButtonLayout::new().build(
                    ctx,
                    |ctx| {
                        SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                            Text::new(label.clone()).build(ctx);
                        });
                    },
                    |ctx| {
                        SplitButtonDefaults::trailing_button()
                            .on_click(|| {})
                            .build(ctx, |ctx| {
                                let text = Text::new("v");
                                let text = match trailing_height {
                                    Some(height) => {
                                        text.modifier(Modifier::new().height(height))
                                    }
                                    None => text,
                                };
                                text.build(ctx);
                            });
                    },
                );
            });
        });
        composer
    }

    fn measure_split(
        composer: &mut Composer,
        width: f32,
        height: f32,
    ) -> (usize, Vec<usize>, Size, f32, f32) {
        composer.layout(Constraints::new(0.0, width, 0.0, height));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        let root_size = nodes[root].measured_size;
        let leading = nodes[children[0]].measured_size.width;
        let trailing = nodes[children[1]].measured_size.width;
        (root, children, root_size, leading, trailing)
    }

    #[test]
    fn the_pair_is_a_leading_button_a_two_dp_gap_and_a_trailing_button() {
        let mut composer = compose_split("Add", None, LayoutDirection::Ltr);
        let (_, children, root_size, leading, trailing) = measure_split(&mut composer, 400.0, 200.0);
        let nodes = composer.arena_nodes();
        assert!(
            (leading + trailing + SplitButtonDefaults::SPACING - root_size.width).abs() < 0.01,
            "the layout is leading + gap + trailing"
        );
        assert!(
            (nodes[children[1]].position.x - (leading + SplitButtonDefaults::SPACING)).abs() < 0.01,
            "the gap sits between the two buttons"
        );
        // A pair that fits its parent wraps its content instead of filling it.
        assert!(root_size.width < 400.0, "the layout hugs its content");
        assert_eq!(nodes[children[0]].measured_size.height, 40.0);
        assert_eq!(nodes[children[1]].measured_size.height, 40.0);
    }

    #[test]
    fn the_trailing_button_is_measured_first_and_the_leading_one_gets_what_is_left() {
        // The label wants far more than the parent can give: material3 measures the trailing button
        // against the full width first, then hands the leading button the remainder.
        let mut composer = compose_split(
            "A split button label far too long for the space",
            None,
            LayoutDirection::Ltr,
        );
        let (_, _, root_size, leading, trailing) = measure_split(&mut composer, 200.0, 200.0);
        assert!(
            trailing >= SplitButtonDefaults::MIN_BUTTON_WIDTH,
            "the trailing button keeps its own width ({trailing})"
        );
        assert!(
            (leading - (200.0 - trailing - SplitButtonDefaults::SPACING)).abs() < 0.01,
            "the leading button takes the remainder ({leading} vs {})",
            200.0 - trailing - SplitButtonDefaults::SPACING
        );
        assert_eq!(root_size.width, 200.0, "the pair fills the parent it cannot fit in");
    }

    #[test]
    fn both_buttons_are_forced_to_one_height() {
        // The trailing button's content asks for a taller box; material3 gives the pair the taller of
        // the two intrinsic heights and measures BOTH at it, so they read as one control.
        let mut composer = compose_split("Add", Some(72.0), LayoutDirection::Ltr);
        let (_, children, _, _, _) = measure_split(&mut composer, 400.0, 400.0);
        let nodes = composer.arena_nodes();
        let height = nodes[children[1]].measured_size.height;
        assert!(
            (height - 72.0).abs() < 0.01,
            "the taller content decides the shared height (got {height})"
        );
        assert_eq!(
            nodes[children[0]].measured_size.height, height,
            "the leading button is stretched to the trailing button's height"
        );
    }

    #[test]
    fn rtl_puts_the_leading_button_on_the_right() {
        let mut composer = compose_split("Add", None, LayoutDirection::Rtl);
        let (_, children, root_size, leading, _) = measure_split(&mut composer, 400.0, 200.0);
        let nodes = composer.arena_nodes();
        let first = &nodes[children[0]];
        let last = &nodes[children[1]];
        assert!(
            (first.position.x + first.measured_size.width - root_size.width).abs() < 0.01,
            "the leading button ends at the right edge in RTL (x={}, width={}, root={})",
            first.position.x,
            leading,
            root_size.width
        );
        assert!(last.position.x.abs() < 0.01, "the trailing button starts at the left edge in RTL");
    }

    // ── The morph ──

    /// The shapes a node actually paints, read off its modifier chain the way the renderer (and the
    /// ripple's clip inference) reads them.
    fn drawn_backgrounds(composer: &Composer, node: usize) -> Vec<Shape> {
        composer.arena_nodes()[node]
            .modifier
            .elements()
            .iter()
            .filter_map(|el| match el {
                crate::modifier::ModifierElement::Background { shape, .. } => Some(*shape),
                _ => None,
            })
            .collect()
    }

    /// A split button whose buttons take a hoisted interaction source, so a test can put a state in
    /// place BEFORE the first composition — the state read is a composition dependency, so the first
    /// pass already sees it (this is what material3's `interactionSource` parameter is for).
    fn compose_split_with_source(
        interaction: &MutableInteractionSource,
        checked: Option<State<bool>>,
        dir: LayoutDirection,
    ) -> Composer {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = interaction.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                SplitButtonLayout::new().build(
                    ctx,
                    |ctx| {
                        SplitButtonDefaults::leading_button(|| {})
                            .interaction_source(source.clone())
                            .build(ctx, |ctx| {
                                Text::new("Add").build(ctx);
                            });
                    },
                    |ctx| {
                        let trailing = match checked {
                            Some(state) => TrailingButton::checked(state),
                            None => SplitButtonDefaults::trailing_button().on_click(|| {}),
                        };
                        trailing.build(ctx, |ctx| {
                            Text::new("v").build(ctx);
                        });
                    },
                );
            });
        });
        composer
    }

    fn split_children(composer: &mut Composer) -> Vec<usize> {
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        composer.arena_nodes()[root].children.clone()
    }

    #[test]
    fn shape_for_state_is_material3s_shape_by_interaction() {
        let leading = SplitButtonDefaults::leading_shapes(ButtonSize::Small);
        let trailing = SplitButtonDefaults::trailing_shapes(ButtonSize::Small);
        assert_eq!(SplitButtonDefaults::shape_for_state(&leading, false, false), leading.shape);
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&leading, true, false),
            leading.pressed_shape,
            "pressed wins over the resting shape"
        );
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&leading, false, true),
            leading.shape,
            "the leading button has no checked shape, so it keeps its own"
        );
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&trailing, false, true),
            Shape::Pill,
            "the checked trailing button morphs to the stadium"
        );
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&trailing, true, true),
            trailing.pressed_shape,
            "pressed wins over checked, as `shapeByInteraction` orders them"
        );
    }

    #[test]
    fn a_resting_button_draws_its_resting_shape_and_no_state_layer() {
        let interaction = MutableInteractionSource::new();
        let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
        let children = split_children(&mut composer);
        let leading = SplitButtonDefaults::leading_shapes(ButtonSize::Small);
        let trailing = SplitButtonDefaults::trailing_shapes(ButtonSize::Small);
        assert_eq!(
            drawn_backgrounds(&composer, children[0]),
            vec![leading.shape],
            "the leading button paints its container once, in the resting shape"
        );
        assert_eq!(drawn_backgrounds(&composer, children[1]), vec![trailing.shape]);
    }

    #[test]
    fn a_pressed_button_draws_the_pressed_shape() {
        let interaction = MutableInteractionSource::new();
        // Pressed BEFORE the first composition: the state read is a dependency, so the first pass
        // already picks the pressed shape, and the animation starts at its target.
        interaction.emit_press();
        let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
        let children = split_children(&mut composer);
        let expected = SplitButtonDefaults::leading_shapes(ButtonSize::Small).pressed_shape;
        assert_eq!(
            drawn_backgrounds(&composer, children[0]),
            vec![expected],
            "a pressed leading button paints the pressed shape (inner corner 12 instead of 4)"
        );
        assert_eq!(
            drawn_backgrounds(&composer, children[1]),
            vec![SplitButtonDefaults::trailing_shapes(ButtonSize::Small).shape],
            "the trailing button is untouched by its neighbour's press"
        );
    }

    /// material3 paints the checked state layer with `drawWithContent { drawContent(); drawOutline(...) }`
    /// — over the content, not behind it under the container. The order is checked structurally rather
    /// than by pixels, because the layer's colour IS the content colour and the content paints in that
    /// same colour: blending a colour over itself leaves every glyph pixel unchanged, so no pixel reading
    /// can separate the two orders. What can be measured is where the node asks for the layer.
    #[test]
    fn the_checked_state_layer_paints_after_the_content() {
        let read = |checked: bool| {
            let interaction = MutableInteractionSource::new();
            let state = if checked { Some(State::new(true)) } else { None };
            let mut composer = compose_split_with_source(&interaction, state, LayoutDirection::Ltr);
            let children = split_children(&mut composer);
            let backgrounds = drawn_backgrounds(&composer, children[1]);
            let layers = composer.arena_nodes()[children[1]].modifier.draw_wrap_nodes().count();
            (backgrounds, layers)
        };
        let (resting_backgrounds, resting_layers) = read(false);
        let (checked_backgrounds, checked_layers) = read(true);
        eprintln!(
            "state layer: resting {resting_backgrounds:?}/{resting_layers} checked {checked_backgrounds:?}/{checked_layers}"
        );
        assert_eq!(
            checked_layers,
            resting_layers + 1,
            "the checked half asks for one more after-content layer"
        );
        assert_eq!(
            checked_backgrounds.len(),
            resting_backgrounds.len(),
            "and not for one more background under the content ({checked_backgrounds:?} vs {resting_backgrounds:?})"
        );
        assert_eq!(
            checked_backgrounds.first(),
            Some(&Shape::Pill),
            "the checked container is still the token's stadium"
        );
    }

    // ── What the pair actually paints ──

    /// The inset of the top row of a rounded rectangle IS that corner's radius (for `RoundedCornerShape`
    /// the row spans `left + topStart .. right - topEnd`), so measuring the painted pixels gives the four
    /// radii of each button, not just the shape the modifier chain asked for.
    ///
    /// Measured, because "the two parts' corners look different" is exactly the kind of claim a modifier
    /// chain cannot settle: the pair must mirror — the leading button rounds its LEFT corners fully and
    /// its right ones by the inner-corner token, the trailing button the other way round.
    #[test]
    fn the_pair_paints_the_token_radii_on_every_corner() {
        let interaction = MutableInteractionSource::new();
        let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let rects: Vec<(f32, f32, f32, f32)> = nodes[root]
            .children
            .iter()
            .map(|i| {
                let node = &nodes[*i];
                (
                    node.position.x,
                    node.position.y,
                    node.measured_size.width,
                    node.measured_size.height,
                )
            })
            .collect();

        let (w, h) = (400, 200);
        let mut surface = skia_safe::surfaces::raster_n32_premul((w, h)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        let painted = |x: usize, y: usize| px[y * w as usize + x][0] < 200;

        let mut measured = Vec::new();
        for (bx, by, bw, bh) in &rects {
            let x0 = bx.round() as usize;
            let y0 = by.round() as usize;
            let x1 = (bx + bw).round() as usize - 1;
            let y1 = (by + bh).round() as usize - 1;
            // The corner arc spans exactly `radius` rows/columns: the first row whose leftmost painted
            // pixel reaches the button's left edge sits at `top + top_left`. Measuring the EXTENT of the
            // arc, not the inset of one antialiased row, is what makes this readable off a raster.
            let first = |y: usize| (x0..=x1).find(|x| painted(*x, y));
            let last = |y: usize| (x0..=x1).rev().find(|x| painted(*x, y));
            let left_edge = (y0..=y1).filter_map(first).min().expect("a painted button");
            let right_edge = (y0..=y1).filter_map(last).max().expect("a painted button");
            let top_edge = (y0..=y1).find(|y| painted(x0 + (x1 - x0) / 2, *y)).expect("painted");
            let bottom_edge =
                (y0..=y1).rev().find(|y| painted(x0 + (x1 - x0) / 2, *y)).expect("painted");
            let row_left = |y: usize| first(y).expect("painted row");
            let row_right = |y: usize| last(y).expect("painted row");
            let col_top = |x: usize| (y0..=y1).find(|y| painted(x, *y)).expect("painted column");
            let col_bottom = |x: usize| (y0..=y1).rev().find(|y| painted(x, *y)).expect("painted");
            let tl = (top_edge..=bottom_edge).find(|y| row_left(*y) <= left_edge + 1).unwrap()
                - top_edge;
            let tr = (top_edge..=bottom_edge)
                .find(|y| row_right(*y) >= right_edge.saturating_sub(1))
                .unwrap()
                - top_edge;
            let bl = bottom_edge
                - (top_edge..=bottom_edge)
                    .rev()
                    .find(|y| row_left(*y) <= left_edge + 1)
                    .unwrap();
            let br = bottom_edge
                - (top_edge..=bottom_edge)
                    .rev()
                    .find(|y| row_right(*y) >= right_edge.saturating_sub(1))
                    .unwrap();
            let _ = (col_top, col_bottom);
            measured.push((tl, tr, bl, br));
        }
        eprintln!("split button corners (top-left, top-right, bottom-left, bottom-right): {measured:?}");

        let inner = SplitButtonDefaults::inner_corner_size(ButtonSize::Small);
        let outer = rects[0].3 / 2.0;
        // Measured through a coverage threshold, so the arc reads a few pixels short of its real radius
        // (a true 20 dp corner measures ~12 here and a true 4 dp one ~1). What must hold exactly is the MIRROR — the same two radii,
        // swapped — and that the outer corner is the round one.
        for (i, corners) in measured.iter().enumerate() {
            let (far, near) = if i == 0 { (corners.0, corners.1) } else { (corners.3, corners.2) };
            assert!(
                far > near + 4,
                "button {i} must round its outer corner more than its inner one, measured {corners:?}"
            );
            let (other_far, other_near) =
                if i == 0 { (measured[1].3, measured[1].2) } else { (measured[0].0, measured[0].1) };
            assert!(
                far.abs_diff(other_far) <= 2 && near.abs_diff(other_near) <= 2,
                "button {i} must mirror the other half: {corners:?} against the other's two radii"
            );
            assert!(
                (outer * 0.5..=outer + 1.0).contains(&(far as f32)),
                "button {i}'s outer corner is CornerFull ({outer}), measured {far}"
            );
            assert!(
                (near as f32) <= inner + 1.5,
                "button {i}'s inner corner is the tier's {inner}, measured {near}"
            );
        }
    }

    /// The checked stadium is APPROACHED, not jumped to: while the morph is still running the half
    /// draws the animated corners, and only a radius that has reached `outer` is the stadium.
    ///
    /// This is the regression guard for the transition itself — reading the shape straight off the
    /// state (as it used to) makes the `mid` case below return the stadium immediately, i.e. a snap.
    #[test]
    fn the_checked_stadium_is_reached_through_the_morph() {
        let outer = SplitButtonDefaults::outer_corner_size(SplitButtonDefaults::container_height(
            ButtonSize::Small,
        ));
        let inner = SplitButtonDefaults::inner_corner_size(ButtonSize::Small);
        assert_eq!(
            morph_or_token(SplitButtonRole::Trailing, outer, outer, false, true, Shape::Pill),
            Shape::Pill,
            "a settled trailing half draws material3's own checked shape"
        );
        assert_eq!(
            morph_or_token(SplitButtonRole::Trailing, outer, inner, false, true, Shape::Pill),
            Shape::corners(inner, outer, outer, inner),
            "mid-morph the half draws the animated corners, so the stadium is approached"
        );
        let nearly = morph_or_token(
            SplitButtonRole::Trailing,
            outer,
            outer - 1.0,
            false,
            true,
            Shape::Pill,
        );
        assert_ne!(
            nearly, Shape::Pill,
            "a radius one dp short of `outer` is not the stadium yet: {nearly:?}"
        );
    }

    /// The checked trailing half is a stadium in the PAINTED pixels, not only in the modifier chain:
    /// material3's `TrailingCheckedShape = CircleShape` (`SplitButton.kt:427`), which for a
    /// wider-than-tall button is every corner at `outer`.
    #[test]
    fn a_checked_trailing_paints_the_stadium() {
        let interaction = MutableInteractionSource::new();
        let expanded = State::new(true);
        let mut composer =
            compose_split_with_source(&interaction, Some(expanded), LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let trailing = nodes[root].children[1];
        let (x, y, w) = (
            nodes[trailing].position.x.round() as usize,
            nodes[trailing].position.y.round() as usize,
            nodes[trailing].measured_size.width.round() as usize,
        );
        let mut surface = skia_safe::surfaces::raster_n32_premul((400, 200)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        let painted = |x: usize, y: usize| px[y * 400 + x][0] < 200;
        let row = y + 1;
        let left = (x..x + w).find(|x| painted(*x, row)).expect("painted") - x;
        let right = (x + w - 1) - (x..x + w).rev().find(|x| painted(*x, row)).expect("painted");
        eprintln!("checked trailing painted corners (left, right): ({left}, {right})");
        assert!(
            left.abs_diff(right) <= 2,
            "a checked trailing half is a stadium, so both of its top corners round the same: \
             measured ({left}, {right})"
        );
        assert!(
            left > 8,
            "and its gap-side corner has grown to the outer radius, measured {left}"
        );
    }

    /// A press rounds the inner corner — and does NOT round it all the way. material3's pressed inner
    /// corner is `SmallInnerCornerSizePressed = 12.dp` against a 40 dp container (full would be 20), so
    /// the pressed half is asymmetric on purpose: the outer corner stays `CornerFull`.
    ///
    /// The inset of the top row of the painted rect is that corner's radius (measured with the raster's
    /// bias, which under-reads both the same way, so the comparison between the two states holds).
    #[test]
    fn a_press_rounds_the_inner_corner_without_making_it_full() {
        let measure = |pressed: bool| {
            let interaction = MutableInteractionSource::new();
            if pressed {
                interaction.emit_press();
            }
            let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let leading = nodes[root].children[0];
            let (x, y, w) = (
                nodes[leading].position.x.round() as usize,
                nodes[leading].position.y.round() as usize,
                nodes[leading].measured_size.width.round() as usize,
            );
            let mut surface =
                skia_safe::surfaces::raster_n32_premul((400, 200)).expect("surface");
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            crate::render::render(composer.arena_nodes(), root, canvas);
            let pixels = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
            let painted = |x: usize, y: usize| px[y * 400 + x][0] < 200;
            let row = y + 1;
            let first = (x..x + w).find(|x| painted(*x, row)).expect("painted") - x;
            let last = (x..x + w).rev().find(|x| painted(*x, row)).expect("painted");
            (first, (x + w - 1) - last)
        };
        let (resting_outer, resting_inner) = measure(false);
        let (pressed_outer, pressed_inner) = measure(true);
        eprintln!(
            "split button leading, painted corners (outer, inner): resting ({resting_outer}, \
             {resting_inner}) pressed ({pressed_outer}, {pressed_inner})"
        );
        assert!(
            pressed_inner > resting_inner + 4,
            "a press rounds the inner corner (resting {resting_inner} -> pressed {pressed_inner})"
        );
        assert!(
            pressed_inner < pressed_outer - 4,
            "and it does not round it all the way: material3's pressed inner corner is 12 dp against \
             the 20 dp CornerFull outer (pressed ({pressed_outer}, {pressed_inner}))"
        );
        assert!(
            pressed_outer >= resting_outer - 2,
            "the outer corner stays CornerFull under the press ({resting_outer} -> {pressed_outer})"
        );
    }

    /// A press must change the SHAPE and not move the content: the optical offset reads the settled
    /// radius, not the animated one. The press has to show up somewhere or the assertion proves nothing,
    /// so the same state is also asked for the shape it paints.
    #[test]
    fn a_press_does_not_move_the_content() {
        let measure = |pressed: bool| {
            let interaction = MutableInteractionSource::new();
            if pressed {
                interaction.emit_press();
            }
            let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
            split_children(&mut composer);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let buttons = nodes[root].children.clone();
            let offsets: Vec<f32> = buttons
                .iter()
                .map(|b| {
                    // Two levels down: the Button holds its content in a centring box, and the offset
                    // lives on the Row inside it. Reading the box itself measures the centring, which
                    // does not move with the offset — that is how this test kept passing while the
                    // content did move (measured: swapping the offset back to the pressed-aware radius
                    // left every library test green).
                    let panel = nodes[*b].children.first().expect("the button's content box");
                    let wrapper = nodes[*panel].children.first().expect("the offset wrapper");
                    nodes[*wrapper].position.x
                })
                .collect();
            let shapes: Vec<Option<Shape>> = buttons
                .iter()
                .map(|b| drawn_backgrounds(&composer, *b).into_iter().next())
                .collect();
            (offsets, shapes)
        };
        let (resting_offsets, resting_shapes) = measure(false);
        let (pressed_offsets, pressed_shapes) = measure(true);
        eprintln!("split button content offsets: resting {resting_offsets:?} pressed {pressed_offsets:?}");
        assert_eq!(
            resting_offsets, pressed_offsets,
            "a press must not move the content (the optical offset reads the settled radius)"
        );
        assert_ne!(
            resting_shapes, pressed_shapes,
            "the press must still morph the shape, or this test proves nothing"
        );
    }

    /// The trailing half's content is optically offset while it is unselected and centred once the menu
    /// is open: material3's per-size "menu icon offset when unselected" and "the icon becomes centered
    /// when selected". This is the layout half of the checked transition — the painted shape is guarded
    /// by `a_checked_trailing_paints_the_stadium`.
    #[test]
    fn a_checked_trailing_centres_its_content() {
        let content_offset = |checked: bool| {
            let interaction = MutableInteractionSource::new();
            let state = if checked { Some(State::new(true)) } else { None };
            let mut composer = compose_split_with_source(&interaction, state, LayoutDirection::Ltr);
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let trailing = nodes[root].children[1];
            let panel = nodes[trailing].children.first().copied().expect("the content box");
            let wrapper = nodes[panel].children.first().copied().expect("the offset wrapper");
            nodes[wrapper].position.x
        };
        let unselected = content_offset(false);
        let selected = content_offset(true);
        eprintln!("trailing content offset: unselected {unselected} selected {selected}");
        assert_eq!(selected, 0.0, "an open menu centres the trailing half's content");
        assert!(
            unselected < -1.0,
            "and an unselected one sits offset toward the gap, measured {unselected}"
        );
    }

    /// The optical shift is a geometric direction, and RTL does not mirror the answer: the trailing half
    /// sits on the LEFT there with the gap to its right, so the correction has to come out positive where
    /// LTR wants it negative. `Modifier::absolute_offset` is what carries it, because a plain `offset`
    /// has its x mirrored a second time by the parent's direction and sent both halves AWAY from the gap.
    ///
    /// Measured against the same composition with the correction switched off, so the reading needs no
    /// assumption about where the content would otherwise sit.
    #[test]
    fn the_optical_shift_points_at_the_gap_in_both_directions() {
        let wrapper_x = |dir: LayoutDirection, optical: bool| -> f32 {
            let theme = ThemeColors::light_from_seed(0x6750A4);
            let interaction = MutableInteractionSource::new();
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                    SplitButtonLayout::new().build(
                        ctx,
                        |ctx| {
                            SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                                Text::new("Add").build(ctx);
                            });
                        },
                        |ctx| {
                            let trailing = SplitButtonDefaults::trailing_button()
                                .on_click(|| {})
                                .interaction_source(interaction.clone());
                            let trailing = if optical {
                                trailing
                            } else {
                                trailing.without_optical_shift()
                            };
                            trailing.build(ctx, |ctx| {
                                Text::new("v").build(ctx);
                            });
                        },
                    );
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let trailing = nodes[root].children[1];
            let panel = nodes[trailing].children.first().copied().expect("the content box");
            let wrapper = nodes[panel].children.first().copied().expect("the offset wrapper");
            nodes[wrapper].position.x
        };
        let ltr = wrapper_x(LayoutDirection::Ltr, false) - wrapper_x(LayoutDirection::Ltr, true);
        let rtl = wrapper_x(LayoutDirection::Rtl, false) - wrapper_x(LayoutDirection::Rtl, true);
        eprintln!("trailing content correction (unshifted minus shifted): ltr {ltr} rtl {rtl}");
        assert!(
            ltr > 1.0,
            "LTR: the gap is on the content's left, so it moves that way (measured {ltr})"
        );
        assert!(
            rtl < -1.0,
            "RTL: the gap is on its right, so the correction points the other way (measured {rtl})"
        );
    }
}
