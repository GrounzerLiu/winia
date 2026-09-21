//! Modal navigation drawer — Compose Material3 `ModalNavigationDrawer` parity.
//!
//! Structure (mirrors androidx `NavigationDrawer.kt`):
//! - [`ModalNavigationDrawer`] owns the state, the anchors, the placement and the
//!   scrim, and puts the app content behind both. It is an **in-tree** component
//!   (a `Stack`), exactly like Compose's — not an overlay like [`ModalBottomSheet`]:
//!   the drawer is part of the app shell, and the content under it keeps its
//!   composition state.
//! - [`ModalDrawerSheet`] is the surface the caller nests inside the drawer slot:
//!   width, corner shape, container color and content padding.
//! - [`NavigationDrawerItem`] is the label/icon/badge row with the pill indicator.
//!
//! Anchors (Compose's `calculateAnchors`): `Open at 0`, `Closed at ∓sheetWidth` —
//! negative in LTR (the sheet leaves through the left edge) and positive in RTL,
//! where the drawer docks at the right. The host aligns the sheet to the leading
//! edge, so the same `0 → closed` offset drives both directions.
//!
//! Deviations from Compose (winia degradations), all deliberate:
//! - **Gestures.** Compose hangs a horizontal `anchoredDraggable` on the whole
//!   drawer box, so the moves reach it whatever child is under the finger. winia
//!   captures ONE gesture node per down (the innermost with a gesture), so the drag
//!   is wired on the host stack and on the scrim — a swipe from the sheet's own
//!   surface or from the scrim drags the drawer, but a swipe starting on a row or a
//!   button is captured by that child and does not. Scroll containers and inner
//!   draggables keep their own area (`app.rs` `inner_component_drag`). See
//!   `docs/navigation-drawer.md` for the table.
//! - **Escape does not close the drawer.** winia's Escape handling lives in the
//!   overlay path (`app.rs`), which only main-tree overlays reach; the drawer is
//!   in-tree with no key focus of its own. Close it from the scrim, a gesture, or
//!   `DrawerState::close()` — e.g. from a hamburger button.
//! - **No suspending API.** `open`/`close` push a tween onto the offset state
//!   (Compose suspends); the animation is the shared settled-anchor tween.
//! - **`DismissibleNavigationDrawer` / `PermanentNavigationDrawer`** are not
//!   implemented yet — only the modal form and its sheet.
//! - The scrim keeps winia's modal-scrim alpha (110/255, the same one
//!   `open_overlay` draws for modals) instead of re-deriving Compose's
//!   `ScrimTokens` value; override it per drawer with
//!   [`ModalNavigationDrawer::scrim_color`].

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::anchored_draggable::AnchoredDraggableState;
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::layout_components::{Column, Row, Stack};
use crate::ui::theme::WiniaTheme;
use crate::unit::{current_density, Dp};
use std::sync::Arc;
use std::time::Duration;

/// `NavigationDrawerTokens.ContainerWidth` — the sheet's maximum width (360dp).
pub const DRAWER_MAX_WIDTH: f32 = 360.0;
/// `MinimumDrawerWidth` (240dp) — only reached when `maximum_drawer_width` is
/// configured below it.
pub const DRAWER_MIN_WIDTH: f32 = 240.0;
/// `NavigationDrawerTokens.ContainerShape` = `ShapeKeyTokens.CornerLargeEnd`, the
/// M3 Large corner (16dp) on the two corners facing the content.
pub const DRAWER_CORNER_RADIUS: f32 = 16.0;
/// `NavigationDrawerTokens.ActiveIndicatorHeight` — the drawer item's height (56dp).
pub const DRAWER_ITEM_HEIGHT: f32 = 56.0;
/// `NavigationDrawerTokens.IconSize` (24dp).
pub const DRAWER_ITEM_ICON_SIZE: f32 = 24.0;
/// Horizontal padding the sheet applies to its content (12dp each side), which is
/// what makes `NavigationDrawerItem` span `ActiveIndicatorWidth` = 336dp.
pub const DRAWER_SHEET_HORIZONTAL_PADDING: f32 = 12.0;
/// `NavigationDrawerItem`'s content padding — start 16dp, end 24dp.
pub const DRAWER_ITEM_START_PADDING: f32 = 16.0;
pub const DRAWER_ITEM_END_PADDING: f32 = 24.0;
/// Gap between the icon, the label and the badge (12dp).
pub const DRAWER_ITEM_SLOT_GAP: f32 = 12.0;

/// `DrawerVelocityThreshold` = 400dp/s — the drawer needs a deliberate flick, not
/// the 125dp/s `AnchoredDraggable` default.
const DRAWER_VELOCITY_THRESHOLD_DP: f32 = 400.0;

/// Scrim alpha (110/255) — shared with winia's modal overlay scrim so a drawer and
/// a dialog darken the page by the same amount.
const DRAWER_SCRIM_ALPHA: u8 = 110;

// ═══════════════ DrawerValue ═══════════════

/// Which anchor the drawer is parked at (Compose `DrawerValue`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrawerValue {
    /// Fully hidden — off the leading edge.
    Closed,
    /// Fully visible — flush with the leading edge.
    Open,
}

impl PartialOrd for DrawerValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DrawerValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // In LTR the closed anchor sits at a SMALLER position than the open one, which
        // is the natural order. RTL reverses the positions but not this ordering: it is
        // only used to key the anchor map and to make `pairs()` deterministic, while
        // every lookup (closest / directional / min / max) goes through the positions.
        let order = |v: &DrawerValue| match v {
            DrawerValue::Closed => 0,
            DrawerValue::Open => 1,
        };
        order(self).cmp(&order(other))
    }
}

// ═══════════════ DrawerState ═══════════════

