# State Handles Refactor — `exp/state-handles`

> Status: steps 1–2 landed (`7f27486` framework, `cccf3a4` demos); step 3
> (delete legacy) still open. Branch `exp/state-handles` cut from `v2@d2d1545`.
> Goal: full refactor (no legacy leftovers) — move the four write semantics
> from **function names** into **handle types** so misuse fails to compile.

## 1. Problem

`State<T>` (`winia/src/core/state.rs:299`) exposes five write entry points on a
single type:

| Function | Dedup | Notify | Wake loop | Production call sites |
|---|---|---|---|---|
| `set` | yes (`PartialEq`) | yes | yes | ~80 (default write) |
| `update` | **no** (always notifies) | yes | yes | ~40 (in-place mutation) |
| `set_silent` | no | no | no | ~45 (measure/layout write-back, frame baselines, state-machine staging) |
| `set_no_wake` | yes | yes | **no** | ~10 (`Animatable::update` per-frame ticks) + ~15 tests |
| `set_visual` | no (`PartialEq` bound but body never compares) | no | no | 2 (`Infinite::update`) |

All five compile on every `State<T>`. Choosing the wrong one is silent:

- Real incident (deferred, `exp/search-bar` follow-up): `DockedSearchBar` pill
  flips `read_only = !active`, but neither `DockedSearchBar::build` nor
  `Surface::build` declares it via `ctx.changed()`. The anchor group's
  Skip gate sees equal params + equal modifier and skips; the new
  `read_only=false` never executes; the stale `kb_handler` (`read_only=true`)
  swallows keys (`text_field.rs:1237`); `state.query` never changes;
  `filtered()` never re-runs — "typing does not filter" with zero framework
  errors. The `State` notification chain itself was innocent; the component
  Skip contract (`changed` allowlist) dropped the change silently.
- Mirror hazard: calling `set_visual` where recomposition was intended drops
  the update with no error.

`set_visual`'s signature also lies: `impl<T: PartialEq>` but the body never
compares. `update` vs `set` dedup is inconsistent for the same reason.
`notify_version / take_notify_version` (`state.rs:310/343`) has zero production
callers — dead weight.

Reference: Compose keeps the same four scheduling semantics internally
(snapshot write + schedule flags) but never exposes them as peer functions on
one `MutableState`. Winia exposed them — the exposure is the bug, not the
semantics (audit confirms every variant has dozens of legitimate uses; none
can be deleted — see §4).

## 2. Design: semantics move from function names into handle types

Each scheduling semantic gets its own `#[repr(transparent)]` handle over the
same `Arc<StateInner<T>>`. Each handle exposes exactly one `set` with fixed
semantics. Callers stop choosing functions and start receiving types from the
framework.

```rust
// state.rs — internal representation is unchanged
pub(crate) struct RawState<T> { inner: Arc<StateInner<T>> }  // today's State<T>

#[repr(transparent)] pub struct Reactive<T>(RawState<T>);    // recompose + wake (today's set/update)
#[repr(transparent)] pub struct Animating<T>(RawState<T>);   // recompose, no wake (today's set_no_wake)
#[repr(transparent)] pub struct Visual<T>(RawState<T>);      // write only, no recompose (today's set_visual)
#[repr(transparent)] pub struct Backchannel<T>(RawState<T>); // write only, no notify (today's set_silent)
```

Reads:

```rust
impl<T: Clone + 'static> Reactive<T>    { pub fn get(&self) -> T;  pub fn peek(&self) -> T; }
impl<T: Clone + 'static> Animating<T>   { pub fn get(&self) -> T;  pub fn peek(&self) -> T; }
impl<T: Clone + 'static> Visual<T>      { pub fn peek(&self) -> T; }  // NO get — draw-phase reads never subscribe
impl<T: Clone + 'static> Backchannel<T> { pub fn peek(&self) -> T; pub fn get(&self) -> T; }
// Backchannel keeps `get` (same tracking as Reactive): write-back values
// (fling_limit, content_height) are read in BOTH phases — measure phase
// (layout dep, no recompose) and compose phase (scrollbar thumb math,
// needs recompose on change). The handle boundary governs writes only.
```

Writes (one `set` per handle, semantics fixed by the type):

```rust
impl<T: PartialEq + 'static> Reactive<T>  { pub fn set(&self, v: T); pub fn update(&self, f: impl FnOnce(&mut T)); }
impl<T: PartialEq + 'static> Animating<T> { pub fn set(&self, v: T); }  // enqueue, skip WAKE_FN
impl<T: 'static>             Visual<T>    { pub fn set(&self, v: T); }  // honest bound (today's set_visual lies)
impl<T: 'static>             Backchannel<T> { pub fn set(&self, v: T); } // honest bound
```

