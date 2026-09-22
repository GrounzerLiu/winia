//! Segmented buttons (material3 `SegmentedButton`) — a row of equal-width segments where one or
//! several are selected.
//!
//! Facts taken from `androidx-main`'s `SegmentedButton.kt` and `tokens/OutlinedSegmentedButtonTokens.kt`
//! (the numbers are the token file's, not the spec's prose):
//! - Container 40 dp high, 1 dp outline, 18 dp icon, 8 dp icon spacing, content padding 12 dp
//!   horizontal and 8 dp vertical, `labelLarge` text, base shape `CornerFull`.
//! - `SegmentedButtonDefaults::item_shape(index, count)`: one item is a full stadium, the first rounds
//!   its start corners, the last its end corners, everything between is a rectangle. winia resolves the
//!   direction from [`WiniaTheme::direction`], so the caller passes the same index/count in LTR and RTL.
//! - Colors resolve from `enabled × active`: active is `secondaryContainer` / `onSecondaryContainer`
//!   with an `outline` border, inactive is transparent / `onSurface` with the same border, and disabled
//!   is `onSurface` (content) and `outline` at the token alphas. The container color does NOT animate
//!   between states; only the check icon animates.
//! - The row spaces items by NEGATIVE `BorderWidth` (adjacent 1 dp borders coincide into one line) and
//!   each item takes `weight(1f)` inside a row sized to `IntrinsicSize.Min` — the items come out equal
//!   to the widest one and the row wraps its content, which is what [`SegmentedRowPolicy`] reproduces.
//! - BOTH flavours show a check mark while active (`SegmentedButtonDefaults.Icon(active)` is the default
//!   icon in either row scope); a caller can replace it with `.icon(...)` or pair it with
//!   `.inactive_icon(...)`, which crossfades instead of sliding the content.
//! - A checked item (and one being pressed or focused) paints above its neighbours
//!   (`interactionZIndex = interactionCount + 5`), which winia takes from `Modifier::z_index`.
//! - The check fades in while scaling up from the BOTTOM-LEFT corner and disappears instantly on
//!   deselect (`enter = fadeIn + scaleIn(0f, TransformOrigin(0f, 1f))`, `exit = None`). The motion specs
//!   are not published as tokens for this component (the source carries its own TODO), so the spring
//!   parameters here are ours while the behavior is the source's.

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement, Point, Size, measure_node};
use crate::modifier::{Color, GraphicsLayerParams, Modifier, Shape, TransformOrigin};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::text::ProvideTextStyle;
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// Material Icons "check" (24 dp viewBox, filled) — the icon an active segment shows by default.
pub const CHECK_ICON_PATH: &str = "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";

/// The colors of a segmented button, by state (material3 `SegmentedButtonColors`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentedButtonColors {
    pub active_container: Color,
    pub active_content: Color,
    pub active_border: Color,
    pub inactive_container: Color,
    pub inactive_content: Color,
    pub inactive_border: Color,
    pub disabled_active_container: Color,
    pub disabled_active_content: Color,
    pub disabled_active_border: Color,
    pub disabled_inactive_container: Color,
    pub disabled_inactive_content: Color,
    pub disabled_inactive_border: Color,
}

impl SegmentedButtonColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        active_container: Color,
        active_content: Color,
        active_border: Color,
        inactive_container: Color,
        inactive_content: Color,
        inactive_border: Color,
        disabled_active_container: Color,
        disabled_active_content: Color,
        disabled_active_border: Color,
        disabled_inactive_container: Color,
        disabled_inactive_content: Color,
        disabled_inactive_border: Color,
    ) -> Self {
        Self {
            active_container,
            active_content,
            active_border,
            inactive_container,
            inactive_content,
            inactive_border,
            disabled_active_container,
            disabled_active_content,
            disabled_active_border,
            disabled_inactive_container,
            disabled_inactive_content,
            disabled_inactive_border,
        }
    }

    /// The container fill for the state (Compose's `containerColor(enabled, active)`).
    pub fn container_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_container,
            (true, false) => self.inactive_container,
            (false, true) => self.disabled_active_container,
            (false, false) => self.disabled_inactive_container,
        }
    }

    /// The label / icon color for the state.
    pub fn content_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_content,
            (true, false) => self.inactive_content,
            (false, true) => self.disabled_active_content,
            (false, false) => self.disabled_inactive_content,
        }
    }

    /// The outline color for the state.
    pub fn border_color(&self, enabled: bool, active: bool) -> Color {
        match (enabled, active) {
            (true, true) => self.active_border,
            (true, false) => self.inactive_border,
            (false, true) => self.disabled_active_border,
            (false, false) => self.disabled_inactive_border,
        }
    }
}

