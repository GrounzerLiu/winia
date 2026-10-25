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
- an unbounded box (its size reaches the `f32::MAX` sentinel) centres such a child in that
  box, so the offset is astronomically large (~1.7e38). Compose has the same shape with
  `Int.MAX_VALUE`; no real tree reaches it — the branch exists for intrinsic probes — but it is
  the arithmetic the guard implies, and it is deliberately left unpinned.
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
- CHAIN ORDER for `padding` is Compose's: `padding` is a step in the size walk
  (`SizeStep::Padding`), so it deflates the constraints where it stands. `.padding(8).size(300)`
  fixes 300 inside the already-deflated range and reports a 316 dp node;
  `.size(300).padding(8)` absorbs the padding; `.padding(8).aspect_ratio(2.0)` derives from
  the deflated range (284x142) and reports 300x158. Window-INSET padding is the exception: an inset
  has no chain position, so it is applied after the walk — a size written inside an inset therefore
  behaves the way element padding did before this change. Making insets chain steps needs the inset
  consumers (scroll viewport, `padding_from`) re-ordered with them, so it is a separate job.
- the two growth steps interact with a match-parent child differently, and both match Compose.
  `aspect_ratio` moved into the size walk (`SizeStep::AspectRatio`): the constraints are fixed to
  the derived size before the policy, as Compose's `AspectRatioNode` does, so the content — and a
  match-parent child — see the aspect box, and the padding is no longer dropped by a post-measure
  rewrite. `minimum_interactive_size` stays a post-measure wrapper, because Compose's
  `MinimumInteractiveModifierNode` also measures its child with the incoming constraints and grows
  around that placeable (`material3/InteractiveComponentSize.kt:101-137`); a match-parent child
  therefore matches the PRE-growth box — the scrim covers the visual content, not the 48 dp touch
  target — and `a_match_parent_child_matches_the_pre_growth_box_under_minimum_interactive_size` pins it.

## Tests

`winia/src/layout/box_layout.rs::tests`: size from the largest child, centre alignment, RTL
mirroring, two-axis corners, `propagate_min_constraints`, per-child alignment precedence, overflow
reports content, the minimum floor, empty box, and the `Stack` builder carrying both knobs.
`match_parent_size`: does not size the box, fills a forced box, only-match takes the minimum
(loose and tight), an escaping child is centred, the child's own alignment beats the implicit
`Center`, the `Constraints.Infinity` guard (both halves, per axis, `f32::MAX` and `f32::INFINITY`),
a later frame follows the box, its own `Modifier::offset` still applies, toggling the flag
re-measures, and an ancestor box grows with an overflowing child. The aspect cases are
`aspect_ratio_fixes_the_box_and_its_content` and `aspect_ratio_and_a_match_parent_child` here, plus
`layout::node::tests` for the candidate order
(`aspect_ratio_falls_back_to_the_minimum_when_both_maxima_are_unbounded`,
`aspect_ratio_falls_back_to_an_unsatisfied_candidate_before_giving_up`) and the chain orders
(`wrap_content_outside_an_aspect_ratio_relaxes_then_restores_the_range`,
`an_aspect_ratio_outside_wrap_content_keeps_its_box_and_centres_the_content`,
`padding_outside_a_size_deflates_before_the_size_is_fixed`,
`a_size_outside_padding_absorbs_the_padding`,
`padding_outside_an_aspect_ratio_derives_from_the_deflated_range`,
`an_intrinsic_width_request_counts_its_padding_once`,
`wrap_content_outside_padding_clamps_the_inner_size`), and `layout::node::intrinsic_tests` for the
ratio's intrinsic answers. The min-interactive parity pin is
`a_match_parent_child_matches_the_pre_growth_box_under_minimum_interactive_size`.

Some of those drive a `Composer` end to end (the later-frame, offset, alignment, ancestor and
toggling cases) rather than the policy alone; `layout::node::intrinsic_tests` covers the intrinsic
path, and `layout::row::tests` pins that a Row ignores the flag.

`winia/tests/ui_box_test.rs` drives `examples/box_layout_demo.rs` through the fixture: per-child
alignment, overflow, `BoxWithConstraints` alignment, the `match_parent_size` scrim, and RTL.
