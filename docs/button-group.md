# ButtonGroup

material3's `ButtonGroup` (an M3 Expressive component) — a row of buttons that share their state
and, in the CONNECTED form, their container.

Source of truth: `androidx-main`'s `ButtonGroup.kt` plus
`tokens/ButtonGroupSmallTokens.kt` and `tokens/ConnectedButtonGroupSmallTokens.kt`. The numbers
below are the token files', not the spec's prose.

## Constants

| | value | token |
|---|---|---|
| container height (a BUTTON size, not the row's — see below) | 40 | `ButtonGroupSmallTokens.ContainerHeight` (and `Connected...`, the two agree) |
| between-space, plain | 12 | `ButtonGroupSmallTokens.BetweenSpace` |
| between-space, connected | 2 | `ConnectedButtonGroupSmallTokens.BetweenSpace` |
| expanded ratio | 0.15 | `ButtonGroupDefaults.ExpandedRatio` |
| inner corner | 8 | `InnerCornerCornerSize` = `CornerValueSmall` |
| pressed inner corner | 4 | `PressedInnerCornerCornerSize` = `CornerValueExtraSmall` |

### The row has no height of its own

`ContainerHeight` is a BUTTON size. `ButtonGroup.kt` never reads it — the token appears only in
`tokens/ButtonGroupSmallTokens.kt`, while the layout size is

```kotlin
val height = placeables.fastMaxBy { it.height }?.height ?: constraints.minHeight   // :795
```

so the row is as tall as its tallest item. This used to apply `Modifier.height(40)` to the whole
group, which SQUASHED anything taller: measured, an item declared 56 dp inside a 40 dp group came
back 40 dp tall. A group of `ButtonSize::Medium` buttons was unrepresentable.

`verticalAlignment` (upstream's parameter) decides where a SHORTER item sits on the cross axis —
`Start` by default, which is Compose's `Alignment.Top`. An item's own `ButtonGroupItem::align`
overrides it, the way `Modifier.align(Alignment.Vertical)` does upstream (`:799-802`, `:853-859`).

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
stadium), `item_shapes`, weighted items, the press growth, the overflow indicator with its menu,
`clickable` / `toggleable` items with their own overflow rows, and `ButtonGroup` — a row that
spaces its items by the token for the form it is in, is as tall as its tallest item, honours the
incoming minimum width, mirrors under RTL and distributes its leftover space through
`horizontalArrangement`.

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

THE ROW'S WIDTH IS THE SUM OF ITS SLOTS. The growth pass keeps `widths[]` at a constant sum,
and both the placement advance and the row's own size come from those slots — never from what
a child reports back. This matters because a child can refuse to shrink: a button floors at its
48 dp touch target (`minimum_interactive_component_size`), so a squeezed icon item reports 48
for a 41 dp slot. Advancing by the reported size made the row GROW on every press (the demo's
five-icon row went 288 -> 295) and shoved everything to the right of the pressed item along.
Upstream never has this problem: its positions come from `widths[]` and its layout size from
`mainSpace` / `mainAxisLayoutSize` (`ButtonGroup.kt:688`, `:783`), with the placeables' own
sizes never entering either. (`desiredWidth` at `:682` is only the fits-everything test — and in
the overflow path it still counts the items that were hidden, so it is NOT the row's size.)

Measured on the demo's five-icon row (items 48 dp, 12 dp apart, row 288): pressing the FIRST
one gives the slots **55 / 41 / 48 / 48 / 48** — they sum to 240, which with the four 12 dp gaps
is the 288 the row keeps. The second item still REPORTS 48 (it floors at its touch target while
sitting in a 41 dp slot), which is exactly why the layout must not read that number back: the
pressed item grows into its own slot, its neighbour starts 7 dp later, and the three after it do
not move at all.

`limit` is the neighbour's `compressionLimit` — the most it will let a pressed sibling take off
it, which upstream defaults to the item's END content padding
(`ButtonGroup.kt:1044-1055`; `ButtonDefaults.ContentPadding`'s is 24 dp).
`ButtonGroupItem::compression_limit(f32)` overrides it.

The progress is a `State<f32>` read during MEASURE, so an animation frame re-measures without
recomposing — the same route the navigation rail's width animation takes.

## The overflow

The indicator upstream ships as the default is a `FilledIconButton` showing
`Icons.Filled.MoreVert`, and its click TOGGLES the menu (`ButtonGroup.kt:296-336`):

```kotlin
FilledIconButton(
    onClick = { if (menuState.isShowing) menuState.dismiss() else menuState.show() },
    shape = IconButtonDefaults.filledShape,   // a CIRCLE
) { Icon(Icons.Filled.MoreVert, ...) }         // a VERTICAL ellipsis
```

So it is a circle, not a pill, and the dots are stacked vertically. The demo builds it from
`IconButton` + `IconButtonStyle::Filled` (whose Round family is `Shape::Circle`) and the
`more_vert` path.

Upstream puts it in a `TooltipBox` reading `Strings.ButtonGroupMoreOptions` ("More options"),
anchored above it. winia's `Tooltip` is the same thing with the same defaults — hover-triggered,
`TopCenter`, 8 dp of offset — so the demo wraps the indicator in one.


`ButtonGroup::overflow_indicator(|ctx, menu_state| ...)` is upstream's required
`overflowIndicator` slot, except winia makes it optional: a group without one simply never
overflows.

The layout reproduces upstream's (`ButtonGroup.kt:682-711`):

0. With NO indicator there is no fitting at all: every item is laid out however narrow the row
   is, which is what a plain Row does. Fitting without one is not just pointless, it is wrong —
   a WEIGHTED item's width is derived from the budget, so the widths plus the gaps sum to exactly
   it and `widths[i] <= remaining` becomes a floating-point coin flip on the last item. The row
   dropped it at some widths and kept it at others, so it blinked while the window was resized.
1. `desiredWidth` (every item plus the gaps) is compared against the row BEFORE the indicator is
   measured. If it fits — or the row is unbounded, as inside a scroll — every item is shown and the
   indicator is not shown at all, never measured into the layout.
2. Only once that test FAILS is the indicator measured; it takes its width first, and the items
   then fill what is left greedily, from the start, until one does not fit.
3. The ones that did not fit are parked OUTSIDE the row at their NATURAL size (`max_w + 1000`)
   and are in the menu instead — never at zero width, and never at the origin — and the
   indicator is placed one spacing after the last item that did fit.

Step 1 is the one that used to be missing, and it was visible: the indicator was measured first and
placed unconditionally, so a group that called `overflow_indicator()` wore a "more" button with
nothing to hide. Three 100 dp items and a 40 dp indicator fit in a 340 dp row (324) and still came
back 264 dp wide with the indicator at x=224.

Upstream expresses the two content sets as a `MultiContentMeasurePolicy`. winia has no
multi-content layout, so the indicator is the group's LAST child and the policy knows how many
children are items. The menu's own anchor is a further child the policy places where the indicator
sits, so the menu drops from the indicator rather than from the start of the row.

**A hidden item still has a subtree.** winia's renderer draws a hidden node's children, so
parking one off-screen is not enough on its own — it would still paint, over the items that did
fit. (Upstream
never measures the hidden ones at all, which is why it does not have to think about this.) Each
item is therefore wrapped in its own group node carrying a `graphics_layer` whose alpha reads the
count back and is 0 for the ones in the menu. The alpha is evaluated at RENDER time, so it is last
measure's answer. A group with no indicator does not pay for the wrapper.

How many items fit is decided in MEASURE, so the count comes back through a `Backchannel` — the
write-only handle the measure/layout write-back path exists for — and the menu is composed from
it on the next composition, which is also when the indicator's click has just shown it. Upstream
has the same one-pass lag in `ButtonGroupOverflowState`.

`ButtonGroupMenuState` is `show` / `dismiss` / `is_showing` / `expanded_state`, and
`ButtonGroupItem::menu(...)` is the row an item shows when it does not fit — upstream's
`menuContent`, and deliberately `Fn` rather than `FnOnce` because a menu is an overlay that
recomposes every frame it is up.

## The label is single-line, and that is the ITEM's rule

An item's label must not wrap. Upstream's `ClickableButtonGroupItem` says so explicitly
(`ButtonGroup.kt:1118-1124`):

```kotlin
Text(text = label, maxLines = 1, softWrap = false, overflow = TextOverflow.Visible)
```

`Button` does NOT force it — not in material3 and not here. A wrapped label makes the item taller
than its siblings (the row takes the tallest item), and because a pressed item narrows its
neighbours, a wrapping label would reflow in the middle of a press. `ButtonGroupItem::clickable` /
`toggleable` set it for the caller; a caller composing its own item sets `max_lines(1)` and
`soft_wrap(false)` on its label itself.

`clickable` / `toggleable` also set `overflow(TextOverflow::Visible)` and `align(TextAlign::Center)`,
which is the second half of what material3 asks for: an item that a press has narrowed is narrower
than its own label, and the label must neither be cut nor jump off-centre while that happens.
`TextOverflow::Visible` is new (`winia/src/text/style.rs`): the paragraph is laid out at its
intrinsic width and the paint path no longer re-lays it out at the node's width, which is what used
to cut it, and the centring offset is allowed to go negative so an overflowing label still centres.

## The item constructors

Upstream's scope offers three (`ButtonGroup.kt:870-907`); winia mirrors them as constructors on
`ButtonGroupItem`:

| upstream | winia |
|---|---|
| `clickableItem(onClick, label, icon, weight, enabled)` | `ButtonGroupItem::clickable(label, on_click)` + `.icon(…)` `.weight(…)` `.enabled(…)` |
| `toggleableItem(checked, label, onCheckedChange, icon, weight, enabled)` | `ButtonGroupItem::toggleable(label, checked, on_checked_change)` + the same modifiers |
| `customItem(buttonGroupContent, menuContent)` | `ButtonGroupItem::new(closure)` + `.menu(…)` |

The labelled forms build their own `Button` — padding from `ButtonDefaults`, the single-line label
above, and a `DropdownMenuItem` overflow row that runs the action and then dismisses the menu
(`ButtonGroup.kt:1131-1142`, `:1205-1216`). A toggled item paints `primary` while checked and
`surfaceContainer` while not (`FilledButtonTokens.SelectedContainerColor` /
`UnselectedContainerColor`).

`ButtonGroupDefaults::overflow_indicator` is upstream's default indicator: a `FilledIconButton`
showing `Icons.Filled.MoreVert` whose click TOGGLES the menu, inside a tooltip reading "More
options" (`ButtonGroup.kt:294-333`). Pass it straight to `overflow_indicator(…)`.

## What is still open

- **Saved overflow state.** Upstream's `ButtonGroupOverflowState` is a `rememberSaveable`, so the
  visible/​total counts survive a configuration change; winia keeps them in a `Backchannel`, which
  is not saved.
- **`Arrangement.spacedBy(space, alignment)`.** winia keeps the gap on the group (`spacing`) and
  takes only the distribution from `arrangement`, so the per-arrangement alignment half of
  `spacedBy` is expressed as `Arrangement::Aligned` instead.
- **The menu items' styling.** Upstream's menu rows are material3 `DropdownMenuItem`s rendered by
  the item itself; winia's labelled items do the same, but a custom item still renders whatever its
  `menu(…)` closure composes.
