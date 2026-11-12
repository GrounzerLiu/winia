# CompositionLocal — Definition and Usage

> Source of truth: `winia/src/runtime/composition_local.rs`. Rationale and migration history:
> [`docs/composition-local-redesign.md`](composition-local-redesign.md). This document describes
> what exists; the other one explains why it exists and what was removed to get here.
>
> How to *use* providers from a component is
> [`docs/component-authoring.md`](component-authoring.md) §13. This document is about the mechanism
> itself, for whoever changes the framework.

## 1. What a CompositionLocal is

A `CompositionLocal<T>` is an **implicitly propagated value**: a provider puts it in scope for a
subtree, and any composable inside reads it without a parameter. Compose's `LocalContentColor`,
`LocalTextStyle`, `LocalDensity`.

An instance is a `static` (or `LazyLock<static>`); its identity is a process-unique `u64` taken from
`NEXT_ID`. **That placement is load-bearing, not style**: `id` is what a read matches on, and two
instances built from one `CompositionLocal::new` call would share an id while holding different `T`s
— the read would then downcast to the wrong type and panic. Every construction site in the
framework is inside a `LazyLock`.

```rust
pub struct CompositionLocal<T: Clone + 'static> {
    id: u64,
    default: fn() -> T,
    same: fn(&T, &T) -> bool,                    // does this count as "changed"?
    same_erased: Arc<dyn Fn(&dyn Any, &dyn Any) -> bool + Send + Sync>,
    notifies: bool,                              // may a change WAKE readers?
}
```

The five fields split into three questions a local answers once, at construction: what to return
when nothing provides one (`default`), whether a value moved (`same`), and whether that move may
disturb anybody (`notifies`). `same_erased` is `same` with the downcast folded in, built once so a
multi-value provide can compare a value whose type it no longer names (§6).

## 2. The stack

```rust
thread_local! {
    static SLOTS: RefCell<Vec<(u64, Arc<Supplied>)>>;
}
```

One flat, thread-local stack of `(local id, entry)` pairs. A provider pushes before running its
content and pops after; a read scans from the back for the first matching id, so the innermost
provider wins and the same local may nest inside itself.

```rust
pub struct Supplied {
    value: RwLock<Arc<dyn Any>>,
    signal: Option<Arc<StateSignal>>,
}
```

An entry is **one provide site's** state — its current value plus the signal that announces changes.
The value sits behind a lock rather than being replaced wholesale for a specific reason: readers
subscribe to the *signal*, so swapping the whole `Supplied` on a later pass would leave every
existing subscriber pointing at a signal nobody ever notifies again. One entry per site, one signal,
value swapped in place.

`signal: Option` — `None` means "this provide cannot invalidate anybody". See `provides` in §5.

## 3. Reads: `current()` subscribes

```rust
pub fn current(&self) -> T;          // read + subscribe; falls back to `default`
pub fn try_current(&self) -> Option<T>;  // no default; `None` when nothing provides one
```

**The read is the subscription.** `current()` finds the entry, hands its signal to
`register_dependency` — the same call `State::get` makes — and returns the value. The scope doing
the read becomes a dependent of that signal, so a change re-enters exactly the scopes that read it.

This is the property the whole design rests on. Before it, `current()` was a pure pull that
registered nothing, so a reader only ever saw a new value if its own group happened to run again —
and a group declaring nothing about itself is Skipped, keeping what it composed with. Every provider
that cared had to compensate by remembering its last value and calling `ctx.mark_subtree_dirty`,
which re-enters a whole subtree because the readers were unknown. Four copies of that dance lived in
four components.

Two details worth knowing before changing this code:

- `find()` drops the `SLOTS` borrow **before** the caller subscribes. `register_dependency` reaches
  into the composer and can run code that reads another local, which would re-enter the `RefCell`.
- `subscribe()` discards `register_dependency`'s return value on purpose. `State::get` keeps it for
  the post-read `enqueue_if_changed` handshake, which closes the window between reading the value and
  subscribing. A local has no such window: the value comes from the same entry the signal belongs
  to, and a change is a whole composition pass rather than a concurrent write from another thread.

## 4. Three comparison policies

| Constructor | `same` | `notifies` | Compose equivalent |
|---|---|---|---|
| `new(default)` | `a == b` (`T: PartialEq`) | `true` | `compositionLocalOf` |
| `new_by(default, same)` | the caller's predicate | `true` | `compositionLocalOf` with a custom policy |
| `new_static(default)` | `\|_,_\| false` — always replace | `false` | `staticCompositionLocalOf` |

