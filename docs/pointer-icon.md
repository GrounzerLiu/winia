# Pointer icons (mouse cursor)

`Modifier::pointer_hover_icon(icon)` and `pointer_hover_icon_override_descendants(icon)` set the
window cursor while the node (or one of its descendants) is hovered — Compose Desktop's
`Modifier.pointerHoverIcon`.

`PointerIcon` (`winia::input::PointerIcon`) is a newtype over winit's cursor enum, with the common
Compose desktop constants: `DEFAULT`, `HAND`, `TEXT`, `CROSSHAIR`, `CELL`, `MOVE`, `WAIT`,
`HELP`, `PROGRESS`, `NOT_ALLOWED`, `GRAB`, `GRABBING`, `COPY`, `ALIAS`, `NO_DROP`,
`CONTEXT_MENU`, `ALL_SCROLL`, `ZOOM_IN`, `ZOOM_OUT`, the three resize constants, and
`PointerIcon::new(winit::cursor::CursorIcon::…)` for the rest.

## Resolution

The cursor is resolved on every pointer move from the hit-test path, leaf first:

1. the deepest `pointer_hover_icon` is the nearest one;
2. an ancestor with `override_descendants` wins over everything below it — the first such ancestor
   met while walking up from the leaf (its icon is used immediately);
3. no icon on the path means the platform default arrow.

Overlays: when the pointer is over an overlay layer, that layer's tree is hit-tested for the cursor
as well; if it declares none, the default is used (the covered main tree never contributes).

## Application

`PerWindow` keeps the last applied icon and calls winit's `set_cursor` only when the resolved icon
changes; `PointerLeft` resets it to the default. The update happens on pointer movement, not on
layout changes under a stationary pointer — the same timing Compose gets from hover events.

Not ported: custom image cursors (Compose Desktop's `PointerIcon(Cursor)`), and touch input has no
cursor concept.
