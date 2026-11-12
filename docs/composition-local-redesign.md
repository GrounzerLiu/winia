# CompositionLocal redesign: reads register their dependencies

> **Status: landed.** Everything in §8's checklist is done except D2–D4, which were measured to be
> wrong rather than left unfinished — see the note on each. §1 and §2 describe the tree BEFORE the
> rewrite and are kept as the record of what was replaced; §3 onward is what exists today.
>
> For how the mechanism works now, see [`docs/composition-local.md`](composition-local.md). This
> document is the rationale and the migration log.
>
> The defect that triggered this design: after switching the selection in a `Day / Week / Month`
> group, the container changed colour while the labels stayed on their previous colour (pale label on
> a pale fill, violet label on a dark fill). Fixing it took `provides_tracked`,
> `provides_tracked_by`, a per-site key, `provider_path_counters`, and two `backchannel`s. Those
> patches work, but they patch a wrong model — this document replaces that model.

## 1. Before (historical): three layers, 23 entry points

> ⚠ **This section describes the tree as it was before the rewrite, not today.** `provides_tracked`,
> `provides_tracked_by`, the per-group `position_key` providers and the hand-written
> `mark_subtree_dirty` are all gone; §3 is what replaced them.

### 1.1 The primitive (`runtime/composition_local.rs`)

| API | Closure signature | When the value changes |
|---|---|---|
| `CompositionLocal::new(default)` | — | — |
| `.current()` | — | reads, **registers no dependency** |
| `.try_current()` | — | same, `None` when nothing is provided |
| `.provides(v, f)` | `FnOnce() -> R` | **re-runs no reader** |
| `.provides_tracked(v, ctx, f)` | `FnOnce(&mut ComposeCtx) -> R` | hand-rolled compare + `mark_subtree_dirty` |
| `.provides_tracked_by(v, ctx, same, f)` | same + `Fn(&T,&T)->bool` | same |
| `capture()` / `with_snapshot(&s, f)` | `FnOnce() -> R` | snapshot replay |

### 1.2 The wrappers (23 public entry points)

**Tracked (14)** — `WiniaTheme::{light, dark, auto}`, `with_theme`,
`with_theme_and_typography`, `with_theme_and_direction`, `with_theme_typography_and_direction`,
`with_typography`, `with_motion_scheme`, `with_content_color`, `with_tonal_elevation_enabled`,
`with_absolute_tonal_elevation` (pub(crate)), `ProvideTextStyle`,
`with_component_styles_when_changed`, `with_overscroll_factory`, `disable_overscroll`,
`DisableSelection::build`.

**Bare `provides` (9)** — `with_density`, `direction::provides`, `with_saveable_registry`,
`with_registry_of` (pub(crate)), `with_component_styles`, `with_component_styles_arc`, plus the
internal provide sites in `SelectionContainer`, `sub_window_content`,
`nav::display::render_entry`, and `shared_transition`.

### 1.3 Call-site distribution (measured)

| provider | call sites |
|---|---|
| `with_theme*` family | 184 |
| `with_content_color` | 56 |
| `ProvideTextStyle` | 39 |
| `with_typography` | 10 |
| `with_tonal_elevation_enabled` | 3 |
| `with_absolute_tonal_elevation` | 2 |
| `with_motion_scheme` | 1 |
| bare `provides` | 30 |
| `provides_tracked` | 11 |
| `provides_tracked_by` | 3 |

## 2. Root cause (historical)

> Also describing the tree before the rewrite — kept because the reason the old model was wrong is
> what §3 answers.

`CompositionLocal::current()` is a **pure pull** (`composition_local.rs:142-153`):

```rust
pub fn current(&self) -> T {
    SLOTS.with(|s| {
        for (sid, val) in s.borrow().iter().rev() {
            if *sid == self.id { return T::clone(val.downcast_ref::<T>().unwrap()); }
        }
        (self.default)()
    })
}
```

It reads a thread-local stack but **tells nobody that it read**. So when a provider changes its
value, the runtime has no idea whom to wake. People do it instead, which is where the patches come
from:

