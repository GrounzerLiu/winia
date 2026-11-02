# Scroll layout: how it works, and where it stands against Compose

Status: study, 2026-11-02. Not a contract - `docs/component-authoring.md` is. This file is what a
reader needs to predict what a scroll container will do, where to change it, and which Compose
behaviour is deliberately absent.

Method: read the sources listed below, then compare against `reference/compose`. Where a Compose
claim cannot be checked against the vendored bundle it is marked **(not vendored)** instead of being
stated as fact - the bundle holds foundation / foundation-layout / material / material3 /
animation-core, but NOT the `ui` module, and nested-scroll and many `ScrollState` docs live there.

## 1. The layers

| Layer | File | Lines | Role |
| --- | --- | --- | --- |
| State | `winia/src/scroll/state.rs` | 318 | `ScrollState`: offset, in-progress flag, fling limit, snap spec, page-step goal, pluggable `FlingBehavior` |
| Protocol | `winia/src/scroll/nested.rs` | 180 | `ScrollDelta` / `ScrollVelocity` / `NestedScrollSource` / `NestedScrollConnection` / dispatcher |
| Kinematics | `winia/src/scroll/fling.rs` | 275 | `FlingContext`, `SnapInfo`, `FlingPlan` (Spring / Decay / None), `SnapFlingBehavior` |
| Overscroll | `winia/src/scroll/overscroll.rs` | 217 | pull accumulator + delayed return tween |
| Dispatch | `winia/src/app/scroll.rs` | 727 | the nested chain, target lookup, delta application, fling routing |
| Wheel | `winia/src/app/wheel.rs` | 307 | wheel deltas, per-container axes, page steps for paged lists |
| Chrome | `winia/src/components/scrollbar.rs` | 824 | thumb geometry, drag, fade, lazy/horizontal variants |
| Lazy | `winia/src/layout/lazy/*` | - | viewport measurement writes `fling_limit` back each measure |

## 2. The protocol

- **Signs.** A scroll offset grows as content moves up/left. A negative vertical delta moves content
  down (offset decreases). Every consumer follows it; a sign inversion is a bug that looks like
  "scrolling the wrong way only in nested cases".
- **Units.** Offsets and deltas are logical pixels; `fling_limit` is `content - viewport`, written by
  the layout as a `Backchannel<f32>`, and `f32::MAX` means "not measured yet". A real `0` means the
  content fits and the container cannot move - the distinction is load-bearing (`scroll/state.rs:11-19`:
  reading `0` as "unknown" made a three-item menu fling after a drag that moved nothing, offset 74).
- **Sources.** `Wheel`, `Drag`, `Fling`, `SideEffect`. `is_user_input()` is `Drag | Wheel`
  (`scroll/nested.rs:43-57`); a bottom sheet must gate on that and ignore `Fling`/`SideEffect`.
- **Order.** Pre-scroll walks the path OUTSIDE-IN; post-scroll walks it inside-OUT and includes the
  target's own connection (`app/scroll.rs:118-146`, comment at :136-139 explains why the target is
  included: a TopAppBar connection hangs off the scroll node and recolours through post-scroll).
- **Clamping.** Every returned consumption is clamped to what was available
  (`nested.rs:9-38`), so a connection that overshoots cannot flip the remaining delta's sign.
- **Fling chain.** Pre-fling outside-in, then the child decays, and the velocity still live at a
  boundary is handed to post-fling inside-out (`app/scroll.rs:150-200`).

## 3. One wheel notch, end to end

1. `app/wheel.rs:150 wheel_scroll_target` picks the innermost node under the pointer that can move on
   either axis; `wheel_axes_for` (:276) zeroes the axis the container cannot use.
2. A paged container (`ScrollState::snaps()`) takes `wheel_page_step` (:181) instead of pixels - one
   notch moves one page. This is a winia-ONLY behaviour: Compose lets the wheel nudge a pager by a few
   pixels and barely moves it (`scroll/state.rs:123-132` records the deviation and its reason).
3. Otherwise `apply_wheel_to_target` (:227) runs the nested chain, applies the consumed delta and,
   when nothing moved, bumps `ScrollState::scroll_pulse` (:290) so a scrollbar can still flash "at the
   edge" feedback.

## 4. One drag release, end to end

1. The gesture path (pointer drag) accumulates the delta and calls `apply_scroll_delta`
   (`app/scroll.rs:262`), which cancels any running animation first.
2. On release, `ScrollState::fling_with_boundary` (`scroll/state.rs:200`) asks the `FlingBehavior` for
   a plan: `Spring { target, spec }` for a snapping container, `Decay` otherwise, `None` for a
   strategy that declines.
3. A plan that aims past the limit is clamped, and the clamp is REPORTED to the post-fling chain
   (`:262-277`) - a spring has no leftover velocity, but the ancestor still has to learn the child hit
   its end.
4. A decay clamps per frame through a `Backchannel` read (`:280-298`), and the instantaneous remainder
   at impact goes to `on_post_fling`.

## 5. Against Compose

