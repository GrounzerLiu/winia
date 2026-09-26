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
>
> **This is the branch copy of the note (`exp/lookahead-probe`, frozen at `6551334`), so it carries
> the prototype's trail: §5c, §5d and "The defect's root cause" document the facility, the defects
> found on the way, and the one structural defect that stops it. The mainline's copy carries §5b and
> the recommendation without the code references that only exist here. Reading order for a newcomer:
> the status note in `docs/lookahead-probe-handover.md` first, then §5c → §5d → the root cause, then
> the handover's restart steps (it has the commands).**

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
>
> (Both steps were then run: see §5b for the numbers and §5c onwards for where they stopped.)

## 5b. What the experiment found (branch `exp/lookahead-probe`, 2026-09-26)

Both steps of §5 were run on an experiment branch. Every claim below is a test in this branch's
`winia/src/ui/subcompose_probe.rs`, which is still in the tree: the probe tests are kept, because they
are what define the facility's guarantees for whoever picks it up.

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

### 5d. What the materialize-contract round added, and where it stopped

The next round did the materialize work, and the subcomposed subtree now lives through a reused parent:

- `LayoutNode` gained `subcomposed_child` (the adopted child, kept OUT of the descriptor-driven child
  bookkeeping) and `subcomposed_measurements` (the subtree's measured geometry, relative to its root,
  replayed when the child is re-attached — without it the subtree comes back 0x0).
- `materialize`'s **both** reuse arms detach that child across the descriptor-driven rebuild and
  re-attach it after; the Skip arm's shape check subtracts it, or every reuse would look like a
  structure change and fall back to a rebuild.
