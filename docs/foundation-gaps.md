# Foundation gaps

A survey of the cross-cutting layers winia still lacks — the same category as
[effects](effects.md) and [flows](flow-design.md): things every application touches, that sit
*under* the components, and that a component cannot provide for itself.

The criterion matters. A missing component (a Chip variant, a toolbar) is not a foundation: it can be
written on top of what exists. A missing foundation shows up as *the same work repeated by every
caller* — or as machinery that has been approximated three times in three places.

Status as of 2026-10-19 (branch `v2`, 1592 lib tests).

## What is missing

### 1. Snapshot — the state system underneath everything

This is the largest gap, and the one the rest of this document keeps pointing back to.

**What Compose has.** A global snapshot version; `snapshot { }` / `MutableSnapshot` for atomic
multi-state writes; snapshot observation, on which `derivedStateOf` and `snapshotFlow` are built.
A reader inside one snapshot sees one consistent moment, even if ten states change underneath it.

**What winia has.** `State` = a value + a revision + a `Subscriber` list (`runtime/state.rs`), plus
a *pass-scoped* dependency frame: reads are recorded during a compose pass and reconciled at commit.
Since the feasibility study, also a global write version and `write_batch` — the cheap half of a
snapshot system, with no per-snapshot records. See [snapshot-feasibility.md](snapshot-feasibility.md).

**Why it is a foundation gap.** Three separate pieces of machinery are currently approximating the
same absent primitive:

| Approximation | Where | What it exists to solve |
| --- | --- | --- |
| `remember_derived_state` | `runtime/flow.rs` | recompute-on-read / notify-only-on-change (`derivedStateOf`) |
| `snapshot_flow` | `runtime/flow.rs` | observe a set of reads and re-run when they change |
| the dependency frame + revision handshake | `runtime/state.rs` | subscribe-at-read and rollback on panic |

Each carries its own resubscription and dedup logic, and they share a gap: **within one pass, two
reads of two different states can observe different moments.**

A faithful Compose-style snapshot layer is **not** the right answer here — the record-based design
costs thousands of lines in the hottest path and buys properties a single-threaded composer does
not need. What is worth building is the cheaper half: a global write version plus atomic write
batches. See [snapshot-feasibility.md](snapshot-feasibility.md) for the measurements, the four
options and the recommended phases.

### 2. Saveable — SHIPPED (in-memory, same process)

`runtime/saveable.rs`: `remember_saveable` + `SaveableStateRegistry`, wired into the window
lifecycle (`AppState.saveable`, keyed by `created_id`). See [saveable.md](saveable.md).

Persistence across a process restart is opt-in (`saveable::persist::set_store`) and implemented
(versioned, atomic, size-capped JSON). What is *not* done: `nav.rs` still carries its own
`SaveableStateHolder` for navigation entries rather than sharing the registry.


