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
(before either of the two fixes below) the same two ratios were 28x and 70x — the difference was a
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

- **Composition's O(tree) floor, ~1.2 ms at 800 rows with nothing entered** (`compose only, idle` 1225
  µs). The instrumented split puts it in `materialize` (~540 µs), `reconcile_compose_deps` (~510 µs),
  `slot_table.truncate` + `collect_live_keys` (~180 µs) and setup (~110 µs) — all per-frame walks over
  the whole composition, none of them doing anything with the rows that did not change. This is now the
  largest single item in a one-row update (1653 µs), larger than the update's own loop.
- **Layout's O(tree) floor, ~780 µs idle**, of which `LayoutTransaction::new` clones two whole-tree maps
  (`prev_nodes`, `prev_node_by_key`) that layout immediately clears and rebuilds, and
  `collect_nodes`/`collect_node_keys` rebuild them again. Snapshotting-by-move and reusing the map
  allocations is the obvious shape; it needs the same care the first layout fix took (what layout may
  write, and what a rollback must restore).
- **The loop's 800 iterations, +520 µs** (~650 ns each: ~105 ns read, ~330 ns group machinery, the rest
  materialize restoring rows one by one). Attacking `changed()`'s per-call parameter allocation
  (~75 ns) or the `prev_nodes` lookup per Skipped group is now a small win, not the headline.

None of these is claimed as a bug: they are the cost of the current design, now visible and comparable.
Each is a candidate for its own round, with this bench as the measuring stick. A read that is O(tracked
signals) *was* a bug and is fixed above; the difference is worth keeping in mind when reading a target.

## Re-running any of this

The benchmark's own traps are documented in the file where they bit, and the phase splits used for the
investigations are kept as unversioned patches in the working tree:
`target/probe/layout_trace.patch` (`WINIA_LAYOUT_TRACE=1`), `target/probe/compose_trace.patch`
(`WINIA_COMPOSE_TRACE=1`) and `target/probe/skip_prof.patch` (`WINIA_SKIP_PROF=1`, the per-call
profile that found the quadratic read). Apply with `git apply`; they are throwaway instrumentation,
not part of the framework — `cargo bench` in a clean tree prints the tables above and nothing else,
and `cargo bench -p winia -- container` runs just the loop control scene.
