# Window review - 2026-11-24

> Review baseline: `D:/Projects/winia`, branch `fix/window-state-wake`, commit `f3bf319`.
> Scope: the declarative `Window` API, per-window state, creation/command/teardown pipelines, and
> the public surface exposed by `D:/Projects/winia/winia/src/lib.rs`.
> Evidence standard: the original findings were confirmed by static control-flow analysis at the baseline.
> The repair-status section records the targeted runtime regressions executed for the first batch.

## Executive summary

The review found ten deterministic defects and eleven API-gap categories. The defects share one
root cause: declaration state, requested state, observed platform state, and platform mutation are
not kept as separate, ordered stages. In particular, creation is not atomic with queued commands,
observations do not notify the composition system, and teardown does not cover every queue that can
own a future window.

The current first repair batch is:

1. D1 - the content closure is not refreshed after creation.
2. D2 - pending child windows escape the parent-close cascade.
3. D7 - an empty saveable snapshot does not clear a previous snapshot.
4. D9 - an empty title has different framework and platform representations.
5. D10 - `parent_window_id` is never reassigned after the original parent closes.

The first batch is implemented and covered by targeted regressions; see the repair-status table. All other defects and API gaps remain backlog items.

## Confirmed defects

### D1 - The content closure is not refreshed after creation (High)

- **Evidence:** `D:/Projects/winia/winia/src/app/window.rs:253-257` accepts a new content closure on
  every `build`; `D:/Projects/winia/winia/src/app/window.rs:310-316` computes `need_new`; the only call
  that transfers content to the platform queue is inside that creation branch at
  `D:/Projects/winia/winia/src/app/window.rs:351-364`. `D:/Projects/winia/winia/src/app/per_window/core.rs:140-160`
  stores the original boxed closure, and `D:/Projects/winia/winia/src/app/app_state/helpers.rs:193`
  keeps invoking that stored closure.
- **Mechanism:** once `need_new` is false, a later declaration pass can construct a closure over new
  values or a different conditional branch, but it never publishes that closure to the existing
  `PerWindow`. Captured `State` handles may still notify, but a plain captured value cannot change.
- **Impact:** an existing sub-window can display stale text, stale callbacks, or the wrong branch
  until it is destroyed and recreated. The window identity is stable while its content is not.
- **Recommendation:** keep a stable per-declaration content slot. When `build` executes, publish the
  latest closure into that slot and dirty the target window's composer without recreating the native
  window. Define whether publication happens on every declaration pass or only on an explicit
  content revision; avoid an unconditional redraw loop.
- **Suggested verification:** declare a window whose content captures a plain `String`, change that
  string from root state, and assert that the same `WindowId` shows the new string. Repeat with a
  conditional content branch and assert that no close/reopen occurs.

### D2 - Pending child windows escape the parent-close cascade (High)

