# RangeSlider component (material3 aligned)

> Branch: `range-slider` (split from v2)
> Counterpart: material3 `RangeSlider` / `RangeSliderState` / `RangeSliderLogic`
> Source read: `androidx-main` `compose/material3/.../Slider.kt` (the file that carries both sliders)
> M3 spec: https://m3.material.io/components/sliders/specs

## 1. API

```rust
RangeSlider::new((0.2, 0.8))                     // RangeValue or a (start, end) tuple
    .on_value_change(|v: RangeValue| {})         // every step of a drag / tap / key
    .on_value_change_finished(|| {})             // drag end / tap / keyboard KeyUp
    .value_range(min: f32, max: f32)             // 0..1 by default
    .steps(i32)                                  // > 0 discrete, 0 continuous
    .enabled(bool)
    .colors(SliderColors)                        // the same color set as Slider
    .interaction_sources(start, end)             // per-thumb hoist
    .modifier(Modifier)
    .build(ctx);
```

`RangeValue { start, end }` is the stand-in for Compose's `ClosedFloatingPointRange<Float>`; it has
no ordering invariant, and the component normalises (`RangeSlider::new((0.8, 0.2))` renders as
`(0.2, 0.8)`, as Compose's constructor does with `minOf` / `maxOf`).

`RangeThumb { Start, End }` names a thumb; `nearest_thumb`, `range_with_moved_thumb` and
`thumb_center_x` are the pure rules the gestures use, public so a caller can predict them.

## 2. Defaults

The tokens are the slider's (`SLIDER_TRACK_HEIGHT` 16, `SLIDER_THUMB_WIDTH` 4,
`SLIDER_THUMB_HEIGHT` 44, `SLIDER_TRACK_INSIDE_CORNER` 2, `SLIDER_THUMB_GAP` 6,
`SLIDER_TICK_SIZE` 4, `SLIDER_TOUCH_HEIGHT` 48) and so are the colors
(`SliderDefaults::slider_colors`): primary thumbs, primary active track, secondaryContainer
inactive track, and the crossed tick colors (a tick on the active track is secondaryContainer, one
on the inactive track is primary). Compose reads the same `SliderTokens` for `RangeSlider`, which is
why nothing new is introduced here.

## 3. Implementation

Both sliders draw through one function, `slider::draw_track`, so the range variant is the
two-thumbed version of the single one instead of a second reading of the spec. `Slider` calls it
with one thumb and `single_sided = true` (everything left of its thumb is the active track); the
range calls it with two thumbs sorted by value and `single_sided = false`.

Segments, each taking the full-round corner of a track end it reaches and the 2 dp inside corner of
one it faces:

| Segment | Span | Color |
|---|---|---|
| left | `[track_left, start_pos - gap]` | inactive (the active track for a single slider) |
| active | `[start_pos + gap, end_pos - gap]` | primary |
| right | `[end_pos + gap, track_right]` | inactive |

The active segment never reaches a track end: it runs between the thumbs, one gap short of each, and
takes the 2 dp inside corner at both ends. A single slider's active track is the one exception —
everything left of its thumb is active, so it starts at `track_left` and takes the full-round corner
there (`activeTrackStart = 0f` in Compose). The practical effect is that a thumb reads as the end of
the fill and the pixels past it stay clear, single slider and range alike, which is what
`a_range_at_an_end_leaves_the_end_clear` pins.

A segment shorter than the round end it owns is not drawn, and neither is its stop indicator (which
is what the guard `left_seg_end > track_left + threshold` computes, with `threshold = corner`). The
corner is required with AND without ticks: Compose's `Track` uses its default
`enableCornerShrinking = false` for both sliders (the shrinking behaviour belongs to a non-default
overload this component does not use), so `!enableCornerShrinking || tickFractions.isNotEmpty()` is
true either way. An earlier revision of `slider::draw_track_body` dropped the corner when a track had
no ticks and drew sub-corner slivers near the ends that the reference does not —
`slider_tail_near_max_is_left_clear` now pins the correct behaviour for the single slider.