/// Drawer state (Compose `DrawerState`) — two anchors over [`AnchoredDraggableState`].
pub struct DrawerState {
    anchored: AnchoredDraggableState<DrawerValue>,
}

impl DrawerState {
    /// Create the state parked at `initial_value` (usually `Closed`).
    pub fn new(initial_value: DrawerValue) -> Self {
        let mut anchored = AnchoredDraggableState::new(initial_value);
        anchored.set_velocity_threshold_dp(DRAWER_VELOCITY_THRESHOLD_DP);
        Self { anchored }
    }

    /// Veto/confirm hook (Compose `confirmStateChange`); `false` rolls the gesture back.
    pub fn set_confirm_value_change(
        &mut self,
        f: impl Fn(DrawerValue) -> bool + Send + Sync + 'static,
    ) {
        self.anchored.set_confirm_value_change(move |v: &DrawerValue| f(*v));
    }

    /// Recompute the anchors for a sheet `width` px wide.
    ///
    /// Compose's `calculateAnchors`: `Open at 0`, `Closed at -width` in LTR and
    /// `Closed at +width` in RTL — the sign is what makes a drag-away-from-the-edge
    /// close the drawer in both directions, with no delta-sign bookkeeping at the
    /// call site.
    ///
    /// This runs on every compose, so it must be inert unless the geometry actually
    /// changed. androidx guards the same thing on the closed anchor
    /// (`NavigationDrawer.kt`: `currentClosedAnchor != calculatedClosedAnchor`), and the
    /// guard is load-bearing: a live drag moves the offset while the parked value stays
    /// put, so re-aligning the offset to the parked anchor here would undo one drag frame
    /// per compose — i.e. the drawer would not drag at all. (An in-flight *animation* is
    /// not the only case to exclude; it was the only one excluded at first, and the
    /// gesture silently did nothing.)
    pub fn update_anchors(&self, width: f32, rtl: bool) {
        let closed = if rtl { width } else { -width };
        let anchors = crate::ui::anchored_draggable::DraggableAnchors::new([
            (DrawerValue::Open, 0.0),
            (DrawerValue::Closed, closed),
        ]);
        let before = self.anchored.peek_position_of(&DrawerValue::Closed);
        self.anchored.update_anchors(anchors);
        let after = self.anchored.peek_position_of(&DrawerValue::Closed);
        let geometry_changed = match (before.is_nan(), after.is_nan()) {
            (true, false) | (false, true) => true,
            (false, false) => (before - after).abs() > 0.5,
            (true, true) => false,
        };
        if !geometry_changed {
            return;
        }
        // The window or the direction changed: keep the parked value flush with its
        // (new) anchor so an open drawer does not hang over the edge.
        let parked = self.anchored.settled_value();
        let pos = self.anchored.peek_position_of(&parked);
        if pos.is_nan() {
            return;
        }
        let off = self.anchored.offset();
        if (off - pos).abs() > 0.5 {
            self.anchored.offset_state().set(pos);
        }
    }

    // ── reads ──

    /// The parked value — Compose's `DrawerState.currentValue` (the underlying
    /// `settledValue`: an in-flight drag does not change it).
    pub fn current_value(&self) -> DrawerValue {
        self.anchored.settled_value()
    }

    /// Where the drawer is heading: the drag target while dragging, else the
    /// nearest anchor to the current offset.
    pub fn target_value(&self) -> DrawerValue {
        self.anchored.target_value()
    }

    pub fn is_open(&self) -> bool {
        self.current_value() == DrawerValue::Open
    }

    pub fn is_closed(&self) -> bool {
        self.current_value() == DrawerValue::Closed
    }

    /// Animation progress `Closed → Open` (0 parked closed, 1 parked open).
    /// NaN offsets (before the first layout) read as 0.
    pub fn progress(&self) -> f32 {
        if self.anchored.offset().is_nan() {
            return 0.0;
        }
        self.anchored.progress(&DrawerValue::Closed, &DrawerValue::Open)
    }

    /// [`Self::progress`] without registering a dependency — the render-time read.
    pub fn peek_progress(&self) -> f32 {
        if self.anchored.offset_state().peek().is_nan() {
            return 0.0;
        }
        self.anchored.peek_progress(&DrawerValue::Closed, &DrawerValue::Open)
    }

    /// Current offset in px (NaN before the first layout).
    pub fn current_offset(&self) -> f32 {
        self.anchored.offset()
    }

    /// The offset `State` the sheet's placement follows (a layout dependency, so the
    /// slide re-lays-out without recomposing).
    pub fn offset_state(&self) -> State<f32> {
        self.anchored.offset_state()
    }

    pub fn is_animation_running(&self) -> bool {
        self.anchored.is_animation_running()
    }

    /// Last drag velocity (px/s), expiring to 0 after a hold-still.
    pub fn last_velocity(&self) -> f32 {
        self.anchored.last_velocity()
    }

    /// The underlying anchored-draggable state (advanced use: custom thresholds).
    pub fn anchored_draggable(&self) -> &AnchoredDraggableState<DrawerValue> {
        &self.anchored
    }

    // ── writes ──

    /// Animate to the open anchor.
    pub fn open(&self) {
        self.anchored.animate_to(DrawerValue::Open);
    }

    /// Animate to the closed anchor.
    pub fn close(&self) {
        self.anchored.animate_to(DrawerValue::Closed);
    }

    /// Jump to a value with no animation.
    pub fn snap_to(&self, target: DrawerValue) {
        self.anchored.snap_to(target);
    }

    /// Feed a horizontal drag delta (from `Modifier::on_drag`).
    pub fn drag_delta(&self, dx: f32) {
        self.anchored.drag_delta(dx);
    }

