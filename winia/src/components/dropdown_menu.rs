//! `DropdownMenu` and the exposed variant — the menu component, which lives here rather than in
//! `overlay.rs` because it is a COMPONENT that uses the overlay runtime, not part of it.

use crate::components::{Icon, Text};
use crate::composable;
use crate::graphics::{Color, Shape};
use crate::layout::{Column, Spacer};
use crate::modifier::Modifier;
use crate::overlay::{next_overlay_id, OverlayAnimSpec, OverlayDesc, PopupPosition};
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::{Backchannel, State};
use crate::theme::WiniaTheme;
use std::sync::Arc;

/// material3's `MenuVerticalMargin` (`material/Menu.kt`): the clearance a dropdown menu keeps from the top
/// and bottom window edges, used both as the measure cap and as the fit test for every candidate below.
pub(crate) const MENU_VERTICAL_MARGIN: f32 = 48.0;

/// Where material3 puts a dropdown menu, candidate by candidate —
/// `DropdownMenuPositionProvider.calculatePosition` and the `MenuPosition` factories it builds.
///
/// Vertical, in order: below the anchor (`topToAnchorBottom`) → above it (`bottomToAnchorTop`) → centred on
/// the anchor's TOP edge (`centerToAnchorTop`) → pinned to whichever window edge the anchor is nearer
/// (`topToWindowTop` / `bottomToWindowBottom`). Each is taken if the menu fits INSIDE the window's vertical
/// margin, `MenuVerticalMargin`; the window-alignment candidate never fails, so there is always an answer.
///
/// Horizontal, in order: start-aligned with the anchor → end-aligned → whichever window edge the anchor is
/// nearer (`leftToWindowLeft` / `rightToWindowRight`, whose margin is 0 in M3).
///
/// An overlay bigger than the window it must fit in is centred on that axis instead of being pinned out of
/// view, which is what `WindowAlignmentMarginPosition` does (`MenuPosition.kt`).
pub(crate) fn dropdown_menu_position(
    anchor: (f32, f32, f32, f32),
    size: (f32, f32),
    window: (f32, f32),
) -> (f32, f32) {
    let (ax, ay, aw, ah) = anchor;
    let (w, h) = window;
    let v = MENU_VERTICAL_MARGIN;

    let fits = |y: f32| y >= v && y + size.1 <= h - v;
    let y = if fits(ay + ah) {
        ay + ah
    } else if fits(ay - size.1) {
        ay - size.1
    } else if fits(ay - size.1 / 2.0) {
        ay - size.1 / 2.0
    } else if size.1 >= h - 2.0 * v {
        (h - size.1) / 2.0
    } else if ay + ah / 2.0 < h / 2.0 {
        v
    } else {
        h - v - size.1
    };

    let fits_x = |x: f32| x >= 0.0 && x + size.0 <= w;
    let x = if fits_x(ax) {
        ax
    } else if fits_x(ax + aw - size.0) {
        ax + aw - size.0
    } else if size.0 >= w {
        (w - size.0) / 2.0
    } else if ax + aw / 2.0 < w / 2.0 {
        0.0
    } else {
        w - size.0
    };
    (x, y)
}

/// M3 `MenuItemColors`: the foreground roles a menu item resolves by enabled state.
///
/// Mirrors `androidx.compose.material3.MenuItemColors` field for field, and the defaults mirror
/// `ColorScheme.defaultMenuItemColors`: text `onSurface`, icons `onSurfaceVariant`, and the disabled
/// variants are the same roles at `ListItemDisabled*Opacity` (0.38).
#[derive(Clone, PartialEq)]
pub struct MenuItemColors {
    pub text: crate::graphics::Color,
    pub leading_icon: crate::graphics::Color,
    pub trailing_icon: crate::graphics::Color,
    pub disabled_text: crate::graphics::Color,
    pub disabled_leading_icon: crate::graphics::Color,
    pub disabled_trailing_icon: crate::graphics::Color,
}

impl MenuItemColors {
    /// `MenuDefaults.itemColors()` — from the current theme's roles.
    pub fn defaults() -> Self {
        let c = crate::theme::WiniaTheme::colors();
        Self {
            text: c.on_surface,
            leading_icon: c.on_surface_variant,
            trailing_icon: c.on_surface_variant,
            disabled_text: with_alpha_factor(c.on_surface, DISABLED_ALPHA),
            disabled_leading_icon: with_alpha_factor(c.on_surface, DISABLED_ALPHA),
            disabled_trailing_icon: with_alpha_factor(c.on_surface, DISABLED_ALPHA),
        }
    }

    pub fn text_color(&self, enabled: bool) -> crate::graphics::Color {
        if enabled { self.text } else { self.disabled_text }
    }

    pub fn leading_icon_color(&self, enabled: bool) -> crate::graphics::Color {
        if enabled { self.leading_icon } else { self.disabled_leading_icon }
    }

    pub fn trailing_icon_color(&self, enabled: bool) -> crate::graphics::Color {
        if enabled { self.trailing_icon } else { self.disabled_trailing_icon }
    }
}

impl std::fmt::Debug for MenuItemColors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MenuItemColors")
    }
}

/// material3's `MenuDefaults` (`Menu.kt:181-260`): the values `DropdownMenu` and `DropdownMenuItem` fall
/// back to, published so a caller can name them instead of repeating numbers.
///
/// These ARE the menu's own defaults — `DropdownMenu::build` and `DropdownMenuItem::build` resolve their
/// unset fields through this type, so the published value and the drawn one cannot drift apart.
pub struct MenuDefaults;

impl MenuDefaults {
    /// `MenuDefaults.TonalElevation` — `ElevationTokens.Level0`.
    pub fn tonal_elevation() -> f32 {
        0.0
    }

    /// `MenuDefaults.ShadowElevation` — `MenuTokens.ContainerElevation` (`ElevationTokens.Level2`).
    pub fn shadow_elevation() -> f32 {
        3.0
    }

    /// `MenuDefaults.shape` — `MenuTokens.ContainerShape` (`CornerExtraSmall`, 4dp).
    pub fn shape() -> crate::graphics::Shape {
        crate::graphics::Shape::RoundedRect { corner_radius: 4.0 }
    }

    /// `MenuDefaults.containerColor` — `MenuTokens.ContainerColor` (`surfaceContainer`).
    pub fn container_color() -> crate::graphics::Color {
        crate::theme::WiniaTheme::colors().surface_container
    }

    /// `MenuDefaults.itemColors()` — the theme's menu item roles.
    pub fn item_colors() -> MenuItemColors {
        MenuItemColors::defaults()
    }

    /// `MenuDefaults.DropdownMenuItemContentPadding` — `PaddingValues(horizontal = 12.dp, vertical = 0)`.
    pub fn dropdown_menu_item_content_padding() -> (f32, f32) {
        (DROPDOWN_ITEM_HORIZONTAL_PADDING, 0.0)
    }
}

/// `ListTokens.ListItemDisabled*Opacity` — the disabled foreground alpha, shared with the navigation
/// components (which keep their own copy of this constant; this is the menu's).
const DISABLED_ALPHA: f32 = 0.38;

fn with_alpha_factor(color: crate::graphics::Color, factor: f32) -> crate::graphics::Color {
    crate::graphics::Color::from_argb(
        ((color.a as f32 * factor).round().min(255.0)) as u8,
        color.r,
        color.g,
        color.b,
    )
}