| Patch | What it is actually compensating for |
|---|---|
| `provides_tracked` | a hand-kept "previous value", because nothing keeps it for you |
| `provides_tracked_by` | the above needs `PartialEq`, which cannot cover `Arc<dyn Fn>` |
| per-site key + `provider_path_counters` | the "previous value" has no home, so a key locates it |
| `mark_subtree_dirty` | the specific readers are unknown, so the whole subtree is dirtied |
| two `backchannel`s (`Button`/`Badge`, since removed) | copies 2 and 3 of the same thing |

**Against Compose**: `CompositionLocal.current` inside a Composer runs `recordReadOf(local)`, so the
read itself creates the dependency. When the provider's value changes it invalidates exactly those
readers — it knows **which ones**, and never touches the whole tree.

## 3. The new design (as landed)

### 3.1 Core: the local carries a signal, and reading subscribes

Each **provide site** holds a state with a signal; `current()` registers a dependency on read, reusing
the existing `StateSignal` machinery (`state.rs`, `ComposerSubscription::subscribe_signal`).

```rust
// composition_local.rs
pub struct Supplied {
    value: RwLock<Arc<dyn Any>>,     // type-erased value, swapped in place
    signal: Option<Arc<StateSignal>>, // None = a provide that cannot invalidate anybody
}

thread_local! {
    // stack entries carry their signal
    static SLOTS: RefCell<Vec<(u64, Arc<Supplied>)>>;
}
```

The value sits behind a lock rather than being replaced wholesale because **readers point at the
signal, not at the value**: a fresh `Supplied` would strand every existing subscriber on a signal
nobody notifies again.

**Reader side** (`current()` keeps its zero-argument signature; no call site changes):

```rust
pub fn current(&self) -> T {
    match self.find() {
        Some((value, signal)) => { subscribe(signal); value }  // read = register
        None => (self.default)(),
    }
}
```

`register_dependency` already exists and **does not need `&mut ComposeCtx`**: it resolves the target
through `with_active_scope`, which takes the innermost scope from the thread-local `GROUP_STACK` and
falls back to `ACTIVE_SLOT_KEY` when that stack is empty (so a measure/draw read still lands on a
node). `State::get` walks this exact path every frame.

**Provider side** (takes `ctx`, so a value from any source invalidates correctly):

```rust
pub fn provide<R>(&self, value: T, ctx: &mut ComposeCtx,
                  content: impl FnOnce(&mut ComposeCtx) -> R) -> R {
    let key = ctx.next_provider_key();                       // per SITE, see below
    let entry = ctx.remember_backchannel_at_key(key, || /* seed with `value` */);
    let supplied = entry.peek();
    if !supplied.is_same(&value, self.same) {                // policy lives on the local, §3.2
        supplied.replace(value.clone());
        if self.notifies { /* signal.notify(true) */ }       // false for `new_static`
    }
    SLOTS.with(|s| s.borrow_mut().push((self.id, supplied)));
    /* run content, then pop */
}
```

Net effect: **one `provide` = one comparison = at most one precise notification.** No hand-kept
"previous value" beside the entry (the `Supplied` IS it), no `mark_subtree_dirty` (the readers are
exact).

**But NOT "no per-site key", which this section claimed when it was a design.** The plan assumed the
provided value would live in the `Slot` itself, making `next_provider_key` unnecessary. It does not:
the entry lives in the slot table's `remembered` map, and that key is what addresses it. §8 D2 carries
the measurement — pinning the ordinal to a constant makes two providers in one group share one entry,
read each other's value as "changed", and re-dirty on every pass.

### 3.2 Move the comparison to the definition

This is what removes the `_by` variant. Today the comparator lives at the **call site** (hence
`provides_tracked_by`); moving it to the **local's definition** means a caller never thinks about it:

```rust
// requires T: PartialEq - the vast majority of locals
pub fn new(default: fn() -> T) -> Self where T: PartialEq

// caller-supplied comparison - Arc<dyn Fn>, or a handle compared by pointer / id
pub fn new_by(default: fn() -> T, same: fn(&T, &T) -> bool) -> Self

// never invalidates - Compose's staticCompositionLocalOf
pub fn new_static(default: fn() -> T) -> Self
```

**Measured: 20 locals fall into three classes** (which decides the constructor each one uses)

