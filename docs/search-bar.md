# SearchBar / DockedSearchBar (cf. Compose Material3 SearchBar)

> Status: in `exp/search-bar`. Classic active/inactive semantics; upstream's new
> split API (`SearchBarState` + `ExpandedFullScreenSearchBar` /
> `ExpandedDockedSearchBar`) is folded into one classic-shaped component.
> Reference source (sparse checkout, read-only):
> `D:/any/androidx-ref/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/SearchBar.kt`

## 1. API

```rust
// Fullscreen: tap pill → fullscreen Dialog overlay with results.
SearchBar::new()
    .state(state.clone())
    .on_search(move |q| state.close())   // convention: deactivate inside onSearch
    .placeholder(|ctx| { Text::new("Search…").build(ctx); })
    .leading_icon(|ctx| { Icon::svg_path(SEARCH_ICON_PATH).size(24.0).build(ctx); })
    .build(ctx, |ctx| {
        // results content (ColumnScope equivalent: plain content closure)
        LazyColumn::new() /* filtered by state.query_text() */ .build(ctx);
    });

// Docked: same collapsed pill; results drop down under the bar (Popup).
DockedSearchBar::new()
    .state(state.clone())
    /* same slots */
    .build(ctx, |ctx| { /* bounded-height results */ });
```

- `SearchBarState { query: State<TextFieldValue>, active: State<bool> }` —
  `query_text() / set_query() / open() / close() / is_active()`.
- `SearchBarColors { container, divider }` (upstream has exactly these two;
  input-field colors travel separately via `TextFieldColors`).
- `SearchBarDefaults`: `input_field_shape()` (Pill), `full_screen_shape()`
  (Rectangle), `docked_shape()` (rounded 12), `docked_gap()` (2.0),
  `colors(theme)`.
- `SEARCH_ICON_PATH` (Material Icons search, 24 viewport), `BACK_ICON_PATH`
  (arrow_back, expanded default leading), `SEARCH_BAR_HEIGHT` (56.0).

## 2. Behavior

- **Collapsed**: pill Surface + read-only InputField; tap anywhere opens.
  Expanded leading defaults to a back arrow (upstream fullscreen navigation
  icon); caller `leading_icon` overrides it.
- **Expanded (SearchBar)**: fullscreen `Dialog` overlay (modal,
  outside-tap dismisses, `expand_fade` 400ms) + editable field + `Divider` +
  content. Collapsed pill stays composed underneath (stable anchor slot).
- **Expanded (DockedSearchBar)**: `Popup` anchored `BottomLeft` under the bar
  + gap 2, rounded 12, same x/width as the bar (verified against upstream
  `DockedSearchBarLayout`: content `maxWidth = inputFieldWidth`, placed at
  `(0, inputFieldHeight)`). **No scrim in v1** (upstream default has one).
  Dropdown `Surface` takes the bar's `shadow_elevation` (default 0, upstream
  parity — set e.g. 4.0 to float).
- **Search action**: single-line Enter fires `on_search(query)` (new
  `TextField::on_search`; multi-line Enter still inserts newline).
- **Close paths**: back arrow / **Esc** (app-level: topmost overlay closes first
  via `begin_overlay_close` + `on_dismiss`, and clears focus) / outside tap
  (Dialog/Popup dismiss) / `on_search` (caller) / result-tap (caller).
- **Animations** (verified against upstream `SearchBar.kt`):
  - Fullscreen: top-expand reveal + fade 400ms (approximates the bounds-morph
    expand; true shared-element morph needs anchor geometry).
  - Docked dropdown: whole panel slides down from half-height above, clipped
    to its settled bounds (upstream `slideIn(y=-height/2)`), 350ms
    EaseOutCubic; exit mirrors. Content is a plain `Column` (upstream parity —
    all items composed upfront so the reveal is coherent).
  - Framework note: `render_overlays` clip rects must use device px
    (`size * scale` — the canvas isn't content-scaled yet at clip time, same
    as the scrim rect). Using layout units clips only 1/scale of the height
    and reads as a "small panel snapping to full" at animation end.

## 3. Framework fixes (in `app.rs` / `overlay.rs`, shared by all overlays)

- **Overlay focus + typing**: overlay content previously received only
  `on_click` — `TextField` focus (via `on_press → request_focus`) never fired
  inside overlays, so overlay inputs could not be typed into. Now:
  `OverlayWindow` tracks `focused_id/focused_slot_key`; `overlay_down`
  focuses the deepest focusable node + places the caret + dispatches pointer
  Down to the overlay arena; keyboard (`dispatch_key_to_overlay`,
  overlay-first), IME Preedit/Commit, and focus requests all route to the
  focused overlay; focus restores by slot key after layout; interaction
  `emit_focus/unfocus` syncs per overlay (cursor blink, focused colors).
  Drag-select inside overlay inputs is v1-out (tap-to-place caret works).
