# SwipeToDismissBox

A row that can be swiped horizontally to dismiss it: the content translates with the finger, a
caller-supplied background is revealed underneath, and on release the row either settles back or leaves
in the swipe direction. `on_dismiss` reports the direction it left in and the caller removes the item.

Source: `winia/src/ui/swipe_to_dismiss.rs`. Demo: `cargo run -p winia --example swipe_to_dismiss_demo`.
Modeled on Compose Material 3 `SwipeToDismissBox`: the tokens and the callback order below come from
reading the androidx source (not part of this checkout), and where winia deviates it is called out below.

## API

| Compose | winia |
|---|---|
| `enum SwipeToDismissBoxValue { StartToEnd, EndToStart, Settled }` | the same three names (`SwipeToDismissBoxValue`) |
| `SwipeToDismissBoxState(initialValue, …)` | `SwipeToDismissBoxState::new(initial_value)` |
| `rememberSwipeToDismissBoxState(...)` | not needed: the box remembers its own state unless one is passed with `.state(...)` |
| `state.currentValue` / `targetValue` / `settledValue` | `current_value()` / `target_value()` / `settled_value()` |
| `state.offset` / `requireOffset()` | `offset()` (NaN before the first layout) |
| `state.dismissDirection` | `dismiss_direction()` — the offset's sign, `Settled` at rest |
| `state.progress` | `progress()` — see the difference list below |
| `state.snapTo(v)` / `reset()` / `dismiss(v)` (suspend) | `snap_to(v)` / `reset()` / `dismiss(v)` — synchronous: they register the animation and return |
| `state.settle(v)` (internal) | `settle()` / `settle_with_velocity(v)` |
| `SwipeToDismissBox(state, backgroundContent, modifier, enableDismissFromStartToEnd, enableDismissFromEndToStart, gesturesEnabled, onDismiss, content)` | `SwipeToDismissBox::new().state(…).background(…).modifier(…).enable_dismiss_from_start_to_end(b).enable_dismiss_from_end_to_start(b).gestures_enabled(b).on_dismiss(…).build(ctx, content)` |
| `RowScope` for both content lambdas | plain `ctx` closures; put a `Row` inside the background closure for edge-aligned icons |

Tokens (from the Compose source):

- positional threshold **56 px** (`SWIPE_DISMISS_POSITIONAL_THRESHOLD`) — a DISTANCE, not a fraction: at
  the default half-the-distance rule a wide row would need a very long drag;
- fling threshold **125 px/s** (`SWIPE_DISMISS_VELOCITY_THRESHOLD`) — a flick past it dismisses
  regardless of distance;
- anchors: `Settled` at `0`, `StartToEnd` at `+width`, `EndToStart` at `-width` — a dismissed row sits
  exactly one width away, which is the state the caller removes it in.

## Owning the state (do this in a list)

The box remembers a state of its own when none is passed, and in a keyed list that works (measured:
dismiss a row, scroll the list, scroll back, dismiss the row that is now on top — both dismissals land).
Pass a state you own when you want to read `progress()` for a reveal panel, drive `dismiss(v)`, or keep
the row's identity explicit; the fixture (`fixture_swipe_dismiss.rs`) and the demo both create it in the
item's own closure.

Two rebuilds it does not survive, both measured:

- **A rebuild that is not keyed** (a plain `for` loop over the data) gives the arriving item the
  remembered state of the list POSITION the dismissed row left, so it comes up parked at the dismiss
  anchor — content slid out of the row's box and gestures gated off. That is the shape the demo had
  before it was keyed: dismissing one row left the two rows above it showing nothing but their
  background. `LazyColumn::items_from` (or any keyed container) fixes it.
