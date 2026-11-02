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

## Missing API

### M-1 `verticalScroll(reverseScrolling = true)` — ADDED

`Modifier::vertical_scroll` had no reverse flag (only the horizontal one had), even though the render
and `nodes[].scroll_reverse` already understood vertical reversal for lazy lists. Added
`Modifier::vertical_scroll_reverse(bool)`, `is_vertical_scroll_reversed()`, the element field (part of
`param_eq`) and the measure-time write. The vertical delta path also did not mirror the gesture for a
reversed container (the horizontal one always did) — fixed: a drag now follows the finger in both
directions. Tests: `a_reversed_vertical_container_mirrors_the_delta`, plus the flag test above.

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