    /// Settle on release, using the last tracked velocity.
    pub fn settle_with_velocity(&self, velocity: f32) -> DrawerValue {
        self.anchored.settle_with_velocity(velocity, None)
    }
}

impl Clone for DrawerState {
    fn clone(&self) -> Self {
        Self { anchored: self.anchored.clone() }
    }
}

// ═══════════════ DrawerDefaults ═══════════════

/// Token defaults (Compose `DrawerDefaults` / `NavigationDrawerTokens`).
pub struct DrawerDefaults;

impl DrawerDefaults {
    /// `MaximumDrawerWidth` / `ContainerWidth`.
    pub const MAXIMUM_DRAWER_WIDTH: Dp = Dp(DRAWER_MAX_WIDTH);
    /// `MinimumDrawerWidth`.
    pub const MINIMUM_DRAWER_WIDTH: Dp = Dp(DRAWER_MIN_WIDTH);
    /// The sheet's surface shape for a drawer docked at the leading edge: rounded on
    /// the side that faces the content, square against the window edge.
    pub fn shape(direction: LayoutDirection) -> Shape {
        match direction {
            LayoutDirection::Ltr => Shape::RightRoundedRect { radius: DRAWER_CORNER_RADIUS },
            LayoutDirection::Rtl => Shape::LeftRoundedRect { radius: DRAWER_CORNER_RADIUS },
        }
    }

    /// `ModalContainerColor` (`SurfaceContainerLow`).
    pub fn modal_container_color(theme: &crate::ui::theme::ThemeColors) -> Color {
        theme.surface_container_low
    }

    /// The scrim: black at winia's modal alpha.
    pub fn scrim_color() -> Color {
        Color::from_argb(DRAWER_SCRIM_ALPHA, 0, 0, 0)
    }

    /// Item colors (`NavigationDrawerItemDefaults.colors()`).
    pub fn item_colors(theme: &crate::ui::theme::ThemeColors) -> NavigationDrawerItemColors {
        NavigationDrawerItemColors {
            selected_container: theme.secondary_container,
            selected_icon: theme.on_secondary_container,
            selected_text: theme.on_secondary_container,
            unselected_container: Color::TRANSPARENT,
            unselected_icon: theme.on_surface_variant,
            unselected_text: theme.on_surface_variant,
        }
    }
}

/// Resolve the sheet width: `sizeIn(minWidth, maxWidth)` against the window.
fn resolve_sheet_width(max_width: f32, min_width: f32, window_width: f32) -> f32 {
    max_width.min(window_width).max(min_width.min(window_width))
}

// ═══════════════ NavigationDrawerItem ═══════════════

/// Item colors (Compose `NavigationDrawerItemColors`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavigationDrawerItemColors {
    /// Indicator fill while selected (`ActiveIndicatorColor` = `SecondaryContainer`).
    pub selected_container: Color,
    /// Icon tint while selected (`OnSecondaryContainer`).
    pub selected_icon: Color,
    /// Label color while selected (`OnSecondaryContainer`).
    pub selected_text: Color,
    /// Indicator fill while unselected (transparent — the pill is invisible).
    pub unselected_container: Color,
    /// Icon tint while unselected (`OnSurfaceVariant`).
    pub unselected_icon: Color,
    /// Label color while unselected (`OnSurfaceVariant`).
    pub unselected_text: Color,
}

impl NavigationDrawerItemColors {
    pub fn container(&self, selected: bool) -> Color {
        if selected { self.selected_container } else { self.unselected_container }
    }
    pub fn icon(&self, selected: bool) -> Color {
        if selected { self.selected_icon } else { self.unselected_icon }
    }
    pub fn text(&self, selected: bool) -> Color {
        if selected { self.selected_text } else { self.unselected_text }
    }
}

