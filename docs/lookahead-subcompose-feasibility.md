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

## 5b. What the experiment found (branch `exp/lookahead-probe`, 2026-09-26)

Both steps of §5 were run on an experiment branch. Every claim below is a test in that branch's
`winia/src/ui/subcompose_probe.rs` (removed again before the branch was left in its final state; the
findings are what is kept).

**Step 1 — the extra `layout()` is cheap.** Driving 16 flights through `shared_transition_demo` and
timing the pass the frame handler already runs when a flight first attaches its override:
first layout ~405 µs (349–537), the extra pass **~191 µs (178–245)** on a 56-node tree — ~47 % of the
first pass, **1.1 % of a 60 Hz budget**, and once per flight start rather than per animating frame.
So "measure the same tree again under different inputs" is affordable; it is bounds discovery that
would be paid for this way, not the lookahead composition.

**Step 2 — a measure-time subcomposition works, and the blocker is somewhere other than predicted:**

| question | answer |
|---|---|
| does composing inside `measure` run? | yes — a fresh `Composer` composes + lays out inside a policy's `measure` and returns a real size |
| does it survive the TLS guards? | yes — the frame's context is intact afterwards (the test composes again on the same thread), and a **panic inside the subcomposition** leaves the outer arena untouched and the next subcomposition working |
| can its tree be adopted into the outer arena? | yes — nodes moved, child indices re-based, **policy pool appended and every `measure_policy` index re-based** (checked behaviourally against a decoy policy at the colliding index), root stamped with a synthetic key |
| can adoption happen inside `measure`? | **no** — `MeasurePolicy::measure` receives `&mut Vec<LayoutNode>`, not the arena, so it cannot reach `NodeArena::policies`. Adoption therefore has to be called from a site that has the arena: `Composer::layout` or `materialize` |
| does the adopted subtree survive the next frame? | **yes** — parented under its component's node and marked reused (the exact predicate the compose tail's prev-drain reads: `reused_nodes.contains(idx)` → `continue`), it is still in the arena after a second full compose + layout, and the frame still renders |

So the feasibility note's worry was misdirected: **design 2 is not blocked by the slot table — the
inner composition's own table is exactly what makes it safe. It is blocked by the measure trait's
signature**, and lifting that is a contained change (hand the policy the arena, or route adoption
through `layout`/`materialize` for components that declare a subcomposition), far smaller than the
visit-semantics change design 1 needs.

**What was not done in the experiment, and would be the next step if someone picks this up:** a
subcomposition adopted into a LIVE frame (the probe adopted into a local arena and measured there,
which is what proves the re-basing; a real component also has to parent the root under its own node
and survive the frame's prev-drain, which is the "reachability" half of §4). Cost also remains
unmeasured for a subcomposition on a real frame — the probe's tests are correctness tests, not
timings.

## 5c. The facility, and the one structural conflict left (branch `exp/lookahead-probe`)

The follow-up round built the facility for real — `winia/src/ui/subcompose.rs`, with:

- a thread-local marker (`measure_node` arms the node index while a measurement runs),
- a registry on the composer that a policy parks its composition in (reached through a
  `LayoutHostGuard`, the same shape as `ACTIVE_SLOT_KEY`/`GROUP_STACK`),
- an adoption pass in `Composer::layout` (move, re-base children AND policy indices, parent under the
  component's node, mark reused),
- and `BoxWithConstraints` rewritten on top of it, so its content now composes **in the measurement,
  with the real constraints, on the first frame** — the "one composition behind" deviation is gone,
  and its tests assert the first run already sees a finite maximum.

Two defects were measured on the way, both now pinned by tests:

1. **A subcomposing node must never fold.** Its subtree lives in the arena, so a folded frame lets
   materialize clear the parent's `children` and detach it. The node now carries a `subcomposed` flag
   that refuses the constant-fold arm (`the_box_is_measured_every_frame_so_its_content_is_not_detached`).
2. **The real-frame path has one structural conflict left, and it is where this stops being a
   wiring job.** The adopted subtree lives in `parent.children` but has **no descriptor** — it is not
   in the slot table. A reused parent therefore hits both sides of the problem at once:
   - materialize's reuse path does `children.clear()` and rebuilds them from descriptors, which drops
     the adopted child (it is then unreachable from `arena.root`, so `prune_stale_child_links` treats
     its listing as stale and the node leaks: measured on a real window, the arena held the adopted
     text node while the box's `children` was empty and the frame printed 3 nodes instead of 4);
   - keeping it instead and re-adopting a second copy trips `collect_layout_index`'s `[dup-key]` guard,
     because both copies carry the same key.

   Closing it needs the adopted subtree to **survive materialize**: either the parent records its
   subcomposed child (a node field, restored when the descriptor-driven children are re-attached), or
   the subcomposition stops writing into `parent.children` and the materialize walk learns to descend
   into it. Either way it is a change to the materialize/node contract — not the one-line wiring the
   probe suggested, and the last thing between this facility and a component that can ship.

So: the mechanism is proven end to end **inside a frame** (adoption runs, the box measures 101x48 from
its subcomposed content, the subtree is in the arena), and the remaining work is named and bounded.

## 6. Recommendation

- **Do not attempt design 1 (re-composing the same tree in one frame) first.** The blocker is a
  named function with named invariants (`start_slot`'s visit semantics + `visited`-based orphan
  collection), and its failure mode is tree corruption.
- **Price the animation-free second `layout()` first** — it is the cheapest thing that could give
  `animate_item`/lookahead-style information, and it re-enters no slot.
- **Then a minimal design-2 subcomposition** for the cases that genuinely need measure-time
  composition (`BoxWithConstraints`' scope, `TabRow`'s indicator slot). It uses mechanisms the engine
  already has: explicit keying for identity, `RuntimeFrameGuard` for re-entrancy, the existing
  materialize/arena path for the output.
- **Keep the frame-lagged approximation where it already works** (`LazyColumn`, shared elements):
  both have documented, tested workarounds, and the trail costs them little.
