# Text

`Text` is the plain-text leaf — Compose's material3 `Text`, resolved against the same three layers: the
call's parameters, the merged `TextStyle` (`style` over `LocalTextStyle`), and the content colour.

## Sources

| Compose | winia |
| --- | --- |
| `material3/Text.kt` (parameters, colour priority) | `components/text.rs` — `Text` + `ProvideTextStyle` |
| `TextDelegate.layoutText` (`TextDelegate.kt:154-206`, the width / `maxLines` rules) | `layout/node/measure/paragraph.rs` — `build_plain_paragraph` |
| `TextStyle`, `TextAlign`, `TextOverflow` | `text/style.rs` |
| `BasicText` (the leaf this compiles to) | `ctx.start_leaf` + the `TextContent` modifier element |

## Variants

One constructor; everything else is an option that returns `Self`.

| Constructor | Size | When |
| --- | --- | --- |
| `Text::new(content)` | the enclosing `LocalTextStyle` (14 sp outside any `ProvideTextStyle`) | the default |
| `.style(TextStyle)` | the style merged OVER `LocalTextStyle` | a caller that carries a typography token around |
| `ProvideTextStyle(style, ctx, content)` | the style merged into the subtree's local | a component giving its labels one token (button, chip, snackbar) |

Options, in Compose's order: `color`, `font_size`, `font_weight` / `bold`, `italic` / `oblique`,
`letter_spacing`, `align`, `line_height`, `overflow`, `soft_wrap`, `max_lines`, `style`, `background`.

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

A nowrap ELLIPSIS therefore still lays out at the box width and as ONE line (native text layout cannot
produce several ellipses), which is what makes the `…` appear. A nowrap CLIP or `Visible` keeps laying
out at an infinite width: nothing is truncated by the paragraph itself, and an ancestor clips it.

RTL: with no `align`, the alignment follows the layout direction (right under RTL); an explicit
`align` wins. The paragraph's own bidi resolution is Skia's.

## Behaviour

Colour priority: `color` > the merged style's colour > `LocalContentColor` (the theme's content colour,
so `with_content_color(..)` scopes colour the text) — the same chain as Compose's
`color.takeOrElse { style.color.takeOrElse { LocalContentColor.current } }`.

Defaults: `overflow = Clip`, `soft_wrap = true`, `max_lines` unset (unlimited), alignment from the
layout direction. `max_lines(0)` is passed to Skia, which draws one line and reports
`did_exceed_max_lines` — the same delegation Compose performs.

Currency: the leaf declares nothing but its DATA SIGNATURE (the content) with `ctx.changed`; every other
parameter rides the `TextContent` modifier element, whose `param_eq` compares all eleven fields, so a
changed size / colour / cap re-enters the leaf (measured: a state-driven font size 20 -> 40 re-measures
27 -> 53 dp without touching the content).

Modifier order: the payload goes AFTER the caller's chain (the third exception to §2.3 of
`docs/component-authoring.md`) because the element IS the drawing — a caller's `background` must stay
beneath the glyphs. Pinned by
`render_snapshot.rs::a_caller_background_paints_beneath_the_text`.

Semantics: nothing is declared on the node; the tree reads its name out of the `TextContent` element
(`semantics/tree.rs:178`), so a text is a named element with no role — see the `Text`, `RichText` row in
`docs/semantics.md`.

## Differences from Compose

| Compose | winia |
| --- | --- |
| `minLines` | missing (skia-safe 0.99 exposes no `set_min_lines`) |
| `onTextLayout` | missing; `Paragraph::did_exceed_max_lines()` / the line count are not surfaced |
| `autoSize: TextAutoSize?` | missing |
| `fontFamily` | missing on `Text`/`TextStyle` (only `RichText` accepts families) |
| `textDecoration` | only `RichText` honours `TextStyle.underline` / `strikethrough` |
| `color: ColorProducer` overload | only a static `Color` |
| `TextStyle.merge` takes the child's decoration when specified | `ProvideTextStyle` ORs `underline`/`strikethrough` with the parent, so a child cannot turn an inherited decoration off |
| density | layout is logical pixels and the app scales the canvas by the window scale factor, so `Sp` and `Dp` are both raw here — text still scales with the display |

## Tests

| Case | Where |
| --- | --- |
| a nowrap ellipsis lays the paragraph out at the box width | `layout::node::tests::a_nowrap_ellipsis_lays_the_paragraph_out_at_the_box_width` |
| the style chain (ProvideTextStyle -> Text builder) and the measurement | `components::text::tests` |
| the caller's background stays beneath the glyphs | `render_snapshot.rs::a_caller_background_paints_beneath_the_text` |
| the type scale / letter spacing / `max_lines` measured boxes | `tests/ui_text_test.rs` (fixture `text`) |
| an ellipsis stops at the box, `Visible` paints past it (pixels) | `tests/ui_text_test.rs::a_nowrap_ellipsis_stops_at_the_box_and_visible_does_not` |
| a centred nowrap ellipsis stays inside its box (pixels) | `tests/ui_text_test.rs::a_centred_nowrap_ellipsis_stays_inside_its_box` |

Demo: `cargo run -p winia --example text_demo`. Fixture scenario: `text`.
