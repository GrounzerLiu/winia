# SegmentedButton (material3 aligned)

> Branch: `segmented-button`. Sources: `androidx-main` `compose/material3/.../SegmentedButton.kt` and
> `.../tokens/OutlinedSegmentedButtonTokens.kt` (the token values below come from the token file).
> The implementation plan and the difference-closure work live in `docs/segmented-button-plan.md`.

## 1. API

```rust
SingleChoiceSegmentedButtonRow::new()          // or MultiChoiceSegmentedButtonRow::new()
    .modifier(Modifier)                        // .space(f32) overrides the 1 dp overlap
    .build(ctx, |ctx| {
        SegmentedButton::new(selected, || { /* pick */ })        // selectable (single-choice)
            .shape(SegmentedButtonDefaults::item_shape(0, 2))    // index, count — as Compose requires
            .build(ctx, |ctx| Text::new("Day").build(ctx));
        SegmentedButton::toggle(checked, |now| { /* toggle */ }) // toggleable (multi-choice)
            .shape(SegmentedButtonDefaults::item_shape(1, 2))
            .build(ctx, |ctx| Text::new("Week").build(ctx));
    });
```

Shared builders: `.enabled(bool)`, `.colors(SegmentedButtonColors)`, `.content_padding(h, v)`,
`.border(width, color)`, `.interaction_source(source)`, `.icon(|ctx| …)`, `.inactive_icon(|ctx| …)`,
`.modifier(Modifier)`.

`SegmentedButtonDefaults::item_shape(index, count)` resolves the direction from
`WiniaTheme::direction()`: a single item is `Shape::Pill`, the first rounds its start corners, the last
its end corners, anything in between is a rectangle — and "start" is the right side under RTL, so a
caller passes the same index/count in either direction.

Under RTL the row mirrors its items (item 0 goes to the right edge), which is what keeps those shapes on
the outer edges, and the content inside each item mirrors with it — the check moves to the trailing side
of the label and its scale-in grows from the corner facing the label. Compose gets the first by using a
`Row` for the strip; its content layout places the icon and the label absolutely and does NOT mirror
them, so the second is a deliberate difference here.

## 2. Tokens (from `OutlinedSegmentedButtonTokens`, the file Compose reads)

| Token | Value | Where |
|---|---|---|
| `ContainerHeight` | 40 dp | the row's height, and the item's floor |
| `OutlineWidth` | 1 dp | the item's border, and the row's default `space` (negative → the borders coincide) |
| `IconSize` | 18 dp | the check mark |
| — (`IconSpacing`) | 8 dp | Compose's private constant; the icon slot is `18 + 8 = 26` |
| `ContentPadding` | 12 dp horizontal, 8 dp vertical | item content inset |
| — (`ButtonDefaults.MinWidth`) | 58 dp | the narrowest an item gets |
| `Shape` | `CornerFull` | the base shape `item_shape` derives from; our per-corner shapes take a radius, so the exports are `OUTER_CORNER_RADIUS = HEIGHT / 2` |
| — (`CheckedZIndexFactor`) | 5 | the z a checked item carries |

