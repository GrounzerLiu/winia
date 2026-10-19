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
- A composer can carry one (`Composer::set_saveable_registry`, crate-internal), which is how the
  window lifecycle uses it: the registry is provided for every pass, not only the first. From outside
  the crate, `with_saveable_registry` is the public route — and it has to wrap EVERY pass, not just
  the first, or a call site that appears later is never registered.

Without a registry in scope, `remember_saveable` is plain `remember` with an explicit key — the same
call site works in tests and previews, it just never leaves the slot.

## What restores what

```text
window closes  ->  registry.perform_save()  ->  HashMap<saveable_key, HashMap<key, SavedValue>>
window reopens ->  registry = new(saved)    ->  remember_saveable(key, ..) restores by key
```

The map is keyed by `saveable_key`: the DECLARATION SITE of the window, not its `WindowId` and not
its `created_id` (a fresh one is minted for every window, so it cannot carry state across a reopen).
`Window::build` passes the salted key of its own call site; `open_window_with_saveable_key` lets a
programmatic opener supply its own.

Snapshots are taken in BOTH close paths: `CloseRequested` (the user closed the window) and the
declarative close (`process_detached`, when the declaring tree stops declaring the window — the
usual "close" for `if show { Window::new(..) }`). App exit is not covered.


## Persistence (opt-in)

```rust
winia::runtime::saveable::persist::set_store(Some(
    winia::runtime::saveable::persist::PersistStore::open(data_dir.join("myapp")),
));
```

With a store configured, snapshots are written on both close paths and read when a window is
created, so state survives a process restart. Without one, everything stays in memory (the default,
and what tests want).

The on-disk form is JSON with a `version` field and one tagged entry per value (`{"i":42}`,
`{"t":"hi"}`, `{"b":true}`, `{"f":"<hex bits>"}`). `f64` is stored as its bits, not a JSON number:
JSON has no NaN or infinity, and rounding a saved value would be a silent corruption.

What makes it safe to keep a cache-like file around:

- **Never fatal.** Every failure — missing directory, permissions, disk full, truncated file,
  corrupt JSON, an unknown version, an oversized file — is logged and treated as "nothing saved".
- **Atomic replace.** The body goes to `<name>.json.tmp` and is renamed into place, so a crash
  mid-write leaves the previous snapshot intact instead of a half file.
- **Bounded.** A snapshot larger than 4 MiB is refused unread; a saved state is kilobytes.
- **Tolerant reads.** A single malformed entry is skipped and the rest of the snapshot survives;
  an unknown future `version` is ignored rather than guessed at.

## Limits, stated up front
- **A closed set of types.** `SavedValue` is a closed enum on purpose (Compose restricts it to Bundle
  types for the same reason): a value that must be reconstructable is data, not a handle. A custom
  type implements `Saveable` and flattens itself.
- **Keys are explicit and ordered.** Two `remember_saveable` calls with the same key in one
  composition: restore goes to the first to compose (the value is consumed once) and save also goes to
  the first registration, so both legs pick the same owner. A duplicate key asserts in debug builds.
- **Removal drops the provider.** A composition point that goes away stops feeding the registry, so a
  dead composition cannot overwrite the next window's state.

## Tests

`cargo test -p winia --lib runtime::saveable` — round trip across two compositions, one-shot restore,
degradation without a registry, provider removal, and the composer-carried registry on every pass.
