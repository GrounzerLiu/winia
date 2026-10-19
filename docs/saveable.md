# Saveable state

`remember` lives in a slot, and a slot dies with its composition: close a window and the slot table
goes with it. `remember_saveable` registers the value with a registry that lives OUTSIDE the
composition, so it can be snapshotted before the window goes away and handed to the next one.

Status: implemented and wired into the window lifecycle (in-memory, same process).

## API

```rust
use winia::prelude::*;

// Inside a component: a counter that survives its window.
let count = remember_saveable(ctx, "editor.count", || 0i32);
count.update(|v| *v += 1);
```

- `remember_saveable(ctx, key, init) -> State<T>` — `T` must implement [`Saveable`]: `bool`, `i32`,
  `i64`, `f64`, `String` today. The key is explicit and must be stable per composition point.
- `SaveableStateRegistry::new(restored)` — where values come from and go to; `perform_save()` returns
  the current snapshot; `consume_restored(key)` takes a value once.
- `with_saveable_registry(registry, || { ... })` — provide one for a composition you manage yourself.
- A composer can carry one (`Composer::set_saveable_registry`), which is how the window lifecycle uses
  it: the registry is provided for every pass, not only the first.

Without a registry in scope, `remember_saveable` is plain `remember` with an explicit key — the same
call site works in tests and previews, it just never leaves the slot.

## What restores what

```text
window closes  ->  registry.perform_save()  ->  HashMap<created_id, HashMap<key, SavedValue>>
window reopens ->  registry = new(saved)    ->  remember_saveable(key, ..) restores by key
```

`AppState` keeps that map keyed by the window's `created_id` (the declarer's id, not the volatile
`WindowId`), so a window recreated with the same id picks its state back up. The snapshot is taken on
`CloseRequested`, while the composition that provides the registry is still alive.

## Limits, stated up front

- **Same process only.** Nothing is serialized to disk. Surviving a process restart needs a
  persisted form (`SavedValue` -> bytes), which is a separate decision about where it lives.
- **A closed set of types.** `SavedValue` is a closed enum on purpose (Compose restricts it to Bundle
  types for the same reason): a value that must be reconstructable is data, not a handle. A custom
  type implements `Saveable` and flattens itself.
- **Keys are explicit and ordered.** Two `remember_saveable` calls with the same key in one
  composition: the first to be composed restores, the second looks like a fresh value — a duplicated
  key is visible rather than silently shared.
- **Removal drops the provider.** A composition point that goes away stops feeding the registry, so a
  dead composition cannot overwrite the next window's state.

## Tests

`cargo test -p winia --lib runtime::saveable` — round trip across two compositions, one-shot restore,
degradation without a registry, provider removal, and the composer-carried registry on every pass.
