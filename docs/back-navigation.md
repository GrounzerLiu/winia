# Back Navigation and Predictive Back

> Status: design record, 2026-11-17. Not implemented yet.
> Supersedes gap 15 in `docs/navigation3.md` ("navigation event / predictive back, deliberately
> not done"), which was written when Android was not a committed target. The platform is now real:
> the template targets API 36 through GameActivity, so the earlier conclusion no longer holds.
> Scope: Android first; the Rust dispatcher itself is platform-agnostic so a desktop event source can
> feed it later.
> References used to pin the behaviour: androidx.activity 1.13.0 sources (`BackHandler.kt`,
> `PredictiveBackHandler.kt`, `OnBackPressedDispatcher.kt`, `ComponentActivity.kt`),
> androidx.navigationevent 1.0.0 (`OnBackInvokedInput.android.kt`, `NavigationEventProcessor.kt`),
> material3 1.5.0-beta01 (`BottomSheet.kt`, `NavigationDrawer.kt`, `SearchBar.kt`), and the
> Android 36 SDK manifest attribute documentation.

## 1. Goal

Winia should handle the Android system back gesture the way Jetpack Compose does, with the same
observable ordering and completion rules:

- Discrete back: `BackHandler(enabled = true) { ... }`.
- Predictive back: a handler receives start, per-frame progress (0..=1), and exactly one terminal
  event (completed or cancelled), plus the swipe edge and touch point.
- The most recently composed enabled handler wins. Handlers composed later are closer to the user.
- A gesture is owned by the handler that received its start. Progress and the terminal event go to
  that handler even if the handler table changes mid-gesture.
- When no handler is enabled, the platform performs its normal back action (finish the activity);
  winia must not swallow the event.
- Predictive back drives the navigation pop animation while the top entry stays on the back stack.
  The stack is modified only when the gesture completes, so cancellation needs no rollback.

Non-goals for this slice: iOS, browser back, forward navigation, multi-window Android, and gesture-
driven sheet/drawer motion (sheets and drawers register discrete handlers first; animating them with
progress is a later phase).

## 2. The Compose reference chain

### 2.1 Platform

| API level | Entry point | Events |
| --- | --- | --- |
| 33 | `OnBackInvokedDispatcher.registerOnBackInvokedCallback(PRIORITY_DEFAULT = 0, cb)` | `onBackInvoked()` only |
| 34+ | the same, with `OnBackAnimationCallback` (`extends OnBackInvokedCallback`) | `onBackStarted`, `onBackProgressed`, `onBackCancelled`, `onBackInvoked` |
| < 33, or `android:enableOnBackInvokedCallback="false"` | `Activity.onBackPressed()` | one call |

`PRIORITY_OVERLAY = 1000000` is reserved for overlays above the activity window. A `BackEvent`
carries `progress` (0..1), `swipeEdge` (`EDGE_LEFT = 0`, `EDGE_RIGHT = 1`, `EDGE_NONE = 2`),
`touchX`, `touchY`, and `frameTimeMillis` (API 36+). Predictive progress exists only from API 34;
API 33 delivers the terminal event alone. On API 34+ a hardware/three-button back also arrives as
`onBackStarted` with `EDGE_NONE` followed by `onBackInvoked`, so the synthetic-start path in the
Rust dispatcher is only used on API 33 or when the manifest flag is off (measured, see section 10).

### 2.2 androidx.activity

`ComponentActivity` creates `onBackPressedDispatcher` lazily and installs the system callback at
`ON_CREATE` via `setOnBackInvokedDispatcher(onBackInvokedDispatcher)`. Registration is dynamic: the
system callback exists only while at least one enabled handler exists, so once the last enabled
handler is gone the system plays its own back animation. Since 1.13 the event model lives in `androidx.navigationevent`:
two inputs are registered (default and overlay priority), handlers are ordered by priority and then
recency, and the handler chosen at `started` owns the rest of the gesture. Without a handler, the
dispatcher's fallback calls `super.onBackPressed()` (finish).

### 2.3 Compose

`BackHandler` / `PredictiveBackHandler` read the dispatcher owner from
`LocalOnBackPressedDispatcherOwner` (falling back to the view tree and the context), register once,
toggle `enabled` through a side effect, and unregister on dispose. `PredictiveBackHandler` allocates
one channel and one job per gesture: started launches the coroutine with the progress flow,
progressed feeds it, completed closes it, cancelled cancels it. The handler must collect the flow.

The documented precedence rule is composition order: the last composed enabled handler wins. Calling
the effect conditionally changes that order, which is why the API asks for an `enabled` argument
instead of an `if`.

## 3. Winia architecture

```text
Android system
  |- API 34+  OnBackAnimationCallback: started / progressed / cancelled / invoked
  |- API 33   OnBackInvokedCallback:   invoked only
  |- API <33  Activity.onBackPressed()
        |  Java main thread
        v
com.winia.back.WiniaBack (generated Java, com.winia package, like WiniaPermissions)
        |  five native methods: enqueue + wake_loop(); never touch UI state
        v
BackInbox (process-wide, Mutex<VecDeque<BackSignal>>, Send + Sync)
        |  event-loop thread: AppState::new_events drains it each turn
        v
BackDispatcher (one per window, Rc/RefCell, UI thread only)
        |  scan handlers newest-first, overlay priority before default
        v
BackHandler / PredictiveBackHandler
        v
user code: NavDisplay preview pop, dialogs, pages
```

New modules:

| File | Responsibility |
| --- | --- |
| `winia/src/back/mod.rs` | public types, the two composables, `ctx.back()` |
| `winia/src/back/dispatcher.rs` | handler table, ordering, gesture state machine |
| `winia/src/back/inbox.rs` | queue, progress coalescing, wake |
| `winia/src/platform/android/back.rs` | JNI entry points, attach/detach, enable toggling |
| `winia/src/app/app_state/handlers.rs` | drain in `new_events`, suspend/resume gating |
| `winia/src/app/per_window/core.rs` | `PerWindow` gains `back: BackDispatcher` |

Threading rule: `BackInbox` is the only `Send + Sync` boundary. `BackDispatcher`, its handlers, and
the `State`/Backchannel values they write live on the event-loop thread and are never touched from
JNI. JNI methods do exactly two things: append to the inbox and call the existing wake hook
(`APP_PROXY.wake_up()` through `runtime::state::set_wake_fn`).

## 4. Rust API

```rust
pub enum BackSwipeEdge { Left, Right, None }

#[derive(Clone, Copy)]
pub struct BackEvent {
    pub touch: (f32, f32),          // logical pixels, density already applied
    pub progress: f32,              // 0.0..=1.0
    pub edge: BackSwipeEdge,
    pub frame_time: std::time::Duration,
}

pub enum BackUpdate {
    Started(BackEvent),
    Progressed(BackEvent),
    Completed,
    Cancelled,
}

#[composable]
pub fn back_handler(ctx: &mut ComposeCtx, enabled: bool, on_back: impl Fn() + 'static);

#[composable]
pub fn predictive_back_handler(
    ctx: &mut ComposeCtx,
    enabled: bool,
    on_back: impl FnMut(BackUpdate) + 'static,
);

pub enum BackPriority { Default, Overlay }

pub enum BackHandlerSpec {
    Discrete(Box<dyn Fn()>),
    Predictive(Box<dyn FnMut(BackUpdate)>),
}

impl ComposeCtx {
    /// Low-level handle for framework consumers (NavDisplay, overlays).
    pub fn back(&self) -> BackDispatcher;
}

impl BackDispatcher {
    pub fn register(&self, priority: BackPriority, enabled: bool, handler: BackHandlerSpec)
        -> BackRegistration;
    pub fn set_enabled(&self, registration: BackRegistration, enabled: bool);
    pub fn remove(&self, registration: BackRegistration);
    pub fn has_enabled(&self) -> bool;
}
```

Registration uses the existing effect model: `ctx.remember` a `BackRegistration` (a Copy id),
register on first build, `side_effect` writes `enabled` each pass, `on_forget` removes it. The
cleanup closure captures the id and the window id, resolves the dispatcher through the UI-thread
registry, and stays `Send`, which `on_forget` requires.

A handler registered in the application root BEFORE a `Window` composes belongs to a Composer no
window pumps, so it never fires. Register inside the window content (or any child composer that
shares the window dispatcher, including overlays). This is the same shape as Compose's requirement
that a `BackHandler` live under an `OnBackPressedDispatcherOwner`.

### 4.1 Dispatch rules (Compose parity)

1. Handlers are scanned newest-first; overlay-priority handlers are scanned before default ones.
2. Disabled handlers are skipped. If none is enabled, dispatch returns `false`.
3. `Started` picks an owner. The owner receives every `Progressed` and the terminal event.
4. If the owner is removed or disabled mid-gesture, it receives a synthetic `Cancelled`, the
   gesture is cleared, and the terminal event is dispatched against the current table. This is a
   deliberate difference from Compose, where the gesture has no route once the flow is cancelled;
   the platform gesture still has to end somewhere.
5. A non-predictive event (API 33, three-button navigation, `onBackPressed`) is dispatched as
   `Started` (edge `None`, progress 0) followed immediately by `Completed` when the owner is
   predictive, and as a bare `Completed` when it is discrete.
6. `enabled` changes apply to new events only; they never complete an owned gesture early.
7. On desktop, `predictive_back_handler` can be registered but receives no events unless something
   calls `BackDispatcher::dispatch`; the debug server gains injection commands for tests.

### 4.2 Why callbacks and not a suspend/flow API

Compose's flow exists because Kotlin has suspending collection: the code after `collect` means
completion and `CancellationException` means cancellation. `BackUpdate::{Completed, Cancelled}`
express the same state machine without a coroutine, allocation, or cancellation plumbing. The events
arrive at frame cadence and must be applied on the UI thread; a channel would add overhead and no
expressive power. A future async wrapper (`BackGestureStream`) can sit on top without changing the
core.

## 5. Android bridge

New generated file: `android/app/src/main/java/com/winia/back/WiniaBack.java` (fixed `com.winia`
package, sibling of `com.winia.permissions.WiniaPermissions`).

```java
package com.winia.back;

public final class WiniaBack {
    public static void attach(Activity activity);      // from Rust after the event loop exists
    public static void detach();                       // on destroy
    public static void setEnabled(boolean enabled);    // only while enabled handlers exist
    static void finishActivity();                      // Rust fallback: nothing consumed the event

    private static native void nativeBackStarted(float x, float y, float progress, int edge, long nanos);
    private static native void nativeBackProgressed(float x, float y, float progress, int edge, long nanos);
    private static native void nativeBackCancelled();
    private static native void nativeBackCompleted();
    private static native boolean nativeHasHandlers();

    // Called by MainActivity.onBackPressed() on API < 33.
    public static boolean dispatchLegacyBack() { /* nativeHasHandlers() + enqueue completed */ }
}
```

Registration rules:

- API 34+: register an `OnBackAnimationCallback` with `PRIORITY_DEFAULT`; started/progressed/
  cancelled call the matching native method, and `onBackInvoked` calls `nativeBackCompleted()`.
- API 33: register a plain `OnBackInvokedCallback`; `onBackInvoked` calls `nativeBackCompleted()`.
  API 33 sends no progress, so the dispatcher treats it as a non-predictive event.
- Registration happens on `setEnabled(true)` and unregisters on `setEnabled(false)`, posted to the UI
  thread. With no registration the system plays its normal back animation.
- `nativeHasHandlers()` is a fast path only: when it is false, Java calls `activity.finish()`
  directly (what `super.onBackPressed()` does for a plain activity). When it is true the event is
  enqueued; if the dispatcher then finds no handler (a race), it asks Java to finish through
  `finishActivity()`. Never call `onBackPressed()` from the bridge, which would re-enter the override.
- API < 33: `MainActivity` overrides `onBackPressed()`; `dispatchLegacyBack()` enqueues the terminal
  event, otherwise `super.onBackPressed()` runs.
- `attach` is called from `run_app_android` after the `AndroidApp` is available. `detach` cancels the
  in-flight gesture, clears the inbox, and unregisters. The activity can be recreated, so attach is
  idempotent and state lives in the Rust bridge, not in Java statics across instances.

Manifest: add `android:enableOnBackInvokedCallback="true"` to `<application>`. The attribute
documentation states that with `false` the system keeps forwarding `KEYCODE_BACK` and
`onBackPressed()` instead of calling the registered callback. targetSdk 35 and later enable
predictive back by default, but the template sets the attribute explicitly.

Threading: every callback above runs on the Java main thread. The native side must not touch `State`
or dispatcher internals; it appends to the inbox and wakes the loop. Progressed events are coalesced
(replace the tail when it is also `Progressed`), so a fast gesture cannot build a backlog; terminal
events are never dropped. Queue capacity 64.

## 6. Event loop integration

```rust
fn new_events(&mut self, _event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
    for signal in crate::back::drain() {
        if let Some(pw) = self.active_window_mut() {
            pw.back.dispatch(signal);
        }
    }
}
```

`new_events` runs on the loop thread after the wake and before window events, which is exactly the
point where a UI-thread-only dispatcher is legal. Dispatch may mark `State` values dirty, so the
frame that follows picks the update up. Events that arrive before the first window exists stay in the
inbox until the first turn.

Lifecycle: winit's `resumed` / `suspended` toggle the bridge. While suspended, `setEnabled(false)`
is called, incoming events are dropped, and the in-flight gesture is cancelled; on resume the
registration is restored from the live handler table.

## 7. NavDisplay predictive pop

Today `NavTransition` keeps `current_key`, `previous`, `forward`, `progress` (1 = old scene fully
visible, 0 = settled), and `active_spec`. `detect()` starts a transition when the scene key changes
and drives `progress` with `push_animatable`. `calculate_scene(&entries[..base_len])` is a pure
function, so the incoming scene for a pop can be computed without touching the stack.

Add a `Backchannel<Option<PredictivePop>>` to `NavTransition`:

```rust
struct PredictivePop<K: NavKey> {
    incoming: SceneHolder<K>,    // scene for entries[..base_len - 1]
    outgoing: SceneHolder<K>,    // current top scene
    spec: NavTransitionSpec,     // pop spec snapshotted at gesture start
    latest: f32,                 // last reported progress
}
```

Gesture flow:

1. `NavDisplay` registers `predictive_back_handler(enabled = can_pop, ...)` before building scene
   content, so handlers declared inside the scene register later and win (a page can intercept back
   for unsaved changes). `can_pop` requires `base_len > 1` and no other transition in flight.
2. `Started`: compute `incoming` from `entries[..base_len - 1]`, snapshot the current scene as
   `outgoing`, set `current_key` to the incoming key, `active_spec` to the pop spec, and
   `progress = 1 - p`. The back stack is untouched.
3. `Progressed`: write `progress` as a backchannel value; render code peeks it, so no recomposition
   per frame.
4. `Completed`: call `back_stack.pop()`. `detect()` sees the new scene key and would normally start
   a second pop transition; when the key equals the preview's `incoming` key, clear `previous` and
   `progress` without restarting the animation. If the stack changed during the gesture, fall back
   to a normal pop transition.
5. `Cancelled`: animate `progress` back to 1 with the snapshotted spec, clear the preview, restore
   `current_key` to the real top scene.
6. While a preview is active, `detect()` must not start a competing transition on the same progress
   value.

Entry cleanup (`on_pop`, state pools) runs only on a real pop, so cancellation leaves no state.

Dialogs and other overlay entries are excluded: `base_len < stack.len()` means the top entries are
dialogs, and the top dialog's own handler owns back (section 8). Multi-display windows (list-detail,
multiple `NavDisplay`s) should call `.handle_back(false)` and register one handler for the focused
pane.