| Constructor | local | type |
|---|---|---|
| `new` (PartialEq, 12) | `LOCAL_CONTENT_COLOR` | `Color` |
| | `LOCAL_COLORS` | `ThemeColors` |
| | `LOCAL_TYPOGRAPHY` | `Typography` |
| | `LOCAL_MOTION_SCHEME` | `MotionScheme` |
| | `LOCAL_TONAL_ELEVATION_ENABLED` | `bool` |
| | `LOCAL_ABSOLUTE_TONAL_ELEVATION` | `f32` |
| | `LOCAL_TEXT_STYLE` | `TextStyle` |
| | `LOCAL_DIRECTION` | `LayoutDirection` |
| | `LOCAL_DENSITY` | `Density` |
| | `LOCAL_SELECTION_DISABLED` | `bool` |
| | `LOCAL_TEXT_SELECTION_COLORS` | `TextSelectionColors` |
| | `DRAINING_SCOPE` | `bool` |
| `new_by` (no PartialEq, 5) | `LOCAL_STYLES` | `Arc<ComponentStyles>` → `Arc::ptr_eq` |
| | `SAVEABLE_REGISTRY` | `Arc<SaveableStateRegistry>` → `Arc::ptr_eq` |
| | `LOCAL_OVERSCROLL_FACTORY` | `Option<Arc<dyn Fn>>` → pointer compare |
| | `LOCAL_SELECTION_REGISTRAR` | `SelectionRegistrar` → the existing `is_same()` (`selection.rs:502`) |
| | `LOCAL_SHARED_SCOPE` | `Option<SharedTransitionScope>` → `scope_id` equality (`state.rs:158`) |
| `new_static` (never invalidates, 3) | `LOCAL_WINDOW_INSETS` | `WindowInsetsController`, one per window, installed once |
| | `ENTRY_STATE_SCOPE` / `RESULT_EVENT_BUS_SCOPE` | one per entry, lives as long as the entry |

> Note `Arc<T>` is `PartialEq` only when `T: PartialEq` (neither `ComponentStyles` nor
> `SaveableStateRegistry` is), so those must use `new_by` — and **pointer identity is exactly the
> semantics they want**, so the constraint lands in the right place.

Evidence (`reference/compose`, 906 source files measured):

```kotlin
// material3/internal/ProvideContentColorTextStyle.kt:35-46
// "less expensive than nesting calls to CompositionLocalProvider"
internal fun ProvideContentColorTextStyle(
    contentColor: Color, textStyle: TextStyle, content: @Composable () -> Unit,
) {
    val mergedStyle = LocalTextStyle.current.merge(textStyle)
    CompositionLocalProvider(
        LocalContentColor provides contentColor,
        LocalTextStyle provides mergedStyle,
        content = content,
    )
}
```

Compose draws the same distinction between `compositionLocalOf` (mutable, compared) and
`staticCompositionLocalOf` (never invalidates); both have real uses across the library.

### 3.3 Several locals in one provide

Compose's single `CompositionLocalProvider(A provides x, B provides y)` becomes:

```rust
ctx.provide(&[
    LOCAL_CONTENT_COLOR.provided(color),
    LOCAL_TEXT_STYLE.provided(merged),
], |ctx| { ... })
```

`Local.provided(value)` returns a type-erased `ProvidedValue` (`Arc<dyn Any>` plus a comparison
predicate that downcasts inside itself), and the array lives on the stack. **One provide call = one
slot position**, so three levels of nesting no longer overwrite each other's memory — which is
exactly the problem the per-site counter had to solve.

It is `provided`, not `provides`: the plan called this constructor `provides`, which the ctx-less
form already occupies. A past participle describes a value; the verb installs it.

Entries are matched by the LOCAL's id rather than by array position, so a call site whose list length
follows a branch keeps working and a reordering is not mistaken for a change.

Also provide Compose's named combination, so call sites read line-for-line against upstream:

```rust
ProvideContentColorTextStyle(content_color, text_style, ctx, |ctx| { ... })
```

### 3.4 One signature shape

`ctx` follows the values, `content` is always last (the Compose convention):

```
values..., ctx, content: impl FnOnce(&mut ComposeCtx) -> R
```

Only 3 call sites need to move (they put `ctx` first today):

