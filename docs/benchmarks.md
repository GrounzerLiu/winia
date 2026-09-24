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
These are the figures after the first two fixes in this document; the third is inside this machine's
noise at frame level and is evidenced by the profile instead.

| rows | cold | idle | one row moved |
|---|---|---|---|
| 50 | 605 | 107 | 136 |
| 200 | 1568 | 448 | 572 |
| 800 | 7156 | 1927 | 2746 |

| rows (text) | cold | idle | one row moved |
|---|---|---|---|
| 50 | 763 | 136 | 162 |
| 200 | 5268 | 581 | 685 |
| 800 | 28440 | 3192 | 3897 |

16x the rows costs ~18x an idle frame and ~20x a one-row update. In the original figures recorded here
(before any of the fixes in this document) the same two ratios were 28x and 70x — the difference was a
quadratic term, and what remains is the per-frame walk over the tree, which is what the design says it
is. Entering ONE group still does not make the frame cheap: the walk that finds that group is the frame.

The breakdown at 800 rows (boxes) says where it goes:

| | compose | layout |
|---|---|---|
| idle | 1225 | 777 |
| one row moved | 1653 | ~880 |

and the control that splits composition's extra into "the walk" and "the update" — a state the
CONTAINER reads moves, so the container re-enters and the row loop runs while every row's own parameter
is unchanged:

| compose, boxes 800 rows | fast sample | groups entered |
|---|---|---|
| idle (container Skips, so the loop does not run) | 1225 | 0 |
| container dirty, every row Skips | 1745 | 0 |
| one row dirty (the same loop + one rebuild) | 1643 | 1 |

The loop over 800 rows costs **+520 µs**, and re-entering one row inside it costs **nothing measurable**
(-100 µs here, i.e. run-to-run noise). The update is free relative to the walk that finds it.
Instrumented attribution of that 520 µs: 84 µs of state reads (~105 ns each, 800 of them) and ~290 µs
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

### Read placement still does not matter

`row_scoped` (the row reads its own state handle, so the row's scope is the dependent) against `row`
(the list reads and passes the value down). At 800 rows: idle 1994 vs 1927, one row updated 2609 vs
2746 — **no difference beyond noise**. Now that reads are cheap the conclusion is stronger than before:
the frame is the walk, not the reads, so a list does not need to be contorted for read placement. (The
two shapes are otherwise identical; the first version of this scene built less content in the scoped
row and appeared to win, which is the kind of comparison error this document is annotated to avoid.)

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

## What the frame's O(tree) floor is made of (instrumented)

With the per-call profiler, per frame, boxes 800 rows. The two columns are the same tree in the two
states that matter: every group Skipped, and one row's state moved.

| bucket | idle (0 entered) | one row updated |
|---|---|---|
| `materialize` (desc tree + node reuse walk) | ~600 µs | ~560 µs |
| `prune_stale_child_links` (arena walk) | ~245 µs | ~255 µs |
| `slot_table.truncate` + `collect_live_keys` | ~190 µs | ~185 µs |
| `register_modifier_deps` (arena walk) | ~61 µs | ~61 µs |
| compose setup (snapshots, resets, pending drain) | ~125 µs | ~115 µs |
| reconcile (after the third fix) | ~4 µs | ~131 µs |
| the row loop (800 reads + 800 Skip decisions) | — | ~520 µs |
| **compose total** | **~1.25 ms** | **~1.95 ms** |

| layout, same tree | idle | notes |
|---|---|---|
| `LayoutTransaction::new` | ~170 µs | clones `prev_nodes` + `prev_node_by_key` |
| `measure` | **~0.04 µs** | every node folds — nothing re-measures |
| `collect_nodes` | ~325 µs | rebuilds `prev_nodes` wholesale |
| `collect_node_keys` | ~101 µs | rebuilds `prev_node_by_key` wholesale |
| the rest (dirty marks, deps, cleanup) | ~130 µs | |
| **layout total** | **~725 µs** | |

Layout's idle cost is almost entirely bookkeeping *about* the tree rather than work on it: the
transaction snapshots two whole-tree maps, then `collect_nodes`/`collect_node_keys` throw them away and
rebuild them, every frame, for a tree in which measurement folded at 0.04 µs. That — plus
`materialize`'s per-frame desc tree and the two arena walks in compose — is where a frame's O(tree)
floor lives, and it is the next round's work rather than this one's.

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
changed size, and its walk is the tree, not the update. The cost was all in composition, and the
correction above says what it was.

The old flat-in-position and flat-in-count evidence still explains the *shape* of that cost, correctly,
even though the number derived from it was wrong:

| compose, boxes 800 rows | then | now | groups entered |
|---|---|---|---|
| row 0 dirty | 6438 µs | 1653 µs | 1 |
| row 400 dirty | 6310 µs | 1651 µs | 1 |
| row 799 dirty | 6277 µs | 1656 µs | 1 |
| 2 adjacent rows dirty | 6744 µs | 1648 µs | 2 |
| 10 adjacent rows dirty | 6707 µs | 1662 µs | 10 |

Flat in WHERE the dirty row is and flat in HOW MANY rows are dirty, then and now: neither the change
nor its position is the cost, because the container's closure re-runs either way and the loop it
contains is the frame.

## One fix landed: the layout transaction

`LayoutTransaction::new` runs on every `layout()` call and snapshots the arena so a panic mid-layout can
roll back. It used to snapshot each node's `modifier` (a `Vec`, and through a `TextContent`'s `String` a
string per text node) and `children` (another `Vec`) — fields layout cannot touch, because they are
composition products written by `materialize` while composing, and the transaction's lifetime sits
inside `layout()`. At 4001 nodes the phase split put that clone at 1.2 ms — larger than measurement
itself (measure: 115 µs, `collect_nodes`: 700 µs, transaction: 1200 µs).

