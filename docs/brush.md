# Brushes (gradients)

`Modifier::background(color, shape)` fills with one color. `Modifier::background_brush(brush, shape)`
fills with a gradient — Compose's `Modifier.background(brush, shape)`.

```rust
Modifier::new().background_brush(
    Brush::linear_gradient([colors.primary, colors.tertiary]).diagonal(),
    Shape::rounded(16.0),
)
```

## The API

| | |
|---|---|
| `Brush::solid(color)` | one color; the same result as `background`, useful when the choice is made at runtime |
| `Brush::linear_gradient(colors)` | left to right across the node by default |
| `Brush::radial_gradient(colors)` | outward from the centre, reaching the last color at half the node's **shorter** side |
| `Brush::sweep_gradient(colors)` | one full turn |
| `.from_to(from, to)` | linear: where the run starts and ends | 
| `.horizontal()` / `.vertical()` / `.diagonal()` | linear: the three common orientations |
| `.center(x, y)` | radial / sweep: the centre |
| `.radius(r)` | radial: how far the last color reaches, as a fraction of the shorter side |
| `.positions([..])` | where each color sits, one per color in `0.0..=1.0`, strictly increasing |
| `.tile(BrushTile::Clamp/Repeat/Mirror)` | what happens past the ends; `Clamp` by default |

Colors are `Color`s — the same type `background` takes — so a scheme's roles plug straight in. The
brush also accepts a closure (`impl Into<BrushSource>`), which is how a gradient follows animated
state: the closure runs at paint time each frame, so reading an animated `State` inside it moves the
gradient.

## Coordinates are FRACTIONS of the node's bounds — the one difference from Compose

`(0.0, 0.0)` is the node's top-left, `(1.0, 1.0)` its bottom-right, and `radius` is a fraction of the
**shorter side**. Compose instead takes absolute `Offset`s, with `Offset.Infinite` meaning "the
bounds".

Fractions are what a reusable component needs. A card highlight written as "top-left to
bottom-right" fills any card; an absolute offset would need the card's measured size at build time
and would break the moment it is measured differently (a resize, a different font scale, a shared
element in flight). A caller that genuinely wants absolute pixels divides by a size it already knows:

```rust
// Compose's `Brush.linearGradient(colors, start = Offset(0f, 0f), end = Offset(size.width, 24f))`
// becomes, with `w` and `h` already in hand:
Brush::linear_gradient(colors).from_to((0.0, 0.0), (1.0, 24.0 / h))
```

The `radius` convention follows: half the shorter side reaches the nearest pair of edges, so a
"spotlight" gradient looks the same on a tall card and a wide one. `docs/brush.md`'s numbers are what
the tests assert; the reason is that `min(w, h)` is the only scale that does not stretch with aspect
ratio.

## Sweep: a full turn, clockwise, from 3 o'clock

`Brush::sweep_gradient` always covers the whole turn — there is no start/end angle to set, matching
Compose. The direction and origin come from Skia (`skia/include/effects/SkGradient.h`,
`SkSweepGradient`): 0° at the +x axis, clockwise, the CSS `conic-gradient` convention.

Worth knowing when reading the code: **Skia's sweep angles are degrees**, and its range is
`(startAngle, endAngle)` with `endAngle` corresponding to `pos == 1`. An early version of the
renderer passed the range in a unit the wrapper did not expect, which produced two clamped
half-planes instead of a conic gradient; the pixel tests in `render_tests`/`render_snapshot.rs` state
the direction, so that cannot regress silently.

## Tile modes only show where the gradient's SPAN ends

A gradient's span is its `from`→`to` line, and it maps onto the node — so a brush built without
`from_to` already fills the node, and `Repeat`/`Mirror` have nothing left to tile. To get a banded
fill, shorten the span:

```rust
// The ramp covers the left half; the right half repeats it (Repeat), turns around (Mirror), or holds
// the last color (Clamp).
Brush::linear_gradient([a, b]).from_to((0.0, 0.5), (0.5, 0.5)).tile(BrushTile::Repeat)
```

Measured on a 100-wide node with that span, sampling the red channel at x = 0/…/90 with stops
red→blue: `Clamp [252, 201, 150, 99, 48, 0, 0, 0, 0, 0]`, `Repeat [252, 201, 150, 99, 48, 252, 201,
150, 99, 48]`, `Mirror [252, 201, 150, 99, 48, 3, 54, 105, 156, 207]`. Those three rows are the
assertion in `a_tile_mode_repeats_or_mirrors_past_the_gradient`.

## How it is drawn

`render.rs` has one `paint_shape` that turns a `Shape` into a draw call, shared by solid backgrounds
and brushes, so a gradient fills exactly the shapes `background` does — including the one-sided
rounded variants and the radius-morph state of a shared-element transition. The brush resolves to a
Skia shader whose points are computed from the node's rect at paint time, which is what lets one
brush definition fill nodes of different sizes.

A brush is also a *background* for everything that infers a shape from one: the focus ring and the
ripple clip look for the nearest `Background`/`BackgroundBrush`/`Border` shape, so a gradient-filled
button rings and ripples in its own shape.

Degenerate brushes paint nothing rather than panicking: no colors, or a zero-length span. Both are
caller mistakes, and a render pass is the wrong place to find out.

## Not here (and deliberate)

| Compose | why not |
|---|---|
| `Brush.linearGradient(start, end)` with absolute `Offset` | fractions instead; divide by the size you know (above). Absolute support can be added if a caller needs it for something fractions cannot express. |
| `Brush.radialGradient(center, radius, …)` with `Offset.Unspecified` / infinite radius | the centre defaults to the node's middle and the radius to half the shorter side; both are settable |
| `ShaderBrush` / `ImageShader` / `Brush` from a custom `Shader` | would leak Skia's `Shader` into the public API; nothing has needed it |
| `Brush.horizontalGradient` / `verticalGradient` | present as `.horizontal()` / `.vertical()` on any linear brush |
| Animated brush *transitions* (lerp between two brushes) | animate the colors and rebuild the brush in the closure instead — `Brush` has no `ValueAnimatable` implementation, and adding one would need a brush-shaped interpolation the framework has no other use for |
