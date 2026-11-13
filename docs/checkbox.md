# Checkbox

`Checkbox` and `TriStateCheckbox`, mirroring material3's two: `winia/src/components/checkbox.rs`.

## 1. Sources

Compose paths are relative to
`reference/compose/material3-1.5.0-beta01/commonMain/androidx/compose/material3/`.

| Compose | Settles |
| --- | --- |
| `Checkbox.kt:94`, `:200` | the two plain overloads — `Checkbox(checked, onCheckedChange, …)` and `TriStateCheckbox(state, onClick, …)`; neither takes a custom stroke |
| `Checkbox.kt:150`, `:259` | the two `checkmarkStroke`/`outlineStroke` overloads winia does not have (section 5) |
| `Checkbox.kt:344` (`CheckboxDefaults`), `:482` (`StrokeWidth = 2.dp`) | the defaults object, including `colors(...)`'s two forms |
| `Checkbox.kt:446` (`defaultCheckboxColors`) | the thirteen colour slots and the tokens behind each, including the two disabled opacities |
| `Checkbox.kt:770` (`CheckboxColors`) | the thirteen fields, the deprecated twelve-field constructor, and `copy(...)` |
| `Checkbox.kt:538` (`CheckboxImpl`), `:675` (`drawBox`), `:704` (`drawCheck`) | the node the control occupies, the two-layer box fill, and the check/dash geometry |
| `Checkbox.kt:1040` (`colorAnimationSpecForState`), `:1061` (`SnapAnimationDelay = 100`) | the per-state colour springs and the draw fraction's delayed snap |
| `tokens/CheckboxTokens.kt` | `ContainerSize` (18 dp), `SelectedContainerColor` (Primary), `SelectedIconColor` (OnPrimary), `SelectedDisabledContainerColor` (OnSurface @ 0.38), `SelectedDisabledIconColor` (Surface), `UnselectedOutlineColor` (OnSurfaceVariant), `StateLayerSize` (40 dp), `ContainerShape` (2 dp) |
| `InteractiveComponentSize.kt` | `minimumInteractiveComponentSize()`, the 48 dp touch target a CLICKABLE checkbox reserves |
| `ComposeMaterial3Flags.kt:49` (`isCheckboxStylingFixEnabled = false`) | which branch of `CheckboxImpl` is the reference: the flag's default is off, so the 20 dp container, the two-padding and the enabled-blind checkmark colour are the behaviour to match |

## 2. Variants

| Constructor | State | When |
| --- | --- | --- |
| `Checkbox::new(checked: bool)` | two-state | A plain on/off control; the click reports `!checked` |
| `TriStateCheckbox::new(state: ToggleableState)` | `Off` / `On` / `Indeterminate` | A parent whose descendants are partly selected |

Both take the same options: `enabled`, `colors(CheckboxColors)`, `interaction_source(source)`, and
`modifier`. `Checkbox` reports through `on_checked_change(|bool|)`, `TriStateCheckbox` through
`on_click(||)` — the caller decides the next state, as M3's `onClick` does. Neither holds state: pass
the value in and update it in the callback (the same controlled contract as `IconToggleButton`).

`Checkbox` delegates to `TriStateCheckbox(ToggleableState::from_bool(checked))`, wrapping the
callback into `!checked` — M3's `wrapOnCheckedChange` (`:1051`).

