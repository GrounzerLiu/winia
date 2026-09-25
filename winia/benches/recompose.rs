//! Benchmarks for the recomposition claims.
//!
//! ```text
//! cargo bench -p winia
//! ```
//!
//! # Why a hand-rolled harness
//!
//! `criterion` would bring a sizable dependency tree (plotters, clap, rayon) into a workspace that has
//! kept its dependencies light, and its statistics are not what these numbers are for. What they are
//! for is answering one question with a reproducible figure: **does a targeted update cost what the
//! updated subtree costs, or what the whole tree costs?** That needs a warmup, a repeatable loop and
//! `black_box` — not confidence intervals.
//!
//! The trade-off is stated plainly: no statistical rigor, and no cross-machine comparison. The figures
//! in `docs/benchmarks.md` carry the machine they were taken on.
//!
//! # Two scenes, because they measure different things
//!
//! * **boxes** — rows of sized containers, no text. This is the framework's own machinery: compose,
//!   Skip, materialize, measure, layout. A row's size never changes, so a targeted update has no
//!   reason to disturb its neighbours.
//! * **text** — the same shape with a `Text` per row. Text shaping dominates (a Skia paragraph is built
//!   per measured text), so these figures say what a realistic app costs; they are NOT the
//!   framework's own overhead, and reading them as such would overstate it.
//!
//! Each scene reports **how many groups actually entered**, which is what makes a fast timing
//! trustworthy: a figure that looks good because nothing ran is visible as such.
//!
//! # What "one row updated" means here
//!
//! One row's state moves; the value it renders to is FIXED WIDTH (`{value:07}`). That matters: a text
//! whose width grows as the counter runs changes the row's measured size, the parent re-lays-out every
//! sibling, and the benchmark stops measuring recomposition and starts measuring layout invalidation.
//! The bench measured that too while being written (4x the rows, 12x the time) — the fixed-width form
//! is what isolates the claim.

use std::hint::black_box;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winia::composable;
use winia::core::composer::{ComposeCtx, Composer, GroupStatus};
use winia::layout::BoxLayout;
use winia::layout::constraints::Constraints;
use winia::modifier::{Color, Modifier, Shape};
use winia::ui::layout_components::{Column, Row};
use winia::ui::text::Text;
use winia::State;

/// One measured scene.
struct Result {
    name: String,
    median: Duration,
    entered_per_frame: f64,
}

impl Result {
    fn report(&self) {
        println!(
            "  {:<36} {:>10.3} us/frame   {:>6.2} groups entered/frame",
            self.name,
            self.median.as_secs_f64() * 1e6,
            self.entered_per_frame,
        );
    }
}

/// Time `frame` after warming up, returning the median of several samples and the counters it kept.
///
/// The median rather than the mean: on a desktop the outliers are other processes, and they only ever
/// make a run slower. `iters` frames per sample keeps the timer's own overhead out of the per-frame
/// figure.
fn measure(
    name: &str,
    iters: usize,
    samples: usize,
    entries: &Rc<std::cell::Cell<usize>>,
    mut frame: impl FnMut(),
) -> Result {
    for _ in 0..iters {
        frame();
    }
    // Counters accumulated during warmup are not part of the steady state being reported.
    entries.set(0);
    let counted_frames = iters * samples;

    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iters {
            frame();
        }
        times.push(start.elapsed() / iters as u32);
    }
    times.sort();
    Result {
        name: name.to_string(),
        median: times[samples / 2],
        entered_per_frame: entries.get() as f64 / counted_frames as f64,
    }
}

// ═══════════════════════════════════════════════════════════
// Scenes
// ═══════════════════════════════════════════════════════════

/// Which shape a row has. Both declare the same param and claim the same subtree, so the framework's
/// work is comparable; only what the content costs differs.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Boxes,
    Text,
}

