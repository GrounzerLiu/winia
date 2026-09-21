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

Compose's `calculateAnchors`: `Open at 0`, `Closed at ∓sheetWidth` — **negative in LTR**
(the sheet leaves through the left edge), **positive in RTL**, where the drawer docks at
the right. The host `Stack` aligns the sheet to the leading edge, so one offset convention
covers both directions and `drag_delta` needs no sign flip at the call site (`DrawerState::update_anchors(width, rtl)`
is where the sign lives).

The slide is a **layout offset** (`Modifier::absolute_offset`), not a graphics-layer
translation, for the same reason the bottom sheet uses one: placement participates in hit
testing, so a closed drawer parked off the edge cannot be clicked and an open one is hit
where it is drawn. The offset is state-driven, so a frame of the slide re-lays-out without
recomposing.

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
the offset with `get()` — the group (and nothing else) re-enters while the drawer moves,
and when the offset reaches the closed anchor the group composes nothing at all. Pinned
by `scrim_is_present_only_while_the_drawer_is_out` (exactly one node appears and
disappears) and verified on a real window: 38 nodes / 0 click-catchers closed, 39 / 1 open,
38 / 0 after the scrim click.

## Deviations from Compose (deliberate, all degradations)

- **Gestures.** Compose hangs a horizontal `anchoredDraggable` on the whole drawer box, so a
  swipe anywhere drags it; winia does the same (`on_drag` on the host `Stack`) and relies on
  the main tree's innermost-gesture-wins rule (`app.rs` `inner_component_drag`): a down over
  a scroll container never starts the drawer's drag, and an inner draggable (slider, switch)
  keeps its own. Edge-swipe-to-open from the window border is not a separate gesture in
  either implementation.
- **Escape does not close the drawer.** winia's Escape handling lives in the overlay path in
  `app.rs`, which only main-tree overlays reach; the drawer is in-tree and has no key focus of
  its own. Close it from the scrim, a gesture, or `DrawerState::close()` — e.g. from a
  hamburger button.
- **No suspending API.** `open`/`close` push a tween; Compose's coroutine-based
  `animateTo`/`AnchorDraggable` suspending forms do not exist here.
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
scrim presence, and the placement end-to-end through a real compose + layout in both
directions.