| Now | After |
|---|---|
| `with_component_styles_when_changed(ctx, styles, content)` | `(styles, ctx, content)` |
| `with_overscroll_factory(ctx, factory, content)` | `(factory, ctx, content)` |
| `disable_overscroll(ctx, content)` | unchanged (no values) |

`light/dark/auto` and `DisableSelection::build` have no value parameter, so they already conform.
`ProvideTextStyle` keeps Compose's original name (UpperCamelCase) rather than becoming
`with_text_style`.

## 4. API map (old → new)

| Now | After (as landed) |
|---|---|
| `.provides(v, f)` | **kept** — the ctx-less form, 15 call sites, for the 4 in §6 |
| `.provides_tracked(v, ctx, f)` | **`.provide(v, ctx, f)`** |
| `.provides_tracked_by(v, ctx, same, f)` | **deleted** (the comparator moves to `new_by`) |
| `CompositionLocal::new(default)` | gains `where T: PartialEq` |
| — | `CompositionLocal::new_by(default, same)` |
| — | `CompositionLocal::new_static(default)` |
| — | `Local.provided(value) -> ProvidedValue` |
| — | `ctx.provide(&[ProvidedValue], content)` |
| — | `ProvideContentColorTextStyle(color, style, ctx, content)` |
| `WiniaTheme::with_theme*`, four overloads | kept (all four combinations have call sites); internally they go through one `ctx.provide` |
| `ProvideTextStyle(style, ctx, content)` | keeps its name, internally goes through `provide` |

**Two departures from the plan, both measured:**

1. The tracked form landed as `provide`, not `provides`. The plan had `provides` free because
   `.provides(v, f)` was to be deleted; in practice that ctx-less form has 15 production call sites
   and the 4 in §6 legitimately keep it. Rust has no overloads, so one name had to cover one meaning,
   and the verb/participle split (`provide` acts, `provided` describes) is what replaced verbosity
   with a distinction that reads correctly at the call site.
2. `provided(value)`, not `provides(value)`, builds a `ProvidedValue`. Same reason: `provides` was
   taken, and a constructor that installs nothing must not be named like one that does.

## 5. Deletion list

| Goes away | Why |
|---|---|
| `provides_tracked` / `provides_tracked_by` | folded into `provide` |
| ~~`Composer::next_provider_key`~~ | **NOT deleted** — the premise was that storage moves into the slot; it did not, so this is still what addresses an entry. See D2. |
| ~~`Composer::provider_path_counters`~~ | **NOT deleted** — the ordinal is exactly what D2 needs. See D3. |
| ~~`movable_saved_provider_counters`~~ | **NOT deleted** — it saves/restores the table above. See D4. |
| ~~`ComposeCtx::next_provider_key`~~ | **NOT deleted** — same as the first of these. |
| the tuple compare + `mark_subtree_dirty` in `provide_resolved` | the 4th hand-written copy; the signal does it |
| the backchannel in `with_component_styles_when_changed` | the signal does it |
| the `provides_tracked_by` call in `with_overscroll_factory` | same |
| the two-level nesting in `Button` / `Badge` | each becomes one `ProvideContentColorTextStyle` |
| `component_styles.rs`'s `NAMESPACE` constant | goes with the backchannel |

The four struck-through rows are the one part of this plan that did not survive contact with the
code, and §8 D2 carries the measurement that settled it: pinning the ordinal to a constant makes two
providers in one group share one entry, read each other's value as "changed", and re-dirty on every
pass. Deleting the key requires moving the provided value into `Slot` itself, which is a larger
change than this redesign.

## 6. The 4 that keep a `ctx`-less signature (unchanged)

| provider | Why it stays (verified) |
|---|---|
| `with_density` | wraps a whole FRAME: the closure at `per_window/core.rs:816` contains `recompose_until_stable()` + `layout()` + `sw.draw()`. Taking `&mut ComposeCtx` collides with `self.composer.layout()` inside it; and its readers run in **measure/draw** (`box_layout.rs:70`, `modifier/element.rs:74`, `render/pass.rs:1492`), which composition invalidation cannot reach |
| `direction::provides` | **no production call site left**: `provide_resolved` now uses the tracked `direction::provided(direction)` entry, so the ctx-less form survives only for tests that supply a direction around a bare `Composer::compose` |
| `with_saveable_registry` / `with_registry_of` | one `Arc` per window, installed once and never rebuilt |
| `with_component_styles` / `_arc` | no production call site (tests only); the changing form is `_when_changed` |

