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
calls `on_dismiss_request` when it has finished sliding out — a callback that runs when the animation
COMPLETES, not when it starts (an earlier version fired on the anchor change, so the parent set
`visible = false` and the overlay vanished mid-slide).

| Token | Value | Source |
|---|---|---|
| Top corner radius | 28 dp | `BottomSheetDefaults.ExpandedShape`, `SHEET_TOP_CORNER_RADIUS` |
| Container colour | `surface_container_low` | `SheetBottomTokens.DockedModalContainerColor` |
| Container elevation | 1 dp | `DockedModalContainerElevation` |
| Max width | 640 dp, centred | `sheetMaxWidth` |
| `skip_partially_expanded` | off | drops the half-expanded anchor, so a swipe down goes straight out |

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

## Shape: square when it fills the window

M3 squares the sheet's top corners once it is expanded. The radius therefore depends on state that changes
while the sheet slides, and the panel's surface is PAINTED per frame by `SheetPanelNode` rather than
composed into the modifier chain — the sheet expands by animating its offset, which recomposes nothing, so
a build-time shape kept the radius it was built with until an unrelated compose re-ran the closure
(measured on the demo: the corners arrived square only after a list scroll, and stayed square after
collapsing). The clip and the shadow keep a build-time shape, driven by the sheet's VALUE (a tracked read,
one recompose per anchor crossing).

`the_bottom_sheet_panel_is_square_when_expanded_and_rounded_again_when_not` reads the pixels just inside and
clear of the corner: a rounded corner shows what is behind the panel, a square one shows the panel. The
panel only squares up when its height reaches the window's, so a fixture for it needs content taller than
the window.

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
  nothing (harmless) — unlike `SwipeToDismissBox`, whose axis handling is its own open question
  (`docs/swipe-to-dismiss-plan.md`, difference 1).
