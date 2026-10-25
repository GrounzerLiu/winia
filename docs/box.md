# Box (`Stack`)

> Source of truth: `winia/src/layout/box_layout.rs`. Alignment target: Compose
> `androidx.compose.foundation.layout.Box` (`BoxMeasurePolicy`) and its `BoxScope`.

## Sources

- Compose `foundation-layout/Box.kt`: `BoxMeasurePolicy` (child constraints, size,
  `matchParentSize`), `BoxScope.align`, `BoxScope.matchParentSize`.
- winia: `BoxLayout` (the `MeasurePolicy`), `Stack` (the builder), and
  `BoxWithConstraints` for the measure-time scope (`docs/box-with-constraints.md`).

## API

| winia | Compose |
| --- | --- |
| `Stack::new().alignment(a)` | `Box(contentAlignment = <axis-symmetric value>)` |
| `.content_alignment(ContentAlignment::TOP_END)` | `Box(contentAlignment = Alignment.TopEnd)` |
| `.propagate_min_constraints(true)` | `Box(propagateMinConstraints = true)` |
| child `Modifier::content_align_self(a)` | `BoxScope.Modifier.align(a)` |
| child `Modifier::align_self(a)` | `BoxScope.Modifier.align(<axis-symmetric value>)` |
| child `Modifier::match_parent_size()` | `BoxScope.Modifier.matchParentSize()` |

## Semantics

- the SIZING children are measured with the box's constraints LOOSENED (minimum 0), or unchanged
  when `propagate_min_constraints` is on;
- the box is `max(child, constraints.min)` and is never cropped to the maximum;
- placement uses, per child, `content_align_self` > `align_self` (both axes, `Stretch` excepted) >
  the box's `contentAlignment`, with `Center` as the fallback for a match-parent child (Compose's
  `matchParentSize` element carries `alignment = Center`); `Start`/`End` mirror under RTL;
- a `match_parent_size` child does NOT size the box: it is measured in a second pass with the box's
  size as a TIGHT constraint (`min = box`, `max = box` — or `min = 0` when the box size is not finite,
  the `Constraints.Infinity` guard) and keeps its MEASURED size: normally that is the box, and a child
  that escapes the constraints (`required_size`, a post-measure growth step) is centred instead of
  cropped. Content that is all match-parent children therefore gives the incoming minimum (0 under
  loose constraints), and a scrim covers the box without dragging it out to its own parent (a
  `fill_max_size` child would);
- an empty box is `constraints.min`.

## Differences from Compose

- winia has no `BoxScope` receiver: the three child behaviours are ordinary modifiers, read only by
  this policy (a `Row`/`Column` ignores them).
- `Alignment::Stretch` is winia's own cross-axis value; a Box skips it in the `align_self` fallback
  instead of resizing the child on both axes.
- a match-parent child that overflows the box is placed at its measured size and centred; Compose
  gets the same result from `placeInBox` with the implicit `Center`, winia derives it from the same
  alignment path as every other child.
- `BoxWithConstraints` is winia's `Stack` plus a subcomposition; Compose runs the Box policy over
  the subcomposed measurables, winia wraps the content in a real Box node (one extra node, and
  multi-root content is sized by its widest root).
- a BOX-level post-measure growth step (`aspect_ratio`, `minimum_interactive_size`) runs after the
  policy, so a match-parent child matches the PRE-growth size. Compose's modifier nodes put the
  adjusted constraints in front of the box instead. Not introduced by `match_parent_size`, but it
  is the case where the two differ, and it is not pinned by a test because the current behaviour is
  the one being documented, not the one being promised.

## Tests

`winia/src/layout/box_layout.rs::tests` (unit, arena-level): size from the largest child, centre
alignment, RTL mirroring, two-axis corners, `propagate_min_constraints`, per-child alignment
precedence, overflow reports content, the minimum floor, empty box, `match_parent_size` (does not
size the box / fills a forced box / only-match takes the minimum / toggling re-measures), and the
`Stack` builder carrying both knobs.

`winia/tests/ui_box_test.rs` drives `examples/box_layout_demo.rs` through the fixture: per-child
alignment, overflow, `BoxWithConstraints` alignment, the `match_parent_size` scrim, and RTL.