Defaults live in `CheckboxDefaults` — `checkbox_colors(theme)` (the theme's full set),
`colors(theme, six optional overrides)` (M3's six-argument form), `shape()`, `stroke_width()`. The
`CHECKBOX_*` constants are the token values behind them.

## 3. Layout

Two sizes, and which one applies is decided by whether the control takes clicks — M3 gates
`minimumInteractiveComponentSize()` on `onClick != null` too (`:566`):

| The control | Node size | Because |
| --- | --- | --- |
| clickable (`on_*` set) | **48 dp** | `Modifier.minimumInteractiveComponentSize()`; the 20 dp box is centred inside |
| display-only | **24 dp** | the 20 dp box plus `CheckboxDefaultPadding` = 2 dp on each side (`:1064`), and no minimum |

The 20 dp visual box is centred in either, at `CHECKBOX_SIZE`. The ripple halo is a separate,
smaller number: `StateLayerSize` = **40 dp** reaches `ripple(radius = …)` (`:557`), so the halo is
narrower than the 48 dp node — it is named rather than derived from the node's diagonal. The
spreading foreground still grows to the node's diagonal (`render::pass`), so a press near a corner
still covers the control.

The outer node carries the ring's SHAPE without clipping it, through a 0-width transparent `border`
rather than a `clip`: the renderer strokes the focus ring OUTSIDE the node's box, so a clip would
cut the ring away. Nothing here is axis-specific, so RTL needs no mirroring.

## 4. Behaviour

Nothing is drawn per-frame by the component; it composes a 20 dp box (background + border) holding
two icons, and both the colours and the glyph scales are paint-time values.

**Colours** — resolved as `enabled × state`, exactly M3's `boxColor`/`borderColor`/`checkmarkColor`:

| State | Box | Border | Checkmark |
| --- | --- | --- | --- |
| `On` | Primary | Primary | OnPrimary |
| `Off` | transparent | OnSurfaceVariant | transparent |
| `Indeterminate` | Primary | Primary | OnPrimary |
| disabled `On` | OnSurface @ 0.38 | OnSurface @ 0.38 | OnPrimary |
| disabled `Off` | transparent | OnSurface @ 0.38 | transparent |
| disabled `Indeterminate` | OnSurface @ 0.38 | OnSurface @ 0.38 | OnPrimary |

The box and the border split on `enabled`; the checkmark does NOT — its two disabled rows repeat the
enabled colours above them. That is deliberate, and it is M3's flag-off behaviour (section 5): the
flag-off branch calls `checkmarkColor(state)` (`Checkbox.kt:948`), which only looks at the state, so a
disabled checked box keeps painting `checked_checkmark` (OnPrimary) rather than
`SelectedDisabledIconColor` (Surface). Section 5 records that token as the divergence.

No other dimension picks a colour: M3's `CheckboxColors` has no hover/focus/press slots, and the
interaction feedback is the ripple's job.

**Box and border animation** — `animate_color_as_state` with M3's per-state spring
(`colorAnimationSpecForState`, `:1040`): `FastEffects` while the box EMPTIES, `DefaultEffects`
otherwise. A disabled checkbox SNAPS instead — Compose's rule is that no animation runs between the
enabled and disabled states (`:639-641` returns the colour through `rememberUpdatedState`). The
renderer reads both through `peek()` at paint time, so the fade costs no recomposition.

A border whose colour equals the box's is merged into a plain fill: painting a semi-transparent
stroke over the same fill blends twice and leaves the edge visibly darker than the interior.

**Glyph animation** — the draw fraction follows M3's `animateFloat` (`:597-609`): filling from `Off`,
and every move between `On` and `Indeterminate`, runs on the `DefaultSpatial` token; emptying is
`snap(delayMillis = SnapAnimationDelay)` (100 ms). winia has no delayed `Snap`, so it spends a
zero-duration tween carrying that delay. The two glyphs (check for `On`, dash for `Indeterminate`)
each own a scale, 1 or 0 by state, which approximates `checkDrawFraction` plus
`crossCenterGravitation`.

The checkmark tint stays at the CHECKED colour in every state rather than following the state: `Off`
hides the glyph at scale 0, and the outgoing transition (1 → 0) has to stay visible, or an instant
switch to transparent would swallow the exit animation. That colour is a parameter of the inner
group (`ctx.changed`), not just of the modifier chain — the chain cannot carry it, because a `Dyn`
background compares equal to any other `Dyn`, so a `colors(..)` change would otherwise leave the
glyphs on the colour they were built with.

**Focus and interaction** — `clickable_with_source` (press/focus/hover) plus an unbounded ripple
tinted `on_surface`, and the focus ring takes `theme.primary` with the shape the 0-width border
declares. A disabled checkbox registers neither clickable nor ripple nor focusable and uses the
disabled colour group. Semantics are declared unconditionally: `Role.Checkbox` with the tri-state
value and `enabled`, `merge_descendants(true)` — `Off` and `Indeterminate` are different answers,
which is why the value is not a bool.

## 5. Differences from Compose

Deliberate or not yet built; silence in this table would be a claim of parity.

- **No custom stroke overloads.** M3's `checkmarkStroke`/`outlineStroke` forms (`:150`, `:259`) are
  missing, so a checkbox cannot be drawn with a caller's `Stroke`.
- **No `disabledCheckmarkColor`.** M3's `CheckboxColors` has thirteen fields (`:770`); winia has
  twelve and resolves the disabled checkmark to the checked colour. That MATCHES M3's default
  behaviour — `isCheckboxStylingFixEnabled` is `false` (`ComposeMaterial3Flags.kt:49`), and the flag-off
  branch calls `checkmarkColor(state)`, ignoring `enabled`. It diverges only from the flag-on
  behaviour, where the token is `SelectedDisabledIconColor` = Surface. Adding the field is what would
  let winia follow the fixed path. It is also why `copy(..)` takes twelve arguments rather than
  M3's thirteen and why `CheckboxDefaults::colors(..)` cannot seed a disabled checkmark
  (`Checkbox.kt:379` does).
- **`copy(..)` carries "keep" in an `Option`, not in a sentinel colour.** M3 spells it
  `Color.Unspecified` and resolves it through `takeOrElse` (`:888-909`); winia's `Color` has no
  unspecified variant, so every slot is `Option<Color>` with `None` meaning "keep the source value".
  Same contract, different carrier — and it keeps an all-`None` copy an identity.
- **Semantics are declared without a click handler.** M3 attaches `triStateToggleable` only when
  `onClick != null` (`:572-584`), so a display-only checkbox is absent from its semantics tree. winia
  always publishes `Role.Checkbox` and the state, which reads better for a static control and matches
  §9 of `docs/component-authoring.md`; the cost is that `Checkbox::new(x)` with no handler still
  announces itself. (`Card` takes the opposite decision — it declares only when clickable.)
- **Ripple colour is fixed to `on_surface`.** M3 passes `Color.Unspecified` when the flag is off,
  which resolves to `LocalContentColor`; the two agree under the default palette, so this is a fixed
  value rather than a resolved local.
- **The focus ring is an outline in `primary`.** M3 hands `ripple` a `focusRingShape` and lets its
  inset-ring theme configuration draw it; winia's renderer strokes its own ring from the node's
  declared shape and the colour the component supplies.
- **`On` ↔ `Indeterminate` is a cross-fade of two icons**, not a path morph. M3 deforms one check
  path through `checkCenterGravitationShiftFraction`; a continuous morph would need `Canvas` plus
  path measurement.
- **Keyboard `Space` is not wired here.** Toggling comes from the shared `clickable` path, so a
  focused checkbox responds to whatever that path already binds.

## 6. Tests

```bash
# Every unit test of this component (15 in tests.rs + 6 inline in checkbox.rs)
cargo test -p winia --lib components::checkbox

# The regression for the checkmark tint following a colors(..) change
cargo test -p winia --lib a_colors_change_reaches_the_checkmark_tint
```

| Test | What it pins |
| --- | --- |
| `an_interactive_checkbox_reserves_48_dp_and_names_a_40_dp_state_layer` | a clickable checkbox is a 48 dp node with a 40 dp halo; a display-only one is 24 dp and takes no minimum |
| `toggleable_state_conversion` | `ToggleableState::from_bool` and `is_checked` for the three variants |
| `the_draw_spec_matches_m3s_transition` | filling uses `DefaultSpatial`; emptying is a 100 ms-delayed snap |
| `the_colour_spec_is_m3s_effect_spring_per_state` | `FastEffects` while emptying, `DefaultEffects` otherwise |
| `color_state_resolution` | the twelve slots resolve per `enabled × state` |
| `checkbox_callback_receives_inverted_checked` | the click reports `!checked` |
| `tri_state_on_click_fires` | `TriStateCheckbox` fires its handler without changing state itself |
| `disabled_checkbox_has_no_interaction_elements` | disabled registers no clickable and no ripple |
| `checked_checkmark_icon_tint_resolves_to_on_primary` | `On` tints both glyphs with the checked colour |
| `unchecked_has_transparent_checkmark_and_no_fill` | `Off` paints neither a fill nor a visible mark |
| `uncheck_keeps_primary_pixels_during_transition` | unchecking fades rather than snapping — the exit animation survives |
| `disabled_checked_border_merges_with_fill` | a disabled checked box is a merged fill, not a double-blended stroke |
| `a_colors_change_reaches_the_checkmark_tint` | a new `colors(..)` repaints the glyphs (the `ctx.changed` on the inner group) |
| `copy_replaces_exactly_the_slots_it_is_given` | `CheckboxColors::copy(..)` replaces the supplied slots, keeps every `None`, and an all-`None` copy is the identity |
| `defaults_colors_feeds_six_arguments_and_forces_three_transparent` | which slot each of the six `CheckboxDefaults::colors(..)` arguments feeds, and that the three unchecked ones stay transparent |
| `parent_tri_state_follows_children` / `_follows_click` / `_renders_change_on_child_click` / `_follows_child_in_scroll_column` | the select-all group in the demo: parent state derives from the children and survives a scroll |
| `macroized_column_content_state_notify_enters_column` | a state read inside a `#[composable]` container re-enters its content |
| `a_checkbox_styling_overrides_only_what_it_sets` | `CheckboxStyling` replaces the slots it names and leaves the rest at the theme's derivation |

Fixtures: **no** `fixture_all` scenario, so the UI test suite never reaches this component. It appears
in `examples/alert_dialog_demo.rs` and `examples/bottom_sheet_demo.rs`.
