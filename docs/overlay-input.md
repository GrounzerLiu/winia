# Popup input routing

How a pointer event reaches the content of a `Dialog`, `Popup`, `DropdownMenu`, bottom sheet or an
expanded `SearchBar` — and where that differs from the main tree. The two paths are separate
implementations in `app.rs`, so the differences are easy to trip over.

## The two pointer paths

Main tree (`handle_pointer_down` → `handle_pointer_up`):

1. `press_interaction_down` — ripple `PressInteraction.Press` on the innermost clickable that has an
   interaction source (`app.rs:3354`).
2. `gesture_down` (`app.rs:2477`) — picks the press-gesture target and **fires
   `GestureAction::Press`** immediately; also creates the `GestureTracker` that routes move/up.
3. ... on release: `gesture_up` fires `Tap` / `DoubleTap` / `LongPress` / `DragEnd`, then
   `fire_click_along_path` fires the first `on_click` on the path.

Popup (overlay) — `overlay_down` (`app.rs:3093`) → `exec_overlay_click` (`app.rs:3058`):

1. Drag-vs-scroll arbitration, which stores `pw.overlay_drag` / `pw.overlay_drag_scroll`.
2. The same ripple press as the main tree.
3. **The same press gesture**: `press_gesture_target` (`app.rs:2365`) is shared with `gesture_down`,
   so a popup's `on_press` runs on pointer-down exactly as it does in the main tree.
4. Pointer `Down` dispatch (`dispatch_ptr_event`) and tap-to-place caret.
5. ... on release: `fire_click_along_path` fires the first `on_click` on the path.

`press_gesture_target` skips a drag gesture that is an ancestor of a scroll container (Compose's
"content scrolling wins"), so a bottom sheet's panel `on_drag` does not engage while its inner list
is being dragged; an inner gesture component still wins.

## An outside press: dismiss, or consume, or fall through

A press that hits no overlay runs the tail of `overlay_down` (from the top overlay down):

| Overlay | Outside press |
|---|---|
| `dismiss_on_outside` (the default for `Popup`, `Dialog`, `AlertDialog`, `ModalBottomSheet`, `DropdownMenu`, `Tooltip`) | dismisses it (`begin_overlay_close`, synchronously — not waiting for a recompose) and is consumed (unless `click_passthrough`, below) |
| `modal` with `dismiss_on_outside(false)` | nothing closes, but the press is still consumed: a modal scrim blocks what is behind it |
| neither | falls through to the main tree |
| `click_passthrough` (tooltip) | the press always falls through, whether or not the overlay dismisses |
| already `closing` | treated as transparent — no second dismissal, no consumption. So a press arriving during the exit animation (≈200 ms) reaches what is behind it, which is the point of the rule: the fading overlay is not blocking anything. Without it, a dialog closed a moment ago swallows the next press wherever it lands — the "click twice to open" complaint |

`dismiss_on_outside(false)` is how a dialog or popup that must stay open while the rest of the
window is used keeps working (Nav3's `dialogProperties`, `AlertDialog`'s builder); the flag has to
be honoured for a modal overlay too, since `modal` means "blocks what is behind", not "closes on any
press".

## Focus: nobody sets it on a tap

Neither path focuses anything on a tap. A component that wants the keyboard asks for it in its own
`on_press`:

- `TextField`'s container: `.on_press(move |_| fr.request_focus())` (`text_field.rs:1698/1709`).
- Buttons and the rest (`clickable_with_source`, `modifier.rs:1496`): focusable, so they join Tab
  navigation, but they **never** call `request_focus` — the same rule as Compose's `Clickable`, which
  delegates a `FocusableNode` and never requests focus for a click. (Bare `Modifier::clickable` is
  not focusable at all; the `Focusable` element comes from the `_with_source` variant.)