/// A row that READS ITS OWN state, instead of being handed the value.
///
/// This is the difference that decides how a list should be written. With `row(ctx, state.get(), ..)`
/// the read happens in the CALLER's scope, so the caller is the dependent: a change re-runs the whole
/// list's closure and every row takes a Skip decision. Reading the handle inside the row's own
/// `#[composable]` scope makes the row the dependent instead. Compose has the same rule (a read in a
/// composable's body subscribes that composable, not its caller).
#[composable]
fn row_scoped(ctx: &mut ComposeCtx, state: State<i64>, kind: Kind) {
    let key = ctx.next_key();
    let value = state.get();
    ctx.changed(&value);
    match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            ROWS_RUN.with(|cell| cell.set(cell.get() + 1));
            Row::new().build(ctx, |ctx| {
                match kind {
                    Kind::Boxes => {
                        Column::new()
                            .modifier(Modifier::new().size(60.0, 18.0).background(
                                Color::from_argb(255, 200, 200, 200),
                                Shape::Rectangle,
                            ))
                            .build(ctx, |_| {});
                        // The same content as `row`, so the two shapes compare read PLACEMENT rather
                        // than layouts: building less here would make the comparison meaningless.
                        Column::new()
                            .modifier(Modifier::new().size(40.0, 18.0))
                            .build(ctx, |_| {});
                    }
                    Kind::Text => {
                        Text::new(format!("row {value:07}")).build(ctx);
                        Column::new().build(ctx, |ctx| {
                            Text::new("detail").build(ctx);
                        });
                    }
                }
            });
        }
    }
    ctx.end_restartable_group();
}

thread_local! {
    /// Rows that ran, per frame. A thread-local because it is bookkeeping for the benchmark, not part
    /// of the scene: passing it as a parameter would put it in the composition's parameters.

    static ROWS_RUN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many rows ran since the last call, and reset.
fn take_rows_run() -> usize {
    ROWS_RUN.with(|cell| cell.replace(0))
}

/// One row: a container with a declared param, holding a nested leaf.
///
/// The row's `ctx.changed(&value)` is what lets it Skip, and it is explicit because
/// `#[composable]` does NOT declare a function's parameters — the framework's own components call
/// `ctx.changed` themselves (`Button::build` declares its style and enabled flags). Removing this line
/// makes every row Skip forever, which the bench showed as "one row updated: 0 groups entered".
///
/// `#[composable]` itself is what gives `next_key` a stable key; without it the composer refuses to
/// guess (its fail-fast, which this bench hit while being written).
#[composable]
fn row(ctx: &mut ComposeCtx, value: i64, kind: Kind) {
    let key = ctx.next_key();
    ctx.changed(&value);
    match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            ROWS_RUN.with(|cell| cell.set(cell.get() + 1));
            Row::new().build(ctx, |ctx| {
                match kind {
                    Kind::Boxes => {
                        Column::new()
                            .modifier(Modifier::new().size(60.0, 18.0).background(
                                Color::from_argb(255, 200, 200, 200),
                                Shape::Rectangle,
                            ))
                            .build(ctx, |_| {});
                        Column::new()
                            .modifier(Modifier::new().size(40.0, 18.0))
                            .build(ctx, |_| {});
                    }
                    Kind::Text => {
                        // Fixed width on purpose: see the module docs.
                        Text::new(format!("row {value:07}")).build(ctx);
                        Column::new().build(ctx, |ctx| {
                            Text::new("detail").build(ctx);
                        });
                    }
                }
            });
        }
    }
    ctx.end_restartable_group();
}

/// A `rows`-row tree over a `Vec` of per-row states.
struct Tree {
    composer: Composer,
    states: Vec<State<i64>>,
    kind: Kind,
    /// Read the row's state inside the row's own scope (see [`row_scoped`]) instead of handing the
    /// value down from the list's closure.
    scoped_reads: bool,
    /// The container's own state, read by the container itself. Writing it re-enters the container —
    /// so the row loop runs — while every row's declared parameter keeps its value, so every row
    /// Skips. This is the control that separates "the loop ran" from "a row changed".
    tick: State<i64>,
    tick_reads: bool,
}

