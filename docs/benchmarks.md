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
| 50 | 568 | 80 | 108 |
| 200 | 1559 | 328 | 456 |
| 800 | 7036 | 1544 | 2151 |

| rows (text) | cold | idle | one row moved |
|---|---|---|---|
| 50 | 755 | 107 | 133 |
| 200 | 4861 | 444 | 560 |
| 800 | 27153 | 2787 | 3778 |

16x the rows costs ~19x an idle frame and ~20x a one-row update. In the original figures recorded here
(before any of the fixes in this document) the same two ratios were 28x and 70x — the difference was a
quadratic term, and what remains is the per-frame walk over the tree, which is what the design says it
is. Entering ONE group still does not make the frame cheap: the walk that finds that group is the frame.

Two runs were taken for these figures and they agreed to a few percent on the box scene (idle 1544 /
1458, one row 2151 / 2199, layout-idle 287 / 283); the text scene is the noisier of the two (its idle
frame moved 2787 / 2832 while its one-row update moved 3778 / 3426, which is why this document quotes
the fast sample rather than treating a single text figure as a precise one).

The breakdown at 800 rows (boxes) says where it goes:

| | compose | layout |
|---|---|---|
| idle | 1123 | 285 |
| one row moved | 1596 | ~600 |

and the control that splits composition's extra into "the walk" and "the update" — a state the
CONTAINER reads moves, so the container re-enters and the row loop runs while every row's own parameter
is unchanged:

| compose, boxes 800 rows | fast sample | groups entered |
|---|---|---|
| idle (container Skips, so the loop does not run) | 1086 | 0 |
| container dirty, every row Skips | 1514 | 0 |
| one row dirty (the same loop + one rebuild) | 1620 | 1 |

The loop over 800 rows costs **~430 µs**, and re-entering one row inside it costs **nothing measurable**
(+100 µs here, i.e. run-to-run noise). The update is free relative to the walk that finds it.
Instrumented attribution of that ~430 µs: 84 µs of state reads (~105 ns each, 800 of them) and ~290 µs
of group machinery, the rest being the container's own entry plus rows materializing one by one instead
of as one cached subtree.

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

## What the frame's O(tree) floor is made of (instrumented)

With the per-call profiler, per frame, boxes 800 rows. The two columns are the same tree in the two
states that matter: every group Skipped, and one row's state moved. The layout column is after the
fifth fix, the compose column after the third (the fourth and fifth barely touch it).

| bucket | idle (0 entered) | one row updated |
|---|---|---|
| `materialize` (desc tree + node reuse walk) | ~255 µs | ~270 µs |
| `prune_stale_child_links` (arena walk) | ~130 µs | ~150 µs |
| `collect_live_keys` | ~100 µs | ~100 µs |
| `register_modifier_deps` (arena walk) | ~30 µs | ~31 µs |
| compose setup (snapshots, resets, pending drain) | ~70 µs | ~80 µs |
| reconcile (after the third fix) | ~2 µs | ~68 µs |
| the row loop (800 reads + 800 Skip decisions) | — | ~430 µs |
| **compose total** | **~590 µs** | **~900 µs** |

| layout, same tree | idle, before the fourth fix | idle, after the fifth |
|---|---|---|
| `LayoutTransaction::new` | ~170 µs (149 of it the map clone) | ~15 µs |
| `measure` | **~0.01-0.04 µs** — every node folds, nothing re-measures | same |
| `collect_nodes` | ~680 µs | ~280 µs (106 of it the insert) |
| `collect_node_keys` | ~261 µs | ~120 µs |
| the rest (dirty marks, deps, cleanup) | ~120 µs | ~90 µs |
| **layout total** | **~1230 µs** | **~490 µs** |

What is left of an idle frame is bookkeeping *about* the tree rather than work on it, on both sides:
`materialize` rebuilds a descriptor for every node and then walks them to claim the cached ones,
`prune_stale_child_links` walks the whole arena as a defensive repair, `collect_live_keys` walks the
slot tree, and layout rebuilds the two whole-tree maps. None of it does anything with the rows that did
not change — they are the next round's targets, and they are listed below.

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

- **`materialize`: ~255-270 µs per frame in both states** — the largest single item left on the compose
  side. `collect_desc_tree` recurses through a Skipped subtree and builds a `DescNode` per node — one
  `Vec` and one `Modifier` clone each — then `materialize_node` walks those descriptors to reuse the
  cached nodes by key. For a subtree that skipped, both halves are pure overhead: the structure is by
  definition what it already was, and the nodes are already in `prev_node_by_key`. The cheap direction
  is to record the subtree as a single skip descriptor and claim the cached nodes from the arena
  directly (their `children: Vec<usize>` is the walk), which is exactly the protocol the comments in
  this area are full of war stories about — dup-key panics, vanishing subtrees, ghost flights — so it
  needs its own round with the full UI suite, not a quick edit.
- **`prune_stale_child_links`: ~130 µs per frame.** A defensive whole-arena walk, and the current
  comment insists it runs for EVERY compose (moving it into an early-returning function once took the
  repair off the default path and a stale listing reached the `[dup-key]` guard). Skipping it on frames
  that entered nothing is plausible and unproven: what needs establishing first is whether a frame with
  no Entered node can create a stale listing at all — a shared-element flight ending is the case to
  check, since `retain_shared_sources` re-parents nodes on frames where nothing recomposed.
- **`collect_live_keys`: ~100 µs per frame**, a whole-slot-tree walk producing the live-key set the two
  reconciles consume. It is the input to the guards above, so it is the next thing to make incremental.
- **`collect_nodes`'s remaining ~280 µs and the `collect_node_keys` index beside it (~120 µs)**: both
  rebuild whole-tree maps from the arena every frame, for a frame in which measurement folded at
  0.01 µs. The per-entry data is small now (106 µs is the insert of 4000 entries); making the *rebuild*
  incremental, or having the cache borrow the arena instead of copying out of it, is what is left, and
  it needs the same care the fourth fix took around rollback.
- **The row loop's ~430 µs**: 800 iterations at ~540 ns (105 ns read, ~330 ns group machinery, the rest
  materialize restoring rows one by one). Attacking `changed()`'s per-call parameter allocation (~75 ns)
  or the `prev_nodes` lookup per Skipped group is now a small win, not the headline.

None of these is claimed as a bug: they are the cost of the current design, now visible and comparable,
and each is one round of work with this bench as the measuring stick. The five defects that *were*
bugs — a read that was O(tracked signals), a layout snapshot that cloned fields layout cannot write,
a reverse graph rebuilt when its forward graph had not moved, a layout snapshot that deep-copied
two maps it was about to rebuild, and a frame cache that carried a whole `Modifier` per node for one
text comparison — were all found by measuring one bucket and finding something else inside it.

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
