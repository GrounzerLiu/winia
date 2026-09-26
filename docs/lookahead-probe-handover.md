# `exp/lookahead-probe` — what it does now, and how to pick it up

> Status: **the whole objective is in place.** The acceptance test
> (`box_with_constraints_composes_its_content_at_measure_time`) is GREEN and un-ignored: the content
> prints the parent's cap on the first frame, the box takes its size from what it composed, and a cap
> change re-arranges it in both directions. **Cross-frame reuse is implemented too** (`e751261`): the
> subcomposition keeps its composition, measured by a unique id remembered inside the content —
> `[1, 1]` across two frames where it used to be `[1, 2]`. Both suites are green: lib 1092, UI 48 with
> nothing ignored.
>
> So this branch is no longer "a prototype with a known-wrong reading"; it is a facility plus one
> component that uses it, and the remaining question is a MERGE question (does `v2` want
> `BoxWithConstraints` composed at measure time, with its content re-composed every frame the node
> measures) rather than a defect.
>
> Reasoning and findings: `docs/lookahead-subcompose-feasibility.md` (this branch's copy carries the
> full trail; the mainline's copy at `7e66804` carries the result and the recommendation without the
> code references). Read §5c, §5d, "The defect's root cause" and "The cross-frame round" in that order,
> then come back here for the commands.

## What is on the branch (not on `v2`)

`git diff --stat v2 exp/lookahead-probe` → 13 files, 1410 insertions.

