# SwipeToDismissBox — implementation plan

Status: **plan written from the androidx source (`/tmp/compose-ref/SwipeToDismissBox.kt`, 406 lines, read in
full), not implemented yet.** Companion: `docs/segmented-button-plan.md` (the same thread of work).

## What it is

A row that can be swiped horizontally to dismiss it: the content translates with the finger, a caller-supplied
background is revealed underneath (an "archive"/"delete" panel that appears as you drag), and on release the
row either settles back or leaves in the swipe direction. Compose exposes three values — `StartToEnd`,
`EndToStart`, `Settled` — and the caller is expected to remove the item once `onDismiss` fires.

## API mapping (Compose → winia)

| Compose | winia |
|---|---|
| `enum SwipeToDismissBoxValue { StartToEnd, EndToStart, Settled }` | same three names (`SwipeToDismissBoxValue`) |
| `SwipeToDismissBoxState(initialValue, positionalThreshold)` | `SwipeToDismissBoxState::new(initial_value)` + `SwipeToDismissBoxDefaults::positional_threshold()` |
| `state.currentValue` / `targetValue` / `settledValue` | same (`AnchoredDraggableState` already has all three) |
| `state.progress` (0..1 toward the target) | `state.progress()` |
| `state.dismissDirection` (which way it is going) | `state.dismiss_direction()` — derived from the offset's sign, `Settled` at 0 |
| `state.requireOffset()` | `state.offset()` |
| `state.snapTo(v)` / `reset()` / `dismiss(v)` (suspend) | `state.snap_to(v)` / `state.reset()` / `state.dismiss(v)` — winia's are synchronous (an animation is registered, not awaited) |
| `rememberSwipeToDismissBoxState(...)` | `ctx.remember(\|\| SwipeToDismissBoxState::new(...))` (the repo's pattern) |
| `SwipeToDismissBox(state, backgroundContent, modifier, enableDismissFromStartToEnd, enableDismissFromEndToStart, gesturesEnabled, onDismiss, content)` | `SwipeToDismissBox::new(state).background(\|ctx\|…).on_dismiss(\|dir\|…).enable_dismiss_from_start_to_end(bool).enable_dismiss_from_end_to_start(bool).gestures_enabled(bool).build(ctx, \|ctx\|…)` |

## Tokens (from the source)

- `positionalThreshold` = **56 dp** (`SwipeToDismissBoxDefaults.positionalThreshold`) — a *distance*, not a
  fraction: how far the drag must pass for the release to land on the dismiss anchor.
- `DismissVelocityThreshold` = **125 dp** (private) — a fling past this dismisses regardless of distance.
- Anchors: `Settled` at `0`, `StartToEnd` at `+width`, `EndToStart` at `-width` — a *dismissed* row sits
  exactly one width away, which is also the animation end state the caller removes the item at.

## Implementation sketch

1. **State** (`winia/src/ui/swipe_to_dismiss.rs`): wrap `crate::ui::anchored_draggable::AnchoredDraggableState<SwipeToDismissBoxValue>`
   — it already provides `update_anchors`, `drag_delta`, `settle_with_velocity(velocity, positional_threshold)`,
   `animate_to`, `snap_to`, `progress(from, to)`, `set_velocity_threshold_dp`, `set_confirm_value_change`,
   `settled_value`, `offset_state`. The wrapper adds only the three-value vocabulary, the `dismiss_direction`
   derivation and the 56/125 dp defaults.
2. **Anchors** are set from the measured width in the layout callback (the sheet's `on_size_changed` pattern,
   or the policy that knows the size), gated by the two `enable_dismiss_from_*` flags.
3. **Gestures**: `Modifier::on_drag` feeds `1`-axis delta (`drag_delta(dx)`) and `on_drag_end` settles with
   `last_velocity()`; enabled only while `settled_value() == Settled` (Compose gates it the same way, so a
   dismissed row cannot be dragged back).
4. **Layering**: background first (fills the box), content on top, content translated by
   `absolute_offset(state.offset(), 0.0)` — **not** `offset`, which mirrors x under RTL (the sheet has the
   same note).
5. **RTL**: Compose's snippet is offset-based and effectively LTR (`StartToEnd` = `+width`). winia has a real
   layout direction, so the anchors should be direction-aware: under RTL `StartToEnd` moves the content toward
   the *start*, which is to the right, i.e. `+width` stays, but the *reading* order flips which edge is
   "start". Decision to make in implementation: keep `+width` = StartToEnd in LTR and flip under RTL, and
   cover it with a unit test (the segmented row's `item_shape` has the same shape of decision).
6. **`on_dismiss`**: fire once when the state settles in a non-`Settled` value (Compose uses a
   `LaunchedEffect(settledValue)`); winia has no effects in the build path for this, so fire it from the
   settle path (and keep it idempotent so a re-settle cannot call it twice).

## Differences to document (each needs a test or an explicit note)

1. **Axis arbitration is the real risk.** Compose's `anchoredDraggable` is orientation-aware and nests with
   scroll: a *vertical* drag inside a horizontal swipe row scrolls the list. winia's routing
   (`path_scroll_idx` / `path_drag_idx` / `inner_component_drag` in `app.rs`) is **depth-based, not
   axis-aware**: a drag node deeper than the scroll wins the gesture, so a SwipeToDismissBox inside a
   `LazyColumn`/`Column(vertical_scroll)` would swallow vertical drags (the box ignores `dy`, so the list
   would not scroll for that gesture). Options: (a) accept and document it, (b) make the routing axis-aware
   (compare the gesture's dominant axis against the scroll's axis), (c) have the box forward vertical-dominant
   gestures to the enclosing scroll. Recommendation: (b) as a small framework change, since it also affects
   sliders inside lists — but only after a fixture proves the (a) behaviour, so the change is measured rather
   than guessed.
2. **`confirmValueChange`** is deprecated in the current Compose API (it moved out of the constructor), so
   winia matches the *new* signature and leaves `AnchoredDraggableState::set_confirm_value_change` as the
   lower-level hook.
3. **No semantics** (the accessibility layer is deferred — `docs/semantics-gap.md`): Compose's row carries
   `dismissible` semantics with actions. Note the gap where the component is documented.
4. **`RowScope`**: both content lambdas are `RowScope` in Compose (the background is laid out as a row so it
   can position its own icons against each edge). winia's content closures take `ctx`; a `Row` inside the
   background closure is the equivalent, and the docs should show the pattern.
5. **Synchronous state actions**: `snapTo`/`reset`/`dismiss` are suspend functions upstream (callers often
   use them in a coroutine after `onDismiss`); winia's equivalents register the animation and return.

## Tests

- **Unit** (in `swipe_to_dismiss.rs`): anchors for the four flag combinations (both/start-only/end-only/none);
  the 56 dp positional threshold decides settle vs dismiss at a boundary just under and over it; a fling past
  125 dp/s dismisses below the distance threshold; `dismiss_direction` and `progress` as the offset moves;
  the RTL mirroring decision; `gestures_enabled`-style gating after a dismissal.
- **UI fixture** (`fixture_swipe_dismiss.rs`, scenario `swipe_dismiss`): a list of three rows whose text is
  the item's id, each swiped with `d`/`m`/`u`. Cases: a short drag settles back (the row is still there and
  the printed state is `Settled`); a long drag dismisses it and the item is removed from the list (the
  printed count drops); the direction is reported correctly for a right-to-left swipe; a row with
  `enable_dismiss_from_start_to_end(false)` refuses that direction but still dismisses the other way.
- The known-difference case (a vertical drag on a row inside a scrolling list) belongs in the fixture only
  after difference 1 is decided.

## Open questions for the reviewer

1. Difference 1: accept the axis limitation for now, or fix the routing first (a framework change with its own
   test)?
2. Should the background be a required argument (Compose requires it) or optional with a default
   (a plain surface)? Compose requires it; requiring it here would force a `background` closure in the demo
   even when unused.
3. Is the component worth a `Scaffold`-style convenience later (a `SwipeToDismissList`),
   or is the single-row box enough?