Ticks (`steps + 2` dots) sit on the inset axis `corner + (w - 2 × corner) × fraction`, so an end
value's thumb lands exactly on its stop; a tick a thumb would cover is skipped.

Gestures:

- A press resolves the **nearer** thumb (`nearest_thumb`), which then owns the whole gesture: the
  same `RangeThumb` answers the press, the drag and the release, so a drag never swaps thumbs
  halfway. A tie goes to the start thumb only when it sits right of the press — the port of
  `RangeSliderLogic.compareOffsets` plus the press gesture's tie-break.
- The thumbs cannot cross: `range_with_moved_thumb` clamps the start thumb at the end thumb's value
  and vice versa, then into the range.
- Press and drag both jump the resolved thumb to the pointer (`value_at_x`), like `Slider` does.
- Each thumb has its own interaction source and halves in width while ITS gesture runs.

Keyboard and focus — Compose's model, one focus stop per thumb:

- The nesting is: the component's root fills the line and carries the caller's modifier; its single
  child is the TRACK, which carries the pointer gestures AND draws the body; the track's children are
  the two thumbs. The gestures sit on the drawing node on purpose — a gesture callback's coordinates
  are local to the node the gesture resolved, so a caller's `padding` cannot shift the pointer axis
  away from the drawn one (two earlier arrangements got that wrong; `a_caller_padding_does_not_shift_the_pointer_axis`
  pins it, and the padding band stays inert as it does in Compose).
- Each thumb is its own `focusable` node with its own `on_key_event`. Tab / Shift+Tab move between the
  two thumbs (`focus_next` / `focus_prev` in tree order), and the arrow keys move the FOCUSED one — 1%
  of the range without `steps`, one tick with them, PageUp/PageDown `(actualSteps / 10).clamp(1, 10)` steps
  — which is one tick when `steps` is small — and Home/End converge toward the OTHER thumb or the
  range's end (`Home` on the end thumb collapses onto the start value, `End` on the start thumb onto
  the end value).
- A press does **not** move focus, and neither does a drag start: a click must not take the keyboard
  from wherever it was — the rule `clicking_an_overlay_button_does_not_steal_focus` states, and what
  the plain `Slider` does too. The keyboard reaches a thumb through Tab alone, which is also how
  Compose behaves (its press/drag modifier requests no focus).
- Each thumb draws its own focus ring around its own capsule; the track node draws segments, ticks
  and stop indicators only.

## 4. Tests

- Pure rules: `nearest_thumb_picks_the_nearer_thumb` (including both tie cases),
  `a_thumb_cannot_cross_the_other`, `thumb_center_matches_the_drawing_axis`.
- Pixels: `range_slider_renders_two_thumbs_and_the_active_middle` (both thumbs, the active middle,
  and the two thumb gaps), `a_range_at_an_end_leaves_the_end_clear`,
  `range_steps_color_ticks_inside_the_range`, `disabled_range_uses_the_disabled_palette`.
- Gestures through the tree: `pressing_jumps_only_the_nearer_thumb`,
  `a_press_on_the_far_side_moves_the_other_thumb`,
  `a_drag_keeps_the_thumb_it_resolved_at_press` (the drag stays on its thumb and clamps at the
  other one).
