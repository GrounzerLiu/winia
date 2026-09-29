# Intrinsic measurement: Compose's contract, winia's gap, and the cost of closing it

Research note, now also the implementation record. §1-§4 establish what Compose's
intrinsic-measurement contract actually requires and what winia had and lacked when the work started;
§5-§6 are the options and the recommendation as they were written before the decision; §7 lists what
stayed unverified; §8 records option A as it landed (the framework change is there, not here).

The trigger is concrete: two winia components (`DropdownMenu`, `SegmentedButton`) already hand-roll
the same two-pass measurement that `Modifier.width(IntrinsicSize.Max)` expresses in one modifier, and a
third (`TabRow`) approximates it.

## 1. Compose's contract

### 1.1 The modifier

`target/compose-src/commonMain/androidx/compose/foundation/layout/Intrinsic.kt`:

- `enum class IntrinsicSize { Min, Max }` (`Intrinsic.kt:143`).
- Four modifiers build on it: `Modifier.width(IntrinsicSize)` (`:51`), `height` (`:80`),
  `requiredWidth` (`:105`), `requiredHeight` (`:130`). The first two pass `enforceIncoming = true`
  (`:55`, `:110`); the `required` pair passes `false`.
- What the node does at measure time (`IntrinsicWidthNode.calculateContentConstraints`, `:176-190`):
  it asks the child for `measurable.minIntrinsicWidth(constraints.maxHeight)` (for
  `IntrinsicSize.Min`) or `maxIntrinsicWidth(constraints.maxHeight)` (for `Max`), clamps a negative
  answer to 0, and returns `Constraints.fixedWidth(measuredWidth)`. `IntrinsicHeightNode` mirrors it
  with `minIntrinsicHeight(constraints.maxWidth)` / `maxIntrinsicHeight(constraints.maxWidth)`
  (`:237-245`). The constraint for the *queried* axis is tight; the other axis keeps its incoming
  value.
- The node also answers intrinsic queries itself (`:253-265`): **both** `minIntrinsicWidth` and
  `maxIntrinsicWidth` are normalised through `IntrinsicSize.Min/Max` and forwarded as that one
  intrinsic, i.e. this node reports a *fixed* width to its parent's intrinsic query.
- Documented semantics (`:34-49`): "Declare the preferred width of the content to be the same as the
  min or max intrinsic width of the content. **The incoming measurement Constraints may override
  this value**" — which is why `enforceIncoming` exists: `measure` constrains the content
  constraints against the incoming ones (`:281-290`), so `width(IntrinsicSize.Max)` is a *preference*
  and `requiredWidth` is a hard size.

### 1.2 The protocol

In `androidx.compose.ui.layout` (not part of the extracted `target/compose-src`, so this was checked
against the published source and API reference):

- `MeasurePolicy` is a `fun interface` whose four intrinsic functions all have default
  implementations. The KDoc says they "have default implementations that make a best effort attempt
  to calculate the intrinsic measurements by **reusing the [measure] method**. Note this will not be
  correct for all layouts, but can be a convenient approximation."
- The default body of e.g. `maxIntrinsicWidth(measurables, height)` maps the children with
  `DefaultIntrinsicMeasurable(it, IntrinsicMinMax.Max, IntrinsicWidthHeight.Width)`, builds
  `Constraints(maxHeight = height)`, runs the policy's own `measure` inside an `IntrinsicsMeasureScope`
  and returns `layoutResult.width`. `maxIntrinsicHeight` is the mirror image
  (`Constraints(maxWidth = width)` → `.height`).
- The API reference states the same: "When creating a custom Layout or layout modifier, intrinsic
  measurements are calculated automatically based on approximations. Therefore, the calculations
  might not be correct for all layouts", and lists the four functions to override for exact answers.
- Why the mechanism exists at all: "you should only measure your children once; measuring children
  twice throws a runtime exception … Intrinsics lets you query children before they're actually
  measured." The intrinsic pass is therefore a *separate query channel* rather than a real second
  measure of the same node.