- **Popup anchoring**: `#[composable]` pushes a fresh scope at `build` entry,
  so `Popup::build`'s internal `prev_sibling_slot_key()` always sees an empty
  scope (None → window-alignment fallback). New `Popup::anchor_slot()` setter:
  callers capture `ctx.prev_sibling_slot_key()` in their own scope — or, when
  statement keys and node keys diverge (verified 0xec61… vs 0xcc11…), wrap the
  anchor in an explicit `start_restartable_group` and pass its key (the group
  node carries exactly that key; `composer_slot_key()` is wrong — it yields
  the last materialized *inner* node, e.g. the input text leaf at x=38).
- **Popup visible contract**: `Popup::build` must ALWAYS execute
  (`Popup::new(active)`); an early `if !active return` looks like a Skip frame
  and leaks a zombie overlay (tap-to-close appeared to do nothing).

## 4. Implementation notes (SearchBar)

- Input field = `TextField::no_container().single_line(true)` (existing slots:
  placeholder/leading/trailing all reused). Typing writes straight into
  `state.query` (it IS the TextField value State); `on_query_change` is
  notification-only.
- Collapsed bar forces `fill_max_width + height(56)`; user modifier appends
  outside and may override.
- TextField fixes made for SearchBar (in `text_field.rs`, all variant-gated —
  Filled/Outlined behavior unchanged):
  - placeholder builds without container visual (was `has_visual`-gated, v2 oversight);
  - bare (`no_container`) Input/Placeholder/Leading/Trailing center in the
    **measured** content height, not the constraint max (loose parents hand down
    screen-sized maxes);
  - new `on_search` callback (single-line Enter).

### Additional notes

- `SearchBarState` is intentionally minimal (`query + active: bool`) — upstream
  holds two `Animatable<Float>` (`progress` + `contentProgress`) plus
  `expandsToFullScreen/collapsedCoords`. Winia drives animation at the overlay
  container level (`OverlayAnimSpec`) instead of layout-level progress lerp.
- `DockedSearchBar` reuses `SearchBar` internals via composition (`inner:
  SearchBar`) rather than duplicating builder fields — keeps defaults in sync.

## 5. Comparison with upstream `SearchBar.kt` (4233 LOC, sparse checkout `D:/any/androidx-ref/.../SearchBar.kt`)

Upstream is no longer a single component — it ships **3 generations side by
side** (new split API + classic boolean compat). Winia deliberately folds the
new split API into the classic shape. Differences are therefore mostly **scope
choices**, not correctness bugs.

### 5.1 API shape

| Dimension | Compose upstream | Winia `winia/src/ui/search_bar.rs:27` | Verdict |
|---|---|---|---|
| Collapsed | `SearchBar(state, inputField: @Composable()->Unit, shape/colors/tonal/shadow)` — `SearchBarImpl:312` is just `Surface(shape){ inputField }` + `onGloballyPositioned{ collapsedCoords }`. `inputField` is caller-supplied `SearchBarDefaults.InputField` | `SearchBar::new().state().placeholder().leading_icon().trailing_icon().input_colors()` — `inputField` is synthesized internally as `TextField::no_container().single_line(true)` where `state.query` IS the `TextFieldValue`; `on_query_change` is notification-only | **Intentional simplification.** Upstream decouples the field to reuse `TextFieldState / InputTransformation / KeyboardOptions`; Winia's desktop use is simpler — inlining is more ergonomic. Cost: less `RowScope`-level customization of the field. |
| Expanded | 4 separate composables: `ExpandedFullScreenSearchBar:807 / ExpandedFullScreenContainedSearchBar:693` (Dialog fullscreen) + `ExpandedDockedSearchBar:1074 / ExpandedDockedSearchBarWithGap:964` (Popup dropdown, with/without gap) | `SearchBar` → fullscreen `Dialog`; `DockedSearchBar` → anchored `Popup` on the same `SearchBarState`; single `build(ctx, \|ctx\| content)` | **Folded.** Upstream lets callers pick per breakpoint (phone fullscreen vs tablet docked); Winia merges into two classic components (`docs/search-bar.md:5`). Sufficient for desktop; loses adaptive switching. |
| AppBar | `TopSearchBar:381` (`@Deprecated → AppBarWithSearch:456`) / `AppBarWithSearchImpl:540` — `SearchBar` as `Scaffold.topBar` with `navigationIcon/actions/contentPadding/windowInsets/scrollBehavior` and `derivedStateOf(overlappedFraction)` → `animateColorAsState` | Not implemented | **Gap.** Needed only if the bar lives in `Scaffold.topBar` and should react to scroll. |
| Compat | `SearchBar(inputField, expanded: Boolean, onExpandedChange, ...):1259` + `DockedSearchBar:1376` with `Animatable + PredictiveBackHandler + MutatorMutex` | No compat layer — `SearchBarState{ active: State<bool> }` directly | Upstream keeps it for binary compat; Winia does not need it. |