- Skip correctness: `range_slider_track_node_key_covers_all_visual_params` (every visual field must
  change the draw node's key, `track_width` and `focus_alpha` must not) and
  `range_thumb_node_key_covers_all_visual_params` (a thumb's key covers its colors, the halved width
  and its own interaction source, which is what its ring comes from).
- Layout: `both_thumbs_are_placed_on_the_value_axis` — the root's single child is the track, the
  track fills the line and its two children are centred on `thumb_center_x`, the axis both the
  drawing and the hit test use.
- The pointer axis under a caller's padding: `a_caller_padding_does_not_shift_the_pointer_axis` —
  with `padding(16)` pressing the DRAWN thumb must not move it (this is the regression the structure
  note in §3 describes).
- Focus plumbing: `each_thumb_is_its_own_focus_target_with_its_own_keys` — both thumbs carry a
  `focusable` and their own `on_key_event`, and neither the root nor the track carries one (no third
  focus stop).
- Real window: `range_slider_drags_the_thumb_the_press_resolved` and
  `range_slider_keyboard_moves_the_focused_thumb` (`tests/ui_test.rs`, fixture
  `fixture_range_slider`). The keyboard test never presses: it Tabs into the component (the start
  thumb takes focus), steps with an arrow, Tabs again for the end thumb and steps that one. Falsified
  by dropping the thumbs' `focusable` — Tab then focuses nothing, so the test fails at
  `tag_is_focused`. The drag test was falsified by making `nearest_thumb` always answer `Start` (the
  end-thumb drag then moves the start thumb).

## 5. Differences from Compose

1. **No `RangeSliderState`.** The API is the shape of Compose's older value-based overload
   (`value`, `onValueChange`); the value stays a prop. The state object exists in Compose mostly for
   `Modifier.sliderSemantics` and the `startThumb` / `endThumb` / `track` slots, none of which winia
   has yet.
2. **One value→pixel axis, always inset.** winia maps a value to
   `corner + (w - 2 × corner) × fraction` for ticks AND for continuous sliders, so a thumb at an end
   stops on the stop indicator. Compose uses the plain `w × fraction` when there are no ticks (the
   thumb then hangs half off the track's end) and switches to the inset axis with ticks — except at
   the first/last step, where it goes back to the plain mapping. This is the single slider's
   pre-existing rule; the range variant inherits it for consistency, and the payoff is that both
   sliders share one drawing function.
3. **At an end, the fill stops at the thumb.** This is Compose's rule, not a deviation
   (`activeTrackStart = sliderValueStart + startGap` for a range, `0f` for a single slider), and an
   earlier version of this component got it wrong by filling to the track's end when a thumb sat on
   it, which made the range stick out past its own thumbs. What differs from Compose is only where
   the gutter falls, and that follows from (2): Compose's end thumb sits ON the track edge (half of
   it hangs off), leaving one gap of clearance, while winia's end thumb sits `corner` in — 8 px at
   the default height — so there is a sliver before the thumb as well as the gap after it.
4. **Drag is absolute, not accumulated.** Compose keeps a raw pixel offset per thumb, adds deltas,
   re-syncs the other end from the value, and snaps after each step. winia maps the pointer's
   position to a value (`value_at_x`) exactly as `Slider` does. The visible difference is at a
   clamp: with winia the thumb follows the finger immediately when it turns around, with Compose the
   accumulated overshoot has to be given back first.
5. **Snapping mutates nothing.** The displayed value is snapped to the nearest tick whenever `steps`
   is set — a caller-supplied range included, which is what Compose does in `RangeSliderState`'s
   setters — but winia's build never calls `on_value_change`, so the caller's own copy of the value
   stays as it was passed. A caller that renders its own text can therefore print the unsnapped
   value next to a snapped thumb, exactly as it can in Compose (`state.startValue = value.start`
   snaps the state, not the app's variable); the first gesture writes the snapped value back. The
   single `Slider` follows the same rule.
6. **Semantics, and only that.** Two focus stops, per-thumb keys and no focus change on a press are
   all Compose's model (see §3), so the remaining gap is the accessibility tree: each Compose thumb is
   a semantics node with its own `progressBarRangeInfo` and `setProgress` / `stepBy` actions, and
   winia has no semantics layer at all.
7. **Horizontal-only drag is not enforced.** Compose cancels the press when the gesture moves more
   vertically than horizontally (so an ancestor can scroll instead); winia's gesture tracker decides
   by distance alone. Shared with `Slider`.
8. **No RTL mirroring.** Compose flips the track and the value axis under
   `LocalLayoutDirection.Rtl`; winia sliders ignore layout direction (pre-existing, shared with
   `Slider`).
9. **`on_value_change` fires even when the value is unchanged.** Compose compares and skips.
   Shared with `Slider`.
10. **No semantics.** No per-thumb `contentDescription`, no `progressBarRangeInfo`, no
    `setProgress` / `stepBy` actions — winia has no semantics layer yet.
