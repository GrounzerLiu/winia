# Split button

winia's `SplitButtonLayout` is material3's split button — an M3 Expressive component: a leading button
that runs the primary action, a 2 dp gap, and a trailing button that opens a menu of related actions
(`m3.material.io/components/split-button`). This file records where every number and rule came from, and
what winia deliberately does differently.

## Sources, in the order they were used

| What | Where |
| --- | --- |
| Geometry per size tier | `target/compose-src/commonMain/androidx/compose/material3/tokens/SplitButton{XSmall,Small,Medium,Large,XLarge}Tokens.kt` |
| Leading icon size | `tokens/ButtonSmallTokens.kt` (`IconSize = 20.dp`), forwarded through `ButtonDefaults.iconSizeFor` |
| Checked state layer alpha | `tokens/StateTokens.kt` (`PressedStateLayerOpacity = 0.1f`) |
| Optical centring | `androidx/compose/material3/HorizontalCenterOptically.kt` (`CenterOpticallyCoefficient = 0.11f`, the correction, its clamp) |
| The morph | `androidx/compose/material3/internal/AnimatedShape.kt` (each corner radius is its own `Animatable`; the optical offset reads the ANIMATED radii) |
| Measure policy, defaults surface, shapes | `SplitButton.kt` — material3 1.5.0-alpha29 sources, extracted from `tmp/m3sources.jar` into `tmp/m3src/commonMain/androidx/compose/material3/SplitButton.kt` |

## Geometry (the token table)

`SplitButtonDefaults` keys everything by [`ButtonSize`], which is exactly material3's button-height tier
table (32/40/56/96/136 dp for XS/S/M/L/XL).

| | XSmall | Small | Medium | Large | XLarge |
| --- | --- | --- | --- | --- | --- |
| Container height | 32 | 40 | 56 | 96 | 136 |
| Inner corner | 4 | 4 | 4 | 8 | 12 |
| Inner corner, pressed | 8 | 12 | 12 | 20 | 20 |
| Leading padding (start, end) | 12, 10 | 16, 12 | 24, 24 | 48, 48 | 64, 64 |
| Trailing padding (start, end) | 13, 13 | 13, 13 | 15, 15 | 29, 29 | 43, 43 |
| Trailing icon | 22 | 22 | 26 | 38 | 50 |

The outer corners are `CornerFull` (percent 50, `OuterCornerCornerSizePercent = 50.0f`). A percent corner is
half the SHORT side, and Compose resolves it in `createOutline` against the box the shape actually paints
into — which a winia composition does not know yet, so `SplitButtonDefaults::outer_corner_size` returns
`container_height / 2` and `corner_radii` resolves `Pill`/`Circle` the same way. That is exact while the half
is at least as wide as it is tall; the tiers' heights are 32/40/56/96/136 dp against a 48 dp minimum width, so
it holds for XSmall and Small and can be off for the taller tiers with minimal content. The settled checked
shape is the token's own `Shape::Pill`, which the renderer resolves against the real box, so the end of that
morph needs no approximation.

## The measure policy

material3 measures the trailing button FIRST (it has priority for width), gives the leading button what is
left, and forces both to one height. The port keeps every step: a max-width intrinsic question for the
trailing button, `subtractConstraintSafely` for the remainder (clamping at zero rather than flipping the
constraint), a max-height intrinsic question for each button at the width it will get, then measure
trailing → leading with `min = 0` and a tight height, and place them `leading.width + spacing` apart,
centred vertically. `placeRelative` mirrors the pair under RTL, which winia reproduces by mirroring x.

This component is a direct beneficiary of the intrinsic-size protocol: before it, the policy material3
writes could not be expressed at all.

## The shape morph

- Pressed wins over checked, checked falls back to the resting shape when the set has no checked shape —
  `SplitButtonDefaults::shape_for_state` is material3's `shapeByInteraction`.
