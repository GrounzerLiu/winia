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

## What does not hold: the frame is O(tree)

Five rows in the fast column are µs/frame. Boxes = sized containers only; text = a `Text` per row.

| rows | cold | idle | one row moved |
|---|---|---|---|
| 50 | 679 | 125 | 164 |
| 200 | 1609 | 504 | 869 |
| 800 | 13422 | 3452 | 11393 |

| rows (text) | cold | idle | one row moved |
|---|---|---|---|
| 50 | 896 | 176 | 299 |
| 200 | 8015 | 837 | 1355 |
| 800 | 43581 | 6308 | 14315 |

16x the rows costs ~28x an idle frame and ~70x a one-row update. Entering ONE group does not make the
frame cheap, and the gap between idle and one-row widens with the tree rather than staying fixed.

The breakdown at 800 rows (boxes) says where it goes:

| | compose | layout |
|---|---|---|
| idle | 1722 | 1322 |
| one row moved | 7001 | 8836 |

So a one-row update spends ~5.3 ms more in composition and ~7.5 ms more in layout than an idle frame
of the same tree. Composition's extra is the ancestor chain re-running its closures — the read that
drove the update sits in the list's scope, so the list's closure re-runs and every row takes a cheap
Skip decision.

The layout figure is the larger one, and it is not confined to the row that changed. That row's own
share of an idle layout is ~1/800 of 1322 µs, under 2 µs; the extra 7.5 ms is on the order of four
thousand rows' worth. The frame's own sizes are fixed by `Modifier::size` in the box scene, so nothing
changed size and yet layout got 7x more expensive. **A re-entered subtree's layout invalidation is not
local** — that is what the numbers say. (Which internal step spends it — re-materialising nodes,
re-running the measure walk, or the parent's policy visiting every child — is not something this
benchmark distinguishes; it measures the frame, not the pipeline. Pinning that down is the first task
of the optimization round, not a conclusion of this one.)

### Read placement does not matter here

`row_scoped` (the row reads its own state handle, so the row's scope is the dependent) was measured
against `row` (the list reads and passes the value down). At 800 rows: idle 3460 vs 3452, one row
11207 vs 11393 — **no difference beyond noise**. The frame is dominated by the layout walk and the
composition skip-walk, not by how many closures re-run, so a list does not need to be contorted for
read placement. (The two shapes are otherwise identical; the first version of this scene built less
content in the scoped row and appeared to win, which is the kind of comparison error this document is
annotated to avoid.)

## Where a one-row update actually goes (and one correction)

An earlier version of this document said "the layout figure is pathological: one row's update costs 4x
more than re-measuring the whole tree". **That was a measurement artifact of my own making**, and it is
worth recording because it is easy to repeat: the arm labelled "layout, one row updated" ran
`compose_only()` inside the timed closure, so a compose cost was being read as a layout cost. The
benchmark's labels now say exactly what each arm times.

With the arms separated (boxes, 800 rows, after the fix below):

| arm | fast sample |
|---|---|
| compose only, nothing changed | ~1200 µs |
| **compose only, one row updated** | **6250 µs** |
| layout only, nothing composed | 700 µs |
| layout only, every size re-measured | 1495 µs |
| frame (compose + layout), one row updated | 7980 µs |

Frame minus compose puts the layout of a one-row update at **~1730 µs** — between an idle layout (700)
and a full re-measure (1495). **Layout is fine.** The cost is all in composition: **+5 ms for one dirty
row**, and that is the number to explain.

### It is a binary cost, not a per-row one

| compose, boxes 800 rows | fast sample | groups entered |
|---|---|---|
| row 0 dirty | 6438 µs | 1 |
| row 400 dirty | 6310 µs | 1 |
| row 799 dirty | 6277 µs | 1 |
| 2 adjacent rows dirty | 6744 µs | 2 |
| 10 adjacent rows dirty | 6707 µs | 10 |

Flat in WHERE the dirty row is and flat in HOW MANY rows are dirty. One dirty row costs the same as
ten. So this is not per-change work; something switches on when anything is dirty.

### The mechanism

A phase split inside `compose()` (a temporary `WINIA_COMPOSE_TRACE`, kept as an unversioned patch in
the working tree) puts **5.0 ms of the 5.2 ms in the content closure itself** — the loop over rows.

The loop does not run at all when nothing is dirty: the container's slot is Clean, so
`start_restartable_group` Skips it and its body is never entered (measured: content = 0.7 µs). When one
row's state changes, `mark_dirty_path_scope` marks the dirty slot **and every ancestor** dirty — the
ancestors' bodies must run to re-check their children. In this scene the state read sits in the
container's scope, so the container is the dependent, its 800-row loop re-executes, and every row takes
a Skip decision. **799 Skip decisions cost ~5 ms, about 6 µs each.**

That is the shape to attack, and it is a different problem from the one this section originally
described: not layout invalidation, but **what a Skip decision costs** (a slot lookup, a `changed`
parameter push with a heap allocation per call, a modifier comparison) and **how far a dirty mark
propagates**.

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

- **The ancestor re-run is the big one: ~5 ms of the 800-row one-row update.** A state read in a
  container's scope makes the container the dependent; marking it dirty marks its ancestors, and their
  bodies re-execute — here the container's body is the loop over 800 rows, so 799 Skip decisions cost
  ~5 ms. Two levers, and they are separate pieces of work:
  (a) what a Skip decision costs (~6 µs each: a slot lookup, a `changed` parameter push that allocates,
  a modifier comparison);
  (b) how far a dirty mark propagates (does every ancestor need to *enter*, or only to re-check?).
- **`collect_nodes` + `collect_node_keys` rebuild whole-tree maps on every layout** (~0.8 ms of a 4001
  node layout, second only to the transaction). Measured, not yet attacked.
- **The idle frame is not free** (~2.0 ms at 800 rows): composing a tree in which every group Skips
  still walks the slot tree and replays each skipped group's structure.

None of these is claimed as a bug: they are the cost of the current design, now visible and
comparable. Each is a candidate for its own round, with this bench as the measuring stick.

## Re-running any of this

The benchmark's own traps are documented in the file where they bit, and the two phase splits used for
the investigation are kept as unversioned patches in the working tree
(`target/probe/layout_trace.patch`, `target/probe/compose_trace.patch`; apply with `git apply` and run
with `WINIA_LAYOUT_TRACE=1` / `WINIA_COMPOSE_TRACE=1`). They are throwaway instrumentation, not part of
the framework: `cargo bench` in a clean tree prints the tables above and nothing else.
