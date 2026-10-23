# Item animation

Lazy lists and flow layouts share one item animation contract:

```rust
use winia::animation::{item::ItemAnimationSpec, SpringSpec, TweenSpec};

let spec = ItemAnimationSpec::new()
    .placement(SpringSpec::default())
    .fade_in(TweenSpec::default())
    .fade_out(TweenSpec::default());

item.modifier(Modifier::new().animate_item_with(spec));
```

`Modifier::animate_item()` is the default contract (placement spring, no fades).
The container does not need an `animate_placement` switch: a Lazy list or Flow that sees the marker
honors it.

## What moves

- **Placement**: the item keeps its last position by key; a reflow turns the difference into a paint
  offset that travels to zero with the placement curve.
- **Fade-in**: a key first seen after the host's first layout starts at alpha 0 and animates to 1.
- **Fade-out**: while a key is rendered, the renderer captures its frame as a Skia picture and
  refreshes it when the item moves or fades; after removal the owning container draws that picture
  while the fade-out curve runs. This is why removal does not depend on the layout node surviving,
  and why the ghost follows the container's scroll clip and translation.
- **Size** is not part of the item contract. Use `AnimatedSize` around the item or the container;
  Compose splits `animateItem` from `animateContentSize` for the same reason.

## Identity

The host keys state by:

- the Lazy item key from `items_from`;
- the Flow child's stable `slot_key` (use `ctx.key` / `ctx.keyed_range`).

Put the Lazy marker on the item's root content. A marker deeper in the subtree belongs to a nested
Flow/Lazy item and is not used for the outer wrapper. Without a stable key, a reorder looks like a
new item and the old one exits.

## Lifecycle limits

- A Lazy list keeps paint channels only for the composed window plus a bounded set of recently
  off-screen keys. Scrolling far away and back may therefore treat an evicted item as a new item
  (it can fade in again); this is what keeps a long virtualized list bounded.
- Off-screen items do not keep exit pictures. A picture is captured while the item is rendered and
  dropped as soon as the item leaves the composed window.
- Removing `animate_item` or passing an inert spec retires the item's channels without drawing a
  ghost: the child is still live, only its animation was removed.
- Item timing channels must reach their target. `Decay` is unsupported for placement and fades
  because it has no target value. A finite `Repeatable` is allowed only when its last iteration
  ends at the target (`Restart`, or `Reverse` with an odd count); a Spring must have a positive
  rest threshold. This is what lets an exit picture be cleaned up when its fade completes.
- A measure epoch invalidates the cached exit picture when the item is re-measured, so a ghost does
  not show stale text or a stale child. Paint-only changes that do not re-measure the item are not
  tracked by this epoch.

## Demo

```bash
cargo run -p winia --example item_animation_demo
```

The demo adds and removes cards: a new card fades in, the removed card keeps its captured frame
while it fades out, and the container height follows through `AnimatedSize`.