Conversions allow **downgrade only** (`Reactive -> Animating/Visual/Backchannel`);
upgrade (`Visual -> Reactive`) is framework-internal. `update` gains dedup
(`T: Clone + PartialEq`, compare before notify) to match `set`.

`Modifier` dynamic channels stop accepting arbitrary closures:

```rust
// modifier.rs — SizeValue::Dynamic / AxisValue / BackgroundColor closures
// today accept any `Fn() -> f32` (a captured `get()` mis-registers the dep).
// New rule: only `&Visual<T>`-derived peek closures construct dynamic channels,
// so draw-phase reads cannot subscribe by construction.
pub fn animated_size(v: &Visual<f32>) -> SizeValue
```

## 3. Who issues which handle (framework-side rule — callers never choose)

| Source | Issues | Rationale |
|---|---|---|
| `ctx.remember(\|\| v)` / `remember_at_key` | `Reactive<T>` | composition state is reactive by default |
| `ctx.animate_*_as_state` / `push_animatable` / `Transition::animate_*` | `Animating<T>` | animation ticks enqueue without waking (already `request_redraw`-driven) |
| `rememberInfiniteTransition().animate_*` | `Visual<T>` | infinite loops are draw-only, zero recomposition |
| Measure/layout write-back slots (`fling_limit`, `content_height`, `track_width`, `last_scroll`, …) | `Backchannel<T>` | write-back lands next frame, never triggers |
| `Modifier::graphics_layer` / draw closures | `Visual`-only captures | enforced by constructor signatures, not comments |

## 4. Full call-site audit (`winia/src`, 151 write sites)

Counts from `grep -E "\.set_silent\(|\.set_no_wake\(|\.set_visual\(|\.update\("`
(non-test production code; `composer.rs` test-only `set_no_wake` ×15 excluded
from production but listed for migration):

- `set_silent` ×45 — `composer.rs` ×7 (slot restore sync), `nav.rs` ×10
  (progress/spec staging, seq counters), `lazy_column.rs` ×5 (content height,
  fling limits, caches), `app.rs` ×4 (window size ×2, overlay progress ×2),
  `tab_row.rs` ×4 (indicator offsets), `progress_indicator.rs` ×4 (test pose
  staging), `scrollbar.rs` ×3 (frame baselines, grab offset), `slider.rs` ×3
  (track width write-back), `navigation_suite.rs` ×3 (type pre-staging),
  `animated_visibility.rs` ×3 (anchors, content size), `node.rs` ×2
  (fling limits), `animated_size.rs` ×0 prod (its 2 are `set_no_wake`),
  `animated_content.rs` ×2 (size snapshots), `switch.rs` ×2 (offsets),
  `window.rs` ×1 (`created_id` — comment documents the key-avalanche hazard),
  `state.rs` ×1 (own test). **All become `Backchannel::set`.**
- `set_no_wake` ×10 prod — `animation.rs` ×3 (`Animatable::update` final +
  per-frame), `animated_size.rs` ×2 (size/target goals), `composer.rs` tests
  ×15 (animation-step simulation). **All become `Animating::set`** (tests may
  use `pub(crate) RawState` directly).
- `set_visual` ×2 — `animation.rs:121/129` (`Infinite::update` restart/reverse).
  **Both become `Visual::set`.**
- `update` ×40 — `nav.rs` (stack push/pop), `search_bar.rs:50` (query text),
  `text_field.rs` (blink toggle, selection, registrar sync), `checkbox.rs` /
  `switch.rs` / `radio_button.rs` (checked flips), `top_app_bar.rs:101`
  (content offset), `app.rs:719/724` (`scroll_pulse` wrapping add),
  `wavy_progress_indicator.rs` (cache/shape updates). **Stay
  `Reactive::update`** (with new dedup).
- `set` ×~80 — default path, unchanged semantics under the `Reactive` name.

`State<T>` name: phase 1 keeps `pub struct State<T>` as a deprecated alias of
`Reactive<T>`; final phase renames internals to `RawState<T> (pub(crate))` and
removes the alias. `notify_version / take_notify_version` deleted (zero
production callers). `PartialEq for State` narrowed to signal-id identity
(today: same-id OR equal-value — `state.rs:484` — misfires `changed` when two
distinct states hold equal values).

`DerivedValue`: extend `impl_derived_arith` beyond `f32` to `Dp / Offset /
Size` (all have `AnimatableValue`; `unit.rs` arithmetic exists — wrap it), and
re-key operators from `&State<f32>` to `&Reactive<f32>`.

## 5. Execution plan (each step independently committable + revertible)

1. **Types + shims** — add `Reactive/Animating/Visual/Backchannel`
   (`repr(transparent)` over shared inner) + `From` downgrades; mark
   `set_silent / set_no_wake / set_visual` `#[deprecated]`; `cargo test`
   green with zero behavior change.
