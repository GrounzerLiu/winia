# Benchmarks

```bash
cargo bench -p winia
```

`benches/recompose.rs`, one binary, no benchmark framework. It exists to answer one question with a
reproducible figure: **does a targeted update cost what the updated subtree costs, or what the whole
tree costs?**

Machine these figures came from: AMD Ryzen 9 8940HX, 15 GB RAM, `rustc 1.98.1`, Windows.
`cargo bench` uses the `bench` profile, and the run prints `debug_assertions = false` — load-bearing,
because the composer carries `#[cfg(debug_assertions)]` instrumentation that calls `std::env::var` per
skipped group. If that stamp ever reads `true`, every figure below is instrumented rather than
production and must not be compared with these.

**Read the fast sample, not the median.** The same frame varied by 2-3x run to run on this machine
(an idle 800-row frame landed anywhere from 2.7 ms to 7.5 ms while other work was running), and noise
only ever ADDS time. The reported figure is the fastest of 9 samples; the median is printed beside it
to show how loud the machine was.

## What holds exactly: the scoping

The `entered` column is the point of the benchmark. It says how many of the tree's groups actually ran
— without it a fast timing could just mean nothing happened.

| scene (800 rows) | entered / frame |
|---|---|
| cold frame (fresh tree) | 800 |
| idle frame | **0.00** |
| one row's state moved | **1.00** |

Exactly one group re-enters, and an idle frame re-enters none, at every size (50 / 200 / 800 rows).
The claim that recomposition is scoped to the changed subtree is **true as stated**.

## The frame is O(tree), and now it is linear

Five rows in the fast column are µs/frame. Boxes = sized containers only; text = a `Text` per row.
These are the figures after the fixes in this document.

| rows | cold | idle | one row moved |
|---|---|---|---|
| 50 | 236 | 33 | 63 |
| 200 | 983 | 134 | 260 |
| 800 | 4594 | 597 | 1142 |

| rows (text) | cold | idle | one row moved |
|---|---|---|---|
| 50 | 715 | 46 | 77 |
| 200 | 4876 | 198 | 345 |
| 800 | 26168 | 1618 | 2326 |

The cold column has moved in the last three fixes: the twelfth stopped the arena growing by doubling,
the thirteenth removed the last per-node SipHashes, the fourteenth the per-container modifier copy — all
three are Enter-path costs, which is why the cold frame (every group entering) moves most and the idle
frame (no group entering) does not move at all.

16x the rows costs ~18x an idle frame and ~20x a one-row update. In the original figures recorded here
(before any of the fixes in this document) the same two ratios were 28x and 70x — the difference was a
quadratic term, and what remains is the per-frame walk over the tree, which is what the design says it
is. Entering ONE group still does not make the frame cheap: the walk that finds that group is the frame.

Eight runs were taken for these figures (the machine was loaded for some of them — its text cold frame
read 38-42 ms against 26 ms, and noise only ever ADDS time), so each cell above is the fastest of the
eight. The text scene is also intrinsically the noisier of the two, so a single text figure should not
be read as precise.

The breakdown at 800 rows (boxes) says where it goes:

| | compose | layout |
|---|---|---|
| idle | 421 | 218 |
| one row moved | 832 | ~320 |

and the control that splits composition's extra into "the walk" and "the update" — a state the
CONTAINER reads moves, so the container re-enters and the row loop runs while every row's own parameter
is unchanged:

| compose, boxes 800 rows | fast sample | groups entered |
|---|---|---|
| idle (container Skips, so the loop does not run) | 421 | 0 |
| container dirty, every row Skips | 835 | 0 |
| one row dirty (the same loop + one rebuild) | 839 | 1 |

The loop over 800 rows costs **~415 µs**, and re-entering one row inside it costs **nothing measurable**
(~4 µs here, i.e. run-to-run noise). The update is free relative to the walk that finds it. The loop's
own 415 µs has come down from ~490 µs as the per-node costs the later rounds removed stopped being paid
800 times over; its internal breakdown is instrumented and therefore only indicative (the thirteenth and
sixteenth rounds' sections explain why: a probe that costs ~35 ns per point dominates a bucket whose
calls are ~50 ns).

### Correction: the 5 ms was NOT the Skip decision

An earlier version of this document (and the work that followed it) attributed the one-row update's
extra ~5 ms to "799 Skip decisions at ~6 µs each". **That number was wrong in the same way the layout
artifact further down was wrong: it came from dividing a section total by a count.** The phase split
showed 5.0 of 5.2 ms inside the content closure, the closure contains the row loop, the loop runs 800
times — and the quotient was read as a per-iteration cost without ever measuring an iteration.

Measuring the iteration says otherwise. With a temporary per-call profiler (`WINIA_SKIP_PROF`, kept as
`target/probe/skip_prof.patch`), at 800 rows with one dirty row:

| per call | before the fix | after |
|---|---|---|
| `State::get` (the loop's read) | **6221 ns** | **105 ns** |
| `changed()` (param declaration) | 284 ns | 75 ns |
| `start_restartable_group` (slot lookup + Skip decision + slot writes) | 587 ns | 323 ns |
| `next_key()` | 124 ns | 48 ns |
| the whole frame's 800 reads | **4989 µs** | 84 µs |

The 6 µs was a **read**, not a Skip decision: 800 reads cost 5.0 ms of a 7.4 ms compose. The Skip
machinery was ~1.1 µs per row all along (~0.9 ms for the loop), so attacking it first would have bought
very little.

### The mechanism: quadratic subscription bookkeeping

`State::get` records a dependency, and `ComposerSubscription::subscribe_signal` — the "is this signal
already tracked?" step — **scanned every signal the composer had ever read, twice per read** (`retain`
plus `any`, each with a `Weak::upgrade` per entry). A list of N rows reading N states therefore cost
O(N²) weak-pointer upgrades per frame: at 800 rows, 800 × 800 × 2 ≈ 1.3M, ~5 ms. The scan's answer was
almost always the same (`yes, tracked`) — it asked a membership question of a container that had no
index.

### The fix

`ComposerSubscription::signals` is a `HashMap<StateId, Weak<StateSignal>>` keyed by signal id, so the
question is a hash lookup and the answer is stored once per signal. Dead entries (a dropped State's
weak) are overwritten on the next read of that id, and dropped by the per-frame `retain_signals` and by
`unsubscribe_all` at teardown — the two places that already iterated the container for real work.

| arm | before | after | |
|---|---|---|---|
| one row updated, 800 rows (boxes) | 7701 µs | **2746 µs** | -64% |
| cold frame, 800 rows (boxes) | 9716 µs | **7156 µs** | -26% |
| idle frame, 800 rows (boxes) | 1998 µs | **1927 µs** | flat, as it should be |

("before" is the state after the layout-transaction fix, so the two compose: 11393 µs → 2746 µs on the
box one-row update, -76% overall. The text scene moved the same way — at 800 rows, one row updated
14315 µs originally and 3897 µs now, cold 43581 µs and 28440 µs — but its "before" there is the
original figure rather than post-layout-fix, so it is quoted here rather than tabulated.)

Two regression tests came with it, of two deliberately different kinds:

* `repeated_reads_track_one_entry_per_signal` — deterministic, pins the container's shape (repeat reads
  must not grow it).
* `reading_many_states_does_not_rescan_every_tracked_signal` — a **ratio**, deliberately loose: 40x the
  states may not cost more than 200x the time. Linear work measured ~40x, and the old shape was verified
  to fail it by reinstating the scans (774 µs vs 1.004 s, a 1300x ratio). It is in the suite because this
  defect was invisible to every correctness assertion in the repo.

### A second fix, from the round before this one: the layout transaction's fields

`LayoutTransaction::new` runs on every `layout()` call and snapshots the arena so a panic mid-layout can
roll back. It used to snapshot each node's `modifier` (a `Vec`, and through a `TextContent`'s `String` a
string per text node) and `children` (another `Vec`) — fields layout cannot touch, because they are
composition products written by `materialize` while composing, and the transaction's lifetime sits
inside `layout()`. At 4001 nodes the phase split put that clone at 1.2 ms — larger than measurement
itself (measure: 115 µs, `collect_nodes`: 700 µs, transaction: 1200 µs).

Snapshotting only the fields layout writes took an 800-row idle frame from **3452 µs to 1998 µs (-42%)**
and a one-row update from **11393 µs to 7701 µs (-32%)**, with the full library suite green. The fourth
fix below replaced the next-most-expensive part of the same snapshot, the two whole-tree maps.

