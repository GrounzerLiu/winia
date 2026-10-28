# Text subsystem review: findings and checklist

> Review of `winia/src/text.rs` and `winia/src/text/*` against the Compose reference in
> `reference/compose` (foundation 1.13.0-beta01, material3 1.5.0-beta01). Performed 2026-10-28 on
> branch `v2` at `e2de045`. Everything below was measured with throw-away probe tests compiled
> inside the crate (`cargo test -p winia --lib text::probe_tmp`); the probes were deleted before
> the review closed, so the tree is unchanged and no production behaviour has been fixed yet.
>
> Companion: [`docs/text.md`](text.md) (what the text stack is). This file is the work list.
>
> Convention: `- [x]` done and verified, `- [ ]` open. Every row carries its evidence and, where a
> row is deliberately left open, the reason. Keep the reason column honest — a row that is
> "won't do" needs a why, not silence.

## Status at a glance

| Bucket | Open | Fixed |
|---|---|---|
| Defects (wrong behaviour; T-9 is latent, see its row) | 2 | 7 |
| Deviations from Compose (deliberate or accepted) | 4 | 0 |
| Missing API surface | 10 | 0 |
| Investigated and ruled out | — | 5 |

T-1, T-2, T-4, T-5, T-6, T-7 and T-8 were fixed on 2026-10-28 (see their rows for the mechanism and the tests); the
rest are open. When a row is fixed, tick it, say what closed it, and leave the evidence line in place so the
next reader can re-check.

## Defects

- [x] **T-1 — `SelectionContainer` wiped the whole selection registry when it re-entered, so any
  selection silently disappeared.**
  The old code called `registrar.reset_offsets()` on every `GroupStatus::Enter`, which cleared
  `segments`; a `Text` whose parameters did not change Skips and so never re-registered.
  Measured before the fix: two texts ("hello"/"world") in one container, one State read in the
  container body — frame 0 `total=10`, selection `"helloworld"`; frame 1 onward `total=0`,
  `build_selection() == None`. Same with `.padding(scrolled)` on the container.
  Fix: `reset_offsets` is gone. Offsets are now rebuilt from the LAYOUT TREE at the end of every
  layout pass (`Composer::reindex_selection_registrars` → `SelectionRegistrar::reindex`), in
  document order, keeping segments of skipped texts and dropping only slots the tree no longer
  reports — which is also how Compose's `SelectionRegistrarImpl` gets its order. Registration
  order is no longer load-bearing, so a text inserted between two others lands in the middle
  instead of at the end.
  Tests: `selection_container::tests::a_container_reentering_with_skipped_texts_keeps_its_selection`
  (end to end through `Composer`; asserts `[10, 10]` where the old code gave `[10, 0]`),
  `test_reindex_orders_by_tree_and_drops_dead_segments`,
  `test_reindex_follows_document_order`. Commit `7ffe80f`. Full `--lib` suite: 2025 passed.

- [x] **T-2 — indices drifted past a RichText placeholder, so selection and caret on trailing text
  mapped to the wrong bytes.**
  `RichTextScope::placeholder`/`image` write the element's text into `content` and record its
  character range, but `measure_and_cache_richtext` always passed the one-byte string `" "` to
  `ParagraphBuilder::add_placeholder`, and that call advances the real-index space by the LENGTH
  of the string it is given. Every placeholder therefore shifted the real space by the wrong
  amount — including a plain `image()`, which is one U+FFFC (3 content bytes) against a 1-byte
  stand-in.
  Measured before the fix: `"ab" + image + "cd"` mapped the trailing `"cd"` to real bytes 3..5
  where the content has it at 5..7, so `get_rects_for_range(5..7)` returned an empty vec and the
  tail could not be selected at all. A multi-char placeholder drifted by its whole length.
  Fix: the measure path passes the RANGE'S OWN TEXT, so the real space advances by exactly the
  bytes the content occupies; Skia still sees one U+FFFC per element (the string is not drawn,
  only measured). No public API changed and no contract was narrowed to one character — the
  content range is whatever `RichTextScope` recorded.
  Test: `layout::node::intrinsic_tests::text_after_an_inline_element_keeps_its_real_indices`
  (fails on the old behaviour with "the text after an inline image must be addressable").
  Commit `b2bfb14`. Full `--lib` suite: 2027 passed.