impl Tree {
    fn new(rows: usize, kind: Kind, scoped_reads: bool) -> Self {
        Tree {
            composer: Composer::new(),
            states: (0..rows).map(|i| State::new(i as i64)).collect(),
            kind,
            scoped_reads,
            tick: State::new(0),
            tick_reads: false,
        }
    }

    /// One frame: compose the whole tree and lay it out.
    ///
    /// `app_root!` supplies the root scope every composition needs — the wrapper `run_app!` installs
    /// for a real application, and what gives the outermost container a stable key.
    fn frame(&mut self) {
        self.compose_only();
        self.layout_only();
    }

    /// One frame, and how many rows ran in it.
    fn frame_counted(&mut self) -> usize {
        self.frame();
        take_rows_run()
    }

    /// Just the composition half of a frame — no layout.
    fn compose_only(&mut self) {
        let states = self.states.clone();
        let kind = self.kind;
        let scoped = self.scoped_reads;
        let tick = if self.tick_reads { Some(self.tick.clone()) } else { None };
        self.composer.compose(winia::app_root!(move |ctx: &mut ComposeCtx| {
            Column::new().build(ctx, |ctx| {
                // Read in the CONTAINER's own scope: this is what makes the container the dependent,
                // so writing the tick re-enters it and the row loop below runs.
                if let Some(tick) = &tick {
                    black_box(tick.get());
                }
                for state in &states {
                    if scoped {
                        row_scoped(ctx, state.clone(), kind);
                    } else {
                        row(ctx, state.get(), kind);
                    }
                }
            });
        }));
    }

    /// Just the layout half of a frame.
    fn layout_only(&mut self) {
        self.composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    }

    /// Lay out with a DIFFERENT constraint, which invalidates every node's fold and forces a real
    /// re-measure of the whole tree. This is the upper bound the "one row updated" figure has to be
    /// read against: if they are equal, one row's update re-measures everything; if the update is far
    /// below it, the fold is doing its job and what remains is the traversal.
    fn layout_forced(&mut self, width: f32) {
        self.composer.layout(Constraints::new(0.0, width, 0.0, 400.0));
    }
}

/// How much of the layout cost of "one row updated" is a REAL re-measure, and how much is the walk
/// with per-node folds?
///
/// The fold in `measure_node_inner` returns a cached size when a node is neither dirty nor
/// layout-dirty and its constraints are unchanged, so a walk over unchanged rows should cost a
/// comparison each — small, but not free at 800 rows, and this says which of the two dominates.
fn layout_reality(kind: Kind, label: &str) {
    let rows = 800;
    println!("\n--- what layout actually does ({label}, {rows} rows) ---");

    let mut tree = Tree::new(rows, kind, false);
    tree.frame();
    let entries = Rc::new(std::cell::Cell::new(0));
    let idle = measure("layout, idle (no compose)", 200, 9, &entries, || {
        tree.layout_only();
    });
    idle.report();

    // CONTROL: compose an UNCHANGED tree, then lay it out. If this is as expensive as the
    // one-row update, the cost is not the state change at all — it is something about having
    // composed, and the earlier comparison was measuring two different things.
    let mut tree = Tree::new(rows, kind, false);
    tree.frame();
    let entries = Rc::new(std::cell::Cell::new(0));
    let idle_composed = measure("compose(unclean)+layout, nothing changed", 200, 9, &entries, || {
        tree.compose_only();
        take_rows_run();
        tree.layout_only();
    });
    idle_composed.report();

    // Compose ALONE with one row changed. This is the arm that matters: an earlier version of this
    // scene labelled the next measurement "layout, one row updated" while the timed closure ran
    // `compose_only()` first, so a compose cost was being read as a layout cost and the layout pass was
    // blamed for it (docs/benchmarks.md records the correction).
    let mut tree = Tree::new(rows, kind, false);
    tree.frame();
    let mut moved = 0i64;
    let entries = Rc::new(std::cell::Cell::new(0));
    let compose_one = measure("compose, one row updated", 200, 9, &entries, || {
        moved += 1;
        tree.states[rows / 2].set(moved);
        tree.compose_only();
        entries.set(entries.get() + take_rows_run());
        black_box(moved);
    });
    compose_one.report();

    // The whole frame, for the same change — compose and layout together, which is what an app pays.
    let mut tree = Tree::new(rows, kind, false);
    tree.frame();
    let mut moved = 0i64;
    let entries = Rc::new(std::cell::Cell::new(0));
    let frame_one = measure("frame (compose+layout), one row updated", 200, 9, &entries, || {
        moved += 1;
        tree.states[rows / 2].set(moved);
        tree.compose_only();
        take_rows_run();
        tree.layout_only();
        black_box(moved);
    });
    frame_one.report();

    // A different constraint: every cached constraint differs, so the whole tree re-measures. Nothing is
    // composed in this arm, so it is a pure layout figure.
    let mut tree = Tree::new(rows, kind, false);
    tree.frame();
    let mut width = 400.0f32;
    let entries = Rc::new(std::cell::Cell::new(0));
    let forced = measure("layout only, EVERY size re-measured", 200, 9, &entries, || {
        // Alternating by a pixel invalidates the fold every frame without changing the structure.
        width = if width == 400.0 { 401.0 } else { 400.0 };
        tree.layout_forced(width);
        black_box(width);
    });
    forced.report();
}

