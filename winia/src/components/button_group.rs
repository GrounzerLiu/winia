//! Button group (material3 `ButtonGroup`, an M3 Expressive component) — a row of buttons that
//! share their state and, in the CONNECTED form, their container.
//!
//! Facts taken from `androidx-main`'s `ButtonGroup.kt` and the two token files beside it (the
//! numbers are the token files', not the spec's prose):
//!
//! - A plain group spaces its items by `ButtonGroupSmallTokens.BetweenSpace` (12 dp).
//! - A CONNECTED group spaces them by `ConnectedButtonGroupSmallTokens.BetweenSpace` (2 dp). Its
//!   items share ONE container: the leading item rounds its start corners (`ContainerShape` =
//!   `CornerFull`) and squares its end ones to `InnerCornerCornerSize` (`CornerValueSmall` = 8),
//!   the trailing item is the mirror, and a middle item is `ShapeDefaults.Small` (8 on every
//!   corner). While pressed the inner corners tighten to `PressedInnerCornerCornerSize`
//!   (`CornerValueExtraSmall` = 4).
//! - A CHECKED item leaves the shared container entirely: `connectedButtonCheckedShape` is
//!   `CornerFull`, so a selected item pops out as a stadium regardless of where it sits.
//! - `ButtonGroupDefaults.ExpandedRatio` is 0.15: the pressed item grows and its neighbours give
//!   up the room, which is what `Modifier.animateWidth(interactionSource)` drives upstream.
//!
//! The row has NO height of its own. `ButtonGroupSmallTokens.ContainerHeight` (40) is a BUTTON
//! size — it appears only in the token file, never in `ButtonGroup.kt`, whose layout size is
//! `height = placeables.fastMaxBy { it.height }?.height` (`:795`). Forcing 40 here squashed a
//! Medium (56 dp) button, so the row simply takes its tallest item (`docs/button-group.md`).
//!
//! `CornerFull` is a PERCENT corner (half the short side), and a composition does not know the box
//! it will be measured into — so, exactly as `SplitButtonDefaults::outer_corner_size` does, the
//! item height resolves it: 40 / 2 = 20. `Shape::Pill` carries the percent form where the whole
//! item takes it (the checked state), which stays right if a caller ever changes the height.
//!
//! The measure policy carries the rest of upstream's rules: weighted items, the OVERFLOW
//! indicator (a second content set plus a `DropdownMenu` listing the items that did not fit),
//! the pressed-width expansion (`Modifier.animateWidth`, `compressionLimit`) and the parking
//! of an item that did not fit. `docs/button-group.md` records each with its source.

use crate::composable;
use crate::graphics::Shape;
use crate::input::interaction::MutableInteractionSource;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement, measure_node};
use crate::layout::{Alignment, Arrangement, Constraints, LayoutDirection, compute_spacing};
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::Backchannel;
use crate::unit::{Offset, Size};
use std::sync::Arc;

/// `ButtonGroupSmallTokens.ContainerHeight` — also
/// `ConnectedButtonGroupSmallTokens.ContainerHeight`, the two agree.
pub const BUTTON_GROUP_HEIGHT: f32 = 40.0;
/// `ButtonGroupSmallTokens.BetweenSpace`.
pub const BUTTON_GROUP_BETWEEN_SPACE: f32 = 12.0;
/// `ConnectedButtonGroupSmallTokens.BetweenSpace` — the shared container needs only a hairline.
pub const CONNECTED_BUTTON_GROUP_BETWEEN_SPACE: f32 = 2.0;
/// `ButtonGroupDefaults.ExpandedRatio`: how much of its height the pressed item grows by.
pub const BUTTON_GROUP_EXPANDED_RATIO: f32 = 0.15;
/// `ConnectedButtonGroupSmallTokens.InnerCornerCornerSize` (`CornerValueSmall`).
pub const CONNECTED_INNER_CORNER_SIZE: f32 = 8.0;
/// `ConnectedButtonGroupSmallTokens.PressedInnerCornerCornerSize` (`CornerValueExtraSmall`).
pub const CONNECTED_PRESSED_INNER_CORNER_SIZE: f32 = 4.0;

/// How far a neighbour will let a pressed item compress it — upstream's `compressionLimit`,
/// which `animateWidth(source)` resolves to the item's END content padding
/// (`ButtonGroup.kt:1044-1055`) and which the item constructors pass explicitly. For a default
/// material3 button that is `ButtonDefaults.ContentPadding`'s 24 dp, which is winia's baseline
/// button padding too.
pub const BUTTON_GROUP_COMPRESSION_LIMIT: f32 = 24.0;

/// The three shapes a button-group item morphs between — material3's `ToggleButtonShapes`, which is
/// also what `ButtonGroupDefaults.connected*ButtonShapes` build.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonGroupShapes {
    /// The resting shape.
    pub shape: Shape,
    /// The pressed shape.
    pub pressed_shape: Shape,
    /// The checked shape — `connectedButtonCheckedShape`, a stadium.
    pub checked_shape: Shape,
}

impl ButtonGroupShapes {
    /// The shape to paint for a given interaction state, in upstream's precedence (pressed beats
    /// checked beats resting — `shapeByInteraction`).
    pub fn shape_for(&self, pressed: bool, checked: bool) -> Shape {
        if pressed {
            self.pressed_shape
        } else if checked {
            self.checked_shape
        } else {
            self.shape
        }
    }
}

/// Defaults for a button group (material3 `ButtonGroupDefaults`).
pub struct ButtonGroupDefaults;

impl ButtonGroupDefaults {
    /// `CornerFull` resolved against the group's own container height — see the module note.
    pub fn corner_full() -> f32 {
        BUTTON_GROUP_HEIGHT / 2.0
    }

    /// `connectedButtonCheckedShape` = `CornerFull`. A stadium: `Shape::Pill` keeps the
    /// percent form, so it stays a stadium whatever height the item ends up.
    pub fn checked_shape() -> Shape {
        Shape::Pill
    }

    /// `connectedLeadingButtonShapes` — `CornerFull` on the START corners,
    /// `InnerCornerCornerSize` on the end ones, `PressedInnerCornerCornerSize` while pressed.
    ///
    /// `Shape::Corners` is geometric (see its own doc), so the caller resolves start: pass the
    /// row's direction, which is what `layout::direction::current()` is inside a composition.
    pub fn connected_leading_shapes(direction: LayoutDirection) -> ButtonGroupShapes {
        let full = Self::corner_full();
        let cornered = |r: f32| match direction {
            LayoutDirection::Ltr => Shape::corners(full, r, r, full),
            LayoutDirection::Rtl => Shape::corners(r, full, full, r),
        };
        ButtonGroupShapes {
            shape: cornered(CONNECTED_INNER_CORNER_SIZE),
            pressed_shape: cornered(CONNECTED_PRESSED_INNER_CORNER_SIZE),
            checked_shape: Self::checked_shape(),
        }
    }

    /// `connectedMiddleButtonShapes`: `ShapeDefaults.Small` (8) resting, the pressed inner
    /// corner (4) while pressed. Direction-independent — every corner is the same.
    pub fn connected_middle_shapes() -> ButtonGroupShapes {
        ButtonGroupShapes {
            shape: Shape::rounded(CONNECTED_INNER_CORNER_SIZE),
            pressed_shape: Shape::rounded(CONNECTED_PRESSED_INNER_CORNER_SIZE),
            checked_shape: Self::checked_shape(),
        }
    }

    /// `connectedTrailingButtonShapes` — the leading set mirrored.
    pub fn connected_trailing_shapes(direction: LayoutDirection) -> ButtonGroupShapes {
        let full = Self::corner_full();
        let cornered = |r: f32| match direction {
            LayoutDirection::Ltr => Shape::corners(r, full, full, r),
            LayoutDirection::Rtl => Shape::corners(full, r, r, full),
        };
        ButtonGroupShapes {
            shape: cornered(CONNECTED_INNER_CORNER_SIZE),
            pressed_shape: cornered(CONNECTED_PRESSED_INNER_CORNER_SIZE),
            checked_shape: Self::checked_shape(),
        }
    }

