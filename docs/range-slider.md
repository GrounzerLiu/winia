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
is what the guard `left_seg_end > track_left + threshold` computes). The threshold follows the
source: `gap + corner` when the track has ticks, `gap` alone otherwise — look at
`slider::draw_track`'s `left_threshold` / `right_threshold`.

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

- The component is three nodes: the root carries the pointer gestures, and each thumb is its own
  `focusable` node with its own `on_key_event`. Tab / Shift+Tab move between the two thumbs
  (`focus_next` / `focus_prev` in tree order), and the arrow keys move the FOCUSED one — 1% of the
  range without `steps`, one tick with them, PageUp/PageDown ten steps, Home/End the ends.
- A press (or drag start) hands focus to the thumb it resolved (`FocusRequester::request_focus`), so
  the keyboard follows the mouse. Compose leaves that to the platform; a superset, not a gap.
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
- Layout: `both_thumbs_are_placed_on_the_value_axis` — the row is [track, start thumb, end thumb],
  the track fills it and each thumb is centred on `thumb_center_x`, the axis both the drawing and the
  hit test use.
- Focus plumbing: `each_thumb_is_its_own_focus_target_with_its_own_keys` — both thumbs carry a
  `focusable`, an `on_key_event` and a `FocusRequester`, and the root carries none (no third stop).
- Real window: `range_slider_drags_the_thumb_the_press_resolved` and
  `range_slider_keyboard_moves_the_focused_thumb` (`tests/ui_test.rs`, fixture
  `fixture_range_slider`). The drag test was falsified by making `nearest_thumb` always answer
  `Start`; the keyboard test by removing the focus hand-off (then no thumb has focus and the first
  arrow only moves focus, so the value assertion fails). The keyboard test waits for the focus to
  land (`tag_is_focused`) before sending a key — the hand-off arrives on a later frame than the value
  change, which made it flaky once — and retries the key itself via the harness's `key_until`.

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
6. **A press focuses the thumb it resolves; Compose leaves focus to the platform.** Two focus stops
   and per-thumb keys are now Compose's model (see §3). What Compose does not do is move focus on a
   pointer press — a desktop click picks up whichever focusable node it landed on, which for a 4 px
   thumb is not the point you pressed. winia takes focus for the resolved thumb, so the keyboard
   always follows the mouse. The remaining focus gap is semantics: each Compose thumb is a semantics
   node with its own `progressBarRangeInfo` and `setProgress` / `stepBy` actions, and winia has no
   semantics layer.
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