/// Defaults and factories for segmented buttons (material3 `SegmentedButtonDefaults`).
pub struct SegmentedButtonDefaults;

impl SegmentedButtonDefaults {
    /// `OutlinedSegmentedButtonTokens.ContainerHeight`
    pub const HEIGHT: f32 = 40.0;
    /// `OutlinedSegmentedButtonTokens.OutlineWidth` — also the row's default overlap.
    pub const BORDER_WIDTH: f32 = 1.0;
    /// `OutlinedSegmentedButtonTokens.IconSize`
    pub const ICON_SIZE: f32 = 18.0;
    /// Compose's private `IconSpacing`.
    pub const ICON_SPACING: f32 = 8.0;
    /// The icon slot the label's x is measured from: `IconSize + IconSpacing`.
    pub const ICON_SLOT: f32 = Self::ICON_SIZE + Self::ICON_SPACING;
    /// Where the label sits with no icon: half the slot, so it is centred without one (Compose's
    /// `-(IconSize + IconSpacing) / 2`).
    pub const LABEL_OFFSET_HIDDEN: f32 = -Self::ICON_SLOT / 2.0;
    /// `ContentPadding`'s horizontal and vertical halves.
    pub const CONTENT_PADDING_H: f32 = 12.0;
    pub const CONTENT_PADDING_V: f32 = 8.0;
    /// `ButtonDefaults.MinWidth` — the narrowest an item gets.
    pub const MIN_WIDTH: f32 = 58.0;
    /// Half of [`Self::HEIGHT`]: the base shape is `CornerFull`, and our per-corner shapes take a
    /// radius rather than a percentage, so the ends are exactly round at the token height.
    pub const OUTER_CORNER_RADIUS: f32 = Self::HEIGHT / 2.0;
    /// Compose's private `CheckedZIndexFactor`.
    pub const CHECKED_Z: f32 = 5.0;
    /// One interaction's worth of z (Compose counts interactions; pressed or focused counts as one).
    pub const INTERACTING_Z: f32 = 1.0;

    /// The four-state default colors (Compose's `defaultSegmentedButtonColors`).
    pub fn colors(theme: &ThemeColors) -> SegmentedButtonColors {
        let disabled_content =
            Color::from_argb(97, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b);
        let disabled_border =
            Color::from_argb(31, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b);
        SegmentedButtonColors::new(
            theme.secondary_container,
            theme.on_secondary_container,
            theme.outline,
            Color::TRANSPARENT,
            theme.on_surface,
            theme.outline,
            theme.secondary_container,
            disabled_content,
            disabled_border,
            Color::TRANSPARENT,
            disabled_content,
            disabled_border,
        )
    }