    /// The shapes for the item at `index` in a row of `count`: leading, middle or trailing.
    /// A lone item takes the leading set, which rounds its start corners.
    pub fn item_shapes(
        index: usize,
        count: usize,
        direction: LayoutDirection,
    ) -> ButtonGroupShapes {
        if index == 0 {
            Self::connected_leading_shapes(direction)
        } else if index + 1 >= count {
            Self::connected_trailing_shapes(direction)
        } else {
            Self::connected_middle_shapes()
        }
    }

    /// `Icons.Filled.MoreVert` — a VERTICAL ellipsis, which is what upstream's indicator shows.
    pub const MORE_VERT_PATH: &'static str = "M12,8c1.1,0,2-0.9,2-2s-0.9-2-2-2s-0.9,2-2,2S10.9,8,12,8z \
         M12,10c-1.1,0-2,0.9-2,2s0.9,2,2,2s2-0.9,2-2S13.1,10,12,10z \
         M12,16c-1.1,0-2,0.9-2,2s0.9,2,2,2s2-0.9,2-2S13.1,16,12,16z";

    /// `ButtonGroupDefaults::OverflowIndicator` (`ButtonGroup.kt:294-333`) — the indicator upstream
    /// ships as the default: a `FilledIconButton` showing `Icons.Filled.MoreVert` whose click
    /// TOGGLES the menu, inside a tooltip reading `Strings.ButtonGroupMoreOptions`.
    ///
    /// ```ignore
    /// ButtonGroup::new()
    ///     .item(...)
    ///     .overflow_indicator(ButtonGroupDefaults::overflow_indicator)
    ///     .build(ctx);
    /// ```
    pub fn overflow_indicator(ctx: &mut ComposeCtx, menu_state: ButtonGroupMenuState) {
        let label = "More options";
        let state_for_click = menu_state.clone();
        crate::components::tooltip::Tooltip::new(label).build(ctx, |ctx| {
            crate::components::icon_button::IconButton::filled()
                // `IconButtonDefaults.filledShape` is `CornerFull`, i.e. a CIRCLE — not the pill a
                // rectangular button would use (`ButtonGroup.kt:318`).
                .shape(Shape::Circle)
                .on_click(move || {
                    if state_for_click.is_showing() {
                        state_for_click.dismiss();
                    } else {
                        state_for_click.show();
                    }
                })
                .build(ctx, |ctx| {
                    crate::components::icon::Icon::svg_path(Self::MORE_VERT_PATH)
                        .content_description(label)
                        .build(ctx);
                });
        });
    }
}

/// material3's `ButtonGroupMenuState`: what the overflow indicator opens and dismisses.
#[derive(Clone)]
pub struct ButtonGroupMenuState {
    is_showing: crate::runtime::state::State<bool>,
}

impl ButtonGroupMenuState {
    pub fn new() -> Self {
        Self::new_with_initial(false)
    }

    /// `ButtonGroupMenuState(initialIsShowing)` (`ButtonGroup.kt:337`) — the menu can start open,
    /// which is what a preview or a restored state wants.
    pub fn new_with_initial(initial_is_showing: bool) -> Self {
        Self {
            is_showing: crate::runtime::state::State::new(initial_is_showing),
        }
    }

    pub fn is_showing(&self) -> bool {
        self.is_showing.get()
    }

    pub fn show(&self) {
        self.is_showing.set(true);
    }

    pub fn dismiss(&self) {
        self.is_showing.set(false);
    }

    /// The handle `DropdownMenu::new` wants.
    pub fn expanded_state(&self) -> crate::runtime::state::State<bool> {
        self.is_showing.clone()
    }
}

impl Default for ButtonGroupMenuState {
    fn default() -> Self {
        Self::new()
    }
}

/// One item of a group. Upstream builds the same thing from `customItem` plus `Modifier.weight`
/// and `Modifier.animateWidth(interactionSource)` inside the scope; winia has no parent data for a
/// modifier to carry any of that, so an item is assembled before it is added.
pub struct ButtonGroupItem {
    weight: f32,
    press: Option<MutableInteractionSource>,
    compression_limit: f32,
    /// This item's own vertical alignment — upstream's `Modifier.align(Alignment.Vertical)`, which
    /// "will have priority over the ButtonGroup's `verticalAlignment` parameter"
    /// (`ButtonGroup.kt:853-859`). `None` defers to the group's.
    align: Option<Alignment>,
    /// The item's own content. Upstream has THREE implementations behind one interface
    /// (`clickableItem` / `toggleableItem` / `customItem`, `ButtonGroup.kt:1061-1234`); winia
    /// keeps them behind [`ItemKind`] for the same reason: only the labelled forms can build their
    /// own overflow row, and only they know their label.
    kind: ItemKind,
    /// The row this item shows in the overflow menu when it does not fit — upstream's
    /// `MenuContent`. A labelled item builds its own ([`LabelItem::compose_menu_row`]); a custom
    /// one is given one, because only the caller knows what it should look like as a menu row.
    ///
    /// `Fn`, not `FnOnce`: the menu is an overlay and composes its content again on every frame it
    /// is up, exactly as `ModalWideNavigationRail`'s content does.
    menu: Option<Box<dyn Fn(&mut ComposeCtx, ButtonGroupMenuState)>>,
}

/// What an item IS — upstream's three `ButtonGroupItem` implementations collapsed into one enum.
enum ItemKind {
    /// `customItem`: the caller composes the button and supplies the menu row.
    Custom(Box<dyn FnOnce(&mut ComposeCtx, MutableInteractionSource)>),
    /// `clickableItem` / `toggleableItem`: a label, an optional icon, and an action. Upstream
    /// builds both the button and the menu row from those (`ButtonGroup.kt:1075-1217`).
    Label(LabelItem),
}

/// A labelled item — upstream's `ClickableButtonGroupItem` / `ToggleableButtonGroupItem`.
///
/// `Clone`, because the overflow menu composes a row from the SAME data the button used, and the
/// button's content is `FnOnce`: the menu cannot re-run it.
#[derive(Clone)]
struct LabelItem {
    label: String,
    icon: Option<Arc<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    enabled: bool,
    action: LabelAction,
}

#[derive(Clone)]
enum LabelAction {
    Click(Arc<dyn Fn() + Send + Sync>),
    Toggle {
        checked: bool,
        on_change: Arc<dyn Fn(bool) + Send + Sync>,
    },
}

impl LabelItem {
    /// `ToggleButton`'s container roles: `SurfaceContainer` unchecked, `Primary` checked
    /// (`FilledButtonTokens.UnselectedContainerColor` / `SelectedContainerColor`).
    fn toggle_colors(
        theme: &crate::theme::ThemeColors,
        checked: bool,
    ) -> crate::components::button::ButtonColors {
        use crate::graphics::Color;
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        let (container, content) = if checked {
            (theme.primary, theme.on_primary)
        } else {
            (theme.surface_container, theme.on_surface)
        };
        crate::components::button::ButtonColors::new(
            container,
            content,
            alpha(theme.on_surface, 0.12),
            alpha(theme.on_surface, 0.38),
        )
    }