## 8. Overlays and Escape

Today overlay dismissal lives in `PerWindow::escape_key`: it closes the topmost layer that has an
`on_dismiss` callback, swallows when `dismiss_on_back_press` is false, and clears focus when nothing
is open. That is Escape semantics, not Compose back semantics, and it must not leak into Android.

The unified model:

- `BackPriority::Overlay` handlers are scanned before default handlers. Dialog, AlertDialog,
  ModalBottomSheet, NavigationDrawer, Popup and DropdownMenu register a discrete handler at overlay
  priority while visible, with `enabled = visible && dismiss_on_back_press`.
- Their `on_back` calls the existing `on_dismiss`/`begin_overlay_close` path, so
  `dismiss_defers_to_content` keeps working.
- `dismiss_on_back_press = false` now means "handler disabled", so the event falls through to the
  page below. That is Compose behaviour (the dialog simply does not consume back) and replaces the
  current "swallow the key" rule. The old rule existed because winia draws overlays in the same
  window as the page; for Android back that reason does not apply.
- Escape inherits the same rule: with `dismiss_on_back_press = false` the key falls through to the
  page (and the focus fallback may run) instead of being swallowed behind the scrim. This is an
  accepted behaviour change, recorded here so it is not mistaken for a regression.
- Escape dispatches a synthetic discrete back (`edge = None`, progress 0) through the same
  dispatcher. If no handler consumes it, `PerWindow` keeps its winia-specific fallback: clear focus.
  The overlay branch inside `escape_key` is deleted.

