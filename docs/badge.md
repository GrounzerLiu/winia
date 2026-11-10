# Badge / BadgedBox (material3 alignment)

> Branch: `badges` (split from v2)
> Reference: material3 1.4.0 `Badge` / `BadgedBox` (Compose androidx-main `Badge.kt`)
> M3 spec: https://m3.material.io/components/badges/specs

## 1. API

```rust
// Small badge (no content — a 6×6 dot)
Badge::new().build(ctx);

// Large badge (a number or a short label)
Badge::new()
    .content(|ctx| { Text::new("3").build(ctx); })   // Compose's `content` (RowScope)
    .container_color(c)                              // Compose's `containerColor` (default Error)
    .content_color(c)                                // Compose's `contentColor` (default: see below)
    .modifier(Modifier)                              // the caller's modifier
    .build(ctx);

// BadgedBox: hangs the badge on an anchor (an icon, a label)
BadgedBox::new(|ctx| { Badge::new().build(ctx); })   // Compose's `badge` parameter
    .modifier(Modifier)
    .build(ctx, |ctx| { /* anchor content (an icon, …) */ });
```

- `Badge::content()` makes this a large badge (min 16×16, corner 8, horizontal padding 4,
  LabelSmall text 11sp/Medium in the container's paired content colour); without it the badge is a
  small dot (6×6, corner 3).
- `BadgedBox`'s badge closure is a constructor argument (required); the anchor is the build content
  closure, as with every other container component.
- A badge wider than 6dp counts as having content (Compose:
  `badgePlaceable.width > BadgeTokens.Size`) — the offset switches with it.
- **Content colour** defaults to `contentColorFor(containerColor)`, i.e. derived from the container
  (`error → onError`, `primary → onPrimary`, `surfaceContainerHigh → onSurface`, …), falling back to
  `LocalContentColor` for a colour that is not in the scheme; `.content_color(c)` overrides both.
- **The caller's modifier is the OUTERMOST link**: a `Badge(modifier = Modifier.padding(10.dp))`
  puts the padding OUTSIDE the badge's background (Compose chains
  `modifier.defaultMinSize().background().padding()`).

## 2. Defaults (against `BadgeTokens` v0_103 / the M3 spec)

| Item | Small badge | Large badge |
|---|---|---|
| Size | 6×6 (`Size`) | min 16×16 (`LargeSize`) |
| Corner | 3 (`CornerFull`) | 8 (`CornerFull`) |
| Horizontal padding | — | 4 (`BadgeWithContentHorizontalPadding`) |
| Container colour | Error | Error (`LargeColor`) |
| Content colour | — | `contentColorFor(containerColor)` (M3's `LargeLabelTextColor` is OnError only because the default container is Error) |
| Text style | — | LabelSmall (11sp / Medium 500) |
| Largest form | — | 16×34dp (M3: 99+) |

> The corner is the fixed 3 / 8 rather than a percentage `CornerFull`: `min_height = 16` keeps the
> short side at or above 16, so half of it is exactly 8 for a one-digit 16×16 badge and exactly 3
> for the 6×6 dot — the same answer `CornerFull` gives at every documented size (a multi-line 16×32
> and the 42×16 a 60pt font produces are still 8).

### BadgedBox placement (against Compose's `Badge.kt` layout)

| Form | Horizontal offset | Vertical offset | Meaning |
|---|---|---|---|
| No content | 6dp | 6dp | the badge's bottom-leading corner 6×6 from the anchor's top-trailing one (`BadgeOffset`) |
| With content | 12dp | 14dp | the leading edge 12 from the anchor's trailing edge, the bottom edge 14 from its top (`BadgeWithContent*Offset`) |

## 3. Implementation

- **Badge**: `default_min_size` for the size + `background` (the CornerFull shape) + horizontal
  content padding, then `WiniaTheme::with_content_color` + `ProvideTextStyle` (LabelSmall) around a
  Row (vertically centred) when it has content — Compose's `ProvideContentColorTextStyle`.
  A content-colour change is tracked through a backchannel that remembers the previous value and
  calls `mark_subtree_dirty()` when it moved: a `CompositionLocal` does not invalidate its readers,
  and the inner Row group declares only arrangement/alignment/spacing, so entering the outer group
  alone never reaches the label (the same defect `WiniaTheme::provide_resolved` records for a theme
  switch).
- **BadgedBox**: a custom `MeasurePolicy` (`BadgedBoxPolicy`) — the anchor is measured with the
  full constraints and the badge with a loosened height (`constraints.loosen_height()`; Compose is
  `constraints.copy(minHeight = 0)`, so **only the height is loosened, not the width** — the text
  must not claim more height than it needs); the reported size is the anchor's; the badge is placed
  at `(anchorW - offset_x, -badgeH + offset_y)` (it may overhang the anchor's top and trailing
  edges — both rendering and hit testing handle negative coordinates).
- Composition structure: `start_container` (the custom policy) → two `start_restartable_group`
  calls (anchor centred / badge centred), each skipping independently.
- The badge's container colour and size are static (no animation — a badge carries no state
  transition; Compose is the same); the content colour does follow the container colour and the
  theme (see above).
- 15 tests: structure/placement (the dot at (18,0), the large badge at (12,-2), the RTL mirror),
  alignment (the content-colour derivation, baselines reporting only the anchor, the min width
  loosening only the height), reactivity (the label following a container colour that moves at
  runtime, an explicit content colour taking effect), caller-modifier semantics (shrinkable to 2dp,
  the token size applying without an override), and pixels (the dot's Error fill, the large badge's
  fill plus label, a render-level BadgedBox check).

## 4. Not implemented (gaps against Compose)

- `BadgeTopRuler` / `BadgeEndRuler` (the navigation-bar boundary clamp that keeps a badge inside
  the bar); winia has no NavigationBar component, so the badge is placed against the anchor alone
  (to be added if a navigation bar lands);
- **the `RowScope` receiver for `content`**: Compose declares
  `@Composable (RowScope.() -> Unit)?`, winia takes `Box<dyn FnOnce(&mut ComposeCtx)>`.
  `layout::RowScope` now EXISTS (`Row::build` hands its content a scope that derefs to
  `ComposeCtx`, so `scope.weight(…)` is available), but `Badge::content`'s signature still takes a
  bare closure — weighting badge content therefore needs a nested `Row` inside the content to get a
  scope;
- `ProvideContentColorTextStyle`'s textStyle push-down (LabelSmall) is approximated;
- accessibility semantics (reading the badge content aloud) — winia has no semantics system yet.

