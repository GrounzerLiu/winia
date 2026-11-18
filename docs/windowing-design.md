# Windowing design: identity, lifetime, and single-authority state

> Status: design record, 2026-11-18. Not implemented yet.
> Scope: the `Window` component's identity, lifetime, state, and open/close surface on Windows,
> macOS, Linux and Android.
> Companion reading: `docs/architecture.md` (module layout), `docs/back-navigation.md` (per-window
> runtime state), `docs/saveable.md` (persistence).

## 1. What the component is today

`Window` is a declaration that carries creation parameters. It is not a window object, and that
single fact explains most of its limitations:

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

One consequence is worth stating on its own, because it turns a naming question into a correctness
question. After a native close the declaration is still in the tree, and `need_new`
(`winia/src/app/window.rs:224`) is true again because the closed id was removed from `CREATED` but `created_id`
was never cleared. The next compose pass therefore asks for a new window. Applications avoid the
re-creation only because the `on_close` callback usually flips the state that hides the
declaration, and that state change happens to land before the parent re-runs. That is an ordering
dependency, not a contract, and a multi-window application can observe the window re-opening.

The missing piece is not a feature list. It is a model: nothing in the API can answer who owns a
window's size, or which of the two destruction paths is the real one.

## 2. Invariants

Everything below follows from four rules. When a detail conflicts with one of them, the invariant
wins.

1. **The declaration is the only authority over a declarative window's existence.** A `Window` in
   the tree exists; its absence destroys. No callback may decide this on its behalf.
2. **No callback returns a lifetime decision.** A callback reports what happened; agreement is
   expressed as state or as an explicit command.
3. **Every attribute has exactly one writer.** The other side reads a projection or submits a
   queued command.
4. **Every platform mutation goes through one frame-end queue**, applied in submission order and
   visible on the next frame.