/// A drawer row: icon, label, optional badge, pill indicator (Compose
/// `NavigationDrawerItem`).
///
/// Compose has no `enabled` parameter on this item, and neither does this one —
/// gate the callback on the caller's side if a row must be inert.
pub struct NavigationDrawerItem {
    label: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    badge: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    shape: Option<Shape>,
    colors: Option<NavigationDrawerItemColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl NavigationDrawerItem {
    pub fn new(
        label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
        selected: bool,
    ) -> Self {
        Self {
            label: Box::new(label),
            selected,
            on_click: None,
            icon: None,
            badge: None,
            shape: None,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    /// Text label shorthand (the common case).
    pub fn text_label(label: impl Into<String>, selected: bool) -> Self {
        let label = label.into();
        Self::new(move |ctx| { crate::ui::Text::new(label).build(ctx); }, selected)
    }

    pub fn on_click(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(cb));
        self
    }

    pub fn icon(mut self, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.icon = Some(Box::new(icon));
        self
    }

    pub fn badge(mut self, badge: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.badge = Some(Box::new(badge));
        self
    }

    /// Indicator shape (Compose `shape` = `ActiveIndicatorShape`, i.e. `CornerFull`).
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    pub fn colors(mut self, colors: NavigationDrawerItemColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.selected);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| DrawerDefaults::item_colors(&theme));
        let selected = self.selected;
        // `ActiveIndicatorShape` = CornerFull → the pill.
        let shape = self.shape.unwrap_or(Shape::Pill);
        // The indicator color animates between the unselected and selected fills
        // (Compose `animateColorAsState`). It is a *paint* value: the animation state
        // is peeked by the background closure, so the fade costs no recomposition.
        let indicator = ctx.animate_color_as_state(
            colors.container(selected),
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                Duration::from_millis(150),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
        let interaction = self
            .interaction_source
            .clone()
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let ripple_color = colors.icon(selected);

        let mut modifier = Modifier::new()
            .fill_max_width()
            // androidx uses `heightIn(min = ActiveIndicatorHeight)`, not a fixed height:
            // a single-line row is 56dp either way, but a two-line label or a large badge
            // grows the row instead of being squeezed into it.
            .min_height(DRAWER_ITEM_HEIGHT)
            .background(move || indicator.peek(), shape)
            .padding_sides(DRAWER_ITEM_START_PADDING, 0.0, DRAWER_ITEM_END_PADDING, 0.0)
            .then(self.modifier);
        if let Some(cb) = self.on_click {
            modifier = modifier
                .clickable_with_source(&interaction, move || cb())
                .ripple_with_shape(&interaction, ripple_color, true, shape);
        }

        let icon = self.icon;
        let label = self.label;
        let badge = self.badge;
        let content_color = colors.text(selected);
        let icon_color = colors.icon(selected);

        match ctx.start_restartable_group(
            key,
            modifier,
            BoxLayout::new().alignment(Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let has_badge = badge.is_some();
                Row::new()
                    .alignment(Alignment::Center)
                    // androidx inserts the gap explicitly before the label and before the
                    // badge; with the label taking the slack, the row's spacing is what
                    // keeps the icon from sitting flush against the text.
                    .spacing(DRAWER_ITEM_SLOT_GAP)
                    .build(ctx, |ctx| {
                        if let Some(icon) = icon {
                            WiniaTheme::with_content_color(icon_color, ctx, |ctx| {
                                Stack::new()
                                    .modifier(Modifier::new().size(
                                        DRAWER_ITEM_ICON_SIZE,
                                        DRAWER_ITEM_ICON_SIZE,
                                    ))
                                    .build(ctx, |ctx| { icon(ctx); });
                            });
                        }
                        // Label takes the slack between the icon and the badge.
                        Column::new()
                            .modifier(Modifier::new().layout_weight(1.0))
                            .alignment(Alignment::Start)
                            .build(ctx, |ctx| {
                                WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                                    label(ctx);
                                });
                            });
                        if has_badge {
                            if let Some(badge) = badge {
                                WiniaTheme::with_content_color(icon_color, ctx, |ctx| {
                                    badge(ctx);
                                });
                            }
                        }
                    });
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

// ═══════════════ ModalDrawerSheet ═══════════════

/// The drawer's surface — width, corners, container color, content padding
/// (Compose `ModalDrawerSheet`).
///
/// Nest it in [`ModalNavigationDrawer`]'s drawer slot. Compose's `ModalDrawerElevation`
/// is `ElevationTokens.Level0`, so the default draws no shadow; set
/// [`Self::elevation`] to add one.
pub struct ModalDrawerSheet {
    modifier: Modifier,
    shape: Option<Shape>,
    container_color: Option<Color>,
    content_color: Option<Color>,
    elevation: f32,
    content_padding: bool,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
}

impl ModalDrawerSheet {
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            modifier: Modifier::new(),
            shape: None,
            container_color: None,
            content_color: None,
            elevation: 0.0,
            content_padding: true,
            content: Box::new(content),
        }
    }

    /// Surface shape (default: [`DrawerDefaults::shape`] resolved from the layout
    /// direction).
    pub fn shape(mut self, shape: Shape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// `drawerContainerColor` (default `ModalContainerColor` = `SurfaceContainerLow`).
    pub fn container_color(mut self, color: Color) -> Self {
        self.container_color = Some(color);
        self
    }

    /// `drawerContentColor` (default `contentColorFor(containerColor)` = `OnSurface`).
    pub fn content_color(mut self, color: Color) -> Self {
        self.content_color = Some(color);
        self
    }

    /// `drawerTonalElevation` in dp (default 0).
    pub fn elevation(mut self, dp: f32) -> Self {
        self.elevation = dp;
        self
    }

    /// Apply the sheet's 12dp horizontal content padding. The drawer's own rows want
    /// it (that padding is what sizes a `NavigationDrawerItem`'s indicator to
    /// `ActiveIndicatorWidth`); a full-bleed header may want it off.
    pub fn content_padding(mut self, enabled: bool) -> Self {
        self.content_padding = enabled;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = WiniaTheme::colors();
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        ctx.changed(&direction);
        ctx.changed(&self.shape);
        ctx.changed(&self.container_color);
        ctx.changed(&self.content_color);
        ctx.changed(&self.elevation);
        ctx.changed(&self.content_padding);
        let shape = self.shape.unwrap_or_else(|| DrawerDefaults::shape(direction));
        let container = self.container_color.unwrap_or_else(|| DrawerDefaults::modal_container_color(&theme));
        let content_color = self.content_color.unwrap_or(theme.on_surface);
        let elevation = self.elevation;
        let padding = self.content_padding;
        let content = self.content;
        // Shadow sits OUTSIDE the clip (it must be able to paint past the rounded
        // corner), then the surface fills and clips its children.
        let mut modifier = Modifier::new()
            .fill_max_height()
            .shadow(elevation, shape, false, Color::from_argb(40, 0, 0, 0))
            .background(container, shape)
            .clip(shape);
        if padding {
            modifier = modifier.padding_sides(
                DRAWER_SHEET_HORIZONTAL_PADDING,
                0.0,
                DRAWER_SHEET_HORIZONTAL_PADDING,
                0.0,
            );
        }
        let modifier = modifier.then(self.modifier);
        WiniaTheme::with_content_color(content_color, ctx, |ctx| {
            Column::new().modifier(modifier).build(ctx, content);
        });
    }
}

// ═══════════════ ModalNavigationDrawer ═══════════════

/// Modal navigation drawer (Compose `ModalNavigationDrawer`).
///
/// The app content stays in the tree underneath; the drawer slides over it behind a
/// scrim, and the scrim blocks the content's input while the drawer is out.
pub struct ModalNavigationDrawer {
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    drawer_content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    drawer_state: Option<DrawerState>,
    gestures_enabled: bool,
    scrim_color: Option<Color>,
    maximum_drawer_width: Dp,
    minimum_drawer_width: Dp,
    modifier: Modifier,
}

impl ModalNavigationDrawer {
    /// `content` is the app shell the drawer slides over.
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            content: Box::new(content),
            drawer_content: None,
            drawer_state: None,
            gestures_enabled: true,
            scrim_color: None,
            maximum_drawer_width: DrawerDefaults::MAXIMUM_DRAWER_WIDTH,
            minimum_drawer_width: DrawerDefaults::MINIMUM_DRAWER_WIDTH,
            modifier: Modifier::new(),
        }
    }

