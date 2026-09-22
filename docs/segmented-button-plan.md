# SegmentedButton — implementation plan (and how each Compose difference gets closed)

> Status: **planned, not started**.
> Sources read: `androidx-main` `compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/SegmentedButton.kt`
> and `.../tokens/OutlinedSegmentedButtonTokens.kt` (the token values below are from the token file, not from memory).
> Predecessors: the RangeSlider work (merged as `13053fe`) and the popup gesture work.
> This document exists because the feature queues up more than one piece of work: the component itself,
> a small framework addition it wants (`z_index`), and a couple of smaller parity items. It is the plan,
> not a record of work done — nothing here has been implemented yet.

## 1. What Compose does (facts from the source)

### Tokens (`OutlinedSegmentedButtonTokens`)

| Token | Value | Used for |
|---|---|---|
| `ContainerHeight` | 40 dp | row min height, item min height |
| `OutlineWidth` | 1 dp | item border, and the row's negative spacing |
| `IconSize` | 18 dp | the check icon |
| `Shape` | `CornerFull` | the base shape `itemShape` derives from |
| `LabelTextFont` | `LabelLarge` | the label |
| `ContentPadding` | 12 dp horizontal, 8 dp vertical | item content inset |

Compose also uses `ButtonDefaults.MinWidth` (58 dp) / `MinHeight` (40 dp) as the item's minimum, and a
private `IconSpacing = 8 dp`, `CheckedZIndexFactor = 5f`.

### Shapes

`SegmentedButtonDefaults.itemShape(index, count, baseShape = CornerFull)`:

- `count == 1` → the whole base shape (a full stadium, `CornerFull`);
- `index == 0` → `baseShape.start()` (start corners only);
- `index == count - 1` → `baseShape.end()`;
- otherwise → `RectangleShape`.

### Row

```kotlin
Row(
    modifier.selectableGroup().defaultMinSize(minHeight = ContainerHeight).width(IntrinsicSize.Min),
    horizontalArrangement = Arrangement.spacedBy(-1.dp),
    verticalAlignment = Alignment.CenterVertically,
)
```

Each item takes `Modifier.weight(1f)`, so **the items are equal width, that width being the widest
item's natural width** — the row wraps its content rather than filling the parent — and adjacent 1 dp
borders overlap into a single line.

### Item

`Surface(selected/checked, onClick/onCheckedChange, shape, color = containerColor(enabled, state),
contentColor = contentColor(...), border = borderStroke(borderColor(...)))`, with the content being a
two-slot layout (`SegmentedButtonContent` + `SegmentedButtonContentMeasurePolicy`):

- the width always reserves `max(IconSize, iconWidth) + IconSpacing` = 26 px for the icon slot;
- with no icon the label sits at 26 − 13 = **13 px** (visually centred in the container), with the icon
  it animates to **26 px**;
- the label is vertically centred; the icon too.

The check icon is `SegmentedButtonDefaults.Icon(active)`:

```kotlin
AnimatedVisibility(
    visible = active,
    exit = ExitTransition.None,
    enter = fadeIn(...) + scaleIn(initialScale = 0f, transformOrigin = TransformOrigin(0f, 1f), ...),
) { ActiveIcon() }   // Icon(Icons.Filled.Check, size = IconSize)
```

i.e. **fade in while scaling up from the bottom-left corner, and disappear instantly on deselect**. The
animation specs are not in the token file (the source carries its own `TODO Load the motionScheme
tokens`), so the behavior is the part worth matching.

### Colors (12 fields, resolved by `enabled × active`)

| state | container | content | border |
|---|---|---|---|
| enabled + active | `secondaryContainer` | `onSecondaryContainer` | `outline` |
| enabled + inactive | transparent | `onSurface` | `outline` |
| disabled + active | `secondaryContainer` | `onSurface` @ 38% | `outline` @ 12% |
| disabled + inactive | transparent | `onSurface` @ 38% | `outline` @ 12% |

The container color is **not** animated between states; only the icon animates.

### Sibling z-order

`Modifier.interactionZIndex(checked, interactionCount)` places each item at
`z = interactionCount + (checked ? 5f : 0f)`, so a checked item (and one being pressed/focused) paints
ABOVE its neighbours. That is what lets a checked item's fill cover the shared 1 dp edge.

## 2. winia design