### A third fix: don't rebuild a graph that did not change

The same shape of defect, one level up. Both `reconcile_compose_deps` and the layout tail derive the
reverse dependency index (`slot_deps`, `layout_deps`) from the forward one (`compose_slot_reads`,
`layout_slot_reads`) by clearing it and rebuilding it from scratch — and they did that on every frame,
including the frames where the forward graph is exactly what it already was. That is the common case:
an idle frame records no reads at all (`recorded=0`, verified with the profiler), and the frame where
one row's state moved re-records the *same* 800 reads, so the set is unchanged.

Both now compare the new read set with the stored one per entered slot (and watch for dead-slot removals)
and only rebuild when something actually moved. `cleanup_signal_subscriptions` still runs every frame
from the layout path with the union of both graphs, so the subscriptions are still pruned exactly once
per frame.

| instrumented bucket, boxes 800 rows | idle compose | one row updated |
|---|---|---|
| reconcile (rebuild + cleanup) | 110 µs → **3.9 µs** | 190 µs → **131 µs** |
| frame-level figure | within noise (±10%) | within noise (±10%) |

The frame-level effect is **inside this machine's run-to-run noise**, and that is worth saying plainly
rather than quoting one lucky run: the profile is the evidence here, not the headline timer. The
reason it is still worth keeping is that the rebuild's cost is proportional to the *dependency graph*,
not to the work the frame did — the bench's own graph is small (one slot reading 800 states), where a
real screen has many slots each reading several, so the same guard removes a larger figure there.

### A fourth fix: layout's two whole-tree maps

`LayoutTransaction::new` snapshots the composer so a panic mid-layout can roll back. Two of the things it
snapshotted are maps layout is about to rebuild anyway — `prev_nodes` and `prev_node_by_key` — and it
CLONED them. The clone was a deep copy of every node's `CachedNode`: a `Modifier` (a `Vec` of elements,
some holding strings) and a `RefCell`, four thousand times over, on a frame whose actual measurement
folded in 0.04 µs. The rebuild immediately clears both maps, so nothing reads their old content while the
new one is built; the snapshot needs them only for rollback. Moving them (`mem::take`) is therefore
equivalent and removes the copy — with the one caveat that the map being rebuilt now starts with no
capacity, so `collect_nodes`/`collect_node_keys` reserve up front instead of letting a `HashMap` grow
into 4000 entries (that growth was measured at 128 µs, more than the clone the move removed).

While in there, a second piece of the same kind: both collects cloned each node's `children` `Vec` to
satisfy the borrow checker while recursing. Copying one `usize` out of the arena ends the borrow just as
well, so the clone — one heap allocation per node, per frame — went away.

| instrumented bucket, boxes 800 rows, idle layout | before | after |
|---|---|---|
| `LayoutTransaction::new`: `prev_nodes` clone | 149 µs | **0.03 µs** (a `mem::take`) |
| `collect_nodes` | 680 µs | **582 µs** |
| `collect_node_keys` | 261 µs | **226 µs** |
| **layout total** | **1230 µs** | **958 µs** |

| frame-level, boxes 800 rows | before | after |
|---|---|---|
| layout only, idle | 726 µs | **460 µs** (-37%) |
| idle frame | 1927 µs | **1572 µs** (-18%) |
| one row updated | 2746 µs | **2383 µs** (-13%) |
| layout only, every size re-measured | 1419 µs | **1162 µs** (-18%) |

Two regression tests came with it, and the first is the one that matters: the move is only correct
because rollback puts the maps back, and *nothing else in the suite would notice* if it stopped — a
missing entry does not panic, it silently fails the next frame's Skip and rebuilds the subtree.
`test_layout_panic_restores_cached_node_maps` drives a measure panic, then asserts both maps still hold
the previous frame's keys and that the frame after the retry can Skip. It was verified to FAIL with the
restore disabled, and it had to be built carefully: the first two versions armed the panic on a code path
that never ran (a leaf whose slot is clean and whose layout is not dirty keeps its cached measurement —
the same "the test passed because nothing happened" trap this document keeps recording).

### A fifth fix: the frame cache carried a whole `Modifier` per node

`collect_nodes` rebuilds a per-node cache (`prev_nodes`) on every layout. Each entry was a `CachedNode`
holding a copy of the node's **`Modifier`** — a `Vec` of elements, and through every `TextContent` a
`String` — so a frame allocated and freed a `Vec` per node (and a string per text node) to keep data
almost nothing read. Under the per-call profiler, at 4001 nodes: **182 µs of an idle layout's
`collect_nodes`**, on a frame whose real measurement folded in 0.01 µs.

The modifier was read in exactly two places, both asking the same question — "is this node's text
different from last frame's, while the slot stayed clean?" (`Text` whose string is recomputed by a
parent that re-entered, `TextField` input). So the cache now stores what that question needs: a
`text_snapshot` — the content plus every style field that affects measurement or rendering (colour
included: an alpha 0→1 fade changes no size, and a folded measurement would keep painting the cached
paragraph transparent), or `None` for the thousands of nodes with no text at all.

| instrumented bucket, boxes 800 rows, idle | before | after |
|---|---|---|
| `collect_nodes`: `to_cached` + insert | 182 µs | **106 µs** |
| `collect_nodes` total | 339 µs | **282 µs** |

| frame-level, boxes 800 rows (best of two runs) | before | after |
|---|---|---|
| **layout only, idle** | 460 µs | **285 µs** (-38%) |
| idle frame | 1572 µs | **1458 µs** (-7%) |
| one row updated | 2383 µs | **2151 µs** (-10%) |

The isolated layout arm moves far more than the frame, and the difference is where the old design paid
twice: dropping a `CachedNode` with a `Modifier` inside it frees that `Vec`, so the cache's whole
lifetime (fill, then drop when the transaction commit replaces it) was allocation churn. The rest of
the bucket — 106 µs — is the `HashMap` insert of 4000 small entries, which is the next step if this
path needs more.

Two tests guard the semantics, and both were verified to fail when the behaviour they cover is
removed — which is the only reason to trust them:

* `test_text_content_change_remeasures` (pre-existing) fails with the comparison disabled: the reused
  leaf never re-measures (`w=0` both frames).
* `test_text_style_change_remeasures` (new) changes **only the colour** and requires the leaf to be
  dirty; it fails when `color` is dropped from the snapshot, which is the failure mode a
  fields-not-copied rewrite has.

**One honest negative result**: the UI suite does NOT exercise this path. With a freshly built fixture
and the comparison disabled, `click_updates_state_and_keeps_structure` and the three `text_field` tests
all still pass — the flows they drive make the text leaf dirty in some other way. The unit tests above
are the ones that cover it; the UI suite being green says nothing about it. (Checked that the fixture
really was fresh: `cargo test --test ui_test` does rebuild `target/debug/fixture_all.exe` — mtime
verified moving with a source edit — so a green UI run after a lib change is testing the new code.)

### A sixth fix: two per-node hash structures in `materialize`'s claim path

`materialize` walks the descriptor tree and, for every Skipped node, claims the cached arena node: it
looked the key up in `prev_node_by_key`, then looked it up AGAIN to remove it (a `.get` guarded by a
shape check, then `.remove().unwrap()`) — two hashes per node, 4000 times a frame — and marked the
claim in `reused_nodes: HashSet<usize>`, a third hash. Both are now one operation each: the key is
removed once (the shape check runs on the removed value and, in the rare mismatch case, the key is put
back so the compose tail still recycles its node), and the reuse marks are a bit vector over arena
indices — `contains` and `insert` are a shift and a test, which is what the tail's `free_node_skip` and
the shared-element detach need as well.

| frame-level, boxes 800 rows (best of two runs) | before | after |
|---|---|---|
| **compose only, idle** | 1123 µs | **992 µs** (-12%) |
| idle frame | 1458 µs | **1276 µs** (-12%) |
| one row updated | 2151 µs | **1992 µs** (-7%) |

The skip-recovery tests are the ones that cover this path (`test_skip_recovery_sig_mismatch_direct`
asserts the key survives a shape mismatch, which is exactly what the put-back has to preserve), and the
full UI suite passes with them.

### A seventh fix: a skipped subtree is claimed in place, not re-encoded