`new_by` is for a value with no usable `PartialEq`: an `Arc<dyn Fn>`, a handle compared by pointer,
a registry compared by inner `Arc`. Compare by the identity that actually distinguishes two states —
answering "same" too readily leaves readers on a stale value with no symptom, answering "different"
too readily re-enters them every pass.

**`new_static` answering `false` looks wrong and is not.** It means "always replace the value", and
`notifies: false` is the separate promise that keeps the signal quiet. Conflating the two — making
`same` answer `true` so nothing notifies — freezes the value at its first pass, and every reader that
composes *later* for its own reasons then reads something older than what is in scope. Compose's
`staticCompositionLocalOf` does not freeze values; it only declines to wake anybody.

## 5. The verbs

Four ways in, and the difference between them is the whole API:

| Form | Takes `ctx`? | Installs? | Invalidates readers? |
|---|---|---|---|
| `local.provide(value, ctx, \|ctx\| …)` | yes | one local | **yes** |
| `ctx.provide(&[A.provided(x), B.provided(y)], \|ctx\| …)` | yes | several locals, one slot position | **yes**, per entry |
| `local.provides(value, \|…\|)` | no | one local | **no** |
| `local.provided(value)` | — | nothing — it *builds* an entry | — |

`provided` is a past participle describing a value; `provide` is the verb that installs it. They are
two roles in Compose's one syntax `A provides x`, and they cannot share a name in Rust.

### `provide`

```rust
pub fn provide<R>(&self, value: T, ctx: &mut ComposeCtx,
                  content: impl FnOnce(&mut ComposeCtx) -> R) -> R
```

Takes a **per-site key** (§7), remembers the entry under it, compares the new value against the
previous pass under this local's own `same`, and — only on a real move — swaps it in and calls
`signal.notify(true)`. Then it pushes, runs the content, and pops.

This is the form every state-fed provider uses. No component writes invalidation code.

### `provides`

The ctx-less form, kept for the four providers that cannot or should not take a `ctx`:
`with_density` (wraps a whole FRAME — its readers run in measure/draw, where composition
invalidation cannot reach them), `direction::provides` (its readers are measure-time or it wraps a
frame), `with_saveable_registry` and `with_component_styles` (installed once, never rebuilt).
It pushes an entry with **no signal**, so reading it subscribes to nothing.

Adding a `ctx` to these would not improve them; §6 of the redesign document records why each is here.

## 6. Multi-value provide

```rust
pub struct ProvidedValue {
    id: u64,
    value: Arc<dyn Any>,
    same: Arc<dyn Fn(&dyn Any, &dyn Any) -> bool + Send + Sync>,
    notifies: bool,
}
```

One local's contribution to a `ctx.provide` call. The payload is type-erased because
`&[ProvidedValue]` is a **homogeneous** array while the locals in it are not — `LocalContentColor`
holds a `Color`, `LocalTextStyle` a `TextStyle`, in the same slice. The two halves that need `T`
travel with each value: the payload as `dyn Any`, the comparison as a predicate that downcasts inside
itself. `same` and `notifies` are **copies** of the local's fields, taken when `provided` bundled it.

A type mismatch in that comparison answers `false` ("changed") — a site now holding a different type
is something no reader of the old value can call unchanged.

`provide_all` matches entries by the **local's id**, not by array position. Two behaviours follow,
both deliberate:

- a call site whose list grows or shrinks between passes keeps working. Existing entries keep their
  signals, a local joining the list is seeded with the value it is being given, one leaving just
  stops being on the stack. Panicking on the arity would abort a shipping window for ordinary code —
  `if checked { &[A] } else { &[A, B] }` is not a mistake.
