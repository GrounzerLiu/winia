# FlowRow / FlowColumn (foundation parity)

> Status: implemented on branch `v2`.
> Reference: Compose foundation `FlowRow` / `FlowColumn`
> (`FlowLayout.kt`, `FlowLayoutBuildingBlocks.kt`, `FlowLayoutOverflow.kt`, 1.13.0-beta01).
> Implementation: `winia/src/layout/flow.rs` is the facade; the algorithm lives in
> `winia/src/layout/flow/measure.rs`, the two policies/builders in `flow/row.rs` and
> `flow/column.rs`, and the tests in `flow/tests.rs`.
> Demo: `winia/examples/flow_demo.rs`.

## 1. API

```rust
// Flowing row: wraps when the main axis is full (chip groups, tag clouds).
FlowRow::new()
    .main_spacing(8.0)        // gap inside a line (Compose horizontalArrangement.spacedBy)
    .cross_spacing(8.0)       // gap between lines (Compose verticalArrangement.spacedBy)
    .max_items_in_row(3)      // per-line cap (unlimited by default)
    .arrangement(Arrangement::Start)  // in-line main-axis distribution
    .alignment(Alignment::Start)      // in-line cross-axis alignment
    .modifier(Modifier)
    .build(ctx, |ctx| { /* children */ });

// Flowing column: same parameters, transposed.
FlowColumn::new()
    .main_spacing(8.0)
    .cross_spacing(8.0)
    .max_items_in_column(2)
    .build(ctx, |ctx| { /* children */ });
```

## 2. Semantics

- **Wrapping** (Compose `getWrapInfo`): never wrap at the start of a line; wrap past
  `max_items`; wrap when the remaining main-axis space cannot fit the next item. An
  unbounded main axis never wraps.
- **Child measurement**: every child is measured with the incoming main-axis maximum and a
  main-axis minimum of 0, exactly like Compose's `measureConstraints` (`FlowLayout.kt:1224-1231`).
  A text child therefore wraps at the flow's width instead of reporting its unwrapped width.
- **Line sizes**: the main axis is the raw content extent (clamped only at the container);
  the cross axis is the tallest item that actually fits in the line.
- **Cross-axis clipping** (Compose's default `Clip` overflow): items are added to a line one
  at a time. The first item whose cross size exceeds the remaining cross space stops the flow;
  items already accepted, including the shorter items before it on the same line, stay placed.
  The gap before a line counts against that budget. This is the default Clip behaviour, not the
  `maxLines` / overflow-indicator API.
- **In-line distribution**: the main axis uses `arrangement` + `main_spacing`; the cross axis
  currently has only a fixed `cross_spacing` (no vertical/horizontal cross arrangement).
- **Container size**: main = widest visible line (wrap) or the constraint (fill); cross = the
  visible line heights plus the gaps.
- **RTL**: final mirror pass (container width − x − width), shared with the flex containers.
- **Composition**: the builders are deliberately not macros (a `State.get()` at the content's
  top level must bubble to the parent), and every parameter is declared with `ctx.changed`.

## 3. Differences from Compose

- No weight. Compose re-distributes a line's remaining space in a second measure pass; winia
  measures a `layout_weight` child at its natural size and silently ignores the weight. Remove
  the weight when migrating a Row to a FlowRow.
- No `maxLines` or overflow-indicator API (`ExpandIndicator`, `ExpandOrCollapseIndicator`).
  The default Clip cross-axis cut is implemented; the indicator variants are not.
- No intrinsic overrides: the four `MeasurePolicy` intrinsic methods fall back to the generic
  approximation, which treats the flow as one unbounded line.
- Row layout size is `max(content, constraints.min)`: a fill parent gives a container-wide line
  with in-line alignment; a wrap parent gives a content-wide line. Compose's line layout size is
  the content width; the difference is invisible under wrap-content and under fill it only moves
  Start/Center/End/SpaceBetween inside the container, as winia does.

## 4. Running and tests

```bash
cargo run -p winia --example flow_demo
cargo test -p winia --lib layout::flow
```

The flow tests cover wrapping, an unbounded main axis, `max_items` breaks, spacing and
in-line centring, RTL mirroring, column wrapping, fill-tight containers, child measurement
against the container width, cross-axis item clipping (including a tall item after short ones),
a first line that cannot fit, cross spacing in the cut-off, and the two composition paths.
