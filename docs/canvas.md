# Canvas / DrawScope

> Source of truth: `winia/src/ui/draw_scope.rs`. Alignment target: Compose
> `androidx.compose.foundation.Canvas` / `androidx.compose.ui.graphics.drawscope.DrawScope`.
> Companion: `docs/modifier-node.md` (the `DrawNode` / `DrawWrapNode` extension points this is built on).

## What it is

The two public forms of "draw it myself":

```rust
// The component — occupies the space its modifier gives it:
Canvas::new()
    .modifier(Modifier::new().fill_max_width().height(80.0))
    .build(ctx, |scope| {
        let (cx, cy) = scope.center();
        scope.draw_rect(skia_safe::Rect::new(0.0, 0.0, scope.width(), scope.height()), TRACK);
        scope.draw_circle(cx, cy, 24.0, ACCENT);
    });

// The modifier forms, on any node:
Text::new("decorated")
    .modifier(draw_behind(Modifier::new().padding(6.0), |scope| {
        scope.draw_rect(scope.rect(), HIGHLIGHT);
    }))
    .build(ctx);

Text::new("banded")
    .modifier(draw_with_content(
        Modifier::new().background(BG, Shape::Rectangle),
        |_scope| {},                                  // before the content
        |scope| scope.draw_rect(TOP_BAND, RULE),      // after it (on top)
    ))
    .build(ctx);
```

`DrawScope` hands the drawing closure the node's own region and a set of primitives:

| API | Meaning |
|---|---|
| `rect()` / `size()` / `width()` / `height()` / `top_left()` / `center()` | the node's region, in logical pixels |
| `density()` | the frame's scale factor |
| `skia_canvas()` | escape hatch: the underlying `skia_safe::Canvas` |
| `draw_rect`, `draw_round_rect`, `draw_oval`, `draw_circle` | filled shapes |
| `draw_line(from, to, stroke_width, color)` | stroked line |
| `draw_path(&Path, color)`, `draw_path_paint(&Path, &Paint)` | filled path, or any paint (gradient/shader/stroke) |
| `draw_text(text, x, y, font_size, color)` | a text run, shaped through the framework's own paragraph code; returns `TextMetrics` |
| `width_dp()` / `height_dp()` / `size_dp()` / `center_dp()` | the same numbers as `Dp` (logical, NOT `to_px`) |

## Where it lands in the pipeline

`Canvas` is a **leaf**: it draws in its own rect through `DrawWrapNode::draw_before`, on the same
layer as a background (after the modifier chain's own paints, before text and children). With no
size — from a modifier or a parent — it measures to zero, exactly like Compose's `Spacer`-backed
`Canvas`. `draw_with_content`'s `after` closure runs after children and the ripple, so it paints over
the node's content; `draw_behind`'s runs on the background layer.

## Deliberate differences from Compose

1. **The scope speaks the layout coordinate system, not `Dp`.** winia HAS `Dp` (`unit::Dp`, exported
   by the prelude and accepted by `Modifier::size`, `padding`, `offset`, ...) — an earlier version of
   this file claimed otherwise and was simply wrong. What the scope hands back is `f32` logical pixels,
   because that is what `Constraints`, `measured_size` and every drawing rect in the engine are in, and
   mixing the two is the trap `Dp::to_px`'s own docs warn about: `to_px` returns PHYSICAL pixels, which
   is wrong anywhere the number meets layout geometry. The scope therefore offers `width_dp()`,
   `height_dp()`, `size_dp()` and `center_dp()`, whose contract is exactly "the same number, spelled on
   the type" (`Dp::to_logical` is the identity here). Rectangles travel as `skia_safe::Rect` rather
   than a `unit::Size` plus a `unit::Offset` because a drawing call needs one rectangle; points and
   lengths are `(f32, f32)` / `f32`.
2. **The region is the whole node rect and is not clipped.** Compose clamps a `DrawScope` to the
   drawing bounds it was handed; here clipping stays the caller's decision (`Modifier::clip`), which is
   how the framework's own nodes draw.
3. **No `TextMeasurer` object, but the measurement is returned.** `draw_text` builds its paragraph
   through `text::build_plain_paragraph` (the same construction a `Text` node uses, so there is one
   shaping path), lays it out to the scope's width, and returns `TextMetrics`:

   ```rust
   let m = scope.draw_text("label", 0.0, 0.0, 14.0, Color::BLACK);
   // centre the *next* run, or size a box around this one, from the measured width:
   scope.draw_circle(m.width() / 2.0, m.height() / 2.0, 3.0, ACCENT);
   ```
   `TextMetrics` carries `width()` / `height()` / `size()` / `rect()` / `origin()` / the `_dp` forms,
   and `paragraph()` for anything beyond that (per-line metrics, hit testing). It can also draw its own
   run again — `draw_at(scope, x, y)` and `draw_centered(scope, width, height)` — **reusing the single
   shaping pass**, so a repeated label or a centred title does not re-shape the string per call. Before
   this, a caller had to re-derive the layout or hard-code an estimate — the kind of code that drifts
   the moment the font or the string changes.
4. **No `drawImage` yet.** An image needs a public image/bitmap handle, which the framework does not
   have (icons and `Image` draw through internal paths). Use `skia_canvas()` in the meantime; a
   `draw_image` follows when an image handle exists.
5. **Text is a run, not a measured block.** Compose's `drawText(textMeasurer, …)` returns layout
   results; `draw_text` here is fire-and-draw. A caller that needs the measured size should build a
   paragraph itself (or use a `Text` node instead of drawing text).

## What it does not replace

The `Modifier::draw` closure (`ModifierElement::CustomDraw`) and the `DrawNode` extension point are
still there and are what components use internally; `Canvas` / `draw_behind` / `draw_with_content` are
the *caller-facing* shape on top of `DrawWrapNode`. A component that wants its own `node_key`
fingerprint (for the Skip decision) should keep implementing `DrawNode`/`DrawWrapNode` directly — the
closures here are anonymous and their `node_key` is the node type's name, so two different closures on
the same node type do not distinguish themselves in the Skip fingerprint. That is fine for a canvas
whose content comes from `State::peek` at render time and wrong for a node whose *static parameters*
decide what is drawn; use a struct node for the latter (the rule is in `docs/modifier-node.md`).

## Tests

`ui::draw_scope::tests` renders through a real `Composer` to a raster surface and reads pixels back —
the same pattern `modifier.rs` uses for its draw nodes:

| test | what it pins |
|---|---|
| `canvas_draws_a_circle_where_the_scope_says` | the closure's output reaches the canvas, at the node's coordinates |
| `scope_geometry_follows_the_node_not_the_window` | `size` / `center` / `rect` describe the node |
| `canvas_is_a_leaf_that_takes_the_size_its_modifier_gives_it` | zero without a size, the modifier's size with one |
| `draw_with_content_paints_over_the_nodes_own_background` | the `after` pass is on top of the node's own paint |
| `dp_accessors_carry_the_logical_numbers` | the `_dp` accessors carry the logical numbers (`to_logical` is the identity) |
| `draw_text_reports_the_run_it_drew` | the returned metrics are the run's own (two strings differ in width; origin and rect follow the call) |
| `draw_centered_lands_the_run_at_the_centred_origin` | the centring arithmetic, checked against the paragraph's own line metrics rather than against glyph shapes |
| `every_primitive_draws_without_disturbing_the_rest_of_the_frame` | every primitive plus degenerate inputs (zero radius, zero-length line, empty path) render without taking the frame down |
