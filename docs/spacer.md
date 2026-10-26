# Spacer

A blank of a size the caller chooses, for explicit gaps between siblings.

## Sources

Compose foundation `Spacer` (
`reference/compose/foundation-layout-1.13.0-beta01/commonMain/androidx/compose/foundation/layout/Spacer.kt:42-58`):
a `@NonRestartableComposable` around a childless `Layout` whose measure policy reads its own
constraints — an axis whose minimum equals its maximum takes that value, every other axis is 0.
The size therefore comes from the modifier, never from a constructor argument:
`Spacer(Modifier.size(8.dp))`, `Spacer(Modifier.height(8.dp))`, `Spacer(Modifier.weight(1f))` and
`Spacer(Modifier.fillMaxHeight())` are one component with different chains.

## API

```rust
Spacer::new()                                            // size entirely from the modifier
Spacer::vertical(8.0)                                    // 8 high, width open (0 unless filled)
Spacer::horizontal(8.0)                                  // 8 wide, height open
Spacer::vertical(8.0).modifier(Modifier::new().fill_max_width())
Spacer::new().modifier(Modifier::new().layout_weight(1.0))
```

`vertical` and `horizontal` set ONE axis (`Modifier::height` / `Modifier::width`, the other axis
stays `Auto`), so a `fill_max_*` or a `layout_weight` in the caller's modifier still owns the other
axis. A later `size` in the chain overrides only the axes it names, in chain order.

## Layout

The component is an empty `BoxLayout`: with no children the box reports its own minimum, which is
exactly the fixed value a size, a weight or a fill handed it. It paints nothing and composes no
children, so it costs one restartable group and no draw work.

## Behaviour

| chain | measured |
| --- | --- |
| `Spacer::vertical(10.0)` | 0 x 10 |
| `Spacer::horizontal(10.0)` | 10 x 0 |
| `Spacer::horizontal(10.0).fill_max_height()` | 10 x parent height |
| `Spacer::new().layout_weight(1.0)` in a Row | the leftover width |
| `Spacer::new().size(10.0, 10.0)` | 10 x 10 |
| `Spacer::vertical(8.0).size(20.0, 20.0)` | 20 x 8 (the later `size` only sets width) |

## Differences from Compose

- **A non-fixed axis reports the incoming minimum.** Compose's policy returns 0 unless `min == max`
  (`Spacer.kt:48-51`), so `Spacer(Modifier.widthIn(100.dp, 200.dp))` measures 0 wide there. winia
  composes an empty box and an empty box reports its own minimum, so the same chain measures 100.
  Deliberate: the rule is uniform with every other empty container, and a caller who wants 0 simply
  writes no constraint. Pinned by `an_empty_spacer_reports_the_incoming_minimum`.
- **A restartable group per spacer.** Compose marks `Spacer` `@NonRestartableComposable`; winia
  composes it through `build_container`, which opens one group. It reads no state, so the group
  never invalidates; it only costs a slot.
- No `Spacer(Modifier)`-only overload is needed in Rust: `Spacer::new().modifier(m)` is the same
  thing, and `Default` forwards to `new()`.

## Tests

`winia/src/layout/spacer.rs` (`layout::spacer::tests`, 8 cases):

| test | pins |
| --- | --- |
| `a_vertical_spacer_is_a_zero_wide_gap` | 0 x 10 |
| `a_horizontal_spacer_is_a_zero_high_gap` | 10 x 0 |
| `a_spacer_fills_the_axis_its_constructor_left_open` | the fix: 10 x parent height, was 10 x 0 |
| `a_weighted_spacer_takes_its_share` | 200 of a 300 row |
| `a_spacer_sized_only_by_its_modifier_takes_that_size` | `Spacer::new()` path |
| `a_bare_spacer_takes_no_space` | 0 x 0 |
| `a_later_size_overrides_only_the_axis_it_names` | chain order |
| `an_empty_spacer_reports_the_incoming_minimum` | the documented difference |

## History

Before this round `vertical`/`horizontal` baked a TIGHT 0 on the other axis (`size(0.0, h)`), so a
`fill_max_*` in the caller's modifier measured against a maximum of 0 and the spacer stayed 0 on that
axis (`Spacer::horizontal(10.0).fill_max_height()` measured 10 x 0). The constructors now set one axis
and leave the other `Auto`.
