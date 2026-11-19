# Windowing design: identity, lifetime, and single-authority state

> Status: implemented, rev 3, 2026-11-19. All five phases are in the tree; the only sketched method
> that stayed on another surface is `WindowId::insets`, which is still `ctx.window_insets()` inside
> window content. Section 14 records what landed and how it was verified.
> Rev 2 keeps the model from rev 1 and drops API surface that did not earn its keep: no handle
> wrapper, no key type, no scope type, no options struct, no window manager, one creation entry.
> Section 15 records what was removed and why.
> Scope: the `Window` component's identity, lifetime, state, and open/close surface on Windows,
> macOS, Linux and Android.
> Companion reading: `docs/architecture.md` (module layout), `docs/back-navigation.md` (per-window
> runtime state), `docs/saveable.md` (persistence).

## 1. What the component was before this work

`Window` is a declaration that carries creation parameters. It is not a window object; that fact
still explains the model. The table below records the pre-work behaviour, not the implemented one
(sections 2-15 define what replaced it):

| Current behaviour | Where |
| --- | --- |
| Configuration is read only on the frame that creates the window; a later `.title()` or `.size()` is ignored | `winia/src/app/window.rs:223-259` |
| `build` returns `()`, so nothing outside the window can address it afterwards | `winia/src/app/window.rs:175` |
| The content closure is `Fn(&mut ComposeCtx)`, so a page cannot see its own window | `winia/src/app/window.rs:175` |
| A native close destroys the window immediately; `on_close` is a notification, not a request | `winia/src/app/app_state/handlers.rs:536-553` |
| Destruction has two implementations with different hook order | `winia/src/app/window.rs:96-120` vs `winia/src/app/app_state/handlers.rs:536-553` |
| The persistence key silently falls back to a per-instance id | `winia/src/app/app_state/helpers.rs:108` |
| `WindowState` has three fields and `WindowOptions` two; nothing covers position, placement or visibility | `winia/src/app/window.rs:42-46`, `winia/src/app.rs:43-46` |
| Identity lives in globals (`NEXT_ID`, `CREATED`) and is not public | `winia/src/app/window.rs:31-33` |

One consequence deserves its own paragraph, because it turns a naming question into a correctness
question. After a native close the declaration is still in the tree, and `need_new`
(`winia/src/app/window.rs:224`) is true again: the closed id was removed from `CREATED`, but
`created_id` was never cleared. The next compose pass therefore asks for a new window.
Applications avoid the re-creation only because the `on_close` callback usually flips the state
that hides the declaration, and that change happens to land before the parent re-runs. That is an
ordering dependency, not a contract, and a multi-window application can watch the window re-open.

The missing piece is not a feature list. It is a model: nothing in the API can answer who owns a
window's size, or which of the two destruction paths is the real one.

## 2. Invariants

Everything below follows from four rules. When a detail conflicts with one of them, the invariant
wins.

1. **The declaration is the only authority over a window's existence.** A `Window` in the tree
   exists; its absence destroys. No callback decides this on its behalf.
2. **No callback returns a lifetime decision.** A callback reports what happened; agreement is
   expressed as state.
3. **Every attribute has exactly one writer.** The other side reads a projection or submits a
   queued command.
4. **Every platform mutation goes through one frame-end queue**, applied in submission order and
   visible on the next frame.

## 3. The whole public surface

```rust
// One identity. Everything that acts on a window is a method on it.
pub struct WindowId(u64);
impl WindowId {
    pub fn raw(self) -> u64;
    pub fn is_realized(self) -> bool;
    pub fn is_focused(self) -> bool;
    pub fn state(self) -> WindowState;
    pub fn capabilities(self) -> WindowCapabilities;
    pub fn close(self);      // request: notifies on_close_requested, may be ignored
    pub fn destroy(self);    // command: always destroys
}

// One state handle. Getters read, setters queue a command.
pub struct WindowState { id: WindowId }
impl WindowState {
    pub fn size(&self) -> (f32, f32);
    pub fn set_size(&self, width: f32, height: f32);
    pub fn position(&self) -> WindowPosition;
    pub fn set_position(&self, position: WindowPosition);
    pub fn placement(&self) -> WindowPlacement;
    pub fn set_placement(&self, placement: WindowPlacement);
    pub fn visible(&self) -> bool;
    pub fn set_visible(&self, visible: bool);
    pub fn title(&self) -> String;
    pub fn set_title(&self, title: &str);
    pub fn resizable(&self) -> bool;
    pub fn set_resizable(&self, resizable: bool);
}

// One builder, one creation entry.
let id = Window::new()
    .title("Settings").size(600.0, 400.0).resizable(true).decorations(true)
    .key("settings")
    .on_close(|| {})                              // after the fact (compatibility)
    .on_close_requested(|id, reason| {})          // notification only
    .build(ctx, |ctx| { /* content */ });

// Context accessors.
ctx.window_id()                         // the window this content belongs to
ctx.windows()                           // every live window, in creation order
ctx.set_last_window_policy(LastWindowPolicy::Exit)

// Enums.
WindowPlacement { Windowed, Maximized, Fullscreen, Minimized }
WindowPosition  { Default, At(f32, f32) }
CloseReason     { User, Command }
LastWindowPolicy{ Exit, StayResident }
WindowCapabilities { position, placement, multi_window, native_close }
```