/// material3's `DropdownMenuItemDefaultMinWidth` / `_MaxWidth` (`Menu.kt:527-528`), applied by
/// [`DropdownMenuItem`] as the item's `sizeIn` range — the same place and the same order material3
/// applies them (`Row(modifier.fillMaxWidth().sizeIn(...))`), and the reason the range reaches the menu's
/// width: `sizeIn` sits in the ITEM's own chain, so the menu's `width(IntrinsicSize.Max)` sees the clamped
/// width of each item instead of its raw label.
const DROPDOWN_ITEM_MIN_WIDTH: f32 = 112.0;
const DROPDOWN_ITEM_MAX_WIDTH: f32 = 280.0;

/// material3's `DropdownMenuItemHorizontalPadding` (`Menu.kt:526`) — the horizontal half of
/// `MenuDefaults.DropdownMenuItemContentPadding` (`PaddingValues(horizontal = 12.dp, vertical = 0)`).
const DROPDOWN_ITEM_HORIZONTAL_PADDING: f32 = 12.0;

/// Dropdown menu (mirrors Compose material3 `DropdownMenu`) — anchored to a trigger container;
/// clicking outside dismisses it.
///
/// Every knob mirrors the material3 signature, and so does its default:
///
/// | M3 | winia | default |
/// |---|---|---|
/// | `offset: DpOffset` | [`Self::offset`] | `(0, 0)` |
/// | `shape` | [`Self::shape`] | `MenuTokens.ContainerShape` — CornerExtraSmall (4dp) |
/// | `containerColor` | [`Self::container_color`] | `MenuTokens.ContainerColor` — `surfaceContainer` |
/// | `tonalElevation` | [`Self::tonal_elevation`] | `ElevationTokens.Level0` |
/// | `shadowElevation` | [`Self::shadow_elevation`] | `MenuTokens.ContainerElevation` — Level2 (3dp) |
/// | `border` | [`Self::border`] | `null` |
///
/// ```ignore
/// let expanded = ctx.remember(|| false);
/// DropdownMenu::new(expanded.clone())
///     .build(ctx,
///         |ctx| { Button::new().on_click(|| expanded.set(true)).build(...) },  // 锚点
///         |ctx| {  // 菜单项
///             DropdownMenuItem::new("选项 A").on_click(|| ...).build(ctx);
///         });
/// ```
pub struct DropdownMenu {
    expanded: crate::runtime::state::State<bool>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    /// M3 `modifier` — applied to the menu's own container (the surface), so a caller can tag it or
    /// adjust it. Appended outside the internal modifier, like every other component here.
    modifier: crate::modifier::Modifier,
    /// M3 `scrollState` — `rememberScrollState()` when unset. The container scrolls, so a menu longer
    /// than the space it was given stays reachable instead of hanging off the window edge.
    scroll_state: Option<crate::modifier::ScrollState>,
    /// material3's `matchAnchorWidth` — see [`DropdownMenu::match_anchor_width`]. Off for a plain menu.
    match_anchor_width: bool,
    /// material3's `PopupProperties(focusable = …)`, decided by the anchor type the menu hangs off
    /// (`ExposedDropdownMenu.kt:354` `popupPropertiesForAnchorType(anchorType, alwaysFocusable)`; the
    /// default `DefaultMenuProperties` is `PopupProperties(focusable = true)`,
    /// `androidMain/AndroidMenu.android.kt`). Off for an editable anchor, whose menu must open WITHOUT
    /// taking the keyboard so the text field keeps the caret and the IME.
    focus_scope: bool,
    offset: (f32, f32),
    shape: Option<crate::graphics::Shape>,
    container_color: Option<crate::graphics::Color>,
    tonal_elevation: f32,
    shadow_elevation: Option<f32>,
    border: Option<crate::components::surface::SurfaceBorder>,
}