    /// The drawer's own content — typically a [`ModalDrawerSheet`].
    pub fn drawer_content(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.drawer_content = Some(Box::new(content));
        self
    }

    /// Externally owned state (default: remembered internally, parked closed).
    pub fn drawer_state(mut self, state: DrawerState) -> Self {
        self.drawer_state = Some(state);
        self
    }

    /// Whether a horizontal drag on the page moves the drawer (default true).
    pub fn gestures_enabled(mut self, enabled: bool) -> Self {
        self.gestures_enabled = enabled;
        self
    }

    /// Scrim color (default [`DrawerDefaults::scrim_color`]).
    pub fn scrim_color(mut self, color: Color) -> Self {
        self.scrim_color = Some(color);
        self
    }

    /// `MaximumDrawerWidth` (default 360dp).
    pub fn maximum_drawer_width(mut self, width: Dp) -> Self {
        self.maximum_drawer_width = width;
        self
    }

    /// `MinimumDrawerWidth` (default 240dp).
    pub fn minimum_drawer_width(mut self, width: Dp) -> Self {
        self.minimum_drawer_width = width;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let direction = self.modifier.get_layout_direction().unwrap_or(WiniaTheme::direction());
        let rtl = direction == LayoutDirection::Rtl;
        ctx.changed(&direction);
        ctx.changed(&self.gestures_enabled);
        ctx.changed(&self.scrim_color);
        ctx.changed(&self.maximum_drawer_width);
        ctx.changed(&self.minimum_drawer_width);

        let holder = ctx.remember(|| DrawerState::new(DrawerValue::Closed));
        let state = match self.drawer_state {
            Some(s) => s,
            None => holder.get(),
        };
        // The sheet width is known at compose time (it is a token clipped to the
        // window), so the anchors never lag a frame behind the layout — unlike the
        // bottom sheet, which has to measure its content first.
        //
        // Units: layout coordinates are LOGICAL px and this framework's own convention
        // is `1dp == 1 logical px` (`Dimension::Dp` needs no conversion), so the dp
        // tokens are used as-is. `Dp::to_px` would be the physical value and would
        // size the drawer against the wrong space (at 1.5x a 360dp drawer would try to
        // be 540 logical px wide and simply fill the window).
        let window_w = crate::ui::window_size().0;
        let sheet_w = resolve_sheet_width(
            self.maximum_drawer_width.0,
            self.minimum_drawer_width.0,
            window_w,
        );
        state.update_anchors(sheet_w, rtl);

        let scrim_color = self.scrim_color.unwrap_or_else(DrawerDefaults::scrim_color);
        let gestures = self.gestures_enabled;
        let content = self.content;
        let drawer_content = self.drawer_content;
        let mut host = Modifier::new().fill_max_size();
        if gestures {
            let drag_state = state.clone();
            let end_state = state.clone();
            host = host
                .on_drag(move |_pos, (dx, _dy)| drag_state.drag_delta(dx))
                .on_drag_end(move || {
                    let v = end_state.last_velocity();
                    end_state.settle_with_velocity(v);
                });
        }
        let host = host.then(self.modifier);

        Stack::new()
            .modifier(host)
            // The sheet is placed against the leading edge, so one offset convention
            // (open at 0, closed at ∓width) covers both directions.
            .alignment(if rtl { Alignment::End } else { Alignment::Start })
            .build(ctx, |ctx| {
                content(ctx);
                scrim(ctx, &state, scrim_color, sheet_w, gestures);
                if let Some(drawer_content) = drawer_content {
                    let sheet_modifier = Modifier::new()
                        .width(sheet_w)
                        .fill_max_height()
                        // Layout offset, not a graphics-layer translation: placement
                        // participates in hit testing, so a closed drawer (parked off
                        // the edge) cannot be clicked, and the open one is hit where it
                        // is drawn. The offset state re-lays-out per frame, so the
                        // slide costs no recomposition.
                        .absolute_offset(state.offset_state(), 0.0);
                    Stack::new()
                        .modifier(sheet_modifier)
                        .build(ctx, drawer_content);
                }
            });
    }
}

