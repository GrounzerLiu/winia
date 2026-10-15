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
//! NOT IMPLEMENTED YET, and it is the other half of upstream's measure policy: the OVERFLOW
//! indicator (a second content set plus a `DropdownMenu` listing the items that did not fit) and
//! the pressed-width expansion (`Modifier.animateWidth`, `compressionLimit`). This module is
//! the token geometry and the row; both are recorded in `docs/button-group.md`.

use crate::composable;
use crate::graphics::Shape;
use crate::layout::node::{measure_node, LayoutNode, MeasurePolicy, Placement};
use crate::layout::{Constraints, LayoutDirection};
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

/// The row: items side by side, spaced by the group's token, all `BUTTON_GROUP_HEIGHT` tall.
///
/// The caller builds the items and hands each one the shape it should paint
/// (`ButtonGroupDefaults::item_shapes(i, count, direction)`), the way
/// `SingleChoiceSegmentedButtonRow` leaves `SegmentedButtonDefaults::item_shape` to its caller.
pub struct ButtonGroup {
    connected: bool,
    spacing: Option<f32>,
    /// `(weight, item)`. A weight of 0 means take your natural width — upstream expresses the
    /// same thing with `Modifier.weight`, which winia has no parent data for, so it is a builder
    /// argument here.
    children: Vec<(f32, Box<dyn FnOnce(&mut ComposeCtx)>)>,
    modifier: Modifier,
}

impl ButtonGroup {
    pub fn new() -> Self {
        Self { connected: false, spacing: None, children: Vec::new(), modifier: Modifier::new() }
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

    pub fn item(mut self, item: impl FnOnce(&mut ComposeCtx) + 'static) -> Self {
        self.children.push((0.0, Box::new(item)));
        self
    }

    /// An item that takes a PROPORTIONAL share of the space left after the unweighted items have
    /// taken their natural width. Upstream spells this `Modifier.weight` inside the group
    /// (`ButtonGroup.kt:826`) and divides the leftover by the total weight in its measure policy
    /// (`:610-650`) — that branch is what this reproduces: leftover / totalWeight * weight.
    pub fn weighted_item(
        mut self,
        weight: f32,
        item: impl FnOnce(&mut ComposeCtx) + 'static,
    ) -> Self {
        self.children.push((weight.max(0.0), Box::new(item)));
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
        let height = BUTTON_GROUP_HEIGHT;
        let key = ctx.next_key();
        let weights: Vec<f32> = self.children.iter().map(|(w, _)| *w).collect();
        let policy = ButtonGroupPolicy { spacing, height, weights };
        let modifier = self.modifier.height(height);
        let mut children = self.children;
        match ctx.start_restartable_group(key, modifier, policy) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                for (_, child) in children.drain(..) {
                    child(ctx);
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

/// Lays the items out in a row at a fixed height, spaced by the group's token, dividing the space
/// left after the unweighted items among the weighted ones.
///
/// Upstream does the same arithmetic in `ButtonGroupMeasurePolicy` (`:610-650`): the unweighted
/// items are probed at their intrinsic width, the weighted ones take
/// `(max - fixed - spacingTotal) / totalWeight * weight` at a FIXED width, and the whole row is
/// then arranged along the main axis. The overflow indicator and the pressed-width expansion live
/// in that same policy and are still open here.
#[derive(Debug)]
struct ButtonGroupPolicy {
    spacing: f32,
    height: f32,
    /// One entry per child; 0 means the item takes its natural width.
    weights: Vec<f32>,
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
        let gaps = if n > 1 { self.spacing * (n as f32 - 1.0) } else { 0.0 };
        let weight_of = |i: usize| self.weights.get(i).copied().unwrap_or(0.0);
        let total_weight: f32 = (0..n).map(weight_of).sum();

        // Pass 1: the unweighted items at their natural width. That is what the leftover is
        // measured against, and it is the width they are finally laid out at too (upstream probes
        // with `maxIntrinsicWidth` as well).
        let mut widths = vec![0.0f32; n];
        let mut fixed = 0.0f32;
        for i in 0..n {
            if weight_of(i) > 0.0 {
                continue;
            }
            let (size, _) = measure_node(
                nodes,
                policies,
                children[i],
                Constraints::new(0.0, max_w, 0.0, constraints.max_height),
            );
            widths[i] = size.width;
            fixed += size.width;
        }

        // Pass 2: the weighted items share what is left, proportionally.
        if total_weight > 0.0 {
            let remaining = (max_w - fixed - gaps).max(0.0);
            let unit = remaining / total_weight;
            for i in 0..n {
                let w = weight_of(i);
                if w > 0.0 {
                    widths[i] = unit * w;
                }
            }
        }

        // Pass 3: settle at those widths and place, left to right.
        let mut placements = Vec::with_capacity(n);
        let mut x = 0.0f32;
        let mut used_height = self.height;
        for i in 0..n {
            let (size, _) = measure_node(
                nodes,
                policies,
                children[i],
                Constraints::new(widths[i], widths[i], 0.0, constraints.max_height),
            );
            placements.push(Placement { size, position: Offset::new(x, 0.0) });
            used_height = used_height.max(size.height);
            x += size.width;
            if i + 1 < n {
                x += self.spacing;
            }
        }
        (Size::new(x.min(max_w), used_height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (i, &child) in children.iter().enumerate() {
            nodes[child].position = placements[i].position;
            nodes[child].measured_size = placements[i].size;
        }
    }
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
                    .item(|ctx| leaf(ctx, 60.0, 40.0))
                    .item(|ctx| leaf(ctx, 60.0, 40.0))
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
                    group = group.weighted_item(w, |ctx| leaf(ctx, 60.0, 40.0));
                } else {
                    group = group.item(|ctx| leaf(ctx, 60.0, 40.0));
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

    /// The spacing each form defaults to, and the override.
    #[test]
    fn the_spacing_follows_the_form() {
        assert_eq!(ButtonGroup::new().resolved_spacing(), BUTTON_GROUP_BETWEEN_SPACE);
        assert_eq!(ButtonGroup::connected().resolved_spacing(), CONNECTED_BUTTON_GROUP_BETWEEN_SPACE);
        assert_eq!(ButtonGroup::connected().spacing(7.0).resolved_spacing(), 7.0);
        assert!(ButtonGroup::connected().is_connected());
        assert!(!ButtonGroup::new().is_connected());
    }
}