    /// The button — upstream's `ButtonGroupContent` (`ButtonGroup.kt:1084-1129`).
    fn compose_button(self, ctx: &mut ComposeCtx, source: MutableInteractionSource) {
        use crate::components::button::{Button, ButtonStyle};
        let theme = crate::theme::WiniaTheme::colors();
        let has_icon = self.icon.is_some();
        // `ButtonDefaults.ButtonWithIconContentPadding` with an icon, `ContentPadding` without
        // (`ButtonGroup.kt:1087-1092`). Both end at 24 dp, which is why the item's default
        // `compressionLimit` is that number either way.
        let padding = if has_icon {
            crate::components::button::ButtonDefaults::button_with_icon_content_padding()
        } else {
            crate::components::button::ButtonDefaults::content_padding(ButtonStyle::Filled)
        };
        let mut button = Button::new()
            .interaction_source(source)
            .enabled(self.enabled)
            .content_padding(padding);
        match &self.action {
            LabelAction::Click(callback) => {
                let callback = callback.clone();
                button = button.on_click(move || callback());
            }
            LabelAction::Toggle { checked, on_change } => {
                let on_change = on_change.clone();
                let now = *checked;
                button = button.on_click(move || on_change(!now));
                button = button.colors(Self::toggle_colors(&theme, now));
            }
        }
        let icon = self.icon.clone();
        let label = self.label;
        button.build(ctx, |ctx| {
            if let Some(icon) = &icon {
                icon(ctx);
            }
            // `maxLines = 1, softWrap = false, overflow = Visible` (`ButtonGroup.kt:1122-1127`):
            // a wrapped label would make the item taller than the row, and a press NARROWS the
            // item, so it would reflow mid-press.
            crate::components::text::Text::new(label)
                .max_lines(1)
                .soft_wrap(false)
                .overflow(crate::text::style::TextOverflow::Visible)
                .align(crate::text::style::TextAlign::Center)
                .build(ctx);
        });
    }

    /// The overflow row — upstream's `MenuContent` (`ButtonGroup.kt:1132-1142`): a
    /// `DropdownMenuItem` with the same label and leading icon, which runs the action and then
    /// dismisses the menu.
    fn compose_menu_row(&self, ctx: &mut ComposeCtx, state: ButtonGroupMenuState) {
        let mut row = crate::components::dropdown_menu::DropdownMenuItem::new(self.label.clone())
            .enabled(self.enabled);
        if let Some(icon) = self.icon.clone() {
            row = row.leading_icon(move |ctx| icon(ctx));
        }
        row = match &self.action {
            LabelAction::Click(callback) => {
                let callback = callback.clone();
                row.on_click(move || {
                    callback();
                    state.dismiss();
                })
            }
            LabelAction::Toggle { checked, on_change } => {
                let on_change = on_change.clone();
                let now = *checked;
                row.on_click(move || {
                    on_change(!now);
                    state.dismiss();
                })
            }
        };
        row.build(ctx);
    }
}

/// A row of the overflow menu: either a caller-supplied one or a labelled item's own.
enum MenuRow {
    Custom(Box<dyn Fn(&mut ComposeCtx, ButtonGroupMenuState)>),
    Label(LabelItem),
}

impl ButtonGroupItem {
    /// An item, handed the interaction source the group wants its presses reported through.
    ///
    /// USE THAT SOURCE on whatever clickable the item composes. Every item grows while it is
    /// pressed and takes the room off its neighbours — that is not opt-in upstream:
    /// `clickableItem` and `toggleableItem` attach `EnlargeOnPressElement` to every item they
    /// build, in the plain group as much as the connected one (`ButtonGroup.kt:1075-1126`). An
    /// item that ignores the source simply never grows, which is upstream's `customItem`.
    pub fn new(item: impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static) -> Self {
        Self {
            weight: 0.0,
            press: None,
            compression_limit: BUTTON_GROUP_COMPRESSION_LIMIT,
            align: None,
            kind: ItemKind::Custom(Box::new(item)),
            menu: None,
        }
    }

    /// Upstream's `clickableItem(onClick, label, icon, weight, enabled)` (`ButtonGroup.kt:870-876`)
    /// — a `Button` whose shape, padding, single-line label and overflow row the group builds.
    ///
    /// The item's `compressionLimit` comes from the padding the button gets, which upstream reads
    /// off `ButtonDefaults.ContentPadding` (`ButtonGroup.kt:1094`) — winia's default is the same
    /// 24 dp, so the token is unchanged either way.
    pub fn clickable(
        label: impl Into<String>,
        on_click: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::labelled(LabelItem {
            label: label.into(),
            icon: None,
            enabled: true,
            action: LabelAction::Click(Arc::new(on_click)),
        })
    }

    /// Upstream's `toggleableItem(checked, label, onCheckedChange, icon, weight, enabled)`
    /// (`ButtonGroup.kt:888-895`) — the checked container is `primary`, the unchecked one
    /// `surfaceContainer` (`FilledButtonTokens.SelectedContainerColor` / `UnselectedContainerColor`).
    pub fn toggleable(
        label: impl Into<String>,
        checked: bool,
        on_checked_change: impl Fn(bool) + Send + Sync + 'static,
    ) -> Self {
        Self::labelled(LabelItem {
            label: label.into(),
            icon: None,
            enabled: true,
            action: LabelAction::Toggle {
                checked,
                on_change: Arc::new(on_checked_change),
            },
        })
    }

    fn labelled(label: LabelItem) -> Self {
        Self {
            weight: 0.0,
            press: None,
            compression_limit: BUTTON_GROUP_COMPRESSION_LIMIT,
            align: None,
            kind: ItemKind::Label(label),
            menu: None,
        }
    }

    /// The item's leading icon (upstream's `icon` parameter). Ignored on a custom item, which
    /// composes its own content.
    pub fn icon(mut self, icon: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        if let ItemKind::Label(label) = &mut self.kind {
            label.icon = Some(Arc::new(icon));
        }
        self
    }

    /// Whether the item responds to input (upstream's `enabled`).
    pub fn enabled(mut self, enabled: bool) -> Self {
        if let ItemKind::Label(label) = &mut self.kind {
            label.enabled = enabled;
        }
        self
    }

    /// This item's own vertical alignment, overriding the group's `vertical_alignment` —
    /// upstream's `Modifier.align(Alignment.Vertical)` (`ButtonGroup.kt:853-859`).
    pub fn align(mut self, alignment: Alignment) -> Self {
        self.align = Some(alignment);
        self
    }

    /// What this item looks like in the overflow menu. Without one it simply does not appear in
    /// the menu, which is the honest thing: winia's items carry no label of their own.
    pub fn menu(mut self, menu: impl Fn(&mut ComposeCtx, ButtonGroupMenuState) + 'static) -> Self {
        self.menu = Some(Box::new(menu));
        self
    }

    /// Take a proportional share of the space left over — `Modifier.weight`.
    ///
    /// PANICS on a non-positive or NaN weight, which is upstream's `require(weight > 0.0)`
    /// (`ButtonGroup.kt:1402`). Clamping silently was worse than a panic: `weighted_item(0.0, …)`
    /// quietly became an item at its NATURAL width, so a row that meant to divide its space evenly
    /// stayed at whatever its labels happened to measure.
    pub fn weight(mut self, weight: f32) -> Self {
        assert!(
            weight > 0.0 && weight.is_finite(),
            "invalid weight {weight}; must be greater than zero (`ButtonGroup.kt:1402`)"
        );
        self.weight = weight;
        self
    }

    /// The source the group should watch for this item's presses, when the CALLER already owns
    /// one — `Modifier.animateWidth(interactionSource)` with a source of your own. Without this
    /// the group creates one and hands it to [`ButtonGroupItem::new`]'s closure.
    pub fn press_source(mut self, source: MutableInteractionSource) -> Self {
        self.press = Some(source);
        self
    }

    /// The most this item will let a pressed NEIGHBOUR take off it — upstream's `compressionLimit`.
    /// Defaults to `BUTTON_GROUP_COMPRESSION_LIMIT`.
    pub fn compression_limit(mut self, limit: f32) -> Self {
        self.compression_limit = limit.max(0.0);
        self
    }
}

/// The row: items side by side, spaced by the group's token, as tall as its tallest item.
///
/// The caller builds the items and hands each one the shape it should paint
/// (`ButtonGroupDefaults::item_shapes(i, count, direction)`), the way
/// `SingleChoiceSegmentedButtonRow` leaves `SegmentedButtonDefaults::item_shape` to its caller.
pub struct ButtonGroup {
    connected: bool,
    spacing: Option<f32>,
    children: Vec<ButtonGroupItem>,
    /// The indicator that opens the overflow menu — upstream's required `overflowIndicator` slot.
    /// Composed as the group's LAST child, so the policy can measure it before deciding how many
    /// items fit; without one the row never overflows.
    overflow: Option<Box<dyn FnOnce(&mut ComposeCtx, ButtonGroupMenuState)>>,
    /// `ButtonGroup(expandedRatio = …)` — how much of its own width a pressed item grows by.
    expanded_ratio: f32,
    /// `ButtonGroup(horizontalArrangement = …)` — how leftover main-axis space is distributed.
    arrangement: Option<Arrangement>,
    /// `ButtonGroup(verticalAlignment = …)` — where a short child sits on the cross axis.
    vertical_alignment: Alignment,
    modifier: Modifier,
}