- **Evidence:** pending requests carry an `owner` at `D:/Projects/winia/winia/src/app.rs:73-75` and
  wait in `pending_content` at `D:/Projects/winia/winia/src/app/app_state.rs:27`. The queue is drained
  separately at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:44-53`. The recursive cascade at
  `D:/Projects/winia/winia/src/app/window.rs:153-175` only iterates the live `windows` map; the other
  teardown entry at `D:/Projects/winia/winia/src/app/window.rs:218-233` has the same limitation.
- **Mechanism:** if a parent closes after its child has been queued but before that child is opened,
  the child is invisible to `close_owned_children`. A later drain opens it with an owner that no
  longer exists, producing an orphan native window and a leaked declaration id.
- **Impact:** parent close is not a reliable lifetime boundary. This is especially visible when a
  parent's initial composition declares a child and also requests the parent's destruction.
- **Recommendation:** make cancellation operate over both live and pending windows. Before opening a
  pending request, validate its owner chain; when an owner closes, recursively remove matching
  pending requests and unregister their declaration ids. Specify whether `on_close` runs for a
  never-realized window so the behavior is uniform across teardown paths.
- **Suggested verification:** queue a child (and a grandchild), destroy the parent before the next
  drain, and assert that no native child appears, no `CREATED` id remains, and callback counts match
  the chosen never-realized policy.

### D3 - Commands queued before realization are delayed past the first frame (Medium)

- **Evidence:** setters queue immediately at `D:/Projects/winia/winia/src/app/window_state.rs:367-370`;
  declaration and realization are separate at `D:/Projects/winia/winia/src/app/window_state.rs:387-406`.
  `open_window` realizes at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:162-170`, then composes,
  draws, and calls `set_visible(true)` at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:171-219`,
  and only inserts the window into the map at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:221`.
  Command application enumerates that map at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:93-104`.
  `new_events` applies commands before processing pending windows at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:247-256` and
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:421-422`.
- **Mechanism:** commands queued by the declaring composition are not in `self.windows` when the
  pre-creation drain runs. Creation then presents a frame using seed/default values; the commands
  are applied only by a later `about_to_wait`/`new_events` pass.
- **Impact:** the first presented frame can flash the wrong title, size, placement, or visibility.
  A pre-realization `set_visible(false)` cannot prevent the initial show.
- **Recommendation:** use an explicit creation transaction: create hidden, insert the `PerWindow`,
  apply pre-realization commands, compose/layout/draw, apply commands queued by that composition,
  and only then show the window. Preserve queue order across both drains.
- **Suggested verification:** immediately after `build`, queue `set_visible(false)`, `set_title`,
  `set_size`, and `set_placement`. Capture native state before the first presented frame and assert
  that no default/visible frame escaped.

### D4 - Window state is not reactive (High)

- **Evidence:** `WindowCell` is built from `Cell`/`RefCell` at
  `D:/Projects/winia/winia/src/app/window_state.rs:334-347`; lookups are plain thread-local map reads at
  `D:/Projects/winia/winia/src/app/window_state.rs:379-385`; identity and state getters at
  `D:/Projects/winia/winia/src/app/window_state.rs:24-231` do not register a composition dependency.
  Platform observations at `D:/Projects/winia/winia/src/app/window_state.rs:416-424` only assign cells.
  `ctx.windows()` also returns a plain vector at `D:/Projects/winia/winia/src/runtime/composer/ctx.rs:112-117`.
  The existing fixture documents the workaround at `D:/Projects/winia/winia/tests/ui_fixtures/window_lifecycle.rs:76-87`.
- **Mechanism:** reading `size()`, `position()`, `placement()`, `visible()`, `title()`, `resizable()`,
  `is_realized()`, `is_focused()`, or `ctx.windows()` subscribes to nothing. `wake_loop()` schedules a
  pass but does not tell a composer which slot became dirty.
- **Impact:** UI bound only to window state keeps its first value. The demo must increment a separate
  refresh `State` after every command, and OS-driven changes have no automatic UI path.
- **Recommendation:** expose a tracked snapshot or per-attribute `State`/version token. Separate
  requested values from observed values, publish observations through the notification system, and
  make window-list reads reactive as well.
- **Suggested verification:** render state readouts without a manual refresh tick, resize/move/focus
  the native window, and assert that the affected readouts recompose automatically.

### D5 - Position is never written back from the platform (Medium)

- **Evidence:** the position cell is initialized to `Default` and is only read or queued against at
  `D:/Projects/winia/winia/src/app/window_state.rs:153-170` and
  `D:/Projects/winia/winia/src/app/window_state.rs:334-345`. Realization updates only size and title at
  `D:/Projects/winia/winia/src/app/window_state.rs:397-406`. `WindowEvent::Moved` only refreshes the frame
  interval at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:747-750`.
- **Mechanism:** there is no `observed_position` path. `set_position(At(..))` does not optimistically
  update the cell, and neither creation nor a move event supplies the actual outer position.
