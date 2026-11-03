# Scroll subsystem review

Status: **2026-11-02 — 3 defects fixed, 1 missing API added, 0 open.**

This covers the whole scroll stack: `scroll/` (state, nested, fling, overscroll), `app/scroll.rs`
(delta + fling dispatch), `app/wheel.rs`, the scroll modifiers, `components/scrollbar.rs` and the lazy
list's use of the state. The lazy list's own policy is covered by its 120+ tests and was not re-audited
here; the text pipeline has its own review (`docs/text-review.md`).

## Defects

### S-1 `scroll_to` did not cancel a running fling — FIXED

Compose's `ScrollState.scrollTo` runs through `scrollableState.scroll`, which cancels any fling. winia's
`scroll_to` only wrote the offset, so the decay kept writing its own trajectory over it: measured
`before=212.15`, `scroll_to(0)` then the next frame read **260.26**. Fixed in `scroll/state.rs`
(`cancel_fling()` before the write). Test: `a_scroll_to_cancels_a_running_fling`.

### S-2 A non-finite scroll target poisoned the offset — FIXED

`scroll_to(f32::NAN, 100.0)` stored NaN (Rust's `clamp` passes NaN through), and a NaN offset is
unclampable later: every comparison against it is false, so the measure clamp cannot catch it and the
render translates by NaN. `scroll_by(NaN)` and `animate_scroll_to(NaN)` had the same hole; `scroll_by`
with infinity was already safe (the clamp catches it). Fixed with `is_finite` guards in all three.
Test: `a_non_finite_scroll_is_ignored`.

### S-3 The reverse setters wrote to the wrong element — FIXED

`horizontal_scroll_reverse` and the new `vertical_scroll_reverse` set the flag with
`self.elements.last_mut()`. Since overscroll became ON BY DEFAULT, `horizontal_scroll()` appends an
ambient `Overscroll` element AFTER the state element, so the setter wrote to the effect and the flag
was silently dropped — the RTL tab row (`components/tab_row/scrollable.rs:206`) was affected. Fixed to
scan for the last `HorizontalScroll` / `VerticalScroll` instead of assuming it is last.
Test: `a_reverse_flag_survives_the_ambient_overscroll_element`.

### S-4 The scrollbar dragged with build-time geometry — FIXED

The thumb drag captured `track`, `thumb_len` and `max_offset` from the build that created the
callbacks. A bar built before its viewport write-back exists (the first frame) has `track = 0`,
`thumb = 0`, and `scrollbar_offset_for_thumb_pos` floors the travel at 1 px - so a 50 px move wrote
the whole range (measured: offset 200 of 200), and with a pre-measure limit the same arithmetic
would have written `f32::MAX` into the offset. A lazy list's limit also changes while the bar is
alive, so the capture could go stale in normal use. The drag and drag-start callbacks now read
`viewport`/`fling_limit`/`offset` from the states and recompute the geometry per event, and a bar
without a thumb ignores the drag entirely. Test: `a_thumb_drag_uses_the_live_geometry` (a press 3 px
into the thumb plus a 50 px move is 50/64 of a 200 px range = 156.25; with a stale capture it read 0).

### S-5 A drag start took over the gesture with no thumb to drag — FIXED

`on_drag_start` cancelled the list's fling and set `is_scroll_in_progress` before checking that the
bar had a thumb. On a bar whose content fits (or whose viewport is not known yet) the move callback
now returns early, so the flag stayed set until the pointer was released - a press on an empty bar
cancelled a running fling for nothing. The take-over is now behind the same live geometry check the
move uses. Test: `a_drag_start_without_a_thumb_leaves_the_container_alone`.

### S-6 A press on the bar scrolled the page instead of the thumb — FIXED

A press that lands on a draggable component inside a scroll container is arbitrated by the finger's
dominant axis: a vertical drag belongs to the scroll (that is what keeps a horizontal slider inside a
vertical list working). A scrollbar thumb is the opposite case - its drag IS the scroll's axis - so
every vertical move went to the page and the thumb never moved. Measured in `scrollbar_demo`: drag the
card-2 bar 60 px and the page offset stayed 0 while the band stayed 0 too (the gesture was swallowed);
with the fix the band moves to 201 and the page stays at 0.

The fix is a marker: `Modifier::drag_outranks_scroll()` pushes `ModifierElement::DragPreemptsScroll`, the
scrollbar sets it, and `handle_pointer_down` leaves `scroll_slot` empty for such a target so the axis
arbitration never runs. Test: `the_bar_declares_that_its_drag_outranks_the_scroll`.

### S-7 A padded scroll container clipped the last `pad_bottom` pixels away — FIXED

The scroll range is computed against the node's own height (padding included), but the paint clipped
the content to the padding-DEDUCTED viewport. With `padding(16)` the last 16 px the range allowed
into view were clipped away, so a page scrolled to its very end still cut the bottom of its last
card (reported with a screenshot; reproduced in `scrollbar_demo` at 760x431: the readout card's last
line was half cut at the clip edge).

A PLAIN container now clips to its own box (`h`/`w`); a LAZY list keeps using the viewport field,
because there the measured size is the CONTENT and the field is the real viewport. Test:
`render_snapshot::a_padded_scroll_container_paints_to_its_own_bottom_edge` (at the limit, the child's
row 89 must paint; it was white before the fix).

### R-1 A press on the track before the first measure wrote a garbage offset — FIXED

A fresh `ScrollState` reports `f32::MAX` as its limit (the "not measured yet" sentinel). The new
press-to-jump path used it as a real range: `content = f32::MAX + viewport`, so one press on the
track of a bar whose container had not been measured wrote an offset of **2.6e38** and the content
disappeared (measured in a unit probe: fresh limit 3.4e38, offset after press 2.66e38). The same
sentinel reached `scrollbar_geometry`, where it would have drawn a one-pixel thumb.

`measured_limit()` now rejects a non-finite or `f32::MAX` limit, and both the press jump and the thumb
drag return without writing. The press jump also cancels a running fling first (`scroll_to` does the
same), so a click during a fling does not get overwritten on the next frame. Test:
`a_press_before_the_first_measure_does_not_write_a_garbage_offset`.

## Missing API

### M-1 `verticalScroll(reverseScrolling = true)` — ADDED

`Modifier::vertical_scroll` had no reverse flag (only the horizontal one had), even though the render
and `nodes[].scroll_reverse` already understood vertical reversal for lazy lists. Added
`Modifier::vertical_scroll_reverse(bool)`, `is_vertical_scroll_reversed()`, the element field (part of
`param_eq`) and the measure-time write. The vertical delta path also did not mirror the gesture for a
reversed container (the horizontal one always did) — fixed: a drag now follows the finger in both
directions. Tests: `a_reversed_vertical_container_mirrors_the_delta`, plus the flag test above.

### M-2 Hover/drag colour feedback — ADDED

CMP's `ScrollbarStyle` takes an `unhoverColor` and a `hoverColor`; winia only faded the alpha, so a
hovered bar looked identical. The shared build now resolves the thumb colour per interaction state
(`scrollbar_thumb_tint`) and animates it with `animate_color_as_state` over 300 ms. The default hover
colour is the same theme outline token at full alpha (the bar darkens rather than changing hue).
Test: `the_thumb_tint_follows_the_interaction`. A pixel test of the tint is not possible in the
harness (it cannot deliver a hover), so the visual check is by hand in `scroll_demo`.

### M-3 Hover thickening — ADDED

WinUI and macOS grow the bar under the pointer; CMP only recolours it. The thumb's cross-axis size is
now chosen per interaction state (`scrollbar_thumb_thickness`) and animated over 150 ms. The bar's
LAYOUT width stays `thickness`, so the growth is centred in the slot and never reflows the content;
the thicker thumb overhangs its slot by two pixels each side at the default settings. Setters:
`.hover_thickness(dp)` / `.hover_color(color)` on all four components. Test:
`the_thumb_thickness_follows_the_interaction`.

### M-4 A press on the empty track jumps the thumb — ADDED

CMP has no track handling at all; WinUI and macOS centre the thumb under the press and keep dragging
from there. A press outside the thumb now jumps the offset so the thumb's centre lands under the
pointer, and the grab offset becomes half the thumb, so the drag continues without a jump. A press on
the thumb keeps its grab offset (nothing moves). Test: `pressing_the_track_centres_the_thumb`
(96 px track, 32 px thumb, press at 70 -> offset 168.75 of 200).

### M-5 A dedicated demo — ADDED

`winia/examples/scrollbar_demo.rs` shows seven cards: the fading default bar, `always_show`, the
horizontal bar, a lazy column with `LazyScrollbar`, a reversed vertical container, a custom style
(thicker, warm colours, a 40 px minimum thumb) and a live readout of the card-1 state.

## Checked and ruled out

- **Wheel paging gate**: a precision-touchpad stream banks travel and a page turns only after
  `PIXEL_PAGE_TRAVEL`; a notch always steps once. `AtEnd` deliberately does NOT pixel-scroll (that was
  the "stuck between pages" bug). Covered by `app/wheel_paging_tests.rs`.
- **Nested scroll order**: pre outer→inner, post inner→outer including the target, consumed clamped to
  what is available. Covered by `nested_scroll` tests.
- **Overscroll**: damped curve, per-state effect cache, ambient default, key steps clamped, restored
  offsets clamped — see `docs/scroll-extras.md` (three review rounds today).
- **Keyboard**: arrows/Page/Home/End pick the nearest scrollable ancestor that can still move, stop at
  the edge without feeding the rubber band; focus reveal scrolls a newly focused node into view.
- **Saveable position**: restores and clamps; a lazy list does not save its first visible item yet
  (documented in `scroll-extras.md`).
- **Scrollbar**: geometry (min/max thumb, hidden when the content fits), fade, RTL placement (a bar last
  in a `Row` follows the direction). No code needed.
- **Fling**: NaN velocity ignored, 1 px/s decay floor, snap plans settle on a page, reverse fling
  velocity mirrored.

## How to re-check

Each fix names its test; all three fail on the old code. The quickest end-to-end check is
`scroll_demo`: card 3 (overscroll, two stiffnesses), card 4 (keyboard), card 5 (paging), plus a quick
drag past the end to see the rubber band.