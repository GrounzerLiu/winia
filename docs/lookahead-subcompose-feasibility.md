# Lookahead / SubcomposeLayout — feasibility

> Status: **research note, no code changed.** Question asked: can winia have a lookahead pass (or a
> measure-time subcomposition), the mechanism behind several recorded deviations? The note records
> what the engine already provides, which of the two designs the slot machinery can actually carry,
> the concrete failure the other one hits, and what would have to be measured before either is built.
>
> Motivation, all recorded elsewhere: `docs/box-with-constraints.md` (the scope reads its constraints
> one composition late), `docs/shared-element-transition.md` §3.1 (bounds discovered without a
> lookahead pass, "substitutable"), `docs/animation-gap-analysis.md` (`animate_item` dropped: "needs
> lookahead or layer-shift"), `docs/tab-row.md` §7 (no SubcomposeLayout, so a custom indicator slot
> cannot be injected from the measure-time positions), `docs/shared-element-gaps.md`
> (`skipToLookaheadSize`, "no lookahead system exists").

## 1. What Compose's two mechanisms actually do

They are different tools with different insertion points:

| | where it composes | what it is for |
|---|---|---|
| **Lookahead pass** | a *second* measurement pass, before the real one, over the same composition | "measure it as if the animation were not running", so the real pass knows where things will end up. Used by shared elements, `animateItem`, `LookaheadScope`. |
| **SubcomposeLayout** | composition *inside* measurement, for a subset of content chosen using the measured constraints | "compose the children I now know I need": `LazyColumn`'s visible items, `TabRow`'s indicator fed with tab positions, `BoxWithConstraints`' scope. |

winia has neither. What it has instead, in each case, is a **frame-lagged approximation**: an anchor
state or a backchannel written during measurement and read by the next composition
(`LazyColumn`'s `first_visible_index`, `BoxWithConstraints`' constraint state).

## 2. What the engine already provides (the reusable half)

This is more than a naive reading of "compose first, measure after" suggests:

1. **Slot identity is position-based and runtime-built.** A slot key is
   `mix_key(call-site hash, per-base counter)`, and the call-site hash for a loop iteration is
   `fnv(scope, statement id, sibling_position)` where `sibling_position` is a FNV hash of the slot
   tree's own `child_counters` chain (`composer.rs`, `SlotTable::sibling_position`). The iteration
   identity is therefore **built while composing, from the slot tree's structure** — not from a
   source-level index that a second composition into the same tree could not reproduce.
2. **Explicit keying already exists for contexts where the implicit chain is not enough.**
   `ctx.key(id, f)` pins a subtree's base to a caller-supplied id, and `start_scope_keyed(source_hash)`
   does it for a scope. That is exactly the tool a synthetic subcomposition root needs: the content it
   composes lives at a call site that has no statement identity of its own.
3. **Measurement already mutates nodes.** `measure_node` writes the arena in 84 places (sizes, cached
   constraints, scroll metrics, text snapshots), and `LayoutTransaction` already snapshots
   `nodes_len`/`free_nodes`/per-node state/`root` for rollback. So "the arena changes during measure"
   is not a new class of change.
4. **Nested and re-entrant composing is already handled.** `RuntimeFrameGuard` swaps out
   `ACTIVE_SLOT_KEY`/`GROUP_STACK`/`STMT_STACK`/`MEASURED_LAYOUT_KEYS` on entry and restores them on
   exit or panic, and there are tests for nested `Composer` calls. A composer that composes while
   measuring is the same shape.
5. **A bounded same-frame re-layout hook exists.** The frame handler already takes one extra
   `layout()` pass when a shared flight attached a layout override for the first time
   (`take_layout_override_fresh`, `app.rs`). That is the closest thing to a second pass winia runs
   today, and it is the natural attachment point for a lookahead experiment.

## 3. Why the lookahead variant (design 1) does not work as-is

Rewriting the same tree twice inside one frame is **not possible with the current slot table**, and
the reason is one function. `SlotTable::start_slot` is a *destructive visitor*:

- it takes the next index from `child_counters`, and on a match marks the slot `visited = true` and
  **clears its `dirty` flag** (`parent.children[idx].dirty = false`);
- on a *mismatch* it **truncates the parent's children** at that index (`parent.children.truncate(idx)`)
  and pushes a fresh slot;
- `collect_live_keys` later keeps only slots with `visited`, so a slot that did not take a visit this
  frame is treated as removed and its node is freed.

Consequences for a second pass in the same frame:

- The slots the first pass entered are no longer `dirty`, so the second pass returns `Clean` and
  re-runs nothing — the "lookahead composition" would be a no-op.