- **Impact:** `WindowState::position()` remains `Default` after both programmatic and user moves.
  Position-dependent UI and future geometry persistence cannot use the advertised getter.
- **Recommendation:** add an observation path for initial and subsequent outer positions, convert to
  the documented logical coordinate space, and define whether `At` means screen coordinates or the
  monitor work area. Publish the observation reactively as part of G1.
- **Suggested verification:** set a position, drag the window, move it across monitors with different
  scale factors, and assert that each platform echo reaches `position()` in logical units.

### D6 - Placement and focus projections are distorted (High)

- **Evidence:** `set_placement` immediately overwrites the projected placement before the platform
  command runs at `D:/Projects/winia/winia/src/app/window_state.rs:172-190`; realization does not observe
  placement at `D:/Projects/winia/winia/src/app/window_state.rs:397-406`. Focus is derived from whether a
  content node is focused at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:93-104`. The native
  `Focused(false)` event only cancels a pointer gesture at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:741-750`.
- **Mechanism:** placement reports the last request rather than the platform state and ignores
  maximize/fullscreen/minimize changes made through window chrome. Focus reports content focus, not
  OS window focus; a background window with a focused node remains `is_focused() == true`, while a
  foreground window with no focused node reports false.
- **Impact:** state-driven UI can show the wrong placement and focus. Activation, debugging, and
  future modal behavior cannot rely on these projections.
- **Recommendation:** keep requested and observed placement separate. Observe platform placement
  changes where events exist, or reconcile against the native window at frame boundaries. Expose OS
  focus separately from content focus and gate unsupported operations with capabilities.
- **Suggested verification:** maximize, minimize, restore, and enter fullscreen through both the API
  and native chrome; then Alt-Tab between two windows, including one with no focusable content.

### D7 - An empty saveable snapshot does not clear old values (High)