The largest single item in any composed frame was `materialize`, and almost all of it was spent
re-encoding a skipped subtree into descriptors and then walking those descriptors back into the arena.
The encoding was pure translation: the slot tree says what the subtree is, the descriptors copy that
statement node by node, and the walk reproduces a tree whose content did not change. Measured at 800
rows on an idle frame: **197 µs to build the descriptor tree + 193 µs to walk it**.

That is ~30% of the frame for a subtree where nothing changed, and the observation that removes it is
uncomfortable in how obvious it is: **the arena already holds the answer.** A skipped subtree is by
definition one whose content did not run, so its slots still describe exactly what the arena was built
from, and the arena's nodes for it are still there.

So `collect_desc_tree` now tries to keep the subtree where it is. It walks the SLOT subtree and the
ARENA subtree together, node for node (`SlotTable::try_claim_skipped_subtree`), and only if they match
exactly — every node's key resolving to exactly the arena child the parent lists at that position,
scopes hoisted the same way, in order — does it claim: mark the nodes reused, drop their keys from
`prev_node_by_key` (so the compose tail does not recycle them), and emit ONE descriptor carrying the
claimed index. The materializer then only attaches the node and applies the root's own payload.

Three properties make the claim provably a no-op beyond bookkeeping, and they are what make it safe:

* **Every slot in the subtree must be inert.** No `desc` left, no `skip_modifier`, no `skip_policy` —
  because the fallback's per-node work for those is not "reuse the node", it is "apply this payload and
  consume it" (`take()`), and skipping that would leave the slot in a different state for the next
  frame. The root is exempt: its payload is what the descriptor carries anyway.
* **Verification is node for node, not a count.** A count would accept a tree of the same size with a
  different shape; the walk compares identity and order at every level, which subsumes the shape check
  the fallback performs at skip boundaries.
* **A mismatch claims nothing.** Marks go to a scratch set, merged into the frame's reuse set only after
  the whole subtree verified, so the fallback always starts from an untouched arena.

| instrumented, boxes 800 rows, idle | before | after |
|---|---|---|
| `collect_desc_tree` | 197 µs | **100 µs** (now the verification walk) |
| `materialize_node` loop | 193 µs | **0.13 µs** (one attach per frame) |
| materialize total | ~390 µs | **~100 µs** |

| frame-level, boxes 800 rows (best of two runs) | before | after |
|---|---|---|
| **idle frame** | 1276 µs | **994 µs** (-22%) |
| one row updated | 2023 µs | **1652 µs** (-18%) |
| idle frame (text scene) | 2709 µs | **2213 µs** (-18%) |
| cold frame | 6472 µs | 6442 µs (untouched — a cold frame has nothing to claim) |

Two tests pin the halves that matter, and the second is the one that justifies the walk:

* `test_skipped_subtree_is_claimed_in_place` — an idle frame claims, and the arena keeps the SAME node
  indices in the SAME order (nothing was rebuilt); a second idle frame claims again.
* `test_stale_arena_shape_makes_the_claim_bail` — swaps two children in the arena by hand (the only way
  to reach a stale shape deliberately), then requires `skip_claim_bails >= 1` and the children to come
  back in SLOT order: the mismatch is noticed and the fallback repairs the tree. The children below the
  container are still claimed, which is the point of verifying per subtree rather than per frame.

#### Two failures this caused, and what they were really about

Both were places where the rest of the frame relied on `prev_node_by_key` being non-empty as a PROXY,
and the claim legitimately empties it on any frame whose whole tree was skipped:

1. **`test_layout_dep_survives_const_fold`**: `materialize` refills the index from the existing tree
   when it finds that index empty (a repair for compose-without-layout), so the compose tail then
   walked the refilled index and reported every claimed key as *removed* — which pruned a live slot's
   layout dependency and froze its animation. The drain now skips nodes marked reused: a node in this
   frame's tree was not removed, so neither its layout dependencies nor its `on_remove` are due.
2. **`test_same_frame_second_compose_retains_tree`**: that repair is the ONLY thing that lets a
   compose-without-layout reuse nodes instead of rebuilding them, so it cannot be skipped just because
   the index is empty. It is now skipped only when this frame's own claim emptied the index
   (`claims > 0`, where reuse is already guaranteed and the rebuild would be pure waste).

The first failure also cost a measurement until it was dealt with: with the repair firing on every
frame, the claim saved ~290 µs and paid ~100 µs of it straight back.

#### One thing the walk made worse before it got better

The claim's index bookkeeping first ran per claimed subtree (`retain` over `prev_node_by_key` inside
each claim). That is fine for one big skipped tree and quadratic for a list of small ones: the one-row
update went from 2023 µs to **3064 µs**, because 799 row claims each swept an 800-entry map. The sweep
is now one pass per frame, and the one-row figure is 1652 µs. Worth recording because the claim's win
is measured against the one-row frame as much as the idle one, and a list is exactly where a
per-subtree cost stops being cheap.

### An eighth fix: the prune's per-parent sets, and a premise that turned out false

`prune_stale_child_links` runs on every compose and checks one thing per node: that a parent's child
list has no duplicates, and that an unreachable parent keeps no list at all. It did the duplicate check
with a `HashSet` **per parent** plus a filtered `Vec` per parent — at 800 rows, ~1600 allocations per
frame to check lists that are almost always already correct, measured at **187 µs** of a ~1000 µs idle
frame.

It is now a generation stamp per node (indices are dense, so a `Vec<u64>` indexed by node index):
`stamp += 1` per parent, a child is a duplicate if its stamp already equals the current one, and a
parent whose list is clean never allocates or rewrites anything. The stamp is `u64` on purpose — it
increments once per parent per frame, so at 60 fps with 1000 parents a 32-bit counter would wrap in
about 20 hours of continuous running.

| instrumented, boxes 800 rows | before | after |
|---|---|---|
| `prune_stale_child_links` | 187 µs | **16 µs** |

| frame-level, boxes 800 rows (best of three runs) | before | after |
|---|---|---|
| idle frame | 994 µs | **852 µs** (-14%) |
| one row updated | 1652 µs | **1530 µs** (-7%) |
| cold frame | 6442 µs | 6026 µs (-6%) |

#### The premise I went in to check, and why it failed

The documented plan for this function was to make it **conditional**: "skipping it on frames that
entered nothing is plausible and unproven — what needs establishing first is whether a frame with no
Entered node can create a stale listing at all". So the first thing this round did was measure that,
with a temporary probe in the function that logged every frame it REPAIRED (and, in the second pass,
every frame in the low-entered band, so there would be a denominator) plus the thread name, which under
`cargo test` is the test name.

Over the full library suite: **121 repairing frames** (2079 logged in the band). Sorting them by how
much was entered:

| entered | repairing frames | of those, real scenarios |
|---|---|---|
| 0-2 | 5 | `ui::snackbar` (tree torn down, `root=None`), `ui::animated_visibility` |
| 4 | 1 | `ui::animated_visibility::tests::siblings_survive_visibility_toggle` |
| 6-45 | 115 | every navigation, shared-element and lazy-scroll test |

The premise is false, and the counterexample is not a synthetic one: an `AnimatedVisibility` retiring
its subtree repairs a stale listing on a frame where **4** groups entered (its own content is gone,
its siblings are skipped, and only the tick that finishes the exit animation did anything). A rule of
"skip when `entered <= 2`" would have survived this suite and broken the first time that scenario
nested one group deeper. Given that this function's failure mode is a `[dup-key]` panic or a ghost that
paints nothing — and that the codebase's own comments record two rounds of exactly that — the
conditional was dropped and the function was made cheap instead, which needs no new precondition at
all and took the same 187 µs off the frame.

The probe is gone (it was temporary), and the measurement it produced is the reason this section
exists: **a premise can be checked cheaply enough that checking it is the first step, not a
justification for skipping the work.**

### A ninth fix: a hasher for the two layout maps, and a fusion that measured flat

The two maps layout rebuilds every frame — the node cache (`slot_key` → `CachedNode`) and the reuse
index (`slot_key` → arena index) — were `HashMap<u64, _>` with the std default hasher, i.e. SipHash-1-3
on a `u64` that the composer had ALREADY mixed. At 800 rows that is ~4000 inserts into each map per
frame, plus ~4000 lookups back in materialize.

They now use `SlotKeyMap`, whose hasher is `(key ^ (key >> 32)) * FIB`. The multiply is Fibonacci
hashing (one instruction); the xor before it is the part that matters, because a multiply propagates
bits upward only — the LOW bits of a product depend only on the low bits of its input, and hashbrown
indexes with the low bits while taking its control byte from the top 7. Folding the high half down first
gives both ends of the result a dependency on the whole key.

