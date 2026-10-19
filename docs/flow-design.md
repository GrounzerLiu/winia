# Flows: observing state from outside a composition

> Status: **layers 1–3 implemented** — `State::changes`, `snapshot_flow`, `debounce`. The rest of
> layer 3 (`sample`, interaction sequences) is planned here, not built.

## 1. The problem

winia's reactive model is pull-based and pass-scoped: `State::get()` registers a dependency on the
composition or layout pass that is running, and a write marks exactly the slots that read it. That
covers "the UI follows the state". It does not cover three things a UI framework is asked for:

1. **Watching a value from a coroutine.** There is no public way to say "wake me when this changes";
   `StateSignal::subscribe` and the dependency frames are `pub(crate)` and tied to a `Composer`.
2. **Operators.** Debounce, throttle, `combine`, `sample` have no home: the only bridge is
   `observe_watch(ctx, rx, initial)`, which requires the caller to have built a `tokio::sync::watch`
   channel first and gives back a `State`, not something you can `map` over.
3. **Derived flows.** Compose's `snapshotFlow { firstVisibleItemIndex }` recomputes only the states
   its block read, and emits only when the result changes. The equivalent here is hand-rolling a
   poll loop or a `LaunchedEffect` per branch condition.

The gap shows up as workarounds: debounced search and scroll-driven bars are written with a
`LaunchedEffect` that sleeps and re-reads, or with component-specific special cases.

## 2. What exists, and why it does not extend

| Piece | What it is | Why it is not the answer |
| --- | --- | --- |
| `State<T>` + `Animating/Visual/Backchannel` | ownerless cell + signal | public surface is `get/peek/set/update`; reads subscribe only inside a pass |
| `StateSignal` | `revision` + `Mutex<Vec<Subscriber>>` | subscribers are `Weak<ComposerSubscription>` — one consumer type |
| `ComposerSubscription` | per-composer coalesced pending queue | built for a composer's frame loop, not for a coroutine |
| `DerivedValue` | `derivedStateOf` analogue | re-evaluated during a pass; not observable |
| `observe_watch` | watch receiver -> `State` | one bridge, one direction, caller owns the channel |
| `MutableInteractionSource` | four `State<bool>` flags | Compose exposes `Flow<Interaction>`; here the sequence is lost |
| `NestedScrollConnection` | pre/post callbacks | correct as callbacks: it is synchronous negotiation |

`futures_util` is already a dependency (the debug WebSocket server uses it), so `Stream` is
available without adding a crate.

## 3. Constraints the design has to respect

1. **No global state registry.** A `StateSignal` owns its subscribers; there is no
   `StateId -> owner` map and adding one would reintroduce the leak the current design removed.
2. **Lock discipline.** `notify` runs under the signal's subscriber lock today. User callbacks must
   NOT run under it: an observer that reads the same state would re-enter `subscribe` and deadlock a
   non-reentrant mutex. This is the same rule `effect.rs` follows.
3. **Pass-scoped tracking.** `begin_compose_deps_with_queue` records reads into a TLS frame tied to a
   `ComposerSubscription`; there is no way to track an arbitrary closure outside a pass yet.
4. **Send + Sync.** Anything that crosses into a task must be `Send`; a `State<T>` is `Send` only
   when `T: Send + Sync` (the value sits behind `RwLock<T>`).
5. **Multi-window.** No process-wide "current observer" — everything is per-instance.
6. **Conflation is acceptable and preferable.** UI work does not need every intermediate value;
   it needs the latest one, once per frame at most.

## 4. Options

| Option | Verdict |
| --- | --- |
| **A. Adopt `Stream` as the only abstraction** | Yes for the transport, no as the whole story: operators of Compose's `Flow` (`debounce`, `sample`, `collectLatest`) are not in `futures`, and a `Stream` alone cannot express "recompute what the block read". |
| **B. A winia-native `Flow` trait with its own operators** | Rejected. It re-implements a well-understood abstraction, forces every adapter through winia types, and loses interop with tokio/futures combinators for no gain. |
| **C. Callback handle (`observe(f) -> Subscription`)** | Kept as the *implementation* underneath layer 1, not as the public shape: callbacks compose badly and cannot be cancelled by `select!`. |
| **D. `snapshotFlow` only** | Not enough on its own: it needs per-state observation to exist first, and the single-state case is the common one. |

**Chosen: three layers, `Stream` as the transport.**

- **L1 `State::changes() -> StateChanges<T>`** — `impl Stream<Item = T>`. Current value first, then
  each change, conflated to the latest value between polls, cancelled by `Drop`.
- **L2 `snapshot_flow(f) -> impl Stream<Item = T>`** — the `snapshotFlow` analogue, built on a
  tracking frame: run `f`, record the states it read, re-run on any of them, emit when the result
  differs. Needs a standalone dependency frame (constraint 3), which is why it is a separate layer.
- **L3 operators and interaction sequences** — `debounce`/`sample` as free functions over `Stream`,
  and (only if a real use case appears) an interaction sequence channel.

## 5. Layer 1 semantics

```rust
let changes = count.changes();                 // Stream<Item = i32>
LaunchedEffect::new(()).build(ctx, move |_| async move {
    let mut it = Box::pin(changes);
    while let Some(v) = it.next().await { println!("count is now {v}"); }
});
```