## 3. Identity

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WindowId(u64);            // minted at declaration, never reused
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowKey(&'static str);  // application-supplied, stable across re-opens

#[derive(Clone)]
pub struct WindowHandle { id: WindowId }
```

- `WindowId` is minted when the declaration is first composed, not when the platform window
  appears. The handle is therefore always valid; `is_realized()` answers the platform question
  separately. This removes the identity-before-existence gap that a handle wrapping
  `Option<WindowId>` would introduce.
- Persistence is keyed by `WindowKey` only. A window without one keeps its state in memory and
  logs a debug note; it is not persisted. This deletes the silent `or(created_id)` fallback
  (`helpers.rs:108`), whose failure mode is a window that re-opens empty with no error anywhere.
- The automatic key derived from `next_key()` (`window.rs:180`) stays, but only as an in-memory
  slot identifier. It encodes the declaration's position in the tree, so inserting a sibling
  before it shifts every later key; using it for disk persistence would attach saved state to the
  wrong window.

## 4. Declaration and visibility

`Window::new()` keeps its current meaning - declare and create - so existing call sites are
unaffected. Hosting an existing window is a separate entry point:

```rust
Window::new()             // declarative creation (unchanged)
Window::host(handle)      // host an existing window, create nothing
```

Visibility is state, not existence:

| Intent | Expression | Composition | State |
| --- | --- | --- | --- |
| Hide | `state.set_visible(false)` | kept | kept |
| Destroy | drop the declaration, or `handle.destroy()` | disposed | snapshotted, then dropped |

## 5. `WindowState`

```rust
pub fn remember_window_state(ctx: &mut ComposeCtx) -> WindowState;

impl WindowState {
    // Reads: immediate projections.
    pub fn size(&self) -> (f32, f32);
    pub fn position(&self) -> WindowPosition;
    pub fn placement(&self) -> WindowPlacement;
    pub fn visible(&self) -> bool;
    pub fn title(&self) -> String;
    pub fn resizable(&self) -> bool;
    pub fn is_focused(&self) -> bool;    // read-only
    pub fn is_realized(&self) -> bool;   // read-only
    pub fn capabilities(&self) -> WindowCapabilities;

    // Writes: queued commands, applied at the next frame end.
    pub fn set_size(&self, w: f32, h: f32);
    pub fn set_position(&self, p: WindowPosition);
    pub fn set_placement(&self, p: WindowPlacement);
    pub fn set_visible(&self, v: bool);
    pub fn set_title(&self, title: impl Into<String>);
}
```

Methods rather than public `State` fields. A public field is an invitation to write from
anywhere, which is exactly how ownership ambiguity starts; a method can carry a rule.

**Placement is authoritative.** `size` and `position` are the remembered windowed geometry.
While `placement` is `Fullscreen` or `Maximized`, `set_size` and `set_position` only update that
memory and take effect when the window returns to `Windowed`; the call is documented as deferred
and logs a debug line.

**Ownership during platform interaction.** While the user drags an edge or the system animates a
placement change, the platform wins and programmatic writes are dropped (debug log). This mirrors
the existing rule that a sheet or drawer ignores a second gesture while it is settling, so the
framework has one convention for conflicting input instead of two.

## 6. The close pipeline

```rust
pub enum CloseReason { User, Command, Declared, AppExit }

impl WindowHandle {
    pub fn close(&self);     // request: notifies on_close_requested, the application may ignore it
    pub fn destroy(&self);   // command: skips the notification, always destroys
}
```

```rust
Window::host(handle)
    .on_close_requested(move |reason| {
        // A notification, not a decision. Agreement means changing state:
        // set_visible(false), or drop the declaration, or call destroy().
        unsaved.set(true);
    })
    .build_scoped(ctx, |scope, ctx| { /* ... */ });
```

| Initiated by | Notification | Agreement expressed as |
| --- | --- | --- |
| The user closing the window | yes | `set_visible(false)`, drop the declaration, or `destroy()` |
| `handle.close()` | yes | same |
| `handle.destroy()` | no | applied immediately |
| The declaration disappearing | no (`Declared` reaches `on_closed` only) | applied immediately |
| Application exit | no | applied immediately |

There is exactly one pipeline, and its order is fixed:

```
request -> on_close_requested(reason) -> [application agrees] -> saveable snapshot
        -> on_closed(reason) -> composition disposal -> removed from the window table
```

This merges the two current implementations, which differ in whether the snapshot happens before
or after the callback. The existing `on_close` builder method is kept and maps to `on_closed`
(after the fact, no reason), so its behaviour does not change.

## 7. Scope and the two build entry points

```rust
pub struct WindowScope {
    id: WindowId,
    handle: WindowHandle,
    state: WindowState,
}

impl WindowScope {
    pub fn id(&self) -> WindowId;
    pub fn handle(&self) -> WindowHandle;
    pub fn state(&self) -> &WindowState;
    pub fn is_focused(&self) -> bool;
    pub fn insets(&self) -> WindowInsets;
}
```

```rust
// Unchanged signature: every existing call site keeps compiling.
Window::new().title("A").size(800.0, 600.0).build(ctx, |ctx| { /* ... */ });

// Scope plus a handle, for code that needs to address the window later.
let handle = Window::new()
    .key("settings")
    .build_scoped(ctx, |scope, ctx| { /* ... */ });
```

Two explicit names instead of one method whose return type depends on a type parameter: the
question of whether `build` creates a window never has to be answered by inference.

## 8. Options and capabilities

```rust
pub struct WindowOptions {
    pub decorations: bool,
    pub transparent: bool,
    pub window_type: WindowType,       // Regular | Dialog | Tooltip | Popup
    pub parent: Option<WindowHandle>,
}

pub struct WindowCapabilities {
    pub position: bool,
    pub placement: bool,
    pub multi_window: bool,
    pub native_close: bool,
    pub decorations: bool,
    pub always_on_top: bool,
}
```

`resizable` and `always_on_top` move from options to state: both are runtime-toggleable on Windows
and macOS, so calling them creation-time-only would be false. The builder methods `.resizable()`
and `.decorations()` stay as initial values.

The compatibility rule for every builder setter is one sentence: **a builder setter seeds the
initial value and is ignored once the window is realized; live changes go through
`WindowState`.** That is what today's code already does implicitly, now stated as a contract.

## 9. The command queue

```
during compose:   state.set_* / handle.close / handle.destroy  -> queued
end of compose:   declaration additions and removals           -> queued
frame end:        drain in submission order -> apply to platform
                  -> consistency events queue back into the read projections
```

One queue means one ordering: there is no question about whether an open and a close submitted in
the same frame happen this frame or the next. The overlay host already uses this pattern
(`request_back_close` and `flush_back_close_requests` in `overlay_host.rs`), so it is an existing
convention rather than a new mechanism.

## 10. One authority per attribute

| Attribute | Writer | Direction |
| --- | --- | --- |
| existence | the declaration (or `destroy()` for a managed window) | application |
| visible | `WindowState::set_visible` | application to platform |
| placement | `WindowState::set_placement`; authoritative | application to platform |
| size, position while `Windowed` | platform during interaction, application otherwise | split by interaction |
| size, position in other placements | application (remembered only) | application |
| focused | platform | platform to application, read-only |
| title | `WindowState::set_title` | application to platform |
| persistence key | `WindowKey` | application |

Every ambiguity found while designing this came from a row with two writers. The table is
therefore part of the design, not documentation of it: adding a window attribute means adding a
row first.

## 11. Platform notes

Desktop: every capability is available, and `native_close` is true, so `on_close_requested` fires
when the user closes the window from its chrome.

Android: several winit windows are composited into one Activity surface in creation order, so
there is no independent top-level window. `position`, `placement`, `always_on_top`,
`multi_window` and `native_close` are all false there, and `on_close_requested` never fires.
Applications provide their own close entry through the existing back handler
(`docs/back-navigation.md`). Announcing these as capabilities rather than exposing fields that
silently do nothing is the point of `WindowCapabilities`: `title`, `decorations` and `resizable`
are currently accepted on Android and have no effect.

A second Android consequence: the back pump currently routes unfocused events to the window with
the highest `created_id` (`app_state/handlers.rs`). Once the manager owns the window table, that
order can come from the recorded z-order and `is_focused()`, and the `created_id` sort disappears.

## 12. Compatibility checklist

These must be settled before any code changes:

1. Is `WindowState` re-exported from `prelude`? If not, its public fields can be reshaped freely;
   if it is, the fields stay and gain `#[deprecated]` accessors.
2. `WindowOptions` is a public struct with public fields (`app.rs:43-46`), so adding a field breaks
   struct-literal construction. Either keep the existing fields and add setters, or take one small
   break and mark it `#[non_exhaustive]`.
3. `app::open_window_with_title` and `app::open_window_with_saveable_key` are public
   (`app.rs:242`, `app.rs:269`). If external code uses them they stay as thin wrappers around the
   manager.
4. `build` keeps its signature and behaviour, but creation moves from the first `need_new` frame to
   the frame-end queue. The z-order of several windows created in one frame needs a test either
   way.

## 13. Migration phases

| Phase | Content | Risk |
| --- | --- | --- |
| 1 | `WindowId`, `WindowHandle`, `WindowState`, frame-end command queue | additive; no call site changes |
| 2 | `build_scoped`, `WindowScope`, `on_close_requested` as a notification | additive |
| 3 | One close pipeline; `on_close` wired to `on_closed`; `WindowKey` for persistence | behaviour change on the native-close path only |
| 4 | `WindowManager` owning the window table, z-order and `LastWindowPolicy`; remove the `created_id` sort in the back pump | structural |

Phase 1 already removes the hardest limitation (`window.rs:223-259`: configuration is read once),
because live changes stop going through the builder at all.

## 14. Open decisions

1. `LastWindowPolicy`: does hiding the last window keep the process alive (tray-style) or exit?
   Proposed default: exit, with an explicit `StayResident` for applications that want the other
   behaviour.
2. Does a hidden window persist its state on application exit? Proposed: yes - hiding is a user
   intent to keep the window, so the exit path snapshots it like any other window.
3. Are manager-opened windows destroyed on application exit? Proposed: yes, and they are not
   restored on the next run unless the application re-opens them.
4. Naming: `Window::host` versus `Window::attach`. `host` describes the relationship; `attach`
   matches overlay terminology already used by the codebase.

## 15. Rejected alternatives

- **Keep extending the builder.** It cannot express a live value without either re-applying the
  setter every frame (a silent behaviour change) or adding a second write path, which is the
  ambiguity this document exists to remove.
- **`on_close_request` returning `Accept` / `Reject` / `Defer`.** A declaration that says I exist
  cannot also return I may disappear. The framework would have to remember a closed marker per
  declaration site, and losing that marker makes the window re-open. Doing nothing already means
  refusal, which needs no enum.
- **Dropping `Window` for a `Surface` / `View` concept.** Correct for Android's single surface,
  wrong for desktop, where applications need real top-level windows.
- **A public `State` field per attribute.** No place to put the ownership rule, so the rule ends up
  duplicated at every call site.