| Behaviour | Compose | winia |
| --- | --- | --- |
| `scrollBy(pixels)` | `ScrollableState.kt:182` | **shipped** `ScrollState::scroll_by` (clamped, returns what it consumed, cancels a fling first) |
| `dispatchRawDelta` | `ScrollableState.kt:73` | deliberately not mirrored: it exists to bypass the nested dispatcher while clamping, and `ScrollState` has no dispatcher - the chain lives in `app::scroll`, so a second name would just invite the wrong pick |
| `canScrollForward/Backward` | `ScrollableState.kt:92` | present (`layout/lazy/state.rs:160`, plus a `remember_can_scroll_forward` that notifies only on flips) |
| `lastScrolledForward/Backward` | `ScrollableState.kt:115` | **shipped as a sample**: `last_scrolled_forward()` answers "did the offset move toward the end since the previous read" - there is no single write to hook (the delta path and the fling's motion both set the offset) |
| `animateScrollBy` | named in `ScrollableState.kt:33`; the suspend API | absent; `animate_scroll_to(value, max)` exists |
| Wheel on a pager | nudges a few pixels | steps a whole page (deliberate; see section 3) |
| Overscroll applied to flings | `Scrollable.kt:717` `overscroll.applyToFling(...)` | **shipped**: the velocity left when the decay hits a boundary becomes a bend (`pull(velocity / 40)`), capped by the same 120 px |
| Overscroll shape | stretch with scale/glow on some platforms | shift only, capped at 120 px, no scale (`docs/scroll-extras.md`) |
| Keyboard scrolling | `Scrollable` handles **PageUp/PageDown only**, one viewport per press (`foundation/gestures/Scrollable.kt:428-471`); arrow keys move FOCUS there and the next item is brought into view | arrows / PageUp / PageDown / Home / End scroll the focused node's nearest scroll container (winia has no bring-into-view, so the arrows scroll instead) |
| Nested scroll protocol | pre/post scroll + pre/post fling, `Offset` returns **(not vendored)** | same four hooks, same order, clamped (`scroll/nested.rs:59-64`) |

## 6. Gaps, in the order I would close them

1. ~~**No keyboard scrolling for scroll containers.**~~ **Shipped 2026-11-02** - arrows move 48 px,
   Page keys one viewport, Home/End jump to an end; the step walks the focused node's ancestry
   inside-out (else the innermost node under the last pointer press) so a lazy list scrolls before the
   page does, and a container with nothing left to scroll lets the key fall through to focus
   navigation (`app/scroll.rs` `scroll_by_key`, tests in its `key_scroll_tests`). Compose's own
   `Scrollable` handles Page keys only (`foundation/gestures/Scrollable.kt:428-471`); the arrow step
   is a winia addition, because there is no bring-into-view to fall back on.
   Measured in `lazy_layout_demo` through the debug server: three ArrowDown presses took the inner
   list 0 -> 48 -> 96 -> 144, PageDown moved exactly one viewport (150), Home/End reached 0 / 7050.
   The debug `k` command needed `PageUp`/`PageDown` added to its key-name table before any of that
   could be driven; a key the debug path cannot name is a path nobody tests.
2. ~~**No `scrollBy` / `dispatchRawDelta`.**~~ **`scroll_by` shipped 2026-11-02**, together with
   `max_value`, `can_scroll_forward` and `can_scroll_backward` (`scroll/state.rs`, tests in its
   `scroll_by_tests`: moves and reports the consumed amount, clamps at both ends and says so,
   follows a limit that starts open-ended and settles after the first measure). `dispatchRawDelta` is
   intentionally NOT mirrored - see the table above.
3. ~~**Overscroll does not apply to flings.**~~ **Shipped 2026-11-02** - the boundary callback the
   fling already reported now feeds the effect: `pull_for_velocity` turns the remaining velocity into
   a bend (40 px/s per pixel, 120 px cap), so a fling into an end bends and returns instead of just
   stopping. Tests: `a_fling_bend_follows_velocity_and_caps` and `a_fling_into_the_end_leaves_a_bend`
   (the second drives the real `dispatch_nested_scroll_fling` and asserts a non-zero stretch).
4. ~~**No `lastScrolledForward/Backward`.**~~ **Shipped 2026-11-02** as `ScrollState::last_scrolled_forward()`,
   sampled at read time rather than updated per scroll: reading it once a frame is what a bar that
   hides on direction needs, and it costs one float when nobody reads it.
5. **Scrollbar placement is the caller's job** (no RTL mirroring). Already recorded in
   `docs/scrollbar.md` section 3.

### 6.1 Two measurement traps (both hit while writing the demo)

- **The layout tree does not show scrolling.** `t` reports LAYOUT positions; a scroll offset is
  applied when painting, so text positions are identical before and after a scroll. Read the
  container's `vscroll(<offset>)` entry instead - or the state's own readout.
- **A wheel goes to the innermost scrollable under the pointer, not to the page.** Driving a page by
  wheel from a point that sits over a card's inner list scrolls that list; the page only sees the
  delta once the inner one is at its end. Pick a point outside every inner scroller, or press/hover
  the container under test.
- Together these produced a false finding - ".padding().vertical_scroll() never scrolls, the reverse
  order does" - that the probe refuted: both orders measure identically (content 832, viewport 268,
  same node size), and `modifier_order_tests` in `app/scroll.rs` now pins that.