- **Evidence:** `PerWindow::finalize` writes memory and disk snapshots only when `!saved.is_empty()` at
  `D:/Projects/winia/winia/src/app/per_window/core.rs:99-108`. `perform_save` legitimately returns an empty
  map when every provider has been removed at `D:/Projects/winia/winia/src/runtime/saveable.rs:165-180`.
  Reopening consumes the old in-memory value or loads the old disk value at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:129-147`.
- **Mechanism:** removing the last `remember_saveable` registration produces an empty current
  snapshot, but the guard treats that as 'nothing to save' instead of 'clear the previous snapshot'.
- **Impact:** deleted state reappears after close/reopen, in memory or across processes. This is data
  resurrection, not merely a missed update.
- **Recommendation:** when a registry exists, persist its current snapshot even if empty. For disk
  storage, either write a valid empty snapshot or delete the prior record with an explicit,
  versioned policy. Apply the same rule to both the in-memory and persistence keys.
- **Suggested verification:** save a nonempty keyed state, remove all registrations, close, reopen,
  and assert the default value. Run once with memory only and once with a real `PersistStore` across
  two processes.

### D8 - Command order is not preserved across control commands or windows (Medium)

- **Evidence:** the queue is per-window at `D:/Projects/winia/winia/src/app/window_state.rs:343` and
  `D:/Projects/winia/winia/src/app/window_state.rs:367-370`. `apply_window_commands` collects windows by
  iterating a `HashMap` at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:93-98`, then partitions
  each queue into `platform` and `to_close` at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:108-125`. All platform commands run first at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:126-139`; closes run afterward at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:141-143`.
- **Mechanism:** `[Destroy, Title(..)]` becomes a title write followed by a close. A close handler
  runs before later property writes in the same submitted queue, and commands it queues wait for a
  later drain or are discarded if the same batch closes the window. Cross-window submission order is
  lost because queues are grouped by `HashMap` order.
- **Impact:** callbacks can observe stale platform state, commands after a terminal destroy still
  touch the native window, and inter-window dependencies are nondeterministic. This contradicts the
  documented 'submission order' contract in `D:/Projects/winia/docs/windowing-design.md:274-286`.
- **Recommendation:** either introduce a global sequence number/queue, or explicitly narrow the
  guarantee to per-window ordering. Within a window, process terminal commands at their submitted
  position and define whether later commands are dropped. Specify how callback-enqueued commands
  are ordered relative to the current batch.
- **Suggested verification:** use a recording fake window to assert exact operation order for
  `[Destroy, Title]`, `[CloseRequest, Visible]`, and callback-enqueued commands. Add a two-window
  case if global ordering is promised.

### D9 - Empty titles are inconsistent between state and the platform (Low)

- **Evidence:** `WindowSpec::default` uses an empty title at
  `D:/Projects/winia/winia/src/app/window.rs:63-69`; declaration passes it into `WindowSeed` at
  `D:/Projects/winia/winia/src/app/window.rs:328-335`. Native creation substitutes `Winia` at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:59-63`, but realization is given the original
  empty `pending.title` at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:164-169`.
- **Mechanism:** the fallback is applied only to `WindowAttributes`; the live state and the native
  window are seeded from different title values.
- **Impact:** the OS title bar can say `Winia` while `WindowState::title()` says nothing. Accessibility
  and state-driven title UIs can disagree with the visible chrome.
- **Recommendation:** canonicalize the title once, before declaration and native creation, using one
  documented rule for omitted and explicitly empty titles. Pass the canonical value through seed,
  attributes, and realization.
- **Suggested verification:** cover omitted title, `.title("")`, a normal title, and a later
  `set_title`; assert equality between native chrome and `WindowState::title()` in every case.

### D10 - `parent_window_id` is not updated after the parent closes (Medium)

- **Evidence:** the field is declared at `D:/Projects/winia/winia/src/app/app_state.rs:28-29` and set
  only when it is `None` at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:222-225`. The shared
  close path removes the window without reassigning it at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:16-28`; the native `Destroyed` path likewise
  removes it without reassignment at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:697-730`.
  The stale value controls `is_parent` at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:555-558`
  and parent-only debug-event consumption at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:1665-1668`.
- **Mechanism:** the first platform window remains the cached parent even after it no longer exists.
  No surviving window can become `is_parent`, and the legacy debug target is not retargeted.
- **Impact:** parent-only routing and debug automation can remain bound to a dead window. The field
  is no longer a lifetime authority, but it is still live routing state and must not be stale.
- **Recommendation:** centralize parent selection and recompute it after every removal: choose the
  earliest surviving creation-order window, clear it when none remain, and update the legacy debug
  target in the same transaction.
- **Suggested verification:** open two root windows, close the original parent, and assert that the
  survivor becomes the parent/debug target. Repeat with an OS `Destroyed` event and with closing the
  last window.

## API gaps

### G1 - Observable state

- **Current evidence:** the public identity and state getters are plain cell reads at
  `D:/Projects/winia/winia/src/app/window_state.rs:24-231`; `ctx.windows()` is a plain vector at
  `D:/Projects/winia/winia/src/runtime/composer/ctx.rs:112-117`.
- **Impact:** applications cannot bind UI directly to realized, focused, geometry, placement, or
  window-list changes. This is the public-surface consequence of D4.
- **Proposed API:** a tracked `WindowSnapshot`/`WindowState` handle, or per-attribute observable
  values, with a documented requested-versus-observed distinction.
- **Suggested verification:** a UI fixture that reads each public state value and updates solely from
  native events, with no manual refresh state.

### G2 - Initial position, placement, and visibility