Two types carry behaviour, one builder builds, three context calls configure the app. There is no
`WindowHandle`, no `WindowKey`, no `WindowScope`, no `WindowOptions` and no `WindowManager`: each
would be a second name for something already reachable.

The whole surface in this section is implemented. Window insets are the one exception: they are
still `ctx.window_insets()` inside window content; there is deliberately no `WindowId::insets` yet.

## 4. Identity

- `WindowId` is minted when the declaration is first composed, not when the platform window
  appears. It is therefore always valid, and `is_realized()` answers the platform question
  separately. A `WindowHandle` wrapper around it would only add a conversion at every call site.
- Persistence is keyed by the string passed to `.key(..)`. A window without one keeps its state in
  memory and logs a debug note; it is not persisted. This deletes the silent `or(created_id)`
  fallback (`winia/src/app/app_state/helpers.rs:108`), whose failure mode is a window that
  re-opens empty with no error anywhere.
- The automatic slot key derived from `next_key()` (`winia/src/app/window.rs:180`) stays, but only
  as an in-memory slot identifier. It encodes the declaration's position in the tree, so inserting
  a sibling before it shifts every later key; using it for disk persistence would attach saved
  state to the wrong window.

## 5. Declaration and visibility

`Window::new().build(..)` keeps its current meaning - declare and create - so existing call sites
are unaffected. There is no second creation entry: a window that should appear on demand is a
declaration behind a state flag.

```rust
let show_tools = ctx.remember(|| false);

if show_tools.get() {
    Window::new().title("Tools").build(ctx, |ctx| { /* ... */ });
}
```

The root Composer is kept alive and recomposed, so this also works in `run_app!`: changing
`show_tools` after startup adds or removes the declaration. Multiple root declarations are tracked by
active id; removing one closes its window even while a sibling declaration is rebuilt.

Ownership is explicit on every pending window: `owner = Some(0)` for a root declaration,
`Some(window_id)` for a declaration inside a window's content, and `None` for the platform-level
`app::open_window_*` entry. Root declarations are reconciled by active-id diff after each root
recompose. A window-content declaration is reconciled through its composer's pending-close set, so
removing two declarations in one pass closes both. Closing a window also finalizes every window whose
`owner` is that window, recursively, on all three teardown entry points; a parent close cannot skip a
child's saveable snapshot or `on_close`.

`run_app` / `run_app_android` take `Fn` rather than `FnOnce` because the root is re-run. A root
closure that moves and consumes a non-`Clone` capture no longer type-checks; build the value inside
the closure or clone it.

Visibility is state, not existence:

| Intent | Expression | Composition | State |
| --- | --- | --- | --- |
| Hide | `id.state().set_visible(false)` | kept | kept |
| Destroy | drop the declaration, or `id.destroy()` | disposed | snapshotted, then dropped |

The existing `app::open_window_with_title` / `app::open_window_with_saveable_key` stay as the
platform-layer entry the declarative path itself calls (`winia/src/app.rs:242`, `:269`). They are
not part of the surface applications are meant to use, because they carry no declaration and thus
no lifetime rule.

## 6. `WindowState`

Methods rather than public `State` fields. A public field invites a write from anywhere, which is
how ownership ambiguity starts; a method can carry a rule.

**Placement is authoritative.** `size` and `position` are the remembered windowed geometry. While
`placement` is `Fullscreen` or `Maximized`, `set_size` and `set_position` only update that memory
and take effect when the window returns to `Windowed`; the call logs a debug line.