Affected docs and tests that assert the old Escape behaviour: `docs/dropdown-menu.md` (Escape closes
the menu), `winia/src/app/overlay_host/tests.rs` (`escape_respects_dismiss_on_back_press`), and the
AlertDialog tests that forward `dismiss_on_back_press`. They are updated with the refactor.

Predictive sheets and drawers (Material3 drives their offset from gesture progress) are a follow-up:
the API exists, but this slice registers them as discrete handlers, so the system shows its own
preview and no in-app motion.

## 9. winia-forge changes

| Location | Change |
| --- | --- |
| `templates/blank/android/app/src/main/java/com/winia/back/WiniaBack.java` | new, generated header |
| `templates/blank/android/app/src/main/java/__PACKAGE_PATH__/MainActivity.java.tmpl` | add `onBackPressed()` |
| `templates/blank/android/app/src/main/AndroidManifest.xml.tmpl` | `android:enableOnBackInvokedCallback="true"` |
| `crates/winia-cli/src/commands/new.rs` | add the Java file to the embedded template list |

No migration path is implemented. There are no known external projects, and `winia android sync`
deliberately manages only Gradle build configuration (`managed_android_files()`, guarded by
`managed_files_are_build_configuration_only`). Old generated projects are expected to be recreated.
`winia doctor --target android` may print an informational note when the bridge class is missing, but
it does not modify files.