### 5.2 State

**Compose `SearchBarState:1429`** (`@Stable`):
`animatable: Animatable<Float>` (progress 0→1) + `contentAnimatable` (contentProgress) + `animationSpecForExpand/Collapse` (`SlowSpatial/DefaultSpatial/FastSpatial`) + `animationSpecForContentFadeIn/Out` (`snap`/delayed fade) + `expandsToFullScreen/collapsedCoords/LayoutCoordinates` + `progress/contentProgress/isAnimating/targetValue/currentValue(0.02 tolerance)` + `animateToExpanded/Collapsed/snapTo` + `Saver(progress+contentProgress)` + factories `rememberSearchBarState / rememberContainedSearchBarState / rememberSearchBarWithGapState:1631`.

**Winia `SearchBarState:29`**: `query: State<TextFieldValue> + active: State<bool>` + `open/close/query_text/set_query/is_active`. No `Animatable` — animation lives in `OverlayAnimSpec` at the overlay container.

This is an **architectural fork**: Compose drives geometry via `progress` lerp in `FullScreenSearchBarLayout:3770` (`lerp(collapsedBounds, expandedBounds, progress)`) + `graphicsLayer(alpha=contentProgress)`; Winia uses `Dialog/Popup + expand_fade(400ms)/slide_down(350ms)` — `docs/search-bar.md:59` notes `true shared-element morph needs anchor geometry`.

### 5.3 Layout & windowing

- Compose: `SearchBarLayout:3460 / DockedSearchBarLayout:3617 / FullScreenSearchBarLayout:3770` are pure `Layout` measures with `DockedExpandedTableMaxHeightScreenRatio` and `visible = progress>threshold && target==Expanded` anti-flicker. `WindowInsets:1960` (`systemBarsForVisualComponents / safeDrawing / imePadding / consumeWindowInsets`), `BasicEdgeToEdgeDialog`, and `PopupPositionProvider:1166` (`if(hasScrim) Zero else collapsedBounds.topLeft`, scrim `drawRect(alpha=progress)`).
- Winia: collapsed `Surface(Pill).fill_max_width().height(56)` + `TextField.no_container().height(56)` centered via measured-height centering (not constraint max); fullscreen `Dialog(modal).fill_max_size().Surface(Rectangle) + Column{ input(56) + Divider + content }`; docked explicit `anchor_key` Box wrapper → `Popup(BottomLeft+gap2)` + `Surface(Rounded12)`. `render_overlays` clip was fixed to `size*scale` device px (was 1/scale on HiDPI). No `WindowInsets` / `heightIn(min,max)` cap (demo caps docked to 5 rows).

### 5.4 Animation

| Upstream | Winia | Alignment |
|---|---|---|
| `MotionSchemeKeyTokens` spatial springs + `contentProgress` layered on `progress` (expand: content `snap` then fade-in; collapse: fade-out then container collapse) + `PredictiveBackHandler:1294` with `snapTo(1-transform(back.progress))` | Fullscreen `expand_fade 400ms` (reveal_top+fade), docked `slide_down 350ms EaseOutCubic y=-h/2` clipped to settled bounds (`fade:true` so enter/exit are symmetric) | **Approximate.** Spatial curves differ (`EaseOutCubic` vs M3 springs), but `d4c4cf1` moved docked from "content-level appear-slide" back to "overlay-canvas slide" — parity with `slideIn(y=-height/2)+fadeIn / slideOut+fadeOut`. |
| `LaunchedEffect(expanded){ animateTo(0/1, AnimationEnter/ExitFloatSpec) }` on the classic compat path | `active` bool directly drives `Dialog/Popup` `enter/exit_animation` | Equivalent. |

### 5.5 Scroll linkage — the largest gap