- [ ] **T-3 — `RichText` has no `align` / `max_lines` / `overflow` / `soft_wrap` /
  `line_height`, and the measure path builds a default `ParagraphStyle` anyway.**
  `measure/paragraph.rs:246` is `ParagraphStyle::new()` with no fields set; `RichTextContent`
  (`modifier/element.rs`) carries only content, drawables, ranges and spans, so the parameters do
  not exist to pass. Measured: `RichText` in a 120 dp box wraps to 4 lines with no way to cap it,
  where `Text` with `max_lines(1)` reports 1 line.
  Reason open: it is an API addition (payload fields + builder + param_eq), not a bug fix.

- [x] **T-4 — `Paragraph::get_actual_text_range` converted Skia's paragraph bytes through the
  UTF-16 map.**
  The wrapper fed the result through `get_index`, which expects UTF-16 units, but this call
  returns PARAGRAPH BYTES in this skia-safe build (it has no `_utf16_offset` sibling). Measured on
  a four-character sample (two 3-byte CJK glyphs then `ab`, 8 bytes): the raw call reported `0..8`
  while the wrapper reported `0..0`, because the byte offset missed the UTF-16 map and fell back
  to 0. ASCII hid it — with `"hello"` both units are `0..5`.
  Fix: mapped through `real_index_for_paragraph_byte` (the same clamped byte-space helper T-6
  introduced), which is now also used by `get_glyph_cluster_at` — that one had the identical
  double `unwrap()` and would have panicked at the same boundary.
  Tests: `text::paragraph::paragraph_tests::an_actual_text_range_is_reported_in_content_bytes`
  (fails on the old behaviour with `left: (0, 0)` against `right: (0, 8)`), plus
  `an_actual_text_range_of_ascii_matches_its_byte_length` and
  `an_actual_text_range_stays_inside_the_content_after_a_placeholder`.
  Commit `52edf37`. Full `--lib` suite: 2035 passed.

- [x] **T-5 — `TextLayout::new(paragraph, 0)` made every caret query return the first cluster's
  position instead of failing.**
  The constructor took a `length` and used `length == 0` as an "empty text" shortcut that answered
  with cluster 0 for *any* index. Measured on `"abc"`: with `len=0` every index reported `x=0.0`,
  while the correct positions are `0.0 / 8.14 / 17.55 / 24.94`.
  Four call sites disagreed about the unit: `render/pass.rs` passed `content.len()` (bytes),
  `app/per_window/pointer.rs:331` and `:744` and `app/overlay_host/input.rs:289` passed `0`, and the
  IME path in `app/app_state/handlers.rs` passed `paragraph_byte_to_real_indices.len()` — a map
  ENTRY COUNT. So the three hit-test/IME sites all took the collapse branch.
  Fix: the field is gone — the paragraph is the only authority on its own length
  (`Paragraph::text_len`, the last real index the map knows). `TextLayout::new(paragraph)` takes
  nothing else, so the units cannot disagree again. The caret index is now clamped like Compose
  instead of returning `None`: past the end parks at the end, mid-character parks at that
  character's start (`Paragraph::paragraph_byte_for_real_index`). The first-cluster test also had
  to compare the PARAGRAPH cluster index rather than the raw offset, because a clamped
  mid-character offset used to fall into the previous-cluster branch and ask for glyph index -1.
  The hit-test walk is bounded by the paragraph's own byte length instead of the removed field,
  and its two `.unwrap()`s use the T-6 clamping helper. The public name also loses its doubled
  word: `get_closest_grapheme_cluster_cluster_at` → `get_closest_grapheme_cluster_at`.
  Tests: `text::text_layout::tests::a_caret_position_belongs_to_the_index_that_was_asked_for`
  (fails on the old behaviour with "each index must advance the caret: [0.0, 0.0, 0.0, 0.0]"),
  plus `a_caret_past_the_end_parks_at_the_end`, `a_caret_mid_character_parks_at_the_character_start`,
  `an_empty_paragraph_puts_the_caret_at_the_origin` and `a_hit_test_stays_inside_the_text`.
  Commit `f892c8e`. Full `--lib` suite: 2032 passed; `--test ui_text_test`: 7 passed.

- [x] **T-6 — `Paragraph::get_closest_glyph_cluster_at` panicked on an unmapped cluster endpoint.**
  The mapping from Skia's paragraph byte space to the real content space used two `expect(...)`
  with a formatted dump in the message, on the path `TextLayout`'s hit-testing (drag, selection,
  caret placement) runs on every pointer move.
  Skia reports a cluster in PARAGRAPH bytes; a placeholder is one U+FFFC there (3 bytes) while
  standing for a different number of content bytes, so a boundary inside such a span has no exact
  key. Probes over ASCII, CJK, emoji with ZWJ sequences and placeholders never reached it, so it
  was latent rather than live — one edit to a placeholder's length away from being live, and a
  panic there takes the whole window down instead of clamping.
  Fix: `real_index_for_paragraph_byte` — exact key when there is one, otherwise the nearest
  preceding mapped index, which is the fallback Compose's own clamping amounts to.
  Test: `text::paragraph::paragraph_tests::a_cluster_boundary_with_no_mapping_clamps_instead_of_panicking`
  pins the contract directly (a 2-byte placeholder leaves paragraph bytes 3 and 4 unmapped: they
  clamp to 2, an exact key is unchanged, past-the-end clamps to the last index, and the public
  entry point stays inside the content for every x).
  Commit `92fc7a6`. Full `--lib` suite: 2027 passed.

