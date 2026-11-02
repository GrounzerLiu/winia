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
| `scrollBy(pixels)` | `ScrollableState.kt:182` | absent - only `scroll_to` / `animate_scroll_to` |
| `dispatchRawDelta` | `ScrollableState.kt:73` | absent |
| `canScrollForward/Backward` | `ScrollableState.kt:92` | present (`layout/lazy/state.rs:160`, plus a `remember_can_scroll_forward` that notifies only on flips) |
| `lastScrolledForward/Backward` | `ScrollableState.kt:115` | absent |
| `animateScrollBy` | named in `ScrollableState.kt:33`; the suspend API | absent; `animate_scroll_to(value, max)` exists |
| Wheel on a pager | nudges a few pixels | steps a whole page (deliberate; see section 3) |
| Overscroll applied to flings | `Scrollable.kt:717` `overscroll.applyToFling(...)` | pull only; a fling that hits the end does not bend the content |
| Overscroll shape | stretch with scale/glow on some platforms | shift only, capped at 120 px, no scale (`docs/scroll-extras.md`) |
| Keyboard scrolling | a focused scrollable consumes arrows / Page keys **(not vendored)** | absent: arrows are focus traversal (`app/app_state/handlers.rs:571-574`), PageUp/PageDown appear nowhere except the Slider |
| Nested scroll protocol | pre/post scroll + pre/post fling, `Offset` returns **(not vendored)** | same four hooks, same order, clamped (`scroll/nested.rs:59-64`) |

## 6. Gaps, in the order I would close them

1. **No keyboard scrolling for scroll containers.** A desktop list cannot be scrolled without a
   wheel or a scrollbar drag; Compose's scrollable takes arrows and Page keys when focused. This is
   the one gap a user hits in the first minute.
2. **No `scrollBy` / `dispatchRawDelta`.** Every programmatic scroll has to go through `scroll_to` with
   an absolute value and a caller-computed limit; a "scroll by one line" has no shape.
3. **Overscroll does not apply to flings.** A drag past the edge bends the content, a fling past it just
   stops. Compose pipes the fling through the effect (`Scrollable.kt:717`).
4. **No `lastScrolledForward/Backward`.** A bar that hides on scroll direction has to diff offsets
   itself; the state already knows the direction but does not publish it.
5. **Scrollbar placement is the caller's job** (no RTL mirroring). Already recorded in
   `docs/scrollbar.md` section 3.

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
