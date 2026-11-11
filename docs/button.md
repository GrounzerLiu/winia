# Button

M3 buttons: `winia/src/components/button.rs`.

## Sources

Compose paths are relative to
`reference/compose/material3-1.5.0-beta01/commonMain/androidx/compose/material3/`.

| Compose | Settles |
| --- | --- |
| `Button.kt` | the five public overloads (`Button`, `ElevatedButton`, `FilledTonalButton`, `OutlinedButton`, `TextButton`), `ButtonDefaults` in full, `ButtonShapes` and the press morph (`shapeByInteraction`, `:1922-1939`), and the icon/content paddings |
| `tokens/ButtonXSmallTokens.kt` … `ButtonXLargeTokens.kt` | `ContainerHeight` (32/40/56/96/136), `LeadingSpace`/`TrailingSpace` (16/16/24/48/64), `IconSize` (20/20/24/32/40), `IconLabelSpace` (8/8/8/12/16), `OutlinedOutlineWidth` (1/1/1/2/3), `PressedContainerShape` (CornerSmall/Small/Medium/Large/Large) |
| `tokens/BaselineButtonTokens.kt` | the 24 dp `LeadingSpace` a button with NO size named uses — different from Small's 16 at the same 40 dp height |
| `tokens/{Filled,Elevated,FilledTonal,Outlined,Text}ButtonTokens.kt` | per-state elevation and the five colour roles |
| `InteractiveComponentSize.kt` | `minimumInteractiveComponentSize()`, the 48 dp touch target |
| `ComposeMaterial3Flags.kt` | `isPrecisionPointerComponentSizingEnabled`, default `false`; it gates the 36 dp `MinHeight` and the 8 dp `smallVerticalPadding` alternatives, neither of which winia models |

## Variants

winia keeps ONE type and names the variants as constructors — the five overloads differ only in
defaults.

| Constructor | Compose | Container / content | Elevation | When |
| --- | --- | --- | --- | --- |
| `Button::filled()` (same as `new()`) | `Button` | `primary` / `on_primary` | `0 / 0 / 0 / 1 / 0` | the important, final action |
| `Button::elevated()` | `ElevatedButton` | `surface_container_low` / `primary` | `1 / 1 / 1 / 3 / 0` | a filled button that needs to float |
| `Button::filled_tonal()` | `FilledTonalButton` | `secondary_container` / `on_secondary_container` | `0 / 0 / 0 / 1 / 0` | middle emphasis |
| `Button::outlined()` | `OutlinedButton` | transparent / `on_surface_variant` | none | secondary action, 1 dp border |
| `Button::text()` | `TextButton` | transparent / `primary` | none | lowest emphasis |

Elevation tuples are `(default, pressed, focused, hovered, disabled)`. The Filled and Tonal defaults
are `Level0` in every state except HOVER — not all-zero, which is why they carry a layer at all.

`.style(ButtonStyle)` reaches the same place after construction; `ButtonStyle` is the enum the
constructors set.

## Layout

`build` puts the content in a centred `Row` carrying `defaultMinSize(58, 40)` plus the content
padding, then wraps the whole thing in the 48 dp touch-target node:

```
minimumInteractiveComponentSize(48)        ← own node, so the touch area survives the visual box
  └─ min_width(58) · min_height(40|size) · padding(contentPadding) · background/border
       └─ Row(center, spacing = icon_label_space)
            └─ [caller's content]
```

The wrapper exists because winia decides a size across the WHOLE chain: `min_height(40)` clamps a
48 away, in either chain order. It is `Box(propagateMinConstraints = true)`, as `Surface` is
(`Surface.kt:219`), so a parent that forces a width gets a painted box that width rather than a 48 dp
box with the visual inset inside it.

**`ButtonSize` drives five things at once.** Naming a size selects that size's tokens; naming none
keeps the baseline button:

