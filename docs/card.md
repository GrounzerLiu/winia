# Card: API, implementation details, and what is not implemented

> Goal: match Jetpack Compose material3 1.4.0's `Card` family
> (`Card` / `ElevatedCard` / `OutlinedCard`) with an equivalent declarative
> API on winia and close visual/interaction semantics.
> This document follows the implementation; update it when the API or behaviour changes.

## Sources

Compose paths are relative to
`reference/compose/material3-1.5.0-beta01/commonMain/androidx/compose/material3/`.

| Compose | Settles |
|---|---|
| `Card.kt` | the six overloads (`Card` / `ElevatedCard` / `OutlinedCard`, each with a static and an `onClick` form), `CardDefaults`' shape/elevation/colors/border, `CardElevation.shadowElevation` (`:657`, which takes `defaultElevation` when `interactionSource` is `null`) and the private `animateElevation` (`:668`, the state resolution over the interactions list's `lastOrNull`) |
| `Card.kt:498`, `:546`, `:597` (`defaultCardColors` / `defaultElevatedCardColors` / `defaultOutlinedCardColors`) | the container and **disabled** container colours of the three styles, including how `compositeOver` is used |
| `tokens/{Filled,Elevated,Outlined}CardTokens.kt` | the container colour, the six-state elevation, `ContainerShape`, `DisabledContainerOpacity`, `DisabledOutlineOpacity`, `FocusIndicatorColor` |
| `tokens/ShapeKeyTokens.kt` + `tokens/ShapeTokens.kt:70` | `CornerMedium` = `RoundedCornerShape(12.0.dp)` |
| `tokens/ElevationTokens.kt:24-37` | the Level0/1/2/3/4 → 0/1/3/6/8 dp mapping (**not** an index) |
| `Surface.kt:202` (the `Surface(onClick = …)` overload), `:225` (`minimumInteractiveComponentSize()` on its Box) | why a clickable card has a 48dp touch target |
| `internal/Elevation.kt:46-62` (`animateElevation`) | the elevation transition curve is chosen by **interaction pair**, not by a fixed duration |

## 1. API overview

### 1.1 Constructor variants

