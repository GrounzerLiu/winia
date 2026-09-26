# BoxWithConstraints

> Source of truth: `winia/src/ui/box_with_constraints.rs`. Alignment target: Compose
> `androidx.compose.foundation.layout.BoxWithConstraints` plus its `BoxWithConstraintsScope`.

## What it is

A `Stack` (winia's `Box`) whose content receives the constraints the box was measured with:

```rust
BoxWithConstraints::new()
    .modifier(Modifier::new().fill_max_width().height(48.0))
    .build(ctx, |ctx, scope| {
        if scope.max_width() > 600.0 {
            Row::new().build(ctx, two_columns);
        } else {
            Column::new().build(ctx, one_column);
        }
    });
```

| Scope API | Meaning |
|---|---|
| `constraints()` | the whole `layout::Constraints` the box was measured with |
| `min_width()` / `max_width()` / `min_height()` / `max_height()` | the four bounds, in logical pixels |
| `max_dimension()` / `min_dimension()` | Compose's `maxDimension` / `minDimension` |
| `is_measured()` | `false` while the value is still the initial unbounded one — see the first-frame note |

## How the constraints get there

winia composes before it measures and has no subcomposition, so the value travels on the framework's
measure-to-compose channel: the box's `MeasurePolicy` records the constraints it was measured with,
and the content reads them on its next run.

The channel is a **`Reactive<Constraints>`**, and that choice is load-bearing:

- A `Backchannel` (the channel `LazyListState.content_height` uses) would **not work here** —
  `set_backchannel` only overwrites the value and never moves the signal's revision, so nothing would
  ever wake the content. Measured while writing this component: the tree kept printing
  "not measured yet (frame 1)" forever.
- `State<Constraints>` notifies on change and **dedups on `PartialEq`**, so a constraint that did not
  move notifies nobody. That is what keeps the measure-write → recompose → measure cycle bounded
  instead of self-sustaining.

## Deliberate behaviour: the value is one composition behind

Compose's scope is current *within* the frame (a measure-time subcomposition). Here the content reads
what the **previous** measure wrote. Consequences, stated plainly:

- **The first composition reads the initial value** (`Constraints::UNBOUNDED`: min 0, max `+∞`), so a
  caller must treat an infinite maximum as "not measured yet" — that is what `is_measured()` is for.
  The second run has the real constraints; the write itself wakes the composition, so this needs no
  help from the caller (verified on a real window: the content's second run read
  `max_width: 200.0` for a 200-wide box inside a 420-wide window, and the committed tree reported the
  narrow branch).
- **A change of constraints re-runs the content**, because the write goes through `State::set` and the
  content read registered a composition dependency. The trail is one composition, not one frame in
  the common case (the write happens during the frame's layout, and the recompose loop at the top of
  the frame handler consumes what it enqueued).
- **No auto-refresh when nothing else changes and the constraint is unchanged.** The box is as static
  as its parent; it does not poll.

Closing the trail needs a real lookahead/subcomposition pass — the same missing piece the shared
element work records (`docs/shared-element-transition.md` §3.1) — and that is a framework-level change
rather than a component one.

## Deliberate deviation: no `Dp`

The scope speaks plain `f32` logical pixels, for the same reason as `DrawScope`
(`docs/canvas.md`): winia has no `Dp` unit type in this layer. `constraints()` is exposed as the
framework's own `Constraints` so a caller can hand it to a custom `MeasurePolicy` unchanged.

## Tests

`ui::box_with_constraints::tests`:

| test | what it pins |
|---|---|
| `scope_reports_the_constraints_it_was_handed` | the four bounds, `max_dimension` / `min_dimension`, and that `is_measured()` is false for the initial unbounded value |
| `policy_records_the_incoming_constraints` | the write half: the policy records exactly the constraints it was measured with |
| `box_with_constraints_lays_out_like_a_stack_and_hands_its_content_a_scope` | end to end: the box takes its modifier's size, the content runs, and the first run reads the unmeasured value (the documented trail) |
