//! Modal navigation drawer — Compose Material3 `ModalNavigationDrawer` parity.
//!
//! Structure (mirrors androidx `NavigationDrawer.kt`):
//! - [`ModalNavigationDrawer`] owns the state, the anchors, the placement and the
//!   scrim, and puts the app content behind both. It is an **in-tree** component
//!   (a `Stack`), exactly like Compose's — not an overlay like [`crate::components::bottom_sheet::ModalBottomSheet`]:
//!   the drawer is part of the app shell, and the content under it keeps its
//!   composition state.
//! - [`ModalDrawerSheet`] is the surface the caller nests inside the drawer slot:
//!   width, corner shape, container color and content padding.
//! - [`NavigationDrawerItem`] is the label/icon/badge row with the pill indicator.
//!
//! Anchors: `Open at 0`, `Closed at ∓sheetWidth` — negative in LTR (the sheet leaves
//! through the left edge) and positive in RTL, where the drawer docks at the right.
//! androidx anchors `-width` in both directions and mirrors through
//! `reverseDirection = isRtl` on its gesture; `AnchoredDraggableState` has no reverse
//! flag, so the sign lives in the anchor and the host places the sheet against the edge
//! the direction puts it on. Either way the offset runs from `0` to "fully out", so the
//! drag callback needs no sign flip.
//!
//! Deviations from Compose (winia degradations), all deliberate:
//! - **Gestures.** Compose hangs a horizontal `anchoredDraggable` on the whole
//!   drawer box and hands it the moves whatever child is under the finger. winia's
//!   dispatch selects ONE gesture node per down, so the drag lives on the host stack
//!   and is reachable wherever no child claims the down. A `clickable` child does not
//!   claim it (a click is not a gesture element, and the 18px slop cancels the click
//!   when the finger moves), so a swipe that starts on a drawer row or a button drags
//!   the drawer — like Compose. What does claim it: a child with a real gesture
//!   (`on_drag`/`on_tap`/`on_long_press`: sliders, switches, text fields) and a scroll
//!   container, which `inner_component_drag` keeps in charge of its own area.
//! - **Escape does not close the drawer.** winia's Escape handling lives in the
//!   overlay path (`app.rs`), which only main-tree overlays reach; the drawer is
//!   in-tree with no key focus of its own. Close it from the scrim, a gesture, or
//!   `DrawerState::close()` — e.g. from a hamburger button.
//! - **No suspending API.** `open`/`close` push a tween onto the offset state
//!   (Compose suspends); the animation is the shared settled-anchor tween.
//! - **All three drawers are here**: [`ModalNavigationDrawer`] (scrim over the content),
//!   [`DismissibleNavigationDrawer`] (pushes the content, no scrim) and
//!   [`PermanentNavigationDrawer`] (no state at all; `Row { sheet, content }`).
//! - The scrim keeps winia's modal-scrim alpha (110/255, the same one
//!   `open_overlay` draws for modals) instead of re-deriving Compose's
//!   `ScrimTokens` value; override it per drawer with
//!   [`ModalNavigationDrawer::scrim_color`].

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::{Alignment, BoxLayout, LayoutDirection};
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::overlay::anchored_draggable::AnchoredDraggableState;
use crate::interaction::MutableInteractionSource;
use crate::layout::components::{Column, Row, Stack};
use crate::theme::WiniaTheme;
use crate::unit::Dp;
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
/// `NavigationDrawerTokens.IconSize` (24dp) — the token a CALLER sizes a drawer item's icon with.
/// The item itself does not apply it: upstream only provides the content colour, and `Icon` already
/// falls back to 24, so an item can carry a larger icon.
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

    /// Recompute the anchors for a sheet `width` px wide, and keep a settled drawer on
    /// its parked anchor.
    ///
    /// The ± sign: `Open at 0`, `Closed at -width` in LTR and `+width` in RTL. androidx
    /// anchors `-width` in BOTH directions and takes the mirror from
    /// `reverseDirection = isRtl` on the gesture plus RTL-aware `offset`/`placeRelative`;
    /// winia's `AnchoredDraggableState` has no reverse flag, so the direction is carried
    /// by the anchor instead, and the host places the sheet on the edge the direction
    /// puts it on.
    ///
    /// This runs on every compose, so the re-align has to be explicit about who owns the
    /// offset. A settled drawer must sit exactly on its parked anchor — that is what keeps
    /// a closed drawer off-screen and an open one flush — but a finger or a running tween
    /// owns the offset while it lasts. Two earlier versions got this wrong in opposite
    /// directions: guarding only on "no animation" undid one drag frame per compose (the
    /// drawer could not be dragged at all), and guarding only on "the anchor moved" cut a
    /// drag short when a resize landed mid-gesture and left a mid-tween resize parked at
    /// the tween's stale target. Owners are now asked directly.
    pub fn update_anchors(&self, width: f32, rtl: bool) {
        let closed = if rtl { width } else { -width };
        self.anchored
            .update_anchors(crate::overlay::anchored_draggable::DraggableAnchors::new([
                (DrawerValue::Open, 0.0),
                (DrawerValue::Closed, closed),
            ]));
        if self.anchored.is_dragging() || self.anchored.is_animation_running() {
            return;
        }
        let parked = self.anchored.settled_value();
        let pos = self.anchored.peek_position_of(&parked);
        let off = self.anchored.offset();
        if !pos.is_nan() && !off.is_nan() && (off - pos).abs() > 0.5 {
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
    pub fn modal_container_color(theme: &crate::theme::ThemeColors) -> Color {
        theme.surface_container_low
    }

    /// The scrim: black at winia's modal alpha.
    pub fn scrim_color() -> Color {
        Color::from_argb(DRAWER_SCRIM_ALPHA, 0, 0, 0)
    }

    /// Item colors (`NavigationDrawerItemDefaults.colors()`).
    pub fn item_colors(theme: &crate::theme::ThemeColors) -> NavigationDrawerItemColors {
        NavigationDrawerItemColors {
            selected_container: theme.secondary_container,
            selected_icon: theme.on_secondary_container,
            selected_text: theme.on_secondary_container,
            unselected_container: Color::TRANSPARENT,
            unselected_icon: theme.on_surface_variant,
            unselected_text: theme.on_surface_variant,
            selected_badge: theme.on_secondary_container,
            unselected_badge: theme.on_surface_variant,
        }
    }
}

/// Item defaults (Compose `NavigationDrawerItemDefaults`).
///
/// Upstream's `colors()` takes all eight colors as OPTIONAL parameters. Every field of
/// [`NavigationDrawerItemColors`] is public here, so an override is written as a struct field and
/// this object only has to name the defaults — which is also why it carries no eight-argument
/// builder of its own.
pub struct NavigationDrawerItemDefaults;

impl NavigationDrawerItemDefaults {
    /// `ItemPadding`: the horizontal padding a drawer puts around its items, which is what makes an
    /// item span `ActiveIndicatorWidth` (336dp) inside the 360dp container.
    pub const ITEM_PADDING: f32 = DRAWER_SHEET_HORIZONTAL_PADDING;

    /// `NavigationDrawerItemDefaults.colors()` — `ActiveIndicatorColor` / `ActiveIconColor` /
    /// `ActiveLabelTextColor` and their inactive counterparts, with both badge colors following the
    /// LABEL colors as upstream's own defaults do.
    pub fn colors(theme: &crate::theme::ThemeColors) -> NavigationDrawerItemColors {
        DrawerDefaults::item_colors(theme)
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
    /// Badge color while selected — `badgeColor(selected)`, which upstream defaults to
    /// `selectedTextColor` (`NavigationDrawer.kt:1218-1219`).
    pub selected_badge: Color,
    /// Badge color while unselected (`unselectedTextColor` by default).
    pub unselected_badge: Color,
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
    /// The badge's color. It defaults to the LABEL's, not the icon's: they agree at the token
    /// defaults (`OnSecondaryContainer` / `OnSurfaceVariant` for both), so this only shows when a
    /// caller overrides one of the two.
    pub fn badge(&self, selected: bool) -> Color {
        if selected { self.selected_badge } else { self.unselected_badge }
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
        Self::new(move |ctx| { crate::components::Text::new(label).build(ctx); }, selected)
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
        // The indicator colour fades between the unselected and selected fills. M3's
        // `NavigationDrawer.kt` contains no `animateColorAsState`, so the fade is a winia addition
        // rather than a port — kept, and recorded here instead of presented as alignment. It is a
        // *paint* value: the animation state is peeked by the background closure, so it costs no
        // recomposition.
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
        // Accessibility: upstream declares `role = Role.Tab` on the row (`NavigationDrawer.kt:1137`)
        // and carries the selection the way its `Surface(selected = …)` does. winia's accessibility
        // layer maps `SemanticsRole::Tab` to a UIA tab item, so without this a drawer reads as a
        // pile of unnamed buttons.
        modifier = modifier.semantics(
            crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::Tab)
                .merge_descendants(true)
                .state(crate::semantics::SemanticsState::new().selected(selected)),
        );

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
                            // The icon is composed at ITS OWN size, as upstream does — the slot only
                            // provides the content color, and `Icon` already falls back to 24 when the
                            // caller set no size. Wrapping it in a fixed 24 box squashed anything
                            // larger.
                            WiniaTheme::with_content_color(icon_color, ctx, |ctx| {
                                icon(ctx);
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
                                WiniaTheme::with_content_color(colors.badge(selected), ctx, |ctx| {
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
/// Which drawer the sheet belongs to. The three differ ONLY in the container color their
/// defaults resolve to (`ModalContainerColor` = `SurfaceContainerLow` against
/// `StandardContainerColor` = `Surface`, `NavigationDrawerTokens.kt`); the shape, the elevation
/// default (`Level0`) and the padding are the same for all three, which is why this is one type
/// with three constructors rather than three near-identical structs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawerSheetKind {
    /// `ModalDrawerSheet`. Container `SurfaceContainerLow`; the modal drawer paints a scrim over
    /// the content behind it.
    Modal,
    /// `DismissibleDrawerSheet` / `PermanentDrawerSheet`. Container `Surface`; nothing obscures the
    /// content, which is pushed aside instead.
    Standard,
}

impl DrawerSheetKind {
    /// The container color this kind resolves to when the caller gave none.
    pub(crate) fn container_color(self, theme: &crate::theme::ThemeColors) -> Color {
        match self {
            DrawerSheetKind::Modal => DrawerDefaults::modal_container_color(theme),
            // `StandardContainerColor` = `Surface` (`NavigationDrawerTokens.kt`).
            DrawerSheetKind::Standard => theme.surface,
        }
    }

    /// The shape this kind resolves to when the caller gave none. Only the MODAL sheet rounds its
    /// end corners (`DrawerDefaults.shape`); upstream's dismissible and permanent sheets both
    /// default to `RectangleShape` (`NavigationDrawer.kt:736`, `:821`).
    pub(crate) fn shape(self, direction: LayoutDirection) -> Shape {
        match self {
            DrawerSheetKind::Modal => DrawerDefaults::shape(direction),
            DrawerSheetKind::Standard => Shape::Rectangle,
        }
    }
}

pub struct DrawerSheet {
    kind: DrawerSheetKind,
    modifier: Modifier,
    shape: Option<Shape>,
    container_color: Option<Color>,
    content_color: Option<Color>,
    elevation: f32,
    content_padding: bool,
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
}

impl DrawerSheet {
    /// The MODAL sheet (`ModalDrawerSheet`).
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self::of_kind(DrawerSheetKind::Modal, content)
    }

    /// `DismissibleDrawerSheet`.
    pub fn dismissible(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self::of_kind(DrawerSheetKind::Standard, content)
    }

    /// `PermanentDrawerSheet`.
    pub fn permanent(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self::of_kind(DrawerSheetKind::Standard, content)
    }

    fn of_kind(
        kind: DrawerSheetKind,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
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
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        ctx.changed(&self.shape);
        ctx.changed(&self.container_color);
        ctx.changed(&self.content_color);
        ctx.changed(&self.elevation);
        ctx.changed(&self.kind);
        ctx.changed(&self.content_padding);
        let shape = self.shape.unwrap_or_else(|| self.kind.shape(direction));
        let container = self.container_color.unwrap_or_else(|| self.kind.container_color(&theme));
        let content_color = self.content_color.unwrap_or(theme.on_surface);
        let elevation = self.elevation;
        let padding = self.content_padding;
        let content = self.content;
        // Shadow sits OUTSIDE the clip (it must be able to paint past the rounded
        // corner), then the surface fills and clips its children.
        // `DrawerSheet` applies `sizeIn(minWidth = MinimumDrawerWidth, maxWidth =
        // NavigationDrawerTokens.ContainerWidth)` (androidx `NavigationDrawer.kt:854`, `:866`) —
        // BOTH bounds, on the sheet itself: a narrow-content sheet is still 240dp wide, and one
        // whose content wants more stops at 360dp. The drawer that hosts it may narrow it further
        // (the modal and dismissible ones give it the token clipped to the window).
        let mut modifier = Modifier::new()
            .fill_max_height()
            .min_width(DRAWER_MIN_WIDTH)
            .max_width(DRAWER_MAX_WIDTH)
            // `paneTitle = navigationMenu` upstream (`NavigationDrawer.kt:832`). winia's semantics
            // has no paneTitle property of its own, so the pane is named with a content description
            // — the closest thing the accessibility layer publishes. Declared BEFORE the caller's
            // modifier so a caller's own description merges over it.
            .semantics(
                crate::semantics::SemanticsConfig::new().content_description("Navigation menu"),
            )
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

/// Compose spells the modal sheet as its own type; it is the same sheet here, built with
/// [`DrawerSheet::new`].
pub type ModalDrawerSheet = DrawerSheet;
/// Built with [`DrawerSheet::dismissible`] — `DismissibleDrawerSheet::new` would give the MODAL
/// container color, which is why the constructor is named after the variant.
pub type DismissibleDrawerSheet = DrawerSheet;
/// Built with [`DrawerSheet::permanent`]. See [`DismissibleDrawerSheet`] for why `new` is not the
/// constructor to reach for.
pub type PermanentDrawerSheet = DrawerSheet;

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
        let direction = self.modifier.get_layout_direction().unwrap_or(crate::layout::direction::current());
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
        let window_w = crate::layout::window_size().0;
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
            // The sheet is placed against the LEADING edge, so one offset convention (open at 0,
            // closed at ∓width) covers both directions. That used to be spelled out by flipping the
            // alignment by hand; the box mirrors `Start` under RTL now — as Compose's
            // `Alignment.TopStart` does — so naming the leading edge is enough.
            .alignment(Alignment::Start)
            .build(ctx, |ctx| {
                content(ctx);
                scrim(ctx, &state, scrim_color, sheet_w);
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

// ═══════════════ DismissibleNavigationDrawer ═══════════════

/// Dismissible navigation drawer (Compose `DismissibleNavigationDrawer`).
///
/// Same state, anchors and gestures as [`ModalNavigationDrawer`]; what differs is where the content
/// goes. There is NO scrim, and the content is PUSHED rather than covered: upstream measures both
/// children and places the content at `sheetWidth + drawerState.requireOffset()`, mirrored under RTL
/// by `placeRelative` (`NavigationDrawer.kt:570-590`), so a closed drawer costs the content nothing
/// and an open one costs it exactly the sheet's width.
///
/// Pair it with [`DrawerSheet::dismissible`] — the standard container is `Surface` and the standard
/// shape is a RECTANGLE (`NavigationDrawer.kt:736`), unlike the modal sheet's rounded end corners.
pub struct DismissibleNavigationDrawer {
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    drawer_content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    drawer_state: Option<DrawerState>,
    gestures_enabled: bool,
    maximum_drawer_width: Dp,
    minimum_drawer_width: Dp,
    modifier: Modifier,
}

impl DismissibleNavigationDrawer {
    /// `content` is the app shell the drawer pushes aside.
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            content: Box::new(content),
            drawer_content: None,
            drawer_state: None,
            gestures_enabled: true,
            maximum_drawer_width: DrawerDefaults::MAXIMUM_DRAWER_WIDTH,
            minimum_drawer_width: DrawerDefaults::MINIMUM_DRAWER_WIDTH,
            modifier: Modifier::new(),
        }
    }

    /// The drawer's own content — typically [`DrawerSheet::dismissible`].
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

    /// Whether a horizontal drag moves the drawer (default true).
    pub fn gestures_enabled(mut self, enabled: bool) -> Self {
        self.gestures_enabled = enabled;
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
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        let rtl = direction == LayoutDirection::Rtl;
        ctx.changed(&direction);
        ctx.changed(&self.gestures_enabled);
        ctx.changed(&self.maximum_drawer_width);
        ctx.changed(&self.minimum_drawer_width);

        let holder = ctx.remember(|| DrawerState::new(DrawerValue::Closed));
        let state = match self.drawer_state {
            Some(s) => s,
            None => holder.get(),
        };
        let window_w = crate::layout::window_size().0;
        let sheet_w = resolve_sheet_width(
            self.maximum_drawer_width.0,
            self.minimum_drawer_width.0,
            window_w,
        );
        state.update_anchors(sheet_w, rtl);

        let content = self.content;
        let drawer_content = self.drawer_content;
        let mut host = Modifier::new().fill_max_size();
        if self.gestures_enabled {
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

        let key = ctx.next_key();
        let policy = DismissibleDrawerPolicy {
            offset: state.offset_state(),
            sheet_width: sheet_w,
            rtl,
        };
        match ctx.start_restartable_group(key, host, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let sheet_modifier = Modifier::new().width(sheet_w).fill_max_height();
                match drawer_content {
                    Some(drawer_content) => {
                        Stack::new().modifier(sheet_modifier).build(ctx, drawer_content)
                    }
                    // The policy reads `children[0]` as the sheet, so the slot stays even when the
                    // caller gave none — it is simply never seen.
                    None => Stack::new().modifier(sheet_modifier).build(ctx, |_| {}),
                }
                Stack::new()
                    .modifier(Modifier::new().fill_max_size())
                    .build(ctx, content);
            }
        }
        ctx.end_restartable_group();
    }
}

/// Places a dismissible drawer's two children: the sheet at the drag offset, the content PUSHED aside
/// by the width the sheet takes out of it.
///
/// The two directions have DIFFERENT offset ranges, because the closed anchor is `-width` under LTR
/// and `+width` under RTL (`DrawerState::update_anchors`). So the pushed distance is
///
///     LTR:  sheet_width + offset   (offset -360 closed .. 0 open  ->  0 .. +360)
///     RTL:  offset - sheet_width   (offset    0 open .. +360 closed -> -360 .. 0)
///
/// The first version negated the LTR expression under RTL, which put the page at -720 when the drawer
/// was CLOSED: `-(360 + 360)`. It looked right in LTR only because `360 + (-360)` happens to be 0.
#[derive(Debug)]
struct DismissibleDrawerPolicy {
    offset: State<f32>,
    sheet_width: f32,
    rtl: bool,
}

impl crate::layout::node::MeasurePolicy for DismissibleDrawerPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::node::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::node::Placement>) {
        let sheet = crate::layout::node::measure_node(nodes, policies, children[0], constraints);
        let content = crate::layout::node::measure_node(nodes, policies, children[1], constraints);
        // `get()` during measure: the placement depends on the drag offset, so this slot has to be
        // re-measured while the drawer moves. The modal drawer reads the same state through a dynamic
        // offset for the same reason.
        let offset = self.offset.get();
        // The closed anchor carries the direction's sign (`-width` LTR, `+width` RTL), so the pushed
        // distance is a different expression per direction — see this struct's doc.
        let content_x = if self.rtl {
            offset - self.sheet_width
        } else {
            self.sheet_width + offset
        };
        (
            content.0,
            vec![
                crate::layout::node::Placement {
                    position: crate::unit::Offset::new(offset, 0.0),
                    size: sheet.0,
                },
                crate::layout::node::Placement {
                    position: crate::unit::Offset::new(content_x, 0.0),
                    size: content.0,
                },
            ],
        )
    }

    fn place(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        children: &[usize],
        placements: &[crate::layout::node::Placement],
    ) {
        for (i, &child) in children.iter().enumerate() {
            nodes[child].position = placements[i].position;
            nodes[child].measured_size = placements[i].size;
        }
    }
}

// ═══════════════ PermanentNavigationDrawer ═══════════════

/// Permanent navigation drawer (Compose `PermanentNavigationDrawer`).
///
/// No state and no gestures: the sheet is ALWAYS on screen and the content takes what is left, so
/// upstream composes it as a plain `Row(modifier.fillMaxSize()) { drawerContent(); Box { content() } }`
/// (`NavigationDrawer.kt:618-623`). Reach for it on a wide layout that keeps destinations visible.
///
/// Pair it with [`DrawerSheet::permanent`] (standard container, square).
pub struct PermanentNavigationDrawer {
    content: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    drawer_content: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
}

impl PermanentNavigationDrawer {
    pub fn new(content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        Self {
            content: Box::new(content),
            drawer_content: None,
            modifier: Modifier::new(),
        }
    }

    /// The drawer's own content — typically [`DrawerSheet::permanent`].
    pub fn drawer_content(
        mut self,
        content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.drawer_content = Some(Box::new(content));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        let content = self.content;
        let drawer_content = self.drawer_content;
        let modifier = self.modifier.then(Modifier::new().fill_max_size());
        Row::new().modifier(modifier).build(ctx, |ctx| {
            if let Some(drawer_content) = drawer_content {
                drawer_content(ctx);
            }
            Stack::new().build(ctx, content);
        });
    }
}

/// The scrim, in a group of its own.
///
/// Its *presence* is structural — a full-window click-catcher must actually vanish
/// once the drawer parks closed, or it would swallow every click on the page — and
/// that cannot be decided by a render-time peek the way the fade can. The group
/// reads the offset with `get()`, which re-enters this group (and its ancestors on the
/// dirty path) while the drawer moves; the app content subtree beside it is untouched.
/// The fade is a background closure peeking the progress, so nothing recomposes for the
/// color.
///
/// No drag of its own: `clickable` is not a gesture element, so a down here already
/// falls through to the host's `on_drag` — measured both ways, with and without one
/// wired here. The tap is what this node owns.
fn scrim(
    ctx: &mut ComposeCtx,
    state: &DrawerState,
    color: Color,
    sheet_width: f32,
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
                Stack::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_size()
                            .background(
                                move || {
                                    let a = (fade.peek_progress() * color.a as f32)
                                        .round()
                                        .clamp(0.0, 255.0) as u8;
                                    Color::from_argb(a, color.r, color.g, color.b)
                                },
                                Shape::Rectangle,
                            )
                            .clickable(move || dismiss.close()),
                    )
                    .build(ctx, |_| {});
            }
        }
    }
    ctx.end_restartable_group();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;

    fn comp() -> Composer {
        Composer::new()
    }

    /// Holds the global animation lock for the test's lifetime and, on drop, clears the
    /// animations of every state the test registered with [`AnimGuard::track`]. AGENTS.md
    /// requires the isolation of any test that pushes animations (`settle`/`open`/`close` all
    /// do), because the table is process-wide and an entry outliving the test keeps an `Arc`
    /// to its state alive.
    ///
    /// The cleanup is targeted rather than `clear_all_animations()`: that call is global, and
    /// not every animation test takes the lock (the existing `sheet_state` and
    /// `anchored_draggable` tests push without it), so a blanket clear would pull an
    /// animation out from under a test running on another thread.
    struct AnimGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        states: Vec<crate::runtime::state::StateId>,
    }

    impl AnimGuard {
        /// Clean up this state's animations when the guard drops.
        fn track(&mut self, state: &DrawerState) {
            self.states.push(state.offset_state().state_id());
        }
    }

    impl Drop for AnimGuard {
        fn drop(&mut self) {
            crate::animation::clear_animations_for_states(&self.states);
        }
    }

    fn anim_guard() -> AnimGuard {
        AnimGuard {
            _lock: crate::animation::tests::TEST_SERIAL
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
            states: Vec::new(),
        }
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
        let mut anim = anim_guard();
        let s = DrawerState::new(DrawerValue::Closed);
        anim.track(&s);
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
        let mut anim = anim_guard();
        // LTR: the drawer lives at the left, so dragging LEFT closes it. The release
        // has no velocity, so the target is the nearest anchor — past the midpoint.
        let s = DrawerState::new(DrawerValue::Open);
        anim.track(&s);
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
        let mut anim = anim_guard();
        // Slow release short of the midpoint → back to the anchor it came from.
        let s = DrawerState::new(DrawerValue::Open);
        anim.track(&s);
        s.update_anchors(360.0, false);
        s.drag_delta(-100.0); // 100 of 360 — nearer Open
        assert_eq!(s.settle_with_velocity(0.0), DrawerValue::Open);
        // A deliberate flick short of the midpoint still closes: that is the whole point of a
        // velocity rule. Read at 1.5x density with an UNSCALED logical velocity, so a
        // threshold that converted the dp token to physical px/s (600 here instead of 400)
        // would spring back and fail.
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.5), || {
            let s = DrawerState::new(DrawerValue::Open);
            anim.track(&s);
            s.update_anchors(360.0, false);
            s.drag_delta(-40.0);
            assert_eq!(s.settle_with_velocity(-500.0), DrawerValue::Closed);
        });
    }

    #[test]
    fn the_drawer_threshold_is_higher_than_the_anchored_draggable_default() {
        let mut anim = anim_guard();
        // 200 logical px/s clears the 125dp/s `AnchoredDraggable` default but NOT the drawer's
        // own 400dp/s token, so a lazy drag that stops just short of the midpoint springs back
        // instead of flinging the drawer shut. Read at 1.5x density with an unscaled velocity:
        // the earlier version scaled the velocity by the density exactly as the buggy
        // threshold did, so the two cancelled out and the test could not see the bug.
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.5), || {
            let s = DrawerState::new(DrawerValue::Closed);
            anim.track(&s);
            s.update_anchors(360.0, false);
            s.snap_to(DrawerValue::Open);
            s.drag_delta(-170.0); // just short of the 180 midpoint
            assert_eq!(s.settle_with_velocity(-200.0), DrawerValue::Open);
        });
    }

    #[test]
    fn confirm_value_change_vetoes_a_gesture() {
        let mut anim = anim_guard();
        // The same gesture twice, once with a veto. Without it the drawer closes —
        // which is what makes the vetoed run's outcome meaningful.
        let mut outcomes = Vec::new();
        for veto in [false, true] {
            let mut s = DrawerState::new(DrawerValue::Open);
            anim.track(&s);
            s.update_anchors(360.0, false);
            if veto {
                s.set_confirm_value_change(|v| v != DrawerValue::Closed);
            }
            s.drag_delta(-300.0);
            let target = s.settle_with_velocity(0.0);
            outcomes.push((target, s.is_open(), s.is_animation_running()));
        }
        let (target, open, _) = outcomes[0];
        assert_eq!(target, DrawerValue::Closed);
        assert!(!open, "without a veto the flick closes the drawer");

        let (target, open, animating) = outcomes[1];
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
    fn resizing_keeps_the_open_drawer_flush_and_the_closed_one_off_screen() {
        // The window has to be NARROWER than the 360dp token for the sheet — and so the
        // closed anchor — to move at all: on a wider window the token is the cap.
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        drive_drawer_sized(&mut c, &state, 300.0, 600.0);
        assert_eq!(state.current_offset(), -300.0);
        assert_eq!(sheet_x(&c), -300.0, "closed on a 300-wide window");

        // Widen it: the closed drawer follows its anchor so it stays off-screen.
        drive_drawer_sized(&mut c, &state, 520.0, 600.0);
        assert_eq!(
            state.anchored_draggable().position_of(&DrawerValue::Closed),
            -360.0,
            "the anchor follows the new width (the token, now that it fits)"
        );
        assert_eq!(state.current_offset(), -360.0, "and the offset is re-aligned to it");
        assert_eq!(sheet_x(&c), -360.0, "so nothing of the closed drawer is on screen");

        // Open on the narrow window, then widen: it must stay flush with the edge.
        state.snap_to(DrawerValue::Open);
        drive_drawer_sized(&mut c, &state, 300.0, 600.0);
        assert_eq!(sheet_x(&c), 0.0, "open on the narrow window");
        drive_drawer_sized(&mut c, &state, 520.0, 600.0);
        assert_eq!(sheet_x(&c), 0.0, "still flush after the window widened");
    }

    #[test]
    fn a_resize_mid_drag_does_not_cancel_the_gesture() {
        // The offset belongs to the finger while a drag lasts, so a geometry change must
        // not re-align it — that is the P0's failure mode with a narrower trigger.
        crate::layout::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Open);
        let mut c = comp();
        drive_drawer(&mut c, &state);
        state.drag_delta(-120.0);
        assert!(state.anchored_draggable().is_dragging());
        drive_drawer_sized(&mut c, &state, 300.0, 600.0); // narrow enough to move the anchors
        assert_eq!(
            state.current_offset(),
            -120.0,
            "the drag keeps its offset; the resize must not teleport it to the anchor"
        );
        assert_eq!(sheet_x(&c), -120.0, "and the sheet stays under the finger");
    }

    #[test]
    fn a_resize_mid_settle_is_repaired_once_the_tween_finishes() {
        let mut anim = anim_guard();
        // The other half of the same ownership question: while a tween runs it owns the
        // offset, and the re-align must not fight it — but once it has finished (here at a
        // target captured before the resize) the settled drawer is put back on its anchor.
        // Start on a window narrower than the token, so the closed anchor really moves when
        // the window widens (300 -> 800 puts it at -300 -> -360).
        let state = DrawerState::new(DrawerValue::Open);
        anim.track(&state);
        let mut c = comp();
        drive_drawer_sized(&mut c, &state, 300.0, 600.0);
        assert_eq!(sheet_x(&c), 0.0, "open");
        assert_eq!(
            state.anchored_draggable().position_of(&DrawerValue::Closed),
            -300.0
        );

        state.close(); // tween 0 -> -300
        assert!(state.is_animation_running());
        let before = state.current_offset();
        drive_drawer_sized(&mut c, &state, 800.0, 600.0); // widens while the tween runs
        assert!(
            (state.current_offset() - before).abs() < 1.0,
            "a running tween owns the offset; composing must not snap it (was {before}, now {})",
            state.current_offset()
        );

        // Let the tween finish at its stale target, then compose at the same window: the
        // drawer is settled Closed but sits where the old geometry put it (-300 against an
        // anchor of -360), so the invariant repair puts it back on the anchor.
        std::thread::sleep(std::time::Duration::from_millis(400));
        while crate::animation::update_animations() {}
        drive_drawer_sized(&mut c, &state, 800.0, 600.0);
        assert_eq!(
            state.current_offset(),
            -360.0,
            "after the tween ends the closed drawer is re-aligned to the current anchor"
        );
        assert_eq!(sheet_x(&c), -360.0, "so it is fully off-screen again");
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
                crate::components::Text::new("page").build(ctx);
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
                crate::components::Text::new("page").build(ctx);
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
                crate::components::Text::new("page").build(ctx);
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

    /// Tag the frame helpers put on the drawer sheet, so tests find it by identity.
    const SHEET_TAG: &str = "drawer-sheet";

    /// Drawer content the frame helpers compose: a tagged sheet whose content fills it, so
    /// the surface is exactly as wide as the element the drawer positions (an empty sheet
    /// would be only its own padding wide, or the 240dp minimum).
    fn tagged_sheet() -> impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static {
        |ctx| {
            ModalDrawerSheet::new(|ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |_| {});
            })
            .modifier(Modifier::new().test_tag(SHEET_TAG))
            .build(ctx);
        }
    }

    /// The drawer sheet's place on screen and its width: `(x, w)` of the tagged node, with
    /// its ancestors' offsets summed. Identified by tag rather than by width — a width match
    /// also hits the app content, which is just as wide, and stops matching at all once the
    /// window is narrower than the drawer token. A node's own `position` is parent-relative,
    /// so the walk accumulates from the root.
    fn sheet_x_and_width(c: &Composer) -> (f32, f32) {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
        ) -> Option<(f32, f32)> {
            let node = &nodes[idx];
            let abs_x = x + node.position.x;
            if node.modifier.get_test_tag() == Some(SHEET_TAG) {
                return Some((abs_x, node.measured_size.width));
            }
            node.children
                .iter()
                .find_map(|&child| walk(nodes, child, abs_x))
        }
        let root = c.layout_root_idx().expect("the drawer was laid out");
        walk(c.arena_nodes(), root, 0.0)
            .expect("the drawer sheet is laid out somewhere in the tree")
    }

    fn sheet_x(c: &Composer) -> f32 {
        sheet_x_and_width(c).0
    }

    // The placement is the whole design: a layout offset (so hit testing follows the
    // slide) driven by the anchors (so one convention covers LTR and RTL), applied to
    // a leading-edge-aligned sheet. It is also the part a unit test of the state alone
    // cannot see, so it is exercised through a real compose + layout.
    fn drive_drawer(c: &mut Composer, state: &DrawerState) {
        drive_drawer_sized(c, state, 800.0, 600.0)
    }

    /// Compose + lay out one frame of the drawer with the window it reads set to `w`x`h`.
    /// The size has to be set *inside* the compose: a running Composer reads its own
    /// adaptive context, which was seeded from the fallback when the Composer was created,
    /// so writing the fallback afterwards would not reach it.
    fn drive_drawer_sized(c: &mut Composer, state: &DrawerState, w: f32, h: f32) {
        let st = state.clone();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(w, h);
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(tagged_sheet())
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, w, 0.0, h));
    }

    #[test]
    fn the_sheet_parks_off_the_edge_and_lands_flush_when_opened() {
        crate::layout::adaptive::set_window_size(800.0, 600.0);
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
        crate::layout::adaptive::set_window_size(800.0, 600.0);
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
        crate::layout::adaptive::set_window_size(520.0, 620.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.5), || {
            c.compose(move |ctx| {
                ModalNavigationDrawer::new(|ctx| {
                    crate::components::Text::new("page").build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(tagged_sheet())
                .build(ctx);
            });
            // The layout runs under the same density, as it does in the app.
            c.layout(crate::layout::Constraints::new(0.0, 520.0, 0.0, 620.0));
        });
        // Identity-based, not a width filter: a width match also hits the app content
        // (just as wide) and stops matching once the window is narrower than the token.
        let (x, w) = sheet_x_and_width(&c);
        assert_eq!(
            w, DRAWER_MAX_WIDTH,
            "the sheet is the token in LOGICAL px at 1.5x density, not the 540 the physical \
             value would give (which the window would clamp to its own 520)"
        );
        assert_eq!(
            x, -DRAWER_MAX_WIDTH,
            "and it is parked off the leading edge, at the closed anchor"
        );
        // `reset_window_size_state` clears the State but not the fallback cell that
        // `set_window_size` wrote outside a composer, so restore the size as well —
        // otherwise this thread reports a 520-wide window to whatever test runs next on it.
        crate::layout::adaptive::reset_window_size_state();
        crate::layout::adaptive::set_window_size(800.0, 600.0);
    }

    #[test]
    fn a_narrow_sheet_keeps_the_minimum_width() {
        // androidx's `DrawerSheet` carries `sizeIn(minWidth = MinimumDrawerWidth)`, so a
        // drawer whose content does not fill it is 240dp wide rather than collapsing to its
        // own padding — which is what an empty sheet measured (24px: the 12dp sides) before
        // the minimum was applied.
        let state = DrawerState::new(DrawerValue::Open);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            crate::layout::adaptive::set_window_size(800.0, 600.0);
            ModalNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page").build(ctx);
            })
            .drawer_state(st.clone())
            .drawer_content(|ctx| {
                ModalDrawerSheet::new(|ctx| {
                    crate::components::Text::new("hi").build(ctx);
                })
                .modifier(Modifier::new().test_tag(SHEET_TAG))
                .build(ctx);
            })
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        let (x, w) = sheet_x_and_width(&c);
        assert_eq!(x, 0.0, "open");
        assert_eq!(
            w, DRAWER_MIN_WIDTH,
            "a sheet narrower than the minimum is {DRAWER_MIN_WIDTH}, not its own padding"
        );
    }

    #[test]
    fn the_rtl_drawer_parks_off_the_trailing_edge() {
        crate::layout::adaptive::set_window_size(800.0, 600.0);
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let st = state.clone();
        c.compose(move |ctx| {
            crate::theme::WiniaTheme::with_theme_and_direction(
                WiniaTheme::colors(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    ModalNavigationDrawer::new(|ctx| {
                        crate::components::Text::new("page").build(ctx);
                    })
                    .drawer_state(st.clone())
                    .drawer_content(tagged_sheet())
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
            crate::theme::WiniaTheme::with_theme_and_direction(
                WiniaTheme::colors(),
                LayoutDirection::Rtl,
                ctx,
                |ctx| {
                    ModalNavigationDrawer::new(|ctx| {
                        crate::components::Text::new("page").build(ctx);
                    })
                    .drawer_state(st.clone())
                    .drawer_content(tagged_sheet())
                    .build(ctx);
                },
            );
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        assert_eq!(sheet_x(&c), 440.0, "open in RTL: flush with the right edge");
    }

    /// Walks to a tagged node and answers its ABSOLUTE x (accumulating the parent chain, which is
    /// what `sheet_x_and_width` does for the sheet alone).
    fn x_of(c: &Composer, tag: &str) -> Option<f32> {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            x: f32,
            tag: &str,
        ) -> Option<f32> {
            let node = &nodes[idx];
            let abs_x = x + node.position.x;
            if node.modifier.get_test_tag() == Some(tag) {
                return Some(abs_x);
            }
            node.children
                .iter()
                .find_map(|&child| walk(nodes, child, abs_x, tag))
        }
        let root = c.layout_root_idx()?;
        walk(c.arena_nodes(), root, 0.0, tag)
    }

    const PAGE_TAG: &str = "drawer-page";

    /// The three sheets differ ONLY in the defaults their kind resolves: `ModalContainerColor`
    /// (`SurfaceContainerLow`) with the rounded `ContainerShape`, against `StandardContainerColor`
    /// (`Surface`) with a RECTANGLE for both standard sheets (`NavigationDrawer.kt:646-650`,
    /// `:736-737`, `:821-822`).
    #[test]
    fn the_sheet_kind_resolves_the_upstream_defaults() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        assert_eq!(
            DrawerSheetKind::Modal.container_color(&theme),
            theme.surface_container_low,
            "ModalContainerColor"
        );
        assert_eq!(
            DrawerSheetKind::Standard.container_color(&theme),
            theme.surface,
            "StandardContainerColor"
        );
        assert_eq!(
            DrawerSheetKind::Modal.shape(LayoutDirection::Ltr),
            Shape::RightRoundedRect { radius: DRAWER_CORNER_RADIUS },
            "ContainerShape = CornerLargeEnd"
        );
        assert_eq!(
            DrawerSheetKind::Standard.shape(LayoutDirection::Ltr),
            Shape::Rectangle,
            "drawerShape = RectangleShape for BOTH standard sheets"
        );
        assert_eq!(
            NavigationDrawerItemDefaults::ITEM_PADDING,
            DRAWER_SHEET_HORIZONTAL_PADDING,
            "ItemPadding = 12dp"
        );
    }

    /// The badge's DEFAULT is the label colour — upstream's `colors()` has `selectedBadgeColor` /
    /// `unselectedBadgeColor` defaulting to the text colours (`NavigationDrawer.kt:1218-1219`), while
    /// winia used to hard-wire the ICON colour. The default is a struct question; that the item PAINTS
    /// it is the test below.
    #[test]
    fn the_default_badge_color_is_the_label_color() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let colors = NavigationDrawerItemDefaults::colors(&theme);
        assert_eq!(colors.badge(true), colors.text(true), "selected: the label color");
        assert_eq!(colors.badge(false), colors.text(false), "unselected: the label color");
    }

    /// The badge is painted in its OWN colour, not the icon's.
    ///
    /// This reads the colour THE RENDERER WILL PAINT (`ModifierElement::TextContent.color`), not the
    /// colour struct: comparing structs passed even while `build` painted the badge with
    /// `colors.icon(selected)` — which is the exact bug this replaced, since the two defaults agree.
    /// So the assertion overrides ONE of the two colours and looks at the pixels' source.
    #[test]
    fn the_badge_is_painted_in_the_badge_color() {
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let badge_color = Color::from_argb(255, 1, 2, 3);
        let mut colors = NavigationDrawerItemDefaults::colors(&theme);
        colors.selected_badge = badge_color;
        let mut c = comp();
        c.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme, ctx, |ctx| {
                NavigationDrawerItem::text_label("Inbox", true)
                    .colors(colors)
                    .badge(|ctx| {
                        crate::components::Text::new("7").build(ctx);
                    })
                    .build(ctx);
            });
        });
        c.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes = c.arena_nodes();
        let painted = |want: &str| -> Option<Color> {
            nodes.iter().find_map(|n| {
                n.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::TextContent { content, color, .. }
                        if content.as_str() == want =>
                    {
                        Some(*color)
                    }
                    _ => None,
                })
            })
        };
        assert_eq!(
            painted("7"),
            Some(badge_color),
            "the badge slot paints the BADGE colour"
        );
        assert_eq!(
            painted("Inbox"),
            Some(colors.text(true)),
            "and the label still paints its own"
        );
    }

    /// A DISMISSIBLE drawer pushes the content instead of covering it: upstream places the content at
    /// `sheetWidth + offset` and the sheet at `offset` (`NavigationDrawer.kt:578-587`), so an open
    /// drawer costs the content exactly the sheet's width and a closed one costs it nothing.
    #[test]
    fn a_dismissible_drawer_pushes_its_content_instead_of_covering_it() {
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let mut drive = |c: &mut Composer| {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(800.0, 600.0);
                DismissibleNavigationDrawer::new(|ctx| {
                    crate::components::Text::new("page")
                        .modifier(Modifier::new().test_tag(PAGE_TAG))
                        .build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(|ctx| {
                    DrawerSheet::dismissible(|_| {})
                        .modifier(Modifier::new().test_tag(SHEET_TAG))
                        .build(ctx);
                })
                .build(ctx);
            });
            c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        };
        drive(&mut c);
        assert_eq!(x_of(&c, SHEET_TAG), Some(-360.0), "closed: the sheet is off the leading edge");
        assert_eq!(x_of(&c, PAGE_TAG), Some(0.0), "and the content is NOT pushed");

        state.snap_to(DrawerValue::Open);
        drive(&mut c);
        assert_eq!(x_of(&c, SHEET_TAG), Some(0.0), "open: the sheet is flush with the edge");
        assert_eq!(
            x_of(&c, PAGE_TAG),
            Some(360.0),
            "and the content is pushed out by exactly the sheet's width, not covered"
        );
    }

    /// The same two endpoints under RTL, where the CLOSED anchor is `+width` instead of `-width`
    /// (`DrawerState::update_anchors`). The content's pushed distance is therefore a different
    /// expression per direction; the first version negated the LTR one, which parked the page at
    /// `-(360 + 360) = -720` while the drawer was closed — every element of the page off the left
    /// edge. Reported from the demo: Dismissible + RTL showed an empty screen.
    #[test]
    fn a_dismissible_drawer_pushes_its_content_the_right_way_under_rtl() {
        let state = DrawerState::new(DrawerValue::Closed);
        let mut c = comp();
        let mut drive = |c: &mut Composer| {
            let st = state.clone();
            c.compose(move |ctx| {
                crate::layout::adaptive::set_window_size(800.0, 600.0);
                DismissibleNavigationDrawer::new(|ctx| {
                    crate::components::Text::new("page")
                        .modifier(Modifier::new().test_tag(PAGE_TAG))
                        .build(ctx);
                })
                .drawer_state(st.clone())
                .drawer_content(|ctx| {
                    DrawerSheet::dismissible(|_| {})
                        .modifier(Modifier::new().test_tag(SHEET_TAG))
                        .build(ctx);
                })
                .build(ctx);
            });
            c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        };
        // The page is a HUGGING label, so under RTL it right-aligns inside its box: `x_of` reports the
        // label's absolute x, which is not 0 even when the box is. What matters is that the box is
        // inside the window when closed, and that opening moves it LEFT by the sheet's width.
        crate::layout::direction::provides(LayoutDirection::Rtl, || {
            drive(&mut c);
            let closed_page = x_of(&c, PAGE_TAG).expect("the page is laid out");
            // A LITERAL, not a range: the page is a 31dp label right-aligned in its 800dp box, so
            // 800 - 31 = 769. A range would pass a page parked at, say, 400 — which is what a wrong
            // push would produce — and this test exists to catch exactly that.
            assert!(
                (closed_page - (800.0 - 31.0)).abs() < 0.5,
                "closed under RTL the page sits at its right-aligned position, got x={closed_page}"
            );
            let closed_sheet = x_of(&c, SHEET_TAG).expect("the sheet is laid out");
            assert!(
                closed_sheet >= 800.0,
                "and the sheet parks off the trailing edge, got x={closed_sheet}"
            );

            state.snap_to(DrawerValue::Open);
            drive(&mut c);
            let open_page = x_of(&c, PAGE_TAG).expect("the page is laid out");
            assert!(
                (closed_page - open_page - 360.0).abs() < 0.5,
                "opening pushes the page LEFT by the sheet's width: closed {closed_page}, open {open_page}"
            );
            let open_sheet = x_of(&c, SHEET_TAG).expect("the sheet is laid out");
            assert!(
                open_sheet < closed_sheet,
                "and the sheet slides in from the right: closed {closed_sheet}, open {open_sheet}"
            );
        });
    }

    /// A PERMANENT drawer has no state at all: the sheet is always on screen and the content follows
    /// it (`Row { drawerContent(); Box { content() } }`, `NavigationDrawer.kt:618-623`).
    #[test]
    fn a_permanent_drawer_keeps_the_sheet_beside_the_content() {
        let mut c = comp();
        c.compose(|ctx| {
            crate::layout::adaptive::set_window_size(800.0, 600.0);
            PermanentNavigationDrawer::new(|ctx| {
                crate::components::Text::new("page")
                    .modifier(Modifier::new().test_tag(PAGE_TAG))
                    .build(ctx);
            })
            .drawer_content(|ctx| {
                DrawerSheet::permanent(|_| {})
                    .modifier(Modifier::new().test_tag(SHEET_TAG))
                    .build(ctx);
            })
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 600.0));
        let (sheet_x, sheet_w) = sheet_x_and_width(&c);
        assert_eq!(sheet_x, 0.0, "the sheet is docked at the leading edge");
        assert!(
            (sheet_w - 240.0).abs() < 0.5,
            "and sits at MinimumDrawerWidth: the sheet sizes to its content, clamped to [240, 360] \
             (NavigationDrawer.kt:866), and this one's content is empty, got {sheet_w}"
        );
        assert!(x_of(&c, PAGE_TAG).unwrap_or(-1.0) >= sheet_w, "the content starts after it");
    }

    /// The sheet's own clamp is TWO-sided: `sizeIn(240, ContainerWidth)` on the sheet
    /// (`NavigationDrawer.kt:854`, `:866`). winia used to apply only the minimum, so a sheet whose
    /// content wanted more than 360dp simply grew — nothing else in the permanent path bounds it.
    #[test]
    fn the_sheet_stops_at_the_container_width_token() {
        let mut c = comp();
        c.compose(|ctx| {
            crate::layout::adaptive::set_window_size(900.0, 600.0);
            DrawerSheet::permanent(|ctx| {
                crate::components::Text::new("x".repeat(200))
                    .modifier(Modifier::new().test_tag(PAGE_TAG))
                    .build(ctx);
            })
            .modifier(Modifier::new().test_tag(SHEET_TAG))
            .build(ctx);
        });
        c.layout(crate::layout::Constraints::new(0.0, 900.0, 0.0, 600.0));
        let (_x, w) = sheet_x_and_width(&c);
        assert!(
            (w - 360.0).abs() < 0.5,
            "a wide sheet stops at ContainerWidth = 360, got {w}"
        );
    }
}