CI: add a `javac` compile step for the generated Java files against `android.jar` and the
games-activity artifact, so a broken bridge cannot land.

## 10. Phases

Phase 0 (spike, no production code): **verified 2026-11-17 on the agnes36 AVD (API 36, x86_64)**
with a GameActivity 4.4.0 probe. Results:

- GameActivity 4.4.0 does not register a competing back callback; a plain `PRIORITY_DEFAULT`
  registration is enough, no `dispatchKeyEvent` fallback is needed.
- `android:enableOnBackInvokedCallback="true"` is honoured: `registerOnBackInvokedCallback` at
  `PRIORITY_DEFAULT` succeeded without an exception on the GameActivity instance.
- An edge swipe delivers `onBackStarted(progress = 0, edge = LEFT)` then a stream of
  `onBackProgressed` frames at ~16 ms, reaching `progress ≈ 0.575` before `onBackInvoked`.
- `adb shell input keyevent KEYCODE_BACK` delivers `onBackStarted(edge = NONE)` and
  `onBackInvoked`, with no progress frames (the log showed `started progress=0.0 edge=2`).
- The activity stays alive after `onBackInvoked` returns (the probe consumed back and did not call
  `finish()`), which is the behaviour the bridge needs.

Phase A: `back` module, dispatcher, priorities, effects, unit tests. Desktop debug injection.