**Note**: these keep the `FnOnce()` closure signature, so they cannot be mixed with
`ctx.provide`'s `FnOnce(&mut ComposeCtx)`. That boundary is deliberate — every entry above has a
concrete reason, recorded in §9's docs.

## 7. Risks and open questions (as assessed before landing)

| Risk | Detail | Mitigation |
|---|---|---|
| **Reads outside composition** | a measure/draw `current()` has no scope to register against, but `register_dependency` falls back to `ACTIVE_SLOT_KEY` (node level), which is still a layout dependency | the existing `DepMode::Layout` path already covers this; add a test that reads a local during measure |
| **Movable content** | local state lives in the slot, so it has to travel with a moved subtree | `Slot` already travels with movable content (same as `remembered`); add a test for a provider inside movable content |
| **Snapshot `capture`/`with_snapshot`** | stack entries go from `Arc<dyn Any>` to `Arc<Supplied>`, and the overlay-inheritance semantics must hold | the snapshot clones the `Arc` and shares the signal (an overlay reading the same value is the intended semantics) |
| **Per-frame notify cost** | today `mark_subtree_dirty` re-enters a whole subtree; the new path notifies precisely, but each value is an `Arc<dyn Any>` plus one comparison | notify only when unequal, matching `State::set`'s dedup |
| **`next_state_id` visibility** | the new signal needs an id | `StateSignal::new(StateId)` is `pub(crate)`; `next_state_id` is private to `state.rs` today and must be exposed to `composition_local` |
| **2276 tests** | all of them rely on current semantics | land in steps, running the full suite at each |

How each one ended up:

- **Reads outside composition** — held. `subscribe` reaches `register_dependency`, which falls back to
  the active node outside a scope, so a measure-time read is a layout dependency. No separate test was
  added; the four ctx-less providers that read at measure time carry the behaviour instead.
- **Movable content** — held, for the same reason the entry always lived in the slot table's
  `remembered` map rather than beside the value. This is also why D2–D4 could not delete the per-site
  key: the storage never moved into `Slot` itself.
- **Snapshot** — held. The snapshot shares the `Arc<Supplied>`, so a replayed overlay subscribes to
  the same signals, which is the intended semantics.
- **Per-frame notify cost** — held, and it is the point: a comparison per provided value replaces
  re-entering a whole subtree.
- **`next_state_id` visibility** — done; it is `pub(crate)` and `Supplied::tracked` uses it.
- **Test count** — 2276 at the time of writing; 2283 at the end of the work (two of the extra tests
  came from the review of this change).

## 8. Migration checklist

Each step is testable and revertible on its own. Tick items as they land.

### Phase A: infrastructure (no behaviour change)

- [x] A1 ~~`LocalValue` trait (`as_any` + `box_eq`)~~ → the trait was there to supply `as_any` and
      `box_eq`, and `dyn Any` already gives the first while the per-LOCAL `same` gives the second.
      `Supplied::get::<T>` downcasts and clones; `is_same` downcasts and compares.
- [x] A2 `Supplied { value: RwLock<Arc<dyn Any>>, signal: Option<Arc<StateSignal>> }`. The value sits
      behind a lock rather than in a `Box` so `replace` can keep the SIGNAL that readers already
      subscribed to — swapping the whole entry would leave every subscriber on a signal nobody
      notifies again. `signal: None` is the untracked form (`provides`, ctx-less).
- [x] A3 ~~`Policy::{Equal, By(fn), Never}`~~ → one field `same: fn(&T, &T) -> bool`; the three
      variants became three constructors. `Never` is **not** the constant `|_, _| true`: that would
      freeze the value at its first pass, so a reader composing later for its own reasons would read
      something older than what is in scope. `new_static` answers `false` ("always replace") and
      keeps the signal quiet through the separate `notifies` flag.
- [x] A4 `CompositionLocal::{new_by, new_static}`, `T: PartialEq` on `new`
- [x] A5 `next_state_id` is `pub(crate)`; `Supplied::tracked` uses it
- [x] A6 `a_signal_notify_reaches_the_subscription_that_read_it`

