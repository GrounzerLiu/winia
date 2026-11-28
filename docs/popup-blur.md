# Frosted popups

A popup can blur what is behind it without any platform material: host it in the window overlay layer
(`PopupHost::InWindow`) and give its content `Modifier::backdrop_blur(radius)`. The popup is painted
on the same surface as the window content, so the modifier samples the pixels that were already
drawn underneath it. See `winia/examples/popup_blur_demo.rs`.

```rust
Popup::new(open)
    .host(PopupHost::InWindow)
    .anchor_slot(anchor)
    .build(ctx, |ctx| {
        Column::new()
            .modifier(
                Modifier::new()
                    .width(320.0)
                    .backdrop_blur(18.0)
                    .background(Color::from_argb(150, 16, 16, 20), Shape::rounded(16.0))
                    .padding(20.0),
            )
            .build(ctx, |ctx| { /* content */ });
    });
```

## What it samples

The blur runs before the node draws its own content, so the snapshot holds only what lies beneath it:
ancestors and earlier siblings. Two consequences:

- the node needs its own translucent fill, otherwise it covers the result;
- the material is the *window content*, not the desktop. Wrapping the popup in a window that shows
  the desktop through a system material is a different mechanism — see
  [window-backdrop.md](window-backdrop.md).

`radius` is the blur sigma in logical pixels; the snapshot is grown by `3 * radius` so the sample
does not touch the surface edge. `0.0` still takes the snapshot and draws it back unblurred, so
turning the effect off with a zero radius is not free — skip the modifier instead when the value is
zero and the cost matters.

## Why not a native popup

`PopupHost::Native` creates a real child window, which cannot read the owner's pixels: the blur
would have to be reproduced cross-window. Windows system backdrops do not help either — measured on
Windows 11 build 26300, `DWMWA_SYSTEMBACKDROP_TYPE` is accepted on a `WS_POPUP` window but DWM
paints a flat fill instead of the material and drops the window's per-pixel alpha. A transparent
native popup with no fill stays transparent, so it cannot carry a frosted backdrop.

## Limits

- Blur is drawn before the node's graphics layer, so the node's own `alpha`, scale and rotation do
  not apply to the blurred layer, and a rotated ancestor is approximated with translate plus scale.
- Each node with `backdrop_blur` takes one snapshot per frame. Keep the blurred node close to the
  size of what it covers instead of blurring a full-screen container.
- The snapshot is clipped to the surface, so a node that hangs off the window edge samples only the
  intersecting part.