impl ButtonGroup {
    pub fn new() -> Self {
        Self {
            connected: false,
            spacing: None,
            children: Vec::new(),
            overflow: None,
            expanded_ratio: BUTTON_GROUP_EXPANDED_RATIO,
            arrangement: None,
            vertical_alignment: Alignment::Start,
            modifier: Modifier::new(),
        }
    }

    /// The connected form: `ConnectedButtonGroupSmallTokens.BetweenSpace` (2 dp) between items, and
    /// the caller is expected to paint them with `item_shapes` so the container reads as one shape.
    pub fn connected() -> Self {
        Self::new().as_connected()
    }

    pub fn as_connected(mut self) -> Self {
        self.connected = true;
        self
    }

    /// `ButtonGroup(expandedRatio = …)` — the percentage of its own width a PRESSED item grows by,
    /// taking the room off its neighbours (`ButtonGroupDefaults.ExpandedRatio` is 0.15).
    ///
    /// `0.0` disables the growth entirely and `1.0` doubles the pressed item
    /// (`ButtonGroup.kt:116-122`). Negative values clamp to zero, which is upstream's
    /// `@FloatRange(0.0)`.
    pub fn expanded_ratio(mut self, ratio: f32) -> Self {
        self.expanded_ratio = ratio.max(0.0);
        self
    }

    /// The ratio this group will actually use.
    pub fn resolved_expanded_ratio(&self) -> f32 {
        self.expanded_ratio
    }

    /// `ButtonGroup(horizontalArrangement = …)` — how the row distributes the main-axis space that
    /// no item claims. Defaults to [`Arrangement::Start`], which packs the items from the leading
    /// edge.
    ///
    /// Upstream's default is `Arrangement.spacedBy(BetweenSpace)`, whose spacing part is this
    /// group's own `spacing`; winia keeps the gap on the container (as `Row::spacing` does) and
    /// takes only the distribution here (`docs/arrangement.md`).
    pub fn arrangement(mut self, arrangement: Arrangement) -> Self {
        self.arrangement = Some(arrangement);
        self
    }

    /// The arrangement this group will actually use.
    pub fn resolved_arrangement(&self) -> Arrangement {
        self.arrangement.unwrap_or(Arrangement::Start)
    }

    /// `ButtonGroup(verticalAlignment = …)` — where a child SHORTER than the row sits on the cross
    /// axis. Upstream's default is `Alignment.Top`, which is [`Alignment::Start`] here.
    ///
    /// A per-item override is [`ButtonGroupItem::align`], upstream's `Modifier.align`.
    pub fn vertical_alignment(mut self, alignment: Alignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    /// Override the spacing. Defaults to the token for the form this group is in.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = Some(spacing);
        self
    }