- Forcing the second pass to re-enter means clearing `visited`/`dirty` for the subtree between passes
  and teaching `start_slot` that a slot can be visited twice in one pass — i.e. changing the meaning
  of the two flags the entire Skip/materialize/drain pipeline reads. The `[dup-key]` guard, the
  `visited`-based orphan collection and the prev-drain all key off them.

So design 1 is not a cheap experiment: it is a change to the slot table's visit semantics, with the
failure mode being exactly the tree corruption the existing guards were built to catch.

## 4. The design the machinery does support (design 2: a side table)

Compose's SubcomposeLayout is also a **separate composition**, and that is the part winia can carry:

- A `SubcomposeLayout`-shaped component owns its own composition — a second `Composer` (or a nested
  `SlotTable`) — so the content's identity comes from *that* table's call-site hashes and counters and
  cannot collide with the outer one's slots. `ctx.key(id, f)` / `start_scope_keyed(source_hash)` stay
  useful for the case where the content is composed into the outer table instead, or where the
  component wants to pin identity across a set of passes it numbers itself — the mechanism
  `docs/tab-row.md` §7 already points at for a measure-fed slot.
- The inner composition needs what a normal child needs and no more: it runs inside the frame's
  existing TLS guard (`RuntimeFrameGuard` swaps and restores `ACTIVE_SLOT_KEY`/`GROUP_STACK`/
  `STMT_STACK` on entry, exit and panic, with tests for nested composers), and its output has to
  become arena nodes, because they are drawn and hit-tested like any other node.
- **That last part is the real integration work, and it is in materialize, not in the slot table.**
  A node that draws has to be reachable from `arena.root` and re-materialized every frame, or the
  compose tail's prev-drain will reclaim it (`prev_node_by_key` + `reused_nodes` decide that, and a
  node that is neither re-materialized nor marked reused is freed by design — measured this session
  while pricing the same mechanism for the flight ghosts). So a subcomposed subtree must either be
  handed to the outer `materialize` as a prebuilt descriptor (the cleanest shape: an `Option<usize>`
  on `DescNode` saying "this child already exists"), or be added to `reused_nodes` so the drain leaves
  it alone. Which of the two composes better with the outer table's Skip/claim path is the first
  thing a probe should answer.

What this buys, per motivating case:

| case | design 2 helps? |
|---|---|
| `BoxWithConstraints` scope | yes — the content can compose inside the measure, with the real constraints |
| `TabRow` custom indicator fed with `tabPositions` | yes — that is the textbook SubcomposeLayout use |
| `LazyColumn` visible items | marginal — the anchor model already works; it would remove the one-frame trail on fast flings |
| `animateItem` (add/remove/move) | partly — it needs the *previous* sizes, which is lookahead |
| shared elements' bounds | no — already solved without it (`docs/shared-element-transition.md` §3.1) |

And what a real lookahead (bounds, `animateItem`) would still need on top: the *provisional*
measurement of the tree that will be composed next, which is design 1's problem again — unless it is
expressed as "measure the unchanged tree under the animation-free constraints", which is a second
`layout()` call with different inputs rather than a second composition. That variant is worth pricing
before design 1 is declared impossible: it does not re-enter any slot.

## 5. What to measure before writing either

The honest next step is a probe round, not an implementation:

1. **Price the second `layout()`.** Take the demo, call `layout(animation_free_constraints)` then
   `layout(real_constraints)` on a frame where a flight starts, and measure. `app.rs` already runs a
   second pass on exactly those frames, so the incremental cost is knowable today.
2. **Price a minimal design-2 component.** A `SubcomposeProbe` component with its own `Composer`,
   composing two children from a constraint it just measured, materializing them as real nodes.
   If the arena side is as clean as this note reads, this is a day-sized experiment; if it is not,
   the experiment says so cheaply.
3. **Check the failure modes that matter**: whether a subcomposed subtree survives the outer frame's
   prev-drain without being marked reused (that is the mechanism that decides it, and the probe in
   point 2 either confirms the "prebuilt descriptor" route or shows it needs the other one), and
   whether a panic inside the inner compose rolls back the outer `LayoutTransaction` correctly.

## 5b. The experiment that was run — and its result (branch-only)

> **This section describes work that lives on the branch `exp/lookahead-probe` only.** The code it
> talks about (`ui::subcompose`, the `BoxWithConstraints` rewrite, the materialize-contract change) is
> NOT in this branch's tree. What follows is the finding, kept here so nobody repeats it; the code is
> on that branch for whoever picks it up.

The design-2 direction was prototyped end to end on that branch. It works far enough to be worth
recording, and it stops at one structural place:

1. **A measure-time subcomposition can be built and adopted.** A policy composes its content into its
   own `Composer` during `measure`, parks it, and an adoption pass in `Composer::layout` moves the tree
   into the outer arena — re-basing child indices AND policy indices, attaching under the component's
   node, marking the subtree reused. The branch's tests cover composing inside a measure call, its TLS
   isolation, a panic inside it, and adoption's index re-basing.
2. **The extra layout a lookahead would need is affordable.** Timed on a real window's flight-start
   frames (the frame handler already runs a second `layout()` there): first pass ~405 µs, the extra
   pass ~191 µs on a 56-node tree — 1.1 % of a 60 Hz budget, once per flight start.
3. **Re-composing the same tree in one frame (the lookahead shape) is blocked by the slot table.**
   `SlotTable::start_slot` is a destructive visitor — it clears `dirty` on a match, truncates on a
   mismatch, and `collect_live_keys` keeps only visited slots — so a second pass over the same tree
   re-runs nothing, and forcing it changes the flags the whole Skip/materialize/drain pipeline reads.
   That is a visit-semantics change whose failure mode is the corruption the `[dup-key]` guard exists
   to catch.
4. **The subcomposition path stops at a structural defect, observed rather than guessed.** On a later
   compose generation the subcomposition composes NOTHING (measured: `nodes=0`, `root=None`), so the
   policy reports `0x0` and overwrites the size the adoption pass wrote — the box reads `[0,0]` while
   its adopted child reads `[192,19]`. The cause: adoption MOVES the composition's tree into the outer
   arena, so nothing carries the subcomposition across generations. Fixing it needs cross-frame reuse
   (keep the composition alive between frames and re-arrange it) — a structural piece of work, not
   another write at the adoption site: two such writes were tried and measured false.
5. **What it would buy, and what is already solved without it.** `BoxWithConstraints`' scope and
   `TabRow`'s indicator slot genuinely need measure-time composition; `LazyColumn`'s visible items and
   the shared-element bounds discovery do NOT — both have documented, tested workarounds
   (`docs/shared-element-transition.md` §3.1).

So: feasible and prototyped, the cheap half of the lookahead idea is priced, and the remaining work is
named (cross-frame reuse of a composition) with a reproduction on that branch
(`winia/tests/ui_test.rs`'s `#[ignore]`d `box_with_constraints_composes_its_content_at_measure_time`
plus the `bwc` fixture).

## 6. Recommendation

What the experiment (§5b) leaves behind:

- **Design 1 (re-composing the same tree in one frame) should not be attempted first.** Its blocker is
  a named function with named invariants (`start_slot`'s visit semantics plus `visited`-based orphan
  collection) and its failure mode is tree corruption.
- **The cheap half is priced and worth using on its own:** a second `layout()` under different inputs
  costs ~191 µs on a 56-node tree (1.1 % of a 60 Hz budget) and re-enters no slot. Anything that needs
  "where would this be if the animation were not running" can start there.
- **Design 2 is feasible but has one prerequisite, now named: `cross-frame reuse of a composition`.**
  The prototype composed at measure time, adopted the tree into the arena, and survived a frame — until
  a later compose generation (the observed root cause in §5b.4). That prerequisite is structural: the
  composition has to stay alive between frames and be re-arranged, instead of being re-created and
  moved. Once it exists, `BoxWithConstraints`' scope and `TabRow`'s indicator slot follow.
- **Keep the frame-lagged approximation where it already works** (`LazyColumn`, the shared-element
  bounds): both have documented, tested workarounds, and the trail costs them little.

The prototype, its tests and its reproduction are on `exp/lookahead-probe`, **frozen at `6551334`** and
deliberately not merged; this branch's tree does not contain them.

## 7. If this work is picked up

Start at `docs/lookahead-probe-handover.md` — but note that it sits **on that branch**, not here (this
branch has no code to point at, so the file would dangle until a fix lands). It carries what a restart
needs, and what one would not want to rediscover:

- **The acceptance criterion**, already written as a test: the `#[ignore]`d UI test
  `box_with_constraints_composes_its_content_at_measure_time` with the `bwc` fixture, plus the exact
  commands for the one fixture binary and the UI suite.
- **The restart order**, starting from `Composer::subcomposition_cache` — the branch's stub for exactly
  the missing piece, cross-frame reuse of a composition.
- **The baselines** measured on that branch: `cargo test -p winia --lib` 1090 passed; the UI suite 47
  passed with the 1 ignored reproduction.
- **The four things already ruled out** by measurement — including the two writes at the adoption site
  that were tried and refuted, and the reason design 1 is not a cheap experiment.

The branch is a prototype plus a reproduction, not shippable code: on it `BoxWithConstraints` reads
`[0,0]` for its own size in a real window while its adopted child reads `[192,19]`. That is why it is
frozen rather than merged, and why the reproduction is the acceptance criterion rather than a caveat.