2. **Framework issues handles** — `remember* → Reactive`, `animate_* →
   Animating`, `InfiniteTransition → Visual`, write-back slots → `Backchannel`;
   migrate the 45+10+2 sites (`set_silent/set_no_wake/set_visual` → `set`);
   fix `update` dedup + `set_visual` bound honesty + `DerivedValue` operators.
3. **Delete legacy** — remove `set_silent / set_no_wake / set_visual`,
   `notify_version / take_notify_version`, the `State` alias; rename to
   `RawState (pub(crate))`; narrow `PartialEq`; close the `changed`-allowlist
   gap for `TextField::read_only`-class fields (the docked-filter root cause)
   as a separate tracked item.

## 6. Non-goals

- No change to `StateSignal / ComposerSubscription / DependencyFrame` mechanics
  (handshake, fan-out, two-phase deps all stay — audited sound).
- No change to `changed()`/Skip semantics in this branch (tracked separately).
- No public API stabilization promise yet — handles are the new public API
  surface; keep them `#[doc(hidden)]`-free but version-unpinned until v2 ships.

## 7. Implementation progress (landed)

### 7.1 Framework handles (`7f27486`)

- `Reactive / Animating / Visual / Backchannel` added as `#[repr(transparent)]`
  wrappers over the shared `RawState` storage (`winia/src/core/state.rs`),
  with downgrade-only conversions (`Reactive -> Animating/Visual/Backchannel`).
  `State<T>` stays as the ergonomic `Reactive` default.
- `ComposeCtx` issues handles: `remember_backchannel` (`composer.rs:235`),
  `remember_animating` (`composer.rs:247`), `remember_visual`
  (`composer.rs:259`); animation engine works on handles
  (`push_animatable_handle`, `push_infinite_visual`,
  `cancel_animation_by_id`); overlay progress and nav write-back slots moved
  off the legacy functions.
- `update` gained dedup (clone-compare-swap, matching `set`); legacy
  always-notify kept as `pub(crate) update_untracked` for pulse counters.
- 21 `set_no_wake` sites in `composer.rs` tests migrated to
  `as_raw().set_animating` (same-crate `pub(crate)` access, identical
  dedup + notify + no-wake semantics); `state.rs` self-test rewritten to
  `test_backchannel_write_lands_without_notify` using `Backchannel::set`.
- Zero legacy callers remain in lib outside `state.rs` itself (the
  `Visual::set` internal forward and the deprecated `State::set_visual` shim,
  both removed in step 3). Stale old-name comments cleaned across lib.
- Gate: `cargo test -p winia --lib` — 798 passed, 0 failed.

### 7.2 `letclone` convention (`7f27486` + `cccf3a4`)

- Workspace depends on `letclone 0.3.0` (root `Cargo.toml:24`,
  `winia/Cargo.toml:26`) for closure captures.
- Rule (enforced in review, not by the compiler): `clone!` is **only** for
  closures — `{ clone!(x); move || ... }`. Direct value passing keeps
  `.clone()` (`TextField::new(v.clone())`, `.state(s.clone())`), and
  single-use intermediate clone variables are deleted instead of renamed
  (`letclone` has no rename syntax).
- Rationale: `{ clone!(x); x }` for value passing adds a macro for zero
  benefit; `clone!` earns its place exactly where the capture list would
  otherwise need a separate `let` per variable.

### 7.3 `run_app` owns the tokio runtime (`7f27486` + `cccf3a4`)

- `run_app` (`winia/src/app.rs:4810`) creates the tokio runtime itself:
  reuse `Handle::try_current()` when the caller already entered one, else
  `Runtime::new()`, holding the enter guard across the blocking event loop.
  Effects (`LaunchedEffect`, `remember_coroutine_scope`) and the debug WS
  server rely on `Handle::try_current()` during composition.
- `#[tokio::main]` was rejected: it `block_on`s, which conflicts with the
  blocking `EventLoop::run_app`. Nested `Runtime::new()` under an entered
  guard is a silent shadow (spawns land on a never-driven runtime), so demos
  must not create their own — the reuse branch exists only to keep the
  migration midpoint panic-free.
- All 62 files under `winia/examples/` migrated (`cccf3a4`): `clone!` for
  closure captures per §7.2, `Runtime::new + enter` boilerplate deleted from
  every `main`, dead code removed (`badge_demo` unused `badged_icon`),
  unused imports dropped. Each demo verified with `cargo check` (zero new
  warnings) and `cargo run` (exit 0). Zero `Runtime::new` and zero
  value-style `{ clone!(x); x }` remain under `winia/examples/`.

### 7.4 The four `remember` shapes (`bfc1c17` + `f029a33`)

`ctx.remember` hands back a `State<T>` — it wraps the init closure itself
(`composer.rs:306`, delegating to `Slot::remember` at `composer.rs:938`). So
`ctx.remember(|| State::new(x)).get()` builds a `State<State<T>>`: an outer slot
whose value is an inner handle, unwrapped once.