    /// The spacing this group will actually use.
    pub fn resolved_spacing(&self) -> f32 {
        self.spacing.unwrap_or(if self.connected {
            CONNECTED_BUTTON_GROUP_BETWEEN_SPACE
        } else {
            BUTTON_GROUP_BETWEEN_SPACE
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn item(
        mut self,
        item: impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static,
    ) -> Self {
        self.children.push(ButtonGroupItem::new(item));
        self
    }

    /// An item that takes a PROPORTIONAL share of the space left after the unweighted items have
    /// taken their natural width. Upstream spells this `Modifier.weight` inside the group
    /// (`ButtonGroup.kt:826`) and divides the leftover by the total weight in its measure policy
    /// (`:610-650`) — that branch is what this reproduces: leftover / totalWeight * weight.
    pub fn weighted_item(
        self,
        weight: f32,
        item: impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static,
    ) -> Self {
        self.add(ButtonGroupItem::new(item).weight(weight))
    }

    /// The general form: an item assembled with `ButtonGroupItem`.
    pub fn add(mut self, item: ButtonGroupItem) -> Self {
        self.children.push(item);
        self
    }

    /// The overflow indicator, laid out after the items that fit. Upstream requires this slot;
    /// winia makes it optional, and a group without one simply never overflows.
    pub fn overflow_indicator(
        mut self,
        indicator: impl FnOnce(&mut ComposeCtx, ButtonGroupMenuState) + 'static,
    ) -> Self {
        self.overflow = Some(Box::new(indicator));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.connected);
        let spacing = self.resolved_spacing();
        ctx.changed(&spacing);
        let expanded_ratio = self.resolved_expanded_ratio();
        ctx.changed(&expanded_ratio);
        let arrangement = self.resolved_arrangement();
        ctx.changed(&arrangement);
        let vertical_alignment = self.vertical_alignment;
        ctx.changed(&vertical_alignment);
        // The two POLICY inputs that are not `connected`/`spacing` either. The Skip contract is a
        // whitelist — an undeclared parameter is not compared, so a Clean slot skips past it and
        // the new value silently never executes (`docs/state-handles.md`). The children list and
        // the indicator are the ones a caller actually changes: a group that grows an item or
        // gains an indicator must re-execute, and the weights ride on the children.
        //
        // Latent until now: winia's dirty marking reaches the scope and its DIRECT children
        // (`composer.rs`), which saves a group that is a direct child of the scope that read the
        // changed value. It stops saving one that sits a level below it.
        ctx.changed(&self.children.len());
        ctx.changed(&self.overflow.is_some());
        let key = ctx.next_key();
        let weights: Vec<f32> = self.children.iter().map(|c| c.weight).collect();
        let compression: Vec<f32> = self.children.iter().map(|c| c.compression_limit).collect();
        // The per-item parameters ride on the CHILDREN, so `children.len()` alone does not cover
        // them: changing an item's weight (or its compression limit, or which source it reports
        // through) keeps the count the same, and a Clean slot then Skips past the new values —
        // measured: a 1:1 row changed to 3:1 kept both items at 194 dp. Upstream has no such
        // problem because a modifier that changes re-runs the whole composition.
        ctx.changed(&weights);
        ctx.changed(&compression);
        // An item's own `Modifier.align` — the same whitelist problem, and it is what decides where
        // a short item sits on the cross axis (`ButtonGroup.kt:853-859`).
        ctx.changed(&self.children.iter().map(|c| c.align).collect::<Vec<_>>());
        // Which item already brought its own press source. A source has no `PartialEq`, so the
        // IDENTITY that matters is whether there is one at all plus this build's ordinal — an item
        // that hands a different source changes the list length of the `Some` mask.
        ctx.changed(
            &self
                .children
                .iter()
                .map(|c| c.press.is_some())
                .collect::<Vec<_>>(),
        );
        // The press progress, one per item — `FastSpatial`, the group's own spring upstream
        // (`ButtonGroup.kt:143-144`). An item with no press source stays at zero, which is what
        // makes `animateWidth` opt-in.
        // One source per item, created here unless the caller brought its own, plus the press
        // progress the policy reads. Every item gets them: upstream's own item constructors attach
        // the growth unconditionally, and an item that ignores the source simply never presses it.
        // REMEMBERED, not created per build. `clickable_with_source` watches the source it was
        // handed, so a fresh one on every recomposition forgets the press the instant anything
        // else re-composes the group — the button keeps working (it re-registers with the new
        // source) but the progress this reads goes back to 0 and the item never grows. Upstream
        // remembers its per-item source for the same reason
        // (`ButtonGroup.kt:1085`: `remember { MutableInteractionSource() }`).
        // A BACKCHANNEL, not `remember`: `remember(..).get()` hands back a CLONE, so pushing into
        // it grows nothing that survives the build — every recomposition then pushed a fresh batch
        // and the button got a different source each time, which resets its press state on the spot
        // (no state layer, no ripple) and leaves the group reading `pressed = false` for ever.
        // The backchannel is the write-only handle that persists the grown list without asking for
        // a recomposition.
        let store: Backchannel<Vec<MutableInteractionSource>> = ctx.remember_backchannel(Vec::new);
        let wanted = self.children.len();
        let mut sources = store.peek();
        if sources.len() < wanted {
            while sources.len() < wanted {
                sources.push(MutableInteractionSource::new());
            }
            store.set(sources.clone());
        }
        // An item that brought its own source keeps it.
        for (index, child) in self.children.iter().enumerate() {
            if let Some(own) = &child.press {
                sources[index] = own.clone();
            }
        }
        let mut progress = Vec::with_capacity(sources.len());
        for source in &sources {
            let down = source.state(true).pressed;
            let target = if down { 1.0 } else { 0.0 };
            progress.push(ctx.animate_float_as_state(
                target,
                crate::animation::AnimationSpec::Spring(
                    crate::animation::SpringSpec::spatial_fast(),
                ),
            ));
        }
        // Which EDGE a squeezed item's content is pinned to. Only a pressed item takes room from
        // its neighbours, and each neighbour gives it up on the side that faces the press: the one
        // BEFORE it keeps its leading edge, the one AFTER keeps its trailing edge. Without this the
        // content is centred in the narrowed slot, so both edges move — the first item's left edge
        // slid left and the third item's right edge slid right while the second was held.

        // Which items fit is decided during MEASURE, so the count comes back through a
        // backchannel — a write-only handle, exactly what the measure/layout write-back path is
        // for. The menu is composed from it on the next composition, which is also when the
        // indicator's click has just shown it.
        let visible: Backchannel<usize> = ctx.remember_backchannel(|| usize::MAX);
        let menu_state: ButtonGroupMenuState = ctx.remember(ButtonGroupMenuState::new).get();
        let item_count = self.children.len();
        let has_indicator = self.overflow.is_some();
        // The direction the row mirrors under. Captured HERE, at composition: the policy is a bare
        // struct during measure and `layout::direction::current()` is a composition-time local.
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        let policy = ButtonGroupPolicy {
            spacing,
            arrangement,
            vertical_alignment,
            direction,
            expanded_ratio,
            weights,
            progress,
            compression,
            aligns: self.children.iter().map(|c| c.align).collect(),
            item_count,
            has_indicator,
            visible: visible.clone(),
        };
        // NO fixed height. Upstream never sets one — its layout size is
        // `height = placeables.fastMaxBy { it.height }?.height` (`ButtonGroup.kt:795`), so a row of
        // Medium buttons (56 dp) is 56 dp tall. Forcing `ContainerHeight` (40) here SQUASHED them:
        // measured, a 56 dp item inside a 40 dp group came back 40 dp tall. The token is a button
        // size, not the group's height — `ContainerHeight` appears only in the token file itself,
        // never in `ButtonGroup.kt`.
        let modifier = self.modifier;
        let mut children = self.children;
        // The menu rows are taken out before the items are composed: an item's button content is
        // `FnOnce`, so it cannot be composed twice. The vector is INDEX-PAIRED with the items —
        // `None` where an item declared no menu — because the hidden list is chosen by ITEM
        // INDEX below. Compacting it here mapped an item index onto a shorter list and showed
        // the wrong rows: three items with only the last one given a menu opened an EMPTY menu,
        // and with only the first one given a menu it listed the item that was on screen.
        // Upstream has no such compression — it walks the item list itself
        // (`for (i in overflowState.visibleItemCount until overflowState.totalItemCount)
        // `scope.items[i].MenuContent(menuState)`, `ButtonGroup.kt:157-161`).
        let mut menus: Vec<Option<MenuRow>> = children
            .iter_mut()
            .map(|c| match &c.kind {
                // A labelled item builds its own row from the data it already carries — upstream's
                // `MenuContent` on `ClickableButtonGroupItem` / `ToggleableButtonGroupItem`
                // (`ButtonGroup.kt:1131-1142`, `:1205-1216`).
                ItemKind::Label(label) => Some(MenuRow::Label(label.clone())),
                // A custom item only has a row if the caller gave it one, which is upstream's
                // `customItem(buttonGroupContent, menuContent)` (`ButtonGroup.kt:904-907`).
                ItemKind::Custom(_) => c.menu.take().map(MenuRow::Custom),
            })
            .collect();
        let overflow = self.overflow;
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // An item that did not fit is placed at nothing, but its SUBTREE still has a
                // size and would paint over the items that did fit — winia's renderer draws a
                // zero-sized node's children. Upstream never measures the hidden ones at all, so
                // the equivalent here is to wrap each item in a layer whose alpha reads the
                // count back: 0 for the ones in the menu. The alpha is evaluated at RENDER time
                // (`graphics_layer` takes a closure), so it is last measure's answer.
                for (index, child) in children.drain(..).enumerate() {
                    let source = sources[index].clone();
                    match child.kind {
                        ItemKind::Custom(item) => {
                            if has_indicator {
                                let shown = visible.clone();
                                let alpha = Modifier::new().graphics_layer(move || {
                                    crate::graphics::GraphicsLayerParams {
                                        alpha: if index < shown.peek() { 1.0 } else { 0.0 },
                                        ..Default::default()
                                    }
                                });
                                wrap_item(ctx, alpha, |ctx| item(ctx, source));
                            } else {
                                item(ctx, source);
                            }
                        }
                        ItemKind::Label(label) => {
                            if has_indicator {
                                let shown = visible.clone();
                                let alpha = Modifier::new().graphics_layer(move || {
                                    crate::graphics::GraphicsLayerParams {
                                        alpha: if index < shown.peek() { 1.0 } else { 0.0 },
                                        ..Default::default()
                                    }
                                });
                                wrap_item(ctx, alpha, |ctx| label.compose_button(ctx, source));
                            } else {
                                label.compose_button(ctx, source);
                            }
                        }
                    }
                }
                if let Some(indicator) = overflow {
                    indicator(ctx, menu_state.clone());
                }
                if has_indicator {
                    let shown = visible.peek();
                    // The rows of the items that did NOT fit, picked by item index — see
                    // `hidden_rows`.
                    let hidden = hidden_rows(&mut menus, shown);
                    if !hidden.is_empty() {
                        let state_for_dismiss = menu_state.clone();
                        let state_for_rows = menu_state.clone();
                        // The anchor is an EMPTY node the policy places where the indicator sits,
                        // so the menu drops from the indicator rather than the row's start —
                        // `DropdownMenu::build` anchors to whatever its anchor closure composed
                        // and its group is just another child of this policy.
                        crate::components::dropdown_menu::DropdownMenu::new(
                            menu_state.expanded_state(),
                        )
                        .on_dismiss_request(move || state_for_dismiss.dismiss())
                        .build(
                            ctx,
                            |_ctx| {},
                            move |ctx| {
                                // A `Fn` closure: the menu composes its rows again on every frame
                                // it is up, so the rows are BORROWED, never consumed.
                                for row in &hidden {
                                    match row {
                                        MenuRow::Custom(f) => f(ctx, state_for_rows.clone()),
                                        MenuRow::Label(label) => {
                                            label.compose_menu_row(ctx, state_for_rows.clone())
                                        }
                                    }
                                }
                            },
                        );
                    }
                }
            }
        }
        ctx.end_restartable_group();
    }
}

impl Default for ButtonGroup {
    fn default() -> Self {
        Self::new()
    }
}

/// The menu rows belonging to the items that did NOT fit, chosen by ITEM INDEX.
///
/// `rows` is INDEX-PAIRED with the items (`None` where an item declared no menu), which is the
/// whole point: the cut is an item count, so the list it indexes must be the item list. Building
/// it with `filter_map` — collecting only the items that HAVE a menu — silently mapped an item
/// index onto a shorter list: of three items with only the LAST one given a menu the menu came
/// out EMPTY (that item was unreachable), and with only the FIRST one given a menu it listed the
/// item that was on screen. Upstream keeps the two together by construction —
/// `for (i in overflowState.visibleItemCount until overflowState.totalItemCount)
/// `scope.items[i].MenuContent(menuState)` (`ButtonGroup.kt:157-161`).
fn hidden_rows<T>(rows: &mut [Option<T>], shown: usize) -> Vec<T> {
    rows.iter_mut()
        .enumerate()
        .filter(|(index, _)| *index >= shown)
        .filter_map(|(_, row)| row.take())
        .collect()
}