- Adoption now releases the parent's PREVIOUS adopted subtree before inserting the new one: synthetic
  keys are derived from the parent key and the node's offset, so every frame's composition produces the
  same keys — correct for identity, but two live copies claim one key and trip `collect_node_keys`
  (measured: `[dup-key] … 覆盖了已有节点`, both carrying the adopted Text's key).
- `BoxWithConstraints` reports the subcomposed size as measured (clamping in both the policy and the
  engine made the reported width depend on which layer ran last).

Measured result: `cargo test -p winia --lib` 1090 passed, including tests that the content sees real
constraints on the FIRST frame and that the box stays sized on the second; and in a real window the
subcomposed text is in the tree at 98x48 with `BWC max 200`, and paints (9384 dark pixels inside the
box region of a 630x240 frame).

### The defect's root cause, observed (not inferred)

A round of *targeted observation* — logging every write to a node's measured size with the site that
made it, plus what each `subcompose()` call produced — settled it:

```
[bwc] policy: gen=1 -> composed, size=86x19            (frame 1: correct)
[bwc] policy: gen=2 -> cached from gen 1, refused as stale -> re-composed
[sub] composer: nodes=1 root=Some(0) size=86x19        (frame 1: the content composed)
[sub] composer: nodes=0 root=None size=0x0             (frame 2: the content composed NOTHING)
[size] measure_node:result: idx=9 -> 0x0               (and the policy overwrote the adopted 86x19)
```

So on a later compose generation the subcomposition comes out **empty**, the policy reports `0x0`, and
that overwrites the size the adoption pass had written. The cause is structural, not a missing line:
**adoption MOVES the composition's tree into the outer arena**, so there is nothing left to carry the
subcomposition into the next generation — a fresh `Composer` re-composes the content, and that content
lays out to nothing. Making the box's size correct across frames therefore needs the cross-frame reuse
this document already records as deferred ("the subcomposition is re-composed whenever its node
measures"), not another write at the adoption site. Two earlier attempts at that write were measured
and refuted before this observation was made; both are still in the code and neither moves the reading.

The reproduction is the `#[ignore]`d UI test in `winia/tests/ui_test.rs`, whose reason now carries this
root cause, and the fixture `bwc` in `tests/ui_fixtures/`.

**Correction to the paragraph that used to be here.** An earlier version of this section said the
window reading was resolved. It was not: the reading came from the `exp/lookahead-probe` verification
example (where the box's own width is driven by its content through a slightly different path), and
when the same thing was put into the UI suite as a fixture — the honest test of "does the app show
it" — the box's own node read `[0,0]` while its adopted child read `[192,19]`.

What is measured, in the app's frame path:

| reading | value |
|---|---|
| the subcomposed content's text | `BWC max 200` — the real cap, on the FIRST frame (asserted by the fixture's passing assertions) |
| the adopted child's size | `[192,19]` — real |
| the box's OWN measured size | `[0,0]` — **wrong**, and the unit tests that assert it pass |

So the subcomposition works, the content is composed with the real constraints at measure time, and
the parent's own size is the open defect. Fixing it started from two hypotheses, both of which the
measurements refuted (the policy's per-frame cache, and adoption's write of the parent size are both
already in place and neither moves the reading), and several wrong turns were made on the way — one of
them reading output from a **stale binary** because `cargo build --example` had failed and the failure
was not read. The reproduction is committed as an `#[ignore]`d UI test with its reason, so a green run
of that test is the acceptance criterion for the fix, and `docs/ui-testing.md`'s guidance about
`click_until` applies to it (the fixture's cap buttons are clicked through it).

**What the earlier paragraph described, for the record:** the box read `[0, 0]` while its child read `[98, 48]` because
the policy composed a FRESH subcomposition on every measurement, and the frame handler measures such a
node more than once per frame (the flight-override pass). The first composition is the one the adoption
pass attaches; the second replaced it and reported a size from a tree nobody would see. The policy now
reports its first measurement of the frame instead of composing again
(`first_measure`), and the window is stable across runs:

```
tree: [420,160] root -> [420,160] pad -> [98,31] box -> [98,31] BWC max 200
screenshot 630x240: 14400 dark pixels inside the box region
```

So `BoxWithConstraints` now does what Compose's does — content composed in the measurement, with the
real constraints, on the first frame, sized to that content — and the facility is exercised by a real
component rather than only by tests.

**What is still not there** (and would be the next round's work if the facility is to be used more
widely): the subcomposition is re-composed whenever its node measures, so a settled subtree is still
re-built each frame; the arrangement cache (`Composer::subcomposition_cache`) is stubbed with its
reason; and nothing else in the framework uses the facility yet — `LazyColumn` still runs on its anchor
model, and `TabRow`'s indicator slot is untouched.

## This branch, and that it is frozen

Everything above is an experiment: the code stays on `exp/lookahead-probe` and is **frozen at
`6551334`**, deliberately not merged, because `BoxWithConstraints` here reads `[0,0]` for its own size
in a real window while its adopted child reads `[192,19]` — a component with a known-wrong reading is
worse than the frame-lagged one `v2` ships. What the branch is good for: the facility and its five
guarantee tests, the materialize/node contract work the adopted subtree needed, a priced alternative
(a second `layout()`, §5b), and a reproduction that defines the acceptance criterion.

Restart instructions — exact commands, test names, baselines, what was already ruled out — are in
`docs/lookahead-probe-handover.md`. That file is the entry point; this note is the reasoning behind it.

## 6. Recommendation

Both parts of §5 were run, so this is no longer a plan but a reading of the results:

- **Do not attempt design 1 (re-composing the same tree in one frame) first.** The blocker is a named
  function with named invariants (`start_slot`'s visit semantics plus `visited`-based orphan
  collection) and its failure mode is tree corruption.
- **The cheap half is priced and worth using on its own:** a second `layout()` under different inputs
  costs ~191 µs on a 56-node tree (1.1 % of a 60 Hz budget) and re-enters no slot. Anything that needs
  "where would this be if the animation were not running" can start there.
- **Design 2 is feasible but has one prerequisite, now named: cross-frame reuse of a composition.**
  The prototype composed at measure time, adopted the tree into the arena, and survived a frame —
  until a later compose generation came out empty (§5d). The prerequisite is structural: keep the
  composition alive between frames and re-arrange it, instead of re-creating it and moving it. Once it
  exists, `BoxWithConstraints`' scope and `TabRow`'s indicator slot follow.
- **Keep the frame-lagged approximation where it already works** (`LazyColumn`, the shared-element
  bounds): both have documented, tested workarounds, and the trail costs them little.

The prototype, its tests and its reproduction are on this branch; the mainline's tree does not contain
them.