**Platform interaction.** There is no gesture-level suppression yet: a setter always queues a
command, and the platform's next size/position observation overwrites the local projection. A future
interaction gate must be documented here before it is implemented; the earlier claim that writes are
dropped while the user drags an edge described intended, not actual, behaviour.

## 7. Reaching the window from content

```rust
Window::new().build(ctx, |ctx| {
    let me = ctx.window_id();
    let width = me.state().size().0;
    /* ... */
});
```

`ctx.window_id()` follows the framework's existing shape: ambient, per-window facilities hang off
`ComposeCtx` (`ctx.permissions()` at `winia/src/runtime/composer/ctx.rs:116`, `ctx.back()`,
`ctx.window_lifecycle()` at `:128`). Passing the window as a closure parameter would change the
content closure's arity, which breaks every call site, and a `WindowScope` type would be a third
name for `WindowId` plus `WindowState`.

Outside window content the id is not meaningful; the call returns an unrealized id and asserts in
debug builds, which is diagnosable rather than a crash.

## 8. Building a window

One rule for every builder setter: **it seeds the initial value and is ignored once the window is
realized; live changes go through `WindowState`.** That is what today's code does implicitly, now
stated as a contract.

`.key("settings")` is the declarative way to make saved state leave the process: it is hashed into
the saveable store, and everything else is in-memory for the run. Persistence also needs a store to
be configured once at startup (`persist::set_store(PersistStore::for_app(..))`); without one nothing
is written, which is the right default for tests and previews.

`app::open_window_with_saveable_key` predates the split and keeps its old behaviour: it uses the
same explicit key for memory and disk persistence. That is a compatibility exception, not a second
declarative entry; new code uses `.key(..)` or `open_window_with_keys`.

`resizable` is runtime-toggleable on Windows and macOS, so it lives in the state (the builder setter
seeds it). There is no `always_on_top` builder method today. `WindowCapabilities` reports what the
current platform honours, so an application can branch on `native_close` or `multi_window` instead of
on `#[cfg]`.

## 9. The close pipeline

```rust
id.close();     // request: on_close_requested(reason) fires, the application may ignore it
id.destroy();   // command: no notification, always destroys
```

```rust
Window::new()
    .on_close_requested(move |id, reason| {
        // A notification, not a decision. Agreement means changing state:
        // set_visible(false), or drop the declaration, or call destroy().
        // The id is passed in because the handler acts on its own window.
        unsaved.set(true);
    })
    .build(ctx, |ctx| { /* ... */ });
```

| Initiated by | Notification | Agreement expressed as |
| --- | --- | --- |
| The user closing the window | yes, `CloseReason::User` | `set_visible(false)`, drop the declaration, or `destroy()` |
| `id.close()` | yes, `CloseReason::Command` | same |
| `id.destroy()` | no | applied immediately |
| The OS destroying the window (`Destroyed`) | no | finalizer runs, then the table entry is removed |
| The declaration disappearing | no (`on_close` still runs) | applied immediately |
| Application exit | no | applied immediately |

`CloseReason` has two variants because only two are reachable: a declaration removal and an
application exit never ask for permission, they simply proceed.

`LastWindowPolicy` is thread-local to the UI thread, not process-wide: it is read while the window
table is touched there. Setting it from another thread does not change the teardown decision.

Destroying a window whose declaration is still composing leaves that declaration inert rather than
re-creating the window on the next frame. The marker is cleared when the declaration disappears, so
removing and re-adding it is the supported way to get a fresh window.

There is one teardown (`PerWindow::finalize`) with several entry points, and its order is fixed:

```
request -> on_close_requested(reason) -> [application agrees] -> saveable snapshot
        -> on_close() -> composition disposal -> removed from the window table
```

The two older implementations that disagreed about snapshot/callback order are gone. The existing
`on_close` builder method keeps its meaning (after the fact, no reason). Root declarations reconcile
by active-id diff after the root recompose; window-content declarations use the per-composer pending
set and close every removed id in the same pass.

## 10. The command queue

```
during compose:   state.set_* / id.close / id.destroy  -> queued
end of compose:   declaration additions and removals    -> queued
frame end:        drain in submission order -> apply to the platform
                  -> consistency events queue back into the read projections
```

One queue means one ordering: whether a close submitted this frame happens now or next frame is
never a question. The overlay host already works this way (`request_back_close` and
`flush_back_close_requests` in `winia/src/app/overlay_host.rs`), so this is an existing convention
rather than a new mechanism.

## 11. One authority per attribute