So the contract has two halves: a **query** side (each node answers min/max intrinsic on a given
axis) and a **consumer** side (`width`/`height(IntrinsicSize)` turn one answer into a tight
constraint). The default gives every policy a usable answer without any work; layouts that know
better override.

### 1.3 Row/Column override the default

`Column.kt:225-263` forwards all four functions to
`IntrinsicMeasureBlocks.Vertical{MinWidth,MinHeight,MaxWidth,MaxHeight}` (horizontal variants for
`Row`), implemented in `foundation/layout/RowColumnImpl.kt:261-369`:

- `intrinsicMainAxisSize` (`:371-394`): unweighted children contribute `mainAxisSize(∞)`; weighted
  children contribute through `weightUnitSpace = max(size / weight)`, so the result is
  `weightUnitSpace * totalWeight + fixedSpace + (n - 1) * mainAxisSpacing`.
- `intrinsicCrossAxisSize` (`:396-452`): first walk unweighted children, tracking
  `mainAxisSpace = min(child.mainAxisSize(∞), remaining)` and
  `crossAxisMax = max(crossAxisMax, child.crossAxisSize(mainAxisSpace))`; then distribute
  `weightUnitSpace = (mainAxisAvailable - fixedSpace) / totalWeight` to the weighted children and ask
  their cross size with that main-axis budget.
- `weight` is **parent data** (`LayoutWeightNode : ParentDataModifierNode`, `:485-492`), not a
  modifier on the child itself.

### 1.4 Who uses it

`Modifier.width/height(IntrinsicSize)` appears 35 times in the extracted sources. The layout sites:

| Site | Use |
|---|---|
| `material3/Menu.kt:411` | `Column(width(IntrinsicSize.Max))` — the menu is as wide as its widest item |
| `material3/SegmentedButton.kt:338`, `:373`, `:399` | `width(IntrinsicSize.Min)` (strip shrinks to content) and `height(IntrinsicSize.Min)` |
| `material/Chip.kt:212`, `material/Menu.kt:215` | same pattern, M2 |
| `foundation/contextmenu/ContextMenuUi.kt:163` | same |

Components that implement the protocol directly instead:

- M3: `AppBar.kt:3136-3157`, `Chip.kt:2297-2313`, `NavigationItem.kt:627-828`,
  `ListItem.kt:333-380` (`leadingMeasurable.firstOrNull()?.minIntrinsicWidth(constraints.maxHeight) ?: 0`),
  `OutlinedTextField.kt:1005-1088`, `TextField.kt:1027-1109`.
- Foundation: `Scroll.kt:478-500` (`measurable.minIntrinsicWidth(if (isVertical) Constraints.Infinity else height)`),
  `Size.kt:869-917` (`constraints.constrainWidth(measurable.minIntrinsicWidth(childHeight))`),
  `AspectRatio.kt:123-154`, `BasicMarquee.kt:299-317`, `FlowLayout.kt:736-802`, `Grid.kt:1743`, `:1803`
  (auto tracks call `minIntrinsicWidth(Constraints.Infinity)`), `TextStringSimpleNode.kt:427-445`.

`foundation/layout/Padding.kt` contains no intrinsic code at all: padding is expressed by offsetting
the constraints at measure time, so it never appears in the intrinsic layer.

## 2. winia today

### 2.1 Leaf intrinsics exist

- Text: `winia/src/text/paragraph.rs:156-161` `min_intrinsic_width()` / `max_intrinsic_width()`
  (forwarded to the skia paragraph).
- Images / icons: `winia/src/modifier.rs:1719` `image_intrinsic_size()`,
  `winia/src/ui/icon.rs:215` `IconSource::intrinsic_size()` (`:396` `file_intrinsic_size`).
- Consumers read them directly, at leaf level only: `app.rs:1795`, `app.rs:5061-5062`,
  `render.rs:1285-1286`, `render.rs:2034`, `render.rs:2281-2282`, `ui/draw_scope.rs:217`.

### 2.2 There is no protocol and no `IntrinsicSize` API

- `winia/src/layout/node.rs:767-792` is the whole trait: `measure` (`:772`), `place` (`:781`),
  `subcomposes` (`:789`, default `false`). A container cannot ask "how wide would you be given
  height h", and cannot be asked either.
