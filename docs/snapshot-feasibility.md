# Snapshot feasibility

Research note for the first item in [foundation-gaps.md](foundation-gaps.md): can winia have a
Compose-style snapshot layer, and is it worth having?

Measurements taken on branch `v2` at 1578 lib tests.

## What Compose's snapshot layer actually is

Three separable things, often discussed as one:

1. **A version.** One global counter, bumped by every write. Observation and `derivedStateOf` are
   built on "did the version advance past what I read".
2. **State records.** Each state keeps the value *per open snapshot*, not one live value, so a
   snapshot pins a consistent view even as writers move on. This is what makes reads inside a
   snapshot atomic.
3. **Mutable snapshots.** A write buffer you apply atomically (with conflict detection and rebase) or
   discard.

## What winia has today

| Piece | Where | Shape |
| --- | --- | --- |
| one live value per state | `runtime/state.rs:431` `set_reactive` | `value: RwLock<T>`, dedup by `PartialEq`, then `notify(true)` |
| write observation | `runtime/state.rs` `notify` | per-state revision + `Subscriber` list + `StateObserver` list |
| read tracking | `runtime/state.rs` `record_dep` | thread-local dependency frame, active during a compose pass, reconciled at commit |
| pass loop | `app.rs` `recompose_until_stable` | composes again while anything is pending, then one layout |
| approximations | `flow.rs` | `remember_derived_state` (recompute + dedup), `snapshot_flow` (subscribe + reconcile) |

There is no global version and no records. Writes take effect immediately and notify immediately;
the composer drains its pending set at the start of the next pass.

Blast radius if the value cell itself changes shape: **679** `State::new` sites, **726** `.set(`
calls, **883** `.get()` calls, **312** `.peek()` calls.

## What a snapshot layer would buy, itemised

| Property | Today | Worth having? |
| --- | --- | --- |
| Atomic multi-state write: a reader never sees half of a pair | two writes notify separately | yes — the common "update several derived fields at once" case |
| Consistent read inside one pass (no torn pass) | a write from another thread mid-pass is visible to the *next* read in that same pass | yes, but see the thread model below |
| Write-then-discard a transaction | none | no — nothing in a UI framework rolls back a composition write |
| Cheap, correct observation for `derivedStateOf` / `snapshotFlow` | hand-rolled: resubscribe + dedup | yes, and this is where a version pays off |

**Thread model matters.** winia composes on one thread; the writers that can interleave are tokio
tasks (`produce_state`, effects, background work). So "torn pass" is real but rare, and it is the
only property that actually requires *records*.

## Options

### A. Full record-based snapshots (Compose parity)

Change `StateInner.value: RwLock<T>` into a record list, add a global open-snapshot registry, apply
observers, conflict detection, rebase and GC of dead records.

- Every read becomes a record lookup instead of a lock-and-clone.
- Values are retained while any snapshot is open — a new lifetime question for every `T`.
- Touches all 679 creation sites conceptually even if the public API keeps its shape.

Estimate: 2-3k lines, weeks of work, high regression risk in the most exercised path in the crate.
**Verdict: not worth it** for a framework that composes on one thread and has no transaction-cancel
use case. It is also the option that would *add* code, not delete it — the opposite of what
`foundation-gaps.md` originally claimed.

### B. Write batch + global version (recommended first step)

- `GLOBAL_VERSION: AtomicU64`, bumped once per applied write (or once per batch).
- `snapshot(|w| { w.set(a, 1); w.set(b, 2); })`: stage the writes, apply them eagerly (so the writer
  reads its own writes), and notify **once per touched state** after all of them are in; observers
  wake once.
- Storage is unchanged: still one live value per state.

Cost: roughly 300 lines in `state.rs` plus the public surface. No change to existing `.get()` /
`.set()` call sites: a plain `set` is a batch of one.

Unlocks: atomic groups, one wake per batch, and a version other machinery can consult.

### C. Pass-consistency gate (only if torn passes show up)

A process-wide `RwLock<()>`: a compose pass holds the read side, a write from a thread that is not
composing takes the write side. Writes from the composing thread skip it (thread-local flag), because
a component may write during its own pass.

Cost: ~60 lines, one lock per write. Risk: a background task that blocks on the write side while the
pass waits for it is a deadlock; it needs `try_write` plus a deferred queue rather than a blocking
lock. **Do this only after instrumenting for torn passes** — nobody has measured one yet.

### D. Version-only consistency inside a derive

On top of B: `remember_derived_state` samples the version before and after its computation and
recomputes if it moved (bounded retries). That is a seqlock, and it gives a *consistent* derived value
without records.

Cost: ~40 lines on top of B. This is where the version earns its keep.

## Recommendation

| Phase | Work | Cost | Why |
| --- | --- | --- | --- |
| 1 | B: write batch + global version | **done** | atomic groups; one wake per batch; the version everything else wants |
| 2 | D: derives check the version | hours | makes `remember_derived_state` correct under concurrent writers |
| 3 | C: pass gate, only if measured | 1 day | closes torn passes; needs the `try_write` design |
| — | A: records | weeks | do not do this |

Phase 1 and 2 are additive and testable without touching the composition machinery, which is why
they are worth doing first even though they are not "the snapshot system".

## How to validate each phase

- **B**: a `snapshot { a.set(..); b.set(..) }` batch, observed through `changes()`: the pair must be
  visible together, and the collector must be woken once. A test can count wakes with a counting
  waker.
- **D**: a background thread writing a two-state pair under the batch API while a derive computes;
  the derived value must never be the mixed pair. Run it in a loop with a seeded writer.
- **C**: instrument `get` with a pass id and a write version, and assert (in debug builds) that no
  read in a pass returns a value written after the pass began; promote to an error once the assertion
  has run for a while without firing.