| Attribute | Writer | Direction |
| --- | --- | --- |
| existence | the declaration, or `destroy()` | application |
| visible | `WindowState::set_visible` | application to platform |
| placement | `WindowState::set_placement`; authoritative | application to platform |
| size, position while `Windowed` | platform during interaction, application otherwise | split by interaction |
| size, position in other placements | application (remembered only) | application |
| focused | platform | platform to application, read-only |
| title, resizable | `WindowState` setters | application to platform |
| persistence key | `.key(..)` at declaration | application |

Every ambiguity found while designing this came from a row with two writers. The table is part of
the design, not documentation of it: adding a window attribute means adding a row first.

## 12. Platform notes

Desktop: every capability is available and `native_close` is true, so `on_close_requested` fires
when the user closes a window from its chrome.

Android: several winit windows are composited into one Activity surface in creation order, so
there is no independent top-level window. `position`, `placement`, `multi_window` and
`native_close` are false there, and `on_close_requested` never fires; applications provide their
own close entry through the back handler (`docs/back-navigation.md`). Reporting this through
`WindowCapabilities` is the point - `title`, `decorations` and `resizable` are accepted on Android
today and have no effect.

Verified on an Android 16 (API 36) x86_64 AVD (`agnes36`) with an app generated by `winia new` and
built by `winia build --target android --abi x86_64`: logcat printed `WINIA_CAPS position=false
placement=false multi_window=false native_close=false`, the Activity stayed alive, and the screenshot
showed the rendered content. This exercises the real Android branch of `WindowCapabilities::platform`,
not the desktop one.

A second Android consequence: the back pump routes unfocused events to the most recently created
window. That order now comes from the recorded window list (`windows_in_creation_order`), the same
list `ctx.windows()` returns; only windows opened through the platform-level API, which have no
declaration, still fall back to their `created_id`.

## 13. Compatibility facts (settled)

1. `WindowState` was not in `prelude` before this work; it and the other new types are exported
   from it now, so they are part of the public surface.
2. `build` returns `WindowId` instead of `()`. Ignoring a return value is legal in Rust, so no call
   site had to change; `#[must_use]` was deliberately not added.
3. `app::open_window_with_title` and `app::open_window_with_saveable_key` stay as wrappers
   (`winia/src/app.rs`); the new `open_window_with_keys` carries the separate disk key.
4. Creation still begins in `Window::build`, but it only pushes a `PendingWindow` into the existing
   global queue; the loop creates the platform windows from that queue in submission order. The
   frame-end command queue added here covers mutations and close, not creation.
5. `run_app` and `run_app_android` now take `Fn`, not `FnOnce`, and the root Composer is retained in
   `AppState.root`. This is a source-breaking change for a root closure that consumes a non-`Clone`
   capture; it is required for declarations behind a State flag to be re-evaluated.
6. Pending windows carry an `owner`: `Some(0)` for the root, the declaring window's platform id for
   content, `None` for the platform-level API. Root removals reconcile by active id; window-content
   removals use a pending-id set, not a single id.
7. `open_window_with_saveable_key` still writes the same key to disk for compatibility; the strict
   “only `.key(..)` persists” rule applies to the declarative path and to `open_window_with_keys`.

## 14. Migration phases

| Phase | Content | Risk |
| --- | --- | --- |
| 1 | `WindowId`, `WindowState`, frame-end command queue, `build` returns the id | additive; no call site changes |
| 2 | `ctx.window_id()`, `on_close_requested` as a notification | additive |
| 3 | One close pipeline; `on_close` wired to the merged path; `.key(..)` for persistence | behaviour change on the native-close path only |
| 4 | `LastWindowPolicy`, recorded z-order, back pump uses the recorded order | structural |
| 5 | Persistent root Composer, owner-aware declarations, active-id cleanup, multi pending close | structural; `run_app` closure becomes `Fn` |

Phase 1 is in the tree. Verified on desktop with a probe window: a `set_title` and a `set_size`
issued from a `side_effect` reached the OS window (the title read back through the window handle,
and an outer size of 535x371 for an inner request of 520x333), and the platform echo updated both
`WindowState` and `window_size()` in the next composition. That removes the hardest limitation
(`winia/src/app/window.rs:223-259`: configuration was read once), because live changes no longer go
through the builder at all.

Phases 1 and 2 are also in the tree. The close pipeline was verified on desktop by sending a
native close request to a probe window: with an `on_close_requested` handler that does not act, the
window stayed open and its title changed to the value the handler wrote; with a handler that calls
`destroy()`, the process exited. Both runs printed `close requested: User for WindowId(1)`.