The hasher has a test, and building it was instructive enough to record. Two versions of that test were
wrong before one was sharp:

* v1 asserted the busiest bucket stayed within 4x the average. It failed — but the "failure" was my
  threshold, not the hasher: 800 keys in 256 buckets have a worst bucket around **9 even for sha256**.
  Recomputing the bound against a strong hash is what showed it.
* v2 (loosened to 20x) passed… with a **pass-through hasher** deliberately installed. The reason is
  worth knowing: `slot_key`s are pre-mixed by `mix_key`, so their low bits already spread, and a
  pass-through is injective — every distinct key gets a distinct hash. The test could not see the
  difference because it was measuring the same thing twice.
* v3 is the sharp one, and it tests the property the comment claims: take 256 key pairs that differ ONLY
  in bit 40 and compare the low 24 bits of their hashes. A pass-through collapses all 256 (the low bits
  are identical), a bare multiply collapses all 256 (it cannot bring bit 40 down), and the shipped
  hash collapses 0. The distribution half is kept too, explicitly labelled as "can only catch a total
  collapse".

| instrumented, boxes 800 rows, idle layout | before | after |
|---|---|---|
| both maps (cache + index) | ~300 µs | **~220 µs** |

| frame-level, boxes 800 rows (best of three runs) | before | after |
|---|---|---|
| **idle frame** | 852 µs | **757 µs** (-11%) |
| one row updated | 1530 µs | **1423 µs** (-7%) |
| compose only, idle | 830 µs | **784 µs** (-6%) |
| layout only, idle | 272 µs | **222 µs** (-18%) |

The compose column moving is the same hasher seen from the other side: `materialize` looks up
`prev_node_by_key` once per node and `prev_nodes` at every reused leaf.

#### The fusion that did not pay

The two maps were filled by two separate walks (`collect_nodes`, post-order so `dirty` bubbles up;
`collect_node_keys`, pre-order for the index). They visit the same nodes along the same edges, so this
round fused them into one (`collect_layout_maps`) sharing the duplicate-key failure path.

**Measured: within noise.** The reason is visible once stated: the second walk runs immediately after
the first over the same 4000 nodes and the same child vectors, so it is entirely cache-hot — the first
walk is where the memory traffic is. It is kept anyway (one traversal, one place where the `[dup-key]`
diagnostic lives, and no second signature to keep in sync), but it is recorded here as a null result
rather than dressed up as a win. The hasher is what the ~100 µs in the table above came from.

### A tenth attempt: skip the map rebuild when nothing changed — REVERTED

The largest item left on the layout side was the fused map walk (~220 µs per frame, ~29% of an idle
frame). The maps are a pure function of the arena tree, so a frame in which the tree did not change —
nothing entered, nothing measured — should keep them. That is the shape of every fix in this document
so far, and it was written, measured and **reverted**: the code is not in the tree, and this section is
what the attempt produced.

The reason is not that it was slow to write. It is that **"the frame changed nothing" is not a property
any part of this codebase maintains**, and the cache depends on node fields written by five different
subsystems. The signals the attempt added (`arena.structural_version` for identity, `materialize`'s
touch flag, the measurement keys for measurement) each looked sufficient, and each was refuted by the
test suite in turn:

1. **A layout with no root armed the validity flag while leaving the maps empty.** With the arena's
   nodes still present, a later frame whose root came back *without allocating* (a test assembling a
   tree by hand) passed the check with empty maps and kept them. 58 tests failed on that one line.
2. **`HashMap::drain()` empties the map.** The first version of "keep the index when the frame was
   untouched" kept nothing, because the loop that reclaims removed nodes had already drained it. The
   switch suite caught it (text vanished).
3. **`materialize` has an early-return path that never reaches `materialize_node`** — the frame that
   tears the tree down when the content produces no descriptors. `touched` stayed false while the whole
   tree went away: measured on the snackbar's teardown frame, six index entries, none of them reachable
   from the root.
4. **Focus is written outside compose and layout.** `focus_next`/`focus_prev` set `node.focused`, and the
   cache carries `focused`. Two UI tests that press Tab (the range slider's keyboard model, the modal
   overlay's) failed: a stale cache restored `focused: false` over the focus the app had just set. The
   debug-only equivalence check did not catch this one either, which is the important part — the net that
   had been catching the others has a hole of its own.

Every one of those was fixable, and after the first three the suite was green (1060 lib tests). The
fourth is what settled it: the list of subsystems whose writes the cache must observe is *open* —
materialize, measure, focus routing, shared-element retention, the reclaim path — and the cost of
missing one is silent stale state in the area whose comments already record two rounds of dup-key
panics and a ghost that painted nothing. The per-frame saving does not buy that.

**What the finding points at instead**, if this is ever attempted again: stop asking "did anything
change this frame?" (a global question nothing maintains) and make the cache invalidate **per node** —
the same marks that already work for identity. `reused_nodes` is the proof this is expressible: a bit
vector over dense arena indices, set by whoever puts a node into the frame. The same shape would work
for the cache: whoever writes a field the cache carries marks that node, and `layout` updates only the
marked entries instead of rebuilding all of them. That is local, it fails loudly (a missing mark leaves
one node's cache stale, not the frame's), and it does not need any new global invariant.

### An eleventh fix: the live-key set, and the residue question behind it

`collect_live_keys` walks the whole slot tree at the end of every compose and builds a set of the keys
that are still part of the composition — ~4000 inserts at 800 rows. Instrumented, it cost **194 µs of
an idle frame**, which made it the largest single item left on the compose side. Both halves of that
were fixable:

* the set was a plain `HashSet<u64>` — SipHash again, on slot keys the composer had already mixed;
* it was built without a `reserve`, so a `HashSet` growing into 4000 entries rehashed everything several
  times on the way.

`SlotKeySet` (the cheap hasher, added with the ninth fix) plus a `reserve` sized from the slot tree's own
node count (`children_count`, which `end_slot` maintains) took it to **96-122 µs across two probe runs**.

| instrumented, boxes 800 rows | before | after |
|---|---|---|
| `collect_live_keys` | 194 µs | **96-122 µs** |

Splitting the walk from the insert showed where the rest goes: with the inserts skipped and only the
traversal (`visited` checks, the in-skip propagation, the child loops) it is **27 µs**. So ~90 µs is
inserting 4000 keys into a 32 KB table — cache misses, not hashing. That is the floor for any
hash-set-shaped answer, which is what makes the next question worth asking.

#### Is the set even necessary?

The set exists to answer a handful of lookups per frame (a read recorded against a slot that is no
longer live must be dropped). It is a set of `slot_key`s that are live — and "live" here means "composed
this frame, or inside a skipped subtree". If every slot still in the tree after `truncate()` were live,
the set would be exactly "the keys in the tree" and could be replaced by recording the keys that LEAVE
(the rare, small event), which needs no per-frame walk at all.

So that was measured, and the first answer was wrong twice:

1. **A probe over the whole library suite and the bench found zero residue** — no slot ever survived a
   compose unvisited while outside a skipped subtree. Tempting to conclude the replacement is safe.
2. A test written to *produce* residue found 9 slots on the first shape it tried… **all of them live**.
   It was counting after the frame, and materialize consumes the frame's markers (`desc` and
   `skip_modifier` are `take()`n), so a skipped subtree read afterwards looks unvisited and un-skipped.
   The count was an artifact of WHEN it was taken.
3. Counting at the right moment — inside `collect_live_keys`, via a test-visible counter — the first
   shape (a restartable group whose interior shrinks) reported **0**. Correct: a group that re-enters
   prunes its own children (`end_restartable_group`'s `retain(visited)`), so nothing is left behind.
4. A **scope** whose contents shrink is the shape that does it: a scope is visited, never skips, and
   never prunes (`end_scope` is `end_slot`), so the leaves that stopped being composed stay in the tree.
   That test now asserts residue > 0 (`test_live_keys_exclude_unvisited_slots`), which is what makes the
   replacement unsafe — and it is the same trap the tenth attempt fell into: a plausible invariant that
   the test suite simply never exercises.

| frame-level, boxes 800 rows (best of two runs) | before | after |
|---|---|---|
| **idle frame** | 757 µs | **666 µs** |
| compose only, idle | 784 µs | **682 µs** |
| one row updated | 1423 µs | **1328 µs** |

### A twelfth fix: two `reserve`s that were asking for two elements

The cold frame — every group entering, every node built and measured — had never been split by this
document's probes. It is the slowest path in the framework by far (6063 µs against an idle frame's 683),
so this round profiled it first and found two things.

