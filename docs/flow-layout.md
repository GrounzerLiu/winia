# FlowRow / FlowColumn (foundation parity)

## Sources

> Status: implemented on branch `v2`.
> Reference: Compose foundation `FlowRow` / `FlowColumn`
> (`FlowLayout.kt`, `FlowLayoutBuildingBlocks.kt`, `FlowLayoutOverflow.kt`, 1.13.0-beta01).
> Implementation: `winia/src/layout/flow.rs` is the facade; the algorithm lives in
> `winia/src/layout/flow/measure.rs`, the two policies/builders in `flow/row.rs` and
> `flow/column.rs`, and the tests in `flow/tests.rs`.
> Demo: `winia/examples/flow_features_demo.rs` (also compiled into the UI fixture as the
> `flow_features` scenario).

## 1. API

```rust
// Flowing row: wraps when the main axis is full (chip groups, tag clouds).
FlowRow::new()
    .main_spacing(8.0)        // gap inside a line (Compose horizontalArrangement.spacedBy)
    .cross_spacing(8.0)       // gap between lines (Compose verticalArrangement.spacedBy)
    .cross_arrangement(Arrangement::Start) // line distribution (Top/Center/Bottom/Space*)
    .max_items_in_row(3)      // per-line cap (unlimited by default)
    .max_lines(2)             // row cap (unlimited by default)
    .overflow(FlowOverflow::Clip) // Clip (default) or Visible
    .arrangement(Arrangement::Start)  // in-line main-axis distribution
    .alignment(Alignment::Start)      // in-line cross-axis alignment
    .modifier(Modifier)
    .build(ctx, |ctx| { /* children */ });

// Child-level line fill (FlowRowScope.fillMaxRowHeight / FlowColumnScope.fillMaxColumnWidth).
FlowRow::new().build(ctx, |ctx| {
    Text::new("short")
        .modifier(Modifier::new().fill_max_row_height())
        .build(ctx);
});

// Flowing column: same parameters, transposed.
FlowColumn::new()
    .main_spacing(8.0)
    .cross_spacing(8.0)
    .cross_arrangement(Arrangement::Start)
    .max_items_in_column(2)
    .build(ctx, |ctx| { /* children */ });
```

## 2. Semantics

- **Wrapping** (Compose `getWrapInfo`): never wrap at the start of a line; wrap past
  `max_items`; wrap when the remaining main-axis space cannot fit the next item. An
  unbounded main axis never wraps. `max_items(0)` and `max_lines(0)` produce an empty flow.
- **Child measurement**: every child is measured with the incoming main-axis maximum and a
  main-axis minimum of 0, exactly like Compose's `measureConstraints` (`FlowLayout.kt:1224-1231`).
  A text child therefore wraps at the flow's width instead of reporting its unwrapped width.
- **Line sizes**: the main axis is the raw content extent (clamped only at the container);
  the cross axis is the tallest item that actually fits in the line.