Colors resolve from `enabled × active` (Compose's `containerColor` / `contentColor` / `borderColor`):

| state | container | content | border |
|---|---|---|---|
| enabled + active | `secondaryContainer` | `onSecondaryContainer` | `outline` |
| enabled + inactive | transparent | `onSurface` | `outline` |
| disabled + active | `secondaryContainer` | `onSurface` @ 38 % | `outline` @ 12 % |
| disabled + inactive | transparent | `onSurface` @ 38 % | `outline` @ 12 % |

## 3. Implementation

- **Row.** One policy: measure each item loosely (`min_width` = 58) to find the widest and the tallest,
  then place every item at THAT width, spaced by `-space` so adjacent 1 dp borders land on the same
  line. Because the width is the widest item's rather than the parent's, the strip wraps its content —
  Compose's `width(IntrinsicSize.Min)` + `weight(1f)` — and when the parent is narrower than the strip
  the items shrink to fit instead of overflowing (see §4).
- **Item.** One node: `background` + `border` in the item's shape, `ripple_with_shape`, and the content
  policy — an icon slot of `IconSize + IconSpacing` that is reserved whether or not an icon is drawn,
  then the label, whose x is `slot + offset`. `offset` is a `State<f32>` animated at measure time
  (`-slot / 2` with no icon → `0` with one), the same two-phase pattern TabRow uses for its indicator;
  the icon-and-label block is centred in the item, as Compose's `Box(contentAlignment = Center)` is.
- **Check.** Both flavours show a check while active (Compose's `SegmentedButtonDefaults.Icon(active)`
  is the default in either row scope). It appears through a graphics layer whose scale and alpha ride
  one progress value about the BOTTOM-LEFT corner — `scaleIn(initialScale = 0f, transformOrigin =
  TransformOrigin(0f, 1f))` plus `fadeIn` — and goes invisible the moment the segment is inactive, which
  is the source's `exit = None` (no exit animation). The spring parameters are ours: Compose publishes
  no tokens for this animation (the source carries its own TODO). `.inactive_icon(...)` switches to the
  source's other branch: the slot holds an icon in both states and the two fade between each other
  through the framework's `Crossfade` (see the difference below), so nothing slides.
- **Stacking.** A checked item carries `z_index(5)` and a pressed or focused one `z_index(1)`
  (`Modifier::z_index`); an idle item carries none, so a row where everything is idle keeps the
  renderer's plain tree-order loop. Compose counts interactions instead of testing two booleans, which
  is the same z for one interaction.

## 4. Differences from Compose

1. **No accessibility semantics.** `selectableGroup()`, `Role.RadioButton` / `Role.Checkbox` and the
   `selectable` / `toggleable` semantics are part of the semantics tree, and winia has no semantics layer
   (see the backlog note in `docs/segmented-button-plan.md`).
2. **A row that shrinks instead of overflowing.** Compose's row is sized to the items' `IntrinsicSize.Min`
   and a narrower parent simply clips/ellipsizes the labels; winia divides the available width between
   the items. Same geometry when the parent is wide enough — which is what a caller expects — with a
   defined behaviour when it is not.
3. **No custom `icon`/`label` slot shapes beyond the two icon hooks.** Compose lets a caller pass any
   composable to `icon`/`label` and a `BorderStroke` per item; winia offers `.icon` / `.inactive_icon`,
   a label closure, `.content_padding` and `.border(width, color)` — enough for the shape of the API but
   not for, say, an icon plus a trailing badge inside one segment.
4. **The crossfading pair uses this framework's `Crossfade`.** With `.inactive_icon(...)` the slot is
   occupied in both states and the two icons fade between each other, as Compose's
   `Crossfade(targetState = active)` does — but ours fades the outgoing one out, swaps, and fades the
   incoming one in (sequential), where Compose's cross-dissolves the two at once. The visible
   difference is a brief empty slot in the middle of the swap; the slot is occupied at rest either way,
   which is what keeps the label still.
5. **An interacting item outranks a checked one.** Compose's `interactionZIndex` is
   `interactionCount + (checked ? 5 : 0)`, which leaves a focused unchecked item (1) BELOW a checked
   neighbour (5) — harmless there, because the M3 ring is drawn inside the item's bounds. winia uses the
   framework's ring, drawn just outside the rect, so a checked neighbour cut the ring's shared edge
   away; an interacting item therefore carries `CHECKED_Z + INTERACTING_Z` and paints above everything
   in the row. Two simultaneous interactions still collapse to one z step (Compose counts them).

## 5. Tests

- `item_shape_follows_index_count_and_direction` — the four shape cases in LTR and RTL.
- `rtl_mirrors_the_strip_so_the_rounded_corners_stay_on_the_outer_edges` — RTL places item 0 at the
  strip's right edge and the last item at its left, so the shapes `item_shape` hands out land on the
  outer edges instead of the shared inner ones (reported by eye in the demo).
- `rtl_swaps_the_icon_and_the_label_inside_the_item` — the check goes to the trailing side of the label
  under RTL, keeping its distance from the item's outer edge.
- `colors_resolve_by_state` — the twelve-field table above.
- `items_are_equal_width_and_share_their_borders` — equal widths, the `n × w − (n − 1)` row width, the
  overlap arithmetic, and the 40 dp row height.
- `the_row_wraps_to_the_widest_item_and_shrinks_when_it_must` — the `IntrinsicSize.Min` behaviour and
  the shrink path in a 120 px parent.
- `a_checked_item_is_raised_above_its_neighbours` — `z_index(5)` on the checked item, none on an idle one.
- `the_checked_item_is_filled_and_the_rest_are_outlined` — pixels: the checked fill, the transparent
  unchecked container with its 1 dp outline, and the checked item's fill covering the shared edge (the
  `z_index` visible in the output).
- `a_disabled_item_uses_the_disabled_palette`.
- `segmented_buttons_pick_and_toggle` (real window, `tests/ui_test.rs`, fixture `fixture_segmented_button`):
  clicking a single-choice item moves the selection, and a multi-choice item toggles on its own —
  falsified by making the toggle always report `true`, which fails the last assertion.