- **A reorder does not carry state to the moved item.** Measured: dismiss a row (without removing the
  item), then move that item to the end of the data — every position comes back `off 0` / `Settled`, so
  the state was re-created rather than following its item. `LazyColumn` items are keyed by (position,
  item key), which stabilizes the scroll anchor and stops the state leaking to the wrong row, but an item
  that changes position gets a fresh state. A caller who needs state to follow an item across reorders
  should keep it in its own data, keyed by the item.

```rust
LazyColumn::new()
    .items_from(items, |m| m.id, move |ctx, _i, m| {
        let state = ctx.remember(|| SwipeToDismissBoxState::default()).get();
        SwipeToDismissBox::new().state(state)
            .on_dismiss(move |dir| { /* drop the item from the data */ })
            .build(ctx, |ctx| { /* row content */ });
    });
```

**A row's state does not survive scrolling out of the composed window** (the viewport plus
`LAZY_BEYOND_BOUNDS` prefetch). Measured with a row parked at `-360` (dismissed, item deliberately not
removed): scrolled to the end of the list and back, the row returns at `off 0` / `Settled`. That matches
Compose, whose `remember` inside a lazy item is likewise discarded (that is what `rememberSaveable` is
for), and it has one consequence worth knowing: the arrival check that calls `on_dismiss` lives in the
item, so a dismissal whose animation is still running when the row scrolls out of the window never
reports — the caller sees the row come back. Keep the state in the caller's own data (not in the item)
if it has to outlive the window, and treat a dismissal as final before the row can leave the screen.

**Known limit — a keyed list does not carry a row's state across a REORDER.** Measured: dismiss a row
(without removing the item) and then move that item to the end of the data; every position comes back
`off 0` / `Settled`, i.e. the state was re-created rather than following its item. The lazy list's item
key currently stabilizes the *scroll anchor* (and stops a rebuild from handing the next item the departed
row's state), but per-item composition state is not retained when items move. A caller who needs that
should keep the per-item state in its own data (a keyed map outside the item), not in the item's
composition scope.

## Gesture arbitration (the framework part)

A press inside a row is claimed by two owners at once: the row's `on_drag` and the scroll container it
sits in. Which one keeps the gesture is decided by the finger's **dominant axis**, once, on the first
move that is decisive (`app::gesture_move`, `input::gesture::ScrollAxis`):

- **horizontal** → the row drags (the scroll session is never opened);
- **vertical** → the list scrolls; the row's drag is cancelled *before it starts*, so the row receives no
  `on_drag`/`on_drag_start`/`on_drag_end` at all for that gesture;
- below the 8 px touch slop, or on an exact diagonal, **neither** owner starts — the next move decides.

The decision is never revisited mid-gesture: handing over later would mean replaying the deltas the row
already consumed. Compose arbitrates in the same place and the same way — its drag detectors each wait for
the touch slop along their own orientation.

This also changes the neighbouring components for the better: a `Slider` inside a list used to swallow a
vertical drag (it is deeper than the scroll, so it won the depth-based routing) and do nothing with it;
now the list scrolls. The improvement covers a press that lands on a plain tap target inside that drag
component too (an `on_press` node such as a `TextField` inside a `Slider`): the arbitration is armed by
the *hit path* containing an inner drag, not by the press target having drag callbacks of its own.

**Known limits.** Two, both called out so the next person knows where to look:

- **Overlays.** The arbitration is main-tree only. Inside a popup the drag runs through `overlay_drag`
  (its own session, which fires the callbacks itself) rather than the gesture tracker, so a swipe row
  inside a popup list does not hand vertical drags to that list. The row still locks its own axis (see
  below), so a mostly-vertical swipe through a popup row will not dismiss it, but the popup's list will
  not scroll either. Nothing shipped uses that combination yet.
- **A horizontal scroll ancestor.** The row is the deeper node, so a horizontal drag over a row inside a
  `LazyRow` belongs to the row and that list cannot be scrolled over a row. Compose settles the same
  competition the same way (the inner detector wins the touch slop); it is a limit of nesting a
  horizontal drag inside a horizontal scroll, not of the arbitration.

The row also locks its own axis, which is the half of the arbitration a component can do alone: from the
drag's start it accumulates travel and ignores deltas until they lead sideways past the touch slop (then
stays locked). Without it a row with no scroll ancestor — or one in an overlay — would be dismissed by
the lateral drift of a mostly-vertical swipe, and a flick's `dx` tail could satisfy the velocity
threshold.