**The arena grew into place.** `materialize` builds the frame's nodes by pushing into
`NodeArena::nodes`, and a `Vec` growing 0 → 4000 does it by doubling: ~12 reallocations, each MOVING
every node already in it. A `LayoutNode` is a large struct (a modifier plus a dozen cells), so that is
megabytes of `memcpy` per cold frame. It is now sized once, up front, from the slot tree's node count.

**And the number it was sized from was 2.** The first version of that reserve read
`root_slot.children_count` — and that field is written by `end_slot`, which nothing ever calls for the
ROOT slot, so it holds its constructor value of 1 forever. Both reserves built on it (this one and the
live-key set's from the previous round) asked for two elements and did nothing at all: the previous
round's win came entirely from the hasher that shipped with it, and its "reserve" was decoration. The
bound is now computed by summing the root's children's subtree counts, and a test asserts it against the
tree materialize actually built. That test was written against the buggy version first, where it reports
`the bound says 2 nodes but materialize built 9` — and its own first attempt asserted nothing at all,
because it re-composed a tree whose container had not changed, so the group Skipped and every shape
built two nodes.

| instrumented, cold frame, boxes 800 rows | before | after |
|---|---|---|
| `materialize`'s node loop | 1076 µs | **678 µs** |
| `collect_live_keys` | 116 µs | **69 µs** |
| **compose total** | 3821 µs | **3321 µs** |

**The `entered_compose_keys` set got the same treatment** as the live-key set: a `SlotKeySet` instead of
a plain `HashSet` (one insert per entering group — 4000 on a cold frame), and the dependency reconcile
now TAKES it rather than cloning it. Those are small next to the arena's memcpy, and their frame-level
effect is inside this machine's noise; the probe is what shows them, and the probe is where the numbers
above come from.

| frame-level, boxes 800 rows (best of eight runs) | before | after |
|---|---|---|
| **cold frame** | 6063 µs | **4829 µs** (-20%) |
| idle frame | 666 µs | **597 µs** (-10%) |
| one row updated | 1328 µs | **1262 µs** (-5%) |

Eight runs were needed because the machine was loaded for several of them (its text cold frame read
38-42 ms against 26 ms). Noise only ever ADDS time, so every figure here is the fastest of the eight —
the rule this document has used throughout, applied at a larger sample because this round's effect on
the idle frame is close to the noise floor.

### A thirteenth fix, and the content closure finally split

The cold frame's content closure was the largest bucket this document had never looked inside (1817 µs,
more than materialize and the whole layout pass). A probe on the entry points every node passes through
— `next_key`, `changed`, the two group functions, `start_node`/`end_node` — split it, boxes 800 rows,
3201 groups (one per row, per Row, per Column) and 8803 `changed` calls:

| content closure, cold frame | µs/frame | ns/call |
|---|---|---|
| `start_restartable_group` | **1441** | 450 |
|   of which `start_slot` | 359 | 112 |
|   of which the skip decision | 189 | 59 |
|   of which `prev_modifier` CLONE | **297** | **93** |
|   of which the `NodeDesc` write | 115 | 36 |
|   of which the params write, direction, pushes | ~340 | ~106 |
| `changed()` | 503 | 57 |
| `Box::new(policy)` at the call site (not inside the group) | 135 | 42 |
| `end_restartable_group` | 305 | 95 |
| `next_key()` | 212 | 66 |
| `end_node()` | 103 | 32 |

(These figures carry the probe's own `Instant::now` pairs — several per call — so they read high in
absolute terms. **The sixteenth round showed they read so high that the buckets sum to 2699 µs inside a
container measured at 1817 µs**, i.e. the probe dominates the per-call column and the table cannot be
added up. It names PLACES that cost something; the sizes have to come from removing a cause and watching
the frame, or from a standalone measurement. See that round's section.)

**The fix this round took is the bottom row.** `next_key`'s path counter and `start_slot`'s dirty check
were still `std::HashMap`/`HashSet<u64>` — SipHash on keys the composer had already mixed, in a path
that runs 3201 times per cold frame — and so were the `remember` counters. They are `SlotKeyMap`/
`SlotKeySet` now, the same substitution the ninth fix made for the layout maps and the eleventh for the
live-key set. Instrumented: `next_key` 212 → 153 µs, `start_slot` 359 → 327 µs.

| frame-level, boxes 800 rows (best of eight runs) | before | after |
|---|---|---|
| one row updated | 1262 µs | **1201 µs** |
| idle frame | 597 µs | 608 µs (noise) |
| cold frame | 4829 µs | 4981 µs (noise — see below) |

The idle and cold columns are within this machine's run-to-run spread, and that is worth stating rather
than hiding: the hasher swap helps frames with many ENTERS (a cold frame, a structure change), because
an idle frame never runs the group loop at all. The instrumented figures are the evidence; the
frame-level one only confirms the direction.

#### The three targets the split identified, and why none was taken this round

The probe's value is that it turned "content is 1817 µs" into three named candidates with sizes. All
three are redesigns in the skip decision — the highest-risk area in this codebase, whose comments record
two rounds of dup-key panics and a ghost that painted nothing — and each needs its own round with the
reasoning written down first:

* **`prev_modifier` clone, ~297 µs (93 ns/call).** Every Entering container clones its whole `Modifier`
  (a `Vec` of elements) into the slot so the NEXT frame can compare against it. Two ways out, and the
  difference between them is the point: (a) store a digest of the comparable params instead — but a
  hash collision then means "params unchanged" and the container Skips with a stale modifier, i.e. the
  one failure direction this code must not have; (b) compare against the arena node's modifier instead
  of storing one — the node IS holding last frame's modifier while compose runs (materialize has not
  run yet), so the answer is the same, but the node-index map is drained by materialize and refilled,
  so the same-frame-second-compose case has to be worked out before this is safe. (b) is the promising
  one: it removes the copy rather than approximating it.
* **`changed()` ~503 µs over 8803 calls (57 ns/call).** Each call boxes `param.clone()` into
  `pending_params` for the next frame's comparison, and walks the slot path to find the previous value.
  Removing the allocation means replacing `Box<dyn ParamValue>` with something inline — a redesign of
  the parameter mechanism, used by every component in the crate.
* **`Box::new(policy)` ~135 µs (42 ns/call).** Every container build boxes its policy fresh; the arena
  has a policy POOL for exactly this reason, and the pool is what `materialize` moves the box into. A
  version where compose allocates into the pool directly needs `NodeDesc` to carry a policy index
  instead of a box, which changes the policy pool's invalidation rules (a node's policy index is
  replaced when its type changes).

### A fourteenth fix: the skip decision reads the node instead of a copy of its modifier

The largest item the content-closure split named. Every Entering container cloned its whole `Modifier`
(a `Vec` of elements, plus any strings they carry) into the slot so the NEXT frame's Skip decision could
compare against it: ~93 ns per container, ~297 µs on a cold frame. The clone is gone, and so is the
field it filled (`Slot::prev_modifier` — one writer, one reader).

The comparison now reads the modifier off the ARENA NODE, found through `prev_node_by_key`:

```rust
match self.prev_node_by_key.get(&key) {
    Some(&idx) => modifier.param_eq(&self.arena.nodes[idx].modifier),
    None => false, // no basis for comparison: Enter, which is what a missing prev_modifier did
}
```

That is the SAME value, not an approximation. During compose the node still holds what materialize
applied last frame — materialize runs at the END of compose, and the only other writer of an arena
node's modifier is materialize itself (checked across the crate: every other `.modifier =` is a
component builder's own field). Which is why this was preferred over the tempting alternative of storing
a digest of the comparable parameters: **a digest can only ever answer "unchanged" wrongly**, and
"unchanged" here means the container Skips while keeping a modifier that no longer matches — the one
failure direction this code must not have. Reading the node removes the copy instead of approximating it.

#### The refill that makes it work, and why it is free

The index maps are consumed by materialize (its node loop removes each key it reuses, the compose tail
drains and clears the rest) and rebuilt by `layout`. So between two composes in the SAME frame — which is
exactly the shape the app's frame loop produces, since it composes repeatedly until its notification
queue is quiet — the index starts EMPTY on the second pass. Without a fix, every container would find no
node and Enter: a full re-run of the tree on every frame where a notification arrived mid-compose.

