# ModalBottomSheet

A sheet that slides up from the bottom over a dimmed scrim, with three states (`Hidden`,
`PartiallyExpanded`, `Expanded`), a drag handle, drag-to-collapse and a dismissal on an outside click.

```rust
let visible = ctx.remember(|| false);
Button::text().on_click({ let v = visible.clone(); move || v.set(true) }).build(ctx, |ctx| Text::new("open").build(ctx));
ModalBottomSheet::new(visible.get())
    .on_dismiss_request({ let v = visible.clone(); move || v.set(false) })
    .build(ctx, |ctx| { /* content */ });
```

`visible` is the single source of truth (Compose's `sheetState.isVisible`): the sheet animates itself and
calls `on_dismiss_request` when it has finished sliding out — a callback that runs when the animationCOMPLETES, not when it starts (an earlier version fired on the anchor change, so the parent set
`visible = false` and the overlay vanished mid-slide).

| Token | Value | Source |
|---|---|---|
| Top corner radius | 28 dp | `BottomSheetDefaults.ExpandedShape`, `SHEET_TOP_CORNER_RADIUS` |
| Container colour | `surface_container_low` | `SheetBottomTokens.DockedModalContainerColor` |
| Container elevation | 1 dp | `DockedModalContainerElevation` |
| Max width | 640 dp, centred | `sheetMaxWidth` |
| Drag handle | 32x4 dp, padding 22 dp, `on_surface_variant` | `DockedDragHandleWidth/Height`, `DragHandleVerticalPadding` (`SheetDefaults.kt:788`), `DockedDragHandleColor` |
| `skip_partially_expanded` | off | drops the half-expanded anchor, so a swipe down goes straight out. Applies only to a state this component created — a caller-supplied one keeps its own flag |

## Dragging: who owns the gesture

The sheet is Compose M3's `ConsumeSwipeWithinBottomSheetBoundsNestedScrollConnection`:

- an UPWARD delta is the sheet's first (`expands-first`): the sheet expands, and only what is left over
  scrolls the list;
- a DOWNWARD delta belongs to the list while it can scroll, and to the sheet once it cannot — so dragging
  down with the list at its top collapses the sheet, in two steps (`Expanded` → `PartiallyExpanded` →
  `Hidden`);
- the panel itself is draggable anywhere outside the list (its header, its padding), which is the
  fallback the hit test picks when the press is not on a scrollable.

This is pinned by `bottom_sheet_drag_routing_keeps_the_list_in_charge_of_its_own_scroll` (five steps,
asserted through the debug tree) because the arbitration is three-way — an inner drag component beats a
scroll, a scroll beats the panel's `on_drag` — and breaks silently. The report that started it ("a drag
inside the list dismisses the sheet") turned out to be the documented M3 behaviour rather than a bug.

**That test no longer exists.** It was one of the UI scenario tests deleted with the fixture refactor
(`98847b2`, "remove UI scenario tests pending fixture refactor", 4747 lines out of `tests/ui_test.rs`),
and nothing replaced it: `docs/ui-testing.md` still lists it, but `rg "fn bottom_sheet_drag_routing"`
finds no definition. The three-way arbitration above is therefore currently **uncovered** — the code is
unchanged and was correct when the test last ran, but no test would notice it breaking. Restoring it is
a known gap, not something this round fixed.

## Shape: square when it fills the window

**winia's own rule, not Material 3's.** An expanded sheet whose panel reaches the window height drops its
top corners. Current M3 passes `shape` through untouched — `BottomSheetDefaults.ExpandedShape` is
`DockedContainerShape` (`CornerExtraLargeTop`, the same 28 dp) and nothing in `ModalBottomSheet.kt` switches
on the sheet's state, so the square-corner rule came from the original winia implementation and is kept
deliberately. Do not "align" it away; the test below locks it.

The surface is PAINTED per frame by `SheetPanelNode` (a `DrawNode`), because the radius cannot be composed
in that closure at all: its reads of the sheet's state happen outside any composition group and register
nothing, so a shape derived from them never updates. Measured twice — once as the original build-time shape,
once as one driven by an explicit tracked read — both times the corners went square only after an unrelated
compose (a list scroll) re-ran the closure, and stayed square after collapsing. Paint-time reads use `peek`,
so nothing in the draw path marks a slot dirty.

The CLIP behind the surface stays a build-time shape (the framework's `clip` takes a plain `Shape`, not a
closure), so it can lag the painted radius by one anchor crossing. Nothing in this component's own content
is affected — the corners are padding — but a caller who paints content into the panel's top corners will
see the clip and the surface disagree mid-drag.

`the_bottom_sheet_panel_is_square_when_expanded_and_rounded_again_when_not` used to read the pixels just
inside and clear of the corner — a rounded corner shows what is behind the panel, a square one shows the
panel — with a fixture whose content is taller than the window, since the panel only squares up when its
height reaches the window's. **That test was deleted too** (`98847b2`, the same fixture refactor), so the
square-corner rule is now pinned by nothing: the rule itself is unchanged and documented here, but a
regression in `SheetPanelNode::current_radius` would no longer be caught.

## Differences from M3 (beyond the shape rule above)

- **Settle curve.** M3 settles, shows and hides with motion-scheme tokens — `DefaultSpatial` for the settle
  and the show, `FastEffects` for the hide (`BottomSheet.kt:135-141` sets `state.showMotionSpec` /
  `hideMotionSpec`; the scaffold uses the same three at `BottomSheetScaffold.kt:257-263`) — while winia uses
  a 250 ms `EaseOutCubic` tween for all three. A tween
  cannot overshoot, which is why M3's `verticalScaleUp`/`verticalScaleDown` (the drawing-layer scale that hides
  the gap a bouncy settle leaves under the panel's bottom edge) has no counterpart here; it becomes necessary
  the moment the settle turns into a spring. The positional and velocity gates do match:
  `BottomSheetDefaults.PositionalThreshold = 56.dp` (`SheetDefaults.kt:557`) and `VelocityThreshold = 125.dp`
  (`SheetDefaults.kt:559`).
- **Scroll sources.** M3's connection answers `NestedScrollSource.UserInput`, which in Compose covers the
  mouse as well as touch — `MouseWheelScrollingLogic.dispatchMouseWheelScroll` dispatches the wheel with
  `NestedScrollSource.UserInput` (`foundation/gestures/MouseWheelScrollingLogic.kt:303-307`). winia splits that
  case in two, so its callbacks accept `NestedScrollSource::Drag | Wheel` through
  `NestedScrollSource::is_user_input()` and reject `Fling`/`SideEffect` (Compose's `SideEffect`). In practice a
  wheel behaves exactly like a finger here. Mind the SIGN when reading or testing this: a negative `y` means the
  content moves up — a finger dragging up, or the wheel rotated DOWN — and that is the delta which expands the
  sheet first (the repository's own convention, `ui_test.rs`'s "Negative dy scrolls FORWARD"). A downward
  (positive) delta scrolls the list first and hands the sheet what the list could not use. The fling callbacks
  are unaffected — they carry a velocity, not a source.

Two things described above were deviations in an earlier version and are now fixed, each covered by a test: the
positional threshold fell back to half the travel distance (a drag of 100 px toward a nearer anchor sprang back
where M3 commits), and the callbacks ignored the source, so any nested delta could move the sheet. The source
guard itself is cheap insurance rather than a fix for something reachable today: no production call site passes
`Fling` or `SideEffect` to these callbacks (the four wheel sites pass `Wheel`, the two pointer sites `Drag`),
and flings travel through `on_pre_fling`/`on_post_fling`, which carry a velocity and no source. A third fix in
that round went too far and was corrected the same day: filtering to `Drag` alone also rejected the wheel, but a
wheel IS user input in M3, so it drives the sheet exactly like a finger.

### Two defects found against this source and fixed

Both were measured before the fix and each is pinned by a test now.

- **`sheet_gestures_enabled(false)` only half worked.** Compose puts both halves of the gesture behind
  that one flag: the `Modifier.nestedScroll(..)` is skipped AND `anchoredDraggable(enabled =
  gesturesEnabled && currentValue != Hidden)` (`BottomSheet.kt:282,332`) turns the drag off. winia's
  `if gestures` block covered only the nested-scroll connection, so the flag disabled the
  list-to-sheet routing while a finger on the panel — or on its handle — still dragged and collapsed
  the sheet. Measured: both builds produced 2 drag-handler nodes. Now the panel's and the handle's
  `on_drag` are inside the same gate; `turning_gestures_off_removes_every_drag_handler` asserts the
  count is 0 with the flag off.
- **A caller-supplied `SheetState` lost its `skip_partially_expanded`.** Compose has no such parameter
  on `ModalBottomSheet` at all — the flag is fixed when the state is built
  (`rememberModalBottomSheetState(skipPartiallyExpanded)`, `ModalBottomSheet.kt:241-252`). winia wrote
  the builder's `false` default over the state on every build, so
  `state.set_skip_partially_expanded(true)` before showing the sheet was a silent no-op: the
  half-expanded anchor came back. Now the builder only writes the flag when it created the state
  itself; `a_caller_supplied_state_keeps_its_skip_partially_expanded` pins it.

## Not implemented (gaps against Compose)

Measured against `ModalBottomSheet`'s signature (`ModalBottomSheet.kt:96-110`) and `BottomSheet`'s
(`BottomSheet.kt:117-133`). Each row is a parameter or behaviour Compose has and this component does not.

| Compose | Here |
|---|---|
| `modifier` | none — the panel's chain is built internally |
| `contentColor` | none; content colours come from the theme |
| `tonalElevation` (default `0.dp`) | none; the panel always paints a 1 dp shadow |
| `shadowElevation` | fixed at 1 dp |
| `scrimColor` | none; the overlay host draws the scrim with its own colour |
| `dragHandle: @Composable (() -> Unit)?` | a `bool`. No custom handle content, no `BottomSheetDefaults.DragHandle` tokens exposed |
| the handle's TooltipBox + `clickable` state machine | neither. Compose's handle is a BUTTON: `Expanded` dismisses, `PartiallyExpanded` expands, `Hidden` shows (`BottomSheet.kt:366-372`). Here it is only a drag target |
| `contentWindowInsets` | none; no inset is consumed at all |
| `properties` / `ModalBottomSheetProperties` | none; `dismiss_on_outside` and `dismiss_on_back_press` are hard-coded `true` |
| `content: @Composable ColumnScope.() -> Unit` | `impl Fn(&mut ComposeCtx)`, no `ColumnScope` receiver |
| `confirmValueChange` (via the state) | `SheetState::set_confirm_value_change` exists, but `ModalBottomSheet` never exposes it, so a veto has to be wired by handing in a state |
| `backHandlerEnabled` | none |
| Hidden state: `HiddenShape` / `DockedMinimizedContainerShape` | none; one 28 dp shape, square when full height |
| `isTraversalGroup`, `paneTitle`, `traversalIndex`, drag-handle semantics | none (`docs/semantics-gap.md`) |
| `modalBottomSheetFlingBehavior`'s `BoundaryDampeningZone` (125 dp velocity dampening near the floor) | none; the settle is a plain tween |

**Back press is one step, not two.** Compose's `settleToDismiss` collapses to `PartiallyExpanded` when
the sheet is `Expanded` AND has a half stop, and only hides from there (`ModalBottomSheet.kt:125-134`),
so one Escape from a fully expanded sheet lands half-expanded. winia's escape path calls `hide()`
directly. That is a behavioural difference, not a bug in isolation — the two-step form exists so a back
gesture does not dismiss outright — but it means an `Expanded` sheet closes on a single Escape here.

## Dismissal paths

- Clicking the scrim (or anywhere outside the panel) → `on_dismiss_request` → the parent flips `visible`,
  the overlay fades out and is removed.
- A downward drag past the threshold, from the panel or from the list at its top.
- The overlay's own exit animation is a 200 ms fade; the sheet's slide and the fade overlap.

## Known limits

- No semantics (`docs/semantics-gap.md`): the sheet has no role, no dismiss action for assistive tech.
- `enableDismissFromStartToEnd`-style direction gating does not exist here: a sheet always dismisses
  downward.
- The drag-to-collapse gesture starts wherever the press landed, so a *horizontal* drag on the panel does
  nothing (harmless) — unlike `SwipeToDismissBox`, which owns its axis and hands a vertical drag back to
  the list (`docs/swipe-to-dismiss.md`, Gesture arbitration).

## Demo

`cargo run -p winia --example bottom_sheet_demo` opens the sheet behind a control panel: an
"Open sheet" button plus two switches. `gestures enabled` off shows the fix above in the flesh — the
panel and its handle stop accepting a drag while the sheet stays up; `skip partially expanded` on
removes the half stop, so a downward drag goes straight out instead of stopping half way. The sheet's
content is deliberately taller than half the window, which is what makes the `PartiallyExpanded`
anchor exist at all.
