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

`Modifier::animate_item()` is Compose's default contract: **placement, fade-in and fade-out all on**,
each a spring at `StiffnessMediumLow` (400 — winia spells that constant
`SpringSpec::STIFFNESS_MEDIUM`, the same note `animated_size.rs` carries), damping 1.0, threshold
0.01. A channel is switched off the way Compose's `null` argument does it:

```rust
ItemAnimationSpec::new().without_fade_in().without_fade_out()   // placement only
```

The container does not need an `animate_placement` switch: a Lazy list or Flow that sees the marker
honors it. Because fade-out is on by default, a marked item also has its frame captured every frame
it is rendered — that is the cost of a removal that survives the node (see Lifecycle limits).

## What moves

- **Placement**: the item keeps its last position by key; a reflow turns the difference into a paint
  offset that travels to zero with the placement curve. Only a TARGET that MOVED starts a new
  animation: a scroll re-lays out on every frame, and treating the in-flight offset as movement
  restarts the spring each frame with zero velocity, so the item crawls towards its slot instead of
  settling (measured on an item 48 dp out of place: ten 16 ms frames of scrolling leave it at 7.7 dp
  now, and the restart rule visibly froze the same case).

  Moving the target again RE-ANCHORS the curve from the item's current paint position: the running
  motion is cancelled first, so its velocity is not carried into the new one. A placement that is
  re-targeted several times before it settles (a drag-reorder, a pinned sticky header) therefore reads
  as stepped rather than continuous. Compose re-aims the running motion instead; this is a known
  difference.

  A data change that moves the VIEWPORT with the content does not animate: inserting a row at the head
  while the list is SCROLLED pushes every row down and the anchor correction scrolls the list by the
  same amount, so nothing moves on screen. The list hands that offset change to the host, which rebases
  every item's remembered position by it (Compose does the same with `applyScrollOffset`). Without it
  the correction and the animation compensate the same shift twice and every row flashes one row up
  before springing back.

  At the very TOP there is no reading position to preserve, so the correction is skipped there: the
  viewport stays at 0, the new row is visible, and the rows below animate down into their new slots.
  Compose anchors by key in that case too and silently scrolls the list past the new row; this is a
  deliberate deviation (recorded here, not a gap).

  One placement is NOT scroll-invariant and therefore does not fit this rule: a PINNED sticky header
  is re-placed every frame by the scroll offset itself. Give a sticky header no marker — a marker
  there re-anchors its paint offset on every scroll frame, which reads as the header riding along
  with the content. (Known limitation, same behaviour as before this rule existed.)
- **Fade-in**: a key that is new to the container's DATA starts at alpha 0 and animates to 1. A key
  that is merely new to the composed WINDOW is not an appearance: scrolling a Lazy list composes keys
  that have been in the data all along, and fading those in makes a scroll look like a strobe
  (measured: a jump to offset 4000 composed keys 35..39 at alpha 0). The list decides per key — it
  keeps the set of keys it has seen, queues the difference, and the policy consumes one key per place,
  so a fade starts exactly once even when a frame takes several compose passes. A container that
  composes every child (a Flow) has no window to scroll through, so every new key there is a data
  change. The FIRST layout never fades: the list's initial content appears at once.
- **Fade-out**: while a key is rendered, the renderer captures its frame as a Skia picture and
  refreshes it when the item's CONTENT or paint epoch changes (a still item reuses the picture, it is
  not re-recorded every frame); after removal the owning container draws that picture while the
  fade-out curve runs. This is why removal does not depend on the layout node surviving,
  and why the ghost follows the container's scroll clip and translation.
- **Size** is not part of the item contract. Use `AnimatedSize` around the item or the container;
  Compose splits `animateItem` from `animateContentSize` for the same reason.

## Identity

The host keys state by:

- the Lazy item key from `items_from`;
- the Flow child's stable `slot_key` (use `ctx.key` / `ctx.keyed_range`). A Flow composes every child,
  so an unseen key there is always a data change — with one consequence worth knowing: a child that
  returns from the CROSS axis clip (Flow's `overflow` clipping) is retired while it is clipped and
  therefore fades in when it comes back, even though its key never left the data.

Put the Lazy marker on the item's root content. A marker deeper in the subtree belongs to a nested
Flow/Lazy item and is not used for the outer wrapper. Without a stable key, a reorder looks like a
new item and the old one exits.

## Lifecycle limits

- A Lazy list keeps paint channels only for the composed window plus a bounded set of recently
  off-screen keys (256). An item that was merely scrolled out and back is still in the data, so it
  comes back at full alpha — no fade, no ghost. Only a key that left the DATA and comes back fades.
- Off-screen items do not keep exit pictures. A picture is captured while the item is rendered and
  dropped as soon as the item leaves the composed window.
- Removing `animate_item` or passing an inert spec retires the item's channels without drawing a
  ghost: the child is still live, only its animation was removed.
- Item timing channels must reach their target. `Decay` is unsupported for placement and fades
  because it has no target value. A finite `Repeatable` is allowed only when its last iteration
  ends at the target (`Restart`, or `Reverse` with an odd count); a Spring must have a positive
  rest threshold. This is what lets an exit picture be cleaned up when its fade completes.
- A measure epoch and the draw-layer visual epoch invalidate the cached exit picture, so a ghost
  does not show stale text, a stale child, or a stale `Visual<T>` paint value. A custom draw node
  that reads state without routing it through `Visual<T>` is not tracked by the epoch.
- While an outer item's exit picture is being recorded, nested item paint channels are suppressed;
  a nested Flow/Lazy animation is therefore captured at its layout position. This is a rare visual
  fidelity limitation, not a functional bug.

## Demo

```bash
cargo run -p winia --example item_animation_demo        # a plain column: add / tap to remove
cargo run -p winia --example lazy_item_animation_demo   # a LazyColumn: insert, shuffle, remove
```

The first demo adds and removes cards on a plain column: a new card fades in, the removed card
keeps its captured frame while it fades out, and the container height follows through
`AnimatedSize`.

The second is the VIRTUALIZED case (`tests/ui_lazy_item_animation_test.rs`, fixture scenario
`lazy_item_animation`): insert at the head or in the middle, shuffle (same keys, new slots — pure
placement), remove from the middle or by tapping a card, and cycle the contract between Compose's
default, a placement-only one (`without_fade_in().without_fade_out()`) and a slower custom one, so
the three channels are visible side by side. It also exercises the window: the list is longer than
the viewport, so items leave and re-enter the composed range while the animation state follows the
key.

## Input during a placement animation

Placement is a paint offset, not a layout move. Hit testing and pointer input use the item's final
slot while it is still visually travelling; this matches the graphics-layer placement model rather
than making the hit target follow the painted offset. Tests and components should not assume the
visual box is interactive during the animation.