- **Cross-axis fill**: `Modifier::fill_max_row_height()` (FlowRow) and
  `fill_max_column_width()` (FlowColumn) measure the child against a fraction of the line's natural
  cross extent; the `_fraction` forms accept 0..=1. Weighted children keep their weight
  allocation and are cross-filled as well (Compose's weight branch applies the fraction too).
- **Cross-axis clipping** (`FlowOverflow::Clip`, the default): items are added to a line one
  at a time. The first item whose cross size exceeds the remaining cross space stops the flow;
  items already accepted, including the shorter items before it on the same line, stay placed.
  The gap before a line counts against that budget. `max_lines` caps the number of rows even when
  the cross axis has room (Clip mode). `FlowOverflow::Visible` skips both the cross-axis cut and
  `max_lines`, and keeps painting every row outside the container, matching Compose's
  `willOverflowCrossAxis` order (`FlowLayoutBuildingBlocks.kt:84-90`).
- **Baseline alignment**: `Modifier::align_by` / `align_by_baseline` on a child aligns it by its
  own alignment line instead of the edge; the line grows to the line's distance above the child
  plus the content below it, as Row/Column do. That growth happens after the per-item Clip fit
  check (Compose has the same looseness), so a baseline line can exceed the cross maximum by the
  ascent + descent tail; it is not discarded by the line budget.
- **In-line distribution**: the main axis uses `arrangement` + `main_spacing`; the cross axis
  uses `cross_arrangement` + `cross_spacing` (Start/Center/End/SpaceBetween/SpaceAround/SpaceEvenly).
  A child with `layout_weight` shares the line's free main-axis space after fixed children and
  spacing; the target is the bounded main-axis maximum (Compose's rule), or the natural line size
  when the axis is unbounded. `layout_weight_fill(weight, false)` treats that share as a maximum.
- **Container size**: main = widest visible line (wrap) or the constraint (fill); cross = the
  visible line heights plus the gaps.
- **Item animation**: put `Modifier::animate_item(...)` on a child's root. Flow keeps the child's
  stable slot key and paints the placement offset and opacity from the shared item host; the
  marker carries placement, fade-in and fade-out curves (`ItemAnimationSpec`). Use `ctx.key` /
  `ctx.keyed_range` for the identity to survive a reorder. `AnimatedSize` remains the container
  size animation; item size changes are not part of this contract.
- **RTL**: final mirror pass (container width − x − width), shared with the flex containers.
- **Composition**: the builders are deliberately not macros (a `State.get()` at the content's
  top level must bubble to the parent), and every parameter is declared with `ctx.changed`.

### Migrating from `animate_placement`

`FlowRow/FlowColumn::animate_placement(bool)` and the container-level `placement_spec` are gone:
the contract now lives on the child (`Modifier::animate_item` / `animate_item_with`), and the
container honors any marked child automatically. Replace
`.animate_placement(true)` with nothing, and move the curve from the container to the child's
`ItemAnimationSpec`.

## 3. Differences from Compose

- A fill fraction on the tallest child does not shrink the line: winia keeps the natural line
  cross as the fill target, whereas Compose recomputes the line from the filled sizes. The
  positions of the other children are unaffected; only the enclosing line height differs.
- Clip decisions are made after weight shares are applied, so a weighted child that grows taller
  when its share narrows can be clipped differently from Compose, whose first measurement decides
  the clip. The version that is actually laid out is used here.
- Weight is supported per line: `layout_weight(weight)` fills its share (default `fill = true`),
  `layout_weight_fill(weight, false)` uses the share as a maximum. Intrinsic queries keep using
  the child's intrinsic size even when it carries a weight; weight is a measure-time allocation,
  matching Compose's weighted IntrinsicMeasurable path. Line breaking is computed from the
  children's natural sizes before the shares are allocated, so a heavily weighted child can
  choose a different line than Compose's intrinsic pass would.
- No overflow-indicator API (`ExpandIndicator`, `ExpandOrCollapseIndicator`). `max_lines`
  and the `Clip` / `Visible` modes are implemented; the indicator variants are not.
- Intrinsic queries are implemented: max main is the longest line sum, min main is a binary
  search for the narrowest axis that fits the cross budget and shows every item allowed by
  max_items/max_lines, and cross intrinsics use the same line/max-items/max-lines rules.
- `cross_arrangement` distributes the free cross space between lines; `cross_spacing` remains
  the fixed gap added before the arrangement's share. A single line with `SpaceBetween` is
  centred by the shared `compute_spacing` helper (the legacy flex convention); Compose starts
  it at the leading edge. Other arrangements match Compose.
- Row layout size is `max(content, constraints.min)`: a fill parent gives a container-wide line
  with in-line alignment; a wrap parent gives a content-wide line. Compose's line layout size is
  the content width; the difference is invisible under wrap-content and under fill it only moves
  Start/Center/End/SpaceBetween inside the container, as winia does.

## 4. Running and tests

```bash
cargo run -p winia --example flow_features_demo
cargo test -p winia --lib layout::flow
```

The animated card toggles `max_items_in_row` between 3 and 2. Its children carry
`Modifier::animate_item()`, so the 3x2 -> 2x3 reflow moves each card from its old slot.

The flow tests cover wrapping, an unbounded main axis, `max_items` breaks, `max_lines`,
`Visible` overflow, weight sharing, `fillMaxRowHeight` / `fillMaxColumnWidth`, intrinsic queries,
spacing and in-line centring, cross arrangement (Center/SpaceBetween/RTL), RTL mirroring,
column wrapping, fill-tight
containers, child measurement against the container width, cross-axis item clipping (including
a tall item after short ones), a first line that cannot fit, cross spacing in the cut-off, and
the two composition paths.
