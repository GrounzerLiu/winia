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