It works, and that is exactly why it survived review: the outer handle is consumed
by `.get()` on the spot, so it is structurally unreachable afterwards and can
never be written — a notification it could not make is one the inner handle
already makes. The extra layer is pure overhead: one more `StateInner` plus a
`StateSignal` (a `Mutex<Vec<Subscriber>>` and an `AtomicU64` revision) per
instance, and one more dependency edge in the enclosing scope that can never
fire. In a container that instantiates per row, that is a real cost.

Pick the spelling that matches the semantics you want:

| You want | Write | Not this |
|---|---|---|
| `State<T>` — recompose + wake (the default) | `ctx.remember(\|\| x)` | `ctx.remember(\|\| State::new(x)).get()` |
| `Backchannel<T>` — write only, no notify (measure / layout write-back) | `ctx.remember_backchannel(\|\| x)` | `ctx.remember(\|\| Backchannel::new(x)).get()` |
| `Animating<T>` — recompose, no wake (animation ticks) | `ctx.remember_animating(\|\| x)` | `ctx.remember(\|\| Animating::new(x)).get()` |
| `Visual<T>` — draw-layer write, no recompose | `ctx.remember_visual(\|\| x)` | `ctx.remember(\|\| Visual::new(x)).get()` |

`bfc1c17` converted nine sites of the first shape (`effect.rs`, `nav.rs`,
`ui/date_picker.rs`) and its message claimed nothing else was affected. A review
of that commit found eight more it had missed; `f029a33` converted them — five in
`LazyList::build` (`ui/lazy_column.rs`: the item height cache, the viewport
height, the data signature, and the two write-back slots) and three under
`winia/examples/`. Two of the five turned out to be row 2 rather than row 1:
`content_height` and `fling_limit` became `remember_backchannel(|| …)`, because a
measure-write-back slot wanted the Backchannel scheduling all along and the outer
`State` was never what it was asking for.

**A fifth shape, and why it stays.** A handle that is none of the four —
`LazyListState`, `ScrollState`, `MutableInteractionSource`, `DatePickerState` —
has no direct spelling today: `remember_handle` exists on `SlotTable`
(`composer.rs:953`) and `Composer` (`composer.rs:1487`) but is not exposed on
`ComposeCtx`. Those sites keep `ctx.remember(|| Handle::new()).get()`, and that
is correct rather than pending: `ctx.remember(|| Handle::new())` returns
`State<Handle>`, not `Handle`. Do not tidy them. Whether `ComposeCtx` should
expose a public `remember_handle` is an open decision — see §7.5.

### 7.5 Remaining (step 3 + deferred)

Landed in step 3's first pass:

- `State::set_silent / set_no_wake / set_visual` deleted. `set_no_wake` and
  `set_visual` had no callers at all; `set_silent` had four, all on
  `AnchoredDraggableState::dragging`, which is now a `Backchannel<bool>` — the
  flag is read by `is_dragging` for logic and by no composable, which is what
  the write-only semantics were for. `State` has one setter left, and it means
  one thing.
- `notify_version` / `take_notify_version` deleted. Nothing read the counter:
  `take_notify_version` had four definitions and no callers, while every notify
  did a `fetch_add` on it. That was an atomic RMW on a shared cache line per
  state write, buying nothing. The revision counter that IS load-bearing —
  `StateSignal::revision`, read by the subscribe handshake — is a different
  field and is untouched (pinned by the two `revision_handshake_*` tests in
  `state.rs`).
- `PartialEq` was already narrowed to signal-id identity, so that item was
  carried here by mistake.

Still open:

- `Modifier` dynamic channels restricted to `Visual`-derived peek closures
  (constructor signatures, not comments). This changes a public signature, so
  it is its own piece of work.
- `DerivedValue`: extend `impl_derived_arith` to `Dp / Offset / Size` and
  re-key operators from `&State<f32>` to `&Reactive<f32>`.
- Decide whether `ComposeCtx` exposes a public `remember_handle`, which would let
  the handle-shaped sites in §7.4 drop their `.get()`. Not urgent — the layer is
  inert and costs one `StateInner` per instance — but it is the only reason those
  spellings are unavoidable.
- Reconsider what step 3 called "remove the `State` alias". There is no alias:
  `State<T>` (`state.rs:687`) and `Reactive<T>` (`state.rs:457`) are two
  newtypes over the same `RawState<T>`, and `ctx.remember` hands out `State<T>`.
  Collapsing them is a public-surface migration for the whole crate, not a
  cleanup — it needs its own branch and its own plan.
- Deferred (separate item): the `changed`-allowlist gap for
  `TextField::read_only`-class fields (docked-filter root cause from §1).