/// The scrim, in a group of its own.
///
/// Its *presence* is structural — a full-window click-catcher must actually vanish
/// once the drawer parks closed, or it would swallow every click on the page — and
/// that cannot be decided by a render-time peek the way the fade can. The group
/// reads the offset with `get()`, which re-enters this group (and its ancestors) while
/// the drawer moves; the app content subtree beside it is untouched. The fade itself is
/// a background closure peeking the progress, so nothing recomposes for the color.
///
/// The scrim carries the drag as well as the tap. winia's gesture dispatch captures ONE
/// node per down (the innermost with a gesture), so a drag that starts on the scrim would
/// otherwise be routed to the scrim's tap detector, cancel the tap and move nothing —
/// where Compose's `anchoredDraggable` on the outer box would have dragged the drawer.
fn scrim(
    ctx: &mut ComposeCtx,
    state: &DrawerState,
    color: Color,
    sheet_width: f32,
    draggable: bool,
) {
    let key = ctx.next_key();
    ctx.changed(&color);
    ctx.changed(&sheet_width);
    let state = state.clone();
    match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            let offset = state.current_offset(); // get(): this slot depends on the offset
            let closed = state.anchored_draggable().peek_position_of(&DrawerValue::Closed);
            let parked_closed = offset.is_nan()
                || closed.is_nan()
                || (offset - closed).abs() < 0.5;
            if !parked_closed {
                let fade = state.clone();
                let dismiss = state.clone();
                let mut modifier = Modifier::new().fill_max_size().background(
                    move || {
                        let a = (fade.peek_progress() * color.a as f32)
                            .round()
                            .clamp(0.0, 255.0) as u8;
                        Color::from_argb(a, color.r, color.g, color.b)
                    },
                    Shape::Rectangle,
                );
                if draggable {
                    let drag = state.clone();
                    let end = state.clone();
                    modifier = modifier
                        .on_drag(move |_pos, (dx, _dy)| drag.drag_delta(dx))
                        .on_drag_end(move || {
                            let v = end.last_velocity();
                            end.settle_with_velocity(v);
                        });
                }
                Stack::new()
                    .modifier(modifier.clickable(move || dismiss.close()))
                    .build(ctx, |_| {});
            }
        }
    }
    ctx.end_restartable_group();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;

    fn comp() -> Composer {
        Composer::new()
    }

    /// Holds the global animation lock and clears the registry on drop. AGENTS.md
    /// requires it of any test that pushes animations, and `settle`/`open`/`close` all
    /// do: the entries live in a process-wide table and would otherwise outlive the test
    /// and keep an `Arc` to its state alive.
    fn anim_guard() -> impl Drop {
        struct Guard(Option<std::sync::MutexGuard<'static, ()>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                crate::animation::clear_all_animations();
                self.0.take(); // release the lock after clearing
            }
        }
        let lock = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        Guard(Some(lock))
    }

    // ── anchors ──

    #[test]
    fn anchors_are_open_at_zero_and_closed_off_the_leading_edge() {
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Open), 0.0);
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Closed), -360.0);
        // RTL docks at the right, so the closed anchor flips sign.
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, true);
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Closed), 360.0);
    }

    #[test]
    fn first_layout_parks_the_closed_drawer_at_its_anchor() {
        let s = DrawerState::new(DrawerValue::Closed);
        assert!(s.current_offset().is_nan(), "uninitialized before the first anchors");
        s.update_anchors(360.0, false);
        assert_eq!(s.current_offset(), -360.0);
        assert!(s.is_closed());
        assert_eq!(s.progress(), 0.0, "parked closed reads progress 0");
    }

    #[test]
    fn open_close_move_the_target_and_progress() {
        let _anim = anim_guard();
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        s.open();
        assert_eq!(s.target_value(), DrawerValue::Open);
        assert_eq!(s.current_value(), DrawerValue::Open, "settle is eager; the tween follows");
        s.close();
        assert_eq!(s.target_value(), DrawerValue::Closed);
        assert!(s.is_closed());
    }

    #[test]
    fn progress_is_zero_closed_and_one_open() {
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        assert_eq!(s.progress(), 0.0);
        s.snap_to(DrawerValue::Open);
        assert_eq!(s.current_offset(), 0.0);
        assert_eq!(s.progress(), 1.0);
        s.drag_delta(-180.0);
        assert!((s.progress() - 0.5).abs() < 1e-3, "halfway out reads 0.5");
        assert!((s.peek_progress() - 0.5).abs() < 1e-3, "peek agrees with get");
    }

    #[test]
    fn progress_is_direction_agnostic() {
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, true);
        assert_eq!(s.progress(), 0.0, "RTL parked closed still reads 0");
        s.snap_to(DrawerValue::Open);
        assert_eq!(s.progress(), 1.0);
    }

    // ── gestures ──

    #[test]
    fn drag_toward_the_edge_closes_in_both_directions() {
        let _anim = anim_guard();
        // LTR: the drawer lives at the left, so dragging LEFT closes it. The release
        // has no velocity, so the target is the nearest anchor — past the midpoint.
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, false);
        s.drag_delta(-200.0);
        assert_eq!(s.current_offset(), -200.0);
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Closed);
        // RTL: mirrored — dragging RIGHT closes it, with no sign flip at the call site.
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, true);
        s.drag_delta(200.0);
        assert_eq!(s.current_offset(), 200.0);
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Closed);
    }

    #[test]
    fn drag_cannot_overshoot_either_anchor() {
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, false);
        s.drag_delta(-9000.0);
        assert_eq!(s.current_offset(), -360.0, "clamped at the closed anchor");
        s.drag_delta(9000.0);
        assert_eq!(s.current_offset(), 0.0, "clamped at the open anchor");
    }

    #[test]
    fn a_slow_drag_settles_by_position_and_a_fast_one_by_direction() {
        let _anim = anim_guard();
        let density = current_density().density;
        // Slow release short of the midpoint → back to the anchor it came from.
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, false);
        s.drag_delta(-100.0); // 100 of 360 — nearer Open
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Open);
        // A deliberate flick (400dp/s token) past… actually short of the midpoint still
        // closes: this is the whole point of the drawer's own velocity threshold.
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, false);
        s.drag_delta(-40.0);
        let flick = 500.0 * density;
        assert_eq!(s.settle_with_velocity(-flick), DrawerValue::Closed);
    }

    #[test]
    fn the_drawer_threshold_is_higher_than_the_anchored_draggable_default() {
        let _anim = anim_guard();
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        s.snap_to(DrawerValue::Open);
        let density = current_density().density;
        // 200dp/s clears the 125dp/s default but NOT the drawer's 400dp/s token, so a
        // lazy drag that stops just short of the midpoint springs back instead of
        // flinging the drawer shut.
        s.drag_delta(-170.0); // just short of the 180 midpoint
        let lazy = 200.0 * density;
        assert_eq!(s.settle_with_velocity(-lazy), DrawerValue::Open);
    }

    #[test]
    fn confirm_value_change_vetoes_a_gesture() {
        let _anim = anim_guard();
        // The same gesture twice, once with a veto. Without it the drawer closes —
        // which is what makes the vetoed run's outcome meaningful.
        let run = |veto: bool| {
            let mut s = DrawerState::new(DrawerValue::Open);
            s.update_anchors(360.0, false);
            if veto {
                s.set_confirm_value_change(|v| v != DrawerValue::Closed);
            }
            s.drag_delta(-300.0);
            let target = s.settle_with_velocity(0.0);
            (target, s.is_open(), s.is_animation_running())
        };
        let (target, open, _) = run(false);
        assert_eq!(target, DrawerValue::Closed);
        assert!(!open, "without a veto the flick closes the drawer");

        let (target, open, animating) = run(true);
        assert_eq!(target, DrawerValue::Closed, "the gesture still computes Closed");
        assert!(open, "but the veto leaves the drawer parked Open");
        assert!(animating, "and the offset is driven back to the Open anchor");
    }

    // ── layout / presence ──

    #[test]
    fn width_is_the_token_clipped_to_the_window() {
        assert_eq!(resolve_sheet_width(360.0, 240.0, 800.0), 360.0);
        assert_eq!(resolve_sheet_width(360.0, 240.0, 300.0), 300.0, "narrow window");
        assert_eq!(
            resolve_sheet_width(200.0, 240.0, 800.0),
            240.0,
            "minimum wins over a small maximum (sizeIn semantics)"
        );
        assert_eq!(resolve_sheet_width(360.0, 240.0, 100.0), 100.0, "min clamped too");
    }

    #[test]
    fn resizing_keeps_the_open_drawer_flush_with_the_edge() {
        let s = DrawerState::new(DrawerValue::Open);
        s.update_anchors(360.0, false);
        s.snap_to(DrawerValue::Open);
        s.update_anchors(500.0, false);
        assert_eq!(s.current_offset(), 0.0, "open stays at 0 after a width change");
        assert_eq!(s.anchored_draggable().position_of(&DrawerValue::Closed), -500.0);
        // And a closed drawer follows the new width to stay off-screen.
        let s = DrawerState::new(DrawerValue::Closed);
        s.update_anchors(360.0, false);
        s.update_anchors(500.0, false);
        assert_eq!(s.current_offset(), -500.0);
    }

    #[test]
    fn defaults_match_the_material_tokens() {
        assert_eq!(DrawerDefaults::MAXIMUM_DRAWER_WIDTH, Dp(360.0));
        assert_eq!(DrawerDefaults::MINIMUM_DRAWER_WIDTH, Dp(240.0));
        assert_eq!(DRAWER_ITEM_HEIGHT, 56.0, "ActiveIndicatorHeight");
        assert_eq!(DRAWER_ITEM_ICON_SIZE, 24.0, "IconSize");
        assert_eq!(DRAWER_CORNER_RADIUS, 16.0, "CornerLarge");
        // The sheet rounds the side facing the content and squares the window edge.
        assert_eq!(
            DrawerDefaults::shape(LayoutDirection::Ltr),
            Shape::RightRoundedRect { radius: DRAWER_CORNER_RADIUS },
            "LTR drawer docks left → rounded on the right"
        );
        assert_eq!(
            DrawerDefaults::shape(LayoutDirection::Rtl),
            Shape::LeftRoundedRect { radius: DRAWER_CORNER_RADIUS },
        );
    }

    // ── composition ──

    #[test]
    fn drawer_builds_content_sheet_and_a_live_anchor() {
        let mut c = comp();
        c.compose(|ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::ui::Text::new("page").build(ctx);
            })
            .drawer_content(|ctx| {
                ModalDrawerSheet::new(|ctx| {
                    NavigationDrawerItem::text_label("Inbox", true).build(ctx);
                })
                .build(ctx);
            })
            .build(ctx);
        });
        assert!(c.layout_root().is_some(), "the drawer composes a tree");
    }

    #[test]
    fn scrim_is_present_only_while_the_drawer_is_out() {
        // A closed drawer must compose NO scrim: a full-window click-catcher left in
        // the tree would eat every click on the page behind it.
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::ui::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
            .build(ctx);
        });
        let closed_nodes = count_nodes(&c);
        assert!(c.layout_root().is_some());
        // Open it and compose again: the scrim adds a node.
        state.snap_to(DrawerValue::Open);
        let st2 = state.clone();
        c.compose(move |ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::ui::Text::new("page").build(ctx);
            })
            .drawer_state(st2.clone())
            .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
            .build(ctx);
        });
        let open_nodes = count_nodes(&c);
        assert_eq!(
            open_nodes,
            closed_nodes + 1,
            "exactly one node — the scrim — appears when the drawer opens \
             (closed {closed_nodes}, open {open_nodes})"
        );
    }

    fn count_nodes(c: &Composer) -> usize {
        c.arena_nodes().len()
    }

    /// x of the drawer sheet — the only node as wide as the drawer token.
    fn sheet_x(c: &Composer) -> f32 {
        let node = c
            .arena_nodes()
            .iter()
            .find(|n| (n.measured_size.width - DRAWER_MAX_WIDTH).abs() < 0.5)
            .expect("the drawer sheet is laid out somewhere in the tree");
        node.position.x
    }

    // The placement is the whole design: a layout offset (so hit testing follows the
    // slide) driven by the anchors (so one convention covers LTR and RTL), applied to
    // a leading-edge-aligned sheet. It is also the part a unit test of the state alone
    // cannot see, so it is exercised through a real compose + layout.
    fn drive_drawer(c: &mut Composer, state: &DrawerState) {
        let st = state.clone();
        c.compose(move |ctx| {
            ModalNavigationDrawer::new(|ctx| {
                crate::ui::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
    }

    #[test]
    fn the_sheet_parks_off_the_edge_and_lands_flush_when_opened() {
        crate::ui::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        drive_drawer(&mut c, &state);
        assert_eq!(
            sheet_x(&c),
            -DRAWER_MAX_WIDTH,
            "closed: the sheet sits entirely off the leading edge, so it cannot be hit"
        );
        state.snap_to(DrawerValue::Open);
        drive_drawer(&mut c, &state);
        assert_eq!(sheet_x(&c), 0.0, "open: flush with the leading edge");
        state.snap_to(DrawerValue::Closed);
        drive_drawer(&mut c, &state);
        assert_eq!(sheet_x(&c), -DRAWER_MAX_WIDTH, "and closed again");
    }

    #[test]
    fn a_drag_survives_the_next_compose() {
        // Regression, and the one only an end-to-end frame can prove: `update_anchors`
        // runs on every compose, and it used to re-align the offset to the parked anchor
        // whenever no animation was running — and a live drag is not an animation — so
        // every drag frame was undone by the next compose and the drawer could not be
        // dragged at all. Every state-level test passed with that bug in place.
        crate::ui::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Open);
        let mut c = comp();
        drive_drawer(&mut c, &state);
        assert_eq!(sheet_x(&c), 0.0, "open");

        state.drag_delta(-200.0);
        assert_eq!(state.current_offset(), -200.0);
        drive_drawer(&mut c, &state); // the compose + layout that follows the move
        assert_eq!(
            state.current_offset(),
            -200.0,
            "the drag must survive the compose, not snap back to its anchor"
        );
        assert_eq!(
            sheet_x(&c),
            -200.0,
            "and the sheet is painted where the finger left it"
        );

        assert_eq!(state.settle_with_velocity(0.0), DrawerValue::Closed);
        assert!(state.is_closed(), "released past the midpoint: parked at Closed");
        drive_drawer(&mut c, &state);
        assert!(
            state.is_animation_running(),
            "and the offset is tweening to the closed anchor (it does not teleport there)"
        );
        assert_eq!(state.target_value(), DrawerValue::Closed);
    }

    #[test]
    fn the_sheet_width_is_logical_not_physical() {
        // A HiDPI window is a trap: the width token is compared against the window's
        // LOGICAL width, so converting it with the density (physical px) would make a
        // 360dp drawer 540 "logical" px and silently fill a 520-wide window. Measured
        // on the running demo at 1.5x before this was pinned.
        crate::ui::adaptive::set_window_size(520.0, 620.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        crate::unit::with_density(crate::unit::Density::from_density(1.5), || {
            c.compose(move |ctx| {
                ModalNavigationDrawer::new(|ctx| {
                    crate::ui::Text::new("page").build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
                .build(ctx);
            });
        });
        c.layout(crate::layout::Constraints::new(0.0, 520.0, 0.0, 620.0));
        // Every width in the tree, unfiltered — the content and the host are legitimately
        // 520 wide, so the check is "a sheet-sized node exists", which fails outright when
        // the bug sizes the sheet to the window's full width.
        let widths: Vec<f32> = c
            .arena_nodes()
            .iter()
            .map(|n| n.measured_size.width)
            .filter(|w| *w > 0.0)
            .collect();
        assert!(
            widths.iter().any(|w| (*w - DRAWER_MAX_WIDTH).abs() < 0.5),
            "the sheet must be {DRAWER_MAX_WIDTH} logical px wide at 1.5x density; \
             widths seen: {widths:?}"
        );
        // Position as well as width: the sheet is parked at exactly the closed anchor, not
        // at the window edge. That can be wrong on its own (a wrong anchor with a right
        // width), so it is worth the separate assertion.
        let parked: Vec<f32> = c
            .arena_nodes()
            .iter()
            .filter(|n| (n.measured_size.width - DRAWER_MAX_WIDTH).abs() < 0.5)
            .map(|n| n.position.x)
            .collect();
        assert!(
            !parked.is_empty()
                && parked
                    .iter()
                    .all(|x| (*x + DRAWER_MAX_WIDTH).abs() < 0.5),
            "every sheet-sized node is parked at -{DRAWER_MAX_WIDTH}; x seen: {parked:?}"
        );
        crate::ui::adaptive::reset_window_size_state();
    }

    #[test]
    fn the_rtl_drawer_parks_off_the_trailing_edge() {
        crate::ui::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            crate::ui::theme::WiniaTheme::with_theme_and_direction(
                WiniaTheme::colors(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    ModalNavigationDrawer::new(|ctx| {
                        crate::ui::Text::new("page").build(ctx);
                    })
                    .drawer_state(st.clone())
                    .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
                    .build(ctx);
                },
            );
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        assert_eq!(
            sheet_x(&c),
            800.0,
            "closed in RTL: docked right, parked past the trailing edge"
        );
        state.snap_to(DrawerValue::Open);
        let st = state.clone();
        c.compose(move |ctx| {
            crate::ui::theme::WiniaTheme::with_theme_and_direction(
                WiniaTheme::colors(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    ModalNavigationDrawer::new(|ctx| {
                        crate::ui::Text::new("page").build(ctx);
                    })
                    .drawer_state(st.clone())
                    .drawer_content(|ctx| { ModalDrawerSheet::new(|_| {}).build(ctx); })
                    .build(ctx);
                },
            );
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        assert_eq!(sheet_x(&c), 440.0, "open in RTL: flush with the right edge");
    }
}