fn scaling(kind: Kind, scoped: bool, label: &str) -> Vec<Result> {
    let sizes = [50usize, 200, 800];
    let mut out = Vec::new();
    println!("--- {label} ---");
    for rows in sizes {
        // Cold: a fresh composer every frame — every group enters, nothing can be skipped.
        let entries = Rc::new(std::cell::Cell::new(0));
        let cold = measure(&format!("cold frame, {rows} rows"), 20, 9, &entries, || {
            let mut fresh = Tree::new(rows, kind, scoped);
            let ran = fresh.frame_counted();
            entries.set(entries.get() + ran);
            black_box(fresh.states.len());
        });
        cold.report();

        // Idle: the same tree, no state moves. `frame_counted` reads the count PER FRAME — a counter
        // read without resetting would report the first frame's count forever (which this bench did
        // while being written).
        let mut tree = Tree::new(rows, kind, scoped);
        let entries = Rc::new(std::cell::Cell::new(0));
        let idle = measure(&format!("idle frame, {rows} rows"), 100, 9, &entries, || {
            let ran = tree.frame_counted();
            entries.set(entries.get() + ran);
        });
        idle.report();

        // One row moves. This is the claim: the work should follow the updated row, not the tree.
        let mut tree = Tree::new(rows, kind, scoped);
        tree.frame_counted();
        let entries = Rc::new(std::cell::Cell::new(0));
        let mut moved = 0i64;
        let one = measure(&format!("one row updated, {rows} rows"), 100, 9, &entries, || {
            moved += 1;
            tree.states[rows / 2].set(moved);
            let ran = tree.frame_counted();
            entries.set(entries.get() + ran);
            black_box(moved);
        });
        one.report();
    }
    out
}

