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
| `min_width_dp()` / `max_width_dp()` / `min_height_dp()` / `max_height_dp()` | the same bounds as `Dp` (logical, NOT `to_px`) |
| `max_dimension()` / `min_dimension()` | Compose's `maxDimension` / `minDimension` |
| `is_measured()` | `false` while the value is still the initial unbounded one — see the first-frame note |

## How the constraints get there

The content is composed **during measurement**, inside a subcomposition (`ui::subcompose`), and the
composed tree is ADOPTED as the box's child — so the scope carries the constraints this measurement just
computed, which is the relation Compose has. The box is as big as what its content composed (clamped by
the constraints), so the policy reports the content's measured size instead of a size of its own.

The content closure is `Fn`, not `FnOnce`: a later frame's measurement composes it again — into the
composition the box kept from the previous frame, so a `remember` inside it survives the frame.

## When the box re-measures

`MeasurePolicy::subcomposes()` returns `true` for this policy, and the box's node is marked
`subcomposed`. That is what keeps the node's slot key in the frame's layout invalidation set: the composed
subtree lives in the arena and only a measurement re-attaches it, so a folded frame would let materialize
detach it.

It does NOT mean "measure every frame". The box folds like any other node when nothing about it changed,
and re-measures when one of these moved:

| trigger | what carries it |
|---|---|
| its own `Modifier` differs from last frame's | a value computed in a PARENT's scope, e.g. `.max_width(cap.get())` — `Composer::node_modifier_changed` |
| its node slot was dirtied | a declared parameter or a state read inside the box changed (the compose end also seeds the slot key) |
| a state read DURING the measurement changed | a layout-only dependency (`layout_dirty_keys`) |

Folding is what keeps a screen with many boxes cheap, and the three triggers are what makes a change
land on the frame it happened: `docs/benchmarks.md` has the numbers (`one row updated, 800 rows`:
74915 µs before, 1696 µs after).

## What it is not

The content re-composes on every MEASUREMENT. Compose's subcomposition can skip when nothing it depends
on changed; here a measurement always re-runs the content closure. Since a measurement only happens for
one of the three triggers above, that costs nothing on frames that have no work to do.

## Deliberate difference: the scope speaks the layout coordinate system, not `Dp`

winia HAS `Dp` (`unit::Dp`, exported by the prelude, accepted by `Modifier::size` and friends) — an
earlier version of this file claimed otherwise and was wrong. What the scope hands back is `f32`
logical pixels, because that is what the box was measured in (`Constraints` carries plain numbers) and
because `Dp::to_px` returns *physical* pixels, which must not meet layout geometry. For a caller that
wants a bound on the type, the four `*_dp()` accessors (`min_width_dp`, `max_width_dp`,
`min_height_dp`, `max_height_dp`) carry exactly the same numbers via `Dp::to_logical`, which is the
identity here. `constraints()` is the framework's own `Constraints`, so it can go straight into a
custom `MeasurePolicy`.

## Tests

`ui::box_with_constraints::tests`:

| test | what it pins |
|---|---|
| `scope_reports_the_constraints_it_was_handed` | the four bounds, `max_dimension` / `min_dimension`, the `_dp` forms, and that `is_measured()` is false for the initial unbounded value |
| `the_content_sees_the_real_constraints_on_the_first_frame` | the content composes DURING the measurement, so frame one already has the real cap (the old design's "one composition behind" trail is gone) |
| `the_box_sizes_to_its_content_on_the_first_frame_and_the_next` | the box's size IS the content's size, on the composed frame and on the next one (the reused parent detaches and re-attaches the adopted child) |
| `the_box_keeps_its_adopted_child_across_a_frame_it_did_not_measure` | a frame the box folds does not detach the adopted subtree |
| `a_cap_change_in_the_composition_reaches_the_content_the_box_composes` | the `bwc` UI fixture's failure, reduced: a state read in a PARENT scope feeding `.max_width(cap.get())` must re-measure the box and reach the content it composes, in both directions |
