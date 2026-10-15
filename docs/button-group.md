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
stadium), `item_shapes`, weighted items, the press growth, and `ButtonGroup` — a row that spaces
its items by the token for the form it is in and is the token's height.

## Weighted items

`ButtonGroup::item(...)` takes a natural width; `weighted_item(weight, ...)` takes a proportional
share of what is left after the unweighted items have taken theirs. Upstream spells this
`Modifier.weight` inside the group (`ButtonGroup.kt:826`) and does the arithmetic in
`ButtonGroupMeasurePolicy` (`:610-650`); winia has no parent data for a modifier to carry the
weight, so it is a builder argument. The rule is upstream's:

```
remaining = max_width - sum(unweighted natural widths) - spacing * (count - 1)
each weighted item = remaining / totalWeight * its weight
```

A weighted item is measured at a FIXED width (min = max), which is what makes it fill its share
rather than shrink to its content.

## The press growth

`ButtonGroupItem::press_source(interaction_source)` is upstream's
`Modifier.animateWidth(interactionSource)`: the item declares the source it is pressed through, the
group animates the progress on its own `FastSpatial` spring, and the pressed item GROWS while its
neighbours give up the room — the row's total width does not move.

`ButtonGroupMeasurePolicy`'s loop (`ButtonGroup.kt:729-770`) is what this reproduces:

| where the pressed item is | how much it grows |
|---|---|
| middle | `value * min(expandedRatio * width / 2, limit(left), limit(right))`, taken off BOTH |
| first | `value * min(expandedRatio * width, limit(right))`, taken off the right |
| last | `value * min(expandedRatio * width, limit(left))`, taken off the left |

`limit` is the neighbour's `compressionLimit` — the most it will let a pressed sibling take off
it, which upstream defaults to the item's END content padding
(`ButtonGroup.kt:1044-1055`; `ButtonDefaults.ContentPadding`'s is 24 dp).
`ButtonGroupItem::compression_limit(f32)` overrides it.

The progress is a `State<f32>` read during MEASURE, so an animation frame re-measures without
recomposing — the same route the navigation rail's width animation takes.

## What is still open

- **The overflow indicator.** Upstream takes a required `overflowIndicator` slot and lays out
  TWO content sets — the items and the indicator — with a `ButtonGroupOverflowState` deciding
  how many items fit; the rest move into a `DropdownMenu`. winia has no multi-content layout yet,
  so the row never overflows.
- **The item constructors.** Upstream's scope offers `clickableItem` / `toggleableItem` /
  `customItem`; winia takes `ButtonGroupItem`s and leaves the button to the caller, so an item is
  a `Button` / `IconToggleButton` that the caller gives the right shape.
- **The distribution of the row's leftover space when nothing is weighted.** Upstream arranges
  through `Arrangement.Horizontal` (`Arrangement.spacedBy(12)` by default), which also decides
  what happens to space no item claims; winia lays the items out from the start edge.