- **Current evidence:** the builder exposes title, size, resizable, and decorations at
  `D:/Projects/winia/winia/src/app/window.rs:73-105`. `WindowSeed` already has internal placement and
  visibility fields at `D:/Projects/winia/winia/src/app/window_state.rs:305-331`, but `build` hard-codes
  `visible: true` and `WindowPlacement::Windowed` at `D:/Projects/winia/winia/src/app/window.rs:326-335`.
  No position seed exists, and native attributes are created at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:56-71`.
- **Impact:** a window cannot start hidden, maximized, minimized, fullscreen, or at a chosen position
  without a post-creation command and possible first-frame flash.
- **Proposed API:** `.position(WindowPosition)`, `.placement(WindowPlacement)`, and `.visible(bool)`
  as creation seeds, applied before the first show.
- **Suggested verification:** create each initial state and inspect both `WindowState` and native
  chrome before the first presented frame; verify Android capability fallbacks.

### G3 - Window insets on the public window identity

- **Current evidence:** insets are available only through the content composer's
  `ctx.window_insets()` at `D:/Projects/winia/winia/src/runtime/composer/ctx.rs:95-101`. The controller
  itself is reactive at `D:/Projects/winia/winia/src/window/insets.rs:391-415`, but `WindowId` at
  `D:/Projects/winia/winia/src/app/window_state.rs:24-68` has no inset accessor.
- **Impact:** a declaring parent, controller, or callback cannot read another window's insets; only
  code already inside that window's content can reach them.
- **Proposed API:** `WindowId::insets()` returning a snapshot/controller, with ownership and lifetime
  rules for unrealized and destroyed windows.
- **Suggested verification:** update status-bar, navigation-bar, cutout, and IME insets on Android;
  assert that both content and an external observer receive the same reactive values.

### G4 - Public owner/parent relationship

- **Current evidence:** `owner` is an internal declaration-lifetime field at
  `D:/Projects/winia/winia/src/app.rs:73-75`, populated from the declaring composer at
  `D:/Projects/winia/winia/src/app/window.rs:351-364`, and used only for recursive teardown at
  `D:/Projects/winia/winia/src/app/window.rs:150-175`. Native creation at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:56-71` has no parent/owner attribute.
- **Impact:** applications cannot express native ownership, grouping, minimize/restore-with-owner, or
  a modal parent. Lifecycle ownership is not the same concept as a native parent window.
- **Proposed API:** keep lifecycle ownership explicit and add a separate public parent/modal concept
  for platforms that support it; do not overload `owner` with both meanings.
- **Suggested verification:** assert lifecycle cascade independently from native parent behavior; on
  desktop, minimize/restore and close the owner and observe the native child relationship.

### G5 - Window activation and OS focus control

- **Current evidence:** `WindowId` exposes only identity, state, capabilities, close, and destroy at
  `D:/Projects/winia/winia/src/app/window_state.rs:24-68`; the command enum at
  `D:/Projects/winia/winia/src/app/window_state.rs:251-264` has no activate/focus command.
  `FocusRequester::request_focus` at `D:/Projects/winia/winia/src/focus/requester.rs:55-84` targets a
  component inside a window, not the OS window itself.
- **Impact:** opening or revealing a utility window cannot reliably bring it forward or restore OS
  keyboard focus. Component focus and window activation are conflated in practice.
- **Proposed API:** `WindowId::activate()`/`request_focus()` with capability reporting and a separate
  observed OS-focus value; define initial-focus policy for new windows.
- **Suggested verification:** activate a background window, restore a minimized window, and confirm
  OS focus plus the previously focused component. Verify no focus stealing on Android.

### G6 - Always-on-top, transparency, and runtime decorations

