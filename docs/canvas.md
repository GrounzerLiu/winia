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
| `draw_text(text, x, y, font_size, color)` | a text run, shaped through the framework's own paragraph code |

## Where it lands in the pipeline

`Canvas` is a **leaf**: it draws in its own rect through `DrawWrapNode::draw_before`, on the same
layer as a background (after the modifier chain's own paints, before text and children). With no
size — from a modifier or a parent — it measures to zero, exactly like Compose's `Spacer`-backed
`Canvas`. `draw_with_content`'s `after` closure runs after children and the ripple, so it paints over
the node's content; `draw_behind`'s runs on the background layer.

## Deliberate differences from Compose

1. **No `Dp` / `Size` / `Offset` unit types.** winia has none in this layer, so the scope speaks plain
   `f32` logical pixels and takes `skia_safe::Rect` where a rectangle is needed. (The same absence is
   why `animateRectAsState` does not exist — `docs/animation-gap-analysis.md`.) Introducing those unit
   types is its own change with its own call-site migration, and this module deliberately does not
   start it.
2. **The region is the whole node rect and is not clipped.** Compose clamps a `DrawScope` to the
   drawing bounds it was handed; here clipping stays the caller's decision (`Modifier::clip`), which is
   how the framework's own nodes draw.
3. **No `drawContext` / `TextMeasurer`.** `draw_text` builds its paragraph through
   `text::build_plain_paragraph` (the same construction a `Text` node uses, so there is one shaping
   path) and lays it out to the scope's width, unwrapped beyond that.
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
| `every_primitive_draws_without_disturbing_the_rest_of_the_frame` | every primitive plus degenerate inputs (zero radius, zero-length line, empty path) render without taking the frame down |
