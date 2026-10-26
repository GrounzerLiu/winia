# Arrangement

How `Row` and `Column` distribute their children along the MAIN axis.

## Sources

Compose foundation `Arrangement` (`reference/compose/foundation-layout-1.13.0-beta01/commonMain/androidx/compose/foundation/layout/Arrangement.kt`):
the mirroring family (`Start`, `End`, `Center`, `SpaceBetween`, `SpaceAround`, `SpaceEvenly`), the
`Absolute` object (`:354+`, packed to a physical edge, never mirrored) and `aligned(alignment)` /
`spacedBy(space, alignment)` (`:296-350`).

## API

```rust
Row::new().arrangement(Arrangement::SpaceBetween)
Row::new().arrangement(Arrangement::Absolute(AbsoluteArrangement::Start))
Row::new().arrangement(Arrangement::Aligned(Alignment::End))
Row::new().arrangement(Arrangement::Aligned(Alignment::End)).spacing(10.0)  // Compose's spacedBy
```

The gap between children is the container's `spacing` (also `Column::spacing`); the arrangement only
decides where the LEFTOVER main-axis space goes. `Arrangement::Aligned(a)` plus `spacing(s)` is
Compose's `Arrangement.spacedBy(s, a)`, and `Aligned(a)` alone is `Arrangement.aligned(a)`.

## Layout

`compute_spacing` (`layout/node/measure/paragraph.rs`) turns an arrangement into
`(gap between items, leading offset)`; `measure_flex` adds the container's `spacing` to the gap and
places the children. The final cross-axis size and the alignment-line pass are unaffected.

## Behaviour

| arrangement | leftover goes to | RTL |
| --- | --- | --- |
| `Start` | the far end | mirrors: packs right |
| `End` | the near end | mirrors: packs left |
| `Center` | split | mirrors |
| `SpaceBetween` | the gaps; one child packs at the start | mirrors |
| `SpaceAround` / `SpaceEvenly` | gaps, including the edges | mirrors |
| `Absolute(AbsoluteArrangement::…)` | same six distributions | **does NOT mirror on the main axis** |
| `Aligned(a)` | that edge | mirrors (`End` in RTL is the left edge, as Compose means it) |

Two details worth knowing:

- `Absolute` suppresses the mirror only on the MAIN axis. A `Column`'s main axis is y, and the
  mirror that RTL needs there is on the CROSS axis (x) — so `Absolute` on a Column still hugs the
  right edge under RTL. Pinned by
  `an_absolute_arrangement_still_mirrors_the_column_cross_axis`.
- `Aligned(Alignment::Stretch)` has no meaning on the main axis and behaves as `Center`; Compose's
  `Alignment.Horizontal` cannot express it either.

## Differences from Compose

- **No `Arrangement.spacedBy(space)` value.** Compose carries the space inside the arrangement; winia
  keeps it on the container (`spacing`), because that is where the measure already needed it. The
  observable geometry is the same: `Aligned(a).spacing(s)` == `spacedBy(s, a)`.
- **No `Arrangement.Absolute.Left/Right/Top/Bottom` aliases.** `Absolute(AbsoluteArrangement::Start)`
  covers the same thing on either axis.
- **The enum is closed**, so an application cannot supply its own distribution. Compose allows a
  custom `Arrangement.Horizontal`/`Vertical` implementation.
- `RowScope`/`ColumnScope` do not exist, so a misplaced `layout_weight`/`align_self` is silently
  ignored rather than a compile error.

## Tests

`winia/src/layout/row.rs` (`layout::row::tests`) and `winia/src/layout/column.rs`
(`layout::column::tests`):

| test | pins |
| --- | --- |
| `an_absolute_arrangement_ignores_the_layout_direction` | `Absolute::Start` stays left in an RTL row, while `Start` mirrors |
| `an_absolute_end_arrangement_packs_to_the_right` | `Absolute::End` in LTR |
| `an_aligned_arrangement_gives_the_leftover_to_the_alignment` | Start / Center / End leading offsets |
| `an_aligned_end_arrangement_mirrors_under_rtl` | `End` in RTL packs the pair to the left and reverses the children |
| `an_aligned_arrangement_keeps_the_container_spacing` | `Aligned` + `spacing` == `spacedBy` |
| `an_absolute_arrangement_still_mirrors_the_column_cross_axis` | the main-axis exemption does not leak to the cross axis |

Run them with `cargo test -p winia --lib layout::row layout::column`.