- **Current evidence:** `WindowOptions` contains only `resizable` and `decorations` at
  `D:/Projects/winia/winia/src/app.rs:44-55`; `.decorations(bool)` is creation-only at
  `D:/Projects/winia/winia/src/app/window.rs:95-105`. Commands at
  `D:/Projects/winia/winia/src/app/window_state.rs:251-264` cannot change decorations and have no
  always-on-top or transparency variants; native creation at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:56-71` sets neither.
- **Impact:** utility windows, overlays, and custom chrome cannot be topmost or translucent, and
  decorations cannot be toggled after creation. This gap is partial: initial decorations exist.
- **Proposed API:** creation flags for `always_on_top` and `transparent`, plus a runtime
  `set_decorations` command where supported. Extend `WindowCapabilities` per feature/platform.
- **Suggested verification:** inspect native style flags, z-order, alpha compositing, and client-size
  changes when decorations are toggled; verify graceful no-ops where unsupported.

### G7 - Size constraints

- **Current evidence:** the builder has only a fixed `.size` at
  `D:/Projects/winia/winia/src/app/window.rs:89-93`; runtime state has only `set_size` at
  `D:/Projects/winia/winia/src/app/window_state.rs:135-151`; native creation sets one surface size at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:64-67`; the command path requests one size at
  `D:/Projects/winia/winia/src/app/app_state/handlers.rs:62-66`.
- **Impact:** dialogs and utility windows can be resized to unusable dimensions or beyond the work
  area. `resizable(false)` is too coarse for content that needs a bounded range.
- **Proposed API:** minimum/maximum inner sizes, and optionally resize increments, expressed in
  logical pixels and reconciled with DPI and placement changes.
- **Suggested verification:** attempt sizes below, within, and above the bounds through API, native
  drag, and display-scale changes; assert clamped projections and layout constraints.

### G8 - Window event callbacks/flows

- **Current evidence:** the builder registers only `on_close` and `on_close_requested` at
  `D:/Projects/winia/winia/src/app/window.rs:107-133`. Size and focus observations are internal at
  `D:/Projects/winia/winia/src/app/window_state.rs:416-424`; move, focus-loss, and scale events are
  consumed internally at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:741-769`.
- **Impact:** applications cannot react to move, resize, placement, focus, scale-factor, or inset
  changes without polling or private hooks. Geometry persistence and coordinated layouts lack an
  event boundary.
- **Proposed API:** typed window-event callbacks or observable flows, with UI-thread delivery,
  coalescing rules, and no duplicate delivery when an observation also updates reactive state.
- **Suggested verification:** record event order for programmatic and native changes, including
  rapid drag events, and assert one semantic notification per documented policy.

### G9 - Geometry persistence

- **Current evidence:** `.key` is documented as saved window state at
  `D:/Projects/winia/winia/src/app/window.rs:112-120`; finalization serializes only saveable providers
  at `D:/Projects/winia/winia/src/app/per_window/core.rs:99-108`; restoration reads only that registry
  at `D:/Projects/winia/winia/src/app/app_state/helpers.rs:129-147`; initial geometry still comes from
  the declaration at `D:/Projects/winia/winia/src/app/window.rs:321-335`.
- **Impact:** position, size, and placement are not restored across runs. A future implementation
  also needs monitor and DPI validation to avoid reopening off-screen.
- **Proposed API:** a versioned geometry record keyed by the stable persistence key, separate from
  content saveables, with monitor/work-area validation and an explicit minimized/fullscreen policy.
- **Suggested verification:** move and resize a keyed window, restart, and restore it; repeat after
  removing a monitor, changing DPI, and shrinking the work area.

### G10 - `DialogWindow`

- **Current evidence:** a repository-wide search for `DialogWindow` at baseline returned no symbol.
  The public exports provide the in-window `Dialog` overlay and ordinary `Window` at
  `D:/Projects/winia/winia/src/lib.rs:299-310`; the only declarative builder is `Window` at
  `D:/Projects/winia/winia/src/app/window.rs:46-53`. Native creation has no modal/parent attributes at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:56-71`.
- **Impact:** there is no OS-level modal dialog with parent disabling, native focus trap/restore,
  taskbar policy, or a typed result. The in-window overlay is not a substitute for those behaviors.
- **Proposed API:** a `DialogWindow` specialization with parent, modality, default placement/size,
  dismissal semantics, and a result channel; define an overlay fallback for platforms without
  independent windows.