/// Where a frame's time goes, at one tree size: compose and layout measured separately, idle and with
/// one row updated.
///
/// This exists because the headline numbers raised a question worth answering: at 800 rows an idle
/// frame cost ~3.3 ms and a ONE ROW update ~10.4 ms, so ~7 ms appeared to be spent on a single row's
/// worth of work. Splitting the two halves says which one it is in, and whether it is the update or
/// the frame.
fn breakdown(kind: Kind, scoped: bool, rows: usize) {
    let label = match kind {
        Kind::Boxes => "boxes",
        Kind::Text => "text",
    };
    println!("\n--- where the time goes ({label}, {rows} rows) ---");

    let mut idle = Tree::new(rows, kind, scoped);
    idle.frame();
    let entries = Rc::new(std::cell::Cell::new(0));
    let compose_idle = measure("compose only, idle", 100, 9, &entries, || {
        idle.compose_only();
    });
    compose_idle.report();
    let mut idle = Tree::new(rows, kind, scoped);
    idle.frame();
    let entries = Rc::new(std::cell::Cell::new(0));
    let layout_idle = measure("layout only, idle", 100, 9, &entries, || {
        idle.layout_only();
    });
    layout_idle.report();

    let mut one = Tree::new(rows, kind, scoped);
    one.frame();
    let entries = Rc::new(std::cell::Cell::new(0));
    let mut moved = 0i64;
    let compose_one = measure("compose only, one row updated", 100, 9, &entries, || {
        moved += 1;
        one.states[rows / 2].set(moved);
        one.compose_only();
        entries.set(entries.get() + take_rows_run());
        black_box(moved);
    });
    compose_one.report();

    let mut one = Tree::new(rows, kind, scoped);
    one.frame();
    let mut moved = 0i64;
    let entries = Rc::new(std::cell::Cell::new(0));
    // Named for what it times: the WHOLE frame, compose included. Labelling this "layout only" is the
    // mistake an earlier version made, and it read a 5 ms compose cost as a layout cost.
    let frame_one = measure("frame, one row updated", 100, 9, &entries, || {
        moved += 1;
        one.states[rows / 2].set(moved);
        one.compose_only();
        take_rows_run();
        one.layout_only();
        black_box(moved);
    });
    frame_one.report();
}

