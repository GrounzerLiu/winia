# Navigation drawer

M3 `ModalNavigationDrawer` parity: `winia/src/ui/navigation_drawer.rs`.
Demo: `cargo run -p winia --example navigation_drawer_demo`.

## API

```rust
ModalNavigationDrawer::new(|ctx| { /* the app shell */ })
    .drawer_state(drawer.clone())          // optional: remembered internally otherwise
    .drawer_content(|ctx| {
        ModalDrawerSheet::new(|ctx| {
            NavigationDrawerItem::text_label("Inbox", selected)
                .badge(|ctx| { Badge::new().content(|ctx| { Text::new("3").build(ctx); }).build(ctx); })
                .on_click({ clone!(drawer); move || drawer.close() })
                .build(ctx);
        })
        .build(ctx);
    })
    .build(ctx);
```

`DrawerState` carries the two anchors:

| call | Compose | notes |
|---|---|---|
| `open()` / `close()` | `DrawerState.open` / `.close` | non-suspending here — pushes the shared tween on the offset state |
| `snap_to(v)` | `snapTo` | no animation |
| `current_value()` | `currentValue` | the parked value (the underlying `settledValue`: a drag in flight does not change it) |
| `target_value()` | `targetValue` | drag target while dragging, else the nearest anchor |
| `is_open()` / `is_closed()` | `isOpen` / `isClosed` | |
| `progress()` / `peek_progress()` | `calculateFraction` | 0 parked closed, 1 parked open; the `peek` form registers no dependency (render-time reads) |
| `drag_delta(dx)` / `settle_with_velocity(v)` | `anchoredDraggable` | wire these to `Modifier::on_drag` / `on_drag_end` |
| `set_confirm_value_change(f)` | `confirmStateChange` | `false` vetoes the gesture and rolls back to the parked value |

## Anchors, placement and direction

The anchors are `Open at 0`, `Closed at ∓sheetWidth` — **negative in LTR** (the sheet leaves
through the left edge), **positive in RTL**, where the drawer docks at the right. The host
`Stack` places the sheet against the edge the direction puts it on, so one offset convention
covers both directions and `drag_delta` needs no sign flip at the call site
(`DrawerState::update_anchors(width, rtl)` is where the sign lives).

androidx gets to the same place differently, which is worth knowing before "fixing" this to
match it: `NavigationDrawer.kt` anchors `Closed at minValue` / `Open at maxValue` with
`minValue = -width` **in both directions** and takes the mirror from
`reverseDirection = isRtl` on the gesture plus RTL-aware `offset`/`placeRelative`. winia has
no reverse-direction flag on `AnchoredDraggableState`, so the sign is folded into the anchor
instead. Equivalent in effect — the RTL placement test pins 800 closed / 440 open on an
800-wide window.

The slide is a **layout offset** (`Modifier::absolute_offset`), not a graphics-layer
translation, for the same reason the bottom sheet uses one: placement participates in hit
testing, so a closed drawer parked off the edge cannot be clicked and an open one is hit
where it is drawn. The offset is state-driven, so a frame of the slide re-lays-out without
recomposing.

`update_anchors` runs on every compose and is therefore **inert unless the closed anchor
moved** (androidx guards the same way: `currentClosedAnchor != calculatedClosedAnchor`).
This is load-bearing rather than tidy: a live drag moves the offset while the parked value
stays put, so re-aligning the offset here would undo one drag frame per compose and the
drawer would not drag at all. An in-flight *animation* is not the only case to exclude —
that was the first version's guard, and the gesture silently did nothing. Pinned by
`a_drag_survives_the_next_compose`.

The sheet is `min(maximum_drawer_width, window)` wide — **logical** px. This framework's
convention is `1dp == 1 logical px` (see `Dimension::Dp`), so the dp tokens are used as-is
and `Dp::to_px` (a *physical* value) must not be applied when sizing against the window's
logical width. Pinned by `the_sheet_width_is_logical_not_physical`, which caught exactly
this at 1.5x density (the drawer filled the window).

## Tokens (`DrawerDefaults`, `NavigationDrawerTokens`)

| token | value | used for |
|---|---|---|
| `ContainerWidth` / `MaximumDrawerWidth` | 360dp | sheet width |
| `MinimumDrawerWidth` | 240dp | only reached when the maximum is configured below it |
| `ContainerShape` (`CornerLargeEnd`) | 16dp | the two corners facing the content (`Shape::RightRoundedRect` in LTR, `LeftRoundedRect` in RTL) |
| `ModalContainerColor` | `SurfaceContainerLow` | sheet surface |
| `ModalDrawerElevation` | `Level0` | no shadow by default; `ModalDrawerSheet::elevation` adds one |
| `ActiveIndicatorHeight` | 56dp | `NavigationDrawerItem` height |
| `ActiveIndicatorShape` (`CornerFull`) | pill | item indicator |
| `ActiveIndicatorColor` / `ActiveIconColor` / `ActiveLabelTextColor` | `SecondaryContainer` / `OnSecondaryContainer` | selected item |
| `InactiveIconColor` / `InactiveLabelTextColor` | `OnSurfaceVariant` | unselected item |
| `IconSize` | 24dp | item icon slot |
| `DrawerVelocityThreshold` | 400dp/s | `settle_with_velocity` |
| `DrawerPositionalThreshold` | 0.5 of the anchor distance | `settle_with_velocity` at low speed |