`compose` therefore refills the index from the existing tree when it finds it empty. The refill costs
nothing extra: it calls the same `collect_node_keys` walk that materialize's own repair would have done
on those same frames, just earlier. `test_second_compose_in_a_frame_still_skips_unchanged_containers`
pins it — and it is worth recording that the first version of that test called `layout` between the two
composes, which refilled the index and hid the problem entirely (the test passed with the refill
disabled); the failing pass was the THIRD, not the second. Instrumenting the test is what showed the
real sequence: index 2 entries after the frame's layout → compose 2 skips (index still valid) → compose 3
would Enter on an empty index.

| frame-level, boxes 800 rows (best of eight) | before | after |
|---|---|---|
| **cold frame** | 4981 µs | **4594 µs** (-8%) |
| one row updated | 1201 µs | **1189 µs** |
| idle frame | 603 µs | 603 µs (flat — an idle frame never runs the Enter path) |

The idle column being flat is the expected shape, not a shortfall: this cost is only paid by containers
that Enter, and an idle frame enters nothing. A cold frame enters everything, which is why it moves.

Two tests came with it, and they cover different halves. `test_container_modifier_comparison_skips_
rebuilt_modifiers_and_enters_on_change` pins the comparison's SEMANTICS from the outside — a rebuilt
modifier with the same numbers still Skips (content does not re-run), a changed number still Enters — and
is written so that a version which only checked the Enter half would pass with the comparison removed
entirely. The second pins the refill.

### A fifteenth fix: the parameter buffer recycles instead of allocating per declaration

`changed()` pushed a fresh `Box::new(param.clone())` on every call — 8803 calls on a cold frame, one per
declared parameter per container per frame, and the slot's previous vector was dropped at the same time.
Now the two vectors EXCHANGE places: the slot hands its previous parameters back
(`SlotTable::replace_current_params`) and the next frame's declarations overwrite those boxes in place
(`ParamValue::set_from_any`). In a list — where every row declares the same parameter types — the same
handful of allocations circulates down the rows instead of one box per row.

The write cursor (`pending_next`) replaces `pending_params.len()` as "how many parameters this frame
declared": that vector now arrives carrying the previous frame's list, which may be longer or shorter, so
the Skip comparison reads `&pending_params[..pending_next]`.

| allocations per frame, 40-row list (test-visible counter) | before | after |
|---|---|---|
| first frame | 40 | 40 |
| second frame | 40 | **1** |
| third frame | 40 | **0** |

That counter exists because **no behavioural test can see this**: reusing a box and allocating a new one
produce identical state. `test_parameter_boxes_are_recycled_across_frames` asserts the shape above and was
verified to FAIL (40 allocations on frame 2) with the reuse disabled.

| frame-level, boxes 800 rows (best of eight) | before | after |
|---|---|---|
| one row updated | 1189 µs | **1142 µs** (-4%) |
| one row updated, 200 rows | 257 µs | **244 µs** |
| idle frame | 603 µs | 601 µs (flat — nothing declares params when nothing enters) |
| cold frame | 4594 µs | 4597 µs (unchanged — see below) |

#### The cold frame did NOT move, and that corrects the previous section

The thirteenth fix's split reported `changed()` at ~503 µs on a cold frame and listed it as a target. That
number is real, and this fix does **not** recover it — because of what a "cold frame" is in this bench: it
builds a FRESH `Composer` for every iteration, so there is no previous frame's buffer to recycle from and
every box is first-time storage. A composer that has run before recycles; one that has never run cannot.

So the honest accounting is: the cold frame's parameter boxes are the storage the slots need (one per
declaring group, ~3200 of them at 800 rows), not waste — no fix removes them. What was waste is the
*re-allocation every frame after that*, and that is what is gone: a steady-state frame declaring 800
parameters now allocates about one box instead of 800. The one-row-update column is the steady-state
measurement, and it is the one that moved.

(This is the third figure in this document corrected by asking what a scene actually MEASURES rather than
what its label says. The original layout table was wrong because an arm labelled "layout" ran a compose
inside its timed region; the thirteenth fix's own `entries` column had to be read before believing any of
its numbers; and this one is a scene whose "cold" means a brand-new composer every iteration. The pattern
is consistent enough to be worth stating plainly: **the label is a hypothesis, and the thing to check
first is what the code under the timer does.**)

### A sixteenth round: the content-closure split was measuring itself

This round was going to remove `Box::new(policy)` — the last per-container allocation the thirteenth fix's
split named (~135 µs on a cold frame, 42 ns/call). Instead it established that **most of that table is
instrumentation**, and that the target is not worth what removing it costs.

#### The split's own arithmetic refutes it

The thirteenth fix's buckets, all of them living INSIDE the content closure:

| bucket | µs/frame |
|---|---|
| `start_restartable_group` | 1441 |
| `changed()` | 503 |
| `Box::new(policy)` | 135 |
| `end_restartable_group` | 305 |
| `next_key()` | 212 |
| `end_node()` | 103 |
| **sum** | **2699** |

and the content closure it is a split OF, measured by a single timer pair around `content(ctx)`:
**1817 µs**. The parts exceed the whole by 48%. That cannot happen with real work, and the difference is
exactly the instrumentation: ~24 800 measurement points (3201 groups + 8803 `changed` + the rest) at
~35 ns per point — two `Instant::now()` calls plus two thread-local accumulations — is ~880 µs.

So the per-call column of that table is dominated by the probe, not by the framework, and **the buckets
cannot be added up**. What the split IS still good for is what it was actually used for: naming PLACES
that cost something (the hashers, the `Modifier` clone, the parameter boxes were all real, and each was
confirmed by the frame moving when its cause was removed). What it cannot do is size them.

That is a lesson worth stating plainly, because it is the same shape as the artifacts this document
already records: **an instrument that costs more than the thing it measures reports itself.**

#### What the honest sizes are

Two ways to get a size that is not the probe's, both used in the rounds that followed the split — and
neither of them is "read the bucket":

* **Remove the cause and watch the frame.** The `Modifier` clone: −8% on a cold frame when it stopped
  being copied. The parameter boxes: −4% on a one-row update when they stopped being allocated.
* **Measure the operation standalone.** This round did that for the one target left, because its
  reported size was the smallest and the doubt was largest:

| standalone, 100 000 iterations, best of 9 | ns/call |
|---|---|
| the probe's own enabled path, with the flag a compile-time constant | 0.3 — a CONTROL, not a measurement |
| `Box::new(BoxLayout::new())` + drop | **27.0** |

The first row is why the harness is only good for the second one: with the enable flag foldable, the
compiler deletes the timer entirely, so it cannot price the probe that way. The real per-point cost comes
from the arithmetic above (~880 µs ÷ ~24 800 points ≈ 35 ns), which needs no timer to be believed.

So the allocation is real: 3201 of them on a cold frame is ~86 µs, not 135. Real, and smaller.

#### Why `Box::new(policy)` was not taken

86 µs, and only on frames that Enter containers — an idle frame allocates none, and a one-row update
allocates one. Removing it needs one of two things, both worse than the prize:

* **Compose writes into the arena's policy pool.** The pool is `Vec<Box<dyn MeasurePolicy>>` with a free
  list that `materialize` allocates from; a slot would have to remember its index and overwrite the
  pooled box in place when the policy's concrete type is unchanged (which it is, every frame, for every
  component). This puts arena writes inside the compose phase — the separation the code documents
  deliberately, and the thing that currently makes compose's panic rollback tractable — and it needs its
  own rules for a type change, a slot that disappears, and a panic mid-compose.
* **Inline storage for small policies.** All the built-in policies are 4-24 bytes (`BoxLayout` is one
  enum, `ColumnLayout` four fields), so a small-buffer type would remove the allocation outright. That is
  hand-written unsafe code — or a new dependency, in a workspace that has kept its dependency list
  deliberately short — for 2% of a cold frame and nothing on a steady-state one.

Recorded rather than taken, with the numbers that justify the call. If a future round needs it, the
decision to make first is not "how do I remove the allocation" but "is compose allowed to write to the
arena", and the answer to that one belongs in `docs/architecture-audit.md`, not here.

### A seventeenth round: the verify walk's price was a bucket label

The target was "`materialize`'s verification walk, ~100 µs per frame". Both halves of that are wrong, and
the round is a measurement again rather than a fix.