impl DropdownMenu {
    pub fn new(expanded: crate::runtime::state::State<bool>) -> Self {
        Self {
            expanded,
            on_dismiss: None,
            modifier: crate::modifier::Modifier::new(),
            scroll_state: None,
            match_anchor_width: false,
            focus_scope: true,
            // M3: `DpOffset(0.dp, 0.dp)`. The drop-down placement itself comes from the anchor
            // (`PopupPosition::BottomLeft`), which is the equivalent of the platform popup's anchoring.
            offset: (0.0, 0.0),
            shape: None,
            container_color: None,
            tonal_elevation: 0.0,
            shadow_elevation: None,
            border: None,
        }
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// M3 `modifier` — applied to the menu's container (its surface). Appended outside the internal
    /// modifier, like every other component here.
    pub fn modifier(mut self, modifier: crate::modifier::Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// M3 `scrollState` — the menu's content scrolls through it (`rememberScrollState()` when unset).
    pub fn scroll_state(mut self, state: crate::modifier::ScrollState) -> Self {
        self.scroll_state = Some(state);
        self
    }

    /// material3's `matchAnchorWidth` (on `ExposedDropdownMenu`, which is a `DropdownMenu` with
    /// `exposedDropdownSize`): the menu takes exactly its anchor's width. `ExposedDropdownMenuBox` turns
    /// this on; on its own a menu keeps taking its widest item's width.
    pub fn match_anchor_width(mut self, v: bool) -> Self {
        self.match_anchor_width = v;
        self
    }

    /// material3's `PopupProperties(focusable = …)`: whether the menu takes the keyboard while it is up.
    ///
    /// Default `true`, which is material3's `DefaultMenuProperties`. [`ExposedDropdownMenuBox`] passes
    /// `false` for an editable anchor: material3 opens that menu WITHOUT focus on purpose, so the text
    /// field keeps the caret (and, on a device, the IME) while the list is showing.
    pub fn focus_scope(mut self, v: bool) -> Self {
        self.focus_scope = v;
        self
    }

    /// M3 `offset: DpOffset` — added to the anchored position (x follows the layout direction there;
    /// winia's anchor is explicit, so x is applied as given).
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// M3 `shape` — `MenuDefaults.shape` (CornerExtraSmall, 4dp) when unset.
    pub fn shape(mut self, shape: impl Into<crate::graphics::Shape>) -> Self {
        self.shape = Some(shape.into());
        self
    }

    /// M3 `containerColor` — `MenuDefaults.containerColor` (`surfaceContainer`) when unset.
    pub fn container_color(mut self, color: crate::graphics::Color) -> Self {
        self.container_color = Some(color);
        self
    }

    /// M3 `tonalElevation` — `ElevationTokens.Level0` by default.
    pub fn tonal_elevation(mut self, elevation: f32) -> Self {
        self.tonal_elevation = elevation;
        self
    }

    /// M3 `shadowElevation` — `MenuTokens.ContainerElevation` (Level2, 3dp) when unset.
    pub fn shadow_elevation(mut self, elevation: f32) -> Self {
        self.shadow_elevation = Some(elevation);
        self
    }

    /// M3 `border` — no border by default.
    pub fn border(mut self, border: crate::components::surface::SurfaceBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// `#[composable]`: `remember` (overlay id) / `next_key` (anchor container)
    /// are keyed from the call site.
    #[composable]
    pub fn build(
        self,
        ctx: &mut crate::runtime::composer::ComposeCtx,
        anchor: impl FnOnce(&mut crate::runtime::composer::ComposeCtx),
        menu: impl Fn(&mut crate::runtime::composer::ComposeCtx) + 'static,
    ) {
        let expanded = self.expanded.get(); // Registers dependency — changes trigger recomposition.
        // Anchor container (regular composition — lives in the main tree; the menu
        // is anchored to its position).
        let anchor_key = ctx.next_key();
        let modifier = crate::modifier::Modifier::new();
        let id = ctx.remember(|| next_overlay_id());
        match ctx.start_restartable_group(anchor_key, modifier, crate::layout::box_layout::BoxLayout::new()) {
            crate::runtime::composer::GroupStatus::Skip => {}
            crate::runtime::composer::GroupStatus::Enter => {
                anchor(ctx);
            }
        }
        // The anchor is the GROUP's container node, not whatever the closure happened to compose last:
        // material3 anchors a popup to the parent layout node it sits in, and that is the box this group
        // creates. `composer_slot_key()` after the group reports the last child instead — harmless while
        // an anchor closure ends in its one visible node, wrong as soon as it does not (measured: an
        // `ExposedDropdownMenuBox` anchored to its text field's 24x24 trailing ICON).
        let anchor_slot = anchor_key;
        ctx.end_restartable_group();

        // M3's default is `rememberScrollState()`, i.e. a state the menu owns across frames. Remembered
        // HERE (not inside the menu's content closure) so it keeps one composition position whether or
        // not the menu is open — a conditional `remember` is what shifts the slots after it.
        let scroll_state = match self.scroll_state.clone() {
            Some(s) => s,
            None => ctx.remember(|| crate::modifier::ScrollState::new()).get(),
        };

        // `build` always executes (parameterized by `expanded`) — records active
        // for `sync` to delete (mirrors Popup/Dialog's `visible` parameterization:
        // `expanded=false` records `false` -> delete).
        ctx.record_overlay_active(id.get(), expanded);
        if expanded {
            let shape = self.shape.unwrap_or_else(MenuDefaults::shape);
            let container_color = self
                .container_color
                .unwrap_or_else(MenuDefaults::container_color);
            let shadow_elevation = self.shadow_elevation.unwrap_or_else(MenuDefaults::shadow_elevation);
            let tonal_elevation = self.tonal_elevation;
            let border = self.border;
            let menu_modifier = self.modifier;
            // material3's menu animation targets: scale 0.8 -> 1 with a fade, growing out of the anchor.
            let mut enter_anim = OverlayAnimSpec::default_enter();
            enter_anim.anchor_pivot = true;
            let mut exit_anim = OverlayAnimSpec::default_exit();
            exit_anim.anchor_pivot = true;
            ctx.open_overlay(crate::overlay::OverlayDesc {
                id: id.get(),
                anchor_slot: Some(anchor_slot),
                position: PopupPosition::BottomLeft,
                offset: self.offset,
                anchor_slide: None,
                modal: false,
                // material3's `DefaultMenuProperties = PopupProperties(focusable = true)`
                // (`androidMain/AndroidMenu.android.kt`): a menu's popup owns the keyboard while it is up,
                // so Tab moves within the menu instead of walking the page behind it — and a focused item
                // activates on Enter/Space through the key dispatcher's focused-node path
                // (`app.rs`: "聚焦组件的键盘激活（对标 Compose clickable）"). Esc dismissal does not
                // depend on this: it was already working, and is pinned by a test either way.
                //
                // `ExposedDropdownMenuBox` overrides it per anchor type, which is material3's
                // `popupPropertiesForAnchorType`: an editable anchor's menu must NOT take the keyboard.
                focus_scope: self.focus_scope,
                dismiss_on_outside: true,
                dismiss_on_back_press: true,
                click_passthrough: false,
                // material3's `DropdownMenuPositionProvider`: a menu fits itself around the anchor, so a
                // menu taller than the space below it ends up above or against the window edge instead of
                // hanging off the bottom.
                fit_around_anchor: true,
                // Whatever `ExposedDropdownMenuBox` asked for: material3's `matchAnchorWidth`.
                match_anchor_width: self.match_anchor_width,
                on_dismiss: self.on_dismiss,
                // material3's menu open/close animation (`Menu.kt`'s `DropdownMenuContent`): a transition
                // on `expandedState` driving `graphicsLayer { scaleX/scaleY/alpha }` from
                // `ClosedScaleTarget = 0.8f` / `ClosedAlphaTarget = 0f` to `ExpandedScaleTarget = 1f` /
                // `ExpandedAlphaTarget = 1f`, with `transformOrigin =
                // calculateTransformOrigin(anchorBounds, menuBounds)` — so the menu grows out of its
                // anchor. `OverlayAnimSpec::default_enter/exit` already carries 0.8 + fade, which is
                // exactly those targets; `anchor_pivot` is the transform origin.
                //
                // Not material3's: the duration and curve. M3 reads them from `MotionSchemeKeyTokens.
                // FastSpatial` (scale) and `FastEffects` (alpha), whose values live in the motion scheme
                // and are NOT in the extracted sources here, so this keeps winia's 200ms
                // ease-out-in (Dialog's enters with the same spec) rather than guessing at numbers.
                enter_anim: Some(enter_anim),
                exit_anim: Some(exit_anim),
                // M3's content is `@Composable ColumnScope.() -> Unit`: the items live in a COLUMN that
                // the menu owns, inside the menu's own surface (shape/container/elevation), with
                // `DropdownMenuVerticalPadding` (8dp) above and below. winia has no `ColumnScope`
                // receiver, so the menu wraps the content itself — and the wrapper is load-bearing, not
                // cosmetic: composed as top-level siblings the items collapse to the last one, because a
                // composition's root is a single node (`materialize`: "wrap in a container, or keep
                // emitting siblings, which is its own round"). Measured with three items in a bare
                // composer: arena_len=6 and the root was the LAST item (9x12) — the first two were gone,
                // and the same thing showed up in `overlay_demo` and in the UI fixture's tree.
                content: Box::new(move |ctx| {
                    let mut surface = crate::components::surface::Surface::new()
                        .shape(shape)
                        .color(container_color)
                        .tonal_elevation(tonal_elevation)
                        .shadow_elevation(shadow_elevation);
                    if let Some(b) = border {
                        surface = surface.border(b);
                    }
                    surface.build(ctx, |ctx| {
                        // material3's chain, in its order (`Menu.kt:410`): the caller's modifier, then
                        // `padding(vertical = DropdownMenuVerticalPadding)` (8dp, and OUTSIDE the scroll,
                        // so it stays put while the content moves), then `width(IntrinsicSize.Max)`, then
                        // `verticalScroll(scrollState)`. The intrinsic width is what makes the menu as wide
                        // as its widest item; the scroll is what keeps a menu taller than its space
                        // reachable (a winia scroll container measures `min(its content, the viewport it
                        // was given)`).
                        let m = menu_modifier
                            .clone()
                            // material3 applies `exposedDropdownSize(matchAnchorWidth)` to the menu's
                            // CONTENT, and this is the same thing: `fill_max_width` turns the width the
                            // framework forced (min = max = the anchor's width) into the content's own
                            // inner constraint, which is what the intrinsic step of the pipeline reads.
                            // Without it the forced width would stop at the surface: winia's flex
                            // containers deliberately relax the cross-axis minimum to zero for their
                            // children (`layout/flex.rs`), so the column would fall back to its items'
                            // natural width and the rows' ripples would again cover only part of the panel.
                            // It also overrides the intrinsic width on that axis, which is Compose's own
                            // outcome when the incoming constraints are already fixed (`ExposedDropdownMenu`
                            // forces `minWidth = maxWidth = menuWidth`, and `IntrinsicWidthNode` then
                            // constrains the intrinsic value back into that range).
                            .then(if self.match_anchor_width {
                                crate::modifier::Modifier::new().fill_max_width()
                            } else {
                                crate::modifier::Modifier::new()
                            })
                            .then(crate::modifier::Modifier::new().padding_vertical(8.0))
                            .then(crate::modifier::Modifier::new().width(
                                crate::layout::IntrinsicSize::Max,
                            ))
                            .then(crate::modifier::Modifier::new().vertical_scroll(scroll_state.clone()));
                        crate::layout::Column::new().modifier(m).build(ctx, |ctx| menu(ctx));
                    });
                }),
                local_snapshot: Vec::new(),
            });
        }
    }
}

// ═══════════════ ExposedDropdownMenuBox ═══════════════

/// material3 `ExposedDropdownMenuAnchorType` — what clicking the text field does.
///
/// The enum carries all three of material3's cases so call sites read the same; winia implements the click
/// policy (which is all three differ by at the click itself) and records the rest as absent — see
/// [`ExposedDropdownMenuBox::anchor_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExposedDropdownMenuAnchorType {
    /// A non-editable field, such as a read-only text field. material3: "An anchor of this type will
    /// open the menu with focus" (`ExposedDropdownMenu.kt:459`) — the menu takes the keyboard, so Tab
    /// walks its items and a focused item activates on Enter/Space.
    PrimaryNotEditable,
    /// An editable field, such as a text field the user types into. material3: "An anchor of this type
    /// will open the menu without focus in order to preserve focus on the soft keyboard (IME)"
    /// (`ExposedDropdownMenu.kt:468`) — the menu opens, the field keeps the caret, and typing carries on.
    ///
    /// The pointer still toggles: material3's anchor observes the pointer in the Initial pass and calls
    /// `onExpandedChange` on the up event for every anchor type (`:1430-1433`); the anchor type decides
    /// whether the POPUP takes focus, not whether a click counts. On the keyboard the editable anchor
    /// differs in two ways (`:1436-1457`): the spacebar must not expand the menu (it belongs to the
    /// text), and Tab/ArrowDown/ArrowUp hand the keyboard to the menu instead
    /// (`alwaysFocusable = true` → the popup becomes focusable).
    PrimaryEditable,
    /// An icon that lives inside an editable field and shares the IME with it. material3 opens with
    /// focus only when accessibility services are enabled, and consumes the pointer DOWN so the click
    /// does not move the caret into the field (`ExposedDropdownMenu.kt:1427-1429`).
    ///
    /// winia knows no per-element `menuAnchor` modifier — the anchor here is a whole closure — so this
    /// behaves like [`PrimaryNotEditable`](ExposedDropdownMenuAnchorType::PrimaryNotEditable) (the
    /// accessible branch) and the down-consume is not implemented; recorded in `docs/dropdown-menu.md`.
    SecondaryEditable,
}

/// material3 `ExposedDropdownMenuDefaults`.
pub struct ExposedDropdownMenuDefaults;

impl ExposedDropdownMenuDefaults {
    /// material3's `ExposedDropdownMenuItemHorizontalPadding` — exposed-dropdown items use 16dp of
    /// horizontal padding where plain menu items use 12dp (`ExposedDropdownMenu.kt`).
    pub const ITEM_HORIZONTAL_PADDING: f32 = 16.0;

    /// The content padding such an item passes to
    /// [`DropdownMenuItem::content_padding`] — material3's `MenuItemContentPadding`, which is
    /// `PaddingValues(horizontal = 16dp, vertical = 0)`.
    pub fn item_content_padding() -> (f32, f32) {
        (Self::ITEM_HORIZONTAL_PADDING, 0.0)
    }

    /// material3's `TrailingIcon(expanded)`: `Icons.Filled.ArrowDropDown`, rotated 180° while the menu is
    /// open — a static rotation, exactly as that composable writes it (`modifier.rotate(if (expanded) 180f
    /// else 0f)`, with no animation in this version).
    ///
    /// It takes the STATE rather than the boolean material3 takes. Compose re-runs a composable whose
    /// parameters changed, so `TrailingIcon(expanded)` redraws on its own; winia's groups do not compare
    /// their arguments, so an icon built from a plain bool is composed once and skipped afterwards — the
    /// arrow would stay pointing the way the field first drew it (measured: 16 probe points across the slot,
    /// none of them changed when the menu opened). Reading the state INSIDE the icon's own composition is
    /// what registers the dependency, which is the winia way of saying "redraw me when this changes".
    ///
    /// Compose it into a text field's trailing slot:
    ///
    /// ```ignore
    /// TextField::outlined(value).trailing_icon({
    ///     let expanded = expanded.clone();
    ///     move |ctx| ExposedDropdownMenuDefaults::trailing_icon(ctx, expanded)
    /// })
    /// ```
    pub fn trailing_icon(
        ctx: &mut crate::runtime::composer::ComposeCtx,
        expanded: crate::runtime::state::State<bool>,
        modifier: crate::modifier::Modifier,
    ) {
        let open = expanded.get();
        crate::components::icon::Icon::svg_path(Self::ARROW_DROP_DOWN_PATH)
            .modifier(
                crate::modifier::Modifier::new()
                    .rotate(if open { 180.0 } else { 0.0 })
                    .then(modifier),
            )
            .build(ctx);
    }

    /// Material Icons `arrow_drop_down` (24dp viewBox) — the icon material3 hard-codes for this field's
    /// trailing icon (`ExposedDropdownMenuDefaults.TrailingIcon` draws `Icons.Filled.ArrowDropDown`).
    ///
    /// The data is the icon's OWN: the published 24dp asset from <https://fonts.google.com/icons>
    /// (`google/material-design-icons`, Apache-2.0) is `M7 10l5 5 5-5z`, and winia hands exactly that
    /// string to Skia's SVG parser, which carries the relative commands. Measured, not assumed —
    /// `the_published_arrow_data_draws_the_same_arrow` draws this data and the absolute form of the
    /// same triangle through a real `Icon` and compares the pixels.
    ///
    /// Public because the glyph is material3's, not this component's: a split button's menu trigger is
    /// the same arrow, and its fixture draws it from here instead of copying the path.
    pub const ARROW_DROP_DOWN_PATH: &'static str = "M7 10l5 5 5-5z";
}

/// material3 `ExposedDropdownMenuBox`: a menu hanging off a text field, with the field's width.
///
/// The two halves are the closures it composes — `anchor` is the text field (wrapped so a click toggles
/// the menu, subject to [`ExposedDropdownMenuBox::anchor_type`]) and `menu` is the items, exactly as
/// [`DropdownMenu`] takes them.
///
/// ```ignore
/// let expanded = ctx.remember(|| false);
/// ExposedDropdownMenuBox::new(expanded.clone())
///     .on_expanded_change(move |open| expanded.set(open))
///     .build(ctx,
///         |ctx| { TextField::outlined(value).build(ctx); },
///         |ctx| { DropdownMenuItem::new("选项 A").build(ctx); });
/// ```
pub struct ExposedDropdownMenuBox {
    expanded: crate::runtime::state::State<bool>,
    on_expanded_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    anchor_type: ExposedDropdownMenuAnchorType,
    /// material3's `matchAnchorWidth` on `ExposedDropdownMenu`, default true: the menu is as wide as the
    /// field. M3 forces it, so the content is squeezed rather than the menu outgrowing the field.
    match_anchor_width: bool,
}

impl ExposedDropdownMenuBox {
    pub fn new(expanded: crate::runtime::state::State<bool>) -> Self {
        Self {
            expanded,
            on_expanded_change: None,
            enabled: true,
            anchor_type: ExposedDropdownMenuAnchorType::PrimaryNotEditable,
            match_anchor_width: true,
        }
    }

    /// material3's `onExpandedChange` — called with the new value whenever the box opens or closes.
    pub fn on_expanded_change(mut self, cb: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_expanded_change = Some(Arc::new(cb));
        self
    }

    /// material3's `enabled` on `menuAnchor`: a disabled anchor neither toggles nor opens.
    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }

    /// material3's `menuAnchor(type = …)`.
    ///
    /// Every type toggles on a click — material3's anchor observes the pointer in the Initial pass and
    /// calls `onExpandedChange` on the up event whatever the type is (`ExposedDropdownMenu.kt:1430-1433`).
    /// What the type decides is whether the menu takes the keyboard: `PrimaryNotEditable` opens with focus,
    /// `PrimaryEditable` opens without it so the caret and the IME survive, and Tab/ArrowUp/ArrowDown then
    /// hand the keyboard over. `SecondaryEditable` behaves like `PrimaryNotEditable` here (material3's
    /// accessible branch); see [`ExposedDropdownMenuAnchorType`].
    pub fn anchor_type(mut self, t: ExposedDropdownMenuAnchorType) -> Self {
        self.anchor_type = t;
        self
    }

    /// material3's `matchAnchorWidth` (default `true`).
    pub fn match_anchor_width(mut self, v: bool) -> Self {
        self.match_anchor_width = v;
        self
    }

    #[composable]
    pub fn build(
        self,
        ctx: &mut crate::runtime::composer::ComposeCtx,
        anchor: impl FnOnce(&mut crate::runtime::composer::ComposeCtx) + 'static,
        menu: impl Fn(&mut crate::runtime::composer::ComposeCtx) + 'static,
    ) {
        self.build_inner(
            ctx,
            false,
            Box::new(move |ctx, _anchor_element| anchor(ctx)),
            Box::new(menu),
        );
    }

    /// material3's `Modifier.menuAnchor(type)` applied to an element INSIDE the anchor — its
    /// `SecondaryEditable` shape, where the anchor is an icon in the field rather than the field itself
    /// (`ExposedDropdownMenu.kt:449-482`, `:266-269`).
    ///
    /// The closure receives the modifier to put on that element; the element then owns the toggle, and
    /// the box's own wrapper stops toggling so a click cannot be counted twice. Use this when the anchor
    /// is a field that has its own pointer work (a text cursor): the element takes the press target, so
    /// the click does not move the caret — winia's form of material3's `downEvent.consume()`
    /// (`:1427-1429`). See [`ExposedDropdownMenuAnchorType::SecondaryEditable`].
    #[composable]
    pub fn build_with_anchor_modifier(
        self,
        ctx: &mut crate::runtime::composer::ComposeCtx,
        anchor: impl FnOnce(&mut crate::runtime::composer::ComposeCtx, crate::modifier::Modifier) + 'static,
        menu: impl Fn(&mut crate::runtime::composer::ComposeCtx) + 'static,
    ) {
        self.build_inner(ctx, true, Box::new(anchor), Box::new(menu));
    }

    /// The shared body of [`Self::build`] and [`Self::build_with_anchor_modifier`]; `element_anchor`
    /// tells it which node carries the anchor behaviour.
    #[composable]
    fn build_inner(
        self,
        ctx: &mut crate::runtime::composer::ComposeCtx,
        element_anchor: bool,
        anchor: Box<dyn FnOnce(&mut crate::runtime::composer::ComposeCtx, crate::modifier::Modifier)>,
        menu: Box<dyn Fn(&mut crate::runtime::composer::ComposeCtx) + 'static>,
    ) {
        let expanded = self.expanded.clone();
        let editable = matches!(
            self.anchor_type,
            ExposedDropdownMenuAnchorType::PrimaryEditable
        );
        let secondary = matches!(
            self.anchor_type,
            ExposedDropdownMenuAnchorType::SecondaryEditable
        );
        let enabled = self.enabled;
        // material3's `alwaysFocusable` (`ExposedDropdownMenu.kt:1436-1457`), which feeds
        // `popupPropertiesForAnchorType(anchorType, alwaysFocusable)` (:354): an EDITABLE anchor's menu
        // opens WITHOUT focus — that is the point of `PrimaryEditable`, whose caret and IME must survive —
        // and Tab/ArrowUp/ArrowDown then hand the keyboard over so a keyboard user can reach the list. A
        // non-editable anchor starts with it on, matching `DefaultMenuProperties`.
        //
        // `remember`, not a fresh `State::new`: a plain state built in the composable body would be
        // recreated on every recomposition, so the hand-over below could never outlive the frame that
        // made it (measured: the field kept the keyboard and the escalation test stayed red).
        let keyboard = ctx.remember(|| !editable);
        let on_change = self.on_expanded_change.clone();
        let toggler = {
            let expanded = expanded.clone();
            let on_change = on_change.clone();
            move || {
                let next = !expanded.get();
                expanded.set(next);
                if let Some(cb) = &on_change {
                    (cb)(next);
                }
            }
        };
        // material3's `onPreviewKeyEvent` on the anchor (`ExposedDropdownMenu.kt:1436-1461`), installed on
        // the box so it sees the key before the field's own handling: the anchor owns the activation keys
        // and the "reach for the menu" keys, the field owns everything else — every printable key,
        // including the spacebar.
        //
        // material3's `isClick` is the key UP event, while winia's activation path fires on key DOWN
        // (`app.rs`, "聚焦组件的键盘激活"), so the toggle happens on the way down and the same guard is
        // written in terms of `KeyDown` + `!repeat`.
        let keys = {
            let toggler = toggler.clone();
            let keyboard = keyboard.clone();
            let expanded = expanded.clone();
            move |ke: &crate::input::KbEvent| -> bool {
                use winit::keyboard::{Key, NamedKey};
                if ke.event_type != crate::input::KbEventType::KeyDown || ke.repeat {
                    return false;
                }
                let space = matches!(&ke.key, Key::Character(c) if c.as_str() == " ");
                if space || matches!(&ke.key, Key::Named(NamedKey::Enter)) {
                    // material3: "Primary editable shouldn't expand menu via spacebar" — the space belongs
                    // to the text being typed, so it is left to the field.
                    if editable && space {
                        return false;
                    }
                    if enabled {
                        toggler();
                        return true;
                    }
                    return false;
                }
                if editable
                    && expanded.get()
                    && matches!(
                        &ke.key,
                        Key::Named(NamedKey::Tab)
                            | Key::Named(NamedKey::ArrowDown)
                            | Key::Named(NamedKey::ArrowUp)
                    )
                {
                    // material3's `alwaysFocusable = true`: the menu becomes focusable so the keyboard can
                    // reach it. winia hands the keyboard over instead, by letting the popup become a focus
                    // scope again (`app.rs::claim_keyboard_for_overlay` claims it on the next frame).
                    keyboard.set(true);
                    return true;
                }
                false
            }
        };
        // material3's `Modifier.menuAnchor(type)` on an element inside the anchor, handed to the caller
        // by `build_with_anchor_modifier`.
        //
        // The PRESS registration is the load-bearing part, and it is not decoration: winia's equivalent
        // of material3's `downEvent.consume()` (`ExposedDropdownMenu.kt:1427-1429`) is "be the innermost
        // press target on the hit path", because the press is dispatched to exactly one node
        // (`app.rs::press_gesture_target`). A `Clickable` alone does NOT qualify — `Modifier::has_gesture`
        // lists only the tap and drag callbacks — so an element with `clickable` and no press still lets
        // the field's own `on_press` (the caret placement, `text_field.rs`) fire. Measured:
        // `app::press_target_tests::a_clickable_alone_does_not_take_the_press`.
        //
        // No key handler here, unlike the wrapper form: material3's `onPreviewKeyEvent` reaches an element
        // only while that element (or a descendant) has the focus, and an icon inside a field never does —
        // the field owns the keyboard. That is material3's own outcome for `SecondaryEditable`.
        let anchor_element = {
            let mut m = crate::modifier::Modifier::new();
            if enabled && element_anchor {
                m = m.clickable(toggler.clone()).on_press(|_| {});
                if secondary {
                    // material3's semantics for a secondary anchor (`:1462-1477`): a button that reports
                    // whether the menu is showing. This half is unconditional in material3 — the
                    // accessibility SERVICES condition decides only whether the popup takes focus, and
                    // winia implements the non-accessible branch (see the `focus_scope` call below).
                    m = m.semantics(
                        crate::semantics::SemanticsConfig::new()
                            .role(crate::semantics::SemanticsRole::Button)
                            .merge_descendants(true)
                            .state(
                                crate::semantics::SemanticsState::new().expanded(expanded.get()),
                            ),
                    );
                }
            }
            m
        };
        DropdownMenu::new(expanded.clone())
            .match_anchor_width(self.match_anchor_width)
            // material3's `popupPropertiesForAnchorType(anchorType, alwaysFocusable)`. A non-editable
            // primary anchor opens WITH focus (that is `DefaultMenuProperties`); an editable one opens
            // without it and gets the keyboard handed over by the reach keys above; a SECONDARY anchor's
            // focusability is conditional on accessibility services being on (`ExposedDropdownMenu.kt`
            // :475-482) — a signal winia has no bridge for, so the NON-ACCESSIBLE branch is the one
            // implemented: it opens without focus, and the field that shares its IME keeps the caret. That
            // is also the branch that gives this anchor type its purpose.
            .focus_scope(match self.anchor_type {
                ExposedDropdownMenuAnchorType::PrimaryNotEditable => true,
                ExposedDropdownMenuAnchorType::PrimaryEditable => keyboard.get(),
                ExposedDropdownMenuAnchorType::SecondaryEditable => false,
            })
            .on_dismiss_request({
                let expanded = expanded.clone();
                let on_change = on_change.clone();
                move || {
                    expanded.set(false);
                    if let Some(cb) = &on_change {
                        (cb)(false);
                    }
                }
            })
            .build(
                ctx,
                move |ctx| {
                    // Wrapper behaviour: material3's `menuAnchor` on the field's container, which is what
                    // the primary anchor types use. With the per-element form the element owns the toggle
                    // and the wrapper stays inert — it must not carry a PRESS either, or it would take the
                    // press target back from the element and the caret would move again.
                    let modifier = if enabled && !element_anchor {
                        crate::modifier::Modifier::new()
                            .clickable(toggler)
                            .on_pre_key_event(keys)
                    } else {
                        crate::modifier::Modifier::new()
                    };
                    crate::layout::Column::new()
                        .modifier(modifier)
                        .build(ctx, |ctx| anchor(ctx, anchor_element));
                },
                menu,
            );
    }
}

// ═══════════════ DropdownMenuItem ═══════════════

/// Dropdown menu item — text + click callback.
pub struct DropdownMenuItem {
    text: String,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    /// 调用方 modifier，追加在内部样式**外层**（同 `Button` 约定：可覆盖默认样式；也是测试挂
    /// `test_tag` 的入口）——对齐 M3 `DropdownMenuItem(text, onClick, modifier, …)` 的 modifier。
    modifier: crate::modifier::Modifier,
    /// M3 `colors: MenuItemColors`（未设 = `MenuDefaults.itemColors()`）。
    colors: Option<MenuItemColors>,
    /// M3 `contentPadding`（未设 = 水平 12、垂直 0）。
    content_padding: Option<(f32, f32)>,
    /// M3 `interactionSource: MutableInteractionSource? = null` — the ripple/hover source. `None` means
    /// the item makes and remembers its own.
    interaction_source: Option<crate::interaction::MutableInteractionSource>,
    /// M3 `leadingIcon: @Composable (() -> Unit)? = null` — winia's slot convention is a boxed `FnOnce`,
    /// as in `ListItem::leading_content`.
    leading_icon: Option<Box<dyn FnOnce(&mut crate::runtime::composer::ComposeCtx) + Send + Sync>>,
    /// M3 `trailingIcon: @Composable (() -> Unit)? = null`.
    trailing_icon: Option<Box<dyn FnOnce(&mut crate::runtime::composer::ComposeCtx) + Send + Sync>>,
}

impl DropdownMenuItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_click: None,
            enabled: true,
            modifier: crate::modifier::Modifier::new(),
            colors: None,
            content_padding: None,
            interaction_source: None,
            leading_icon: None,
            trailing_icon: None,
        }
    }

    pub fn modifier(mut self, modifier: crate::modifier::Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn on_click(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(cb));
        self
    }

    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }

    /// M3 `colors: MenuItemColors` — `MenuDefaults.itemColors()` when unset.
    pub fn colors(mut self, colors: MenuItemColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// M3 `contentPadding` — `MenuDefaults.DropdownMenuItemContentPadding` (horizontal 12dp, vertical 0)
    /// when unset. Pass `PaddingValues`-style `(horizontal, vertical)`.
    pub fn content_padding(mut self, horizontal: f32, vertical: f32) -> Self {
        self.content_padding = Some((horizontal, vertical));
        self
    }

    /// M3 `interactionSource` — the press/hover source behind the item's ripple. Left unset the item owns
    /// one (material3's `null` default).
    pub fn interaction_source(
        mut self,
        source: crate::interaction::MutableInteractionSource,
    ) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// M3 `leadingIcon` — drawn in a box at least 24dp wide
    /// (`ListTokens.ListItemLeadingIconSize`), tinted with `MenuItemColors::leading_icon_color`, and the
    /// label starts 12dp after it.
    pub fn leading_icon(
        mut self,
        content: impl FnOnce(&mut crate::runtime::composer::ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.leading_icon = Some(Box::new(content));
        self
    }

    /// M3 `trailingIcon` — same box and tinting on the other side, with 12dp between the label and it.
    pub fn trailing_icon(
        mut self,
        content: impl FnOnce(&mut crate::runtime::composer::ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.trailing_icon = Some(Box::new(content));
        self
    }

    /// `#[composable]`: same contract as Popup/Dialog (marks a composition unit).
    #[composable]
    pub fn build(self, ctx: &mut crate::runtime::composer::ComposeCtx) {
        // M3 geometry (`Menu.kt:439-447`): `sizeIn(minWidth 112dp, maxWidth 280dp, minHeight 48dp)` with
        // `padding(contentPadding)` (horizontal 12dp, vertical 0 by default), and `fillMaxWidth()` so every
        // row spans the menu's width. No per-item background and no per-item corner radius: the MENU's
        // surface paints the container, and the item is a full-width row on top of it.
        //
        // The order is material3's (`modifier.clickable(...).fillMaxWidth().sizeIn(...).padding(...)`).
        // `fillMaxWidth` is NOT part of the item's intrinsic width on purpose: Compose's `FillNode` keeps the
        // default intrinsic approximation (`Size.kt:689`), so the menu's `width(IntrinsicSize.Max)` reads the
        // item's content clamped by this `sizeIn` range — 112dp at the narrow end, 280dp at the wide one.
        let (pad_h, pad_v) = self
            .content_padding
            .unwrap_or(MenuDefaults::dropdown_menu_item_content_padding());
        let modifier = crate::modifier::Modifier::new()
            .fill_max_width()
            .min_width(DROPDOWN_ITEM_MIN_WIDTH)
            .max_width(DROPDOWN_ITEM_MAX_WIDTH)
            .min_height(48.0)
            .padding_horizontal(pad_h)
            .padding_vertical(pad_v);
        let on_click = self.on_click;
        let colors = self.colors.clone().unwrap_or_else(MenuDefaults::item_colors);
        let text_color = colors.text_color(self.enabled);
        // M3's item is `clickable(enabled, onClick, interactionSource, indication = ripple(true))`: a
        // ripple bounded to the item, in the content colour, on an interaction source the item owns unless
        // the caller passes one — the same wiring `Button` uses (`clickable_with_source` +
        // `ripple_with_shape`). The shape is a plain rectangle because M3 gives the item no `shape`: the
        // menu's own Surface clips the corners (`surface.rs` applies `clip(shape)`), and the 8dp vertical
        // padding already keeps the top and bottom items clear of them.
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| crate::interaction::MutableInteractionSource::new()).get());
        let modifier = if self.enabled {
            let modifier = modifier.clickable_with_source(&interaction, move || {
                if let Some(cb) = &on_click {
                    (cb)();
                }
            });
            modifier
                .ripple_with_shape(
                    &interaction,
                    text_color,
                    true,
                    crate::graphics::Shape::Rectangle,
                )
                // No focus RING: material3's menu items mark focus with a state layer, not an outline, and
                // winia draws the ring around any focused node by default (`render.rs`). The highlight is
                // still there — it comes from the same element as the ripple above, whose state layer
                // paints `hover + focus` (`render.rs`: "状态层…hover_opacity + focus_opacity").
                .no_focus_ring()
        } else {
            modifier
        };
        let modifier = modifier.then(self.modifier);
        let text = self.text;
        // M3 typography: `ProvideTextStyle(MaterialTheme.typography.labelLarge)`.
        let style = crate::theme::WiniaTheme::typography().label_large;
        // The item is material3's `Row(verticalAlignment = Alignment.CenterVertically)` — winia's
        // `Row::alignment(Alignment::Center)` centres on the cross axis. (Fixing the centring is what
        // removed the "asymmetric padding" a screenshot showed: the label used to sit at the item's TOP,
        // `pos:[12,0]` in a 48-tall row, i.e. 28px below it and 0 above.)
        //
        // The three children and their geometry are material3's `DropdownMenuItemContent`, verbatim:
        //   leadingIcon  Box(defaultMinSize(minWidth = ListItemLeadingIconSize))     — 24dp
        //   text         Box(weight(1f).padding(start = 12dp if leading, end = 12dp if trailing))
        //   trailingIcon Box(defaultMinSize(minWidth = ListItemTrailingIconSize))    — 24dp
        // each icon tinted with its own colour role through `with_content_color`, which is winia's
        // equivalent of material3's `CompositionLocalProvider(LocalContentColor provides …)`.
        //
        // The label keeps material3's `weight(1f)` unconditionally, and it behaves as material3 intends only
        // because of the menu's own chain: the column is `width(IntrinsicSize.Max)`, so it is as wide as its
        // widest item's intrinsic width, both items are then measured against that fixed width, and the row's
        // `fillMaxWidth` stretches each of them to it. The weight is what pushes the trailing icon to the far
        // end of the row.
        let leading_icon = self.leading_icon;
        let trailing_icon = self.trailing_icon;
        let has_leading = leading_icon.is_some();
        let has_trailing = trailing_icon.is_some();
        let leading_color = colors.leading_icon_color(self.enabled);
        let trailing_color = colors.trailing_icon_color(self.enabled);
        let icon_box = |ctx: &mut crate::runtime::composer::ComposeCtx,
                        color: crate::graphics::Color,
                        content: Box<dyn FnOnce(&mut crate::runtime::composer::ComposeCtx) + Send + Sync>| {
            crate::theme::WiniaTheme::with_content_color(color, ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(crate::modifier::Modifier::new().min_width(24.0))
                    .build(ctx, |ctx| content(ctx));
            });
        };
        crate::layout::Row::new()
            .alignment(crate::layout::node::Alignment::Center)
            .modifier(modifier)
            .build(ctx, |ctx| {
                if let Some(content) = leading_icon {
                    icon_box(ctx, leading_color, content);
                }
                crate::layout::Column::new()
                    .modifier(
                        crate::modifier::Modifier::new()
                            .layout_weight(1.0)
                            .padding_sides(
                                if has_leading { 12.0 } else { 0.0 },
                                0.0,
                                if has_trailing { 12.0 } else { 0.0 },
                                0.0,
                            ),
                    )
                    .build(ctx, |ctx| {
                        crate::components::Text::new(text)
                            .style(style)
                            .color(text_color)
                            .build(ctx);
                    });
                if let Some(content) = trailing_icon {
                    icon_box(ctx, trailing_color, content);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_item_carries_a_ripple_and_no_focus_ring() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            DropdownMenuItem::new("A").build(ctx);
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("the item's node");
        let nodes = composer.arena_nodes();
        let els = nodes[root].modifier.elements();
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::Clickable { .. })),
            "the item is clickable: {:?}",
            els.len()
        );
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::Ripple { .. })),
            "the item carries a ripple (material3's indication)"
        );
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::NoFocusRing)),
            "…and no focus ring: material3 marks menu-item focus with a state layer"
        );
    }

    #[test]
    fn dropdown_menu_position_follows_the_material3_candidates() {
        let window = (420.0, 520.0);
        let size = (120.0, 100.0);
        // 1. Below the anchor, when the whole menu fits under it.
        let below = dropdown_menu_position((16.0, 100.0, 80.0, 32.0), size, window);
        assert_eq!(below, (16.0, 132.0), "below the anchor, start-aligned");
        // 2. Above it, when below would overflow: the anchor sits low, the menu is too tall for the room left.
        let above = dropdown_menu_position((16.0, 400.0, 80.0, 32.0), (120.0, 200.0), window);
        assert_eq!(above, (16.0, 200.0), "above the anchor");
        // 3. Centred on the anchor's TOP edge — both of the first two fail, this one fits.
        let centred = dropdown_menu_position((16.0, 260.0, 80.0, 32.0), (120.0, 400.0), window);
        assert_eq!(centred.1, 60.0, "centred on the anchor's top edge: 260 - 400/2");
        // 4. Pinned to the nearer window edge: nothing fits, so the anchor's half decides which edge. The
        //    menu has to be no taller than the margin band (520 - 96), or the case below takes over.
        let pinned_top = dropdown_menu_position((16.0, 100.0, 80.0, 32.0), (120.0, 400.0), window);
        assert_eq!(pinned_top.1, MENU_VERTICAL_MARGIN, "anchor in the top half pins to the top margin");
        let pinned_bottom = dropdown_menu_position((16.0, 420.0, 80.0, 32.0), (120.0, 400.0), window);
        assert_eq!(
            pinned_bottom.1,
            520.0 - MENU_VERTICAL_MARGIN - 400.0,
            "anchor in the bottom half pins to the bottom margin"
        );
        // 5. Taller than the margin band: centred, not pushed out of the window.
        let too_tall = dropdown_menu_position((16.0, 100.0, 80.0, 32.0), (120.0, 520.0), window);
        assert_eq!(too_tall.1, 0.0, "a window-sized menu is centred: (520 - 520) / 2");

        // Horizontal candidates, with M3's zero margin.
        assert_eq!(
            dropdown_menu_position((16.0, 100.0, 80.0, 32.0), (120.0, 100.0), window).0,
            16.0,
            "start-aligned when it fits"
        );
        let end = dropdown_menu_position((380.0, 100.0, 40.0, 32.0), (120.0, 100.0), window);
        assert_eq!(end.0, 380.0 + 40.0 - 120.0, "end-aligned: right edges meet");
        // Neither alignment fits, so the anchor's half decides the edge. The menu has to be wide enough that
        // both start- and end-alignment overflow, or one of them takes the case.
        let pinned_left = dropdown_menu_position((30.0, 100.0, 40.0, 32.0), (400.0, 100.0), window);
        assert_eq!(pinned_left.0, 0.0, "neither alignment fits: the anchor's half picks the edge");
        let pinned_right = dropdown_menu_position((300.0, 100.0, 40.0, 32.0), (400.0, 100.0), window);
        assert_eq!(pinned_right.0, 420.0 - 400.0, "and the other half picks the other edge");
    }

    #[test]
    fn menu_defaults_are_material3s_tokens() {
        assert_eq!(
            MenuDefaults::tonal_elevation(),
            0.0,
            "MenuDefaults.TonalElevation = ElevationTokens.Level0"
        );
        assert_eq!(
            MenuDefaults::shadow_elevation(),
            3.0,
            "MenuDefaults.ShadowElevation = MenuTokens.ContainerElevation = ElevationTokens.Level2"
        );
        assert!(
            matches!(
                MenuDefaults::shape(),
                crate::graphics::Shape::RoundedRect { corner_radius } if corner_radius == 4.0
            ),
            "MenuDefaults.shape = MenuTokens.ContainerShape = CornerExtraSmall (4dp), got {:?}",
            MenuDefaults::shape()
        );
        assert_eq!(
            MenuDefaults::dropdown_menu_item_content_padding(),
            (12.0, 0.0),
            "MenuDefaults.DropdownMenuItemContentPadding = PaddingValues(horizontal = 12.dp, vertical = 0)"
        );
        assert_eq!(
            MenuDefaults::dropdown_menu_item_content_padding(),
            (DROPDOWN_ITEM_HORIZONTAL_PADDING, 0.0),
            "the published padding and the constant the item applies must be the same value"
        );
    }

    #[test]
    fn menu_defaults_are_what_the_components_resolve_to() {
        let theme = crate::theme::WiniaTheme::colors();
        assert_eq!(
            MenuDefaults::container_color(),
            theme.surface_container,
            "MenuDefaults.containerColor = MenuTokens.ContainerColor = surfaceContainer"
        );
        let colors = MenuDefaults::item_colors();
        assert_eq!(colors.text, theme.on_surface, "MenuDefaults.itemColors() text = onSurface");
        assert_eq!(
            colors.leading_icon, theme.on_surface_variant,
            "MenuDefaults.itemColors() leading icon = onSurfaceVariant"
        );
        assert_eq!(
            colors.disabled_text,
            with_alpha_factor(theme.on_surface, DISABLED_ALPHA),
            "MenuDefaults.itemColors() disabled text = onSurface at the disabled opacity"
        );
        // The item's own fallback goes through the same accessor, so an unset `colors` and
        // `MenuDefaults.itemColors()` cannot drift.
        assert_eq!(
            crate::components::dropdown_menu::MenuItemColors::defaults().text,
            colors.text,
            "MenuItemColors::defaults (what an item resolves to) must equal the published default"
        );
    }

    /// The published `arrow_drop_down` data must draw the arrow, at the icon's own coordinates, and draw
    /// the SAME arrow as the absolute form of the same triangle (what this constant used to carry).
    ///
    /// Skia's SVG parser is what carries the relative commands, so this is the measurement that makes it
    /// safe to keep the icon's own data here instead of a re-derived form; the bounds assertion is what
    /// says the triangle is the real one (it spans x 7..17, y 10..15 in the 24 dp box).
    #[test]
    fn the_published_arrow_data_draws_the_same_arrow() {
        let published = render_arrow(ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH);
        let absolute = render_arrow("M7 10L12 15L17 10z");
        let ink = published.iter().filter(|on| **on).count();
        // Measured: 30 of the 576 pixels in the box. The triangle's area is 25 (½ · 10 · 5) and its
        // bounding box is 50, so the coverage has to land in between, and near the area.
        assert!(
            (20..=45).contains(&ink),
            "the arrow covers {ink} of the 576 pixels in its 24 dp box, not the 25 dp² triangle"
        );
        let diff = published.iter().zip(absolute.iter()).filter(|(a, b)| a != b).count();
        assert!(diff <= 2, "the published data draws the same arrow ({diff} pixels differ)");
        assert_eq!(
            ink_bounds(&published),
            (7, 10, 17, 15),
            "the triangle sits where the official asset puts it"
        );
    }

    /// The 24x24 ink mask of an arrow drawn through the real `Icon` pipeline (node -> render -> pixels).
    fn render_arrow(data: &str) -> Vec<bool> {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            crate::components::icon::Icon::svg_path(data)
                .tint(crate::graphics::Color::BLACK)
                .size(24.0)
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 24.0, 0.0, 24.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((24, 24)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        px.iter().map(|p| p[0] < 128).collect()
    }

    /// The half-open pixel box the mask covers, `(left, top, right, bottom)`, or `(0, 0, 0, 0)` when
    /// nothing was drawn.
    fn ink_bounds(mask: &[bool]) -> (usize, usize, usize, usize) {
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (i, on) in mask.iter().enumerate() {
            if !on {
                continue;
            }
            let (x, y) = (i % 24, i / 24);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
        if left == usize::MAX { (0, 0, 0, 0) } else { (left, top, right, bottom) }
    }
}