    /// The shape of the item at `index` in a row of `count` (material3 `itemShape`). Direction-aware:
    /// "start" follows [`WiniaTheme::direction`], so an RTL row rounds the other two corners.
    pub fn item_shape(index: usize, count: usize) -> Shape {
        if count <= 1 {
            return Shape::Pill;
        }
        let rtl = WiniaTheme::direction() == crate::layout::LayoutDirection::Rtl;
        if index == 0 {
            if rtl {
                Shape::right_rounded(Self::OUTER_CORNER_RADIUS)
            } else {
                Shape::left_rounded(Self::OUTER_CORNER_RADIUS)
            }
        } else if index + 1 >= count {
            if rtl {
                Shape::left_rounded(Self::OUTER_CORNER_RADIUS)
            } else {
                Shape::right_rounded(Self::OUTER_CORNER_RADIUS)
            }
        } else {
            Shape::Rectangle
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Rows
// ═══════════════════════════════════════════════════════════

macro_rules! segmented_row {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        ///
        /// Equal-width items that share their 1 dp borders and are vertically centred in the row's
        /// height (see [`SegmentedRowPolicy`]). Give each item its
        /// [`SegmentedButtonDefaults::item_shape`] — the shape has to be known when the item is built,
        /// and only the caller knows the index and the count (Compose makes the same demand).
        pub struct $name {
            modifier: Modifier,
            space: f32,
        }

        impl $name {
            pub fn new() -> Self {
                Self { modifier: Modifier::new(), space: SegmentedButtonDefaults::BORDER_WIDTH }
            }

            pub fn modifier(mut self, m: Modifier) -> Self {
                self.modifier = self.modifier.then(m);
                self
            }

            /// How much adjacent items overlap (Compose's `space`). Defaults to the border width, so
            /// two 1 dp outlines coincide into one line; a caller using a thicker border passes it here.
            pub fn space(mut self, space: f32) -> Self {
                self.space = space;
                self
            }

            pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
                let key = ctx.next_key();
                let m = Modifier::new().fill_max_width().then(self.modifier);
                match ctx.start_restartable_group(key, m, SegmentedRowPolicy { overlap: self.space }) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => content(ctx),
                }
                ctx.end_restartable_group();
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

segmented_row!(
    SingleChoiceSegmentedButtonRow,
    "A row of segmented buttons where exactly one is selected (material3 `SingleChoiceSegmentedButtonRow`)."
);
segmented_row!(
    MultiChoiceSegmentedButtonRow,
    "A row of segmented buttons where several may be selected (material3 `MultiChoiceSegmentedButtonRow`)."
);

/// Places the items as one strip: equal width (the widest item's natural width, floored at
/// [`SegmentedButtonDefaults::MIN_WIDTH`]), each vertically centred, adjacent items overlapping by
/// `overlap` so their 1 dp borders coincide.
///
/// Compose reaches the same geometry with `Arrangement.spacedBy(-space)` and `weight(1f)` inside a row
/// sized to `IntrinsicSize.Min` plus `Alignment.CenterVertically`. One deliberate difference: when the
/// parent is narrower than the strip wants to be, winia shrinks the items instead of overflowing.
#[derive(Debug)]
struct SegmentedRowPolicy {
    overlap: f32,
}

impl MeasurePolicy for SegmentedRowPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        if children.is_empty() {
            return (Size::new(0.0, 0.0), Vec::new());
        }
        let n = children.len() as f32;
        let avail = constraints.max_width;

        // Natural pass: the widest item's width and the tallest item's height.
        let mut natural = SegmentedButtonDefaults::MIN_WIDTH;
        let mut height = SegmentedButtonDefaults::HEIGHT;
        for &child in children {
            let (size, _) = measure_node(
                nodes,
                policies,
                child,
                Constraints::new(SegmentedButtonDefaults::MIN_WIDTH, avail, 0.0, f32::MAX),
            );
            natural = natural.max(size.width);
            height = height.max(size.height);
        }
        // All of them equally wide, shrunk to what the parent can hold when it is too narrow. The
        // placement uses the TIGHT size rather than whatever the item's own policy reported: an item
        // whose label is narrower than the row stretches, and it is this size the renderer applies.
        let fit = ((avail + (n - 1.0) * self.overlap) / n).max(0.0);
        let item_w = natural.min(fit);
        let mut placements = Vec::with_capacity(children.len());
        for (i, &child) in children.iter().enumerate() {
            let _ = measure_node(
                nodes,
                policies,
                child,
                Constraints::new(item_w, item_w, height, height),
            );
            placements.push(Placement {
                size: Size::new(item_w, height),
                position: Point::new(i as f32 * (item_w - self.overlap), 0.0),
            });
        }
        (Size::new(n * item_w - (n - 1.0) * self.overlap, height), placements)
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

// ═══════════════════════════════════════════════════════════
// Item
// ═══════════════════════════════════════════════════════════

/// One segment (material3 `SegmentedButton`).
///
/// [`SegmentedButton::new`] is the selectable (single-choice) flavour and [`SegmentedButton::toggle`]
/// the toggleable (multi-choice) one; the difference is what a click reports. Both show the check icon
/// while active, as Compose's two row scopes do.
pub struct SegmentedButton {
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    shape: Shape,
    colors: Option<SegmentedButtonColors>,
    content_padding: Option<(f32, f32)>,
    border: Option<(f32, Color)>,
    interaction_source: Option<MutableInteractionSource>,
    icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    inactive_icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
}

impl SegmentedButton {
    /// A selectable segment: `selected` drives the visuals, `on_click` reports the pick.
    pub fn new(selected: bool, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_state(selected, Some(Arc::new(on_click)), None)
    }

    /// A toggleable segment: `checked` drives the visuals, `on_checked_change` reports the new value.
    pub fn toggle(checked: bool, on_checked_change: impl Fn(bool) + Send + Sync + 'static) -> Self {
        Self::with_state(checked, None, Some(Arc::new(on_checked_change)))
    }

    fn with_state(
        selected: bool,
        on_click: Option<Arc<dyn Fn() + Send + Sync>>,
        on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    ) -> Self {
        Self {
            selected,
            on_click,
            on_checked_change,
            enabled: true,
            // A caller that forgets `item_shape` gets a plain rectangle rather than a shape that
            // silently depends on the item's position.
            shape: Shape::Rectangle,
            colors: None,
            content_padding: None,
            border: None,
            interaction_source: None,
            icon: None,
            inactive_icon: None,
            modifier: Modifier::new(),
        }
    }

    /// The item's shape — normally [`SegmentedButtonDefaults::item_shape`].
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = shape;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: SegmentedButtonColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Content inset (Compose's `contentPadding`); defaults to
    /// `(SegmentedButtonDefaults::CONTENT_PADDING_H, CONTENT_PADDING_V)`.
    pub fn content_padding(mut self, horizontal: f32, vertical: f32) -> Self {
        self.content_padding = Some((horizontal, vertical));
        self
    }

    /// Border override (Compose's `border`); defaults to 1 dp of the state's border color.
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Some((width, color));
        self
    }

    /// Hoist the interaction source (press / hover / focus observation).
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// Replace the default check mark shown while the segment is active.
    pub fn icon(mut self, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.icon = Some(Box::new(icon));
        self
    }

    /// An icon to show while the segment is NOT active. Given one, the two crossfade and the content
    /// does not slide (Compose's `Icon(active, activeContent, inactiveContent)`); without one, the
    /// label slides as the active icon appears.
    pub fn inactive_icon(mut self, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.inactive_icon = Some(Box::new(icon));
        self
    }

    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = self.modifier.then(m);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, label: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.selected);
        ctx.changed(&self.enabled);
        ctx.changed(&self.shape);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| SegmentedButtonDefaults::colors(&theme));
        let shape = self.shape;
        let enabled = self.enabled;
        let active = self.selected;

        let container = colors.container_color(enabled, active);
        let content_color = colors.content_color(enabled, active);
        let border_color = colors.border_color(enabled, active);
        let (border_w, border_c) =
            self.border.unwrap_or((SegmentedButtonDefaults::BORDER_WIDTH, border_color));
        let (pad_h, pad_v) = self.content_padding.unwrap_or((
            SegmentedButtonDefaults::CONTENT_PADDING_H,
            SegmentedButtonDefaults::CONTENT_PADDING_V,
        ));

        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let st = interaction.state(enabled);
        // Compose raises a checked item by 5 and each interaction by 1, so a pressed or focused segment
        // covers its neighbours; winia's pressed/focused are booleans, which is the same z for one
        // interaction.
        let z = if active { SegmentedButtonDefaults::CHECKED_Z } else { 0.0 }
            + if st.pressed || st.focused { SegmentedButtonDefaults::INTERACTING_Z } else { 0.0 };

        let mut m = Modifier::new().background(container, shape);
        // Only attach a z when it is actually raised: an item that is neither active nor interacting
        // carries no z, so a row of them keeps the renderer's plain tree-order loop.
        if z != 0.0 {
            m = m.z_index(z);
        }
        if border_w > 0.0 {
            m = m.border(border_w, border_c, shape);
        }
        if enabled {
            let click: Option<Arc<dyn Fn() + Send + Sync>> = match (&self.on_click, &self.on_checked_change) {
                (Some(cb), _) => Some(cb.clone()),
                (None, Some(cb)) => {
                    let cb = cb.clone();
                    Some(Arc::new(move || cb(!active)))
                }
                (None, None) => None,
            };
            if let Some(cb) = click {
                m = m
                    .clickable_with_source(&interaction, move || cb())
                    .ripple_with_shape(&interaction, content_color, true, shape);
            }
        }
        m = m.then(self.modifier);

        // The label slides between "centred without an icon" and "after the icon slot" as the check
        // appears — Compose's `SegmentedButtonContentMeasurePolicy`, whose offset is an animated pixel
        // value (`-slot / 2` with no icon, `0` with one).
        let slot_offset = ctx.remember(|| SegmentedButtonDefaults::LABEL_OFFSET_HIDDEN);
        let icon = self.icon;
        let inactive_icon = self.inactive_icon;
        let crossfade = inactive_icon.is_some();
        let icon_on = active || (crossfade && !active);
        let target_offset = if active || crossfade { 0.0 } else { SegmentedButtonDefaults::LABEL_OFFSET_HIDDEN };
        let policy = SegmentedButtonContentPolicy {
            slot_offset: slot_offset.clone(),
            target_offset,
            has_icon: icon_on,
            pad_h,
            pad_v,
        };

        match ctx.start_restartable_group(key, m, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                if icon_on {
                    // The enter animation rides a graphics layer: scale and alpha from 0 about the
                    // bottom-left corner, exactly Compose's `scaleIn(0f, TransformOrigin(0f, 1f))` +
                    // `fadeIn`. When the segment goes inactive the icon is simply not composed, which
                    // is the source's `exit = None` (no exit animation).
                    let progress = ctx.remember(|| 0.0f32);
                    crate::animation::push_animatable(
                        progress.clone(),
                        1.0,
                        crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
                    );
                    let animated = active;
                    let layer = progress.clone();
                    let icon_modifier = Modifier::new()
                        .size(SegmentedButtonDefaults::ICON_SIZE, SegmentedButtonDefaults::ICON_SIZE)
                        .graphics_layer(move || {
                            let p = if animated { layer.get().clamp(0.0, 1.0) } else { 1.0 };
                            GraphicsLayerParams {
                                scale_x: p,
                                scale_y: p,
                                alpha: p,
                                transform_origin: TransformOrigin(0.0, 1.0),
                                ..Default::default()
                            }
                        });
                    let icon_key = ctx.next_key();
                    WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                        match ctx.start_restartable_group(icon_key, icon_modifier, crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                let chosen = if active { icon } else { inactive_icon };
                                let inner: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync> = chosen
                                    .unwrap_or_else(|| {
                                        Box::new(|ctx: &mut ComposeCtx| {
                                            crate::ui::icon::Icon::svg_path(CHECK_ICON_PATH)
                                                .size(SegmentedButtonDefaults::ICON_SIZE)
                                                .build(ctx);
                                        })
                                    });
                                inner(ctx);
                            }
                        }
                        ctx.end_restartable_group();
                    });
                }
                let label_key = ctx.next_key();
                WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                    let mut text_style = WiniaTheme::typography().label_large;
                    text_style.color = Some(content_color);
                    ProvideTextStyle(text_style, ctx, |ctx| {
                        match ctx.start_restartable_group(label_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => label(ctx),
                        }
                        ctx.end_restartable_group();
                    });
                });
            }
        }
        ctx.end_restartable_group();
    }
}

