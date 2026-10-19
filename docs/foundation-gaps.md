# Foundation gaps

A survey of the cross-cutting layers winia still lacks — the same category as
[effects](effects.md) and [flows](flow-design.md): things every application touches, that sit
*under* the components, and that a component cannot provide for itself.

The criterion matters. A missing component (a Chip variant, a toolbar) is not a foundation: it can be
written on top of what exists. A missing foundation shows up as *the same work repeated by every
caller* — or as machinery that has been approximated three times in three places.

Status as of 2026-10-19 (branch `v2`, 1587 lib tests).

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
| `remember_derived_state` | `flow.rs` | recompute-on-read / notify-only-on-change (`derivedStateOf`) |
| `snapshot_flow` | `flow.rs` | observe a set of reads and re-run when they change |
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

What is *not* done: persistence across a process restart (nothing is serialized to disk), and
`nav.rs` still carries its own `SaveableStateHolder` for navigation entries rather than sharing the
registry.


**Why it matters now.** The effect work made window lifetime explicit (`FrameClock::for_window`,
`forget_window`, overlay composers inheriting the page's id). A recreated window still comes up with
empty `remember` slots, and no mechanism exists to refill them.

### 3. The scroll-container family

`winia/src/layout/` contains `lazy_column.rs` and nothing else in that family.

| Missing | Compose equivalent | Consequence |
| --- | --- | --- |
| `LazyRow` | `foundation.lazy.LazyRow` | horizontal card strips are hand-rolled |
| `LazyVerticalGrid` / `LazyHorizontalGrid` | `foundation.lazy.grid` | photo grids, keypads, galleries |
| `Pager` + `PagerState` | `foundation.pager` | onboarding, carousels, page snapping |

This is the most *user-visible* gap in the list: it cannot be papered over inside a component the way
a semantics attribute can.

### 4. ~~AnchoredDraggable~~ — already shared

Corrected during review: `overlay/anchored_draggable.rs` IS the shared state machine (settle targets,
velocity hand-off, snap after fling), used by `navigation_drawer`, `sheet_state` and the bottom sheet.
There is nothing to extract here.

### 5. Drag and drop

No `DragAndDrop` equivalent at all: no drag source, no drop target, no transfer payload.

### 6. Platform interop

No embedding of a native widget inside the composition.

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

**LazyRow and Pager are the highest-visibility items.** They are additive, touch nothing existing, and
unblock whole categories of screens.

## Re-checking this document

```bash
ls winia/src/layout | grep -i 'lazy\|grid\|pager'      # scroll containers: only lazy_column today
grep -rln 'AnchoredDraggableState' winia/src            # the shared drag state machine (exists)
grep -rln 'CompositionLocal' winia/src | wc -l           # exists
grep -rn 'MutableSnapshot' winia/src                     # records: expect nothing
```