**The ~100 µs was never the verification.** It came from the thirteenth fix's probe of
`collect_desc_tree`, which times the descriptor-tree walk AND the claim together. The verification's own
price had never been measured.

**Measured by removing it.** The claim walk's comparisons (the `prev_node_by_key` lookup per node and the
child-order comparison) were disabled in a bench build and the idle frame compared, six runs each way,
fastest of each:

| idle frame, boxes 800 rows | fast sample |
|---|---|
| with the claim's comparisons | 614 µs |
| comparisons disabled (the walk — and therefore the marks — still ran) | **597 µs** |

So the comparisons cost **~17 µs**, not ~100: the mark-and-traverse part is the rest, and it is not
separable — see below.

**The first attempt at that experiment was wrong in an instructive way.** It disabled the WHOLE walk
(returning "verified" immediately), not just the comparisons — and the bench panicked on its first idle
frame:

```text
[dup-key] slot_key 冲突：sk=0x0 节点 idx=5 … 覆盖了已有节点 idx=1
```

The walk has two jobs: it verifies, and it MARKS every node of the claimed subtree as reused. With the
marks missing, the compose tail frees those nodes — they are in `prev_node_by_key` and not marked — so
the live tree's own descendants get recycled under it, come back as `LayoutNode::default()` with
`slot_key == 0`, and the key walk finds two nodes claiming that key. **"Claim without walking" is not a
cheaper claim; it is a broken one**, and that is now written at the walk in the code rather than here
only, because the experiment that found it looked like a safe one-line change.