- **Suggested verification:** open a modal dialog, attempt to interact with the parent, cancel via
  Escape/native close, return a result, close the parent, and verify focus restoration and cascade.

### G11 - Content-driven window size

- **Current evidence:** the default size is fixed at `D:/Projects/winia/winia/src/app/window.rs:63-69`
  and the builder only accepts explicit dimensions at `D:/Projects/winia/winia/src/app/window.rs:89-93`.
  Creation injects and lays out against those dimensions at
  `D:/Projects/winia/winia/src/app/app_state/helpers.rs:175-196`. Later platform resizes only update
  the window and layout at `D:/Projects/winia/winia/src/app/app_state/handlers.rs:1217-1249`; there is
  no content-measurement-to-window-size path.
- **Impact:** dialogs and popups need magic numbers and can clip content or retain excess empty
  space. Content changes cannot grow or shrink the native window.
- **Proposed API:** an explicit content-sized mode using the existing intrinsic-measurement system,
  combined with min/max constraints and a stable resize policy that prevents measurement feedback
  loops.
- **Suggested verification:** grow and shrink content, change text scale/DPI, and assert one stable
  resize per semantic change within the configured bounds.

## Repair status

Status as of 2026-11-24: the first batch (D1, D2, D7, D9, D10) is implemented and covered by targeted regressions. The remaining defects and API gaps are backlog.

### Defects

| ID | Priority | Status | First action | Validation anchor |
| --- | --- | --- | --- | --- |
| D1 | High | Fixed - first batch | Publish the latest content closure to the existing window | `an_existing_window_receives_the_latest_content_closure` |
| D2 | High | Fixed - first batch | Include pending requests in recursive owner cancellation | `closing_a_parent_cancels_its_pending_child` |
| D3 | Medium | Backlog | Make creation and pre-realization command application atomic | No wrong first frame |
| D4 | High | Backlog | Add tracked window-state snapshots/version notifications | No manual refresh tick |
| D5 | Medium | Backlog | Observe initial and subsequent outer position | Move/DPI echo |
| D6 | High | Backlog | Separate requested/observed placement and OS/content focus | Native chrome and Alt-Tab |
| D7 | High | Fixed - first batch | Treat an empty current snapshot as a clearing snapshot | `an_empty_snapshot_clears_the_previous_persisted_state` |
| D8 | Medium | Backlog | Preserve terminal-command order and define cross-window order | Recording fake window |
| D9 | Low | Fixed - first batch | Canonicalize the title before declaration and creation | `an_empty_title_is_reflected_in_window_state` |
| D10 | Medium | Fixed - first batch | Recompute parent and legacy debug target after removal | `closing_the_parent_rebinds_to_a_remaining_window` |

### API gaps

| ID | Status | Dependency/notes |
| --- | --- | --- |
| G1 Observable state | Backlog | Shared foundation with D4-D6 |
| G2 Initial position/placement/visible | Backlog | Depends on atomic creation (D3) |
| G3 Insets on `WindowId` | Backlog | Reuse the existing reactive inset controller |
| G4 Owner/parent | Backlog | Separate lifecycle ownership from native parent/modal |
| G5 Activate/focus | Backlog | Depends on OS-focus observation (D6) |
| G6 Always-on-top/transparent/decorations | Backlog | Decorations exist only at creation today |
| G7 Size constraints | Backlog | Coordinate with content-driven sizing (G11) |
| G8 Event callbacks/flows | Backlog | Coordinate with observable state (G1) |
| G9 Geometry persistence | Backlog | Depends on position/placement observation (D5-D6) |
| G10 `DialogWindow` | Backlog | Depends on parent/modal and activation APIs |
| G11 Content-driven size | Backlog | Reuse intrinsic measurement; define feedback-loop policy |

### First-batch implementation