Phase B: Java bridge, JNI, inbox, event-loop drain, suspend/resume, legacy `onBackPressed`; AVD
verification of discrete back through both API paths.

Phase C: `predictive_back_handler` semantics end-to-end and `NavDisplay` preview pop with cancel
spring-back; AVD screen recording.

Phase D: overlays at overlay priority, Escape unification, updated tests and affected docs.

Phase E: forge template/manifest/CI, doctor note, release notes.

Acceptance: a page pops with the gesture following the finger; releasing completes the pop without a
second animation; dragging back to the edge restores the page at the same visual position; dialogs
close on back only when `dismiss_on_back_press` allows it; back on the root page finishes the
activity; Escape behaviour on desktop matches the new overlay rules.

## 11. Testing

- Dispatcher unit tests: ordering (last composed wins), overlay before default, enabled toggling,
  gesture ownership, mid-gesture removal, non-predictive event shape, empty table returns false.
- Composer tests: two `back_handler` calls, registration/removal across recomposition, `on_forget`
  cleanup.
- Debug server: `back start|progress|cancel|complete` commands feed the same dispatcher; a UI fixture
  drives NavDisplay and asserts the final stack and progress samples.
- Android: AVD checklist for gesture navigation and three-button navigation, dialog dismissal,
  activity recreation, and API 33 vs 34+ behaviour (a debug variant with targetSdk 33 exercises the
  legacy path if needed).