winia builds no separate composable per M3 variant; the `Card` builder's constructors express them
(the same pattern as Button's filled/elevated/outlined):

| Constructor | material3 equivalent | Difference |
|---|---|---|
| `Card::filled()` (same as `new()`) | `Card` | the Filled style by default (SurfaceContainerHighest container) |
| `Card::elevated()` | `ElevatedCard` | `CardStyle::Elevated` (SurfaceContainerLow container) + `CardElevation::elevated()` |
| `Card::outlined()` | `OutlinedCard` | `CardStyle::Outlined` (Surface container + 1dp outline border) |

### 1.2 Chainable parameters (the `Card` builder)

| Method | Compose | Notes |
|---|---|---|
| `on_click(Fn())` | `onClick` | the click callback (`Send + Sync + 'static`); **None = a display-only card** (not interactive, no ripple) |
| `enabled(bool)` | `enabled` | disabled: the container/content colours switch and no clickable/ripple is registered (so it is not focusable) |
| `style(CardStyle)` | — | Filled / Elevated / Outlined |
| `colors(CardColors)` | `colors: CardColors` | produced by `CardDefaults::card_colors(theme, style)` by default |
| `elevation(CardElevation)` | `elevation` | the shadow elevation is taken per interaction state, and the transition curve is chosen by the interaction pair (`elevation_spec_for_pair`), not by a fixed duration |
| `interaction_source(...)` | `interactionSource` | hoists the interaction source; when it is not passed the component `remember`s one internally |
| `shape(Shape)` | `shape` | one shape for the container/border/shadow/focus ring/ripple; a 12dp corner radius by default (`CornerMedium`) |
| `border(CardBorder)` | `border: BorderStroke` | overrides the style's default border (Outlined defaults to 1dp in the outline colour) |
| `modifier(Modifier)` | `modifier` | appended on the outside, so it can override the default styling |

### 1.3 Value types and defaults

- `CardStyle`: Filled / Elevated / Outlined.
- `CardColors`: `container / content / disabled_container / disabled_content`.
- `CardElevation`: `default / pressed / focused / hovered / dragged / disabled`
  (six states — one more than ButtonElevation, which has no dragged, matching M3's `CardElevation`).
- `CardBorder`: `width + color` (matching `BorderStroke`).
- `CardDefaults`:
  - `shape()` = a 12dp corner radius (all three styles' `ContainerShape` is `CornerMedium`,
    `ShapeTokens.kt:70`: `RoundedCornerShape(12.0.dp)`);
  - `card_colors(theme, style)` — derived from the theme per style:
    | style | container | content | disabled_container | disabled_content |
    |-------|-----------|---------|-------------------|-----------------|
    | Filled | SurfaceContainerHighest | OnSurface | SurfaceVariant@38% | content@38% |
    | Elevated | SurfaceContainerLow | OnSurface | Surface (opaque) | content@38% |
    | Outlined | Surface | OnSurface | Surface (= the container) | content@38% |
  - `card_elevation()` = 0/0/0/1/6/0 dp (FilledCardTokens);
  - `elevated_card_elevation()` = 1/1/1/3/8/1 dp (ElevatedCardTokens);
  - `outlined_card_elevation()` = 0/0/0/1/6/0 dp (OutlinedCardTokens);
  - `outlined_border(theme, enabled)` = 1dp OutlineVariant (Outline@12% when disabled).

  > The values are Dp, not a token level index. `ElevationTokens`' Level0/1/2/3/4 correspond to
  > 0/1/3/6/8 dp respectively — only Level0 and Level1 coincide, and this code once stored the index
  > here, which made a dragged Filled card rise 3dp instead of 6dp and an Elevated one 4dp instead of 8dp.

## 2. Layout and measurement

- **Measure strategy**: `ColumnLayout::new().direction(dir)` — a 12dp rounded container whose content
  is laid out in a **vertical Column**, cross axis aligned `Start`, main axis `Arrangement::Start`, spacing 0.
  Compose is `Surface { Column(content) }`; the column itself carries no padding, the content supplies its own.
- **Size**: the container wraps its content (`fill` comes from the caller's `Modifier`). There is no internal
  fixed height, and no `Card::size`-style parameter exists — sizing always goes through `Modifier`.
- **Content spacing is the content's own business** (matching M3: Card has no `contentPadding` parameter —
  a leaf node's `Modifier.padding` is already fully supported, added back at measure and offset at render).
- **A clickable card gets an extra wrapper**: when `on_click` is present, the card is wrapped in a
  `build_container` + `minimum_interactive_component_size()` (48dp) + centred `BoxLayout` with
  `propagate_min_constraints(true)`. The latter is mandatory: only then does the inner card take the
  width a parent FORCES; otherwise the outer box reports the forced size while the inner still measures
  to its own content width, and focus ring, ripple and container lose their shared idea of the width.
  - Note the wrapper keys on `on_click.is_some()` alone, **not on enabled** — the same as Compose's
    `Surface(onClick = …)`, which applies the minimum interactive size unconditionally too, so a disabled
    clickable card still occupies 48dp. A display-only card has no wrapper, and so no such minimum in Compose.
- **RTL**: a Column's main axis is y, so what mirrors is the **cross axis** (x). `measure_flex` replaces
  each child's `position.x` with `cross_size - x - width` when `direction == Rtl`
  (`flex.rs:513-518`). The main axis is unaffected because a Column's main axis is vertical; the
  mirroring exemption for `Arrangement::Absolute(_)` makes no observable difference on a Column.
- **Intrinsic sizes**: `ColumnLayout` overrides all four intrinsic queries (a port of the
  `IntrinsicMeasureBlocks` in `reference/compose/foundation-layout-1.13.0-beta01/commonMain/androidx/compose/foundation/layout/RowColumnImpl.kt:261-369`
  — note that file is not under material3), and `weight` runs the same arithmetic as measurement.

## 3. Behaviour and implementation details

### 3.1 The build pass and modifier chain order

`Card::build` runs every frame:

1. `ctx.changed(&self.style)` / `ctx.changed(&self.enabled)` declare the Skip parameters;
2. take the theme and the colours (`CardDefaults::card_colors`), read the interaction state;
3. build the shadow animation State (`ctx.animate_float_as_state`) — the transition curve is decided by the
   **interaction pair** (`elevation_spec_for_pair(from, to)`, matching `animateElevation` in
   `material3/internal/Elevation.kt:46-62`),
   not by a single fixed-duration curve; 120ms in, 150ms out, snap on idle→idle;
4. assemble the default modifier: `background(container, shape)` → `border` (Outlined);
5. the shadow goes through a `graphics_layer` dynamic closure (it reads the animated value at render and does not recompose);
6. **when clickable**, declare semantics (`role(Button)` + `merge_descendants(true)` + the `enabled` state);
7. `modifier.then(the caller's modifier)` (the caller's outer layer can override, and semantics can be overridden after it);
8. when enabled and `on_click` is set, append `clickable_with_source` + `ripple_with_shape`;
9. `start_restartable_group` (on Skip the content does not run, but the new modifier is applied to the node through
   `set_skip_modifier`);
10. `set_current_node_focus_color(theme.secondary)` writes the desc channel
    (M3's `FilledCardTokens.FocusIndicatorColor` = Secondary — deliberately different from Button's primary).

Chain order (inner → outer): container (background/border) → shadow (graphics_layer) →
semantics → the caller's modifier → clickable/ripple.

- **The content is a top-aligned Column** (matching M3: `Surface { Column(content) }`);
  the content colour is pushed down through `WiniaTheme::with_content_color` — `Icon::tint(Auto)`
  takes the card's content colour.
- **All three styles have an opaque container colour** (Filled=SurfaceContainerHighest,
  Elevated=SurfaceContainerLow, Outlined=Surface — M3's Card has no transparent-background variant,
  unlike Button's transparent Outlined/Text).

### 3.2 State and colour resolution

- The container/content colours resolve on enabled/disabled only (M3's `CardColors` contract);
  hover/press/focus feedback is drawn by the ripple state layer, not stacked onto the background colour.
- The M3 token colours (1.4.0): Filled = SurfaceContainerHighest/OnSurface;
  Elevated = SurfaceContainerLow/OnSurface; Outlined = Surface/OnSurface.
- Disabled: composed the way Compose does it, not by multiplying alpha — the Filled container is
  `SurfaceVariant@0.38 compositeOver SurfaceContainerHighest` (opaque);
  the Elevated container is `Surface@0.38 compositeOver Surface`, and the result is the **opaque
  Surface** itself, a different colour from the enabled SurfaceContainerLow; the Outlined container does
  not change (M3's DisabledContainerColor is its ContainerColor). Content is OnSurface@38% throughout.

### 3.3 Shadow (the raise animation on hover/press)

- The shadow value is driven by the `graphics_layer.shadow_elevation` dynamic closure, and the curve is chosen
  by **interaction pair**: 120ms in, 150ms out, 120ms leaving hover, snap on idle→idle.
- `for_state` priority: disabled > **dragged** > pressed > hovered > focused > default
  — drag must sit before press, because a drag always comes with a press, and the other order leaves `dragged` unreachable.
- **Only a clickable card can reach the five interaction values.** A card must have hover/press/focus to
  report before it can switch shadows between those states; a card with no `on_click` receives no
  interaction at all and stays on `default` forever. Compose is the same thing: the non-clickable `Card`
  overload passes `interactionSource = null` to `shadowElevation`, which returns
  `remember { mutableStateOf(defaultElevation) }` directly, with no animation (`material3/Card.kt:660-668`).
  winia arrives at the same point from the other side — `Hoverable` is installed by `clickable`
  (`modifier/builders/input.rs:22-31`), and hover routing only sees nodes that declared it
  (`app/per_window/pointer.rs:225-227`). So a static card given `CardElevation::new(0,0,0,8,12,0)` never gets the `hovered` 8.
- The shadow is drawn by Skia's native ambient/spot lights, with default colours of roughly 10% black
  for ambient and 25% black for spot; the shadow shape follows `Card::shape`.
- `graphics_layer` affects appearance only and does not take part in hit testing (already stated in a comment).

### 3.4 Ripple and clipping

- `ripple_with_shape(interaction, contentColor, bounded, shape)`: an explicit container shape, so the
  ripple is clipped to the container shape (a 12dp corner radius by default).

### 3.5 Focus system

- Focus ring: width 3, fully outside (its inner edge sits 2 away from the component), colour = the theme's
  secondary (aligned with M3 Card's FocusIndicatorColor; captured through the desc channel at composition,
  when the render phase's CompositionLocal is already gone), 180ms fade in and out.
- Keyboard activation: Enter/Space fires the **focused node's own** `on_click` only, without bubbling
  to ancestors (the same as Button).

### 3.6 Accessibility

- **A clickable card** declares `role(SemanticsRole::Button)` + `merge_descendants(true)` +
  `SemanticsState::new().enabled(self.enabled)`. The declaration sits **before** the caller's `modifier`
  is appended, so the caller's `semantics(..)` overrides it field by field while keeping the role.
  `merge_descendants` makes the card's text this node's name rather than a separate child node.
- **A non-clickable card** declares no semantics at all and stays transparent in the semantics tree
  (`component-authoring.md` §9, second item). Giving a static card the Button role is the same as placing an unnamed button in front of every card.
- A disabled but clickable card **keeps the role and reports `enabled = false`** — dropping either half
  leaves the screen reader unable to read out both "this is a button" and "it is unavailable".

### 3.7 Performance notes

- The Skip test is style/enabled parameter equality plus modifier parameter equality; the animated value travels through
  `graphics_layer` and never through recomposition.
- The ripple is evaluated against time at render, with no extra animation state.

## 4. Divergences from material3 1.4.0 (not implemented / approximate)

### 4.1 Component and API shape

- **Separate composables**: M3's `ElevatedCard` / `OutlinedCard` are standalone functions;
  winia approximates them with constructor variants (they can be split out later without changing the API).
- **`BorderStroke`**: width + color only; no Brush/gradient/dashed borders.
- **`CardColors.copy()`'s per-parameter override**: winia overrides with `CardColors::new(4 colours)`.
- **`ColumnScope`**: M3's content is `@Composable ColumnScope.() -> Unit`, so inside a card you can call
  `Modifier.weight()` / `align()`; winia's `Card::build` takes
  `impl FnOnce(&mut ComposeCtx)`, and the caller never gets a ColumnScope. The `ColumnScope` type itself already
  exists (`layout/column.rs:29`, with `weight`/`weight_fill`/`align`/`align_by`/
  `align_by_baseline`, and `DerefMut` to `ComposeCtx`, so switching to it would not break the existing `|ctx| …`
  call sites), only no component uses it yet.

### 4.2 Theme and tokens

- **Shape token**: CornerMedium = 12dp, which is `Shape::rounded(12.0)`, with no theme-level
  shape system (the same as Button).
- **Disabled container colour**: computed the way Compose does it — `DisabledContainerColor @0.38
  compositeOver(ContainerColor)` (`Card.kt:507-511`, `:559-563`), composited onto the base colour
  to an **opaque** result. Filled's base is SurfaceContainerHighest, Elevated's base is
  Surface itself (compositing the same colour → Surface), and Outlined's `disabledContainerColor`
  is its ContainerColor to begin with. **No direct alpha multiplication**: that would produce a translucent card that shows
  what is behind it (the old Filled branch did exactly that, and its assertion only compared alpha values, so it never surfaced).
- **Content colour**: M3's `contentColorFor(container)` computes it by contrast; winia takes
  OnSurface uniformly (all three containers are surface-family, so the contrast result agrees).
- **Tonal overlay**: M3 layers a primary overlay by elevation when the container colour is `surface`;
  winia does not implement it (a gap at the `Surface` layer, not specific to Card).

### 4.3 Interaction and rendering

- **What triggers the shadow animation**: M3 uses the interaction flow + `animateElevation` (the interactions
  list's lastOrNull semantics); winia uses a `ComponentState` snapshot + a curve chosen per interaction pair
  (`elevation_spec_for_pair`) — the enter/leave transition curves do not match M3's
  `animateElevation` exactly (an approximation), but the two reachable paths (hover→press,
  press→hover) are already aligned.
- **Ripple**: bounded ripples only; no unbounded, no hover ripple.
- **a11y**: role + enabled + merge_descendants are implemented (§3.6). **Missing**: accessibility labels
  (`contentDescription`) and custom actions.
- **Hit testing**: `graphics_layer` appearance takes no part in the hit test (commented as semantics).

### 4.4 Test gaps

- Keyboard activation, Tab/IME synchronisation and arrow-key navigation have no app-level integration tests
  (the existing coverage is unit-level — the same gap as Button).
- Render assertions on the focus ring's shape/colour are not wired into the UI test framework.
- No `fixture_all` scenario (none of the 13 scenarios is a card), so the UI test suite never reaches this component.

## 5. Tests

```bash
# Every unit test of this component
cargo test -p winia --lib components::card

# Only the two accessibility declaration assertions
cargo test -p winia --lib a_clickable_card_declares_the_button_role_and_its_enabled_state

# Regression: the components and demos that depend on Card
cargo check -p winia --examples
```

13 unit tests, each pinning one invariant:

| Test | What it pins |
|---|---|
| `a_clickable_card_declares_the_button_role_and_its_enabled_state` | clickable → the `Button` role + the `enabled` state + `merge_descendants`; disabled-but-clickable → the role is kept and `false` is reported; not clickable → nothing is declared at all (the control group) |
| `a_clickable_card_reserves_a_48_dp_touch_target` | a clickable card has a 48dp minimum interactive size, a display-only card does not (the control group) |
| `a_clickable_card_honours_a_row_weight` | a clickable card's `weight` reaches the Row: the 48dp wrapper carries the caller's parent data (`Modifier::split_parent_data`), so all three weighted cards measure their share in BOTH the wrapper and the surface inside it — and none falls back to the touch target |
| `the_card_elevation_spec_keys_on_the_interaction_pair` | the transition curve is keyed on the interaction pair: 120ms in, 150ms out, snap on idle→idle |
| `a_dragged_filled_card_raises_the_shadow_it_paints` | a dragged Filled card really paints a 6dp shadow (not the index 3) |
| `test_card_default_shape_is_corner_medium` | the default corner radius = 12dp, with both the comment and the assertion pointing at `ShapeTokens.kt:70` |
| `test_card_colors_match_m3_tokens` | the container/content colours of the three styles; Elevated's disabled container is an **opaque Surface**, and the test asserts `Surface != SurfaceContainerLow` (otherwise that assertion would be meaningless) |
| `test_card_colors_state_resolution` | `ComponentState` → disabled takes `disabled_container` / `disabled_content` first |
| `test_card_elevation_defaults` | the six values of all three elevation sets (Dp, not a level index) |
| `test_card_elevation_priority` | `for_state`'s disabled > dragged > pressed > hovered > focused > default |
| `test_outlined_border_uses_theme_outline_variant` | Outlined's default border is 1dp OutlineVariant, and Outline@12% when disabled |
| `test_card_variant_constructors` | the style and elevation each of the three constructors sets |
| `test_card_builder` | the round-trip and defaults of the builder's fields |

Fixtures: **no** `fixture_all` scenario (see §4.4), so the UI test suite never reaches this component.

## 6. Maintenance conventions

- Adding a builder parameter: put the default in `CardDefaults` and carry it through a modifier in
  `build` (it does not need to be added to `changed()`); add the getter, a unit test and this document along with it.
- When changing visual semantics (colour/shape/focus), keep the principle "match the M3 token first, comment any approximation".
- A change touching focus/IME/keyboard branches must update all four paths together (Tab/arrow keys/click/Escape).
- When changing an accessibility declaration, update the component table in `docs/semantics.md` as well (if that table later gains a Card row).