/// Wraps one item in its own group node, so the group can hang a render-time layer on it.
fn wrap_item(ctx: &mut ComposeCtx, modifier: Modifier, content: impl FnOnce(&mut ComposeCtx)) {
    let key = ctx.next_key();
    // `propagate_min_constraints`: the row measures this wrapper at the SLOT width, and the item
    // inside has to see that as a minimum or it keeps its natural width — a text item whose slot
    // the press widened stayed painted at its old width (measured in section 4: node 75 -> 86, the
    // container did not move). Compose's Box does the same when asked to.
    let policy = crate::layout::box_layout::BoxLayout::new().propagate_min_constraints(true);
    match ctx.start_restartable_group(key, modifier, policy) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => content(ctx),
    }
    ctx.end_restartable_group();
}

/// Lays the items out in a row at a fixed height, spaced by the group's token, dividing the space
/// left after the unweighted items among the weighted ones.
///
/// Upstream does the same arithmetic in `ButtonGroupMeasurePolicy` (`:610-650`): the unweighted
/// items are probed at their intrinsic width, the weighted ones take
/// `(max - fixed - spacingTotal) / totalWeight * weight` at a FIXED width, and the whole row is
/// then arranged along the main axis. The overflow indicator, the pressed-width expansion and the
/// parking of an item that did not fit are all in this policy too.
#[derive(Debug)]
struct ButtonGroupPolicy {
    spacing: f32,
    /// `horizontalArrangement` — how the row distributes space no item claims. The group's
    /// `spacing` is Compose's `spacedBy` gap; this is the distribution around it.
    arrangement: Arrangement,
    /// `verticalAlignment` — where a child shorter than the row sits on the cross axis.
    vertical_alignment: Alignment,
    /// The layout direction the distribution mirrors under. Captured at COMPOSITION
    /// (`build`), because this policy has no node of its own during measure.
    direction: LayoutDirection,
    /// `expandedRatio` — how much of its own width a pressed item grows by.
    expanded_ratio: f32,
    /// One entry per child; 0 means the item takes its natural width.
    weights: Vec<f32>,
    /// One per child: the press progress an `interactive` item animates on. Read during MEASURE,
    /// which registers a layout dependency, so an animation frame re-measures without recomposing —
    /// and read again in `place`, NEVER captured at composition: a press re-measures without
    /// recomposing, so a compose-time snapshot always says nothing is pressed.
    progress: Vec<crate::runtime::state::State<f32>>,
    /// One per child: how far it will let a pressed neighbour compress it.
    compression: Vec<f32>,
    /// One per child: upstream's `Modifier.align`, which overrides `vertical_alignment` for that
    /// item alone (`ButtonGroup.kt:853-859`).
    aligns: Vec<Option<Alignment>>,
    /// How many children are ITEMS. The indicator, when there is one, is the last child.
    item_count: usize,
    /// Whether the row has an OVERFLOW indicator — the extra child after the items. Known at
    /// composition, and it is what decides whether the items are wrapped (`wrap_item`).
    has_indicator: bool,
    /// Written during measure: how many items fit. `usize::MAX` until the first measurement.
    visible: Backchannel<usize>,
}

