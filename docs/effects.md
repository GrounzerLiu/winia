# Effects and the slot lifecycle hook

`effect::on_forget` / `ComposeCtx::remember_on_forget` register a callback that runs when a
composition point is removed. `LaunchedEffect` and `DisposableEffect` are built on it, and so are
`remember_infinite_transition` and `Window`'s cleanup — anything that needs "do this when I go
away" should use it too.

## 1. The hook

```rust
// Per composition point: the callback dies with the branch that registered it.
effect::on_forget(ctx, move || teardown());
```

```rust
// Per enclosing slot: the callback dies only when the whole surrounding slot goes.
ctx.remember_on_forget(move || teardown());
```

Both are the same mechanism. The difference is which slot owns the callback:

| Form | Slot it attaches to | Runs when |
|---|---|---|
| `effect::on_forget(ctx, f)` | its own `#[composable]` scope slot, at the call position | that call stops composing |
| `ctx.remember_on_forget(f)` | whatever slot is current | that whole slot is dropped |

Prefer `effect::on_forget`. A component that stops using a resource while staying composed — a
conditional `remember_infinite_transition`, say — is only handled correctly by the per-call form.

### Guarantees

- Runs **once**, when the slot is dropped. Never on a plain recomposition.
- Registering again at the same call site **replaces** the previous callback instead of stacking
  them, so a component that composes every frame does not accumulate cleanups.
- Runs for **every** way a slot can go away. Two mechanisms cover it: the collection sites that
  drop slots deliberately (tail truncation in `start_slot`, the Enter-time prune of unvisited
  children in `end_restartable_group`, end-of-frame truncation, `end_movable`), and `Slot`'s own
  `Drop`, which catches the rest — a whole `Composer` being dropped, a `movable_store` entry
  removed, a `retain` in `end_scope`. Keeping both matters: a lifecycle callback that silently
  stops running because it moved from a node (which had a `Drop`) into a slot (which did not) is
  exactly the regression this API shipped with for one commit.
- Costs **no layout node** and consumes no layout slot (see §2).
- The callback is `FnOnce + Send`; it must not assume it runs on a particular thread beyond the
  composing one, and it must not compose.

### When it runs

During the composition pass, at the moment the slot is dropped — not after the frame is applied.
The effects module documents the same caveat for effect bodies: a callback that reads layout or
another branch's `State` sees a half-built tree. See `winia/src/effect.rs` module docs.

## 2. Why it is not a leaf node

The first version of this API did not exist. Effects needed a cleanup callback and the slot table
had nowhere to put one, so `attach_cleanup` created an implicit LEAF NODE and hung the callback on
its `on_remove`.

That node was a real child of the enclosing container, so it participated in layout. Measured in a
`Column(spacing = 10)` holding two rows with an effect between them:

| | children's y | arena nodes |
|---|---|---|
| no effect | `[0.0, 29.0]` | 3 |
| effect as a leaf node | `[0.0, 29.0, 39.0]` | 4 |
| effect via `on_forget` | `[0.0, 29.0]` | 3 |

The second row moved from 29 to 39: the phantom node consumed a spacing slot, so a `LaunchedEffect`
between two rows opened a gap nothing in the layout asked for. `an_effect_adds_no_layout_node_and_no_spacing_slot`
pins both halves of the fix.

Compose has the same shape: a `RememberObserver` lives in the slot table and never materializes.

## 3. Effects

### `LaunchedEffect<T>`

```rust
LaunchedEffect::new(key).build(ctx, move |scope| async move {
    // runs when `key` changes, cancelled when the point is removed
});
```

- Starts the body once for a given key; a new key aborts the previous body and starts a new one.
- The replacement **waits for the old body to stop** (`JoinHandle` + `abort`, then await). Not
  pinned by a test: on a `current_thread` runtime the abort lands before the new task is polled
  either way, and the interleaving that needs it only happens on the multi-threaded runtime
  `run_app` builds.
- Body starts **during composition**, not after it. See the module docs.

### `DisposableEffect<T>`

```rust
DisposableEffect::new(key).build(ctx, |k| {
    let handle = install(k);
    Box::new(move || handle.remove())
});
```

- Same key semantics; the OLD cleanup runs before the new setup.
- Setup and cleanup are the caller's code and are never invoked while a framework lock is held —
  a panic in them cannot poison the effect, and re-entering the same state cannot deadlock.

### `remember_updated_state(ctx, value)`

A `State` that always holds the value from the most recent composition:

```rust
let latest = remember_updated_state(ctx, on_event);
LaunchedEffect::new(()).build(ctx, move |_| async move {
    loop {
        wait().await;
        latest.get()();      // the CURRENT callback, not the one this body started with
    }
});
```

The write is **silent** — it does not schedule a recompose. The value changed *because* this
composition ran, so a second frame would buy nothing, and a read later in the same pass already sees
the new value. What this exists for is the long-lived effect: a `LaunchedEffect::new(())` body
captures its surroundings once, and without a handle like this it keeps calling the first frame's
closure forever.

### `observe_watch(ctx, rx, initial)`

Bridges a `watch::Receiver` into a `State`. A **replaced** receiver is re-subscribed (compared with
`same_channel`); a receiver passed once and never changed costs nothing after the first frame.

The subscription is a task on the tokio runtime, and two things end it:

- **The composition point goes away** — an `on_forget` callback aborts the task. Without it the task
  outlived its slot: it is spawned with `rt.spawn`, not `CoroutineScope::spawn`, so
  `ScopeState::drop` never knew about it, and dropping an `AbortHandle` cancels nothing.
- **The receiver is replaced** — the old task is aborted and then *joined* before the new
  subscription starts. The join matters for the same reason `LaunchedEffect` has one: `abort` only
  asks, so the old task can still run the `set` that follows its last `changed()` and write the
  previous channel's value over the new one.

### `with_frame_nanos()`

Awaits the next render tick and returns a monotonic nanosecond timestamp. Driven by
`frame_tick()`, which the render loop calls per frame — a window that is not rendering does not
tick it, so use tokio timers for time-driven work.

## 4. What is NOT guaranteed

1. **Post-apply timing.** Compose runs effects after the composition commits; these run inside the
   compose pass. Moving them needs a commit hook the composer does not have. Treat an effect body
   as "runs somewhere inside the frame".
2. **Cleanup order is not defined.** Callbacks are drained into one list per removed subtree: a
   slot's own callbacks come from a `HashMap` (arbitrary order), and the slots themselves are
   visited parent first. Compose runs `onForgotten` in reverse composition order; this does not.
   A panicking callback also takes the rest of that removal with it — the callbacks after it in the
   list are dropped without running. The slot tree itself is already consistent by then (every site
   finishes the structural change first — `start_slot` runs them after pushing the new slot for
   that reason), so the composer can unwind cleanly, but those cleanups do not happen.
3. **Cancellation is cooperative.** An aborted task stops at its next await; work already done
   before that point is not undone.
4. **Only tasks spawned through the scope belong to it.** `CoroutineScope::spawn` registers the
   task, and the scope aborts its tasks when its composition point goes away — an `on_forget`
   callback, so keeping a clone alive past that point no longer extends their life (it used to:
   teardown hung off `ScopeState::drop` and therefore off the refcount). Framework code that wants
   a different lifetime spawns with `rt.spawn` and cancels the task itself; `LaunchedEffect` and
   `observe_watch` both do that on purpose.

See also: `docs/state-handles.md` for the write semantics these effects drive, and
`docs/movable-content.md` for the slot-storage rules `on_forget` follows.