| Property | Decision | Why |
| --- | --- | --- |
| First item | the current value, immediately | a collector wants the state, not the next delta; matches `snapshotFlow` |
| Delivery | conflated: at most one item per poll | the value lives in the cell, the stream reads it at poll time |
| Dedup | by `PartialEq` at poll time | `set`/`update` already dedup, this only guards the seed and races |
| Cancellation | dropping the stream unsubscribes | no leaked observers; `Drop` removes the weak entry |
| Threading | `Stream` is `Send` when `T: Send + Sync` | usable from any task |
| Waker | stored by the observer, woken by `notify` | no runtime required to *create* a stream; one is required to *await* it |
| Event-loop wake | none | a stream is not a composer; waking the loop on every write would spin frames |

**Subscriber model.** `StateSignal` gains a lazily-created observer list
(`OnceLock<Mutex<Vec<Weak<dyn StateObserver>>>>`): app states never allocate it, and the existing
composer path (the coalesced `pending` queue) is untouched. `notify` collects live observers under
the lock, drops it, then calls `on_change` — constraint 2.

Why not unify both consumer types behind the trait now: the composer's path also participates in the
read handshake (`enqueue_if_changed`) and the per-frame `retain_signals` reconciliation. A second
list keeps that machinery exactly as audited; unification is a refactor with no user-visible gain.

## 6. Layer 2: `snapshot_flow`

```rust
let collapsed = snapshot_flow(move || index(state.clone()) > 0);
// -> impl Stream<Item = bool>, emitted only when the boolean flips
```

No ctx, and nothing is composed: it is a plain function returning a stream.

**Mechanism, as built.** The plan above started from "generalize the dependency frame's queue to a
`dyn StateObserver`". Reading the code showed a cheaper seam: `record_dep` writes the read into the
thread-local buffer *before* it looks at the recorder queue, so a frame pushed with **no queue at
all** already records reads and subscribes nothing. `begin_tracking_frame()` is that frame — three
lines, and the composer's typed queue never had to move.

`poll_next` then: store the waker, push a tracking frame, run the closure, `take_deps()` for the
signals it read, reconcile the subscription set against the previous run, and compare the value with
the last emitted one.

**The value is only accepted from a run whose reads were already subscribed.** Because the
subscription is installed after the read, a write that lands in between reaches nobody: the stream
would emit the pre-write value and then park with no wake. So a run that read a state the tracker was
not listening to is discarded — reconcile, run again, and accept the first value whose reads were all
covered (bounded at four passes). In the steady state that is the first run; a first poll or a branch
change costs one extra run. If the bound is ever exhausted — a closure whose reads keep growing, or
one that alternates between read sets — the value of the LAST pass is emitted even though it was not
covered: it can be stale by a write that landed in that window, and it is corrected on the next write
to any state now subscribed. Pinned by `a_write_during_the_computation_is_not_lost`, where the
closure writes the state it just read.

**Subscription reconciliation matters.** The set is replaced, not added to: a closure whose reads
depend on a branch (read `x` only while `flag` is true) would otherwise keep waking on `x` after the
branch turns off. Pinned by `a_branch_that_stops_reading_a_state_unsubscribes_from_it`.

**Known costs.** One run of the closure per notification batch (two when a state is read for the
first time, and up to four for a closure whose read set never settles) rather than one per state read
— fine
for the intended "watch a threshold" usage, wrong for a closure that is expensive per state change
(compose the work out of it, or debounce at the collector). And a closure that writes a state it
reads is a feedback loop with no natural stop; the caller owns that.

## 7. Layer 3: operators

```rust
let quiet = debounce(query.changes(), Duration::from_millis(300));
```

`debounce` is the operator UI code actually reaches for: a search field that fires once the user
stops typing, a scroll position that has to settle before it is published. It is a `Stream`
adapter in `flow/ops.rs`, not a method on anything.

Semantics, each pinned by a test under a paused tokio clock:

- an item that arrives before the period elapses **replaces** the pending one and restarts the
  clock, so a burst of N becomes one emission of the last value;
- the end of the upstream does not drop the pending value — it still waits out its period, then the
  stream finishes;
- the timer is created on the first item, inside the poll, so a tokio runtime with time enabled is
  required (the same requirement `LaunchedEffect` has).

`sample` (emit the latest value once per period) is not built: `snapshot_flow` already conflates
by frame, which covers the scroll-driven case, and no caller has needed the tick-aligned variant
yet. Interaction sequences remain unbuilt for the same reason — no use case has asked for order
rather than state.

## 8. Non-goals

- **A `Flow` type of our own.** `Stream` + `futures`/`tokio-stream` operators is the transport.
- **Hot vs cold distinction as a public concept.** Both streams are hot in the sense that they
  subscribe when they are created and keep receiving regardless of who polls; `changes()` supports
  any number of collectors, each with its own subscription.
- **Replacing the composer's dependency machinery.** L1's observer list is additive on purpose.
- **Backpressure.** Conflation is the policy; a slow collector sees the latest value, not a queue.

## 9. Risks

1. **Observer leaks.** A `StateChanges` dropped without polling still holds an `Arc`; `Drop`
   unsubscribes, and the weak list prunes dead entries on the next `notify`.
2. **Waker races.** Store the waker before reading the value in `poll_next`; a change in between is
   then either seen by the read or delivered by the wake. For L2 that argument needs the subscription
   to exist already, which is why a value is only accepted from a run whose reads were all
   subscribed (see §6) — the first run of a poll may not be.
3. **Unsubscribe vs notify.** A notification may arrive after `Drop`; the observer is already gone
   from the list, and a late wake on a dropped stream is a no-op.
4. **States that never notify.** `Visual` and `Backchannel` writes do not notify at all, so
   `changes()` over one of them emits the seed and nothing else, and a `snapshot_flow` reading one
   only recomputes when some other read state moves.