```rust
SingleChoiceSegmentedButtonRow::new()                 // material3 SingleChoiceSegmentedButtonRow
    .modifier(Modifier)
    .build(ctx, |ctx| {
        SegmentedButton::new(selected, || { /* pick */ })
            .shape(SegmentedButtonDefaults::item_shape(0, 2))   // Compose's explicit index/count
            .build(ctx, |ctx| Text::new("Day").build(ctx));
        SegmentedButton::new(!selected, || {})
            .shape(SegmentedButtonDefaults::item_shape(1, 2))
            .build(ctx, |ctx| Text::new("Week").build(ctx));
    });

MultiChoiceSegmentedButtonRow::new()
    .build(ctx, |ctx| {
        SegmentedButton::toggle(checked, |now| { /* toggle */ })
            .shape(SegmentedButtonDefaults::item_shape(0, 3))
            .build(ctx, |ctx| Text::new("Bold").build(ctx));
        // ...
    });
```

- Item constructors follow the house "variant constructor" style (`Chip::assist` / `Chip::filter`):
  `SegmentedButton::new(selected, on_click)` for single-choice, `SegmentedButton::toggle(checked,
  on_checked_change)` for multi-choice.
- `SegmentedButtonDefaults::item_shape(index, count)` resolves the direction itself
  (`WiniaTheme::direction()`), so a caller never picks the mirrored shape by hand: `Pill` for
  `count == 1`, `LeftRoundedRect { radius: OUTER_CORNER_RADIUS }` first / `RightRoundedRect` last under
  LTR (mirrored under RTL), `Rectangle` in between. `OUTER_CORNER_RADIUS = 20.0` (half of
  `ContainerHeight`) is exported, since our per-corner shapes take a radius rather than a percent.
- Row: a `MeasurePolicy` (TabRow is the precedent). Measure every item with loose constraints, take
  the widest natural width, clamp to the 58 px minimum, place item `i` at `x = i × (W − 1)` with
  `width = W`; the row is `n × W − (n − 1)` wide and `max(40, tallest item)` high, items vertically
  centred. When the parent is narrower than that, shrink `W` to fit (a superset of Compose, which would
  overflow — see §3).
- Item: one node with the shape's background + 1 dp border, `ripple_with_shape`, and a two-slot
  content policy (`[icon slot, label]`) that places the label at `26 + offset`, where `offset` is a
  `State<f32>` animated in the measure policy between `0` (icon visible) and `-13` (icon hidden) — the
  same measure-time animation pattern TabRow uses for its indicator.
- Icon: the Material "check" filled path already present in `checkbox.rs` (24 dp viewBox), drawn by a
  `.draw` closure that applies the progress as a scale about the bottom-left corner plus alpha, so the
  enter animation matches Compose's `scaleIn(0, bottom-left) + fadeIn` and the exit is instant.
- Colors: the 12-field set with the same four-state resolution; disable state drops the callbacks.
- Optional slots: `.inactive_icon(...)` (crossfaded with the active one, as Compose does when
  `inactiveContent != null`), `.content_padding(...)`, `.border(...)`, `.interaction_source(...)`.

## 3. Every difference, and how it gets closed

| # | Difference | Verdict | How |
|---|---|---|---|
| 1 | No `selectableGroup()` / `Role` / `progressBarRangeInfo` semantics | **Not closed now** | winia has no semantics layer and no OS bridge (Windows UIA / Linux AT-SPI). Write `docs/semantics-gap.md` instead: the gap, Compose's API surface, and the "model first or bridge first" trade-off. A model with no consumer is likely to be the wrong shape (Compose's semantics has merged/unmerged trees, actions, collections). |
| 2 | No sibling z-order (`interactionZIndex`) | **Closed by a small framework branch** | See §4. The component then applies `z_index` per item. |
| 3 | `IntrinsicSize.Min` | **Not a gap** | Our row is content-sized the same way; the only difference is that we shrink instead of overflowing when the parent is too narrow. General intrinsic measurement (a measure protocol + second pass) is its own project and this component does not need it. |
| 4 | Check-in animation "approximate" | **Closed in behaviour** | Implement the structure exactly (scale from the bottom-left + fade, instant exit) with our own spring parameters, documented — Compose itself does not use published tokens here. |
| 5 | No per-item `contentPadding` / `border` / `interactionSource` overrides | **Closed** | Plain builder pass-throughs (~20 lines). |
| 6 | `Icon(active, activeContent, inactiveContent)` crossfade | **Closed** | `.inactive_icon(...)` + the existing `Crossfade`. |