- The modifier chain can only answer *constraints*, never *content*: `resolved_size`, `fixed_size`,
  `min_size_constraint`, `max_size_constraint`, `required_size_constraint`,
  `get_padding_horizontal/vertical`, `is_fill_max_width/height`, `get_layout_weight`,
  `aspect_ratio_constraint` (inventory: `docs/render_modifier_analysis.md:235`).
- Nothing in `winia/src/modifier.rs` corresponds to `Modifier.width(IntrinsicSize.Max)`.

### 2.3 Three components work around it by hand

1. **`DropdownMenu`** — `MenuColumnPolicy`, `winia/src/ui/overlay.rs:812-878`:
   - pass 1 measures each item's **content** (not the item) with
     `Constraints::new(0.0, f32::MAX, 0.0, f32::MAX)` (`overlay.rs:831-838`), adds the item's own
     horizontal padding and clamps to `DROPDOWN_ITEM_MIN_WIDTH = 112.0` /
     `DROPDOWN_ITEM_MAX_WIDTH = 280.0` (`overlay.rs:794-795`, used at `:840`, `:845`);
   - pass 2 re-measures every item at that width (`Constraints::new(width, width, 0.0, f32::MAX)`) and
     stacks them.
   - The comment at `overlay.rs:804-808` records why the obvious version fails: "Measuring the item
     itself would not do — its label is `weight(1f)` … an unbounded pass reports the constraint back
     instead of the content (measured: the menu went from 112 to the 280 maximum the moment the label
     was weighted)". This is Compose's `Column(width(IntrinsicSize.Max))` (`Menu.kt:411`).
2. **`SegmentedButton`** — `SegmentedRowPolicy`, `winia/src/ui/segmented_button.rs:286-345`: pass 1
   measures every item with `Constraints::new(MIN_WIDTH, avail, 0.0, f32::MAX)` for the widest
   (`:309-318`), computes `fit = (avail + (n - 1) * overlap) / n` and `item_w = natural.min(fit)`
   (`:322-323`), then re-measures tightly at `(item_w, item_w, height, height)` (`:328-333`). Its own
   doc comment (`:278-280`) names the Compose equivalent: "`Arrangement.spacedBy(-space)` and
   `weight(1f)` inside a row sized to `IntrinsicSize.Min`".
3. **`TabRow`** — approximation from the first pass, `winia/src/ui/tab_row.rs:22`: "no
   `maxIntrinsicWidth` call, uses the 1st pass measurement instead".