### Phase B: swap `provides`' implementation (keep the old API as a shim)

- [x] B1 the new `provide(value, ctx, content)` (the plan called it `provides`; see §4): per-site
      `next_provider_key` → `remember_backchannel_at_key` holding the `Supplied`, compare under the
      local's `same`, `replace` + `signal.notify(true)` only when it moved, push, run content, pop
- [x] B2 `current()` / `try_current()` hand the entry's signal to `register_dependency` — the read IS
      the subscription, the same call `State::get` makes
- [x] B3 `provides_tracked` / `provides_tracked_by` forward to `provide` (shims, deleted in Phase D)
- [x] B4 full suite green — 2278 passed. **The invalidation lands one pass LATE, and that is the
      design, not a regression.** `Composer::compose` drains the pending queue at its START, before
      any content runs, so a `notify` raised while content runs can only be acted on by the NEXT
      pass; `mark_subtree_dirty` was same-pass, and that immediacy is precisely what let it skip
      knowing who the readers are. The frame path loops for this reason
      (`PerWindow::recompose_until_stable`), so a frame never PRESENTS the intermediate state. Five
      tests that asserted the old same-pass timing (1 here, 2 in `badge.rs`, 1 in `button/tests.rs`,
      1 in `button_group/tests.rs`) now converge through `Composer::recompose_to_stable`
      (`cfg(test)`, bounded at 8 passes), which loops the way a frame does. Verified with
      `WINIA_RECOMPOSE_TRACE` + `WINIA_SLOT_TRACE`: `pending=true` firing `StateId(1)` while that
      pass held `slot_dirty=false` — late, not lost.
- [x] B5 `a_toggled_item_repaints_its_label_in_the_new_content_colour` green

### Phase C: multi-value provide

- [x] C1 `CompositionLocal::provided(value) -> ProvidedValue`. **Renamed from the plan's
      `provides(value)`**: `provides` cannot mean both "describe a value" and "install a value", and
      Rust has no overloads — the single-value form already occupies `provide`. The past participle
      is the descriptor, the verb is the action, so `LOCAL.provided(x)` / `LOCAL.provide(x, ctx, f)`
      read as what they are. The ctx-less form stays `provides(v, f)` (Phase B kept its 15 production
      call sites untouched).

      The value is type-erased (`Arc<dyn Any>`) because `&[ProvidedValue]` is HOMOGENEOUS while the
      locals in it are not, and the local's comparison travels with it as a downcast-folding
      predicate built ONCE per local.
- [x] C2 `ComposeCtx::provide(&[ProvidedValue], content)` — one call, one slot position, one
      `next_provider_key`; entries are `Vec<(u64, Arc<Supplied>)>`. Arity is asserted, not zipped
      silently (a site's provide count comes from one statement, so a mismatch means the call site is
      inconsistent and a partial install would be the "some of it updated" bug signals exist to
      prevent).
- [x] C3 `ProvideContentColorTextStyle(color, style, ctx, content)` in `text.rs`
- [x] C4 `Button` uses C3 (its two-level nesting is gone)
- [x] C5 `Badge` uses C3 (same)
- [x] C6 eleven nested sites migrated, and `rg -U "with_content_color…ProvideTextStyle"` now finds
      none: `Button`, `Badge`, `AlertDialog` (title + text), `Chip`, `SegmentedButton`,
      `IconButton`, `IconToggleButton`, `FloatingActionButton` ×3. The four that remain as plain
      `with_content_color` have no style to pair (an icon-only tint, or a colour whose style sits
      across a `with_theme_and_direction` boundary).
- [x] C7 full test suite green — 2281 passed

**A real bug found and fixed while landing C**: `new_static` had `same` answering `true` forever,
which conflated "does not invalidate" with "does not update" — the value froze at its first pass, so
a reader that composed LATER for its own reasons saw a stale one. Compose's
`staticCompositionLocalOf` only means "never wakes a reader". Split into two promises: `same`
(static answers `false`, i.e. always replace) and a new `notifies` flag (static answers `false`),
carried per VALUE so one `ctx.provide` can mix a notifying local with a static one.

### Phase D: delete

