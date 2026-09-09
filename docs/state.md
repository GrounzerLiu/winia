# State — Definition and Usage

> Source of truth: `winia/src/core/state.rs`. Rationale and migration history:
> `docs/state-handles.md`. This document describes what exists; the other one
> explains why.

## 1. What a State is

A `State<T>` is an **ownerless shared value cell**: internally an
`Arc<StateInner<T>>` holding an `RwLock<T>` plus a `StateSignal` used for
dependency routing (`winia/src/core/state.rs:307`, `RawState`).

Consequences:

- `Clone` is cheap and shares identity: `State::clone` clones the `Arc`, so
  all copies observe the same value and the same signal. There is no owner;
  a state is "alive" as long as anyone holds a handle.
- Identity is the **signal id** (`StateId`, `state.rs:21`), not the value.
  `PartialEq for State` compares `signal.id()` (`state.rs:876`): two distinct
  states holding equal values are *not* equal. The legacy `u32 id()`
  (`state.rs:833`) is public-compatibility only; internal routing uses
  `state_id()`.
- `Debug` prints `{ id, value }`; `Display` forwards to the inner value
  (`T: Display`).

## 2. Handles: one type per write semantic

Write semantics live in **handle types**, not function names. All four are
`#[repr(transparent)]` wrappers over the same `RawState<T>` (zero cost),
each exposing exactly one `set` with fixed semantics:

| Handle | `set` dedup | Notify | Wake loop | Reads | Use for |
|---|---|---|---|---|---|
| `Reactive<T>` (`state.rs:448`) | yes (`PartialEq`) | yes | yes | `get` + `peek` | default composition state |
| `Animating<T>` (`state.rs:453`) | yes (`PartialEq`) | yes | **no** | `get` + `peek` | animation ticks (redraw already requested) |
| `Visual<T>` (`state.rs:458`) | no | no | no | `peek` only | draw-layer props (alpha/scale/color) |
| `Backchannel<T>` (`state.rs:463`) | no | no | no | `peek` + `get` | measure/layout write-back, cross-frame staging |

`State<T>` (`state.rs:678`) is the ergonomic default with `Reactive`
semantics. Bounds are honest: `set` requires `T: PartialEq` everywhere it
dedups; `Visual::set` / `Backchannel::set` take plain `T: 'static` because
they never compare.

Conversions are **downgrade-only** (`Reactive -> Animating/Visual/Backchannel`):

```rust
// by value (consumes the Reactive)
Reactive::into_animating / into_visual / into_backchannel
// by reference (keeps the Reactive, shares the new handle)
Reactive::as_animating / as_visual / as_backchannel
// back-compat bridge still used by animate_value_as_state callers
Animating::into_state  // pub(crate)
```

Upgrading back (`Visual -> Reactive`) is framework-internal: only code
holding the `RawState` can do it (`from_raw` is `pub(crate)`).

## 3. Reads: `get` subscribes, `peek` never does

```rust
impl<T: Clone + 'static> Reactive<T> {
    pub fn get(&self) -> T;   // snapshot + register dependency
    pub fn peek(&self) -> T;  // snapshot, no registration
}
```

- `get()` registers the current dependency frame (`register_dependency`,
  `state.rs:1069`). Which graph it joins depends on **where** it runs:
  inside composition it records a compose dependency (change → recompose);
  inside measurement it records a layout dependency (change → re-measure
  only). The same call does the right thing in each phase — but it must run
  in *some* frame. Calling `get()` in the draw/render phase (outside any
  frame) registers nothing; the value is read but never updates.
- `peek()` (`peek_untracked`, `state.rs:369`) is for exactly those places:
  render-phase closures, animation-tick math, and any read whose change must
  not schedule work. Draw-phase reads that need freshness without
  subscription use `peek` on a `Visual` handle fed by the animation engine.
- `Backchannel` keeps `get` with `Reactive`-identical tracking because its
  values (fling limits, content heights) are read in **both** phases:
  measure phase wants re-measure on change, compose phase (e.g. scrollbar
  thumb math) wants recompose. The handle boundary governs writes only.

## 4. Writes

```rust
// Reactive — dedup + notify + wake (default)
pub fn set(&self, value: T);                    // T: PartialEq
pub fn update(&self, f: impl FnOnce(&mut T));   // T: Clone + PartialEq, deduped

// Animating — dedup + notify, skip WAKE_FN
pub fn set(&self, value: T);                    // T: PartialEq

// Visual / Backchannel — land silently, no dedup, no notify, no wake
pub fn set(&self, value: T);                    // T: 'static
```

- `set` skips notification when the new value equals the current one
  (`PartialEq`); equal writes are free.
- `update` mutates a staged clone, compares, and swaps only on change — no
  lock is held while user code runs. The legacy always-notify variant is
  kept as `pub(crate) update_untracked` for wrapping-add pulse counters.
