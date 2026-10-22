# Draw and layer extras: caches, inner shadows, render effects

Three drawing refinements added for Compose parity.

## `Modifier::draw_with_cache`

Compose's `drawWithCache`. The build closure runs once per `(width, height, density)` and returns
any value; the draw closure runs on every paint with that cached value:

```ignore
modifier.draw_with_cache(
    |scope| make_shader(scope.width, scope.height),      // built once per size
    |shader, canvas, rect| paint_with(canvas, shader, rect),
)
```

What it is for: a Skia shader, a paragraph, a geometry path — construction that should not repeat
every frame. The cache lives in the draw node, so it survives frames while the node is reused; a size
or density change rebuilds it. The closures are not part of `element_param_eq` (callbacks are
compare-equal, the same rule as the other draw APIs).

## `Modifier::inner_shadow(shape, color, blur, offset_x, offset_y)`

An inner shadow drawn ABOVE the node's content (Compose's appearance API draws it after the shape
and its children) and clipped to `shape`. The renderer fills the box into an offscreen layer and
erases the offset shape with `DstOut` plus a blurred mask filter, which leaves a soft rim on the
inside and a hard edge where the shape clips it. `blur` is the mask sigma in logical pixels;
`offset` moves the cut-out, putting the shadow on one side. Several `inner_shadow` elements stack in
chain order.

Not ported: Compose's newer `Shadow`/`Shadows` value type and its multi-shadow interpolation — the
modifier takes explicit parameters.

## `graphicsLayer`: `compositingStrategy` and `renderEffect`

`GraphicsLayerParams` gained two fields:

- `compositing: CompositingStrategy` — `Auto` (the default; a layer only appears when alpha, a color
  filter or a render effect needs one), `Offscreen` (always composite through an offscreen layer),
  and `ModulateAlpha` (accepted for API parity; winia has no per-draw alpha modulation, so it
  behaves like `Auto`).
- `render_effect: Option<RenderEffect>` — only `RenderEffect::Blur { radius }` is ported. It is set
  as the save-layer paint's image filter, so the whole layer (content and children) is blurred;
  `radius` is the Gaussian sigma in logical pixels.

Both travel through `Dyn<GraphicsLayerParams>` like every other layer parameter, so they can be
animated at paint time with `.graphics_layer(closure)`.

Not ported: color-filter chains as render effects, and `graphicsLayer { shape = ... }` (use
`Modifier::clip(shape)`, which clips the same content).

## Elevation compression

`graphicsLayer.shadowElevation` is not passed to Skia unchanged. The renderer maps it through

```text
z = (K + 1) · e / (K + e),   K = 16
```

before it becomes the shadow's Z plane (`render::paint::compressed_elevation`). The curve is zero at
zero, fixes f(1) = 1, is monotonic and approaches K+1 = 17, so the smallest step keeps its size and
larger ones stop growing: 1 dp maps to 1.0, 3 to 2.68, 6 to 4.64, 8 to 5.67, 12 to 7.29, 16 to 8.5,
24 to 10.2.

Why: Skia's spot blur and offset scale with `z/(600-z)`, i.e. roughly linearly through the 1-24 dp
range, while Material's token ladder (1/3/6/8/12) already assumes diminishing visual steps. Without
the curve a 12 dp shadow looks three times as deep as a 3 dp one instead of a step up.

This is a deliberate deviation from Compose's physical Z. It lives in ONE place so every consumer —
`Modifier::shadow`, `Surface`, Card, Button, FAB — gets the same curve; do not compress again in a
component. The configured ambient/spot colours keep their alpha: the curve changes the shadow's
geometry (offset, blur, scale), not its opacity.