impl MeasurePolicy for ButtonGroupPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let max_w = constraints.max_width;
        let n = children.len();
        // The items are the first `item_count` children; an indicator, when present, is the last.
        let items = self.item_count.min(n);
        let indicator_index = if self.has_indicator && n > items {
            Some(items)
        } else {
            None
        };
        let weight_of = |i: usize| self.weights.get(i).copied().unwrap_or(0.0);

        // The width the weighted items divide, exactly as upstream resolves it: `maxWidth`, and
        // `minWidth` only when that is INFINITY (`ButtonGroup.kt:617-622`).
        let bounded = max_w.is_finite();
        let target = if bounded {
            max_w
        } else {
            constraints.min_width
        };
        let gaps = if items > 1 {
            self.spacing * (items as f32 - 1.0)
        } else {
            0.0
        };
        let total_weight: f32 = (0..items).map(weight_of).sum();

        // Pass 1: the unweighted items at their natural width. That is what the leftover is
        // measured against, and it is the width they are finally laid out at too (upstream probes
        // with `maxIntrinsicWidth` as well).
        let mut widths = vec![0.0f32; items];
        // The natural size is kept as well as the width, because a parked item needs a size that
        // is NOT zero — see the placement below.
        let mut natural = vec![Size::new(0.0, 0.0); items];
        let mut fixed = 0.0f32;
        for i in 0..items {
            let (size, _) = measure_node(
                nodes,
                policies,
                children[i],
                Constraints::new(0.0, max_w, 0.0, constraints.max_height),
            );
            // EVERY item keeps its natural size, weighted ones included: a hidden item is parked
            // at that size, and leaving a weighted one at the `0 x height` it was initialised
            // with is the zero-width placement this policy's own note below forbids (see
            // `parked_x`) — and the shape `20692cb` removed for the unweighted case.
            natural[i] = size;
            if weight_of(i) > 0.0 {
                continue;
            }
            widths[i] = size.width;
            fixed += size.width;
        }

        // Pass 2: the weighted items share what is left, proportionally. Upstream divides
        // `targetSpace - fixedSpace - arrangementSpacingTotal` (`ButtonGroup.kt:611-671`), where
        // `targetSpace` is `maxWidth`, or `minWidth` when that is unbounded.
        if total_weight > 0.0 {
            let remaining = (target - fixed - gaps).max(0.0);
            let unit = remaining / total_weight;
            for i in 0..items {
                let w = weight_of(i);
                if w > 0.0 {
                    widths[i] = unit * w;
                }
            }
        }

        // How many of them the budget actually holds — upstream's greedy loop
        // (`ButtonGroup.kt:700-711`). It is decided in MEASURE, so it goes back through the
        // backchannel for the menu to read on the next composition.
        //
        // ONLY AN INDICATOR CAN PUSH AN ITEM OUT. Without one every item is laid out however
        // narrow the row is, which is what a plain Row does and what the `visible` value the
        // menu reads has always said — but the layout used to run the greedy loop anyway, and
        // that made the row's own arithmetic decide it. A WEIGHTED item's width is derived FROM
        // the budget, so the widths plus the gaps sum to exactly `budget`: `widths[i] <= remaining`
        // is then a coin flip on the last item and it blinks in and out as the window is resized.
        // How many of them fit — upstream's `desiredWidth` test followed by its greedy loop
        // (`ButtonGroup.kt:682-711`). The ORDER matters and is what this policy used to get
        // wrong: upstream first asks whether the whole row fits WITHOUT the indicator, and only
        // measures the overflow content once it has failed. Reserving the indicator's width up
        // front made the row reserve room it did not need, so an indicator appeared — and an item
        // went into its menu — on rows that had room for everything.
        //
        // It is decided in MEASURE, so the count goes back through the backchannel for the menu to
        // read on the next composition.
        let desired_width = widths.iter().sum::<f32>() + gaps;
        let (visible, indicator) = if indicator_index.is_none() {
            // No indicator slot at all: every item is laid out however narrow the row is, which is
            // what a plain Row does. Fitting without one is not just pointless, it is wrong — a
            // WEIGHTED item's width is derived FROM the budget, so the widths plus the gaps sum to
            // exactly it and `widths[i] <= remaining` becomes a floating-point coin flip on the
            // last item, which blinked as the window was resized.
            (items, Size::new(0.0, 0.0))
        } else if !bounded || desired_width <= max_w {
            // Everything fits (or the row is unbounded, as inside a scroll): all items show and the
            // indicator is NOT measured, exactly as upstream leaves it to `null`.
            (items, Size::new(0.0, 0.0))
        } else {
            let idx = indicator_index.expect("checked above");
            let (size, _) = measure_node(
                nodes,
                policies,
                children[idx],
                Constraints::new(0.0, max_w, 0.0, constraints.max_height),
            );
            // Everything cannot fit, so the indicator takes its room first and the items fill what
            // is left, greedily, until one does not fit (`ButtonGroup.kt:696-705`).
            let mut remaining = max_w - size.width;
            let mut shown = 0usize;
            for i in 0..items {
                if widths[i] <= remaining {
                    remaining -= widths[i] + self.spacing;
                    shown += 1;
                } else {
                    break;
                }
            }
            (shown, size)
        };
        let indicator_shown = indicator_index.is_some() && visible < items;
        self.visible.set(visible);

        // Pass 3: the press growth — upstream's loop at `ButtonGroup.kt:729-770`. A pressed item
        // grows by `expandedRatio * its own width`, halved for a middle item, and takes that much
        // OFF its neighbour(s); the neighbour's `compressionLimit` caps how much it gives up, and
        // its own width caps it again. The deltas are not rounded here — upstream rounds to
        // integer pixels because it lays out in px, winia measures in dp.
        if visible > 1 {
            for index in 0..visible {
                let value = self.progress.get(index).map(|p| p.get()).unwrap_or(0.0);
                if value <= 0.0 {
                    continue;
                }
                let limit = |i: usize| self.compression.get(i).copied().unwrap_or(0.0);
                let target_growth = if index > 0 && index + 1 < visible {
                    value
                        * (self.expanded_ratio * widths[index] / 2.0)
                            .min(limit(index - 1))
                            .min(limit(index + 1))
                } else if index == 0 {
                    value * (self.expanded_ratio * widths[index]).min(limit(1))
                } else {
                    value * (self.expanded_ratio * widths[index]).min(limit(index - 1))
                };
                let actual_growth = if index > 0 && index + 1 < visible {
                    let left = target_growth.min(widths[index - 1]);
                    let right = target_growth.min(widths[index + 1]);
                    widths[index - 1] -= left;
                    widths[index + 1] -= right;
                    left + right
                } else if index == 0 {
                    let right = target_growth.min(widths[1]);
                    widths[1] -= right;
                    right
                } else {
                    let left = target_growth.min(widths[index - 1]);
                    widths[index - 1] -= left;
                    left
                };
                widths[index] += actual_growth;
            }
        }

        // Pass 4: settle at those widths and place. The items that did NOT fit are measured at
        // nothing and placed off to the side — they are in the menu instead, and the policy still
        // owes `place` one entry per child.
        let mut placements = Vec::with_capacity(n);
        // An item that did not fit is PARKED: placed off to the side with its own natural size,
        // never with a zero one. A zero is a feedback loop, not just a cosmetic choice — the
        // next measure's natural-width probe FOLDS a clean node whose constraints have not
        // changed and reads its `measured_size` back, so a zero there reads as "it fits", the
        // visible count flips, the row's width changes with it and the indicator jumps a spacing
        // to the side on alternate frames. Hovering is enough to trigger it, because a recompose
        // dirties the node and the fold and the real measurement then alternate.
        //
        // Parking it far to the right also keeps it out of the hit test, which a zero-sized node
        // at the origin would not be.
        let parked_x = max_w + 1000.0;
        let mut heights = Vec::with_capacity(items);
        for i in 0..items {
            if i >= visible {
                heights.push(natural[i].height);
                placements.push(Placement {
                    size: natural[i],
                    position: Offset::new(parked_x, 0.0),
                });
                continue;
            }
            let (size, _) = measure_node(
                nodes,
                policies,
                children[i],
                Constraints::new(widths[i], widths[i], 0.0, constraints.max_height),
            );
            heights.push(size.height);
            // The node is the SLOT's width, not the width the child reported back. A button
            // floors at its 48 dp touch target, so a squeezed neighbour answers 48 for a 41 dp
            // slot — and a node wider than its slot is painted with the minimum-interactive
            // inset centred inside it, which makes the item shrink about its CENTRE: its leading
            // edge drifts away from the item before it and a gap opens up. Placing it at the slot
            // width instead keeps the leading edge where the slot put it and shrinks the item
            // from the trailing edge, which is what the row is supposed to look like.
            //
            // The position is filled in below, once the distribution and the row's height are
            // known.
            placements.push(Placement {
                size: Size::new(widths[i], size.height),
                position: Offset::new(0.0, 0.0),
            });
        }

        // The row's HEIGHT is the tallest thing it lays out — upstream's
        // `height = placeables.fastMaxBy { it.height }?.height ?: constraints.minHeight`
        // (`ButtonGroup.kt:795`), never a token. A group of Medium buttons (56 dp) is 56 dp tall;
        // forcing `ContainerHeight` (40) squashed them to 40.
        let indicator_height = if indicator_shown {
            indicator.height
        } else {
            0.0
        };
        let mut row_height = indicator_height;
        let mut any_placed = indicator_shown;
        for i in 0..visible {
            row_height = row_height.max(heights[i]);
            any_placed = true;
        }
        if !any_placed {
            row_height = constraints.min_height;
        }

        // Where the items END: the width the row's content really occupies, which is what the
        // arrangement spreads its leftover over — upstream's `mainSpace`.
        let mut content_w = 0.0f32;
        for i in 0..visible {
            content_w += widths[i];
            if i + 1 < visible {
                content_w += self.spacing;
            }
        }
        let indicator_extra = if indicator_shown {
            (if visible > 0 { self.spacing } else { 0.0 }) + indicator.width
        } else {
            0.0
        };

        // The row's WIDTH is its content, floored by the incoming minimum — upstream's
        // `mainAxisLayoutSize = max(mainSpace.coerceAtLeast(0), mainAxisMin)`
        // (`ButtonGroup.kt:783`). The minimum was IGNORED here, so a group inside a fixed-width
        // container collapsed to its content instead of filling that container.
        let layout_w = (content_w + indicator_extra)
            .max(constraints.min_width)
            .min(max_w);

        // `horizontalArrangement` distributes what is left over — upstream arranges through
        // `Arrangement.Horizontal` (`ButtonGroup.kt:536-543`), so `End`/`Center`/`SpaceBetween`
        // pack the row against the far edge or spread it. winia keeps the GAP on the group
        // (`spacing`) and takes only the distribution from the arrangement.
        let gap_count = if visible > 1 { visible - 1 } else { 0 };
        let (spacing_extra, leading) = compute_spacing(
            self.arrangement,
            (layout_w - indicator_extra - content_w).max(0.0),
            gap_count,
        );

        // Lay the items out along the main axis, aligning each on the cross axis.
        let mut cursor = leading;
        for i in 0..visible {
            let h = heights[i];
            // `Modifier.align` on the item itself wins over the group's `verticalAlignment`
            // (`ButtonGroup.kt:799-802`).
            let align = self
                .aligns
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(self.vertical_alignment);
            let y = match align {
                Alignment::End => row_height - h,
                Alignment::Center => (row_height - h) / 2.0,
                // `Start` is upstream's default and `Stretch` has no meaning for an item that is
                // already measured to its own height.
                Alignment::Start | Alignment::Stretch => 0.0,
            };
            placements[i].position = Offset::new(cursor, y);
            cursor += widths[i]
                + if i + 1 < visible {
                    self.spacing + spacing_extra
                } else {
                    0.0
                };
        }

        // The indicator follows the last item that fit, one spacing after it. When everything
        // fitted it is NOT shown at all — upstream leaves `overflowPlaceables` null and never
        // measures the slot (`ButtonGroup.kt:684-689`). Showing it unconditionally was the bug: a
        // group that called `overflow_indicator()` wore a "more" button even with nothing to hide.
        let indicator_x = if indicator_shown {
            if visible > 0 {
                cursor + self.spacing
            } else {
                leading
            }
        } else {
            parked_x
        };
        let indicator_y = if indicator_shown {
            match self.vertical_alignment {
                Alignment::End => row_height - indicator.height,
                Alignment::Center => (row_height - indicator.height) / 2.0,
                Alignment::Start | Alignment::Stretch => 0.0,
            }
        } else {
            0.0
        };
        let indicator_size = if indicator_shown {
            indicator
        } else if indicator_index.is_some() {
            // Parked, but with a real size — the same reason the parked items keep theirs.
            measure_node(
                nodes,
                policies,
                children[indicator_index.expect("has an indicator")],
                Constraints::new(0.0, max_w, 0.0, constraints.max_height),
            )
            .0
        } else {
            Size::new(0.0, 0.0)
        };
        if indicator_index.is_some() {
            placements.push(Placement {
                size: indicator_size,
                position: Offset::new(indicator_x, indicator_y),
            });
        }

        // ── Mirroring for RTL ──
        //
        // The shapes a connected group paints are already direction-resolved
        // (`item_shapes(i, count, direction)` puts the rounded START corners on the right under
        // RTL), so the POSITIONS have to mirror too or the two disagree — which they did: the
        // corners said right-to-left and the placement ran left-to-right regardless.
        //
        // `Arrangement.Absolute.*` is the one family Compose packs to a PHYSICAL edge, so it is
        // exempt, exactly as `flex::measure_flex` treats it.
        if self.direction == LayoutDirection::Rtl
            && !matches!(self.arrangement, Arrangement::Absolute(_))
        {
            let mirror = |x: f32, w: f32| layout_w - x - w;
            for i in 0..visible {
                placements[i].position.x =
                    mirror(placements[i].position.x, placements[i].size.width);
            }
            if indicator_shown {
                placements[items].position.x = mirror(indicator_x, indicator.width);
            }
        }

        // Anything past the items and the indicator is the overflow menu's ANCHOR (see `build`).
        // The menu is positioned from the anchor's own rect, and its first candidate is "the menu's
        // top at the anchor's bottom, its start at the anchor's start"
        // (`MenuPosition.topToAnchorBottom` + `startToAnchorStart` — `MenuAnchorPosition.Below`,
        // material3's default for `DropdownMenu`). Upstream's anchor is the
        // `Box { indicator; DropdownMenu }`, whose rect IS the indicator's.
        //
        // A ZERO-SIZED node at the indicator's BOTTOM-LEFT gives the same two answers for that
        // first candidate and takes no clicks in practice: the hit test uses inclusive bounds, so
        // the node IS hit at its one exact point, but nothing on that path is clickable
        // (`fire_click_along_path` finds no handler) and siblings are tested in reverse, so only
        // that single pixel is affected. Giving it the indicator's full rect instead
        // put a real, clickable-looking box on top of the indicator (later child, painted last),
        // and the indicator stopped opening its own menu: the click landed on an anchor with no
        // clickable. winia has no "ignore hits" modifier to opt out with.
        let anchor_x = placements
            .get(items)
            .map(|p| p.position.x)
            .unwrap_or(indicator_x);
        while placements.len() < n {
            placements.push(Placement {
                size: Size::new(0.0, 0.0),
                position: Offset::new(anchor_x, indicator_height),
            });
        }
        (Size::new(layout_w, row_height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &child) in children.iter().enumerate() {
            let slot = placements[i].size.width;

            // What the item PAINTS. The container follows the SLOT while keeping the inset the
            // component has at rest — at rest the node IS its touch target and the container sits
            // `inset` in from each edge — so a pressed item really widens and a squeezed one really
            // narrows. The slot geometry does the rest for free: a press takes its room from the
            // neighbour's INNER side, so the neighbour's outer slot edge never moves and neither
            // does the outer edge of the container painted inside it.
            //
            // The inset comes from the TOUCH TARGET the modifier declares, never from
            // `reported - cw`: the reported box follows the slot, so deriving it there made the
            // inset grow with the slot and cancelled the very widening it was meant to show.
            //
            // The node that OWNS the container is the item's own node — or, when the row wrapped
            // the item because there is an overflow indicator (`wrap_item`), the single node INSIDE
            // that wrapper. Reading the wrapper itself published nothing (its modifier carries only
            // the render-time alpha layer, no size), so every container inside such a row skipped
            // this whole block and kept its own un-squeezed box.
            let host = paint_host(nodes, child);
            if let Some((cw, ch)) = nodes[host].minimum_interactive_content {
                let reported = nodes[host].measured_size.width;
                // Where that node sits inside the row's slot. Zero for a direct child — the painted
                // box is then already in its own coordinates — and whatever the wrapper's own layout
                // placed it at otherwise (`Alignment::Start` puts it at the slot's start).
                let local = if host == child {
                    0.0
                } else {
                    nodes[host].position.x
                };
                let inset = nodes[host]
                    .modifier
                    .get_minimum_interactive_size()
                    .map(|size| ((size - cw) / 2.0).max(0.0))
                    .unwrap_or(0.0);
                let painted_w = (slot - inset * 2.0).max(0.0);
                // Always `inset` in from the slot's start, because `painted_w` above is the slot
                // minus that same inset on BOTH sides. A branch used to sit here that picked the edge
                // to hold from `self.progress` (leading neighbour keeps its leading edge, trailing one
                // its trailing edge) — but `slot - painted_w - inset`, `(slot - painted_w) / 2` and
                // `inset` are all the SAME number, so it was dead, and it read as if the anchoring
                // happened here. It does not: the outer edge stays put because of the SLOT GEOMETRY
                // (the measure pass takes a press's room off the neighbour's INNER side, so the outer
                // slot edge never moves).
                let content_x = inset;
                let top = (nodes[host].measured_size.height - ch) / 2.0;
                nodes[host].minimum_interactive_painted =
                    Some((content_x - local, top, painted_w, ch));

                // The content inside the node. The modifier placed it against the box the child
                // MEASURED (its touch target), so it has to follow the container this frame paints:
                // a squeezed item's glyph would otherwise sit off the middle of its container.
                let glyph_w = nodes[host]
                    .children
                    .first()
                    .map(|&c| nodes[c].measured_size.width)
                    .unwrap_or(0.0);
                let current = (reported - glyph_w) / 2.0;
                let desired = nodes[host]
                    .minimum_interactive_painted
                    .map(|(left, _, w, _)| left + (w - glyph_w) / 2.0)
                    .unwrap_or(current);
                let shift = current - desired;
                // Apply the DIFFERENCE from what is already applied: `place` runs every frame, but a
                // node whose constraints did not change is served from the fold cache and its
                // children are not re-placed, so an unconditional `-=` would apply again and again
                // and walk the content out of the box (measured: a glyph at 14 ended up at -3).
                let applied = nodes[host].content_shift.unwrap_or(0.0);
                let delta = shift - applied;
                if delta.abs() > 0.01 {
                    let kids = nodes[host].children.clone();
                    for c in kids {
                        nodes[c].position.x -= delta;
                    }
                    if let Some((cx, cy, cw2, ch2)) = nodes[host].content_box_override {
                        nodes[host].content_box_override = Some((cx - delta, cy, cw2, ch2));
                    }
                }
                nodes[host].content_shift = Some(shift);
            }
            nodes[child].position = placements[i].position;
            nodes[child].measured_size = placements[i].size;
        }
    }
}

/// The node that paints a container of its own inside the row's slot.
///
/// Normally that is the item's own node. When the row has an overflow indicator it wraps every item
/// in a layer (`wrap_item`), so the node it sees is that wrapper — which carries no size and
/// therefore publishes no declared box. The item inside it is the one with the container, and it is
/// the node the painted box has to be written on.
fn paint_host(nodes: &[LayoutNode], child: usize) -> usize {
    if nodes[child].minimum_interactive_content.is_some() {
        return child;
    }
    let kids = &nodes[child].children;
    if kids.len() == 1 {
        let inner = kids[0];
        if nodes[inner].minimum_interactive_content.is_some() {
            return inner;
        }
    }
    child
}

#[cfg(test)]
mod tests;
