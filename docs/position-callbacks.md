# Position callbacks and bring-into-view

`Modifier::on_globally_positioned`, `Modifier::on_placed`, `Modifier::layout_id` and
`Modifier::bring_into_view_requester` — Compose's `androidx.compose.ui.layout` observation APIs in
winia's terms. Sources: `androidx/compose/ui/layout/OnGloballyPositionedModifier.kt`,
`OnPlacedModifier.kt`, `LayoutId.kt` (not part of the local `reference/` checkout, which carries only
foundation/material modules), and `foundation/relocation/BringIntoViewRequester.kt`.

## What the callback receives

`GlobalPosition { x, y, width, height }` — window coordinates in logical pixels. `x`/`y` include the
scroll offsets of every scrolling ancestor (the same arithmetic as the renderer's `translate(-offset)`
and `transition::abs_rect_upward`). Compose hands a `LayoutCoordinates`; winia exposes the placed box
plus `position()`, `size()`, `right()`, `bottom()`, `rect()`. There is no coordinate-tree navigation
(parent/child lookup) in this version.

## When it fires

Both callbacks are CHANGE-gated: they fire when the placed box differs from the last one reported for
that node, or on its first placement. winia has no separate placement pass — placement happens inside
measure, and a folded node is not re-placed — so "moved or resized" is the observable event. In one
pass the `on_placed` callbacks run first, then the `on_globally_positioned` ones, matching Compose's
placement-then-global order.

The dispatch runs at the end of `Composer::layout`, AFTER the dependency frame and the layout
transaction commit: a callback that writes a `State` schedules the next frame instead of being
recorded as a layout dependency. A node that carries no callbacks forgets its last box, so attaching
a callback later fires on the next placement even when the box never moved.

## `layout_id`

`Modifier::layout_id("field")` tags the chain. `Composer::find_by_layout_id("field")` returns the
node's arena index, and the debug tree exposes the tag as `"layoutId"` next to `"tag"`.

## `bringIntoViewRequester`

```ignore
let requester = BringIntoViewRequester::new();
let r = requester.clone();
Button::new()
    .on_click(move || r.bring_into_view())
    .build(ctx, |ctx| Text::new("Reveal").build(ctx));

Column::new()
    .modifier(Modifier::new().bring_into_view_requester(&requester))
    .build(ctx, |ctx| { /* ... */ });
```

A request is queued and served by the post-layout pass; `bring_into_view_rect(x, y, w, h)` asks for a
node-local sub-rectangle instead of the whole node. Requests animate the nearest scroll offsets with
the default spring.

Deliberate scope for this version:

- served by plain `Modifier::vertical_scroll` / `horizontal_scroll` ancestors;
- `reverseLayout` and RTL-mirrored horizontal scrolling are NOT compensated — such an ancestor is
  skipped rather than scrolled the wrong way;
- a lazy list can only reveal nodes that are composed (an off-screen item has no requester to call).

Compose's responder chain (`BringIntoViewResponder`, `BringIntoViewSpec`, the priority queue) is not
ported: there is one responder kind (a scroll container) and one animation (the default spring).