## Differences from Compose (deliberate)

1. **No semantics.** Compose's row carries `dismissible` semantics and actions for assistive technology;
   winia's semantics layer is deferred (`docs/semantics-gap.md`).
2. **Synchronous state actions.** `snapTo` / `reset` / `dismiss` are suspend functions upstream (callers
   usually await them in a coroutine after `onDismiss`); winia registers the animation and returns.
3. **Anchors are physical, not direction-relative.** `StartToEnd` is `+width` and `EndToStart` is
   `-width` under both layout directions. Compose's implementation is offset-driven and also never
   inspects the layout direction, so a direction-aware flip here would be a new rule of winia's rather
   than an alignment — it needs its own test before it is worth having.
4. **`progress()` is a displacement.** Compose documents `progress` as the fraction from `currentValue` to
   `targetValue`, but its arithmetic divides by zero whenever the two agree (which is most of the time,
   and always once the row is parked). winia reports the displacement towards the dismiss anchor instead:
   0 at rest, 1 at the anchor, well defined in every state — which is what a background panel driven by
   this number wants.
5. **`on_dismiss` fires on ARRIVAL, not at the start of the settle.** The caller removes the row in this
   callback, so firing when the settle *begins* would cut the slide-out short. winia waits for the offset
   to reach the dismiss anchor — judged by the DISTANCE (within 4 px, `ARRIVAL_EPSILON`), not by "the
   animation has stopped". Both halves of that are measured: the animation table can keep reporting a
   finished tween as running, and a row parked exactly on its anchor with `is_animation_running()` still
   true never reported; and firing exactly ON the anchor paints one frame of the row with its content
   already slid out, because the caller's rebuild is composed in a later pass — a fully revealed
   background flashing for a frame (reported by eye in the demo). The 4 px is the settle tween's tail,
   enough for the rebuild to land on the frame the content clears the row. Re-parking is the other half:
   if the row is off its anchor and nothing is animating (a resize landed while the tween ran, and
   `update_anchors` leaves a running animation alone), the check puts it back on the anchor.
6. **The background is optional.** Compose requires it; here a row with nothing to reveal (a plain delete
   row) can omit it.
7. **`confirmValueChange` is a lower-level hook.** It is deprecated on Compose's constructor; winia's
   `SwipeToDismissBoxState::anchored_draggable()` exposes `set_confirm_value_change` for callers who need
   the veto.

## Tests

- **Unit** (`winia/src/ui/swipe_to_dismiss.rs`): anchors for the four direction-flag combinations; the
  56 px threshold at a boundary just under and just over it; a fling below the distance threshold; the
  direction and progress derivation; a resize keeping a parked row on its anchor; a row parked in a
  direction that is switched off coming back to `Settled`.
- **Routing** (`winia/src/input/gesture.rs`): axis classification (slop, dominance, exact diagonal); a
  drag held back by the arbitration still starting later; a cancelled drag never re-arming.
- **UI** (`winia/tests/ui_test.rs`, fixture `fixture_swipe_dismiss.rs`): a vertical drag over a row
  scrolls the list and dismisses nothing; a long horizontal drag dismisses and reports the direction; a
  short slow drag springs back; a row with a direction switched off refuses it while the other direction
  still dismisses; and after a dismissal the row that moves up into the freed slot is live — draggable,
  with its content back at the row's own edge (a bounds assertion, because "parked" is a geometry
  symptom), not carrying the departed row's state. That last row also uses the box's BUILT-IN state
  (no `state(...)`), so the default path is exercised too.