`FocusRequester::request_focus()` queues a request; it is applied on the next frame before compose
(`app.rs:1285`), resolving against the main tree first and then the overlays topmost-first, keeping a
single focus and setting the IME accordingly. The press itself does not schedule that frame, and the
main tree does not either: a press that changes nothing (no state, no animation) can leave focus
queued until whatever schedules the next frame. Tapping a field normally changes the caret or the
selection, which wakes the app, so the focus lands on the following frame.

History worth knowing: the popup path used to fire only `on_click` (on release), so a popup's
`on_press` never ran and a popup `TextField` could not be focused by tapping. `overlay_down` carried
a focus heuristic to compensate — the deepest focusable node whose subtree wants the IME — which made
every tap on a popup button steal the keyboard from a field beside it. Dispatching the press gesture
removed the need for the rule (see `clicking_an_overlay_button_does_not_steal_focus` and
`an_overlay_press_zone_receives_the_press_gesture` in `tests/ui_test.rs`).

## Which gesture fires where

Popup content gets the same gesture set as the main tree, from the same primitives:

| Callback | Main tree | Popup |
|---|---|---|
| `on_press` | `gesture_down` | `overlay_down`, both through `press_gesture_target` |
| `on_tap` / `on_double_tap` / `on_long_press` | the shared `GestureTracker` | the same tracker, tagged with the popup's arena (`PerWindow::gesture_arena`) so `gesture_move` / `gesture_up` route the action back into the popup's composer |
| `on_drag_start` / `on_drag` / `on_drag_end` | the tracker | the same tracker; the overlay drag session (`pw.overlay_drag`) is stood down for a node the tracker owns, so one gesture never dispatches these twice. It still drives nested scroll in a sheet, which belongs to a different node (the scroll container the drag is an ancestor of) |
| `on_click` | `detect_click` on release | `exec_overlay_click` on release |

A tap that is deferred to the double-tap window carries its arena in `PendingTap::overlay_id`, so it
is re-fired into the popup it came from — and dropped if that popup is gone by then, like a gesture
still in flight.

An overlay that MOVES during a gesture: the arena conversion has to use the origin captured at press
time (`PerWindow::gesture_arena_origin`, and `overlay_drag_origin` for the drag session), not the live
`screen_pos`. With the live origin the overlay's own motion lands in the measured displacement — a
stationary finger then looks like a drag past the 8 px tap slop, and the tap family is cancelled
(`a_tap_survives_its_own_popup_moving`, whose popup jumps 300 px on press). The expanded `SearchBar` is
the real case: an anchored panel sliding to the window corner for `SEARCH_BAR_EXPAND_MS`, pressable
while it moves. Deltas are unaffected either way — a difference cancels a constant origin.

A node that is BOTH tappable and draggable (a `Slider`, a custom draggable card) gets its whole
gesture set in a popup as well: the tracker owns the node and the same-node overlay drag session is
stood down, because both dispatchers would otherwise fire its `on_drag_*` for one gesture
(`a_popup_drag_target_also_taps_once` pins one start, one end, and no tap from a moved gesture).

Handing that node to the tracker changes one detail of its drag. The overlay session used to send
`DragStart` and, in the same event, a `DragMove` carrying the whole displacement from the press — so
a delta-accumulating component moved from the first over-slop event. The tracker starts the drag AT
the slop crossing and measures the first delta from there, exactly as the main tree already did, so
such a component (a sheet panel mounted with `on_drag`) moves from the crossing onward and lags by up
to the 8 px slop. A `pos`-driven component (`Slider`) is unaffected either way, and the delta rule is
now the same in both trees.

The drag half of that path has its own coordinate rule worth knowing: its calls pass
**layer-local** positions, because the callback contract is node-local and
`fire_gesture_action` subtracts a position from the arena it was handed (a popup's arena is
layer-local). Passing window coordinates there shifts a popup `on_drag`'s `pos` by the popup's
screen origin — `Slider` reads `pos.0`, so a drag in a popup landed on the wrong value until
`a_drag_inside_a_popup_reaches_the_same_value_as_in_the_main_tree` caught it. Deltas need no
conversion; the layer offset cancels in a difference.