Snapshotting only the fields layout writes took an 800-row idle frame from **3452 µs to 1998 µs (-42%)**
and a one-row update from **11393 µs to 7701 µs (-32%)**, with the full library suite green.

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

- **`materialize`: ~600 µs per frame in both states.** `collect_desc_tree` recurses through a Skipped
  subtree and builds a `DescNode` per node — one `Vec` and one `Modifier` clone each — then
  `materialize_node` walks those descriptors to reuse the cached nodes by key. For a subtree that
  skipped, both halves are pure overhead: the structure is by definition what it already was, and the
  nodes are already in `prev_node_by_key`. The cheap direction is to record the subtree as a single
  skip descriptor and claim the cached nodes from the arena directly (their `children: Vec<usize>` is
  the walk), which is exactly the protocol the comments in this area are full of war stories about
  — dup-key panics, vanishing subtrees, ghost flights — so it needs its own round with the full UI
  suite, not a quick edit.
- **`prune_stale_child_links`: ~250 µs per frame.** A defensive whole-arena walk, and the current
  comment insists it runs for EVERY compose (moving it into an early-returning function once took the
  repair off the default path and a stale listing reached the `[dup-key]` guard). Skipping it on frames
  that entered nothing is plausible and unproven: what needs establishing first is whether a frame with
  no Entered node can create a stale listing at all.
- **`slot_table.truncate` + `collect_live_keys`: ~190 µs per frame**, a whole-slot-tree walk that
  produces the live-key set the two reconciles consume. It is the input to the guards above, so it is
  the next thing to make incremental.
- **Layout's two whole-tree map rebuilds (~600 µs idle)**: `LayoutTransaction::new` clones
  `prev_nodes`/`prev_node_by_key` (layout writes both, so the snapshot is legitimate — but a move
  instead of a clone, or reusing the map allocations across frames, removes most of it), and
  `collect_nodes`/`collect_node_keys` rebuild them from scratch afterwards for a tree whose measurement
  folded at 0.04 µs.
- **The row loop's ~520 µs**: 800 iterations at ~650 ns (105 ns read, ~330 ns group machinery, the rest
  materialize restoring rows one by one). Attacking `changed()`'s per-call parameter allocation (~75 ns)
  or the `prev_nodes` lookup per Skipped group is now a small win, not the headline.

None of these is claimed as a bug: they are the cost of the current design, now visible and comparable,
and each is one round of work with this bench as the measuring stick. The three defects that *were*
bugs — a read that was O(tracked signals), a layout snapshot that cloned fields layout cannot write,
and a reverse graph rebuilt when its forward graph had not moved — were all found by measuring one
bucket and finding something else inside it.

## Re-running any of this

The benchmark's own traps are documented in the file where they bit, and the phase splits used for the
investigations are kept as unversioned patches in the working tree:
`target/probe/layout_trace.patch` (`WINIA_LAYOUT_TRACE=1`), `target/probe/compose_trace.patch`
(`WINIA_COMPOSE_TRACE=1`) and `target/probe/skip_prof_full.patch` / `skip_prof_refined.patch`
(`WINIA_SKIP_PROF=1`, the per-call profile that found the quadratic read and then the floor split).
They are throwaway instrumentation, not part of the framework: apply them with `git apply` to the
revision they were taken on (the subscription fix `1940a7a` for the profiler) and revert them before
committing — `cargo bench` in a clean tree prints the tables above and nothing else, and
`cargo bench -p winia -- container` runs just the loop control scene.
