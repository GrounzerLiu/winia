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

| Bucket | Open | Fixed | Blocked by Skia |
|---|---|---|---|
| Defects (wrong behaviour) | 0 | 9 | 0 |
| Deviations from Compose (deliberate or accepted) | 0 | 6 | 0 |
| Missing API surface | 0 | 10 | 0 |
| Investigated and ruled out | — | 5 | — |

Fixed on 2026-10-28: T-1..T-8 and T-10 (defects), D-2, D-3 and the align half of D-4, and M-1, M-2,
M-8, M-9, M-10. 2026-10-29: M-3 (autoSize), M-5 (`TextUnit::Em`), M-7 (`AnnotatedString`), the
shadow / features / drawStyle / brush half of M-6, the ellipsis half of D-4, and M-4 closed as D-5. See each row for the mechanism and the test, and "Second pass" below for the index plus the
items `skia-safe 0.99` cannot express at all.

When a row is fixed, tick it, say what closed it, and leave the evidence line in place so the next
reader can re-check.

## Seventh pass (2026-10-29): the three RichText gaps from the sixth pass

The previous answer listed three things `RichText` still lacked. All three are closed.

| Item | What was wrong | What it is now | Test |
| --- | --- | --- | --- |
| R-4 | `RichText`'s `letter_spacing` was a bare `f32`, so a span could not say `0.1.em` | `TextUnit` end to end: `RichTextScope::letter_spacing`, `RichTextBuilder::letter_spacing`, `SpanStyle::letter_spacing` and `RichSpanStyle` (which stores the RESOLVED pixels, resolved against THAT span's own font size). `TextUnit` also gained a `Default` (zero sp) so a style that says nothing compares cleanly | `components::rich_text::tests::a_span_letter_spacing_em_resolves_against_the_span_size` (0.1 em = 2 px at 20 px, 4 px at 40 px) |
| R-5 | `RichText` had no `line_height_style` | `RichText::line_height_style(LineHeightStyle)` carried in `RichTextContent`, compared by `param_eq`, trimmed in the measure path through the SAME `line_geometry_from` the leaf uses, and applied at paint as `paint_dy` (the selection rectangles follow it) | `components::rich_text::tests::a_rich_line_height_style_trims_its_box` |
| R-6 | start/middle ellipsis on rich text fell back to an end ellipsis, because truncating the string would have mis-aligned the spans and the inline drawables | the CONTENT is truncated before the spans are resolved, and both the spans and the drawable ranges are remapped by `remap_rich_range` (a head range stays, a tail range shifts by `head + 1`, one inside the dropped gap disappears). The paragraph records the same `DisplaySpan` the plain leaf does, so hit tests still map back to content offsets. A run that already FITS is left untouched — the search used to append a marker to a fitting string (measured: a 20-character run came back 23 bytes) | `rich_text::tests::a_rich_start_ellipsis_keeps_the_tail_addressable`, `layout::node::measure::paragraph::truncate_tests::the_rich_truncation_remap_moves_spans_and_drawables` |

Note on R-6: the binary search in the rich path works in CHARACTERS (the span/drawable indices are character based), while the plain leaf's works in bytes — the two are deliberately separate, because one of them has offsets to keep aligned and the other does not.
## Sixth pass (2026-10-29): accessibility text scale, and `letterSpacing` as a `TextUnit`

Both were listed as still missing in the previous answer; this pass closed them.

| Item | What was wrong | What it is now | Test |
| --- | --- | --- | --- |
| N-1 | `Density.font_scale` was hard-wired to 1.0 (`unit.rs` said so), so `sp` behaved exactly like `dp` and the Windows "text size" slider changed nothing | `Density::from_density_and_font_scale(density, font_scale)`; `TextUnit::Sp` multiplies by it; every window builds its density through `runtime::density::density_with_system_font_scale`; the factor is read from `HKCU\Software\Microsoft\Accessibility\TextScaleFactor` (`runtime::platform::font_scale`, measured: `150` → 1.5) and re-read once per 250 ms on the frame path (`refresh_system_font_scale`) because winit forwards `WM_SETTINGCHANGE` only for the THEME. Values outside 0.5..=4.0 are rejected | `unit::tests::font_scale_multiplies_sp_only`, `a_nonsense_text_scale_is_rejected`, `components::text::tests::a_system_text_scale_grows_the_type_and_not_the_layout` (14 sp at scale 2 grows the paragraph, the dp-pinned 40x20 box does not), `the_installed_text_scale_reaches_a_measured_text`, `runtime::platform::tests` |
| N-2 | `TextStyle.letter_spacing` was a bare `f32` of logical pixels, so Compose's `0.1.em` had no way to exist | `TextUnit`: `Em` resolves against the leaf's OWN font size at build time, `Sp` through the density (so it also follows N-1); `Text`/`TextStyle`/`TextExtras` keep the unit, and the text field resolves it to pixels where a style is interpolated | `components::text::tests::a_letter_spacing_unit_resolves_against_size_and_scale` (0.1 em = 2 px at 20 px type, 4 px at 40; 2 sp = 3 px at scale 1.5) |