| file | what it is |
|---|---|
| `winia/src/ui/subcompose.rs` (428 lines) | the facility: a measure-time subcomposition, its parking registry, adoption into the outer arena, `LayoutHostGuard` |
| `winia/src/ui/subcompose_probe.rs` (369 lines) | 5 library tests that define what the facility guarantees — composing inside `measure`, TLS isolation, a panic inside it, adoption's index re-basing, and a subtree surviving the next frame |
| `winia/src/ui/box_with_constraints.rs` (rewritten, +272/−179) | `BoxWithConstraints` on the facility, with a `first_measure` generation guard and the `subcomposed` no-fold flag |
| `winia/src/core/composer.rs`, `winia/src/core/materialize.rs`, `winia/src/layout/node.rs` | the materialize/node contract: `subcomposed_child`, `subcomposed_measurements`, detach/reattach across both reuse arms, previous-subtree release |
| `winia/tests/ui_fixtures/fixture_bwc.rs` + a `("bwc", …)` row | the window fixture: a box whose parent cap changes on demand |
| `winia/tests/ui_test.rs` | the acceptance test — it ran `#[ignore]`d as the reproduction and is now un-ignored and green in the normal suite |
| `winia/tests/ui/mod.rs` | `UiTest::find_tag_size`, the helper the reproduction needs (a test that asserts on a component's own size rather than clicking it) |

## The acceptance criterion (MET)

One test, green in the normal UI suite:

```
cargo build -p winia --bin fixture_all --features debug-server
cargo test -p winia --features debug-server --test ui_test box_with_constraints_composes_its_content_at_measure_time
```

It asserts, in one run: the content prints the real cap on the **first** frame, the box has a
non-zero width and height, the cap change re-arranges the content in **both** directions, and exactly
one subcomposed content node exists after three frames (two live copies would claim one key and trip
the arena's dup-key guard). Disabling the fix that makes it pass turns it red — see the rounds below.

Baselines measured on this branch, for comparison after any change:

| command | reading |
|---|---|
| `cargo test -p winia --lib` | **1092 passed** (`v2` is 1082; the extras are the subcomposition tests) |
| `cargo test -p winia --features debug-server --test ui_test` | **48 passed, 0 ignored** — the acceptance test runs in the normal suite now |

Note on the UI suite's stability, measured while verifying this round: in full-suite runs
`a_long_press_fires_while_the_pointer_is_still_down` flakes under load — a paired A/B (twice with a
tree change, twice without) failed once on each side, and it passes every time when run in isolation
(`cargo test -p winia --features debug-server --test ui_test a_long_press_fires_while_the_pointer_is_still_down`).
Do not read a single red run of it as a regression; re-run the filter before believing it.

## What the earlier cross-frame rounds already fixed

Two causes of the empty later generation are gone (commit `24f2f06`), so a restart starts from a
different place than this file first described:

- **The content is re-runnable.** `BoxWithConstraints`' content is composed inside the measurement, so
  a later frame runs it again; `build` takes `Fn`. With `FnOnce` the second frame composed NOTHING
  (`[sub] ... nodes=0 root=None size=0x0`), and that `0x0` overwrote the size adoption had written.
  Locked by `a_later_frame_runs_the_content_again_and_the_parent_size_follows_it`.
- **A subcomposing node reaches layout as a change.** `MeasurePolicy::subcomposes()` plus seeding the
  node's `slot_key` into `layout_dirty_keys` at the end of `compose` — without it the parent folded
  before descending and the component kept the previous value's size (commit `c579ad3`).

## Round 3 (2026-09-26): why the reuse is not a small change — measured

The reuse needs the composition to survive the frame, and the two obvious routes are both blocked by
facts in the tree, checked this round:

1. **Adoption drains, so a cache holds an empty composer.** `adopt_parked` (`winia/src/ui/subcompose.rs`)
   sets its cache to `None` on purpose, with the reason written down: adoption MOVES the inner tree
   into the outer arena, so what would be left to cache is an empty composer. That is also what the
   unique-id test measures (`remember values across frames: [1, 2]`).
2. **Copying the tree instead of moving it needs a clonable node — it is not.** `LayoutNode` carries
   `on_remove: Option<Box<dyn FnOnce() + Send>>` (node.rs:215), plus `RefCell<Box<dyn Fn(..)>>`
   callbacks and `RefCell` fields, so it cannot be `Clone`; and its `measure_policy` is an index into
   `NodeArena.policies: Vec<Box<dyn MeasurePolicy>>` (node.rs:622), whose element type has no clone
   hook — a copy-adoption would have to map those indices and share or clone the policies.

So the piece that has to be decided first is **what exactly must survive a frame**. Copying the whole
node tree is the expensive answer (a clonable `LayoutNode`, or `Arc<dyn MeasurePolicy>` storage as the
rest of the repo already does for modifiers). The cheaper answer, and the one to try first, is to let
the TREE be rebuilt (it already is, correctly — the acceptance test passes) and carry only what the
rebuild cannot reproduce: the remembered state of the content, keyed by its call sites. That is a
question about `Composer`'s slot table, not about the arena.

## Round 5 (2026-09-26): the reuse LANDED — a subcomposition keeps its composition

`e751261` implements it, and the piece that was missing turned out to be the composer's own bookkeeping
rather than the slot table:

- The last composition per subcomposing node is kept (`SUBCOMPOSITION_CACHE`, in the facility's
  thread-local set). Adoption takes the inner `Composer` by `&mut` and hands it back after moving its
  TREE into the outer arena — the ARENA goes, the SLOT TABLE stays, and the next frame composes the
  content into that table.
- `Composer::prepare_subcomposition_for_recompose` clears the three pieces of per-node state that
  pointed into the drained arena: `prev_node_by_key`, the frame's reuse marks, and `arena.root`. Each
  one was a crash before being cleared (materialize's reuse arm, then `insert_reuse_key` via
  `collect_layout_index`). The slot table is deliberately untouched.

Measured, both directions: a unique id remembered by the subcomposed content returns `[1, 1]` (was
`[1, 2]` — a fresh composition every frame), and disabling the cache lookup turns that test red again.
The acceptance test still passes in the normal UI suite (48 passed, 0 ignored) and the library suite is
1092 passed.

## Round 4 (2026-09-26): the same route before the preparation — measured, then finished in Round 5

The cheap answer was tried: keep the inner `Composer` with the subcomposing node (a thread-local cache
keyed by that node's index), compose the next frame's content INTO it (a `remember` reads its value out
of the slot table by slot key, so the table is the thing that has to survive), and hand it back after
adoption. It compiles, and it does not work yet — the composer's own per-node state assumes the arena
is continuous across a re-arrangement, and adoption leaves that arena EMPTY:

- `index out of bounds: the len is 0 but the index is 0` at `materialize.rs:405` — the stale
  `prev_node_by_key` from the last `layout()` (which the reuse index is rebuilt from) still names the
  inner arena's old indices, and the next `compose` takes them for reusable nodes;
- clearing that index before the recompose moves the crash to `materialize.rs:682` (`[dup-key]`), where
  the tree walk meets the same emptiness from the other side.

So a reusable composer needs its arena story settled FIRST — either adoption that leaves the inner arena
usable (which is the node-copy problem above) or a composer mode that lays out from a slot table into a
fresh arena. Both are bigger than a facility patch, and that is the honest state of this piece: the
component works, the reuse is a framework-shaped piece of work with a named starting point. The
experiment is not in the tree (the branch is at `429d57f`, lib suite 1092 passed).

### The 2026-09-26 round that found the composition→measure link (fixed since)

That round traced the gap and its result is now the fix described above; it is kept here because the
trace technique is reusable: `WINIA_SUBCOMPOSE_TRACE`, driven through the fixture's stdin/pipe with
`c 51 36` then `c 123 36`, printed which keys a recompose marked, what materialize received, and
which nodes each layout pass reached.

## Where this goes next (the objective is done)

1. **The acceptance test is green and un-ignored; keep it that way.** Its commands are above, and
   disabling either of the two mechanisms behind it turns it red (the composition→measure seeding, and
   the reuse lookup).
2. **The remaining question is a MERGE question, not a defect:** does `v2` want `BoxWithConstraints`
   composed at measure time? If yes, this branch is the thing to merge, and the review should look at
   what the facility costs per frame (the content is re-composed whenever its node measures, and the
   content's `remember` state now survives that).
3. **The facility's natural second user is `TabRow`'s indicator slot** — the case the feasibility note
   records as genuinely needing measure-time composition. `BoxWithConstraints` was the first; a second
   user is what would show whether the facility's shape (`subcompose()` called from inside `measure`)
   is the right one to publish.
4. **Before adding that second user, re-run:** the `BoxWithConstraints` tests, the subcomposition tests
   in `subcompose.rs`, the lib suite (1092) and the UI suite (48, nothing ignored).

## What was already ruled out (do not re-try these)

- **Reusing the inner `Composer` without preparing it for a NEW composition** (cache it per node, call
  `compose` on it straight away, hand it back after adoption). Tried in Round 4: it crashes on the frame
  that reuses it because the composer's per-node bookkeeping still points into the arena adoption
  drained (`index out of bounds` at `materialize.rs:405`, then in `insert_reuse_key` reached from
  `collect_layout_index`). Fixed in Round 5 by clearing exactly those three pieces of state — so the
  lesson is "prepare the composer", not "do not reuse it".
- **Re-recording the inner composition's reads on the outer node's slot key** (so a state read inside
  the subcomposition marks the component for re-measurement). Tried in this round: it made the box's
  own reading correct, but it broke `a_long_press_fires_while_the_pointer_is_still_down` (the extra
  re-measures shifted the long-press timing) and did NOT turn the acceptance test green. Reverted —
  and reverted is the point: the mechanism is sound, its current attachment point is not.
- **Writing the parent's size at the adoption site.** Tried twice; measured; neither write moves the
  reading. Both attempts are still in the branch's code — they are paths to read, not templates.
- **A per-frame cache in the policy** (`first_measure`). It is already there, and it is what keeps the
  reading from flapping between two measurements in one frame; it does not fix the empty later
  generation.
- **Re-composing the same tree twice in one frame (design 1 / a true lookahead).** Blocked by
  `SlotTable::start_slot`'s visit semantics (`dirty` cleared on a match, `truncate` on a mismatch, and
  `collect_live_keys` keeping only visited slots). Forcing it means changing the flags the whole
  Skip/materialize/drain pipeline reads, and the failure mode is the tree corruption the `[dup-key]`
  guard exists to catch. It is not a cheap experiment.
- **Believing a smaller reading is stable.** `[98,48]` and `[420,160]` appear in the feasibility doc's
  historical paragraphs; both came from the verification example, where the box's own width is driven
  by its content through a different path than a window's. The UI fixture is the honest test. One
  wrong turn on this branch also came from reading a **stale binary** after a failed
  `cargo build --example` — confirm the binary was rebuilt before trusting its output.

## The cheaper alternative this branch also priced

A second `layout()` under different inputs costs **~191 µs on a 56-node tree (1.1 % of a 60 Hz
budget)**, once per flight start, and re-enters no slot. Anything that needs "where would this be if
the animation were not running" can start there without touching the subcomposition at all. That half
is available today and is recorded on `v2` as well.