- `WindowCell` now retains the latest content closure; initial creation and later frame boundaries consume it without changing window identity.
- Pending creations participate in declaration removal and recursive owner cancellation, including root declarations that disappear before realization. Canceled pending windows still run `on_close`; callbacks execute outside the global pending lock and the queue is rescanned for descendants they enqueue.
- Empty saveable snapshots are written as authoritative clearing snapshots.
- Empty titles are canonicalized to `Winia` before declaration, native creation, and realization.
- Parent/debug-target selection is refreshed after command close, native destruction, and pending reconciliation. Replacements are restricted to top-level windows and ordered by an independent monotonic platform-window creation sequence.

Targeted validation: `window_lifecycle_test` 12/12, `app::app_state::helpers::tests` 1/1, and `window_state` 10/10 passed.

### First-batch sequencing

1. **D1 content refresh:** land the stable content slot and same-id recomposition test first; this
   establishes how a declaration updates an already-created window without changing its lifetime.
2. **D2 pending cascade:** extend cancellation to pending requests and add the parent-before-drain
   regression; reuse the same owner-chain model introduced for content updates.
3. **D7 empty snapshot:** remove the empty-map ambiguity and cover both memory and disk reopen.
4. **D9 empty title:** introduce one canonical-title helper and route seed, native attributes, and
   realization through it.
5. **D10 parent id:** centralize parent/debug-target recomputation on every removal path.

These items should remain separate commits where practical: D1/D2 change lifetime plumbing, D7
changes persistence semantics, D9 changes initial normalization, and D10 changes routing state.

## Suggested validation plan

No build or UI test was run for this documentation-only task. Follow-up repairs should use:

- Focused state/unit coverage, for example:
  `cargo test -p winia --lib app::window_state::tests`.
- The existing window lifecycle integration test with the debug server feature:
  `cargo test -p winia --features debug-server --test window_lifecycle_test`.
- The general UI suite when window behavior is exercised through fixtures:
  `cargo test -p winia --features debug-server --test ui_test`.
- A desktop matrix (Windows first, then macOS/Linux) for native position, placement, focus, parent,
  and chrome behavior; an Android pass for capability fallbacks, insets, and the single-Activity
  multi-window model.

## Existing coverage and remaining gaps

- `D:/Projects/winia/winia/src/app/window_state.rs:458-510` covers queue order within a simple setter
  batch, deferred size, and platform size echo, but not cross-command ordering or pre-realization
  application.
- `D:/Projects/winia/winia/tests/window_lifecycle_test.rs:116-229` covers live size echo, close
  negotiation, last-window policy, destroy tombstones, realized parent/child cascade, and sibling
  rebuilds.
- `D:/Projects/winia/winia/tests/window_lifecycle_test.rs:246` covers a nonempty keyed snapshot across
  runs, but not an empty clearing snapshot.
- `D:/Projects/winia/winia/tests/ui_fixtures/window_lifecycle.rs:76-87` explicitly uses a separate
  refresh tick because window-state reads are not reactive.

Missing regressions correspond directly to D1-D10: refreshed content, pending-child cancellation,
pre-realization command timing, reactive state, position echo, placement/OS-focus echo, empty
snapshot clearing, terminal/cross-window command order, title canonicalization, and parent
reassignment.

## Related reading

- `D:/Projects/winia/docs/windowing-design.md:42-54` - the existing single-authority and command-order
  invariants.
- `D:/Projects/winia/docs/windowing-design.md:130-184` - declaration lifetime, ownership, visibility,
  and the requested window-state model.
- `D:/Projects/winia/docs/windowing-design.md:274-302` - the command queue and attribute-authority
  table.
- `D:/Projects/winia/docs/saveable.md` - saveable registry and persistence semantics.
- `D:/Projects/winia/docs/focus.md` - content focus behavior and `FocusRequester`.
- `D:/Projects/winia/docs/insets-and-pull-to-refresh.md` - inset consumers and controller behavior.

