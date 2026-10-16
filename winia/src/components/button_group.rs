//! Button group (material3 `ButtonGroup`, an M3 Expressive component) — a row of buttons that
//! share their state and, in the CONNECTED form, their container.
//!
//! Facts taken from `androidx-main`'s `ButtonGroup.kt` and the two token files beside it (the
//! numbers are the token files', not the spec's prose):
//!
//! - A plain group spaces its items by `ButtonGroupSmallTokens.BetweenSpace` (12 dp) and is
//!   `ButtonGroupSmallTokens.ContainerHeight` (40 dp) tall.
//! - A CONNECTED group spaces them by `ConnectedButtonGroupSmallTokens.BetweenSpace` (2 dp) and is
//!   the same 40 dp tall. Its items share ONE container: the leading item rounds its start corners
//!   (`ContainerShape` = `CornerFull`) and squares its end ones to `InnerCornerCornerSize`
//!   (`CornerValueSmall` = 8), the trailing item is the mirror, and a middle item is
//!   `ShapeDefaults.Small` (8 on every corner). While pressed the inner corners tighten to
//!   `PressedInnerCornerCornerSize` (`CornerValueExtraSmall` = 4).
//! - A CHECKED item leaves the shared container entirely: `connectedButtonCheckedShape` is
//!   `CornerFull`, so a selected item pops out as a stadium regardless of where it sits.
//! - `ButtonGroupDefaults.ExpandedRatio` is 0.15: the pressed item grows and its neighbours give
//!   up the room, which is what `Modifier.animateWidth(interactionSource)` drives upstream.
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
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::layout::{Constraints, LayoutDirection};
use crate::interaction::MutableInteractionSource;
use crate::runtime::state::Backchannel;
use crate::modifier::Modifier;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::unit::{Offset, Size};

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
    pub fn item_shapes(index: usize, count: usize, direction: LayoutDirection) -> ButtonGroupShapes {
        if index == 0 {
            Self::connected_leading_shapes(direction)
        } else if index + 1 >= count {
            Self::connected_trailing_shapes(direction)
        } else {
            Self::connected_middle_shapes()
        }
    }
}

/// material3's `ButtonGroupMenuState`: what the overflow indicator opens and dismisses.
#[derive(Clone)]
pub struct ButtonGroupMenuState {
    is_showing: crate::runtime::state::State<bool>,
}

impl ButtonGroupMenuState {
    pub fn new() -> Self {
        Self { is_showing: crate::runtime::state::State::new(false) }
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
    /// The item's own content. It is handed the interaction source the group watches for the
    /// press growth, because that is the only way the group can see the press: upstream's
    /// `ClickableButtonGroupItem` creates the source and the `Button` inside it together
    /// (`ButtonGroup.kt:1084-1113`).
    item: Box<dyn FnOnce(&mut ComposeCtx, MutableInteractionSource)>,
    /// The row this item shows in the overflow menu when it does not fit. Upstream's
    /// `customItem` takes a `menuContent` alongside the button content for the same reason: only
    /// the caller knows what the item should look like as a menu row.
    ///
    /// `Fn`, not `FnOnce`: the menu is an overlay and composes its content again on every frame it
    /// is up, exactly as `ModalWideNavigationRail`'s content does.
    menu: Option<Box<dyn Fn(&mut ComposeCtx, ButtonGroupMenuState)>>,
}

impl ButtonGroupItem {
    /// An item, handed the interaction source the group wants its presses reported through.
    ///
    /// USE THAT SOURCE on whatever clickable the item composes. Every item grows while it is
    /// pressed and takes the room off its neighbours — that is not opt-in upstream:
    /// `clickableItem` and `toggleableItem` attach `EnlargeOnPressElement` to every item they
    /// build, in the plain group as much as the connected one (`ButtonGroup.kt:1075-1126`). An
    /// item that ignores the source simply never grows, which is upstream's `customItem`.
    pub fn new(
        item: impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static,
    ) -> Self {
        Self {
            weight: 0.0,
            press: None,
            compression_limit: BUTTON_GROUP_COMPRESSION_LIMIT,
            item: Box::new(item),
            menu: None,
        }
    }

    /// What this item looks like in the overflow menu. Without one it simply does not appear in
    /// the menu, which is the honest thing: winia's items carry no label of their own.
    pub fn menu(
        mut self,
        menu: impl Fn(&mut ComposeCtx, ButtonGroupMenuState) + 'static,
    ) -> Self {
        self.menu = Some(Box::new(menu));
        self
    }

