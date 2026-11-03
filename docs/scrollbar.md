# Scrollbar

Desktop scrollbars, aligned with Compose Multiplatform's `VerticalScrollbar` / `HorizontalScrollbar`
and the M3 `Modifier.nonInteractiveScrollbar` geometry.

## 1. Sources

| What | Where |
| --- | --- |
| `VerticalScrollbar` / `HorizontalScrollbar` | Compose Multiplatform desktop `Scrollbar.kt` (androidx-main; not vendored in `reference/compose`, so the geometry formula below is the contract) |
| Thumb geometry | M3 `nonInteractiveScrollbar`: `thumb = clamp(track * viewport / content, min, track * 0.9)`, `thumbOffset = scroll / max * (track - thumb)` |
| Implementation | `winia/src/components/scrollbar.rs` (`ScrollbarNode`, one shared `scrollbar_build_shared`) |
| Demo | `winia/examples/scrollbar_demo.rs` (seven cards; every behaviour below is one of them) |

## 2. Variants

| Constructor | Axis | State | When |
| --- | --- | --- | --- |
| `VerticalScrollbar::new(ScrollState)` | vertical | `ScrollState` | A `vertical_scroll` container, level with it in a `Row` |
| `HorizontalScrollbar::new(ScrollState)` | horizontal | `ScrollState` | A `horizontal_scroll` container, below it |
| `LazyScrollbar::new(LazyListState)` | vertical | `LazyListState` | A `LazyColumn`; reads the list's own offset and limit |
| `HorizontalLazyScrollbar::new(LazyListState)` | horizontal | `LazyListState` | A `LazyRow` |

All four take the same appearance options: `modifier`, `thickness` (6 dp), `thumb_color` (theme outline at
70%), `track_color` (transparent), `thumb_min_length` (24 dp), `always_show` (false), `hover_color`,
`hover_thickness` (10 dp), and `scroll_reverse` (`None` = not reversed).

## 3. Layout

Three inputs decide everything: `offset` (from the state, read with a subscription), `viewport` (the
bar's own constraint, written back through `on_size_changed`; 0 on the first frame, which draws
nothing until the second), and `content = fling_limit + viewport`.

- `track = viewport - 2 * inset` (inset 2 dp, symmetric).
- The thumb is not drawn when `content <= viewport` or when `track < thumb_min_length`.
- The bar's layout width is `thickness`; a hovered thumb grows to `hover_thickness` and overhangs its
  slot by `(hover_thickness - thickness) / 2` on each side, so hovering never reflows the content.
- RTL: the bar is direction-neutral (symmetric inset, no left/right assumption). Placing it last in a
  `Row`/`Stack` puts it on the end edge, which mirrors to the left under RTL. Test:
  `a_bar_placed_last_sits_at_the_end_edge_in_both_directions`.

## 4. Behaviour

- **Drag**: `on_drag_start` remembers the grab offset (press point inside the track minus the thumb's
  top, clamped to `[0, thumb]`); `on_drag` maps the pointer back to an offset. All geometry is read
  LIVE (`viewport`, `fling_limit`, `offset`) - a build-time snapshot is one frame stale and made a
  50 px drag write the whole range (measured 200/200).
- **Track press**: a press outside the thumb centres the thumb under the pointer and keeps dragging
  from there (WinUI/macOS behaviour). It runs on the PRESS, not on drag start, so a plain click jumps
  without a drag.
- **Preemption**: the bar's drag IS the scroll axis, so it declares
  `Modifier::drag_outranks_scroll()`; without it the page's scroll container takes every vertical move
  and the thumb never moves.
- **Fade**: `always_show`, scrolling (an offset pulse or a boundary pulse), hover, drag, or a recent
  offset change keep it visible; otherwise a 400 ms delay and a 250 ms tween hide it. The fade value
  lives as a `State<f32>` read with `peek` in the paint pass, so the animation never recomposes.
- **Hover**: hovering or dragging tints the thumb (`hover_color`, default: the outline token at full
  alpha) and grows it (`hover_thickness`, 150 ms).
- **Reverse**: `scroll_reverse(Some(true))` matches a reversed container (`vertical_scroll_reverse`,
  `horizontal_scroll_reverse`, `reverse_layout`); geometry and drag use `visual = max - scroll`.
- **Unmeasured state**: a fresh `ScrollState` reports `f32::MAX` as its limit; the bar treats a
  non-finite or `f32::MAX` limit as "not measured yet" and writes nothing (a press used to write
  ~2.6e38 and the content vanished).

## 5. Differences from Compose

- **Track press jumps** - WinUI/macOS have it, CMP has no track handling at all.
- **Hover thickening** - WinUI/macOS grow the bar; CMP only recolours it.
- **No 48 dp touch slot** - the bar is a pointer control (6-10 dp wide); a 48 dp hit strip would cover
  the content it scrolls. Mouse-first, like the desktop reference.
- **Geometry is part of the node key** - a scroll re-enters the group (v1 semantics; the optimisation
  path is a paint-time peek, as `TabRow`'s indicator does).
- **One-frame viewport delay** - `on_size_changed` writes back during measure; the first frame has no
  thumb yet.
- **Vertical reverse** - `scroll_reverse` on `VerticalScrollbar` has no CMP counterpart (the vertical
  container's `verticalScroll(reverseScrolling = true)` is a winia addition).

## 6. Tests

```bash
cargo test -p winia --lib scrollbar      # unit tests over the arena
cargo run -p winia --example scrollbar_demo --features debug-server
```

The demo scenario is `scrollbar_demo`; the fading, hover, track-jump, lazy, reverse and custom-style
cards each exercise one behaviour above.