# Text

`Text` is the plain-text leaf — Compose's material3 `Text`, resolved against the same three layers: the
call's parameters, the merged `TextStyle` (`style` over `LocalTextStyle`), and the content colour.

## Sources

| Compose | winia |
| --- | --- |
| `material3/Text.kt` (parameters, colour priority) | `components/text.rs` — `Text` + `ProvideTextStyle` |
| `TextDelegate.layoutText` (`TextDelegate.kt:154-206`, the width / `maxLines` rules) | `layout/node/measure/paragraph.rs` — `build_plain_paragraph` |
| `TextStyle` | `text/style.rs` |

## API

```rust
Text::new("hello")
    .font_size(16.0)          // Sp
    .color(Color::RED)        // beats the style and LocalContentColor
    .bold() / .italic() / .oblique()
    .letter_spacing(1.5)      // logical px
    .line_height(24.0)        // Sp
    .align(TextAlign::Center)
    .max_lines(3)
    .overflow(TextOverflow::Ellipsis)   // Clip (default) | Ellipsis | Visible
    .soft_wrap(false)
    .style(TextStyle::new().font_size(20.0))
    .background(color, shape)            // delegates to Modifier::background
    .build(ctx);
```

Colour priority: `color` > the merged style's colour > `LocalContentColor` (content colour) — the same
chain as Compose's `color.takeOrElse { style.color.takeOrElse { LocalContentColor.current } }`.
Defaults: `overflow = Clip`, `soft_wrap = true`, `max_lines = Int.MAX_VALUE` (unset here), alignment
follows the layout direction (right under RTL).

## Layout

The paragraph is built once at measure and cached for the render pass; the measured size is
`min(max intrinsic width, constraints.maxWidth)` by `paragraph height` (the engine then clamps it into
the incoming range, which is the same as Compose's `maxIntrinsicWidth.coerceIn(minWidth, maxWidth)`).

The layout WIDTH follows Compose's `TextDelegate.layoutText`:

```text
width_matters   = soft_wrap || overflow == Ellipsis
layout_width    = if width_matters { max_width } else { infinite }   // nothing wraps at infinity
effective_lines = if !soft_wrap && overflow == Ellipsis { 1 } else { max_lines }
```

A nowrap ELLIPSIS therefore still lays out at the box width and as ONE line — native text layout
cannot produce several ellipses — which is exactly what makes the `…` appear. A nowrap CLIP keeps
laying out at an infinite width: nothing is truncated by the paragraph, an ancestor clips it.

## Differences from Compose

| Compose | winia |
| --- | --- |
| `minLines` | missing (skia-safe 0.99 exposes no `set_min_lines`) |
| `onTextLayout` | missing; `Paragraph::did_exceed_max_lines()` / line count are not surfaced |
| `autoSize: TextAutoSize?` | missing |
| `fontFamily` | missing on `Text`/`TextStyle` (only `RichText` accepts families) |
| `textDecoration` | only `RichText` honours `TextStyle.underline` / `strikethrough` |
| `color: ColorProducer` overload | only a static `Color` |
| density | layout is logical pixels and the app scales the canvas by the window scale factor, so `Sp` and `Dp` are both raw here — text still scales with the display |

`max_lines(0)` is passed to Skia, which draws a single line and reports `did_exceed_max_lines` — the
same delegation Compose performs.

## Tests

| Case | Where |
| --- | --- |
| a nowrap ellipsis lays the paragraph out at the box width | `layout::node::tests::a_nowrap_ellipsis_lays_the_paragraph_out_at_the_box_width` |
| the type scale / letter spacing / max_lines measured boxes | `tests/ui_text_test.rs` (demo `text_demo`) |
| an ellipsis stops at the box, `Visible` paints past it (pixels) | `tests/ui_text_test.rs::a_nowrap_ellipsis_stops_at_the_box_and_visible_does_not` |

Demo: `cargo run -p winia --example text_demo`.
