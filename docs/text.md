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
`font_family`, `letter_spacing` (a `TextUnit` — `sp`, or an `em` of the leaf's own size),
`text_decoration`, `align`, `line_height`, `line_height_style`, `overflow`, `soft_wrap`, `max_lines`,
`min_lines`, `auto_size`, `on_text_layout`, `style`, `background`, plus the paint-only `shadow`,
`font_feature_settings`, `draw_style`, `brush` and `text_geometry` (they ride a `TextPaint` modifier
 element, so they never re-measure).

## Layout

The paragraph is built once at measure and cached for the render pass; the measured size is
`min(max intrinsic width, constraints.maxWidth)` by `paragraph height` (the engine then clamps it into
the incoming range, which is the same as Compose's `maxIntrinsicWidth.coerceIn(minWidth, maxWidth)`).
A `textGeometricTransform` multiplies the width by `scale_x` before the clamp (the height is
untouched); a `lineHeightStyle` trim comes off the height afterwards.

The layout WIDTH follows Compose's `TextDelegate.layoutText`:

```text
width_matters   = soft_wrap || overflow.is_ellipsis()   // Clip and Visible do not count
layout_width    = if width_matters { max_width } else { infinite }   // nothing wraps at infinity
effective_lines = if !soft_wrap && overflow.is_ellipsis() { 1 } else { max_lines }
```

A nowrap ELLIPSIS therefore still lays out at the box width and as ONE line (native text layout cannot
produce several ellipses), which is what makes the `…` appear. A nowrap CLIP or `Visible` keeps laying
out at an infinite width: nothing is truncated by the paragraph itself, and the node clips it at its own content box (only `Visible` leaves clipping to an ancestor).

RTL: with no `align`, the alignment follows the layout direction (right under RTL); an explicit
`align` wins. The paragraph's own bidi resolution is Skia's.

## Behaviour

Colour priority: `color` > the merged style's colour > `LocalContentColor` (the theme's content colour,
so `with_content_color(..)` scopes colour the text) — the same chain as Compose's
`color.takeOrElse { style.color.takeOrElse { LocalContentColor.current } }`.

Defaults: `overflow = Clip`, `soft_wrap = true`, `max_lines` unset (unlimited), `min_lines(1)`,
alignment from the layout direction. `max_lines(0)` is REJECTED (`Text::build` asserts both caps are
positive and that `min_lines <= max_lines`, matching Compose's `requirePrecondition`); only the
unvalidated `text::measure_text` path can hand a zero to Skia.

**`min_lines`** — the box reserves that many line boxes even when the text is shorter. skia-safe 0.99
does not expose `set_min_lines`, so the RESULT is padded instead of the paragraph: the height is
`max(text height, min_lines * line height)` and the glyphs stay at the top, which is what Compose's
`minLines` does anyway. The pair is VALIDATED the way Compose validates it
(`HeightInLinesModifier.kt:454-461`, from `BasicText.kt:102`): both caps must be positive and
`min_lines <= max_lines`, otherwise `build` panics with Compose's own wording. Before that check,
`max_lines(0)` measured 44 x 27 (one line, `did_exceed_max_lines` set) and `min_lines(5)` with
`max_lines(2)` reserved three lines the cap can never fill — hence the assert.

**`on_text_layout`** — called after every MEASURE (never from an intrinsic probe, which is a question
about a hypothetical width) with a `TextLayoutResult`: `size` (the box, `min_lines` included),
`text_size` (the paragraph's own size), `line_count`, `did_exceed_max_lines`,
`first_baseline`/`last_baseline`, `did_overflow_width`/`did_overflow_height` and
`has_visual_overflow()`. A leaf measured at candidate widths (a wrapped block, the `autoSize` search)
fires it once per pass — the LAST call is the frame's layout.

`on_text_layout_with_layout(|result, layout| ..)` is the richer sink: the same snapshot plus a
`TextLayout` borrow of the paragraph, which answers Compose's per-line queries — `get_line_for_offset`,
`get_line_range` (`getLineStart`/`getLineEnd`), `get_bounding_box`, `get_cursor_rect`,
`get_offset_for_position`, `get_word_boundary`. They take and return CONTENT byte offsets. Why they
are not fields of `TextLayoutResult`: `Paragraph` holds a Skia `RefHandle`, which is not `Send`
(measured — `assert_send::<Paragraph>()` does not compile), so a `Copy` snapshot cannot carry one. CAVEAT: TRIGGERING A RECOMPOSITION from inside the callback (writing a
`State` it also reads) trips the materialiser in this build — the demo fixture panicked with
`[dup-key]` until the write was removed — so treat it as an observer, or write a `Backchannel`.

The payload compares callbacks by IDENTITY, so a closure built per rebuild is a changed parameter and
the leaf re-measures. A caller that rebuilds often (a text beside an animation, say) creates the sink
once and uses `on_text_layout_shared(Arc<..>)`: the parameter then compares equal and the slot can
Skip. Compose gets the same effect from `rememberUpdatedState`.

**`text_decoration`** — `TextDecoration::UNDERLINE`, `LINE_THROUGH`, or the two combined with `|`;
the plain leaf draws it now (the `TextStyle` flags used to reach only `RichText`).

**`font_family`** — a family NAME handed to Skia's font collection. `RichText` takes a list of names;
`Text` takes one.

Currency: the leaf declares its visual inputs with `ctx.changed` (21 of them, one per resolved field),
AND every parameter rides the payload: `TextContent` carries 17 fields and `TextPaint` the six
paint-only ones, so a
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

The open defects, the deviations below and the missing API are tracked row-by-row — with the
measurements behind each and the reason a row is still open — in
[`docs/text-review.md`](text-review.md). This table is the short version.

| Compose | winia |
| --- | --- |
| `autoSize: TextAutoSize?` | `Text::auto_size(min, max)` — the same fit search, run at measure time |
| `TextOverflow.StartEllipsis` / `MiddleEllipsis` | supported by truncating the string before layout (Skia only ellipsizes the end); the paragraph holds the truncated text and carries a `DisplaySpan`, so a hit test is mapped back to content offsets (`Paragraph::content_offset` / `display_offset`) before the caret or the selection sees it. `RichText` supports them too: the content is truncated first and the spans and inline-drawable ranges are remapped onto the shortened string, so a tail selection still addresses the right characters |
| `TextStyle.lineHeightStyle` | `Text::line_height_style(LineHeightStyle { alignment, trim })`: the extra leading is split by the alignment and trimmed from the box ends. Three limits: `Proportional` behaves as `Center` while every line shares one style; `RichText` does not expose it (its inline runs would each need their own leading); and the extra is measured against the SHAPED first run, so a CJK line reads 20.32 against a Latin line's 21.28 at 16 sp and the same style shifts it about 0.5 px differently |
| `TextStyle.textGeometricTransform` | `Text::text_geometry(TextGeometricTransform { scale_x, skew_x })`: a paint matrix plus the same mapping in the caret, hit test, IME window and selection rectangles |
| `TextStyle.shadow` | `Text::shadow(TextShadow { color, offset, blur })`; paint-only, so the measured box does not change |
| `TextStyle.fontSynthesis` | no knob, and none needed: Skia synthesises a missing face (measured on `SimSun`: 108.0 px regular vs 111.8 px faux bold); it cannot be turned off |
| `TextStyle.fontFeatureSettings` | `Text::font_feature_settings("tnum", "liga off", …)`, parsed into Skia features in order |
| `color: ColorProducer` overload | deliberately absent (D-5): a colour value re-measures the leaf once, and the draw-time channel is the glyph foreground paint that `drawStyle`/`brush` use |
| `minLines` is passed to Skia | padding the RESULT height (skia-safe exposes no `set_min_lines`) — same box, same top alignment |
| `TextStyle.merge` takes the child's decoration when specified | matches: the decoration is `Option<TextDecoration>`, so a child can clear an inherited one |
| density, `Density.fontScale` | `Sp` scales with the platform text size, `Dp` does not: `Density::from_density_and_font_scale(density, font_scale)` multiplies `Sp` only, and every window builds its density through `runtime::density::density_with_system_font_scale`. The factor comes from `HKCU\Software\Microsoft\Accessibility\TextScaleFactor` (`runtime::platform::font_scale`), re-read once per 250 ms on the frame path (`refresh_system_font_scale`) because winit forwards `WM_SETTINGCHANGE` only for the THEME — a text-size change never reaches the event loop. A value outside 0.5..=4.0 is rejected rather than installed. Hold `sp` and `dp` apart in components: only font sizes are `Sp` |

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
| a zero line cap, and `min_lines` above `max_lines`, are rejected | `components::text::tests::a_zero_line_cap_is_rejected`, `::min_lines_above_max_lines_is_rejected` |
| a shared `on_text_layout` sink compares equal, a fresh closure does not | `components::text::tests::a_shared_callback_lets_the_leaf_skip` |
| a decoration reaches the payload, and draws under the glyphs | `components::text::tests::a_child_style_can_clear_an_inherited_decoration`, `render_snapshot.rs::a_text_decoration_draws_under_the_glyphs` |

Demo: `cargo run -p winia --example text_demo`. Fixture scenario: `text`.
