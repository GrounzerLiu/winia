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
| one live value per state | `runtime/state.rs` `set_reactive` | `value: RwLock<T>`, dedup by `PartialEq`, `begin_write()` then `publish(true)` |
| write observation | `runtime/state.rs` `notify` | per-state revision + `Subscriber` list + `StateObserver` list |
| read tracking | `runtime/state.rs` `record_dep` | thread-local dependency frame, active during a compose pass, reconciled at commit |
| pass loop | `app.rs` `recompose_until_stable` | composes again while anything is pending, then one layout |
| approximations | `runtime/flow.rs` | `remember_derived_state` (recompute + dedup), `snapshot_flow` (subscribe + reconcile) |

There are no per-snapshot records. Writes take effect immediately and notify immediately; the
composer drains its pending set at the start of the next pass. (A global version and `write_batch`
were added afterwards — phases 1-2 below.)

Blast radius if the value cell itself changes shape, counted with `grep -rn ... | wc -l` over
`winia/src`: **690** `State::new(` lines, **742** `.set(` calls, **894** `.get()` calls, **318**
`.peek()` calls. (The earlier 679/726/883/312 were counted by line for three of the four and by
occurrence for `State::new`, which is why they did not reproduce.)

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

- `GLOBAL_VERSION: AtomicU64`, bumped once per applied write, BEFORE the value is stored — a bump
  after the store leaves a window where the value is new and the version old, and a reader sampling
  the version across its computation then accepts a pair that never existed.
- `write_batch(|| { a.set(1); b.set(2); })`: every write inside the closure applies eagerly (so the
  writer reads its own writes, though NOT derived states that read them), and notification happens
  **once per touched state** after all of them are in. Each write still bumps the version.
- Thread-scoped: a write from another thread during the closure is published on its own. Making the
  batch global would mean blocking writers behind the composing thread.
- Storage is unchanged: still one live value per state.

Cost: roughly 300 lines in `state.rs` plus the public surface. No change to existing `.get()` /
`.set()` call sites: a plain `set` is a batch of one.

Unlocks: atomic groups, one notification per touched state (note: `wake_loop` still runs per state,
so "one wake per batch" was wrong), and a version other machinery can consult.

### C. Pass-consistency gate (only if torn passes show up)

A process-wide `RwLock<()>`: a compose pass holds the read side, a write from a thread that is not
composing takes the write side. Writes from the composing thread skip it (thread-local flag), because
a component may write during its own pass.

Cost: ~60 lines, one lock per write. Risk: a background task that blocks on the write side while the
pass waits for it is a deadlock; it needs `try_write` plus a deferred queue rather than a blocking
lock. **Do this only after instrumenting for torn passes** — nobody has measured one yet.

### D. Version-only consistency inside a derive

On top of B: `remember_derived_state` samples the version before and after its computation and
recomputes if it moved (bounded retries). That is a seqlock, and it gives a *best-effort consistent*
derived value without records. Its known holes, stated so nobody reads more into it:

- the bound (4 attempts) publishes the last value even if the world kept moving;
- the version is process-wide, so unrelated writes force retries (and during heavy animation the
  bound can be reached by writes that had nothing to do with the read set);
- it protects a READ; nothing makes a multi-state WRITE atomic across threads — `write_batch` is
  thread-scoped by design.

Cost: ~40 lines on top of B. This is where the version earns its keep.

## Recommendation

| Phase | Work | Cost | Why |
| --- | --- | --- | --- |
| 1 | B: write batch + global version | **done** | atomic groups; one wake per batch; the version everything else wants |
| 2 | D: derives check the version | **done** | makes `remember_derived_state` best-effort consistent under concurrent writers (see the holes below) |
| 3 | C: pass gate | **measured, not built** | the instrument found no torn read in the suite (see below), so the gate stays deferred |
| — | A: records | weeks | do not do this |

Phase 1 and 2 are additive and testable without touching the composition machinery, which is why
they are worth doing first even though they are not "the snapshot system".

## Phase 3: what the instrument measured

`state::begin_pass` tags a compose pass; every write records the global version and the pass that
made it; `State::get` inside a pass counts a read whose state was written after the pass began by
somebody else (writes the pass made itself are the normal "a component sets state while building"
pattern and are excluded). The counter and the check are debug-build only; `begin_pass` itself runs
in release (a thread-local set/restore). Two limits worth naming: the check covers reads made while
COMPOSING, not the measure pass that follows, and `set_visual`/`set_backchannel` never record a write
at all (they notify nobody).

A full suite run — 1585 tests, every widget, the date picker, the lazy list, effects, flows,
subcompositions — produced **one** torn read, and that one is the instrument's own test
(`a_foreign_mid_pass_write_is_counted_as_a_torn_read`), which builds the situation on purpose.

Note what this is and is not evidence for: the suite runs single-threaded (`RUST_TEST_THREADS=1`),
so the only concurrent writer in it is that one deliberate test. "No torn pass in 1587 tests" means
the instrument works and the framework does not produce one in its own tests — not that a real
application with tokio workers cannot.

So the gate (option C) is **not** justified yet: nothing in the framework or its tests actually
produces a torn pass. The instrument stays in the debug build, and the gate should be reconsidered
only when a real application trips it — with the `try_write` design, never a blocking lock.

## How to validate each phase

- **B**: a `write_batch(|| { a.set(..); b.set(..); })` batch, observed through `changes()`: the pair
  must be visible together, and the collector must be notified once. A test can count notifications
  with a counting observer.
- **D**: a background thread writing a two-state pair under the batch API while a derive computes;
  the derived value must never be the mixed pair. Run it in a loop with a seeded writer.
- **C**: instrument `get` with a pass id and a write version, and assert (in debug builds) that no
  read in a pass returns a value written after the pass began; promote to an error once the assertion
  has run for a while without firing.
