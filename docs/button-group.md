# ButtonGroup

material3's `ButtonGroup` (an M3 Expressive component) — a row of buttons that share their state
and, in the CONNECTED form, their container.

Source of truth: `androidx-main`'s `ButtonGroup.kt` plus
`tokens/ButtonGroupSmallTokens.kt` and `tokens/ConnectedButtonGroupSmallTokens.kt`. The numbers
below are the token files', not the spec's prose.

## Constants

| | value | token |
|---|---|---|
| container height | 40 | `ButtonGroupSmallTokens.ContainerHeight` (and `Connected...`, the two agree) |
| between-space, plain | 12 | `ButtonGroupSmallTokens.BetweenSpace` |
| between-space, connected | 2 | `ConnectedButtonGroupSmallTokens.BetweenSpace` |
| expanded ratio | 0.15 | `ButtonGroupDefaults.ExpandedRatio` |
| inner corner | 8 | `InnerCornerCornerSize` = `CornerValueSmall` |
| pressed inner corner | 4 | `PressedInnerCornerCornerSize` = `CornerValueExtraSmall` |

## The connected shape set

A connected group's items share ONE container, and the corners say where each item sits:

| item | resting | pressed | checked |
|---|---|---|---|
| leading (LTR) | start corners `CornerFull` (20), end corners 8 | start 20, end 4 | stadium |
| middle | 8 all round | 4 all round | stadium |
| trailing (LTR) | start corners 8, end corners `CornerFull` (20) | start 4, end 20 | stadium |

`ButtonGroupDefaults::item_shapes(index, count, direction)` picks the row by position, the way
`SegmentedButtonDefaults::item_shape` does, and the caller passes the direction because
`Shape::Corners` is geometric (see its own doc) — under RTL the START corners are the RIGHT ones.

`CornerFull` is a PERCENT corner (half the short side) and a composition does not know the box it
will be measured into, so the item height resolves it: `40 / 2 = 20`, exactly as
`SplitButtonDefaults::outer_corner_size` does it. The CHECKED shape keeps the percent form
(`Shape::Pill`) because the whole item takes it, so it stays a stadium whatever height the item
ends up.

`ButtonGroupShapes::shape_for(pressed, checked)` follows upstream's `shapeByInteraction`
precedence: pressed beats checked beats resting.

## What is implemented, and what is not

Implemented: the constants above, the connected shape set (including the RTL mirror and the checked
stadium), `item_shapes`, and `ButtonGroup` — a row that spaces its items by the token for
the form it is in and is the token's height.

NOT implemented, and it is the other half of upstream's `ButtonGroupMeasurePolicy`:

- **The overflow indicator.** Upstream takes a required `overflowIndicator` slot and lays out
  TWO content sets — the items and the indicator — with a `ButtonGroupOverflowState` deciding
  how many items fit; the rest move into a `DropdownMenu`. winia has no multi-content layout yet,
  so the row never overflows.
- **Weighted items.** `Modifier.weight` inside the group distributes the space left after the
  unweighted items have taken their intrinsic width.
- **The pressed-width expansion.** `Modifier.animateWidth(interactionSource)` grows the pressed
  item by `ExpandedRatio` and squeezes its neighbours, on the group's own
  `FastSpatial` animatable. The ratio constant is here; the behaviour is not.
- **The item constructors.** Upstream's scope offers `clickableItem` / `toggleableItem` /
  `customItem`; winia takes plain children and leaves the button to the caller, so an item is a
  `Button` / `IconToggleButton` that the caller gives the right shape.