`winia/src/layout/flow.rs:11` states the absence outright ("no intrinsics — winia has no intrinsic
system").

### 2.4 The gap is already recorded in the docs

| Document | Line | Text |
|---|---|---|
| `docs/dropdown-menu.md` | `:308` | framework row: "intrinsic measurement \| `IntrinsicSize.Max/Min` \| none (the menu implements it itself with `MenuColumnPolicy`; other components have to do the same)" |
| `docs/dropdown-menu.md` | `:172-178` | how stage 2b hit the problem |
| `docs/segmented-button-plan.md` | `:147` | "`IntrinsicSize.Min` \| **Not a gap** \| Our row is content-sized the same way … General intrinsic measurement (a measure protocol + second pass) is its own project and this component does not need it." |
| `docs/segmented-button.md` | `:64`, `:89`, `:121` | the same deviation, component-local |

The `segmented-button-plan.md` verdict stands for that component (it got the geometry another way);
this note is about the framework-level item that verdict defers.

## 3. Why this is not a mechanical port: four hard problems

**(a) A naive intrinsic pass poisons the measure cache.**
`measure_node` folds on `!dirty && !layout_dirty && cached_constraints == Some(constraints)`
(`node.rs:2351` wrapper, `:2375` inner) and a real measurement writes back
`dirty = false; layout_dirty = false; cached_constraints = Some(constraints)` (`node.rs:2796-2798`).
Probing a node by calling `measure_node` with probe constraints therefore (i) leaves the node's cache
holding the *probe* constraints, so the following real measure is either a spurious cache hit or an
extra pass, and (ii) trips the per-measure bookkeeping around it — `swap_measuring_node` and the
`subcomposition_count()` comparison that sets `nodes[idx].subcomposed` (`node.rs:2354-2359`), plus
whatever the current measuring node was before the probe. An intrinsic pass must be a separate entry
point that reads the modifier chain and the child policies **without** writing `measured_size`,
`cached_constraints`, `dirty`, `layout_dirty` or `subcomposed` — the analogue of Compose's distinct
`IntrinsicMeasureScope` receiver.

**(b) The modifier chain has to be inverted, not read.**
The intrinsic answer for a node is its content's answer adjusted by everything `measure_node_inner`
does *before* the policy call, in pipeline order: `resolved_size()` (`:2414-2417`, tighten),
`layout_nodes()` transforms (`:2425-2433`), `min_size_constraint()` (`:2438-2444`),
`max_size_constraint()` (`:2456-2464`), `required_size_constraint()` (`:2470-2479`, overwrites
min/max, the `enforceIncoming = false` analogue), `fixed_size()` (`:2482-2497`), padding
(`:2500-2506` offsets the constraints; `:2687` adds the padding back to the size), `is_fill_max_*`
(`:2509-2518`, only relevant when that axis is bounded), scroll unbinding (`:2521-2564`), flight
override (`:2571-2584`). That order *is* the specification: Compose encodes the same facts in
`Size.kt:869-917` (`constraints.constrainWidth(measurable.minIntrinsicWidth(childHeight))`) and in the
`Intrinsic.kt` nodes. winia would need one walk that applies the same queries in the same order.

**(c) Leaves are heterogeneous.**
Text: intrinsic is width-based and depends on the layout width the paragraph was measured at
(`paragraph.rs:156-161`); answering an *intrinsic width for a given height* needs a height-parameterised
text pass that winia does not have — its text measurement is width-driven
(`measure_and_cache_text(&nodes[idx], layout_width)`, `node.rs:2711`). Images/icons report a fixed
intrinsic (`node.rs:2733-2737`). A plain leaf reports its fixed min or 0 (`node.rs:2738-2755`) and then
gets its padding added back (`:2760`). Whatever the protocol is, these three cases have to be defined
per leaf kind.

**(d) Weight and subcompose.**
Compose keeps `weight` as parent data and reads it in the intrinsic block (`RowColumnImpl.kt:408`,
`:437`). winia keeps it on the child's own modifier (`modifier.get_layout_weight()`, `flex.rs:155`)
and consumes it in `flex.rs` phase 2 (`:202-223`, `allocated = remaining * w / total_weight`), with
phase 1 for unweighted children (`:174-192`) and the final size at `:226-253`
(`A::constrain_main(constraints, total_content_main)`). An intrinsic block that is not the same code
path as those phases reproduces exactly the trap `MenuColumnPolicy` records: a weighted child hands
back whatever maximum it is given, so the "natural width" of a weighted label is the constraint, not
the text. Separately, a policy whose content only exists once composed (`subcomposes()`,
`node.rs:789`; `ui::subcompose`) cannot answer an intrinsic without composing — the same structural
limit already documented for LazyLayout.

## 4. The mechanism that makes this worth doing

The reason a weighted child has a *finite* intrinsic in Compose is not the approximation default — it
is the `weightUnitSpace` arithmetic in `RowColumnImpl.kt:371-394`. That is the missing piece for
winia's menu:

- The menu item's label is `weight(1f)` inside the item's `Row`. Asking the *item* for its
  `maxIntrinsicWidth(h)` would go through that `Row`'s intrinsic block, which computes
  `weightUnitSpace = max(label natural width / 1)` — a finite number — and returns
  icons + gaps + label width. The menu column's own intrinsic is then the max over items, which is
  precisely what `MenuColumnPolicy` pass 1 computes today by measuring the item's *content*.
- So implementing the Row/Column intrinsic blocks makes
  `Column(width(IntrinsicSize.Max))` (`Menu.kt:411`) reproduce `MenuColumnPolicy` — and the
  workaround can be deleted rather than kept in sync by hand.

Predicted acceptance evidence (to be checked when implementing, not assumed): the existing menu width
assertions must not move — the icon menu is 112 dp wide and the demo's "Copy / Ctrl+C" menu is 136 dp
wide (< the 280 dp cap), with all items equal width.

## 5. Options

| | What it is | Cost | What it buys | What it does not |
|---|---|---|---|---|
| **A. Full port** | Intrinsic entry point + modifier-chain inversion + intrinsic blocks for Row/Column/Box/Scroll + the four `MeasurePolicy` defaults + a public `Modifier.width/height(IntrinsicSize)` (`IntrinsicSize` enum, `enforceIncoming`) | Large: new measure-side query protocol, a new modifier element pair, per-container overrides, and a second measurement protocol to keep correct for all ~40 existing policies | User code can write Compose's `Row(Modifier.height(IntrinsicSize.Min))`; M3-derived components (`Chip`, `ListItem`, `TextField`, `AppBar`) stop needing local two-pass code; `MenuColumnPolicy` / `SegmentedRowPolicy` go away | Intrinsics are not a fix for LazyLayout-style measure-time composition, and the defaults stay approximations for policies that do not override |
| **B. Container side only** | Same entry point and modifier-chain inversion, intrinsic blocks for Row/Column/Box/Scroll, but no user-facing `IntrinsicSize` modifier: components call a free function (or a small `Modifier`-internal element) | Medium: entry point + inversion + 2-3 container blocks | Deletes both hand-rolled passes; one implementation of "natural size" instead of three; no new public API to freeze | Third-party/custom layouts cannot *express* an intrinsic request in a modifier chain; still no `width(IntrinsicSize.Max)` for user code |
| **C. Status quo + shared helper** | Leave the protocol out; factor the duplicated pass-1 logic into one documented helper that the menu and segmented row call; keep the deviation in the docs | Small (one helper + doc) | Removes the copy-paste; keeps the documented deviation honest | Nothing structural: every new container-shaped component still needs its own two-pass policy, and the docs keep growing |

Notes on cost that are *not* line-count guesses: adding four default methods to `MeasurePolicy` is
source-compatible with every existing policy; the risky part is (b), because an intrinsic walk that
disagrees with the measure pipeline in any single step produces sizes that look plausible and are
wrong (the `MenuColumnPolicy` comment is a live example of that class of bug).

## 6. Recommendation

Target **B**, staged, with C only if the user wants a smaller first step:

1. **Entry point.** `pub(crate) fn intrinsic_width(nodes, policies, idx, height) -> f32` and
   `intrinsic_height(nodes, policies, idx, width) -> f32` in `winia/src/layout/node.rs`, with a
   non-caching measure path (a probe flag that suppresses the `:2796-2798` write-back and the
   `:2354-2359` subcomposition bookkeeping). Default answer per node kind: modifier chain first
   (`fixed_size` / `required_size_constraint` → that size; padding → add; `fill_max` → clamp to the
   bounded axis; scroll → forward with the scrolled axis unbounded), then the policy.
2. **Policy protocol.** Four functions on `MeasurePolicy`, defaulted to Compose's approximation
   ("measure with the queried axis unbounded"), so existing policies keep working unchanged.
3. **Container blocks.** Row/Column (`flex.rs`) intrinsic blocks implementing
   `intrinsicMainAxisSize` / `intrinsicCrossAxisSize` semantics, sharing the weight arithmetic with
   phases 1-2; Box (max over children); scroll containers (forward with the scrolled axis unbounded,
   mirroring `Scroll.kt:478-500`), and lazy containers answered conservatively (to be decided, §7).
4. **Consumers.** Rewrite `MenuColumnPolicy` (`overlay.rs`) and `SegmentedRowPolicy`
   (`segmented_button.rs`) on top of the protocol, then delete the hand-rolled passes.
5. **Acceptance.** The existing suite must not move: menu widths 112 / 136 dp and equal item widths
   (`dropdown_menu_geometry_matches_the_material3_metrics`, the icon-slot test), segmented strip
   geometry, plus new lib tests for the protocol itself — one test per pipeline step in (b), each with
   the "turn the step off and it must go red" check the repo already applies to behaviour fixes.

Option A's public modifier can be layered on later on top of B without changing the protocol.

## 7. Open questions / unverified

- **Lazy containers.** What Compose answers for `LazyColumn`'s intrinsic width/height was not
  verified; winia's answer would have to come from the item-height estimate, and the honest option may
  be "report the estimate" or "refuse".
- **Height-parameterised text.** `paragraph.rs` exposes `min/max_intrinsic_width()` but winia has no
  measurement path keyed on height; whether skia can answer it cheaply for the current layout width
  needs a probe before step 1 is called done.
- **Box.** No verification was done on which Box-shaped policies need an override versus the
  approximation; `Box` is first-wins in winia (`box-with-constraints` docs), so the default may
  suffice.
- **Query forwarding through `fill_max`/`min_size`** is defined by analogy with Compose's
  `Size.kt:869-917`; there is no local source in `target/compose-src` for the `SizeNode` chain, so the
  exact ordering of "clamp vs tighten" for a queried axis should be re-checked against the published
  `androidx.compose.ui.layout` source while implementing.
- **Test isolation:** intrinsic probes must not leave nodes dirty or caches warm; a regression test for
  that belongs in step 1 (measure twice, compare tree/layout results).

## 8. What was implemented (option A)

Chosen by the user: option A, on branch `intrinsic-size`. This section is the record of the port as it
landed; §1-§4 stay the specification it was written against.

### 8.1 The public modifier API (`winia/src/modifier.rs`)

- `pub enum IntrinsicSize { Min, Max }` — Compose's `Intrinsic.kt:143`.
- `SizeValue::Intrinsic(IntrinsicSize)`, so `Modifier::width(IntrinsicSize::Max)` /
  `height(..)` / `required_width(..)` / `required_height(..)` all take `impl Into<SizeValue>` and stay
  source-compatible with the `f32`/`Dp`/`State<f32>` forms they already accepted.
- `ModifierElement::RequiredSize` now stores `Option<SizeValue>` (it was `Option<f32>`), and
  `required_size_constraint()` resolves numbers only: an intrinsic request carries no number and is
  answered by the measure pipeline instead.
- New queries `Modifier::intrinsic_width_request()` / `intrinsic_height_request() ->
  Option<(IntrinsicSize, bool)>`, where the flag is Compose's `enforceIncoming`: `true` for the
  `size`/`width`/`height` forms (`Intrinsic.kt:51/80`), `false` for `requiredWidth`/`requiredHeight`
  (`:105/:130`).

### 8.2 The protocol (`winia/src/layout/node.rs`)

- Four defaulted methods on `MeasurePolicy` — `min_intrinsic_width`, `max_intrinsic_width`,
  `min_intrinsic_height`, `max_intrinsic_height` — defaulting to Compose's documented approximation:
  measure with the queried axis unbounded and report that axis. `MeasurePolicy` stays object-safe and
  every existing `impl` compiles unchanged.
- `IntrinsicCtx<'a> { nodes, policies, policy_idx }` is the probe's own receiver: `approx_measure` runs
  the policy's own `measure` for the defaults, and `child_intrinsic(child, query, other)` recurses.
  `child_weight(child)` reads `Modifier::get_layout_weight()`, which is where winia stores what Compose
  keeps as `LayoutWeightNode` parent data.
- `intrinsic_size_of(nodes, policies, idx, query, other)` is the entry point. It never calls
  `measure_node`, so it cannot write `dirty` / `layout_dirty` / `cached_constraints` /
  `subcomposed` (`node.rs` write-back sites) and cannot invalidate the frame's measure results.
- `IntrinsicQuery { MinWidth, MaxWidth, MinHeight, MaxHeight }` keeps the two axes explicit; Compose's
  `IntrinsicMinMax`/`IntrinsicWidthHeight` pair is folded into one enum.

### 8.3 The modifier chain is inverted, not read

The pipeline order in `measure_node_inner` is the specification, and the probe replays it on the
modifier chain instead of the node: `resolved_size` → `min_size_constraint` → `max_size_constraint`
("min wins", winia's documented deviation) → `required_size_constraint` → padding. Consequences that
the tests pin:

- A fixed axis short-circuits: a node whose axis is fixed returns that value **without** padding,
  because the pipeline tightens the node's own box before the padding offset is pushed inward.
- Otherwise the answer is `content.clamp(lo, hi) + padding`, mirroring "measure the content, then add
  padding back".
- `fill_max` is deliberately ignored when answering intrinsics, matching Compose's `FillNode`
  (`Size.kt:689`), which does not override the intrinsic methods either.
- A scroll modifier on the queried axis forwards with `other = f32::MAX`, mirroring
  `foundation/Scroll.kt:478-500`.
- `size`/`width`/`height(IntrinsicSize)` clamp the answer to the incoming constraint
  (`enforceIncoming = true`, `Intrinsic.kt:34-49`); the `required*` forms do not.

### 8.4 Containers share the flex arithmetic

`winia/src/layout/flex.rs` gained `flex_intrinsic_main` / `flex_intrinsic_cross`, ports of
`RowColumnImpl.kt:371-394` and `:396-452` — including `weightUnitSpace = max(size / weight)`, which is
the reason Compose's Row/Column report finite intrinsics for weighted children at all. `RowLayout` and
`ColumnLayout` map their four queries onto them exactly as `IntrinsicMeasureBlocks` does
(`RowColumnImpl.kt:261-369`): a main-axis query prices the weighted children by weight unit, a
cross-axis query first resolves the main-axis room each child gets (`mainAxisSize` is always the MAXX
query there) and then asks the children's cross size at that room. One deviation is stated in a
comment: `remaining` is floored at 0.0 where Compose can go negative on ints.

Box and every other container keep the default approximation, which is also what Compose does for Box.

### 8.5 Leaves

- Text answers `MinWidth`/`MaxWidth` from the skia paragraph
  (`Paragraph::min_intrinsic_width` / `max_intrinsic_width`) and a height query by measuring at the
  given width — winia has no height-parameterised text path, so the query is answered at the width the
  parent offers rather than by a height-keyed cache (§7). The probe builds the paragraph through
  `build_text_paragraph` and does **not** touch the render paragraph cache; a test pins that.
- Rich text measures through `measure_and_cache_richtext` at the queried width, images report their
  intrinsic size (min = max), and a plain leaf answers 0 — the fixed-axis case short-circuits before it
  matters.
- A policy whose `subcomposes()` is true is never probed: it reports its last measured size on that
  axis. Compose has no intrinsic scope that could compose, and winia's probes run outside the frame's
  compose step.

### 8.6 The two hand-rolled passes are gone

- `MenuColumnPolicy` (`winia/src/ui/overlay.rs`): pass 1 is now one
  `intrinsic_size_of(item, IntrinsicQuery::MaxWidth, f32::MAX)` per item, clamped to 112/280 dp. The
  item is a `Row` whose label is `weight(1f)`, and it is the Row's intrinsic block that prices it by its
  own width — the exact trap the old hand-written pass documented.
- `SegmentedRowPolicy` (`winia/src/ui/segmented_button.rs`): the natural width is the widest item's max
  intrinsic width, and the height is the tallest item's min intrinsic height **at the width they all end
  up with**, which is the order `IntrinsicMeasureBlocks` asks in.

### 8.7 Tests

`winia/src/layout/node.rs` ends with `#[cfg(test)] mod intrinsic_tests`: one test per pipeline step and
per protocol rule, including the default approximation, the fixed-axis short-circuit, the padding
addition, the probe's cache hygiene, the weighted-label trap, Row/Column main- and cross-axis answers,
`width`/`height(IntrinsicSize)` vs the `required` forms, the scroll forwarding, and the subcomposing
refusal. Both "turn it off" checks were run and are recorded below.

Acceptance is by the existing suite: menu width 112 dp (icon menu) / 136 dp ("Copy / Ctrl+C"), equal
item widths, and the segmented geometry all had to stay put — they did.