Phase 3 is in the tree as well. Saved state was verified across two runs with a probe window that
declared `.key("probe-persist")`: the first run restored 0 and wrote
`window-ab3491ae05cd0306.json`, the second restored 1 from disk. Two things the implementation had
to get right: the in-memory key is still the declaration site's salted key (so close-and-reopen in
one process keeps working), while the disk key is only the application's `.key(..)` - and the
silent `saveable_key.or(created_id)` fallback is gone, because a per-instance id would attach saved
state to "the Nth window ever opened".

Phase 4 is in the tree too. `LastWindowPolicy` was verified with a probe that destroys its only
window: with the default `Exit` the process ended, with `StayResident` it stayed alive. The policy
governs every teardown entry point (the shared close pipeline, declaration removal, and winit's own
`Destroyed` event). `is_parent` no longer short-circuits it: a parent close first finalizes its
owned children, then the ordinary empty-table policy decides.
The back pump now takes its unfocused fallback from the recorded window list, so it and
`ctx.windows()` cannot disagree about creation order.

Phase 5 is in the tree. A probe declared a second window behind a root `State`, opened it from a side
effect, then cleared the state: debug-server `t` showed two windows after the open and one after the
close, with no panic. Window-content declarations now use a pending-id set, so removing two in one
pass closes both instead of only the last id. `mark_content_dirty` rebuilds the remaining windows after
a sibling closes, which is what makes a close-button/label update without pointer motion.

These probes are checked in now: `tests/window_lifecycle_test.rs` drives `fixture_all
window_lifecycle` through live size echo, close veto/accept, `LastWindowPolicy`, and state restored
across two processes. Five scenarios, all passing locally.

Two facts found while implementing, worth remembering for future window work:

- A composer carries two id spaces: `window_id` is the platform handle (frame clocks, focus,
  accessibility) while the declaration mints its own `created_id`. `ctx.window_id()` must report the
  latter; conflating them made the first cut look up a state table entry that did not exist.
- This winit fork exposes no window-focus event, so `is_focused()` reports whether the window held a
  focused node as of the last frame. That is the signal the loop actually has, and it is what the
  back pump already uses to choose a target window.

## 15. Decisions, removed surface, rejected alternatives

Decisions:

| Question | Decision | Why |
| --- | --- | --- |
| Last-window policy | `Exit` by default, triggered only by destruction | hiding is reversible; the framework must not end a process the application did not destroy. `StayResident` for tray applications |
| Hidden windows on exit | snapshotted | hiding means keep; losing state because a panel was collapsed an hour ago is data loss |
| Windows created on demand | a declaration behind a state flag | one creation entry; the lifetime rule stays visible in the tree |
| Persistence across a re-open | geometry only | a restored window needs content, which is application code; restoring the frame is the useful half |
| Naming | no `host`, no `attach` | `attach` already means attaching to a platform object (`WiniaBack.attach`); a second meaning would collide |

Surface removed in rev 2, with the reason:

| Removed | Reason |
| --- | --- |
| `WindowHandle` | a second name for `WindowId`; every call site would convert |
| `WindowKey` | `.key(&str)` says the same thing without a type |
| `WindowScope` | `ctx.window_id()` plus `WindowState` already covers it |
| `WindowOptions` | the builder is the description; a parallel struct duplicates it |
| `WindowManager` | `ctx.windows()` plus one policy call covers creation listing and app policy |
| `build_scoped` | passing the scope as a closure parameter breaks every call site for no gain |
| `CloseReason`'s four variants | only `User` and `Command` are reachable |
| `Lifetime` enum / manual-window API | a conditional declaration expresses the same thing |

Rejected alternatives:

- **Keep extending the builder.** It cannot express a live value without either re-applying the
  setter every frame (a silent behaviour change) or adding a second write path, which is the
  ambiguity this document exists to remove.
- **`on_close_request` returning `Accept` / `Reject` / `Defer`.** A declaration that says I exist
  cannot also return I may disappear. The framework would need a closed marker per declaration
  site, and losing that marker makes the window re-open. Doing nothing already means refusal.
- **Dropping `Window` for a `Surface` / `View` concept.** Correct for Android's single surface,
  wrong for desktop, where applications need real top-level windows.
- **A public `State` field per attribute.** No place to put the ownership rule, so the rule ends up
  duplicated at every call site.