- [x] **T-7 — `on_text_layout` reported a size that was not constraint-clamped.**
  The callback fired inside `measure_and_cache_text`, before `measure/engine.rs:1000-1003` applied
  `constraints.constrain_*`. Measured before the fix: `.fill_max_width()` in a 300 dp box —
  callback `width=12`, node `300`.
  Correction to the original report: the padding half of this row was WRONG. Compose's
  `TextLayoutResult.size` is `constraints.constrain(...)` of the TEXT node, and
  `Modifier.padding` sits outside it, so a padded text legitimately reports `12x19` while its
  node measures `32x39`. That is what winia already did; only the constraint clamp was missing.
  Fix: `measure_and_cache_text` now returns a `TextMeasure` (size + the callback handle) and the
  text arm of `measure_node_inner` fires the callback with `final_text_size` after clamping, so
  `size` is the box the node actually takes and `text_size` stays the paragraph's own width.
  Test: `components::text::tests::on_text_layout_reports_the_constrained_size` (asserts the
  callback width equals the node's measured width, 300, and `text_size.width` stays 12); it fails
  on the old behaviour. Commit `bdba091`. Full `--lib` suite: 2025 passed.

- [x] **T-8 — `TextOverflow::Clip` drew exactly like `Visible`; neither clipped.**
  `render/pass.rs` used `overflow` only to build the paragraph (the ellipsis), never to clip.
  Measured, 40 dp box, nowrap 16 sp text on a 200 px surface: both `Clip` and `Visible` painted
  ink out to `x=176`; only `Ellipsis` stopped (`x=25`). Compose clips whenever its layout
  overflowed — `TextStringSimpleNode.kt:443-448`, `willClip = layoutCache.didOverflow` — which is
  the `Clip` and `Ellipsis` cases, while `Visible` is the documented no-clip value.
  Fix: the text branch saves the canvas and clips to the CONTENT box when the overflow value is
  not `Visible` and the paragraph actually overflows (`did_exceed_max_lines`, `longest_line`
  wider than the box, or `height` taller than it). Content box, not `paint_rect`: in Compose a
  `Modifier.padding` wraps the text node, so text must not spill into its own padding.
  Tests: `render_snapshot::text_overflow_clip_stops_ink_at_the_box_and_visible_does_not` (fails on
  the old behaviour with "Clip must stop at the 40 px box edge, ink reached x=178") and
  `render_snapshot::text_overflow_ellipsis_stays_inside_its_box`. Visually re-checked from a
  rendered PNG: `Clip` stops at the box edge, `Visible` runs far past it, `Ellipsis` ends `W...`
  inside it.
  Commit `5467c12`. Full `--lib` suite: 2027 passed; `--test render_snapshot`: 31 passed;
  `--test ui_text_test` (debug-server): 7 passed; `--test ui_test`: 6 passed.

- [ ] **T-9 — `min_lines` height uses the average line height, where Compose uses the first line
  plus `(n-1) × step`.** `measure/paragraph.rs:187-195` computes
  `text_size.height / line_count * min_lines`; Compose precomputes from a same-style 1-line and
  2-line paragraph (`HeightInLinesModifier.kt:414-443`).
  Measured at one font size they coincide exactly (`16 sp`: 21 → 84 for `min_lines(4)`;
  `40 sp`: 53 / 106 → 212 both ways), and a plain `Text` cannot mix line heights (one font size per
  leaf), so **this is latent, not reachable through `Text` today**: `RichText` is the only leaf
  that could mix heights and it has no `min_lines` at all (T-3).
  Reason open: fixing it only matters once T-3 gives `RichText` the parameter; doing it now would
  add a same-style helper paragraph that nothing can exercise.

## Deviations from Compose

- [ ] **D-1 — `ProvideTextStyle` ORs decorations, so a child cannot turn an inherited underline
  off.** `components/text.rs` (`merge_text_styles`) merges `underline`/`strikethrough` with
  `||`, where `TextStyle.merge` replaces the decoration when the child specifies one. Already
  recorded in `docs/text.md` (§ Differences). Reason not fixed: the flags are `bool` here, so
  "the child said nothing" and "the child said none" are the same value — fixing it needs an
  `Option<TextDecoration>` in `TextStyle`, which is a breaking payload change across
  `param_eq` and every `ProvideTextStyle` caller.

- [ ] **D-2 — line height distributes the extra leading proportionally, not centred.**
  `measure/paragraph.rs` sets `set_height(lh/size)` + `set_height_override(true)`, which scales
  ascent/descent. Measured through `Text` at 16 sp: with `line_height(24)` the baseline is
  `19.47` in a `24.0` box (ascent comes back as `19.47` — the whole 3.0 dp of extra leading went
  above the glyphs); without a line height the baseline is `17.27` in a `21.0` box. M3's default
  `LineHeightStyle(alignment = Center, trim = None)` (`TypographyTokens.kt:329-330`) splits that
  3.0 dp evenly instead, which would put the baseline at `17.27 + 1.5 = 18.77` — arithmetic from
  the measured metrics, **not** measured against a running Compose build. Reason not fixed:
  skia-safe 0.99 exposes no line-height-style control, so centring means post-adjusting the
  paragraph origin per line drawn — a paint-path change that would also move the first-baseline
  alignment line.

- [ ] **D-3 — `TextAlign` is four values, and only `Justify` reaches the paragraph.**
  `text/style.rs:49` has `Left/Center/Right/Justify`; `measure/paragraph.rs:49-52` sets
  `para_style.set_text_align` for `Justify` only, and the other three are applied by shifting the
  paint origin (`render/pass.rs:847-856`). Measured: with `fill_max_width` the placement is
  correct (300 dp box — Center ink at 155..156, Right at 297..298); without it the paragraph is
  still laid out at the full width and shifted, so the model is "one full-width paragraph, moved",
  which cannot express Compose's per-line alignment for a narrow box.
  Reason not fixed: writing `set_text_align` for all four would double-apply the shift (Skia would
  centre inside the full-width paragraph *and* the paint origin would move).

- [ ] **D-4 — `TextOverflow` has no `StartEllipsis` / `MiddleEllipsis`, and `TextAlign` has no
  `Start` / `End`.** Compose's `isEllipsis` already covers the three ellipsis kinds
  (`LayoutUtils.kt:88-93`). Reason not fixed: both are API additions with rendering work
  (start/middle ellipsis need Skia's per-line range control, not just a flag), and nothing in the
  repo asks for them yet.

## Missing API surface

- [ ] **M-1 — no public measurer.** `build_plain_paragraph` is `pub(crate)`
  (`layout/node/measure/paragraph.rs:8`); the only drawing entry is `DrawScope::draw_text`
  (`components/draw_scope.rs:190-195`). Compose's `TextMeasurer.measure()` has no equivalent, so a
  caller that wants a size before painting has to compose a leaf. Reason open: needs a decision on
  whether the measurer takes a `TextStyle` (which is a component-layer type) or the flattened
  parameters the leaf already carries.

- [ ] **M-2 — `TextLayoutResult` is four fields.** `text/layout_result.rs` carries `size`,
  `text_size`, `line_count`, `did_exceed_max_lines`. Compose also exposes `layoutInput`, the
  `MultiParagraph`, `getLineForOffset`, `getBoundingBox`, `getWordBoundary`,
  `getOffsetForPosition`, `firstBaseline` / `lastBaseline`.
  Reason open: the paragraph itself is borrowed mutably at measure time, so handing it out from the
  callback needs an ownership decision (clone the handle? hand out an immutable view?).

- [ ] **M-3 — no `autoSize` / `TextAutoSize`.** Compose 1.13 has it on both `BasicText` and
  `Text` (`BasicText.kt:100`, `material3/Text.kt:82`). Reason not fixed: it is a fit-search loop
  over measure passes, i.e. a new subsystem rather than a parameter, and no caller in the repo
  needs it. Tracked here so it is not rediscovered as a bug.

- [ ] **M-4 — no `ColorProducer` overload.** `material3/Text.kt` has a second `Text` whose
  `color` is a producer so a frequently-changing colour does not recompose. Reason not fixed:
  winia's `TextContent` payload already compares colour by value and re-measures on change, and the
  escape hatch is a `State`-backed dynamic value; adding a producer variant before the payload
  supports dynamic evaluation would only move the recomposition.

- [ ] **M-5 — `TextUnit` has no `Em`.** `unit.rs:357-362` is `Sp` | `Px`. Reason not fixed:
  `Em` resolves against the *enclosing* font size, which the current payload does not carry (it
  stores a resolved logical pixel size). Adding the variant without that context would resolve every
  `Em` against 1.0.

- [ ] **M-6 — no `TextStyle.lineHeightStyle`, `fontSynthesis`, `fontFeatureSettings`,
  `textGeometricTransform`, `shadow`, `drawStyle`, `brush`.** See D-2 for the line-height half.
  Reason not fixed: each is a separate paint/shaping feature; `shadow` and `brush` already exist as
  `Modifier`-level effects, and the rest have no caller.

- [ ] **M-7 — `RichText` has no `AnnotatedString` equivalent.** `RichSpanStyle` is a resolved
  product; there is no span list that can be sliced, merged, re-parsed or read back
  (`getSpanStyles`). Reason not fixed: it is a data-model addition, and the current
  `resolve_spans` splitter would have to be rebuilt on top of it.

- [ ] **M-8 — rich-text spans do not participate in `param_eq`.** `modifier/param_eq.rs:166-170`
  compares content, drawable count and ranges only; its comment says the style ranges are treated as
  identical because every build rebuilds them. So a rebuild that changes only a span style is an
  equal payload and the leaf does not re-measure.
  Measured: toggling `.bold(..)` between frames leaves both the payload's `font_weight` check and
  the cached paragraph unchanged (`width` and `fonts` identical across frames).
  Reason open: comparing spans properly means `RichSpanStyle` grows a `PartialEq` over 30 fields
  including `Vec<String>` families — a per-frame comparison cost in exchange for correctness on a
  rare edit. Needs a measured decision, not a guess.

- [ ] **M-9 — `IndexBiMap::insert` does not enforce its documented precondition.**
  `text/index_bimap.rs:25-29` documents "left and right must each be monotonically increasing" and
  then pushes. `get_by_left` / `get_by_right` are binary searches, so an out-of-order insert
  silently corrupts every later lookup. Reason not fixed: a debug assertion would catch builder
  bugs, but the builder's per-character loop is the hot path at measure time; it should be a
  `debug_assert!` plus a property test rather than a release-path check.

- [ ] **M-10 — `SvgDrawable` claims thread safety it does not have.**
  `text/inline_drawable.rs:84-85` is `unsafe impl Send + Sync` over a `RefCell<svg::Dom>`. The
  comment argues `draw` only calls `render(&self)`, but `borrow()` itself panics if two threads
  draw at once — and `InlineDrawable` is `Send + Sync` by trait bound, so the promise is
  structural. Reason open: the honest fix is
  `Mutex<svg::Dom>` (or `RefCell` + `!Sync` and a container-side lock); choosing between them
  depends on whether a drawable is ever shared across windows.

## Investigated and ruled out

Recorded so the next round does not redo them.

- [x] **`SelectionRegistrar::register` offset growth is correct.** `text/selection.rs:86-103`
  advances past the old extent when a segment grows. Measured: 21 → 26 bytes, full-range selection
  returns `"a-much-longer-headingWORLD"`, no duplicated or dropped text.
- [x] **`IndexBiMap` UTF-16 ↔ byte conversion is correct.** Measured `get_index(1) = 3`,
  `get_index(2) = 6` on the same two-CJK-glyph sample, and `get_rects_for_range` with real byte
  ranges maps to the right boxes (`0..3 → (0,16)`, `6..8 → (32,49.55)`).
- [x] **Hit-testing does not panic and returns consistent byte offsets** across ASCII, CJK and ZWJ
  emoji, both with `length = 0` and with the true length (0 panics over every sweep). The
  `length` field only matters for the *caret* API (T-5), not for hit-testing.
- [x] **`get_word_boundary` clamps out-of-range offsets** (returns `len..len`) instead of
  panicking, and returns byte ranges consistent with the rest of the API.
- [x] **Inserting or removing a `Text` mid-list keeps document order** as long as the enclosing
  column re-enters (measured "AAA"/"CCC" → "AAA"/"BBB"/"CCC"; removal → "AAA"/"CCC"). The failure
  mode with a stale order is T-1, not an ordering bug of its own.

## How to re-check

The probes compiled inside `winia/src/text/` as a `#[cfg(test)]` module and were removed after
the review. To re-run any measurement, add the module back next to the facade declaration in
`winia/src/text.rs` and run:

```bash
cargo test -p winia --lib text::probe_tmp -- --nocapture --test-threads=1
```

Nothing in this document is a substitute for that: the numbers above came from those runs, and the
Compose side of a comparison is a source read of `reference/compose`, not a running Compose build.