The sheet's 12dp horizontal content padding is what sizes an item's indicator to
`ActiveIndicatorWidth` (336dp = 360 − 2×12) — measured on the running demo, both numbers
appear in the layout tree exactly.

## The scrim

A full-window click-catcher painted **above the content and below the sheet**, fading with
`progress` and closing the drawer on click. Its alpha is winia's modal-scrim alpha
(110/255), the same one `open_overlay` draws for a modal, so a drawer and a dialog darken
the page by the same amount; override per drawer with `scrim_color(...)`.

The scrim's *presence* is structural, and getting it wrong is a whole-page input bug: a
full-window click-catcher left in the tree while the drawer is parked closed would swallow
every click on the page. It therefore lives in a restartable group of its own that reads
the offset with `get()`: every offset write re-enters that group, and once the offset
reaches the closed anchor the group composes nothing at all. (Precisely: the framework
marks the whole dirty *path* dirty, so the drawer's `build` and its ancestors on the path
also re-run that frame and then Skip their inner groups by parameter comparison. What is
kept out of it is the app-content subtree, which is a sibling.) Verified on a real window:
38 nodes / 0 click-catchers closed, 39 / 1 open, 38 / 0 after the scrim click.

## Gestures, precisely

`gestures_enabled` (default true) wires the drag on the **host `Stack`** and on the
**scrim**. Where a drag can actually start follows from winia's dispatch model, which
captures ONE gesture node per pointer down (the innermost with a gesture) and routes every
later move to it — Compose instead hands the moves to the outer `anchoredDraggable`
whatever child is under the finger:

| finger down on | what happens |
|---|---|
| the sheet's blank area | drags the drawer |
| the scrim (drawer open) | drags the drawer; a tap instead closes it |
| the page behind (drawer closed) | drags the drawer |
| a scroll container | the container scrolls — `inner_component_drag` skips a drag node that is an ancestor of a scroll container |
| a slider/switch | the widget drags (it is the inner component) |
| a `NavigationDrawerItem`, `Button`, … | **the child captures the down, so a swipe from there does not move the drawer** — tap or drag, the child's tracker decides |

That last row is the honest gap against Compose and the main thing to know before relying
on the gesture: a swipe that starts on a drawer row just cancels the row's tap. Dragging
from the sheet's own surface (its padding, its blank lower half) and from the scrim works.

## Deviations from Compose (deliberate, all degradations)

- **Gestures.** See the table above: the drag is wired on the host and the scrim, but a down
  on a child with its own gesture (a drawer row, a button) is captured by that child, so a
  swipe starting there does not move the drawer. Edge-swipe-to-open from the window border
  is not a separate gesture in either implementation.
- **Escape does not close the drawer.** winia's Escape handling lives in the overlay path in
  `app.rs`, which only main-tree overlays reach; the drawer is in-tree and has no key focus of
  its own. Close it from the scrim, a gesture, or `DrawerState::close()` — e.g. from a
  hamburger button.
- **No suspending API.** `open`/`close` push a tween; Compose's coroutine-based
  `animateTo`/`AnchorDraggable` suspending forms do not exist here.
- **No `window_insets` on the sheet.** androidx's `ModalDrawerSheet` applies
  `DrawerDefaults.windowInsets`; winia's rails and scaffold expose `WindowInsets`, this sheet
  does not (inert on desktop, where they are zero — a follow-up, not a decision).
- **The item height is fixed (56dp)** where androidx uses `heightIn(min = ActiveIndicatorHeight)`,
  so a two-line label or an oversized badge is squeezed rather than growing the row.
- **The sheet applies the 12dp horizontal content padding itself.** androidx exposes it as
  `DrawerDefaults.ItemPadding` and leaves applying it to the caller; winia's default keeps a
  bare `NavigationDrawerItem` sized to `ActiveIndicatorWidth` (336dp) without the caller
  having to know the token. `ModalDrawerSheet::content_padding(false)` opts out.
- **`DismissibleNavigationDrawer` / `PermanentNavigationDrawer` are not implemented** — only
  the modal form, `ModalDrawerSheet` and `NavigationDrawerItem`.
- **The indicator color animation** uses a 150ms `EaseOutCubic` tween; Compose's
  `animateColorAsState` default is a spring.
- `NavigationDrawerItem` has no `enabled` parameter, matching Compose (which has none
  either) — gate the callback on the caller's side if a row must be inert.

## Tests

`winia/src/ui/navigation_drawer.rs` — anchors and progress (including the RTL sign flip),
dragging in both directions, clamping, velocity-vs-position settle, the drawer's own 400dp/s
threshold (fails if `DrawerState::new` stops setting it), the `confirmStateChange` veto
(compared against the same gesture without a veto), width resolution, resize behaviour,
scrim presence, the sheet width at 1.5x density, and the placement end-to-end through a real
compose + layout in both directions. `a_drag_survives_the_next_compose` is the one that
covers the compose following a drag frame — the shape of bug a state-level test cannot see,
and the one this component shipped with until an external review caught it. Tests that push
animations hold `crate::animation::tests::TEST_SERIAL` and clear the registry on drop
(AGENTS.md).

Verified on a real window over the debug server: the open/close cycle (38 nodes / 0
click-catchers → 39 / 1 → 38 / 0), an item click selecting and closing, and both gestures —
dragging the sheet's blank area (sheet at −190 mid-drag, settling to −360 closed) and
dragging the scrim (−160 mid-drag) — with screenshots confirming the trailing-edge corner
radius, the selected pill, the dimmed scrim and the un-dimmed sheet.