## 4. The one framework addition: `z_index`

Why a framework change at all: painting is `for &child in &node.children` (`render.rs:1438`) and hit
testing is the same list reversed (`layout/node.rs:929`) — that comment already calls the reverse
order "z-order semantics", i.e. the layering concept exists but cannot be steered.

Plan (branch `z-index`):

1. `ModifierElement::ZIndex(f32)` + `Modifier::z_index(f32)` (plus the `PartialEq` and `Debug` arms —
   a new element missing from `PartialEq` silently breaks Skip). The value MUST take part in the
   element fingerprint: a changed z changes the paint order.
2. Cache it where children are placed: `LayoutNode.z_index` per node, and `children_have_z: bool` on
   the parent (one scan of the child list in the pass that already touches every child).
3. Render: when `children_have_z` is false (today's only case) keep the direct loop — zero cost; when
   true, paint a stable sort of the children by z (equal z keeps tree order).
4. Hit test: walk the same order in reverse, so a higher-z sibling receives the press first.
5. Tests: a pixel test (two overlapping siblings; z decides which one's color is visible), a hit-test
   test (the higher-z sibling gets the press), and an equality/fingerprint test for the new element
   (the classic stale-paint trap for a new modifier element).

Cost: ~80–120 lines including tests. Depends on nothing; unblocks the component's difference #2.

## 5. Order of work

1. **`z-index`** (framework): element + cached flag + render/hit order + the three tests. Gate: the
   filtered lib tests for the touched areas (layout/render) plus one UI test that presses the
   overlapping pair.
2. **`segmented-button`**: component → lib tests (pure `item_shape` and the color table; row layout:
   equal widths = widest item, `n × W − (n − 1)`, 40 px height, 58 px minimum, the shrink fallback;
   pixels: checked container, transparent+outline unchecked, a shared edge exactly 1 dp, the label's
   colors, the disabled palette, the check icon present and the label at 13 px vs 26 px) → a UI test in
   a real window (a 3-item row with a tag per item: click items 1 and 3, watch the printed selection
   follow; a multi-choice row toggling two items independently) → `docs/segmented-button.md` + a demo.
3. **`docs/semantics-gap.md`** (small): the gap, the Compose API it corresponds to, the model/bridge
   trade-off, and what a first slice would be if someone picks it up.

## 6. Risks

- **The label-offset animation lives in a measure policy.** The pattern is proven (TabRow's indicator
  animates a `State` read at the top of `measure`), but it is per item, so each item's policy owns its
  own state and must read it before measuring children to register the dependency.
- **The 1 dp shared edge in pixel tests.** Assert it by sampling two adjacent columns across the edge
  and requiring the border color on exactly one of them; a 2 dp line (the failure mode of getting the
  overlap wrong) would fail that.
- **`z_index` fingerprint hygiene** (§4.1) — the one place a mistake is invisible until something
  animates.
- **RTL**: shapes are chosen by direction inside `item_shape`; a test must cover both directions, since
  an RTL row would otherwise round the wrong ends.

## 7. Backlog (other things in flight)

- **SwipeToDismissBox**: file downloaded (`/tmp/compose-ref/SwipeToDismissBox.kt`), not read yet.
- **Runtime system-theme switching**: `WiniaTheme::auto` detects the mode once per composition and
  winia never handles `WindowEvent::ThemeChanged`, so switching the OS theme while the app runs does
  not update. ~30–40 lines (store the mode, request recomposition, read the stored mode in
  `is_system_dark_theme`) + a unit test.
- **Demo theme sweep**: 29 demos are still pinned to `WiniaTheme::light`, and some carry hard-coded
  grey labels that only read on a light page.
- **Visual sweep of existing components**: render each component's edge states (min/max, disabled,
  empty, tiny, RTL, both schemes) to PNGs and look at them — the range slider's two appearance bugs
  were both found by looking, not by reasoning.
- **Semantics layer** (model + OS bridge): the largest item here, deliberately not started.
- **Known pre-existing bug**: in `bottom_sheet_demo` a drag inside the sheet's list dismisses the sheet
  (identical on the v2 baseline; needs its own investigation).
