# Scroll extras: clipping, overscroll and item animation

Three Compose 1.7/1.8 additions around a scrolling container.

## `Modifier::clip_scrollable_container(orientation)`

Compose's `clipScrollableContainer` clips a scroll container on its SCROLL axis while leaving
`MAX_SUPPORTED_ELEVATION` (30 dp) of room on the cross axis, so a shadow drawn there is not cut off.
winia already clips every scroll viewport exactly at its bounds; this modifier widens that clip by
30 dp on the cross axis only (`render_pass1`'s scroll block). The main axis stays exact, so scrolled
content still cannot paint over a sibling.

## `Modifier::overscroll(effect)`

`OverscrollEffect` (hoist it with `remember`) turns the delta a container cannot consume into a
rubber-band:

- `apply_scroll_delta` computes the unclamped target and the leftover past the start/end, feeds the
  leftover into `OverscrollEffect::pull`, and counts it as CONSUMED — so the gesture does not fall
  through to an ancestor scroll container (`app/scroll.rs`);
- the renderer translates the content by the accumulated pull along the scroll axis;
- every pull re-arms a 60 ms-delayed 220 ms tween back to 0, so releasing returns the content and
  rapid input keeps the tension.

Deliberate scope: the content SHIFTS past the edge (there is no content scaling and no platform
glow), the pull is capped at 120 logical px, and containers with `reverseLayout` skip the effect
(their offsets are mirrored, so a signed pull would point the wrong way).

## `Modifier::animate_item()`

Place this on a lazy item's root. The lazy list keys each item (`items_from`'s key function) and
remembers, per key, the main-axis slot it occupied on the previous frame. When the data reorders and
the key moves, the item is painted at its OLD slot and springs to the new one; the item's wrapper
node carries the animation, so the whole item moves. An item that was not placed on the previous
frame resets instead of springing from a stale slot.

Deliberate scope: PLACEMENT only. Compose's `animateItem` also fades items in and out; winia has no
exit-node mechanism for a removed lazy item, so appearance/disappearance are not animated. The marker
is inert outside a lazy list.