Compose `SearchBarScrollState:1753 + SearchBarScrollBehavior:1840 + EnterAlways:1882`: `scrollOffset/contentOffset/scrollOffsetLimit` + `overlappedFraction():1869` + `nestedScrollConnection(onPreScroll/onPostScroll/onPostFling)` + `draggable + layout(placeWithLayer(offset)) + onSizeChanged(limit=-height)` + `settleSearchBar:1950` decay+snap + factory `enterAlwaysSearchBarScrollBehavior:2086` consumed by `AppBarWithSearchImpl:602 .then(scrollBehaviorModifier)`.

Winia: `nested_scroll.rs` exists for `TopAppBar`, but `SearchBar` exposes no `SearchBarScrollState/scrollBehavior` and no `AppBarWithSearch` wrapper. Needed only if the bar is a scrolling top bar (`Scaffold.topBar`). Porting `SearchBar.kt:1869-1993` onto existing `NestedScrollConnection` is straightforward.

### 5.6 Other deltas

- **Colors**: Compose `SearchBarDefaults.colors(container=SearchBarTokens.ContainerColor, divider=SearchViewTokens.DividerColor, inputFieldColors)` + `containedColors(state)` switching on `isExpanded`; Winia `SearchBarColors{container,divider}` + `theme.surface_container_high/outline_variant`, `tonalElevation/shadowElevation` default `Level0` — parity.
- **Shapes**: `ContainerShape / FullScreenContainerShape / DockedContainerShape / dockedDropdownShape(12dp)` vs Winia `Pill / Rectangle / Rounded12` (Winia reuses one shape for bar+drops; upstream allows `shape` vs `dropdownShape` to differ).
- **Input**: `SearchBarDefaults.InputField` = `BasicTextField + TextFieldState + InputTransformation/OutputTransformation + KeyboardOptions(imeAction=Search) + TextSelectionColors` vs Winia `input_field:132` (`TextField::no_container().single_line + on_search`, `placeholder/leading/trailing` reused). Divider is configurable in both.
- **Semantics**: Compose `isTraversalGroup/stateDescription/contentDescription + LocalTextSelectionColors + LocalFocusManager`; Winia only `test_tag`.
- **Scrim**: Compose `ExpandedDockedSearchBarWithGap:972 ScrimTokens.ContainerColor@ContainerOpacity`; Winia `docs/search-bar.md:51 No scrim in v1`.
- **Predictive back**: Compose `MutatorMutex + PredictiveBackHandler` gesture-follow (`animationProgress.snapTo(1-transform(back.progress))`); Winia uses `Esc` for desktop (`docs/search-bar.md:18`).

### 5.7 What remains to be "like Compose" (priority order)

1. **Scrim** for docked-with-gap — `Popup` + `Box(fillMaxSize.drawRect(scrim, alpha=progress).clickable(onDismiss))`.
2. **Split API** for adaptive breakpoints — `ExpandedFullScreenSearchBar(state, inputField, content)` + `ExpandedDockedSearchBar(state, inputField, dropdownShape/gap, content)` so callers can switch by `WindowSizeClass`.
3. **True morph** — `collapsedCoords` + `progress` lerp of bounds/shape in a `FullScreenSearchBarLayout` (requires layout-level animation).
4. **`AppBarWithSearch + SearchBarScrollBehavior`** — if the bar lives in `Scaffold.topBar`.
5. **`WindowInsets`** — desktop does not need `safeDrawing/systemBars/imePadding` today; could be a passthrough `Modifier.windowInsetsPadding`.

> Summary: Winia chose "classic one-stop" — minimal API covering ~90% of desktop cases (fruit-filter demo validates the full flow). Framework fixes for overlay focus/keyboard/IME/anchoring/visible-contract are **generic** (all `Dialog/Popup` benefit) and the move from content-level to container-level slide is the **key parity fix**. Remaining gaps are optional Material completeness, not correctness defects.

## 6. Tests

| Test | Covers |
|---|---|
| `search_bar::inactive_hides_results_active_registers_overlay` | inactive→no overlay; open→1 modal Dialog overlay + pill persists; close→gone |
| `search_bar::query_state_round_trips` | query get/set |
| `search_bar::docked_inactive_hides_results` | docked collapsed: no results, no popup |
| `search_bar::docked_active_registers_anchored_popup` | docked open→1 non-modal overlay with resolving anchor; close→gone |
| `search_bar::defaults_shapes_are_distinct` | Pill vs Rectangle vs rounded-12 |
| `text_field::on_search_fires_on_single_line_enter` | Enter → on_search(value) |
| `text_field::on_search_absent_keeps_swallow` | no callback → still consumed |
| `text_field::placeholder_builds_without_visual` | placeholder without variant |

Demo: `winia/examples/search_bar_demo.rs` (fruit filter: fullscreen + docked,
pick-to-close both paths).