    /// Take a proportional share of the space left over — `Modifier.weight`.
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight.max(0.0);
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

/// The row: items side by side, spaced by the group's token, all `BUTTON_GROUP_HEIGHT` tall.
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
    modifier: Modifier,
}

impl ButtonGroup {
    pub fn new() -> Self {
        Self {
            connected: false,
            spacing: None,
            children: Vec::new(),
            overflow: None,
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
        let height = BUTTON_GROUP_HEIGHT;
        let key = ctx.next_key();
        let weights: Vec<f32> = self.children.iter().map(|c| c.weight).collect();
        let compression: Vec<f32> = self.children.iter().map(|c| c.compression_limit).collect();
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
        let store: Backchannel<Vec<MutableInteractionSource>> =
            ctx.remember_backchannel(Vec::new);
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
                crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::spatial_fast()),
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
        let policy = ButtonGroupPolicy {
            spacing,
            height,
            weights,
            progress,
            compression,
            item_count,
            has_indicator,
            visible: visible.clone(),
        };
        let modifier = self.modifier.height(height);
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
        let mut menus: Vec<Option<_>> = children.iter_mut().map(|c| c.menu.take()).collect();
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
                    if has_indicator {
                        let shown = visible.clone();
                        let alpha = Modifier::new().graphics_layer(move || {
                            crate::graphics::GraphicsLayerParams {
                                alpha: if index < shown.peek() { 1.0 } else { 0.0 },
                                ..Default::default()
                            }
                        });
                        wrap_item(ctx, alpha, |ctx| (child.item)(ctx, source));
                    } else {
                        (child.item)(ctx, source);
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
                        crate::components::dropdown_menu::DropdownMenu::new(menu_state.expanded_state())
                            .on_dismiss_request(move || state_for_dismiss.dismiss())
                            .build(
                                ctx,
                                |_ctx| {},
                                move |ctx| {
                                    for row in &hidden {
                                        row(ctx, state_for_rows.clone());
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
fn wrap_item(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    content: impl FnOnce(&mut ComposeCtx),
) {
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
    height: f32,
    /// One entry per child; 0 means the item takes its natural width.
    weights: Vec<f32>,
    /// One per child: the press progress an `interactive` item animates on. Read during MEASURE,
    /// which registers a layout dependency, so an animation frame re-measures without recomposing —
    /// and read again in `place`, NEVER captured at composition: a press re-measures without
    /// recomposing, so a compose-time snapshot always says nothing is pressed.
    progress: Vec<crate::runtime::state::State<f32>>,
    /// One per child: how far it will let a pressed neighbour compress it.
    compression: Vec<f32>,
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
        let indicator_index = if self.has_indicator && n > items { Some(items) } else { None };
        let weight_of = |i: usize| self.weights.get(i).copied().unwrap_or(0.0);

        // The indicator is measured FIRST, because it is what leaves room for the items —
        // upstream measures the overflow content before fitting a single item
        // (`ButtonGroup.kt:695-700`).
        let mut indicator = Size::new(0.0, 0.0);
        if let Some(idx) = indicator_index {
            let (size, _) = measure_node(
                nodes,
                policies,
                children[idx],
                Constraints::new(0.0, max_w, 0.0, constraints.max_height),
            );
            indicator = size;
        }
        let budget = if indicator_index.is_some() {
            (max_w - indicator.width - self.spacing).max(0.0)
        } else {
            max_w
        };
        let gaps = if items > 1 { self.spacing * (items as f32 - 1.0) } else { 0.0 };
        let total_weight: f32 = (0..items).map(weight_of).sum();

        // Pass 1: the unweighted items at their natural width. That is what the leftover is
        // measured against, and it is the width they are finally laid out at too (upstream probes
        // with `maxIntrinsicWidth` as well).
        let mut widths = vec![0.0f32; items];
        // The natural size is kept as well as the width, because a parked item needs a size that
        // is NOT zero — see the placement below.
        let mut natural = vec![Size::new(0.0, self.height); items];
        let mut fixed = 0.0f32;
        for i in 0..items {
            let (size, _) = measure_node(
                nodes,
                policies,
                children[i],
                Constraints::new(0.0, budget, 0.0, constraints.max_height),
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

        // Pass 2: the weighted items share what is left, proportionally.
        if total_weight > 0.0 {
            let remaining = (budget - fixed - gaps).max(0.0);
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
        let visible = if indicator_index.is_some() {
            let mut remaining = budget;
            let mut shown = 0usize;
            for i in 0..items {
                if widths[i] <= remaining {
                    remaining -= widths[i] + self.spacing;
                    shown += 1;
                } else {
                    break;
                }
            }
            shown
        } else {
            items
        };
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
                        * (BUTTON_GROUP_EXPANDED_RATIO * widths[index] / 2.0)
                            .min(limit(index - 1))
                            .min(limit(index + 1))
                } else if index == 0 {
                    value * (BUTTON_GROUP_EXPANDED_RATIO * widths[index]).min(limit(1))
                } else {
                    value * (BUTTON_GROUP_EXPANDED_RATIO * widths[index]).min(limit(index - 1))
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

        // Pass 4: settle at those widths and place, left to right. The items that did NOT fit are
        // measured at nothing and placed at the origin — they are in the menu instead, and the
        // policy still owes `place` one entry per child.
        let mut placements = Vec::with_capacity(n);
        let mut x = 0.0f32;
        let mut used_height = self.height;
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
        for i in 0..items {
            if i >= visible {
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
            // The node is the SLOT's width, not the width the child reported back. A button
            // floors at its 48 dp touch target, so a squeezed neighbour answers 48 for a 41 dp
            // slot — and a node wider than its slot is painted with the minimum-interactive
            // inset centred inside it, which makes the item shrink about its CENTRE: its leading
            // edge drifts away from the item before it and a gap opens up. Placing it at the slot
            // width instead keeps the leading edge where the slot put it and shrinks the item
            // from the trailing edge, which is what the row is supposed to look like.
            placements.push(Placement {
                size: Size::new(widths[i], size.height),
                position: Offset::new(x, 0.0),
            });
            used_height = used_height.max(size.height);
            // Advance by the SLOT, not by what the child reported. A child may come back WIDER
            // than its slot — a button floors at its 48 dp touch target and cannot shrink past
            // it — and letting that widen the row is what made an icon-only group overflow its
            // own width on every press (and shove everything to the right along). Upstream does
            // the same: its positions come from `widths[]` and its layout size from `mainSpace` /
            // `mainAxisLayoutSize` (`ButtonGroup.kt:688`, `:783`), never from the placeables.
            x += widths[i];
            // Between items only — a trailing gap would widen the row by one spacing.
            if i + 1 < visible {
                x += self.spacing;
            }
        }
        // The indicator follows the last item that fit, one spacing after it.
        if indicator_index.is_some() && visible > 0 {
            x += self.spacing;
        }
        let indicator_x = x;
        if indicator_index.is_some() {
            placements.push(Placement {
                size: indicator,
                position: Offset::new(x, 0.0),
            });
            used_height = used_height.max(indicator.height);
            x += indicator.width;
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
        while placements.len() < n {
            placements.push(Placement {
                size: Size::new(0.0, 0.0),
                position: Offset::new(indicator_x, indicator.height),
            });
        }
        (Size::new(x.min(max_w), used_height), placements)
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
                let local = if host == child { 0.0 } else { nodes[host].position.x };
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
                nodes[host].minimum_interactive_painted = Some((content_x - local, top, painted_w, ch));

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
mod tests {
    use super::*;

    /// The constants, against both token files.
    #[test]
    fn the_button_group_constants_are_the_tokens() {
        assert_eq!(BUTTON_GROUP_HEIGHT, 40.0, "ButtonGroupSmallTokens.ContainerHeight");
        assert_eq!(BUTTON_GROUP_BETWEEN_SPACE, 12.0, "ButtonGroupSmallTokens.BetweenSpace");
        assert_eq!(
            CONNECTED_BUTTON_GROUP_BETWEEN_SPACE, 2.0,
            "ConnectedButtonGroupSmallTokens.BetweenSpace"
        );
        assert_eq!(BUTTON_GROUP_EXPANDED_RATIO, 0.15, "ButtonGroupDefaults.ExpandedRatio");
        assert_eq!(
            CONNECTED_INNER_CORNER_SIZE, 8.0,
            "InnerCornerCornerSize is CornerValueSmall"
        );
        assert_eq!(
            CONNECTED_PRESSED_INNER_CORNER_SIZE, 4.0,
            "PressedInnerCornerCornerSize is CornerValueExtraSmall"
        );
    }

    /// The connected corners, item by item, in both directions. The START corners are
    /// `CornerFull` (20 on a 40 dp item) and the inner pair is 8, tightening to 4 while pressed;
    /// a middle item is 8 all round.
    #[test]
    fn the_connected_shapes_are_the_tokens() {
        let lead = ButtonGroupDefaults::connected_leading_shapes(LayoutDirection::Ltr);
        assert_eq!(lead.shape, Shape::corners(20.0, 8.0, 8.0, 20.0), "LTR leading");
        assert_eq!(
            lead.pressed_shape,
            Shape::corners(20.0, 4.0, 4.0, 20.0),
            "LTR leading, pressed"
        );
        assert_eq!(lead.checked_shape, Shape::Pill, "checked pops out as a stadium");

        assert_eq!(
            ButtonGroupDefaults::connected_leading_shapes(LayoutDirection::Rtl).shape,
            Shape::corners(8.0, 20.0, 20.0, 8.0),
            "under RTL the START corners are the right ones"
        );

        let mid = ButtonGroupDefaults::connected_middle_shapes();
        assert_eq!(mid.shape, Shape::rounded(8.0), "a middle item is ShapeDefaults.Small");
        assert_eq!(mid.pressed_shape, Shape::rounded(4.0));

        let trail = ButtonGroupDefaults::connected_trailing_shapes(LayoutDirection::Ltr);
        assert_eq!(trail.shape, Shape::corners(8.0, 20.0, 20.0, 8.0), "LTR trailing");
        assert_eq!(trail.pressed_shape, Shape::corners(4.0, 20.0, 20.0, 4.0));
    }

    /// `item_shapes` picks by position, and the trailing test is `index + 1 >= count` — a
    /// two-item row must not let index 1 fall through to the middle set.
    #[test]
    fn item_shapes_pick_by_position() {
        let ltr = LayoutDirection::Ltr;
        assert_eq!(
            ButtonGroupDefaults::item_shapes(0, 2, ltr).shape,
            ButtonGroupDefaults::connected_leading_shapes(ltr).shape
        );
        assert_eq!(
            ButtonGroupDefaults::item_shapes(1, 2, ltr).shape,
            ButtonGroupDefaults::connected_trailing_shapes(ltr).shape
        );
        assert_eq!(
            ButtonGroupDefaults::item_shapes(1, 3, ltr).shape,
            ButtonGroupDefaults::connected_middle_shapes().shape,
            "the middle of three is a middle item"
        );
        assert_eq!(
            ButtonGroupDefaults::item_shapes(2, 3, ltr).shape,
            ButtonGroupDefaults::connected_trailing_shapes(ltr).shape
        );
    }

    /// `shape_for` follows upstream's `shapeByInteraction` precedence: pressed wins.
    #[test]
    fn the_pressed_shape_beats_the_checked_one() {
        let shapes = ButtonGroupDefaults::connected_leading_shapes(LayoutDirection::Ltr);
        assert_eq!(shapes.shape_for(false, false), shapes.shape);
        assert_eq!(shapes.shape_for(false, true), shapes.checked_shape);
        assert_eq!(shapes.shape_for(true, false), shapes.pressed_shape);
        assert_eq!(shapes.shape_for(true, true), shapes.pressed_shape);
    }

    fn leaf(ctx: &mut ComposeCtx, w: f32, h: f32) {
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new().size(w, h));
        ctx.end_node();
    }

    /// A squeezed icon button still paints its DECLARED square, not the box it was forced into.
    ///
    /// The renderer sizes the painted container from `minimum_interactive_content`, and that field
    /// has to be the size the component declared (`size(40, 40)` for an IconButton) rather than the
    /// MEASURED box: a ButtonGroup forces the node to the slot, so reading the measurement back
    /// painted the icon button as an oval — measured in the demo at 47.33 x 39.33 where the circle
    /// is 40 x 40.
    #[test]
    fn a_squeezed_icon_button_paints_its_declared_square() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .add(
                                ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                                    .press_source(pressed.clone()),
                            )
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path(
                                            "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z",
                                        )
                                        .size(24.0)
                                        .build(ctx);
                                    });
                            })
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (the item's node width, the content size the renderer will paint)
        let painted = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let item = nodes[root].children[1];
            let (cw, ch) = nodes[item]
                .minimum_interactive_content
                .expect("an IconButton publishes its declared content size");
            (nodes[item].measured_size.width, cw, ch)
        };
        build(&mut composer);
        let (node_w, cw, ch) = painted(&composer);
        assert_eq!((node_w, cw, ch), (48.0, 40.0, 40.0), "at rest");

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (node_w, cw, ch) = painted(&composer);
        assert!(node_w < 48.0, "the press squeezed the node, got {node_w}");
        assert_eq!(
            (cw, ch),
            (40.0, 40.0),
            "the painted container is still the declared 40 x 40 square — reading the measured box \
             back made it an oval"
        );
    }

    /// The row's geometry, measured on a real composition: the items sit at the group's spacing
    /// and the box is the token's height. This is the whole of what `ButtonGroupPolicy` claims to
    /// do — an item's own width is its own business.
    #[test]
    fn the_row_spaces_its_items_by_its_token() {
        let measured = |connected: bool| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                let group = if connected { ButtonGroup::connected() } else { ButtonGroup::new() };
                group
                    .item(|ctx, _source| leaf(ctx, 60.0, 40.0))
                    .item(|ctx, _source| leaf(ctx, 60.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let root_size = nodes[root].measured_size;
            let children = nodes[root].children.clone();
            let second_x = nodes[children[1]].position.x;
            (root_size, second_x)
        };

        let (plain, plain_x) = measured(false);
        assert_eq!(plain.height, BUTTON_GROUP_HEIGHT, "the group is the token's height");
        assert_eq!(plain_x, 60.0 + BUTTON_GROUP_BETWEEN_SPACE, "a plain group spaces by 12");
        assert_eq!(plain.width, 120.0 + BUTTON_GROUP_BETWEEN_SPACE);

        let (connected, connected_x) = measured(true);
        assert_eq!(connected.height, BUTTON_GROUP_HEIGHT);
        assert_eq!(
            connected_x,
            60.0 + CONNECTED_BUTTON_GROUP_BETWEEN_SPACE,
            "and a connected one by 2, so its items share a container"
        );
        assert_eq!(connected.width, 120.0 + CONNECTED_BUTTON_GROUP_BETWEEN_SPACE);
    }

    /// Weighted items share what the unweighted ones leave, proportionally — upstream's
    /// `(max - fixed - spacingTotal) / totalWeight * weight` (`ButtonGroup.kt:610-650`).
    #[test]
    fn weighted_items_share_the_space_left_over() {
        // Two equal weights in a 400 dp row, with the pair's 12 dp between them.
        let (size, second_x, second_w) = weighted_row(400.0, &[(1.0, true), (1.0, true)]);
        assert_eq!(second_x, 194.0 + BUTTON_GROUP_BETWEEN_SPACE, "the first takes half the leftover");
        assert_eq!(second_w, 194.0, "and so does the second");
        assert_eq!(size.width, 400.0, "the two fill the row");

        // 3:1 splits the same 388 dp into 291 and 97.
        let (_, three_x, one_w) = weighted_row(400.0, &[(3.0, true), (1.0, true)]);
        assert_eq!(three_x, 291.0 + BUTTON_GROUP_BETWEEN_SPACE);
        assert_eq!(one_w, 97.0);

        // An unweighted item keeps its natural 60 dp and the weighted one takes the rest.
        let (_, natural_x, weighted_w) = weighted_row(400.0, &[(0.0, false), (1.0, true)]);
        assert_eq!(natural_x, 60.0 + BUTTON_GROUP_BETWEEN_SPACE, "the natural item did not move");
        assert_eq!(weighted_w, 328.0, "400 - 60 - 12");

        // Nothing weighted: every item is natural, so the row is 120 + 12 wide.
        let (size, _, _) = weighted_row(400.0, &[(0.0, false), (0.0, false)]);
        assert_eq!(size.width, 120.0 + BUTTON_GROUP_BETWEEN_SPACE);
    }

    /// Lays out a group of `(weight, weighted)` items over a `width`-wide box. An unweighted or
    /// weighted-by-zero item is a 60 x 40 leaf, so a weighted one is visibly different from it.
    fn weighted_row(width: f32, spec: &[(f32, bool)]) -> (Size, f32, f32) {
        let spec = spec.to_vec();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let mut group = ButtonGroup::new();
            for (weight, weighted) in &spec {
                let w = *weight;
                if *weighted {
                    group = group.weighted_item(w, |ctx, _source| leaf(ctx, 60.0, 40.0));
                } else {
                    group = group.item(|ctx, _source| leaf(ctx, 60.0, 40.0));
                }
            }
            group.build(ctx);
        });
        composer.layout(Constraints::new(0.0, width, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let size = nodes[root].measured_size;
        let children = nodes[root].children.clone();
        (
            size,
            nodes[children[1]].position.x,
            nodes[children[1]].measured_size.width,
        )
    }

    /// The press growth — upstream's `Modifier.animateWidth` and its loop at
    /// `ButtonGroup.kt:729-770`. A middle item grows by `expandedRatio * width / 2` and takes
    /// that much OFF BOTH neighbours, so the row's total width does not move; the neighbours'
    /// `compressionLimit` (24 by default) is what caps it.
    #[test]
    fn a_pressed_item_grows_by_squeezing_its_neighbours() {
        // `update_animations` drives the process-wide table, so this takes the same lock the
        // other animation tests hold.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let middle = src.clone();
            composer.compose(move |ctx| {
                ButtonGroup::new()
                    .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                    .add(ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0)).press_source(middle))
                    .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let widths = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let children = nodes[root].children.clone();
            (
                nodes[children[0]].measured_size.width,
                nodes[children[1]].measured_size.width,
                nodes[children[2]].measured_size.width,
            )
        };

        build(&mut composer);
        assert_eq!(widths(&composer), (100.0, 100.0, 100.0), "at rest every item is natural");

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (left, middle, right) = widths(&composer);
        // `expandedRatio * 100 / 2` = 7.5, well under the 24 dp cap, taken off both neighbours.
        assert!(
            (middle - 115.0).abs() < 0.5,
            "the pressed middle item grows by 2 x 7.5, got {middle}"
        );
        assert!((left - 92.5).abs() < 0.5, "and its left neighbour gives up 7.5, got {left}");
        assert!((right - 92.5).abs() < 0.5, "so does its right neighbour, got {right}");
        assert!(
            (left + middle + right - 300.0).abs() < 0.5,
            "the row's total width must not move: it is a redistribution"
        );
    }

    /// The overflow: the indicator is measured first and the items fill what is left of the row,
    /// greedily, from the start — upstream's loop at `ButtonGroup.kt:695-711`. Three 100 dp items
    /// and a 40 dp indicator in a 340 dp row leave `340 - 40 - 12 = 288` for the items, which
    /// holds two of them (100, then 100 out of the 176 left) and not the third.
    #[test]
    fn the_overflow_fits_as_many_items_as_the_indicator_leaves_room_for() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ButtonGroup::new()
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .overflow_indicator(|ctx, _state| leaf(ctx, 40.0, 40.0))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 340.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        assert_eq!(children.len(), 4, "three items and the indicator");

        assert_eq!(nodes[children[0]].position.x, 0.0);
        assert_eq!(nodes[children[0]].measured_size.width, 100.0);
        assert_eq!(nodes[children[1]].position.x, 112.0, "the second item follows the first + 12");
        // The one that did not fit keeps its NATURAL size and is parked outside the row. A zero
        // size here is a feedback loop, not a tidier answer: the next measure folds the clean
        // node and reads that zero back as "it fits" — which is what the flicker test below, and
        // the weighted case in `a_parked_weighted_item_keeps_a_size`, are about.
        assert_eq!(
            nodes[children[2]].measured_size.width, 100.0,
            "the item that did not fit keeps its natural width"
        );
        assert!(
            nodes[children[2]].position.x >= 340.0,
            "and it is parked outside the row, got x={}",
            nodes[children[2]].position.x
        );
        assert_eq!(
            nodes[children[3]].position.x, 224.0,
            "and the indicator sits right after the last item that did fit"
        );
        assert_eq!(nodes[children[3]].measured_size.width, 40.0);
        assert_eq!(
            nodes[root].measured_size.width, 264.0,
            "the row is the two items, their gaps and the indicator"
        );
    }


    /// The menu's rows are picked by ITEM index out of a list that is INDEX-PAIRED with the
    /// items — the bug this pins.
    ///
    /// The rows used to be collected with `filter_map`, which COMPACTED them, so a cut taken at
    /// an item count indexed a SHORTER list: with only the LAST of three items given a menu the
    /// menu came out EMPTY and that item was unreachable, and with only the FIRST one given a
    /// menu it listed the item that WAS on screen.
    ///
    /// A unit test rather than an end-to-end one on purpose: a `State` write during composition
    /// is DEFERRED, so opening the menu the way a click does (flip the state, recompose) cannot
    /// be reproduced inside one composition, and a second composition of an unchanged group
    /// Skips. This pins the rule itself; the demo is the end-to-end case.
    #[test]
    fn the_menus_rows_are_picked_by_item_index() {
        // Only the THIRD item declared a menu, and the third is the one that did not fit.
        let mut rows = [None, None, Some("third")];
        assert_eq!(hidden_rows(&mut rows, 2), vec!["third"]);

        // Only the FIRST item declared a menu, and the first two fit: there is nothing hidden,
        // so the menu gets no row. The compacted form handed it the on-screen item instead.
        let mut rows = [Some("first"), None, None];
        assert!(
            hidden_rows(&mut rows, 2).is_empty(),
            "no item is hidden, so the menu has no row"
        );
        assert_eq!(
            hidden_rows(&mut rows, 0),
            vec!["first"],
            "and the same row IS the hidden one once nothing is visible"
        );

        // Every hidden item's row comes out, in item order, and each is taken once.
        let mut rows = [None, Some("b"), Some("c")];
        assert_eq!(hidden_rows(&mut rows, 1), vec!["b", "c"]);
        assert!(hidden_rows(&mut rows, 1).is_empty(), "the rows are consumed");
    }

    /// A parked item keeps a REAL size even when it is a WEIGHTED one.
    ///
    /// `natural` is what a parked item is placed at, and the pass that fills it used to skip
    /// weighted items — so one that did not fit was parked at the `0 x height` it was
    /// initialised with, the zero-width placement this policy's own note forbids (a later
    /// measure folds a clean node and reads that zero back).
    #[test]
    fn a_parked_weighted_item_keeps_a_size() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ButtonGroup::new()
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .weighted_item(1.0, |ctx, _source| leaf(ctx, 60.0, 40.0))
                .overflow_indicator(|ctx, _state| leaf(ctx, 40.0, 40.0))
                .build(ctx);
        });
        // 100 dp cannot hold the first item, so NOTHING fits and both items are parked.
        composer.layout(Constraints::new(0.0, 100.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        assert!(nodes[children[1]].position.x > 100.0, "the weighted item is parked outside");
        assert!(
            nodes[children[1]].measured_size.width > 0.0,
            "a parked item keeps its natural size, weighted or not, got {}",
            nodes[children[1]].measured_size.width
        );
    }

    /// The FLICKER guard: with no indicator the laid-out item count must not depend on the
    /// available width, swept one dp at a time.
    ///
    /// A weighted item's width is derived FROM the budget, so the widths plus the gaps sum to
    /// exactly it — `widths[i] <= remaining` is then decided by floating point on the last item,
    /// and the row dropped it at some widths and kept it at others. Resizing the window walked
    /// through those widths, so the last button blinked.
    #[test]
    fn a_weighted_row_without_an_indicator_never_drops_an_item() {
        let placed = |width: f32| -> usize {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                ButtonGroup::new()
                    .item(|ctx, _source| leaf(ctx, 82.0, 40.0))
                    .weighted_item(1.0, |ctx, _source| leaf(ctx, 60.0, 40.0))
                    .weighted_item(2.0, |ctx, _source| leaf(ctx, 60.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, width, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .filter(|&&c| nodes[c].measured_size.width > 0.0)
                .count()
        };

        // A fine sweep, because the boundary is a floating-point one.
        let mut width = 180.0f32;
        while width <= 512.0 {
            assert_eq!(
                placed(width),
                3,
                "the row dropped an item at {width} — that is the blink"
            );
            width += 1.0;
        }
    }

    /// Without an indicator the row never overflows: all three items are laid out even though they
    /// are wider than the box, which is what a plain Row does too.
    #[test]
    fn without_an_indicator_nothing_overflows() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ButtonGroup::new()
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 340.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        assert_eq!(children.len(), 3);
        for (i, &child) in children.iter().enumerate() {
            assert_eq!(nodes[child].measured_size.width, 100.0, "item {i} is still laid out");
        }
    }

    /// THE REGRESSION GUARD for the source identity: the source an item is handed must be the
    /// SAME one after a recomposition.
    ///
    /// It was not. The list of sources was grown from `remember(..).get()`, which hands back a
    /// clone — so pushing into it stored nothing, and every build created a fresh batch. The
    /// button got a different source each recomposition, which reset its press state on the spot:
    /// no state layer, no ripple, and a group that read `pressed = false` for ever. Pressing the
    /// source from the first build must be visible through the last one.
    #[test]
    fn the_source_an_item_gets_is_the_same_after_a_recomposition() {
        use std::sync::{Arc, Mutex};
        let seen: Arc<Mutex<Vec<MutableInteractionSource>>> = Arc::new(Mutex::new(Vec::new()));
        let mut composer = crate::runtime::composer::Composer::new();
        // The spacing is alternated so the group's own slot is DIRTY on the second pass and it
        // re-enters: a clean slot skips its closure, and the item would never be handed anything
        // twice to compare.
        let mut build = |composer: &mut crate::runtime::composer::Composer, spacing: f32| {
            let seen = seen.clone();
            composer.compose(move |ctx| {
                ButtonGroup::new()
                    .spacing(spacing)
                    .item(move |ctx, source| {
                        seen.lock().unwrap().push(source);
                        leaf(ctx, 100.0, 40.0);
                    })
                    .item(|ctx, _source| leaf(ctx, 100.0, 40.0))
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        build(&mut composer, 12.0);
        build(&mut composer, 13.0);

        let captured = seen.lock().unwrap().clone();
        assert!(
            captured.len() >= 2,
            "the item composed twice, so it was handed a source twice — got {}",
            captured.len()
        );
        captured[0].emit_press();
        assert!(
            captured[captured.len() - 1].state(true).pressed,
            "the later build's source must be the same one, not a fresh one"
        );
    }

    /// THE ROW'S WIDTH IS THE SUM OF ITS SLOTS, never of what the children reported.
    ///
    /// A button floors at its 48 dp touch target (`minimum_interactive_component_size`), so a
    /// neighbour that a press has squeezed REPORTS 48 for a narrower slot. Neither the row's
    /// width nor the node's may come from that number: taking the advance from it made the row
    /// GROW on every press — the demo's five icon buttons overflowed their own width and
    /// everything to the right of the pressed item moved — and leaving the node at the reported
    /// width made the squeezed item shrink about its CENTRE, so its leading edge drifted away
    /// from the item before it and opened a gap. Upstream never reads either: its positions come
    /// from `widths[]` and its layout size from `mainSpace` / `mainAxisLayoutSize`
    /// (`ButtonGroup.kt:688`, `:783`).
    #[test]
    fn the_row_keeps_its_width_when_a_squeezed_child_cannot_shrink() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                ButtonGroup::new()
                    .add(
                        ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                            .press_source(pressed.clone()),
                    )
                    // Floored at 48 by the SAME modifier a real button uses, so it cannot shrink
                    // and has no spare room to give.
                    .item(|ctx, _source| {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().minimum_interactive_component_size());
                        ctx.end_node();
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let widths = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let children = nodes[root].children.clone();
            (
                nodes[root].measured_size.width,
                nodes[children[0]].measured_size.width,
                nodes[children[1]].measured_size.width,
            )
        };

        build(&mut composer);
        assert_eq!(widths(&composer), (160.0, 100.0, 48.0), "at rest");

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }

        // The floored neighbour reports 48 whatever slot it is given — but its NODE is the slot it
        // was given (33 = 48 - 15), so it shrinks from its trailing edge rather than about its
        // centre, and the pressed item grows into the 115 it was given. The ROW is the sum of the
        // slots, so it does not move at all.
        assert_eq!(
            widths(&composer),
            (160.0, 115.0, 33.0),
            "the pressed item grows into its slot, the squeezed neighbour is the slot it was \
             given (not the 48 it reports), and the row stays the width it started at"
        );
    }

    /// The CONTENT of a squeezed item stays centred in the box the row paints.
    ///
    /// The node is the slot's width, but the child was measured — and its content centred — in
    /// the box the child REPORTED, 48 dp, its touch target. Narrowing the node then left the
    /// content off centre: measured in the demo, the heart inside a squeezed icon item sat 2.0 px
    /// right of its container. `place` shifts the subtree by half the difference.
    #[test]
    fn a_squeezed_items_content_follows_its_container_to_the_trailing_edge() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .add(
                                ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                                    .press_source(pressed.clone()),
                            )
                            // A real icon button: a 40 dp container inside the 48 dp touch
                            // target, which is the only shape whose content the modifier
                            // centres.
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path(
                                            "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z",
                                        )
                                        .size(24.0)
                                        .build(ctx);
                                    });
                            })
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (the item's width, where its content sits inside the item's own box). The content is the
        // node's child: the minimum-interactive modifier places it, and the row re-anchors it when
        // it squeezes the slot.
        let geometry = |composer: &crate::runtime::composer::Composer| -> (f32, f32, f32) {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let item = nodes[root].children[1];
            let glyph = nodes[item].children[0];
            (
                nodes[item].measured_size.width,
                nodes[glyph].position.x,
                nodes[glyph].measured_size.width,
            )
        };
        build(&mut composer);
        let (slot, glyph_x, glyph_w) = geometry(&composer);
        assert_eq!(slot, 48.0, "at rest the item is its touch target");
        assert_eq!(
            glyph_x + glyph_w / 2.0,
            slot / 2.0,
            "and its content is centred in it"
        );

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (slot, glyph_x, glyph_w) = geometry(&composer);
        assert!(slot < 48.0, "the press squeezed it, got {slot}");
        // The item AFTER the pressed one is its TRAILING neighbour, so it holds its trailing edge
        // and gives up width on the leading side: the container (and the glyph inside it) moves
        // towards the trailing edge instead of staying put — centring would have moved BOTH edges.
        //   slot 33, inset 4, container 25 wide at x = 33 - 25 - 4 = 4, glyph centred in it.
        assert_eq!(glyph_x, 4.5, "the content follows the container towards the trailing edge");
    }
    /// The painted box follows a press: the pressed item's container widens BOTH ways, and each
    /// neighbour keeps its OUTER edge and gives up width on the side that faces the press.
    ///
    /// This is the half a test of `measured_size` cannot see. The row's SLOTS were already right when
    /// the demo still showed nothing moving — the container is a separate box (`paint_rect` reads
    /// `minimum_interactive_painted`), so the previous test of the slots stayed green through it.
    #[test]
    fn a_press_widens_the_painted_box_and_holds_the_outer_edges() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                            .size(24.0)
                                            .build(ctx);
                                    });
                            })
                            .add(
                                ButtonGroupItem::new(|ctx, source| {
                                    crate::components::icon_button::IconButton::new()
                                        .interaction_source(source)
                                        .on_click(|| {})
                                        .build(ctx, |ctx| {
                                            crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                                .size(24.0)
                                                .build(ctx);
                                        });
                                })
                                .press_source(pressed.clone()),
                            )
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                            .size(24.0)
                                            .build(ctx);
                                    });
                            })
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (left, right) of every item's painted container, in the row's own coordinates — the node's
        // own position plus the box the row wrote on it.
        let boxes = |composer: &crate::runtime::composer::Composer| -> Vec<(f32, f32)> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            nodes[root]
                .children
                .iter()
                .map(|&c| {
                    let (left, _, w, _) = nodes[c]
                        .minimum_interactive_painted
                        .expect("an IconButton item publishes its painted box");
                    (nodes[c].position.x + left, nodes[c].position.x + left + w)
                })
                .collect()
        };

        build(&mut composer);
        let rest = boxes(&composer);
        assert_eq!(rest.len(), 3, "three items");
        for (l, r) in &rest {
            assert!(
                (r - l - 40.0).abs() < 0.01,
                "at rest each container is the declared 40 dp square, got {}",
                r - l
            );
        }

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let held = boxes(&composer);
        assert!(
            (held[0].0 - rest[0].0).abs() < 0.01,
            "the leading neighbour keeps its leading edge: {} -> {}",
            rest[0].0,
            held[0].0
        );
        assert!(
            held[0].1 < rest[0].1 - 1.0,
            "and gives up width on the trailing side: {} -> {}",
            rest[0].1,
            held[0].1
        );
        assert!(
            held[1].0 < rest[1].0 - 1.0 && held[1].1 > rest[1].1 + 1.0,
            "the pressed item grows BOTH ways: {:?} -> {:?}",
            rest[1],
            held[1]
        );
        assert!(
            (held[2].1 - rest[2].1).abs() < 0.01,
            "the trailing neighbour keeps its trailing edge: {} -> {}",
            rest[2].1,
            held[2].1
        );
        assert!(
            held[2].0 > rest[2].0 + 1.0,
            "and gives up width on the leading side: {} -> {}",
            rest[2].0,
            held[2].0
        );
    }

    /// A group WITH an overflow indicator wraps every item in a layer (`wrap_item`), so the node the
    /// row sees is that WRAPPER — which carries no size, publishes no declared box, and therefore made
    /// the whole painted-box step a no-op. The item inside then kept its own un-squeezed container and
    /// ran over its neighbour's slot. The steering has to reach through the wrapper to the node that
    /// owns the container.
    #[test]
    fn an_icon_item_in_an_overflow_group_is_steered_through_the_wrapper() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let pressed = src.clone();
            composer.compose(move |ctx| {
                crate::theme::WiniaTheme::with_theme(
                    crate::theme::ThemeColors::light_from_seed(0x6750A4),
                    ctx,
                    |ctx| {
                        ButtonGroup::new()
                            .add(
                                ButtonGroupItem::new(|ctx, _source| leaf(ctx, 100.0, 40.0))
                                    .press_source(pressed.clone()),
                            )
                            .item(|ctx, source| {
                                crate::components::icon_button::IconButton::new()
                                    .interaction_source(source)
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        crate::components::icon::Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z")
                                            .size(24.0)
                                            .build(ctx);
                                    });
                            })
                            .overflow_indicator(|ctx, _state| leaf(ctx, 40.0, 40.0))
                            .build(ctx);
                    },
                );
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // (the wrapper the row sees, the item inside it, that item's painted box)
        let wrapped = |composer: &crate::runtime::composer::Composer| -> Option<(f32, f32, f32)> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let wrapper = nodes[root].children[1];
            let inner = *nodes[wrapper].children.first()?;
            assert_ne!(wrapper, inner, "the row wrapped the item, so these are two nodes");
            let (left, _, w, h) = nodes[inner].minimum_interactive_painted?;
            Some((left, w, h))
        };

        build(&mut composer);
        let (left, w, h) = wrapped(&composer).expect("the wrapped item is steered too");
        assert_eq!(
            (left, w, h),
            (4.0, 40.0, 40.0),
            "at rest: the declared square, inset 4 inside its slot"
        );

        source.emit_press();
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        let (left, w, _) = wrapped(&composer).expect("still steered after the press");
        assert!(
            (w - 25.0).abs() < 0.5,
            "the squeezed container narrows with its slot, got {w}"
        );
        assert_eq!(left, 4.0, "and it holds its trailing edge: the press is on its leading side");
    }
}