Deliberately still open: `RichText` keeps `f32` letter spacing (its span resolver works in resolved pixels), and `line_height` stays a `TextUnit` but is not fed through `Em` from `RichText` spans.
## Fifth pass (2026-10-29): a fresh review of the whole stack

This pass re-read the module from scratch and found ONE live defect plus fourteen documentation
claims that no longer matched the code. The docs were re-checked claim by claim against the source;
rows whose row text described an unfixed state are rewritten to say what the code does now, and the
numbers that no live test pins are labelled historical.

| Row | Finding | Evidence | Status |
| --- | --- | --- | --- |
| F-1 | A paragraph truncated by start/middle ellipsis answered every hit test in DISPLAY bytes, while the caret, the selection registrar and the drag path store CONTENT offsets | measured on a 20-byte content truncated to 16: the left edge answered 0 where the content offset is 4, the right edge 16 where it is 20 | fixed (`e625a7d`): `Paragraph` carries a `DisplaySpan`, `content_offset`/`display_offset` are the two directions, all four hit sites and both highlight sites use them; tests `a_hit_test_on_a_truncated_run_maps_back_to_the_content`, `the_display_mapping_round_trips_and_collapses_the_dropped_gap` |
| F-2 | `measure_text` ignored `transform` and `line_height_style`, so the public measurer and `on_text_layout` disagreed about the box | the two are fields of `TextMeasureSpec` now | fixed (`e625a7d`); test `the_measurer_honours_the_transform_and_the_line_height_trim` |
| D-6 | `docs/text.md` said `width_matters` counted only `Ellipsis`; the code counts all three (`is_ellipsis()`) | `measure/paragraph.rs:35` | doc corrected |
| D-7 | `docs/text.md` claimed `max_lines(0)` reaches Skia; `Text::build` rejects it | `components/text.rs:298-305` | doc corrected |
| D-8 | `docs/text.md` claimed the leaf declares only the content and cited a test that does not exist | `components/text.rs:348-368`; `render_snapshot.rs:908` | doc corrected |
| D-9 | `docs/text-review.md` described D-3 / M-1 / M-2 / M-8 / M-9 / M-10 by their PRE-FIX state while ticking them, contradicting itself on T-9, and pointed at five stale `file:line` refs | the ticked rows are rewritten to the current mechanism | doc corrected |

Checked and found correct this pass: the facade re-exports, `FontWeight` clamping, `TextStyle`
merging, the `min_lines` padding rule against `HeightInLinesModifier`, the colour priority chain,
`on_text_layout` firing after the constraint clamp, the transform and line-height geometry chains,
the `SelectionRegistrar::reindex` contract, and the span resolver's split arithmetic (including the
placeholder skip at `rich_text.rs:609`).
## Third pass (2026-10-29): reviewing the review's own fixes

Every row below was found by reading the commits this file produced and then measuring; each now
has the fix plus a test that fails without it. Commit `92fe29f`.

| Row | What was wrong | Measured before the fix | Test |
| --- | --- | --- | --- |
| R-1 | A `textGeometricTransform` was invisible to the alignment math: the render branch re-laid the cached paragraph at the RAW content width (undoing the `content_w / scale_x` the measure path used) and measured the slack against the unscaled paragraph | a centred run at scale 1.5 sat at x=224 in a 300 px box (centre 150), and at 267 at scale 2 | `render_snapshot::a_scaled_run_still_aligns_to_its_box` |
| R-2 | Start/middle ellipsis truncated by CHARACTER, so it could cut a cluster | middle ellipsis over a family emoji gave `"👨\u{200D}…il"`, a dangling zero-width joiner | `layout::node::measure::paragraph::truncate_tests::truncation_never_splits_a_grapheme_cluster` |
| R-3 | Every `Text` pushed a `TextPaint` element even when all five fields were `None` — a comparison per recomposition and a slot per text node — and `text_transform` went through `text_paint`, which CLONES the brush and the feature string on every measure, paint and pointer event | structural: one element per text node, two allocations per transform read | `components::text::tests::a_plain_text_carries_no_paint_element` |

Also settled in this pass: the uncached fallback (`draw_text_with_selection`) applies the same
transform matrix as the cached path, and its overflow test is scaled like the main one's.