**Why it mattered.** The effect work made window lifetime explicit (`FrameClock::for_window`,
`forget_window`, overlay composers inheriting the page's id); a recreated window used to come up with
empty `remember` slots. That is what `remember_saveable` fixes — see [saveable.md](saveable.md).

### 3. The scroll-container family

Most of the family shipped since this document was written: `LazyRow` is the same axis-generic
`LazyList` as `LazyColumn` (`layout/lazy/list.rs`), `LazyVerticalGrid`/`LazyHorizontalGrid` are one
axis-generic `LazyGrid` (`layout/lazy/grid.rs`, see [lazy-grid.md](lazy-grid.md)), and `Pager` lives in
`components/pager.rs`, and `LazyVerticalStaggeredGrid`/`LazyHorizontalStaggeredGrid` are the axis-generic
`LazyStaggeredGrid`. What is left is the generic lazy layout API and the prefetch strategy.

| Missing | Compose equivalent | Consequence |
| --- | --- | --- |
| Generic `LazyLayout` API | `foundation.lazy.LazyLayout` | a custom lazy layout cannot reuse the window, the anchor model or item recycling |
| `LazyListPrefetchStrategy` | `foundation.lazy.LazyListPrefetchStrategy` | winia widens the window with `beyond_bounds(n)` instead of a per-frame prefetch budget |

The generic `LazyLayout` API is the largest item left: without it a custom lazy layout cannot reuse the
window, the anchor model or item recycling.

### 4. ~~AnchoredDraggable~~ — already shared

Corrected during review: `overlay/anchored_draggable.rs` IS the shared state machine (settle targets,
velocity hand-off, snap after fling), used by `navigation_drawer`, `sheet_state` and the bottom sheet.
There is nothing to extract here.

### 5. Drag and drop

No `DragAndDrop` equivalent at all: no drag source, no drop target, no transfer payload.

### 6. Platform interop

No embedding of a native widget inside the composition.

### 7. The layout primitives

Surveyed against `reference/compose/foundation-layout-1.13.0-beta01` (22 public API files) plus the
`foundation.lazy` packages. `winia/src/layout/` covers most of the pre-1.13 surface; the gaps are
concentrated in the APIs 1.13 added, and in the grid family.

| Compose | winia | Notes |
| --- | --- | --- |
| `Row` / `Column` / `Box` / `BoxWithConstraints` / `Spacer` | yes | the box is named `Stack` (`box_layout.rs`), `BoxLayout` is the policy |
| `FlowRow` / `FlowColumn` | yes | `flow.rs`; `Clip`/`Visible` overflow only |
| `FlowRowOverflow.expandIndicator` / `expandOrCollapseIndicator` | **missing** | recorded in `flow.rs`'s header |
| `Arrangement` (Start/End/Center/SpaceBetween/Around/Evenly) | yes | `node/geometry.rs` |
| `Arrangement.Absolute.*` | **closed 2026-10-26** | `Arrangement::Absolute(AbsoluteArrangement)` |
| `Arrangement.aligned()` / `spacedBy(space, alignment)` | **closed 2026-10-26** | `Arrangement::Aligned(Alignment)`; the gap half of `spacedBy` stays on the container (`Row::spacing`) — see [arrangement.md](arrangement.md) |
| `Arrangement.spacedBy(space)` as a value | partial | the gap is `Row::spacing` / `Column::spacing`, not part of the arrangement |
| `Modifier.visible` | **closed 2026-10-26** | `Modifier::visible(bool)`; see [visible.md](visible.md) |
| `Modifier.windowInsetsStartWidth` / `EndWidth` / `TopHeight` / `BottomHeight` | **missing** | `WindowInsetsSize.kt`; winia has the padding form only |
| `Modifier.fitInside` / `fitOutside` (rulers) | **missing** | `RulerAlignment.kt`, new in 1.13 |
| `Grid` / `GridConfigurationScope` / `GridTrackSpec` | **closed 2026-10-26** | `layout::Grid` — fixed/percentage/weighted/intrinsic tracks, spans, gaps, named areas, pass-1.5 deficit distribution; see [grid.md](grid.md) |
| `FlexBox` / `FlexBoxConfig` | **closed 2026-10-27** | `layout::FlexBox` — direction, wrap, justify/align items+content, grow/shrink/basis, order, responsive `config_for`; see [flexbox.md](flexbox.md) |
| `ContextualFlowRow` / `ContextualFlowColumn` | **missing** | `ContextualFlowLayout.kt` |
| `IntrinsicSize`, `AspectRatio`, `Size`, `Padding`, `Offset`, `AlignmentLine` | yes | all chain steps (`SizeStep`) in `modifier/builders/layout.rs` |
| `WindowInsets` + `windowInsetsPadding` | yes | `window/insets.rs`, `Modifier::status_bars_padding` and friends |
| `RowScope` / `ColumnScope` | deliberate non-goal | `layout_weight`/`align_self` are plain modifiers; a misplaced one is silently ignored |
| `LookaheadLayout` | deliberate non-goal | see [lookahead-subcompose-feasibility.md](lookahead-subcompose-feasibility.md); the app re-runs layout for the shared-element pass instead |
| `ConstraintLayout` | out of scope | `androidx.constraintlayout.compose`, a separate artifact |

## What exists and could still deepen

| Area | Today | Note |
| --- | --- | --- |
| Semantics + accessibility | `semantics.rs` (1410 lines) + `accessibility.rs` (1536 lines), `semantics_tree()` | per-component gaps are tracked in [semantics-gap.md](semantics-gap.md) |
| CompositionLocal | `runtime/composition_local.rs` (280 lines): `provides`, `capture`, `try_current` | implemented |
| Navigation | `nav.rs`, modelled on Nav3, with `SaveableStateHolder` | implemented |
| Movable content | composition-level movable placements in `runtime/composer.rs` | implemented |
| Tooltip | `components/tooltip.rs` | implemented |
| Animation | `Animatable`, decay, spring specs | gaps tracked in [animation-gap-analysis.md](animation-gap-analysis.md) |
| Shared elements | — | tracked in [shared-element-gaps.md](shared-element-gaps.md) |

## Deliberate non-goals

**Image / async resource loading.** The standing preference is to use a mature third-party crate for
this rather than build it, so its absence from the foundation list is a decision, not an oversight.

## Recommendation

**Snapshot is the highest-leverage item** — but the study that followed this survey found that the
faithful (record-based) version costs thousands of lines in the hottest path and buys properties a
single-threaded composer does not need. What shipped instead is write batches plus a global version:
it does not delete the three approximations, it gives them something to stand on.
See [snapshot-feasibility.md](snapshot-feasibility.md).

**The generic `LazyLayout` API is the highest-leverage item left in the container family.** The
remaining prefetch strategy is a refinement of the window the lazy lists already have.

## Re-checking this document

```bash
ls winia/src/layout | grep -i 'lazy\|grid\|pager'      # scroll containers: only lazy today
grep -rln 'AnchoredDraggableState' winia/src            # the shared drag state machine (exists)
grep -rln 'CompositionLocal' winia/src | wc -l           # exists
grep -rn 'MutableSnapshot' winia/src                     # records: expect nothing
```
