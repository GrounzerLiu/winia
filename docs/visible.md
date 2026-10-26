# Visible

`Modifier::visible(bool)` — hide a node without giving up its space.

## Sources

Compose foundation `Modifier.visible` (`reference/compose/foundation-layout-1.13.0-beta01/commonMain/androidx/compose/foundation/layout/Visible.kt:51`):
`Modifier.visible(false)` keeps the node measured and occupying its space, but skips its PLACEMENT, and
clears `focused` in semantics — `View.INVISIBLE`, not `View.GONE`.

## API

```rust
Text::new("hidden but still occupying its line")
    .modifier(Modifier::new().visible(false))
    .build(ctx);
```

Last value in the chain wins; a chain without `visible` is visible.

## Layout

The node measures exactly as it would when visible — size, intrinsic answers and sibling positions are
unchanged. winia has no separate placement phase, so "not placed" is expressed where placement is
consumed:

| consumer | behaviour |
| --- | --- |
| paint (`render/pass.rs`) | the node and its subtree are not drawn |
| hit testing (`layout/node/hit.rs`) | the subtree is not hit, so taps fall through to what is below |
| focus (`layout/node/focus.rs`) | the subtree is not collected as a Tab candidate |
| semantics (`semantics/tree.rs`) | the subtree stays out of the tree a screen reader descends |

## Behaviour

- It is measured, so a sibling's position is the same as with the node visible.
- It is not painted, not interactive, not focusable and not in the accessibility tree.
- Toggling it is a modifier change, so the container's Skip decision sees it
  (`element_param_eq` compares the flag) and the node re-enters once.
- Unlike `AnimatedVisibility` it never leaves the layout; unlike an alpha of `0` it is not clickable.

## Differences from Compose

- Compose clears `focused` through a semantics property and re-places nothing; winia drops the whole
  subtree from paint, hit testing, focus traversal and semantics in one rule, and does not clear the
  focused node explicitly — a node that is already focused when it becomes hidden keeps the flag but
  can no longer be reached or clicked. Clearing focus on hide is not implemented.
- `Modifier.visible` is not usable with a shared-element transition in Compose either; winia's
  transition layer respects the flag like the main tree.

## Tests

| test | pins |
| --- | --- |
| `a_hidden_node_keeps_its_space_and_paints_nothing` (`tests/render_snapshot.rs`) | raster: nothing painted, the next sibling still starts after its space |
| `a_hidden_sibling_does_not_take_the_hit` (`layout/node/tests.rs`) | hit testing skips the hidden subtree |
| `param_eq_compares_the_visibility` (`modifier/param_eq_tests.rs`) | a changed flag re-enters; a gained `visible(true)` is a structural change |
