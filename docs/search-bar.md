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
- **Expanded (SearchBar)**: fullscreen `Dialog` overlay (`no_animation`,
  modal, outside-tap dismisses) + editable field + `Divider` + content.
  Collapsed pill stays composed underneath (stable anchor slot).
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
  - Fullscreen: fade 400ms (shared-bounds expand is out of scope).
  - Docked dropdown: panel bg appears instantly full-size; items slide
    64px → 0 in 300ms EaseOutCubic *inside* the panel, clipped by the
    dropdown shape — exactly the upstream structure (box laid out full,
    content `slideIn` within). A whole-canvas slide was tried first and
    rejected: bg travels with content = growing-panel illusion + end snap.
    Exit fades 150ms.

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

## 4. Tests

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
