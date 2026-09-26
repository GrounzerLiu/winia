# `exp/lookahead-probe` — frozen, and how to pick it up

> Status: **frozen at commit `6551334`; the cross-frame round since then (`24f2f06`, `a06774b`) fixed
> two of the three causes it was frozen for, so the freeze now covers a narrower defect.** Nothing here
> is merged into `v2`, and nothing here should be merged as it stands: in a real window the box still
> does not follow a later change to its caller's state (the composition runs, the measurement does
> not), which is worse than the frame-lagged version `v2` ships. The branch is kept as a **worked
> prototype plus a reproduction**, not as shippable code.
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
| `winia/tests/ui_test.rs` | the reproduction, `#[ignore]`d, reason = the root cause; its green run is the acceptance criterion |
| `winia/tests/ui/mod.rs` | `UiTest::find_tag_size`, the helper the reproduction needs (a test that asserts on a component's own size rather than clicking it) |

## The acceptance criterion

One test, currently `#[ignore]`d, and it must run green:

```
cargo build -p winia --bin fixture_all --features debug-server
cargo test -p winia --features debug-server --test ui_test box_with_constraints_composes_its_content_at_measure_time -- --ignored
```

It un-ignores as part of the fix (the attribute's reason text exists only to carry the root cause until
then). It asserts, in one run: the content prints the real cap on the **first** frame, the box has a
non-zero width and height, the cap change re-arranges the content in **both** directions, and exactly
one subcomposed content node exists after three frames (two live copies would claim one key and trip
the arena's dup-key guard).

Baselines measured on this branch, for comparison after any change:

| command | reading |
|---|---|
| `cargo test -p winia --lib` | **1091 passed** (`v2` is 1082; the extra one is the content re-run test) |
| `cargo test -p winia --features debug-server --test ui_test` | **47 passed, 1 ignored** |

Note on the UI suite's stability, measured while verifying this round: in full-suite runs
`a_long_press_fires_while_the_pointer_is_still_down` flakes under load — a paired A/B (twice with a
tree change, twice without) failed once on each side, and it passes every time when run in isolation
(`cargo test -p winia --features debug-server --test ui_test a_long_press_fires_while_the_pointer_is_still_down`).
Do not read a single red run of it as a regression; re-run the filter before believing it.

## What the cross-frame round has ALREADY fixed (commit `24f2f06`)

Two causes of the empty later generation are gone, so the restart starts from a different place than
this file first described:

- **The content is re-runnable.** `BoxWithConstraints`' content is composed inside the measurement, so
  a later frame runs it again; `build` takes `Fn`. With `FnOnce` the second frame composed NOTHING
  (`[sub] ... nodes=0 root=None size=0x0`), and that `0x0` overwrote the size adoption had written.
  Locked by `a_later_frame_runs_the_content_again_and_the_parent_size_follows_it`.
- **A subcomposing node is not folded.** `materialize`'s reuse arm marks it dirty (`measure_node`
  already refused to fold one), so its composition is re-arranged rather than skipped.

The remaining defect, measured after those two: on a frame where the component's body recomposes but
the node is materialized with `dirty=false`, `measure_node` folds and the component is NOT
re-measured — in a window the body ran (traced), the new modifier arrived (`max_width=Fixed(200)`),
and the box's reported size never followed. The acceptance test fails on exactly that.

## The restart order, if this is picked up

1. **Re-confirm the defect is still the one described.** Run the `--ignored` test above and read the
   box's own size out of the failure. If it is not `0x0` anymore, the sections below are stale —
   re-measure before acting.
2. **Start from the composition→measure link, not from the arena.** The remaining gap is that a
   recomposition does not make the node re-measure (see the section above). Read, in this order:
   `Composer::layout`'s fold check (`winia/src/layout/node.rs`, `measure_node`) and the descriptor's
   `dirty` flag as `materialize` receives it (`winia/src/core/composer.rs`, `collect_desc_tree`).
   The question to answer with a measurement is whether the slot that recomposed should have carried
   `dirty=true` into materialize, and why it did not on that frame.
3. **Then the cross-frame piece this file originally pointed at:** keep the composition alive between
   frames and **re-arrange** it, instead of re-creating it per measurement and moving it. The named
   place is `Composer::subcomposition_cache` (a stub in `subcompose.rs` with its reason written down).
   Note what that costs: adoption MOVES the inner tree into the outer arena, so a re-usable cache needs
   the inner side to survive the move (either a copy, or an adoption that leaves the source intact) —
   and `MeasurePolicy` has no clone hook, which is the first thing to settle.
4. **Then, and only then, re-run three things:** the `--ignored` test; the tests in
   `subcompose_probe.rs`; the `BoxWithConstraints` tests (two of them assert first-frame constraints
   and second-frame sizing, which is where a stale cache will show up first). The lib count should stay
   at 1091 unless tests are added.
5. **If it goes green, the merge question is a component question, not a facility question:**
   `BoxWithConstraints` becomes the second component on the facility, and `TabRow`'s indicator slot is
   the natural third. Merge only what is exercised.

## What was already ruled out (do not re-try these)

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