| Size | Height | H-Padding | V-Padding | Icon | Icon gap | Outline |
| --- | --- | --- | --- | --- | --- | --- |
| XSmall | 32 | 16 | 6 | 20 | 8 | 1 |
| Small | 40 | 16 | 10 | 20 | 8 | 1 |
| Medium | 56 | 24 | 16 | 24 | 8 | 1 |
| Large | 96 | 48 | 32 | 32 | 12 | 2 |
| XLarge | 136 | 64 | 48 | 40 | 16 | 3 |
| *(none named)* | 40 | **24** | 8 | — | 8 | 1 |

The vertical padding is per size because it is what keeps the box at its `ContainerHeight`: each
value leaves exactly that size's icon height in the content box — `2 * vertical_padding + icon_size ==
container_height` for all five. One shared 8 dp did not: XSmall's 32 dp container had only 16 dp of
content room, less than the 20 dp a label or an icon needs, so the box measured **36** instead. Every
other size was already exact under both rules.

`.content_padding((s, t, e, b))` and `.min_size(w, h)` take `impl Into<Dimension>` per edge, so a
literal, a `State<f32>`, an `Animating<f32>` or a closure all work; the value is read during measure,
which costs a re-measure rather than a recomposition.

RTL: padding uses `start`/`end` and the measure pass mirrors them (`engine.rs` shifts children by
`pad_end` under `Rtl`), so a button mirrors without any component code.

## Behaviour

**Colours resolve on enabled/disabled only.** Hover, press and focus are drawn by the ripple state
layer, not painted into the container — which is M3's `ButtonColors` contract. Disabled values:

| Style | Disabled container | Disabled content |
| --- | --- | --- |
| Filled / Elevated | `on_surface` @10% | `on_surface_variant` @38% |
| Tonal | `on_surface` @12% | `on_surface` @38% |
| Outlined / Text | transparent | `on_surface_variant` @38% |

**The container morphs while pressed.** Upstream carries `ButtonShapes(shape, pressedShape)` and
interpolates the two on `MotionSchemeKeyTokens.DefaultEffects` (`Button.kt:248-252`, whose comment
reads "intentional here to prevent any bounce"). winia carries it as `Shape::PillToRounded { corner_radius, t }`:
`t = 0` is the pill, `t = 1` is `Shape::rounded(pressed_container_radius)`. The two ends are different
UNITS — a percent corner at rest, a fixed radius pressed — which is why the transition shape carries
both instead of a single number. At exactly 0 or 1 the plain shape comes back, so a button that was
never pressed carries the very shape its caller asked for. The value is read with `peek()` in the
paint closures, so pressing does not recompose the button per frame.

A caller-supplied fixed-radius `shape(..)` interpolates directly to the pressed radius; per-corner,
top-only and rectangle shapes are returned unchanged and do not morph.

**Elevation is a paint-time value.** `ButtonElevation::for_state` picks by interaction state and the
result feeds `graphics_layer` through a closure; `spec_for(from, to)` keys the tween on the
interaction PAIR (120 ms towards an interaction, 150 ms away, 120 ms when leaving hover, snap on
idle-to-idle), which is Compose's rule in `internal/Elevation.kt:50-61`. A style whose every state is
0 creates no layer at all.

**Text style carries its shape without a clip.** It has neither a background nor a border, and the
ripple plus the focus ring both infer the shape from the chain. It used `clip(shape)`, but a clip also
cuts what is drawn OUTSIDE the node — and the focus ring is stroked outside — so a Text button lost
its ring while the other styles kept theirs. A zero-width transparent `border(0.0, transparent, shape)`
states the same shape and clips nothing.

**Content and icons.** The inner `Row` applies `icon_label_space()` itself, where M3 leaves the gap
to the caller's Row. A leading icon wants `ButtonDefaults::button_with_icon_content_padding()`
(16 / 24) so it sits optically tight against the edge while the label keeps its 24 dp;
`text_button_with_icon_content_padding()` is the 12 / 16 pair. Typography: the content is wrapped in
`ProvideTextStyle(label_large)` with the resolved content colour, as Compose does.

**Styling.** `button_visual_with_styles` resolves "explicit parameter > `ComponentStyles` > theme" in
one place, and `build` declares the RESOLVED colours and border through `ctx.changed` before the
group opens — a styling that changes at runtime must re-enter, or a Clean slot keeps the label style
and border it built earlier while only the live closures update.

**Focus.** `clickable_with_source` also pushes `Focusable` and `Hoverable`, so a button is a Tab stop
by construction; `set_current_node_focus_color(theme.primary)` captures the ring colour during
composition, because the render phase can no longer read the theme. Enter and Space activate the
focused node only, without bubbling to ancestors (`app/per_window/keyboard.rs`) — deliberately
asymmetric with the pointer path, matching Compose's `clickable`.

**Accessibility.** The chain declares `role = Button` with `merge_descendants(true)` and the enabled
state, BEFORE the caller's modifier, so a caller's own declaration merges over it per field and a
disabled button keeps its role.

## Differences from Compose

- **One type, five constructors.** Upstream ships five composables; the slots, measurement rules and
  state contract are identical, so separate types would duplicate all of it.
- **No `ButtonShapes` / `ButtonDefaults.shapes()`.** The press morph exists (above) but has no public
  carrier: there is no `ButtonShapes(shape, pressedShape)` parameter and no per-size `squareShape` /
  `pressedShape` constants. `ButtonSize::pressed_container_radius()` is the whole surface.
- **`BorderStroke` is width + colour only** — no brush, gradient or dashed borders.
- **`ButtonColors` takes four colours positionally** (`ButtonColors::new`), not Compose's named
  `containerColor` / `contentColor` / `disabledContainerColor` / `disabledContentColor` copy-form,
  and there are no per-variant factories (`elevatedButtonColors` and friends); the style picks them.
- **No precision-pointer sizing.** `isPrecisionPointerComponentSizingEnabled` is `false` upstream by
  default, so its 36 dp `MinHeight` and 8 dp `smallVerticalPadding` are not modelled at all.
- **Focus ring colour is the theme's `primary`**, where M3 uses `secondary`. The colour travels
  through the desc channel, so it can become a token without moving the API.
- **`graphics_layer` transforms do not take input.** A 3D-transformed or partially transparent button
  keeps its layout hit area, as Compose's `graphicsLayer` does.
- **`TextButton`'s icon-only form does not exist**, and `IconButton` is a `IconButtonStyle` on one
  type rather than separate composables — the same shape as this component.

## Tests

```bash
cargo test -p winia --lib components::button
```

30 unit tests over the arena, plus `cargo run -p winia --example button_demo`. There is no fixture
scenario (`tests/ui_fixtures/fixture_all.rs` has no button entry) and no `ui_test.rs` coverage, so the
unit suite is what runs in CI.

What the suite pins, by area:

- **defaults** — `ButtonDefaults` values, the style's colours, `(58, 40)`, the pill
- **per-size tokens** — heights, paddings (horizontal AND vertical), icon sizes, outline widths, and
  the invariant `2 * vertical_padding + icon_size == container_height`
- **`content_padding_for` uses that padding** — asserted against the returned value, not against the
  constants, so putting the flat 8 back fails the test
- **naming a size vs not** — Small (named) takes 16 dp, an unsized button keeps the baseline 24
- **colour resolution** — enabled/disabled, and the elevation priority order
- **elevation** — the pair-keyed spec, and a hovered Filled/Tonal button actually raising the shadow
  it paints while Outlined carries no layer at all
- **press morph** — the radii are the tokens, the container shape moves over time, and the morph does
  NOT recompose the button per frame
- **styling** — reaching the derivation, deriving from the theme, light/dark, merging sets, an
  explicit colour beating the class styling, and the styling block ending where it should
- **touch target** — a 48 dp box around a 40 dp visual box