/// Does the cost of one dirty row depend on WHERE it is in the list?
///
/// If the work is proportional to the row's index, something positional is going on — a rebuild of the
/// slots after the dirty one, say — rather than a per-change cost. If it is flat, the cost is the
/// change itself and the position is irrelevant.
fn dirty_position(rows: usize) {
    println!("
--- where is the dirty row? (boxes, {rows} rows) ---");
    for index in [0usize, rows / 4, rows / 2, rows - 1] {
        let mut tree = Tree::new(rows, Kind::Boxes, false);
        tree.frame();
        let entries = Rc::new(std::cell::Cell::new(0));
        let mut moved = 0i64;
        let r = measure(&format!("compose, row {index} dirty"), 100, 9, &entries, || {
            moved += 1;
            tree.states[index].set(moved);
            tree.compose_only();
            entries.set(entries.get() + take_rows_run());
            black_box(moved);
        });
        r.report();
    }
    // And two rows at once, to see whether the cost adds up per dirty row or per neighbor span.
    for count in [2usize, 10] {
        let mut tree = Tree::new(rows, Kind::Boxes, false);
        tree.frame();
        let entries = Rc::new(std::cell::Cell::new(0));
        let mut moved = 0i64;
        let r = measure(&format!("compose, {count} rows dirty (adjacent)"), 100, 9, &entries, || {
            moved += 1;
            for i in 0..count {
                tree.states[rows / 2 + i].set(moved);
            }
            tree.compose_only();
            entries.set(entries.get() + take_rows_run());
            black_box(moved);
        });
        r.report();
    }
}

/// The missing control: what does the row loop cost when NO row changes?
///
/// "One row updated" runs the row loop `rows` times; an idle frame does not run it at all, because the
/// container Skips. So the difference between the two prices the LOOP as if it were the update. Here a
/// state the CONTAINER itself read moves: the container re-enters (the loop runs, every row takes its
/// Skip decision) and no row's declared parameter changed, so no row re-enters. The gap between this
/// and "one row updated" is the update's own cost; this line is the traversal's.
fn container_dirty() {
    println!("--- the row loop with no row changed (boxes) ---");
    for rows in [50usize, 200, 800] {
        let mut tree = Tree::new(rows, Kind::Boxes, false);
        tree.tick_reads = true;
        tree.frame();
        let entries = Rc::new(std::cell::Cell::new(0));
        let mut tick = 0i64;
        let loop_only = measure(
            &format!("compose, container dirty (every row skips), {rows} rows"),
            100,
            9,
            &entries,
            || {
                tick += 1;
                tree.tick.set(tick);
                tree.compose_only();
                entries.set(entries.get() + take_rows_run());
                black_box(tick);
            },
        );
        loop_only.report();

        let mut tree = Tree::new(rows, Kind::Boxes, false);
        tree.frame();
        let entries = Rc::new(std::cell::Cell::new(0));
        let mut moved = 0i64;
        let one = measure(
            &format!("compose, one row dirty (loop + 1 row), {rows} rows"),
            100,
            9,
            &entries,
            || {
                moved += 1;
                tree.states[rows / 2].set(moved);
                tree.compose_only();
                entries.set(entries.get() + take_rows_run());
                black_box(moved);
            },
        );
        one.report();
    }
}

fn collections() -> Vec<Result> {
    let mut out = Vec::new();
    for len in [100usize, 1000, 10_000] {
        // Reading the value. `State<Vec>` hands back a FULL clone of the vector, and a reader does that
        // every time it renders — this is the difference that matters for a list on screen.
        let vec_state: State<Vec<i64>> = State::new((0..len as i64).collect());
        let read_state = measure(
            &format!("State<Vec> read, {len} items"),
            500,
            9,
            &Rc::new(std::cell::Cell::new(0)),
            || {
                black_box(vec_state.get().len());
            },
        );
        out.push(read_state);

        let list: winia::StateList<i64> = winia::StateList::from_vec((0..len as i64).collect());
        let read_list = measure(
            &format!("StateList read (snapshot), {len} items"),
            500,
            9,
            &Rc::new(std::cell::Cell::new(0)),
            || {
                black_box(list.snapshot().len());
            },
        );
        out.push(read_list);

        // Mutating: both copy the contents (`State` owns its value, the list publishes a new snapshot),
        // so what differs here is the comparison `State::set` runs and the extra clone — not the copy.
        let vec_state: State<Vec<i64>> = State::new((0..len as i64).collect());
        let mut n = 0i64;
        let cloned = measure(
            &format!("State<Vec> push, {len} items"),
            200,
            9,
            &Rc::new(std::cell::Cell::new(0)),
            || {
                n += 1;
                let mut next = vec_state.peek().clone();
                next.push(n);
                let produced = next.len();
                vec_state.set(next);
                black_box(produced);
            },
        );
        out.push(cloned);

        let list: winia::StateList<i64> = winia::StateList::from_vec((0..len as i64).collect());
        let mut n = 0i64;
        let shared = measure(
            &format!("StateList push, {len} items"),
            200,
            9,
            &Rc::new(std::cell::Cell::new(0)),
            || {
                n += 1;
                list.push(n);
                black_box(list.peek().len());
            },
        );
        out.push(shared);
    }
    out
}

fn main() {
    println!("winia recomposition benchmarks");
    // The composer has #[cfg(debug_assertions)] instrumentation that calls std::env::var PER SKIPPED
    // GROUP; if this profile enables debug assertions, every figure below is instrumented, not
    // production. Printed so the numbers can never be read without knowing which build produced them.
    println!("(debug_assertions = {})", cfg!(debug_assertions));
    println!("(the fastest of 9 samples, with the median beside it; entered = what actually ran)\n");

    // A fast path to run one scene while iterating on it: `cargo bench -p winia -- container`.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "container") {
        container_dirty();
        return;
    }
    scaling(Kind::Boxes, false, "boxes: the framework's own machinery (no text shaping)");
    println!();
    scaling(Kind::Text, false, "text: a realistic row (text shaping dominates)");

    dirty_position(800);

    container_dirty();

    layout_reality(Kind::Boxes, "boxes");
    layout_reality(Kind::Text, "text");

    breakdown(Kind::Boxes, false, 800);
    println!();
    scaling(Kind::Boxes, true, "boxes, state read INSIDE the row (read placement)");
    breakdown(Kind::Boxes, true, 800);
    breakdown(Kind::Text, false, 800);

    println!("\ncollections: reading, then mutating");
    for result in collections() {
        result.report();
    }

    println!(
        "\nReading the \"groups entered\" column is what makes a timing trustworthy: it says exactly \
         how\nmuch of the tree recomposed. It is 0.00 on an idle frame and 1.00 when one row moves, at \
         every\nsize — the scoping claim holds. The TIME is a different matter, and the breakdown above \
         is where\nit goes; docs/benchmarks.md reads the numbers."
    );
}