- CI: `cargo fmt`/`clippy`/focused tests plus the new `javac` check in winia-forge.

## 12. Risks and unknowns

- GameActivity's back-callback behaviour is resolved by the Phase 0 spike: direct registration
  works, so no `dispatchKeyEvent` fallback is planned.
- `registerOnBackInvokedCallback` thread requirements must be honoured by posting to the UI thread.
- The completed event has no synchronous return path to Rust; Java uses `nativeHasHandlers()` as a
  fast path and Rust asks Java to `finishActivity()` when dispatch reports the event unhandled.
- Progress coalescing may drop intermediate frames; only the latest progress matters for animation.
- NavDisplay preview must not fight `detect()`; the "same scene key" handoff is the fragile part and
  needs a focused test.
- Escape semantics change and the affected component tests must move in the same commit.
- With an overlay open and `dismiss_on_back_press = false`, Escape now falls through to the page
  instead of being swallowed; the release notes must mention it.
- Activity recreation must cancel in-flight gestures; otherwise a stale preview survives the new
  composition.
- Predictive back on API 33 is terminal-only; the navigation preview cannot animate there. That is a
  platform limit, not a winia gap.

## 13. Decisions

| Decision | Choice | Why |
| --- | --- | --- |
| NavDisplay registration | automatic by default, `.handle_back(false)` opt-out | preview needs internal transition state; scene content registers later and wins; NavHost precedent |
| Migration | none; templates only | no external users; `android sync` stays build-config only |
| Rust API | `FnMut(BackUpdate)` plus low-level `ctx.back()` | same state machine as Compose's flow without coroutine plumbing; events are frame-rate UI-thread writes |
| Overlays | unified now with `BackPriority::Overlay` and Escape routed through the dispatcher | one semantic on every platform; avoids a permanent Escape/back fork |

Rejected alternatives: a per-callback channel/flow API (allocation and cancellation for no gain);
letting `NavDisplay` stay opt-in only (predictive preview would be unavailable to the default path);
keeping the Escape swallow rule for back (contradicts Compose and hides events from the page);
introducing a migration command (maintenance for zero users).

## 14. Implementation map

| Area | File |
| --- | --- |
| public API | `winia/src/back/mod.rs` |
| dispatcher | `winia/src/back/dispatcher.rs` |
| inbox and wake | `winia/src/back/inbox.rs` |
| Android JNI | `winia/src/platform/android/back.rs` |
| app loop | `winia/src/app/app_state/handlers.rs`, `winia/src/app/app_state.rs` |
| per-window state | `winia/src/app/per_window/core.rs` |
| nav preview | `winia/src/nav/display.rs`, `winia/src/nav/back_stack.rs` |
| overlays | `winia/src/app/overlay_host.rs`, `winia/src/components/alert_dialog.rs`, bottom sheet, navigation drawer, search bar, dropdown menu |
| Escape | `winia/src/app/per_window/core.rs` |
| template | `winia-forge/crates/winia-cli/templates/blank/android/...`, `winia-forge/crates/winia-cli/src/commands/new.rs` |
| CI | `winia-forge/.github/workflows/ci.yml` |