**And the premise I went looking for was refuted by the code.** The walk re-derives a structure the index
maps claim to describe, so the tempting simplification is "the maps ARE the arena — the walk is a
tautology". It is not: `app.rs` calls `poll_shared_flights()` AFTER `layout()`, and a flight detachment
unlinks nodes (`detach_source`'s `children.retain(...)`). That is a real window in which the arena moves
under a map that was built before it, and it is exactly what the child-order comparison covers —
`test_stale_arena_shape_makes_the_claim_bail` already exercises it deliberately.

| what this round established | value |
|---|---|
| the verification's real cost (comparisons only) | **~17 µs**, not ~100 |
| the walk's irreducible part (marks + traversal) | ~80 µs — required, see above |
| the window the comparisons guard | arena mutations after `layout` (flight detach) |
| the target | closed: mispriced by ~6x, and the remaining part is an invariant, not overhead |

## What the frame's O(tree) floor is made of (instrumented)

With the per-call profiler, per frame, boxes 800 rows. The two columns are the same tree in the two
states that matter: every group Skipped, and one row's state moved. The compose column is after the
eighth fix, the layout column after the fifth (the layout side is untouched by the sixth through
eighth). These buckets are instrumented figures, so they are immune to the load that makes one
frame-level run differ from another — they are the better evidence for what a change did, with the
frame-level tables as the sanity check.

| bucket | idle (0 entered) | one row updated |
|---|---|---|
| `materialize` (now: verify-and-claim walk) | ~100 µs | ~100 µs |
| `prune_stale_child_links` (arena walk, after the eighth fix) | ~16 µs | ~16 µs |
| `collect_live_keys` (after the eleventh and twelfth fixes) | ~35 µs | ~35 µs |
| `register_modifier_deps` (arena walk) | ~30 µs | ~31 µs |
| compose setup (snapshots, resets, pending drain) | ~70 µs | ~80 µs |
| reconcile (after the third fix) | ~2 µs | ~68 µs |
| the row loop (800 reads + 800 Skip decisions) | — | ~415 µs |
| **compose total** | **~420 µs** | **~832 µs** |

Those totals are the frame-level `compose only` arms (best of eight) rather than a sum of the buckets
above, which are instrumented and were taken in different rounds; the row-loop figure is the control
scene's difference (`container dirty` minus `idle`: 835 − 420 µs). An earlier version of this table read
~300 / ~840 and was stale by three rounds.

The cold frame's split is different from both columns above, because everything in it runs. The
thirteenth fix measured it as: content 1817 µs (of which `start_restartable_group` 1441, `changed` 503,
`end_restartable_group` 305, `next_key` 212, `end_node` 103, and `Box::new(policy)` 135 outside), plus
materialize 1333 (node loop 678, claim walk 279), tail 343, live keys 69, prune 67 — compose 3321 µs;
layout 1152 (measure 675, the map walk 254, transaction 35, rest 128). The fourteenth fix removed the
`prev_modifier` clone from `start_restartable_group` (measured 297 µs of that 1441).

| layout, same tree | idle, before the fourth fix | idle, after the ninth |
|---|---|---|
| `LayoutTransaction::new` | ~170 µs (149 of it the map clone) | ~15 µs |
| `measure` | **~0.01-0.04 µs** — every node folds, nothing re-measures | same |
| the fused map walk (`collect_layout_maps`) | ~940 µs as two walks | **~220 µs** |
| the rest (dirty marks, deps, cleanup) | ~120 µs | ~90 µs |
| **layout total** | **~1230 µs** | **~325 µs** measured on this arm (222 µs in the frame arm) |

What is left of an idle frame is bookkeeping *about* the tree rather than work on it, on both sides:
`prune_stale_child_links` walks the whole arena as a defensive repair, `collect_live_keys` walks the
slot tree, `materialize`'s verification walk visits both trees, and layout's fused walk rebuilds the two
whole-tree maps. None of it does anything with the rows that did not change — they are the next round's targets,
and they are listed below. Note that the row loop's own cost reads larger than the loop's instrumented
parts: it is measured against the compose-only idle figure, which came down several times while the
loop itself did not change.

### Read placement still does not matter

`row_scoped` (the row reads its own state handle, so the row's scope is the dependent) against `row`
(the list reads and passes the value down). At 800 rows: idle 1589 vs 1572, one row updated 2351 vs
2383 — **no difference beyond noise**. Now that reads are cheap the conclusion is stronger than it was:
the frame is the walk, not the reads, so a list does not need to be contorted for read placement. (The
two shapes are otherwise identical; the first version of this scene built less content in the scoped
row and appeared to win, which is the kind of comparison error this document is annotated to avoid.)

## The layout half (and one artifact of a mislabelled arm)

An earlier version of this document said "the layout figure is pathological: one row's update costs 4x
more than re-measuring the whole tree". **That was a measurement artifact of my own making**, and it is
worth recording because it is easy to repeat: the arm labelled "layout, one row updated" ran
`compose_only()` inside the timed closure, so a compose cost was being read as a layout cost. The
benchmark's labels now say exactly what each arm times.

With the arms separated (boxes, 800 rows, before the subscription fix):

| arm | fast sample |
|---|---|
| compose only, nothing changed | ~1200 µs |
| compose only, one row updated | 6250 µs |
| layout only, nothing composed | 700 µs |
| layout only, every size re-measured | 1495 µs |
| frame (compose + layout), one row updated | 7980 µs |

Frame minus compose put the layout of a one-row update at ~1730 µs — between an idle layout (700) and a
full re-measure (1495), so **layout was never the problem**: it re-measures nothing when nothing
changed size, and its walk is the tree, not the update. Those two figures have since come down to 460
and 1162 µs (the fourth fix, above) — the larger one by re-measuring *better*, not by re-measuring less.

The old flat-in-position and flat-in-count evidence still explains the *shape* of that cost, correctly,
even though the number derived from it was wrong:

| compose, boxes 800 rows | then | now | groups entered |
|---|---|---|---|
| row 0 dirty | 6438 µs | 1580 µs | 1 |
| row 400 dirty | 6310 µs | 1575 µs | 1 |
| row 799 dirty | 6277 µs | 1614 µs | 1 |
| 2 adjacent rows dirty | 6744 µs | 1558 µs | 2 |
| 10 adjacent rows dirty | 6707 µs | 1518 µs | 10 |

Flat in WHERE the dirty row is and flat in HOW MANY rows are dirty, then and now: neither the change
nor its position is the cost, because the container's closure re-runs either way and the loop it
contains is the frame.

## Collections: `StateList` vs `State<Vec>`

The read is where the type pays for itself. A `State<Vec<T>>` read hands back a full clone of the
vector — every element copied — and a reader does that every time it renders. A `StateList` read is one
`Arc` clone.

| items | `State<Vec>` read | `StateList` read | ratio |
|---|---|---|---|
| 100 | 0.047 | 0.011 | 4x |
| 1 000 | 0.100 | 0.011 | 9x |
| 10 000 | **1.884** | **0.013** | **145x** |

`StateList`'s read is flat (it is O(1) in the collection) where `State<Vec>`'s grows linearly. For a
list rendered every frame, that is the difference.

Mutation is the smaller half, and honestly so: **both copy the contents** — `State` owns its value, and
the list publishes a new snapshot — so what differs is `State::set`'s whole-collection comparison and
one clone.

| items | `State<Vec>` push | `StateList` push | ratio |
|---|---|---|---|
| 100 | 1.381 | 0.846 | 1.6x |
| 1 000 | 2.099 | 1.336 | 1.6x |
| 10 000 | 7.201 | 4.658 | 1.5x |

## Three artifacts this benchmark hit, and what they cost

Each is in the file as a comment where it bit, because each looked like a framework problem and was
not:

1. **A counter read without resetting** reported the first frame's count forever, so an idle frame
   read "every row entering" and a one-row update read 750 of 200 rows. `frame_counted` now reads and
   resets.
2. **`#[composable]` does not declare a function's parameters.** Removing the explicit
   `ctx.changed(&value)` (because the macro has one, surely?) made every row Skip forever — reported
   as "one row updated: 0.00 entered". The framework's own components call `ctx.changed` themselves.
3. **Not composing inside `app_root!`** panics: `next_key` requires a stable key source, and the root
   of a composition has none until the macro supplies it. The bench now composes the way an
   application does.

The variance lesson is above: absolute figures from one run are not comparable with another run on a
loaded machine, so the fast sample is the headline and the median is shown for scale.

## Open optimization targets (measured, not attempted)

- **`materialize`'s claim walk — PRICED AND CLOSED (seventeenth round).** The "~100 µs" was a bucket
  label (`collect_desc_tree`, which times the descriptor tree and the claim together), not a measurement
  of the verification: the comparisons cost **~17 µs** (idle frame 614 → 597 with them disabled, best of
  six each way). The rest of the walk is the marking, and that cannot be dropped — a variant that
  skipped the walk corrupted the tree to `[dup-key]` on the first idle frame. A structure fingerprint
  would replace the ~17 µs, not the ~80, so the design it would need (a maintained per-slot digest,
  cross-checked in debug) is not worth its price. See that round's section.
- **`prune_stale_child_links`'s remaining ~16 µs**: what is left after the eighth fix is the
  reachability walk plus two buffer allocations per frame (a `Vec<bool>` and the stamp vector). Both
  could be reused across frames instead of reallocated — that needs a home on the `Composer` (or a
  thread-local) and buys ~1% of the frame, so it waits for a reason.
- **`collect_live_keys`'s remaining ~35 µs** (after the eleventh and twelfth fixes): the inserts dominate,
  and they are cache misses into a table rather than hashing, so no cheaper hasher helps from here. The
  only way further is to not build the set — see the eleventh fix's section: the shape that would make it
  replaceable ("record the keys that leave the tree") is refuted by a test, because scopes leave residue
  behind. A solution in that direction would have to make scopes prune (or record the residue when it is
  created), which is a behaviour change with its own suite to satisfy, not an optimization.
- **Layout's fused map walk, ~220 µs per frame — attempted and REVERTED**, which is worth recording
  because the target looks so obviously winnable. Both maps are functions of the tree, so a frame that
  changed nothing should keep them instead of rebuilding them; the round that tried it established that
  "changed nothing" is not a property anything in this codebase tracks, and the attempt is documented in
  its own section below. What survives is the diagnosis, not the code.
- **The row loop's ~415 µs**: 800 iterations at ~520 ns (105 ns read, ~330 ns group machinery, the rest
  being the container's own re-entry). It has come down from ~490 µs as the per-node costs above were
  removed, which is the shape to expect: the loop is where those costs were paid 800 times. Each iteration now also runs a small claim verification for its
  row (one hash lookup plus a two-node walk), which is why the loop did not get cheaper when the
  descriptor path did — the loop was never in the descriptor path.
- **`Box::new(policy)`, ~86 µs on a cold frame — MEASURED AND NOT TAKEN.** The thirteenth fix's table
  said 135 µs; the sixteenth round measured the operation standalone at 27 ns/call (3201 containers =
  ~86 µs) and established that the difference was the probe. It is paid only by frames that Enter
  containers — an idle frame allocates none, a one-row update one — and removing it needs either compose
  writing into the arena's policy pool (breaking the separation that makes compose's rollback tractable,
  with new rules for a policy type change, a vanished slot and a mid-compose panic) or hand-written
  inline storage for small policies (all the built-ins are 4-24 bytes) in a dependency-light workspace.
  The sixteenth round's section has both designs and the numbers behind the call.
- **A cold frame's parameter boxes are not recoverable** — they are the storage the slots keep, one per
  declaring group, and the fifteenth fix's section explains why a fresh composer has nothing to recycle.
  Anything that removes them has to remove the per-group parameter storage itself, which is a different
  design (the whole `changed`/`params_equal` skip mechanism).

None of these is claimed as a bug: they are the cost of the current design, now visible and comparable,
and each is one round of work with this bench as the measuring stick. The twelve defects that *were* bugs
— a read that was O(tracked signals), a layout snapshot that cloned fields layout cannot write, a reverse
graph rebuilt when its forward graph had not moved, a layout snapshot that deep-copied two maps it was
about to rebuild, a frame cache that carried a whole `Modifier` per node for one text comparison, two
per-node hash lookups in `materialize`'s claim path, ~1600 per-frame allocations in the prune to check
lists that are almost always already correct, a SipHash on 8000 pre-mixed keys per frame, two
`Vec::reserve` calls sized from a field nothing maintains (so they asked for two elements), a SipHash on
the per-node path counters, a `Modifier` clone per Entering container kept only so the next frame could
compare against it, and a `Box` per declared parameter per frame that a slot's previous vector could
have stored — were all found by measuring one bucket and finding something else inside it. The seventh fix is a different shape of change (a design that removes work
rather than a defect), and it produced its own two bugs on the way: both of them cases where the rest of
the frame was treating "the index is empty" as a proxy for something else. The eighth is the one round
whose planned approach measured FALSE before it was written (see its section), which is worth knowing:
the plan was on the list for two rounds and one probe retired it in an afternoon. The tenth went the
other way — written, measured green on the library suite, then reverted when the UI suite found the
fourth subsystem the cache depends on — and that one is written up for whoever tries it next. The
eleventh produced a measurement artifact of its own (a residue count taken after the frame, when the
markers that identify a skipped subtree have already been consumed) and then the shape that answered the
question it was asked, which is why the invariant it pins down now has a test instead of a comment. The
fourteenth nearly shipped a test that could not fail: it called `layout` between the two composes it was
comparing, which refilled the very index the test was written to check, so it passed with the fix
disabled — instrumenting the test instead of reasoning about it is what showed the real sequence
(compose 2 skips on the index the frame's layout left; compose 3 is the one that would have re-entered
everything). The sixteenth changed no code at all: it caught the thirteenth round's own split reporting
its instrumentation (parts summing to 2699 µs inside a 1817 µs container), priced the one target left
standalone, and closed it as not worth its design cost — which is a result too, and the one this
document's method is for. The seventeenth did the same for the last "measured" target on the compose
side: the figure was a bucket label rather than a measurement of the thing it named, the real price is
~6x smaller, and the part that remained turned out to be an invariant — disabling it to measure it
panicked with `[dup-key]` on the first idle frame, which is how the walk's second job (marking the
subtree as reused) came to be documented in the code instead of being rediscovered.

## Re-running any of this

The benchmark's own traps are documented in the file where they bit, and the phase splits used for the
investigations are kept as unversioned patches in the working tree:
`target/probe/layout_trace.patch` (`WINIA_LAYOUT_TRACE=1`), `target/probe/compose_trace.patch`
(`WINIA_COMPOSE_TRACE=1`), `target/probe/skip_prof_full.patch` and `skip_prof_refined.patch`
(`WINIA_SKIP_PROF=1`, the per-call profile that found the quadratic read, then the compose floor), and
`target/probe/p2_layout_prof.patch` (the layout-internal split that found the map clone and the two
`children` clones). They are throwaway instrumentation, not part of the framework: apply one with
`git apply` to the revision it was taken on (`1940a7a` for the compose profile, `6aeef1f` for the
layout one) and revert it before committing — `cargo bench` in a clean tree prints the tables above and
nothing else, and `cargo bench -p winia -- container` runs just the loop control scene.