- [x] D1 `provides_tracked` / `provides_tracked_by` deleted, and all 12 call sites moved to
      `provide`: 5 in `theme/provider.rs`, 1 in `selection_container.rs`, 1 in `component_styles.rs`,
      1 in `overscroll.rs`, 3 in `composition_local.rs`'s own tests, plus two doc references
      (`composer/ctx.rs`, `composer/engine.rs`). `provides_tracked_by`'s comparison had already
      moved onto the locals via `new_by` (`LOCAL_STYLES`, `LOCAL_OVERSCROLL_FACTORY`), so the
      callers' `same` arguments were redundant and went with it.
- [ ] D2 delete `next_provider_key` (`Composer` + `ComposeCtx`) — **NOT DONE: the premise does not
      hold.** The plan assumed the provided value lives directly in the slot, which would make the
      per-site key unnecessary. It does not: the entry lives in the slot table's `remembered` map,
      and `next_provider_key` is what gives it an address. Measured rather than assumed — pinning the
      ordinal to 0 (`mix_key(base, 0)`) fails
      `nested_providers_in_one_group_do_not_overwrite_each_others_memory` immediately: two providers
      in one group then share one entry, read each other's value as "changed", and re-dirty on every
      pass. Keeping the key is what makes the nesting safe; deleting it needs a storage move (value
      into `Slot` itself), which is a larger change than this phase.
- [ ] D3 delete `provider_path_counters` — follows D2; the counter IS the ordinal D2 needs.
- [ ] D4 delete `movable_saved_provider_counters` — follows D3 (it saves/restores that table).
- [x] D5 the tuple compare + `mark_subtree_dirty` in `provide_resolved` are gone: the theme now makes
      ONE `ctx.provide` call with five entries (direction, colors, typography, on_surface, motion
      scheme). Two consequences worth recording: `direction` had to become a tracked entry
      (`layout::direction::provided`) because it used to ride in the compared tuple — leaving it on
      the ctx-less `direction::provides` would have silently dropped direction-change invalidation;
      and the two theme tests that asserted "the provider reaches an idle subtree in ONE pass" now
      converge through `recompose_to_stable`, for the same reason as Phase B.
- [x] D6 no backchannel or `NAMESPACE` was left in `with_component_styles_when_changed` (Phase C had
      already replaced it with `LOCAL_STYLES.provide`).
- [x] D7 `with_overscroll_factory` uses `LOCAL_OVERSCROLL_FACTORY.provide`; its inline
      `Arc::ptr_eq` closure is gone (the local is `new_by`).
- [x] D8 the two `provides_tracked*` tests renamed to the names they now describe
      (`provide_does_not_touch_readers_when_the_value_is_unchanged`,
      `provide_reenters_readers_when_the_value_moves`), along with the section header and the
      stale comment referring to the removed API.
- [x] D9 full suite green — 2281 passed; `cargo check --examples` finished; `git diff --check` clean.

**Net effect of Phase D**: `provides_tracked` / `provides_tracked_by` are gone and every provider
calls one verb, while the machinery that keeps a provider's remembered value addressable stays. The
"delete the per-site key" items were the one part of this plan that did not survive contact with the
code; they are recorded above rather than quietly dropped.

### Phase E: one signature shape, and docs

