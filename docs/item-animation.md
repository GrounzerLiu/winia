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
- **Fade-out**: before a key leaves the tree, the renderer captures its last frame as a Skia
  picture; the exit layer draws that picture while the fade-out curve runs. This is why removal does
  not depend on the layout node surviving.
- **Size** is not part of the item contract. Use `AnimatedSize` around the item or the container;
  Compose splits `animateItem` from `animateContentSize` for the same reason.

## Identity

The host keys state by:

- the Lazy item key from `items_from`;
- the Flow child's stable `slot_key` (use `ctx.key` / `ctx.keyed_range`).

Without a stable key, a reorder looks like a new item and the old one exits.

## Demo

```bash
cargo run -p winia --example item_animation_demo
```

The demo adds and removes cards: a new card fades in, the removed card keeps its captured frame
while it fades out, and the container height follows through `AnimatedSize`.
