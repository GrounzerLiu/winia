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
| Measure policy, defaults surface, shapes | `SplitButton.kt` from `androidx-main`, fetched over the network relay because the local extraction has the tokens and the internals but not this file |

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

The outer corners are `CornerFull` (percent 50, `OuterCornerCornerSizePercent = 50.0f`). A percent corner
is half the SHORT side, so for a button — always wider than it is tall, with a 48 dp minimum width — the
outer radius is `container_height / 2`. That is what `SplitButtonDefaults::outer_corner_size` returns, and
it is why the shape does not need a percent-capable corner type.

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
- The content's optical offset is computed from the ANIMATED radius, exactly as material3 reads it off
  the animated shape.

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
- **RTL is our own rule.** Compose's layout modifier writes an unmirrored `place()` offset; winia resolves
  the direction geometrically so the content always moves toward the gap. The LTR result is the spec's; the
  RTL side has not been checked against a running Compose render.
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

Library (15): the token numbers per size (heights, inner corners and their pressed values, paddings, icon
sizes, the 2 dp gap, `CornerFull` = height/2), the shape sets, `shape_for_state`'s ordering, the optical
shift against the spec's offsets and its clamp, and four measure-policy rules read off a real composition
(the pair is leading + gap + trailing and hugs its content, the trailing button keeps its width while the
leading one takes the remainder, both buttons share one height, RTL mirrors the pair). Three more read the
shape each button actually paints off its modifier chain: resting, pressed (state put in place before the
first composition — the interaction read is a composition dependency), and checked (stadium container plus
a state layer).

UI (`--features debug-server`, scenario `split_button`, 3 tests): the pair's rects (2 dp gap, one shared
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