- `Visual::set` / `Backchannel::set` overwrite unconditionally and notify
  nobody: the next frame reads the value. Writing them where recomposition
  was intended silently drops the update — this is why the type boundary
  exists (misuse must fail to compile, not fail at runtime).
- Deprecated shims on `State` (`set_silent`, `set_no_wake`, `set_visual`)
  forward to the matching handle primitive and emit compile warnings.
  New code must take the handle instead. No production callers remain; the
  shims are removed in step 3 of `docs/state-handles.md`.

## 5. Framework sources: where handles come from

Callers never pick a write semantic; they receive a handle from the
framework source that owns the scheduling decision:

| Source | Issues | Notes |
|---|---|---|
| `ctx.remember(\|\| v)` (`composer.rs:227`) | `State<T>` (= `Reactive`) | slot-stable across recompositions |
| `ctx.remember_at_key(key, \|\| v)` (`composer.rs:300`) | `State<T>` | stable under an explicit key, immune to statement-order drift |
| `ctx.remember_backchannel(\|\| v)` (`composer.rs:235`) | `Backchannel<T>` | write-back slots, cross-frame staging |
| `ctx.remember_animating(\|\| v)` (`composer.rs:247`) | `Animating<T>` | animation-tick state (`T: PartialEq`) |
| `ctx.remember_visual(\|\| v)` (`composer.rs:259`) | `Visual<T>` | draw-layer state |
| `ctx.animate_*_as_state(...)` / `push_animatable` / `Transition::animate_*` | `Animating<T>` | per-frame ticks enqueue without waking |
| `rememberInfiniteTransition().animate_*` | `Visual<T>` | infinite loops never recompose |

`remember` runs `init` once (first composition) and returns the same handle
on every recomposition. `State::new(value)` outside composition creates a
detached cell — useful for tests, hoisted owners, and `Arc`-shared models.

## 6. `DerivedValue`: computed snapshots

```rust
pub struct DerivedValue<T>(pub(crate) Arc<dyn Fn() -> T + Send + Sync>);
DerivedValue::new(f)  // cf. Compose derivedStateOf { }
DerivedValue::get(&self) -> T   // re-runs f; inner get() calls subscribe
pub type DerivedFloat = DerivedValue<f32>;
```

Arithmetic is implemented for `f32` only (`impl_derived_arith`: `Add/Sub/Mul/Div`
for `DerivedFloat` and `&State<f32>`, plus `f32 * &State<f32>`), so layout
code can write `.size(&alpha * 200.0 + 50.0, 30.0)` and have the whole
expression re-evaluate when `alpha` changes. Each operator builds a new
`DerivedValue` closing over the previous one — chains stay lazy. (Extending
the operators to `Dp / Offset / Size` is tracked in `docs/state-handles.md`
§7.4.)

## 7. Passing State around

- Handles are cheap `Arc` clones: pass `&State<T>` to callees that only
  read, move/clone owned handles into components and callbacks that must
  outlive the call.
- **Closures**: use `letclone::clone` — `{ clone!(x); move || ... }`.
  One `clone!` takes several variables (`clone!(a, b)`). `letclone` has no
  rename syntax: if the closure needs a different name, that is a sign the
  code wants restructuring, not a longer alias chain.
- **Value passing keeps `.clone()`**: `TextField::new(v.clone())`,
  `.state(s.clone())`, `.items_from(items.clone(), ...)` — wrapping a plain
  argument in `{ clone!(x); x }` buys nothing and is banned in review.
- **No single-use intermediates**: `let c = x.clone(); f(c)` collapses to
  `f(x.clone())`; `let c = x.clone(); move || c...` collapses to
  `{ clone!(x); move || x... }`.
- `State` is `Send + Sync + 'static`-friendly by construction (the inner
  value lives behind an `RwLock` in an `Arc`), so handles move freely into
  `'static` component slots and effect closures.

## 8. Pitfalls

1. **Silent write to the wrong handle.** `Backchannel`/`Visual` writes never
   notify. If a value that must recompose its readers is stored in one,
   the UI freezes with zero errors. When in doubt, `Reactive` is the
   default for a reason.
2. **`get()` outside a dependency frame.** Render-phase `get()` reads a
   fresh value once and never updates. Draw code uses `peek()` on purpose;
   compose/measure code uses `get()` on purpose. Mixing them up in either
   direction is the classic stale-UI bug.
3. **Equality is identity.** `state_a == state_b` means "same cell", never
   "same value". Compare `a.get() == b.get()` for values.
4. **`update` closures must be pure-ish.** `update` runs `f` on a staged
   clone with no lock held — side effects inside `f` run even when the
   result dedups to a no-op. Keep `f` to pure mutation of its argument.
5. **Write-back loops.** A `Backchannel` slot written during measure and
   `get()`-read during compose is the intended cross-phase channel; writing
   it from compose code that also reads it is a polling loop by another
   name. Keep write-back writes in measure/layout or engine callbacks.
