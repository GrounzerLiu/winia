# `exp/lookahead-probe` — frozen, and how to pick it up

> Status: **frozen at commit `6551334`.** Nothing here is merged into `v2`, and nothing here should be
> merged as it stands: `BoxWithConstraints` on this branch reads `[0,0]` for its own size in a real
> window's frame path (its adopted child reads `[192,19]`), which is worse than the frame-lagged
> version `v2` ships. The branch is kept as a **worked prototype plus a reproduction**, not as
> shippable code.
>
> Reasoning and findings: `docs/lookahead-subcompose-feasibility.md` (this branch's copy carries the
> full trail; the mainline's copy at `7e66804` carries the result and the recommendation without the
> code references). Read §5c, §5d and "The defect's root cause" in that order, then come back here for
> the commands.

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
| `cargo test -p winia --lib` | **1090 passed** (`v2` is 1082) |
| `cargo test -p winia --features debug-server --test ui_test` | **47 passed, 1 ignored** |

## The restart order, if this is picked up

1. **Re-confirm the defect is still the one described.** Run the `--ignored` test above and read the
   box's own size out of the failure. If it is not `0x0` anymore, the sections below are stale —
   re-measure before acting.
2. **Read the two mechanisms the fix has to reconcile**, in this order: (a) `Composer::layout`'s
   adoption pass (`winia/src/ui/subcompose.rs`) — it MOVES the composition's tree into the outer
   arena, which is what leaves the subcomposition empty on a later generation; (b) materialize's
   descriptor-driven child rebuild (`winia/src/core/materialize.rs`) — the adopted child has no
   descriptor, so it needs the detach/reattach arms the branch already added for it.
3. **Build the piece the fix needs: cross-frame reuse of a composition.** The composition has to stay
   alive between frames and be **re-arranged** (re-measured with the new constraints), instead of being
   re-created per measurement and moved. The named place to start is
   `Composer::subcomposition_cache`, currently a stub in `subcompose.rs` with its reason written down:
   a per-parent cache of the last subcomposition, keyed by (parent key, content identity), reused when
   the content function has not changed and the node measures again.
4. **Then, and only then, re-run three things:** the `--ignored` test; the 5 tests in
   `subcompose_probe.rs`; the 4 `BoxWithConstraints` tests (two of them assert first-frame constraints
   and second-frame sizing, which is where a stale cache will show up first). The lib count should stay
   at 1090 unless tests are added.
5. **If it goes green, the merge question is a component question, not a facility question:**
   `BoxWithConstraints` becomes the second component on the facility, and `TabRow`'s indicator slot is
   the natural third. Merge only what is exercised.

## What was already ruled out (do not re-try these)

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