/// The item's content layout: an optional icon, then the label, which slides by the animated slot
/// offset — Compose's `SegmentedButtonContentMeasurePolicy` with its `Animatable<Int>` offset.
///
/// The item's own size is the slot plus the label, with the content padding around them; the row's
/// policy then gives every item the same width and centres it vertically in the row's height.
#[derive(Debug)]
struct SegmentedButtonContentPolicy {
    slot_offset: crate::core::state::State<f32>,
    /// Where the offset is heading: `0` with an icon, `-slot / 2` without one.
    target_offset: f32,
    /// Whether the icon child is present at all (a hidden, uncomposed icon still reserves the slot).
    has_icon: bool,
    pad_h: f32,
    pad_v: f32,
}

impl MeasurePolicy for SegmentedButtonContentPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        // Register the layout dependency BEFORE measuring children (an animation frame then re-measures
        // this node alone), then animate toward the target — the TabRow indicator's pattern.
        self.slot_offset.get();
        crate::animation::push_animatable(
            self.slot_offset.clone(),
            self.target_offset,
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
        );
        let offset = self.slot_offset.peek();

        let inner_w = (constraints.max_width - 2.0 * self.pad_h).max(0.0);
        let label_constraints = Constraints::new(0.0, inner_w, 0.0, f32::MAX);
        let mut placements = Vec::with_capacity(children.len());
        let mut content_h = 0.0f32;
        let mut label_w = 0.0f32;
        if self.has_icon {
            let (icon_size, _) = measure_node(nodes, policies, children[0], label_constraints);
            content_h = content_h.max(icon_size.height);
            placements.push(Placement { size: icon_size, position: Point::new(0.0, 0.0) });
        }
        let label_index = if self.has_icon { 1 } else { 0 };
        if let Some(&label_child) = children.get(label_index) {
            let (label_size, _) = measure_node(nodes, policies, label_child, label_constraints);
            content_h = content_h.max(label_size.height);
            label_w = label_size.width;
            placements.push(Placement { size: label_size, position: Point::new(0.0, 0.0) });
        }
        // The icon and the label are one block of `slot + label`; Compose centres that block in the
        // container (`Box(contentAlignment = Center)`), which is what keeps a stretched item's content
        // centred rather than pinned to its start edge.
        let block_w = SegmentedButtonDefaults::ICON_SLOT + label_w;
        let block_x = self.pad_h + ((inner_w - block_w) / 2.0).max(0.0);
        let cy = |h: f32| (content_h - h) / 2.0;
        if self.has_icon {
            placements[0].position = Point::new(block_x, self.pad_v + cy(placements[0].size.height));
        }
        if let Some(p) = placements.get_mut(if self.has_icon { 1 } else { 0 }) {
            p.position = Point::new(
                block_x + SegmentedButtonDefaults::ICON_SLOT + offset,
                self.pad_v + cy(p.size.height),
            );
        }
        (
            Size::new(block_w + 2.0 * self.pad_h, content_h + 2.0 * self.pad_v),
            placements,
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::LayoutDirection;
    use crate::ui::text::Text;
    use crate::ui::theme::ThemeColors;

    fn compose_row(count: usize, selected: usize, theme: &ThemeColors, dir: LayoutDirection) -> Composer {
        let mut composer = Composer::new();
        let theme = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme, dir, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    for i in 0..count {
                        SegmentedButton::new(i == selected, move || {})
                            .shape(SegmentedButtonDefaults::item_shape(i, count))
                            .build(ctx, |ctx| {
                                Text::new(format!("Item {i}")).build(ctx);
                            });
                    }
                });
            });
        });
        composer
    }

    fn row_children(composer: &Composer) -> Vec<usize> {
        let root = composer.layout_root_idx().expect("root");
        composer.arena_nodes()[root].children.clone()
    }

    // ── Pure rules ──

    #[test]
    fn item_shape_follows_index_count_and_direction() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                ThemeColors::light_from_seed(0x6750A4),
                LayoutDirection::Ltr,
                ctx,
                |_ctx| {
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(0, 1),
                        Shape::Pill,
                        "a lone item is a stadium"
                    );
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(0, 3),
                        Shape::left_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(2, 3),
                        Shape::right_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                    assert_eq!(SegmentedButtonDefaults::item_shape(1, 3), Shape::Rectangle);
                },
            );
        });
        // RTL rounds the other two corners, so a caller passes the same index/count either way.
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                ThemeColors::light_from_seed(0x6750A4),
                LayoutDirection::Rtl,
                ctx,
                |_ctx| {
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(0, 3),
                        Shape::right_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(2, 3),
                        Shape::left_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                },
            );
        });
    }

    #[test]
    fn colors_resolve_by_state() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let c = SegmentedButtonDefaults::colors(&theme);
        assert_eq!(c.container_color(true, true), theme.secondary_container);
        assert_eq!(c.container_color(true, false), Color::TRANSPARENT);
        assert_eq!(c.content_color(true, true), theme.on_secondary_container);
        assert_eq!(c.content_color(true, false), theme.on_surface);
        assert_eq!(c.border_color(true, true), theme.outline, "the active border is the outline too");
        assert_eq!(c.border_color(true, false), theme.outline);
        // Disabled keeps the active fill but dims content and border, as the tokens say.
        assert_eq!(c.container_color(false, true), theme.secondary_container);
        assert_eq!(c.content_color(false, true).a, 97, "onSurface @ 38%");
        assert_eq!(c.border_color(false, true).a, 31, "outline @ 12%");
        assert_eq!(c.container_color(false, false), Color::TRANSPARENT);
    }

    // ── Layout ──

    #[test]
    fn items_are_equal_width_and_share_their_borders() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = compose_row(3, 0, &theme, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(items.len(), 3);
        let w = nodes[items[0]].measured_size.width;
        assert_eq!(nodes[root].measured_size.height, SegmentedButtonDefaults::HEIGHT);
        for (i, &it) in items.iter().enumerate() {
            assert_eq!(nodes[it].measured_size.width, w, "item {i} must match the widest item");
            assert!(w >= SegmentedButtonDefaults::MIN_WIDTH, "items keep the 58 px minimum");
            let expected = i as f32 * (w - SegmentedButtonDefaults::BORDER_WIDTH);
            assert!(
                (nodes[it].position.x - expected).abs() < 0.01,
                "item {i} at x={} but the overlap arithmetic says {expected}",
                nodes[it].position.x
            );
        }
        let total = 3.0 * w - 2.0 * SegmentedButtonDefaults::BORDER_WIDTH;
        assert!(
            (nodes[root].measured_size.width - total).abs() < 0.01,
            "the row is n × w − (n − 1) borders wide"
        );
    }

    /// Equal width means the WIDEST item's width, not the parent's: a narrow row wraps (Compose's
    /// `IntrinsicSize.Min` row), and a parent too narrow to hold the strip shrinks the items instead of
    /// letting them overflow.
    #[test]
    fn the_row_wraps_to_the_widest_item_and_shrinks_when_it_must() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let t = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(t, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(true, || {})
                        .shape(SegmentedButtonDefaults::item_shape(0, 2))
                        .build(ctx, |ctx| {
                            Text::new("A very long segment label").build(ctx);
                        });
                    SegmentedButton::new(false, || {})
                        .shape(SegmentedButtonDefaults::item_shape(1, 2))
                        .build(ctx, |ctx| {
                            Text::new("B").build(ctx);
                        });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 600.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        let wide = nodes[items[0]].measured_size.width;
        assert_eq!(nodes[items[1]].measured_size.width, wide, "the short item stretches to match");
        assert!(
            wide > 200.0 && nodes[root].measured_size.width < 600.0,
            "the row wraps its content (row {} wide, items {wide})",
            nodes[root].measured_size.width
        );

        // The same row in a 120 px parent: the items shrink to fit rather than overflowing.
        composer.layout(Constraints::new(0.0, 120.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let w = nodes[items[0]].measured_size.width;
        let total = 2.0 * w - SegmentedButtonDefaults::BORDER_WIDTH;
        assert!(total <= 120.01, "the strip fits the parent ({total} <= 120)");
    }

    #[test]
    fn a_checked_item_is_raised_above_its_neighbours() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = compose_row(3, 1, &theme, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(
            nodes[items[1]].modifier.get_z_index(),
            Some(SegmentedButtonDefaults::CHECKED_Z),
            "the checked item paints above the shared edges"
        );
        assert_eq!(
            nodes[items[0]].modifier.get_z_index(),
            None,
            "an idle unchecked item carries no z, so the renderer keeps its plain loop"
        );
    }

    // ── Pixels ──

    fn render_row(count: usize, selected: usize, theme: &ThemeColors) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::surfaces;
        let mut composer = compose_row(count, selected, theme, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 60)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        (pm.pixels::<[u8; 4]>().expect("pixels").to_vec(), 300)
    }

    fn at(px: &[[u8; 4]], w: usize, x: f32, y: f32) -> (i32, i32, i32) {
        let p = px[(y as usize) * w + (x as usize)];
        (p[2] as i32, p[1] as i32, p[0] as i32)
    }

    fn close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8 && (a.2 - b.2).abs() <= 8
    }

    #[test]
    fn the_checked_item_is_filled_and_the_rest_are_outlined() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_row(3, 1, &theme);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        let outline = (theme.outline.r as i32, theme.outline.g as i32, theme.outline.b as i32);
        assert!(close(at(&px, w, 150.0, 20.0), sec), "the checked container is secondaryContainer (got {:?})", at(&px, w, 150.0, 20.0));
        // Below the label (the item is 40 px tall and the text is centred), inside the container.
        assert!(close(at(&px, w, 40.0, 35.0), (255, 255, 255)), "an unchecked container is transparent (got {:?})", at(&px, w, 40.0, 35.0));
        assert!(close(at(&px, w, 40.0, 0.0), outline), "…with a 1 px outline on the outer row (got {:?})", at(&px, w, 40.0, 0.0));
        // The strip's own edge carries the outline…
        assert!(close(at(&px, w, 0.0, 20.0), outline), "the strip's outer edge is the outline (got {:?})", at(&px, w, 0.0, 20.0));
        // …while the shared edge next to the CHECKED item is covered by its fill: a checked item paints
        // above its neighbours (Compose's `interactionZIndex`, which is what `z_index` is here for).
        assert!(close(at(&px, w, 99.0, 20.0), sec), "the checked item covers the shared edge (got {:?})", at(&px, w, 99.0, 20.0));
        assert!(close(at(&px, w, 100.0, 20.0), sec), "…and its fill continues past it (got {:?})", at(&px, w, 100.0, 20.0));
    }

    #[test]
    fn a_disabled_item_uses_the_disabled_palette() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let t = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(t, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(true, || {})
                        .shape(SegmentedButtonDefaults::item_shape(0, 1))
                        .enabled(false)
                        .build(ctx, |ctx| {
                            Text::new("Off").build(ctx);
                        });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 120.0, 0.0, 100.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((120, 60)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // A disabled ACTIVE item keeps secondaryContainer as its fill.
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        assert!(close(at(px, 120, 60.0, 20.0), sec), "a disabled active fill stays secondaryContainer (got {:?})", at(px, 120, 60.0, 20.0));
    }
}