- Only the trailing button has a checked shape (`TrailingCheckedShape = CircleShape`; the stadium, in
  winia's shape terms). A checked trailing button also paints a state layer — its own shape, in the
  content colour at `PressedStateLayerOpacity` — over the container.
- The morph animates the INNER corner radius; the outer corners are constant, so one animated value is
  the whole difference.
- The content's optical offset follows the SETTLED radius (`if checked { outer } else { resting_radius }`),
  not the pressed one. material3 reads it off the animated shape (`SplitButton.kt:807-813`), which slides
  the content about 1 dp along the press morph and back out on release; the spec only ever tabulates the
  offset for the two settled states, and that slide reads as jitter, so a press does not move the content
  here (recorded as a deviation below).

## How the animation reaches each property

Both the morph and the offset are driven by the app's animation frames, but they arrive by different
routes, and the route decides whether the value is read once or every frame:

- The SHAPE is rebuilt in composition from the animated radius, so the ordinary recompose each tick
  brings is enough. Measured with the morph stretched to 2000 ms (a pixel read costs about 120 ms, more
  than the real 180 ms morph): the trailing half's painted top-row insets read `(1, 12)` at rest, `(1, 3)`
  on the frame after the tap, then `(3, 3)` as it settles — the intermediate values are visible, so the
  shape travels toward the checked one instead of snapping to it.
- The OFFSET is a LAYOUT input, and an animated layout value has to be handed over as
  `SizeValue::Dynamic` — winia evaluates that during layout and registers a layout dependency
  (`modifier.rs:62-68`; `composer.rs:2862-2870`, layout deps re-measure without recomposing). A static
  number is read during composition instead, so the layout keeps the offset it had until some unrelated
  event forces a frame. Measured on the live fixture, trailing half, menu opening: `-2 dp` while
  unselected, still `-2 dp` four hundred milliseconds after the tap, and `0 dp` only once the pointer
  moved — the reported "the icon only moves when the mouse moves over it".

## Optical centring (the spec's "menu icon offset")

`CenterOpticallyCoefficient * (avgStart - avgEnd)`, clamped into the content padding: the content moves
toward the shared gap, by an amount that grows with the difference between the outer and inner radii. The
M3 spec prints the result as the trailing button's unselected menu-icon offset (XS -1, S -1, M -2, L -3,
XL -6 dp); the formula lands within a dp of each — the spec's numbers are rounded, and the formula is the
implementation. `SplitButtonDefaults::optical_shift` clamps to the padding the content has on the side it
moves toward, so content is never pushed past its room (`HorizontalCenterOptically.kt:63`).

## What winia had to add

`Shape::Corners { top_left, top_right, bottom_right, bottom_left }` — a per-corner radius, Compose's
`RoundedCornerShape(topStart, topEnd, bottomEnd, bottomStart)` in geometric corners, because every
existing winia shape rounded a side uniformly (`RoundedRect` all four, `Right`/`LeftRoundedRect` one side,
`Pill` all four at half the short side). It is wired through the whole renderer: shadow path, fill, stroke,
clip, focus ring, ripple clip, shared-transition radii, and the contained loading indicator — all of which
matched on `Shape` exhaustively and now handle the new variant.

## Deviations and open items

- **Hover and focus do not morph the shape.** The spec page lists hovered/focused/pressed as
  shape-changing states and the tokens carry `InnerHoveredCornerCornerSize`, but material3's
  `shapeByInteraction` reads `pressed` and `checked` only. winia follows the code, as elsewhere.
