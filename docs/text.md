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
`font_family`, `letter_spacing`, `text_decoration`, `align`, `line_height`, `overflow`, `soft_wrap`,
`max_lines`, `min_lines`, `on_text_layout`, `style`, `background`.

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

Defaults: `overflow = Clip`, `soft_wrap = true`, `max_lines` unset (unlimited), `min_lines(1)`,
alignment from the layout direction. `max_lines(0)` is passed to Skia, which draws one line and reports
`did_exceed_max_lines` — the same delegation Compose performs.

**`min_lines`** — the box reserves that many line boxes even when the text is shorter. skia-safe 0.99
does not expose `set_min_lines`, so the RESULT is padded instead of the paragraph: the height is
`max(text height, min_lines * line height)` and the glyphs stay at the top, which is what Compose's
`minLines` does anyway.

**`on_text_layout`** — called after every MEASURE (never from an intrinsic probe, which is a question
about a hypothetical width) with a `TextLayoutResult`: `size` (the box, `min_lines` included),
`text_size` (the paragraph's own size), `line_count`, `did_exceed_max_lines`, and
`has_visual_overflow()`. CAVEAT: TRIGGERING A RECOMPOSITION from inside the callback (writing a
`State` it also reads) trips the materialiser in this build — the demo fixture panicked with
`[dup-key]` until the write was removed — so treat it as an observer, or write a `Backchannel`.

**`text_decoration`** — `TextDecoration::UNDERLINE`, `LINE_THROUGH`, or the two combined with `|`;
the plain leaf draws it now (the `TextStyle` flags used to reach only `RichText`).

**`font_family`** — a family NAME handed to Skia's font collection. `RichText` takes a list of names;
`Text` takes one.

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
| `autoSize: TextAutoSize?` | missing (needs a fit-search strategy API plus a measure-time loop) |
| `color: ColorProducer` overload | only a static `Color` |
| `minLines` is passed to Skia | padding the RESULT height (skia-safe exposes no `set_min_lines`) — same box, same top alignment |
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
| `min_lines` reserves the box height | `components::text::tests::min_lines_reserves_the_box_height` and `tests/ui_text_test.rs::min_lines_reserves_the_box_height` |
| `on_text_layout` reports the layout | `components::text::tests::on_text_layout_reports_what_was_laid_out` |
| a decoration reaches the payload, and draws under the glyphs | `components::text::tests::the_payload_carries_decoration_and_family`, `render_snapshot.rs::a_text_decoration_draws_under_the_glyphs` |

Demo: `cargo run -p winia --example text_demo`. Fixture scenario: `text`.