### 6.2 Focus reveal (shipped 2026-11-02)

Plain scroll containers already had `BringIntoViewRequester` (`layout/relocation.rs`) but nothing
called it, so keyboard focus could land on a row the viewport does not show. `tab_move_focus` and
arrow focus navigation now call `PerWindow::reveal_focused` after the focus move, which resolves the
focused node to an INDEX and reveals it through `reveal_node` - the same `reveal_in` arithmetic and
spring as the requester, but driven from the tree instead of a handle, because the focus owner knows
only an id.

Measured in `scroll_demo` through the debug server: tabbing onto a button that sits below the band
scrolled the band from 0 to 330 (its end). Unit tests in `layout/relocation.rs` cover a row below the
viewport (spring target 200), a visible row (no animation), a row above (target 0) and a nested list
(the inner container moves, the outer one stays).

Limits inherited from `reveal_in`: `reverseLayout` containers are skipped rather than scrolled the
wrong way, and a lazy list can only reveal items that are composed.


### 6.3 Scroll position across a saveable restore (shipped 2026-11-02)

`ScrollState::remember_saveable(ctx, key)` (`scroll/state.rs`) keeps the offset in the saveable
registry, so a list opens where it was after a window recreate or a navigation push/pop. Compose's
`rememberScrollState` is saveable for the same reason. The saved value is the offset alone - for a
plain container the content belongs to the caller's composition, so the offset is the whole state; a
LAZY list also needs its first visible item, which is NOT saved yet.

Test: `a_saved_scroll_position_comes_back_in_the_next_composition` (compose under one registry,
scroll to 240, `perform_save`, compose again under a registry seeded with it, offset is 240).
`Saveable` gained an `f32` impl stored as `F64`, since the registry's closed set had i32/i64/f64.

### 6.4 `max_value` is a peek, not a subscription

The limit is written back by the measure through a `Backchannel` (a `State` write during measure
would notify in the middle of layout), so a readout that reads only `max_value()` never re-renders.
Read the offset too - it is a `State`, and it changes whenever the limit stops mattering. This is
documented on the method and is what `scroll_demo`'s cards do.


### 6.5 The rubber band is damped, and its feel is a parameter (2026-11-02)

`OverscrollEffect` used to be linear: `clamp(raw, +-120)`, so the content tracked the finger exactly
until it hit a hard stop. It now accumulates raw travel and DISPLAYS a saturating curve
(`max_pull * (1 - e^(-raw / reach))`): 1:1 at the origin, fading, approaching the cap without
reaching it. `max_pull` (default 120) and `reach` (default = `max_pull`) are builder methods, and the
fling bend goes through the same curve.

Numbers with the defaults: a 10 px pull shows 9.6, a 20 px pull 18.4, a 240 px pull 103.7, and even
a 5 000 px pull only reads 120 because f32 rounds there. `pull` still returns the whole leftover, so a
nested ancestor never sees an edge gesture.

Tests: `a_pull_starts_one_to_one_and_then_fades_towards_the_cap` (red check: making the curve linear
again fails it), `reach_and_cap_shape_the_band`, `a_fling_bend_follows_velocity_and_fades`, and
`a_live_gesture_keeps_its_tension`. `scroll_demo`'s third card shows two bands side by side -
default and `reach(40)` - so the feel can be chosen by hand.


## 7. Test map (and the two thin spots)

| Area | Tests in file | Where else it is covered |
| --- | --- | --- |
| `scroll/fling.rs` | 3 | `modifier/tests.rs` snap-target maths, `app/wheel_paging_tests.rs` (13) |
| `scroll/nested.rs` | 3 | `app/scroll.rs` (5) |
| `scroll/overscroll.rs` | 4 | `app/scroll.rs:454` ancestor hand-off |
| `scroll/state.rs` | **0** | its snap maths is tested through `modifier/tests.rs`; fling behaviour through `app/wheel_paging_tests.rs` |
| `app/wheel.rs` | **0** | `app/wheel_paging_tests.rs` (13) |
| `app/scroll.rs` | 5 | - |
| `components/scrollbar.rs` | **0** | `components/scrollbar/tests.rs` |

The thin spots are not untested code - they are tests that live somewhere else, which is why a change
to `ScrollState::snap_page` can pass a green run and still break the pager (the tests it relies on
are named after the wheel). Worth a `scroll/state.rs` test module that drives the state directly.

## 8. Where to start

Read `scroll/state.rs` first (it holds every decision a caller can observe), then `app/scroll.rs`
section by section: `nested_scroll_delta` (:96), `dispatch_nested_scroll_fling` (:150),
`apply_scroll_delta_inner` (:273). `app/wheel.rs` and `components/scrollbar.rs` are consumers of those
two and can wait.