## 15. Implementation status (2026-11-17)

Landed on branch `v2`:

- `322bded feat(back)`: dispatcher, priorities, gesture state machine, `back_handler` /
  `overlay_back_handler` / `predictive_back_handler`, per-window Composer wiring, 18 focused tests.
- `bc998be feat(back)`: `com.winia.back.WiniaBack`, five native methods, the cross-thread inbox,
  pump in `new_events` / `about_to_wait`, registration sync. Verified on the agnes36 AVD (API 36,
  x86_64) with a GameActivity probe: automatic registration after composition, key back
  `started(edge=NONE) + invoked`, edge swipe `started(edge=LEFT) + invoked`, the Rust handler wrote
  its on-device marker, and the activity stayed alive.
- `8b5fdad feat(nav)`: `NavDisplay` predictive pop preview (progress from the gesture, stack
  untouched until completion), completion handoff and cancel spring-back, plus two integration
  tests.
- `dd946d6 feat(back)`: overlay-priority handlers for dismissible overlays and Escape routing
  through the dispatcher when no overlay is open.

Review pass (2026-11-17), issues found while auditing the first cut and fixed:

- The predictive settle flags were rebuilt on every composition, so the completion callback's flag
  was invisible to the next build and the preview (with its full-screen input blocker) never
  cleared. Both flags are now remembered, and the motion's finish callback wakes one build.
- Subcomposition composers (lazy items, `BoxWithConstraints`) did not inherit the window's back
  table, so a handler there was inert. `inherit_window` now copies the dispatcher.
- Activity recreation left the Rust `ENABLED` cache and the Java registration out of step; attach
  resets the cache, and `detach` calls a native reset.
- `NavDisplay` disabled its handler during a normal transition, which unregistered the platform
  callback and let a back press finish the activity; it is now enabled whenever the stack can pop.
- The inbox could drop a `Started` event while full, and the pump drained events before any window
  existed. Both are fixed.

Follow-ups after the review are now implemented: overlay back handlers queue a close request that
`PerWindow` drains through `begin_overlay_close` (host exit animation preserved), a completed event
that finds no handler asks Java to `finishActivity`, and the plain `OnBackInvokedCallback` branch
was exercised on the API 36 device with a demo-only `force_plain_back` flag. The real API 33 image
was provisioned afterwards (default and `google_apis` x86_64, both SHA-1 checked): on API 33 the
Java bridge registers an `OnBackInvokedCallback` (confirmed by `CoreBackPreview`), and a plain test
Activity verified that `onBackInvoked` reaches the native callback (`handlers=false`, then finish).
The full app still cannot render on the API 33 emulator: Skia's Vulkan backend crashes inside the
guest `libvulkan.so` at `GetDeviceQueue2` before the window appears. That is an emulator/renderer
limitation, not the back path; the dispatcher logic is platform-independent and was verified on the
API 36 device. A GL build would avoid it, but `skia-bindings 0.99.0` publishes no x86_64 Android GL
binary and the full Skia build fails in `git-sync-deps` on the read-only registry checkout.

Predictive sheet and drawer motion is implemented: `ModalBottomSheet` registers an
overlay-priority predictive handler and maps gesture progress onto the `SheetState` offset between
the current anchor and `Hidden`; `ModalNavigationDrawer` does the same between `Open` and `Closed`.
The demo's "Open predictive sheet" screen follows the gesture on the phone and settles into
`hide()` on release.

The CI javac step covers the WiniaBack template only; the Activity template needs games-activity on
the classpath. `winia doctor` now reports the bridge file and the manifest opt-in (Android projects
only).