- **The morph's timing is ours.** material3 animates it with the motion scheme's `DefaultEffects` spring;
  `MotionSchemeKeyTokens.kt` in the extraction lists the key without a value, so no number is invented —
  winia uses a 180 ms tween (the same house value the button's elevation animation uses).
- **No minimum interactive size.** material3 wraps the layout in `Modifier.minimumInteractiveComponentSize()`
  and provides `0.dp` for the two buttons, because the buttons' own minimum would double up. winia has no
  touch-target concept, so nothing is implemented and nothing is faked.
- **RTL is our own rule.** winia resolves the direction geometrically and hands the correction over as an
  `absolute_offset`, because a plain `offset` mirrors its x a second time under the parent's direction and
  would push both halves away from the gap there. material3 1.5.0-alpha29 does the same thing through
  `placeRelative` (`HorizontalCenterOptically.kt:65`); the copy under `target/compose-src` still shows the
  older `place()`, so the two sources disagree about where the mirror happens and winia follows the newer
  one. `the_optical_shift_points_at_the_gap_in_both_directions` measures both directions.
- **The leading half gets the same optical correction.** material3 centres the trailing half only
  (`SplitButton.kt:804-828` and `:930-954` call `horizontalCenterOptically`; the leading one at `:717-726`
  does not), while winia applies it to either role, since both have a gap side. `without_optical_shift()`
  opts a half out.
- **A plain `on_click` on the checked form runs after the toggle.** material3's checked button takes only
  `onCheckedChange`; winia accepts both, and the action used to be dropped without a word when a checked
  state was present.
- **The leading button's icon size is the plain button's.** `leadingButtonIconSizeFor` forwards
  `ButtonDefaults.iconSizeFor`, which is `ButtonSize::icon_size` in winia.
- **`SplitButtonDefaults.trailing_icon`** does not exist: material3 leaves the trigger glyph to the caller
  (its samples pass `Icons.Filled.ArrowDropDown`), so winia does too. The demo takes its glyphs from
  material3's own icon set — Material Symbols, <https://fonts.google.com/icons>, which winia bundles as
  variable fonts: `Outlined::ADD` (U+E145) and `Outlined::ARROW_DROP_DOWN` (U+E5C5), the codepoints the
  official `google/material-design-icons` tables (`font/MaterialIcons-Regular.codepoints`) map to those
  names. Nothing in the demo is a hand-copied path, which is why it sits behind the
  `material-symbols-outlined` feature (`[[example]] required-features` in `winia/Cargo.toml`, and the
  command in the demo's header). The UI fixture draws the same glyph through
  `ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH` — the published 24dp asset's own path data, now a
  public constant, so the arrow exists once in the tree instead of as a copy per fixture; the UI suite
  runs without the symbols feature, which is why the fixture uses the asset rather than the font.
- **The content's optical offset follows the settled shape's own corners — not the morph, and not the
  tokens.** material3 computes it from the shape it paints (`SplitButton.kt:807-813` passes what
  `shapeByInteraction` returns, and that wraps `rememberAnimatedShape`), so pressing a half slides its
  label and icon about a dp along the morph and back out on release. The spec only tabulates the offset for
  the two settled states — the per-size "menu icon offset when unselected" and "the icon becomes centered
  when selected" — so winia animates the offset on its own, toward the settled radii: both spec numbers are
  unchanged, the press no longer moves anything, and the checked transition still slides the icon to its
  centred position. The radii come out of the shape the half DRAWS, so a caller's own `SplitButtonShapes`
  steers the painted corners and the correction together:
  `the_offset_follows_a_custom_shape_set` measures a symmetric set leaving nothing to compensate and an
  asymmetric one moving by its own corners, `a_press_does_not_move_the_content` pins the press half of it,
  and `the_optical_shift_points_at_the_gap_in_both_directions` pins both directions.
- **The morph rebuilds the shape from all four animated corner radii, so a caller's own set morphs too and
  the checked stadium is approached rather than snapped to.** material3 rebuilds the shape from each corner
  radius the animation holds while the state change itself is instantaneous (`AnimatedShape.kt`); winia
  derives the shape from the four radii the RESOLVED shape's corners feed, which is what makes the press
  morph and the checked stadium animate — it used to run the default set through that path only, so a
  caller's own set was drawn at its resolved state throughout and snapped. Once every corner has reached
  its target the resolved shape itself is drawn, so the settled chain reads exactly like material3's.
  `the_checked_stadium_is_reached_through_the_morph` pins the mid-morph frame.
- **One source of direction.** The layout resolves it (`Modifier::layout_direction` on the pair, else
  `WiniaTheme::direction`) and pins it for the two halves through the theme scope, because the halves
  resolve their shape from the theme while the policy mirrors the placement from the modifier — a
  node-level direction used to place the pair one way and shape it the other.
  `a_node_level_direction_steers_the_placement_and_the_shapes` measures both.
- **The halves do not space the caller's content.** material3 composes the content straight into the
  halves' own `Row` with `Arrangement.Center` and no spacing (`SplitButton.kt:715-728`), unlike its
  `Button`, which wraps content in `Row(spacedBy(ButtonDefaults.IconSpacing))`. winia matches the split
  button, so a caller who wants the button's 8 dp between an icon and its label passes that spacing inside
  its own content. `the_halves_do_not_space_the_callers_content` pins it, because the optical offset's
  wrapper Row is what would quietly eat such a gap.

## API mapping

| material3 | winia |
| --- | --- |
| `SplitButtonLayout(leadingButton, trailingButton, modifier, spacing)` | `SplitButtonLayout::new().spacing(..).build(ctx, leading, trailing)` |
| `SplitButtonDefaults.LeadingButton` | `SplitButtonDefaults::leading_button(on_click)` → `LeadingButton` |
| `SplitButtonDefaults.TrailingButton(checked, onCheckedChange)` / `(onClick)` | `TrailingButton::checked(state)` / `SplitButtonDefaults::trailing_button().on_click(..)` |
| `*ContainerHeight`, `*InnerCornerSize`, `*ContentPadding`, `*TrailingButtonIconSize` | `SplitButtonDefaults::container_height / inner_corner_size / inner_corner_size_pressed / leading_content_padding / trailing_content_padding / trailing_icon_size` |
| `leadingButtonShapesFor` / `trailingButtonShapesFor` | `SplitButtonDefaults::leading_shapes` / `trailing_shapes` |
| `SplitButtonShapes` | `SplitButtonShapes { shape, pressed_shape, checked_shape }` |
| `shapeByInteraction` | `SplitButtonDefaults::shape_for_state` |

## Tests, and what was measured by turning things off

Library (24): the token numbers per size (heights, inner corners and their pressed values, paddings, icon
sizes, the 2 dp gap, `CornerFull` = height/2), the shape sets, `shape_for_state`'s ordering, the optical
shift against the spec's offsets and its clamp, and four measure-policy rules read off a real composition
(the pair is leading + gap + trailing and hugs its content, the trailing button keeps its width while the
leading one takes the remainder, both buttons share one height, RTL mirrors the pair). Three more read the
shape each button actually paints off its modifier chain: resting, pressed (state put in place before the
first composition — the interaction read is a composition dependency), and checked (stadium container plus
a state layer).

UI (`--features debug-server`, scenario `split_button`, 5 tests): the pair's rects (2 dp gap, one shared
40 dp height, the trailing button's 48 dp minimum), the trailing icon's position (nudged toward the gap by
the optical correction, measured as `0.11 * (20 - 4)` ≈ 1.76 px), and the two actions (the leading button
counts exactly once, the trailing button's checked form toggles the menu state).

Two turn-it-off measurements, both restored afterwards:

- `optical_shift` returning 0 → the library test fails with "Medium: the optical shift 0 should be near
  the spec's 2 dp" and the UI test with "the icon moves toward the gap, not away from it (shift 0)".
- the leading button measured against the FULL width instead of the remainder → "the leading button takes
  the remainder (200 vs 150)".

Baselines after this round: `cargo test -p winia --lib` 1148 passed, UI suite 77 passed, both
`cargo check` variants clean.

Later rounds added the checking side of the same story: library guards for the checked trailing half (its
content centres, and the painted shape reaches the stadium through the morph rather than in one step) and
a live UI test that reads the trailing icon's offset out of the debug tree after the tap and before any
further input. Its numbers are the measurement quoted above, and turning the layout value back into a
static number makes it fail at `-2 dp` again — the same test, so the fix is what it measures. In that
same round the morph's own frames were measured with the shape's duration stretched to 2000 ms, because a
single painted-pixel read costs about 120 ms and the real morph is 180 ms: the insets travel through
intermediate values, which a snap could not produce.