- reordering the array is not a change. Zipping by position would report every entry as moved (each
  slot would downcast against another local's payload, fail, and answer "changed"), re-entering every
  reader for a list that had not changed at all.

`ctx.provide` short-circuits a `values.is_empty()` **before** consuming a key, so a provider that
compiles to an empty list must not shift the keys of everything after it.

## 7. The per-site key

`provide` and `ctx.provide` address their entry through `Composer::next_provider_key`, which is
`next_remember_key`'s scheme (statement id mixed with a per-base ordinal) under its **own** counter
table.

Two things it buys, one it costs:

- **Its own counter table** is load-bearing. Providers are commonly written in conditional branches;
  sharing `remember`'s counters would make a branch flipping shift the ordinal of every `remember`
  after it, so those states would change key and lose their value.
- **The ordinal distinguishes providers inside one group.** Keying by `ComposeCtx::position_key` (the
  enclosing group) would make two providers in one statement share one entry, read each other's value
  as "changed", and re-dirty every pass. Pinning the ordinal to a constant does exactly that —
  measured: `nested_providers_in_one_group_do_not_overwrite_each_others_memory` fails immediately.
- **The cost**: when there is no compiler statement to key off — a bare scene in a test, or a
  window's per-frame wrapper built as a plain closure — the key falls back to hashing the slot path,
  which **drifts with the structure**. A drifting key loses the site's memory: the new entry seeds
  itself with the current value, no change is seen, and readers keep what they composed with. This
  is not a regression (`position_key`, which this replaced, had the same property), and for the
  window wrapper specifically the frame makes up for it by re-resolving the theme and calling
  `mark_content_dirty` when the epoch moves (`PerWindow::refresh_theme`).

## 8. Invalidation lands one pass late

`Composer::compose` drains the pending notification queue at its **start**, before any content runs.
A `notify` raised while content is running therefore cannot dirty anything during that pass: it lands
in the queue, and the **next** pass marks the subscribed scope dirty.

This is a consequence, not an oversight. Same-pass invalidation was exactly what
`mark_subtree_dirty` provided, and it could only provide it by not knowing who the readers were.

The frame path loops for its own reasons — `PerWindow::recompose_until_stable` — so a frame converges
and never *presents* the intermediate state. What a caller has to know is the unit-test consequence:
after changing a provided value, converge rather than calling `recompose` once and asserting.
`Composer::recompose_to_stable` (`cfg(test)`, bounded at 8 passes) is that loop.

## 9. Snapshots

```rust
pub type LocalSnapshot = Vec<(u64, Arc<Supplied>)>;
pub fn capture() -> LocalSnapshot;
pub fn with_snapshot<R>(snapshot: &LocalSnapshot, f: impl FnOnce() -> R) -> R;
```

`capture` clones the stack — `Arc::clone` per entry, so O(1) per entry with no value `Clone`.
`with_snapshot` replays it around a closure, which is how an independent composition unit (an overlay
with its own `Composer`) inherits the theme/direction/typography in force when it was opened.

The entries are shared, not copied, so **the replayed subtree subscribes to the same signals** — an
overlay reading the theme is the intended semantics, not an accident.

Restoration is by **stack depth**, not by id: `with_snapshot` truncates to the length it found, so
provides made inside `f` are dropped along with the replay and cannot be mis-popped. `TruncateGuard`
does the same on the panic path.

## 10. Pitfalls

1. **A local not in a `LazyLock`/`static`.** Two instances from one construction share neither an id
   nor a type, and everything downstream assumes one `id` means one `T`. The read's downcast is what
   notices, at runtime, with no context about which local is duplicated.
2. **Providing a value that changes, through `provides`.** The ctx-less form carries no signal, so
   nothing wakes the readers. It looks correct on the frame the value is first provided and goes
   stale afterwards. Use `provide`.
3. **Reading a local a reader never actually reads.** A scope has to call `current()` to subscribe.
   A group that never reads has nothing to wake, so a test that provides a value and never reads it
   proves nothing about invalidation.
4. **Expecting same-pass invalidation.** §8. Two passes, converging within the frame.
5. **Making `new_static`'s `same` answer `true`.** That freezes the value instead of merely declining
   to wake readers; readers composing later see a stale one. §4.
6. **A `ProvidedValue` built once and reused across passes.** It carries a snapshot of the value and
   of the policy, taken at `provided()` time. Build the array inside the composition, from the
   current values.

## 11. Tests

`composition_local.rs`'s own tests pin the mechanism, each failing if its invariant breaks:

| test | pins |
|---|---|
| `provide_does_not_touch_readers_when_the_value_is_unchanged` | an unchanged value must not re-run readers (a per-group key would) |
| `nested_providers_in_one_group_do_not_overwrite_each_others_memory` | the per-site ordinal (§7) |
| `provide_reenters_readers_when_the_value_moves` | the notify reaches the reader at all |
| `one_provide_call_installs_every_local_it_describes` | multi-value install, and that it pops cleanly |
| `a_static_local_in_one_provide_call_never_invalidates` | `notifies` is per value, not per call |
| `a_multi_value_provide_reenters_its_reader_when_one_value_moves` | the reader sees old-then-new, not just "ran again" |
| `a_provide_call_may_change_its_value_list_between_passes` | id matching survives a list that grows |
| `reordering_a_provide_call_is_not_a_change` | id matching is not positional |
| `a_signal_notify_reaches_the_subscription_that_read_it` | the signal→subscription edge, at signal level |
| `a_provided_value_keeps_its_type_and_its_signal` | the stack entry round-trips value and signal |

Plus the ctx-less and snapshot tests inherited from before the redesign.