Re-read clean: the caret clamping in `TextLayout`, `real_index_for_paragraph_byte`, the floor-search
loops around a placeholder boundary, `SelectionRegistrar::reindex` (including a slot that left the
tree) and the `AnnotatedString` span arithmetic (merge order, `substring` rebasing, out-of-range
clamping).
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

- [x] **T-3 — `RichText` had no `align` / `max_lines` / `overflow` / `soft_wrap` / `line_height`,
  and its measure path built a bare `ParagraphStyle` anyway.**
  `RichTextContent` carried only content, drawables, ranges and spans, so the parameters did not
  even exist to pass: a rich block in a 120 dp box wrapped to 4 lines with no way to cap it, where
  `Text` with `max_lines(1)` reported 1 line.
  Fix, end to end: the payload carries `align`, `max_lines`, `overflow`, `soft_wrap`,
  `line_height`, `min_lines`; `RichText` gained the matching builder methods with Compose's
  `BasicText` defaults; `param_eq` compares all six (before this only content, the drawable count
  and the ranges were compared, so a changed cap would not have re-measured); the measure path
  applies them with the SAME rules as the plain leaf (nowrap ellipsis is a one-line ellipsis,
  nowrap lays out at an infinite width, `min_lines` pads the box, a span's own height beats the
  paragraph's); and the paint side re-lays out only when wrapping, shifts the origin for
  Center/Right (D-3) and clips an overflowing layout when it is not `Visible` (T-8).
  Tests: six measurement tests in `components::rich_text::tests` (default wraps; `max_lines(1)`
  leaves one line and a shorter box; `soft_wrap(false)` keeps one line wider than the box;
  `Ellipsis` truncates against the box; `line_height(40)` makes a one-line block 40 dp;
  `min_lines(3)` reserves three lines) plus the pixel test
  `render_snapshot::rich_text_alignment_moves_the_painted_block`. Spot-checked that the wiring is
  real: forcing `max_lines` back to `usize::MAX` makes the cap test report 3 lines, and dropping
  the line height makes it report 19 instead of 40.
  Commit `3ad3a22`. Full `--lib` suite: 2041 passed; `--test render_snapshot`: 32 passed.

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
  `app/per_window/pointer.rs:330` and `:744` and `app/overlay_host/input.rs:288` passed `0`, and the
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
  The callback fired inside `measure_and_cache_text`, before `measure/engine.rs:1028-1031` applied
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

- [x] **T-9 — `min_lines` height used the average line height; resolved as a documented deviation,
  not a defect.**
  The row originally compared `text_size.height / line_count * min_lines`
  (`measure/paragraph.rs`) against `first + (n-1) × step` and assumed the second came from the
  REAL text. Reading the source settles it: `HeightInLinesModifier.kt:414-443` builds a PROBE
  paragraph with `computeSizeForDefaultText` (a placeholder string in the paragraph's style), once
  for one line and once for two, and takes `first + lineHeight × (n - 1)`. It never looks at the
  content.
  For every leaf winia has, the two rules give the same number, and that is measured, not argued:
  `16 sp` gives 21 → 84 for `min_lines(4)` and `40 sp` gives 53 / 106 → 212, both exactly on the
  average formula.
  The one case where they could diverge is a rich block mixing font sizes, where Compose would
  reserve against the paragraph's BASE style while winia averages the lines it actually laid out.
  `RichTextContent` does not carry a base style — each span carries its own size — so reproducing
  Compose's rule there would need a payload field AND would reserve less than the content needs.
  Recorded as a deviation with the numbers above rather than changed.
  A probe attempt was written and reverted: building the same paragraph with `max_lines = 1` and
  `2` returns one line for both on short text, so the step collapses to 0 and the reservation
  vanishes (measured: `min_lines(3)` reserved 19 dp instead of 57). That is why the rule cannot be
  transplanted without the probe-text indirection Compose uses.

- [x] **T-10 — a second materialisation in the same frame cleared a node's `dirty` flag, so a
  payload-only change never reached its measure.** Found through the debug server on the text demo
  (`tools/ws_text_weight.py`, local): picking `weight Bold` flipped the segment and the summary,
  and the sample kept painting Regular. Instrumented run, one frame, same node:
  `[slot] clean=true mod_changed=true dirty=true weight=700` (compose), then
  `[mat-reuse] desc_dirty=true now_dirty=true weight=700` (first materialisation), then
  `[mat-reuse] desc_dirty=false now_dirty=false weight=700` (second compose pass in the SAME frame
  rebuilt the descriptor with `dirty=false`), then `[paint] node=20 intrinsic=595.3` on every frame.
  Root cause: `materialize` assigned `n.dirty = dirty || children_changed`, which OVERWRITES a flag an
  earlier pass in the same frame had set. A node's dirty is cleared by its measure at the end of
  layout, so it must never be cleared by a later materialisation of the same frame.
  Fix: `n.dirty = n.dirty || dirty || children_changed` (`runtime/materialize.rs:448`).
  Test: `ui_text_test::a_weight_pick_repaints_the_sample_leaf` — counts glyph pixels in the sample
  box, picks Bold, requires at least +25% (red without the fix: `regular=656 bold=656`; green with
  it). Commit 7f399b2.
## Deviations from Compose

- [x] **D-1 — `ProvideTextStyle` ORed decorations, so a child could not turn an inherited
  underline off.**
  `merge_text_styles` merged `underline`/`strikethrough` with `||`, where Compose's
  `TextStyle.merge` REPLACES the decoration when the child specifies one. The flags were `bool`,
  so "the child said nothing" and "the child said none" were the same value.
  Fixed the way the row suggested: `TextStyle` now carries `decoration: Option<TextDecoration>`
  (Compose's nullable `textDecoration`), the merge is `override_.decoration.or(base.decoration)`,
  and `Text::build` falls back to `NONE`. `text_decoration(d)` stores `Some(d)`; `.underline()`
  and `.strikethrough()` keep their additive meaning by ORing into whatever is already there.
  A RichText span's `TextStyle` replaces the span decoration the same way.
  Test: `components::text::tests::a_child_style_can_clear_an_inherited_decoration` — fails on the
  old merge with `TextDecoration { underline: true, line_through: false }` where it should be
  `false`. Full `--lib` suite: 2056 passed.

- [x] **D-2 — line height distributes the extra leading proportionally, not centred.**
  `measure/paragraph.rs` sets `set_height(lh/size)` + `set_height_override(true)`, which scales
  ascent/descent. Measured through `Text` at 16 sp: with `line_height(24)` the baseline is
  `19.47` in a `24.0` box (ascent comes back as `19.47` — the whole 3.0 dp of extra leading went
  above the glyphs); without a line height the baseline is `17.27` in a `21.0` box. M3's default
  `LineHeightStyle(alignment = Center, trim = None)` (`TypographyTokens.kt:329-330`) splits that
  3.0 dp evenly instead, which would put the baseline at `17.27 + 1.5 = 18.77` — arithmetic from
  the measured metrics, **not** measured against a running Compose build.
  Fixed: `set_half_leading(true)` IS exposed by skia-safe 0.99 (`TextStyle::set_half_leading`), so the
  plain leaf and the RichText span builder now set it whenever a line height is. Measured after: the
  baseline moves `19.47 → 18.625` in the same 24 dp box, so the residual against Compose's 18.77
  drops from 0.70 to 0.145. Skia splits by its own ascent/descent rather than by half of the naive
  difference, which is why it is close and not exact.
  Test: `text::paragraph::paragraph_tests::a_line_height_centres_the_leading` (fails without
  `set_half_leading`). Commit `2030ae8`.

- [x] **D-3 — alignment used to be a single block shift, and `TextAlign` had four values.**
  WAS: `text/style.rs` had `Left/Center/Right/Justify`; the paragraph only ever received `Justify`
  and every other value moved the paint origin, so a wrapped paragraph was shifted as one block and
  its short lines stayed left-aligned (measured: two lines in a 300 dp box, Center moved line 1 to
  95..204 and left line 2 at 94..121 where it belonged at ~134..165).
  NOW (commit `2030ae8`): six variants (`Start`/`End` added, resolved in `Text::build` against the
  layout direction). When the width is KNOWN — wrapped, or any ellipsis — `set_text_align` goes on
  the paragraph and each line aligns itself (`measure/paragraph.rs:57-67`); the paint shifts the
  origin only for a nowrap, non-ellipsizing paragraph laid out at an infinite width
  (`render/pass.rs:892-909`). Test: `render_snapshot::a_wrapped_center_aligns_the_short_line_too`.

- [x] **D-4 — `TextOverflow` had no `StartEllipsis` / `MiddleEllipsis`, and `TextAlign` had no
  `Start` / `End`.** Compose's `isEllipsis` already covers the three ellipsis kinds
  (`LayoutUtils.kt:88-93`).
  Fixed: `TextAlign::Start` / `End` resolve against the layout direction in `Text::build` and
  `RichText::build` (they never reach a paragraph). `TextOverflow::StartEllipsis` and
  `MiddleEllipsis` exist, and `TextOverflow::is_ellipsis()` is the single rule the measure and paint
  paths consult. Skia's `set_ellipsis` can only mark the END of a line, so the two new modes are
  emulated: `truncate_for_ellipsis` binary-searches the longest tail (start) or head-plus-tail
  (middle) that fits with a `…`, and lays THAT string out — the marker lands where Compose puts it,
  the box keeps its constraint width, and one line is produced. Consequence, recorded rather than
  hidden: the paragraph then holds the truncated text, so caret and selection offsets map onto what
  is shown. Rich text keeps the end ellipsis for all three modes, because truncating inside a span
  run would have to move the spans with it.
  Test: `render_snapshot::start_and_middle_ellipsis_move_the_marker_off_the_end` — all three modes
  stay inside the 120 dp box, and the start mode's leftmost ink sits to the right of the end mode's
  (it is the marker's dots, not the first letter); with the truncation disabled the two coincide
  (`start=0 end=0`) and the test fails. Demo: the overflow row shows all five modes. Commit e3b3a71.
- [x] **D-5 — no `ColorProducer` overload; colour stays a value (deliberate, closed 2026-10-29).**
  `material3/Text.kt` has a second `Text` whose `color` is a producer, so a frequently-changing
  colour does not recompose. winia's `TextContent` carries a `Color` VALUE and the payload compares
  it by field, so a colour change re-enters the leaf and re-measures — the same picture, one
  measure later, and it costs no parent recomposition either (the leaf declares the parameter).
  A producer variant would have to be evaluated while PAINTING, but Skia bakes the colour into the
  paragraph at build time (`TextStyle::set_color`), so the only shape it could take is
  "evaluate at measure" — which moves the recomposition into a re-measure and buys nothing.
  The escape hatch that does buy something is the glyph foreground paint
  (`set_foreground_paint`, used by `drawStyle`/`brush`), and a draw-time colour would ride the same
  path once the payload can carry a shader instead of a colour.
  Reopen if: the paragraph cache gains a draw-time colour channel (a shader or a paint callback),
  at which point `Text::color_source` becomes a thin wrapper over `TextPaint`.

## Missing API surface

- [x] **M-1 — there was no public measurer.** Fixed: `text::measure_text(&TextMeasureSpec)`
  (`text/measure_text.rs:60`, re-exported at `text.rs:25`) takes the FLATTENED parameters the leaf
  already carries — a `TextStyle` would need a density — and answers the same `TextLayoutResult`
  `on_text_layout` sees. Five tests in that module; it now honours `transform` and
  `line_height_style` too (`text/measure_text.rs::tests`).

- [x] **M-2 — `TextLayoutResult` used to be four fields.** Now eight: `size`, `text_size`,
  `line_count`, `did_exceed_max_lines`, `first_baseline`, `last_baseline`, `did_overflow_width`,
  `did_overflow_height` (`text/layout_result.rs:15-34`), with `has_visual_overflow()` as the or of
  the last three. Still missing on purpose: the paragraph handle itself and the per-line queries
  (`getLineForOffset`, `getBoundingBox`, `getOffsetForPosition`) — those need an ownership decision
  at measure time, and nobody has asked for them yet.

- [x] **M-3 — no `autoSize` / `TextAutoSize`.** Compose 1.13 has it on both `BasicText` and
  `Text` (`BasicText.kt:100`, `material3/Text.kt:82`).
  Fixed as a fit window rather than a strategy object: `Text::auto_size(min, max)` carries a
  `(f32, f32)` in the payload, and the text arm of `measure_node_inner` — the only place that knows
  the constraints — walks down from `max` one logical pixel at a time until the laid-out box fits
  both axes, then caches THAT paragraph. Linear rather than binary on purpose: the box height is
  not monotonic in the size once wrapping is involved.
  Test: `components::text::tests::auto_size_shrinks_the_font_until_the_text_fits` (24 sp of a long
  sentence is taller than the 40 dp box; with the window it fits and is strictly shorter).

- [x] **M-5 — `TextUnit` has no `Em`.** `unit.rs` had `Sp` | `Px` only.
  Fixed: `TextUnit::Em(f32)` exists and is resolved where the enclosing size IS known —
  `Text::build` (against the inherited `LOCAL_TEXT_STYLE` size, 14 sp when nobody set one) and
  RichText's `resolve_base` — so the payload keeps storing resolved logical pixels. An `Em` that
  somehow reaches `to_logical_px` answers its raw multiple instead of panicking; a wrong size in
  the measure path beats a crash.
  Test: `components::text::tests::an_em_font_size_resolves_against_the_inherited_size`
  (`2.em` under a 16 sp `ProvideTextStyle` is 32 logical px).

- [x] **M-6 — `TextStyle.lineHeightStyle` was missing** (the last open row of this file).
  Fixed: `LineHeightStyle { alignment, trim }` with Compose's four alignments (`Top`/`Center`/
  `Bottom`/`Proportional`) and four trims (`None`/`FirstLineTop`/`LastLineBottom`/`Both`), set through
  `Text::line_height_style(..)` or `TextStyle::line_height_style(..)`.
  How it works: Skia has no alignment/trim pair, only `set_half_leading`, so the extra leading
  (`lineHeight - fontLineHeight`) is resolved in winia and travels as three numbers — `paint_dy`,
  `trim_top`, `trim_bottom` — from `line_geometry(node, para)` (`layout/node/measure/paragraph.rs`).
  Measure subtracts the trims from the box height; the paint, the caret, the composing underline,
  the selection rectangles, the IME window and both hit tests shift by `paint_dy`.
  Measured on 16 sp text in a 24 px line (the font's own line is 21.29 px, so the extra is 2.71):
  `Top` inks 7..21, `Center` 8..22, `Bottom` 9..23, and `Trim::Both` shrinks a 24 px box to 22.
  Tests: `components::text::tests::a_line_height_style_trims_the_box_and_places_the_run` (all four
  trims, symmetry, trimmed height — reports `none=24 both=24` with the geometry disabled) and
  `render_snapshot::a_line_height_alignment_moves_the_ink_inside_the_line_box` (reports
  `top=8 centre=8 bottom=8` with it disabled). Demo: the `tx-lh-*` boxes.
  Deviations, recorded rather than hidden: `Proportional` is treated as `Center` (every line shares
  one style, so the proportional share is the same on every line), and `RichText` does not expose the
  style yet — its inline runs would each need their own leading, which the span resolver does not
  carry.

  Review of this change (fourth pass, same day):

  - The arithmetic now lives in ONE function, `line_geometry_from(line_height_px, style, natural)`,
    shared by the measured path and the uncached fallback (`draw_text_with_selection`), so the two
    cannot drift; the fallback used to ignore the style entirely.
  - `line_geometry` reads the paint element directly instead of through `text_paint`, which clones
    the brush and the feature string on every measure, paint and pointer event.
  - `natural_line_height` takes its metrics from the shaped first run, and so does Compose's
    per-line metric — EXCEPT that Compose uses the style's primary typeface. Skia answers with the
    resolved FALLBACK for a CJK string (measured at 16 sp: `Ag` 21.28 against two CJK glyphs 20.32),
    so the same style shifts a CJK line about 0.5 px differently from a Latin one. `get_style_metrics`
    answers identically (measured), so this cannot be tightened without re-resolving the typeface
    from the payload's family; recorded in `docs/text.md`.
  - Tests added by the review: `every_line_height_style_keeps_the_run_inside_its_box` (all 3x4
    combinations; measured, e.g. `Bottom/FirstLineTop` -> ink 6..22 in a 24 px box),
    `a_line_height_offset_is_taken_back_out_of_the_pointer` (the hit test at a line boundary:
    cluster 0 with the offset removed, 4 without it),
    `the_line_height_arithmetic_is_one_function` (guards: no style, no line height, a line height
    under the font), `an_empty_text_survives_a_line_height_style`, and the fixture-level
    `ui_text_test::the_line_height_styles_move_the_ink_and_trim_the_box` (`Trim::Both` shortens the
    box through the real demo, `Bottom` inks lower than `Top`).
- [x] **M-7 — `RichText` had no `AnnotatedString` equivalent.** `RichSpanStyle` is a resolved
  product, so styled text could not be stored, sliced or queried.
  Fixed: `text/annotated.rs` carries the value — `AnnotatedString { text, spans }` with
  `with_style`/`push_style`, `span_styles`, `style_at` (spans merge field by field in push order,
  like Compose), `substring` (rebases the spans that survive) and `len` in CHARACTERS. A span is a
  `SpanStyle` (colour, size, weight, slant, letter spacing, background, decoration), every field
  optional. `RichText::new().annotated(&value)` lays it out through the existing span resolver —
  `span_style_to_style` maps one span onto the internal style — and the build closure can still
  append after it, with the cursor starting at the value's length.
  Tests: `text::annotated::annotated_tests::{spans_merge_in_the_order_they_were_pushed,`
  `a_substring_rebases_the_spans_that_survive, a_range_outside_the_text_is_clamped_or_dropped}`, and
  `rich_text::tests::an_annotated_string_seeds_the_text_and_its_spans` (the styled range survives
  into the payload as `2..4` bold). Demo: the rich-text demo's `AnnotatedString` card. Commit c9cb356.
  Not done: `Text` has no `annotated` overload of its own (use `RichText::annotated`, which is the
  same node), and there is no `ParagraphStyle` half — paragraph-level fields stay on `RichText`.
- [x] **M-8 — rich-text spans did not participate in `param_eq`.** WAS: `param_eq.rs` compared
  content, drawable count and ranges only, so a rebuild that changed only a span style was an equal
  payload and the leaf did not re-measure (measured: toggling `.bold(..)` left the cached paragraph's
  `width` and `fonts` identical across frames). NOW: `param_eq.rs:189-191` compares `aspans ==
  bspans` alongside every paragraph field, and `decor::RichSpanStyle` derives `PartialEq`. The cost
  is one `Vec` comparison per rich-text leaf per frame, paid only by `RichText`.

- [x] **M-9 — `IndexBiMap::insert` did not enforce its documented precondition.** WAS: it
  documented "left and right must each be monotonically increasing" and then pushed, while
  `get_by_left` / `get_by_right` binary-search — an out-of-order insert silently corrupted every
  later lookup. NOW: two `debug_assert!`s (`text/index_bimap.rs:32-41`) plus three tests, two of
  them `#[should_panic]` (`:119-133`). Release builds pay nothing, and the builder's per-character
  loop stays a plain push.

- [x] **M-10 — `SvgDrawable` claimed thread safety it did not have.** WAS: `unsafe impl Send + Sync`
  over a `RefCell<svg::Dom>`, whose borrow flag is not atomic — two threads drawing at once was UB.
  NOW: `text/inline_drawable.rs:75` is `parking_lot::Mutex<svg::Dom>`, so the `unsafe impl` markers
  (`:85-86`) are backed by a real lock; a four-thread-by-fifty-draw test at `:153` exercises it.

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

## Second pass (2026-10-28 and 2026-10-29): what closed, what remains

Eleven rows were closed in this pass, each with a test that was first seen to fail on the old
behaviour (or, where the API did not exist at all, with a spot check that removing the wiring puts
the test back to failing). The rows themselves carry the details; this is the index.

| Row | Closed by | Test |
| --- | --- | --- |
| D-2 | `set_half_leading(true)` wherever a line height is set — baseline 19.47 → 18.625 in a 24 dp box | `paragraph_tests::a_line_height_centres_the_leading` |
| D-3 | the paragraph aligns its own lines when the width is known; the origin shift is only for nowrap | `render_snapshot::a_wrapped_center_aligns_the_short_line_too` |
| D-4 (align half) | `TextAlign::Start` / `End`, resolved against the direction in `Text::build` | `components::text::tests::start_and_end_align_resolve_against_the_direction` |
| D-4 (ellipsis half) | `StartEllipsis` / `MiddleEllipsis`, truncated before layout | `render_snapshot::start_and_middle_ellipsis_move_the_marker_off_the_end` |
| M-1 | `text::measure_text(&TextMeasureSpec) -> TextLayoutResult` | `text::measure_text::tests` (5) |
| M-2 | `first_baseline`, `last_baseline`, `did_overflow_width`, `did_overflow_height`; `has_visual_overflow` covers both axes | `text::measure_text::tests::a_nowrap_line_reports_a_width_overflow` |
| M-8 | `param_eq` compares the resolved spans | `param_eq_tests::param_eq_compares_the_rich_text_spans` |
| M-9 | two `debug_assert!`s on strict monotonicity | `text::index_bimap::tests` (3) |
| M-3 | `Text::auto_size(min, max)`: a measure-time fit search, `measure_node_inner` walks down until the box fits | `text::tests::auto_size_shrinks_the_font_until_the_text_fits` |
| M-5 | `TextUnit::Em`, resolved against the inherited size in `Text::build` / `resolve_base` | `text::tests::an_em_font_size_resolves_against_the_inherited_size` |
| M-10 | `RefCell<svg::Dom>` → `parking_lot::Mutex<svg::Dom>` | `inline_drawable::tests::an_svg_drawable_can_be_drawn_from_two_threads` |

Commit `2030ae8`. Full `--lib` suite at that commit: 2053 passed; `--test render_snapshot`: 33
passed.

### What skia-safe 0.99 does and does not give us

An earlier revision of this section listed `shadow`, `fontFeatureSettings` and the start/middle
ellipsis as blocked. That was assembled from a partial read of the binding: `TextStyle` also has
`add_shadow`, `add_font_feature` and `set_foreground_paint`, and the ellipsis can be produced by
truncating the string. All three are implemented now; what follows is the list that survived
contact with the headers.

- **Used by winia** (grep-checked): `set_color`, `set_foreground_paint` (drives `drawStyle` and
  `brush`), `add_shadow`, `add_font_feature`, `set_decoration_type`, `set_font_style`,
  `set_font_size`, `set_font_families`, `set_baseline_shift`, `set_word_spacing`, `set_height`,
  `set_height_override`, `set_half_leading`, `set_letter_spacing`, `set_locale`,
  `set_font_edging`, `set_font_hinting`, `set_subpixel`, and `ParagraphStyle::set_ellipsis` (end of
  line only — start/middle are emulated, see D-4).
- **Available in `skia-safe` 0.99 but unused** (no winia field asks for them): `set_typeface`,
  `set_font_arguments`, `set_text_baseline`, `set_shadow` (the single-shadow setter — winia uses
  `add_shadow`, which appends), and `ParagraphStyle::set_text_style`.
- **`fontSynthesis`** — no knob, and none needed: Skia synthesises a missing face itself. Measured
  with `SimSun` (no bold face): regular 108.0 px / 277 ink px → bold 111.8 px / 442 ink px, a faux
  bold; `Segoe UI` (real bold) 97.7/430 → 105.0/732. That is Compose's default `FontSynthesis.All`;
  winia simply cannot turn it off.
- **`textGeometricTransform`** (scaleX / skewX) — no hook on `TextStyle` or `ParagraphStyle`, so
  it is emulated: the measure path lays the paragraph out at `max_width / scale_x` and scales the
  reported width, the paint applies a canvas matrix, and every geometry reader (caret, both pointer
  hit tests, the IME window, the selection rectangles, the Skia underline pass) applies the same
  mapping — `render/pass.rs:919-922,991,1018`, `app/per_window/pointer.rs:338`,
  `app/overlay_host/input.rs:292`, `app/app_state/handlers.rs:991`. Done in `97bfc59`, reviewed in
  `92fe29f` (R-1 fixed a double-application in the centred case). Limitation: `skew_x` is paint-only
  and does not widen the measured box.
- **`lineHeightStyle`** — no `alignment`/`trim` pair on `TextStyle` (only the boolean
  `set_half_leading`, spent on D-2's centring), so both halves are computed in winia: `LineHeightStyle
  { alignment, trim }` resolves to `paint_dy` / `trim_top` / `trim_bottom` in
  `line_geometry_from` (`layout/node/measure/paragraph.rs:238-270`), the trims come off the box
  height (`:541-544`), and `paint_dy` moves every geometry reader. Done in `a0b6602`, reviewed in
  `4e2a4ea`. Deviations: `Proportional` behaves as `Center`, `RichText` does not expose it, and the
  extra leading is measured against the SHAPED first run (a CJK line reads 20.32 against a Latin
  line's 21.28 at 16 sp).
- **`textIndent`** — absent from both types (no `text_indent` symbol anywhere in `winia/src`; the
  skia-safe 0.99 surface was not re-checked for it).
### Deferred, not blocked

`M-7` (`AnnotatedString`) was the one of this pair that needed no missing Skia API, and it is done
(see its row). `M-4` (`ColorProducer`) is closed as the deliberate deviation **D-5**: Skia bakes the
colour into the paragraph, so a producer could only be evaluated at measure time, which moves the
recomposition into a re-measure without buying anything.

All defect rows are closed. `T-9` (the average-line-height reservation) was re-checked against
`HeightInLinesModifier.kt:414-443` and is recorded as a documented deviation instead of a defect:
Compose probes with a throwaway paragraph, and for a single style the two formulas agree exactly;
the only divergence is a `RichText` that mixes font sizes, which has no paragraph-level line height
to reserve against.
## How to re-check

The measurements in this file came from throwaway `#[cfg(test)]` modules that were removed after each
round; the numbers are HISTORICAL figures from those runs unless a live test pins them. To re-measure,
add such a module next to the facade in `winia/src/text.rs` and run
`cargo test -p winia --lib text::probe_tmp -- --nocapture --test-threads=1`.

The rows that DO pin a number live in `winia/tests/render_snapshot.rs` (`every_line_height_style_keeps_the_run_inside_its_box`,
`a_line_height_alignment_moves_the_ink_inside_the_line_box`) and `winia/tests/ui_text_test.rs`
(`the_line_height_styles_move_the_ink_and_trim_the_box`); a row that only asserts an ORDER (`top <
centre < bottom`) does not pin the numbers quoted beside it.

Every `cargo test -p winia --lib <n> passed` line above is a historical count at the commit named
beside it, not a claim about the current tree. The Compose side of a comparison is a source read of
`reference/compose`, not a running Compose build.