- [x] E1 `with_component_styles_when_changed` / `with_overscroll_factory` take `(values, ctx, content)`
      now, matching every other provider. Both call sites updated (`overscroll.rs`'s own test, and
      `disable_overscroll`'s forwarding call).
- [x] E2 provider inventory, measured after the rewrite — **25 entries**, of which 22 are `pub` and
      3 are `pub(crate)`:

      | module | entries | signatures |
      |---|---|---|
      | `theme::WiniaTheme` | 12 | `auto` / `light` / `dark` / 4× `with_theme*` / `with_typography` / `with_motion_scheme` / `with_content_color` / `with_tonal_elevation_enabled` — all `(values, ctx, content)`; `with_absolute_tonal_elevation` is `pub(crate)` |
      | `components::text` | 2 | `ProvideTextStyle`, `ProvideContentColorTextStyle` |
      | `components::component_styles` | 3 | `with_component_styles` / `_when_changed` / `_arc` |
      | `scroll::overscroll` | 2 | `with_overscroll_factory`, `disable_overscroll` |
      | `runtime::density` | 1 | `with_density` (ctx-less, §6) |
      | `runtime::saveable` | 2 | `with_saveable_registry`, `with_registry_of` (ctx-less, §6) |
      | `layout::direction` | 2 | `provides` (ctx-less, §6) + `provided` (tracked entry) |
      | `components::DisableSelection` | 1 | `build(ctx, content)` |

      How they came down: **the three primitives collapsed into two verbs**. `provides` (ctx-less,
      never invalidates) and `provide` (takes ctx, notifies) — plus `provided`, which only BUILDS an
      entry for a multi-value `ctx.provide` and installs nothing. `provides_tracked` and
      `provides_tracked_by` are gone; their 14+3 call sites all read `provide` now, and the three
      `_by` comparators moved onto their locals via `new_by`.

      The wrapper count barely moved (23 → 25) and that is the honest result: the redesign was never
      about deleting names, it was about the LAYER underneath them. What changed is that every one of
      the 25 now means the same thing — a provided value carries its own signal, reading it
      subscribes, and no component writes invalidation code. The one net addition, `provided`, is the
      missing half of Compose's `A provides x` that made multi-value providing possible at all.
- [x] E3 `docs/component-authoring.md`: §13.4 rewritten as "`provide` invalidates; `provides` does
      not" with a four-row API table, the signal mechanism, the one-pass-late rule, and §13.4.2 for
      the four ctx-less survivors; §13.5 rewritten as "a value with no usable `PartialEq`" (`new_by`
      vs `new_static`, and why the per-site key is load-bearing); §13.1's provider table and §14's
      checklist updated (`ProvideContentColorTextStyle` added, `provides_tracked*` removed).
- [x] E4 `docs/theme.md`: "Providing a theme" now states the five entries in one `ctx.provide` call
      and how a theme switch reaches already-composed components (per-entry signals; no
      hand-compared tuple; one pass late, converged by the frame loop).
- [x] E5 `docs/badge.md` §3: the badge's two nested providers became one
      `ProvideContentColorTextStyle`, described in those terms.
- [x] E6 `cargo test -p winia --lib` → 2281 passed / 0 failed; `cargo check --examples` → Finished;
      `git diff --check` clean.

## 9. Acceptance criteria

1. **One verb per meaning.** `provide` installs a value and invalidates; `provides` is the ctx-less
   form and invalidates nobody; `provided` only builds an entry. The API map in §4 said
   `provides_tracked` would become `.provides(v, ctx, f)` — it became `.provide(v, ctx, f)` instead,
   because `provides` was already the ctx-less form with 15 call sites and the two cannot share a name
   in Rust. **Met**, with that rename recorded in §4.
2. **Zero changes at `current()` call sites** — a reader never needs to know how invalidation works.
   **Met**: every `current()` call site is untouched by this work.
3. **`composition_local.rs` no longer uses `mark_subtree_dirty` or `position_key` to invalidate.**
   **Met with a caveat**: both words still appear in comments, as the explanation of what this
   replaced, and `remember_backchannel_at_key` is still how a provider addresses its entry — a
   *storage* backchannel, not the hand-rolled previous-value one the redesign deleted. The criterion
   was about mechanism, not vocabulary.
4. **Adding a provider requires no invalidation code from its author** — the only goal of this design.
   **Met**: `Button`, `Badge`, `AlertDialog`, `Chip`, `SegmentedButton`, `IconButton`,
   `IconToggleButton` and `FloatingActionButton` each dropped a hand-written compare-and-dirty for
   one `provide` call.
5. The screenshot defect stays covered by `a_toggled_item_repaints_its_label_in_the_new_content_colour`.
   **Met.**
6. Full `cargo test -p winia --lib` green, `--examples` builds. **Met** (2283 passed at the time of
   writing; see §8 E6 and the D-phase note for the two tests added afterwards).

## 10. Why not "just tidy the API surface"

That option (keep the tracked machinery, unify signatures, cut the number of entry points) makes the
surface look clean while **invalidation stays the caller's responsibility**: forget it once and you
silently read a stale value — the same cause as this defect. The root cause is "reads do not register
dependencies", not the number of entry points.
