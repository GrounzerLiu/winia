# Component styling

How an application restyles a whole control class once, including light and dark variants. This is the
document for the FRAMEWORK; the per-component knobs are documented next to each component.

## The two layers

| Layer | Answers | Lives in |
| --- | --- | --- |
| Role colours (the palette) | "what does this application look like" - `primary`, `secondary_container`, `surface` | `WiniaTheme`, provided per window/subtree |
| Class styling | "this control class looks different from the palette's answer" | `ComponentStyles`, provided per subtree |

A component derives its own colours from the roles (`ButtonColors::from_theme`), which is why seeding
the palette green turns every filled button green and leaves a tonal button tonal. That is the right
default and it is what Compose stable offers too. Class styling is the escape hatch for "all buttons
green while the palette stays blue".

Compose is building the same thing (`ComponentStyles.kt` in material3 1.5.0-beta01), but that file is
an internal shim - its header says it "should contain no public APIs" - so there is no public shape to
copy yet. winia's shape is described below and is deliberately smaller: colours and the knobs each
component already exposes.

## The pieces

| Piece | Where | Role |
| --- | --- | --- |
| `Styling<S>` | `component_styles.rs` | one class's styling: `Arc<dyn Fn(&mut S)>`, composable with `then` |
| `ComponentStyles` | `component_styles.rs` | the registry: `TypeId -> styling`, `with`, `get`, `merge` |
| `with_component_styles` | `component_styles.rs` | provides a registry through a `CompositionLocal` |
| `palette_is_dark` | `component_styles.rs` | whether the palette in scope is dark |
| `XStylingScope`, `XStyling`, `x_colors_with_styles` | the component's own file | what the class exposes and how it resolves |

```rust
with_component_styles(
    ComponentStyles::new().with(ButtonStyling::new(|scope| {
        scope.container(if scope.is_dark { LIME } else { GREEN });
    })),
    || app(ctx),
);
```

### Precedence at a call site

```text
1. explicit parameter        Button::colors(..)           wins over everything
2. class styling             ComponentStyles.get::<T>()    applied on top of (3)
3. theme derivation          ButtonColors::from_theme()   the default
```

The styling scope STARTS from the theme's answer, so a styling that sets one field leaves the rest
exactly as the roles left them - disabled alphas, content colour, everything.

### Light and dark

`scope.is_dark` is decided from the PALETTE's `surface` lightness (Oklab L < 0.5), not from the
platform's `isSystemInDarkTheme`: a subtree pinned to `WiniaTheme::light()` under a dark system has to
style as light, and a seeded custom scheme records no mode at all. `scope.theme` is the full palette,
so a styling can also derive from roles (`scope.theme.tertiary`) instead of branching.

Because the theme follows the system (`WiniaTheme::auto` reads a `State`), a mode change re-composes
the subtree and the styling is resolved again with the other answer.

### Composing stylings

| Operation | Semantics |
| --- | --- |
| `a.then(b)` | within one class: `a` runs, then `b`; later setters win |
| `base.merge(page)` | across classes: `page` wins for the classes it mentions, `base` keeps the rest |
| nested `with_component_styles` | REPLACES the whole registry for its subtree (not a merge) - that is what makes "drop the outer styling here" expressible |

## Contract for a component author

To join the framework a component needs three things in its own file:

1. A scope struct with the fields it can restyle (`theme`, `is_dark`, its colour set, and any other
   knob it exposes), with setter methods that write those fields.
2. `pub type XStyling = Styling<XStylingScope>;`
3. One resolution function (`x_colors_with_styles`) that starts from the theme derivation, applies
   `current_component_styles().get::<XStyling>()` when present, and returns the result. `build` MUST
   resolve through it; calling the theme derivation directly silently ignores every styling.

Rule of thumb for what a scope exposes: exactly the knobs the builder already has. A styling is a
class-wide default, not a new API surface.

## What is deliberately not here yet

- **State blocks** ("when pressed, use this colour") in the Compose-shim style. The colour sets winia
  components use are already stateful (`ComponentState`), and splitting them into per-state setters
  needs a resolver that runs per state - a design worth doing once there are two or three components
  that need it, not before.
- **Per-component coverage.** Button and Checkbox are wired. Every other component keeps its theme
  derivation until it is wired; the framework does not care which classes are registered.
- **Instance styling shortcuts.** A single instance is still restyled through the builder
  (`Button::colors`, `Button::shape`), which is also what Compose does.

## Cost

One `CompositionLocal` read per component build (a thread-local stack walk plus a `TypeId` hash
lookup), and the empty registry is allocated once. A styling is resolved per build, not per frame:
building a button happens on recomposition, not on every paint.

## Tests

`cargo test -p winia --lib component_styles` - the registry (by-type lookup, replacement, `then`
order, empty outside a provider, nesting replaces, `merge` precedence, palette mode).
`cargo test -p winia --lib components::button` and `components::checkbox` - the component halves (one
styling reaches the derivation, deriving from the theme, light/dark branching, the styling ending with
its block, no styling means the old derivation for all five button variants).
