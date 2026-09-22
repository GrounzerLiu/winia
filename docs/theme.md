# Theme

Material 3 color scheme, typography and direction, provided to a subtree through composition locals, plus
the route by which a RUNNING window follows a theme change.

## Providing a theme

```rust
winia::run_app!(|ctx| {
    WiniaTheme::auto(ctx, |ctx| {          // or ::light / ::dark / ::with_theme(colors)
        Window::new().size(460.0, 760.0).title("Demo").build(ctx, demo);
    });
});
```

`WiniaTheme::auto` resolves the system mode, the others take a palette. All of them go through one place
(`provide_spec`), which resolves the palette and pushes four locals — `LOCAL_COLORS`, `LOCAL_TYPOGRAPHY`,
`LOCAL_DIRECTION`, `LOCAL_CONTENT_COLOR` — around the content. Reads are `WiniaTheme::colors()`,
`::typography()`, `::direction()`, `::content_color()`, and a component reads them while it builds: a
component's colors are therefore fixed at the moment its group composed.

There is no ambient default theme: outside any `WiniaTheme` node the locals answer with the plain light
palette (and black content color), which is what makes a bare `Composer` test behave like the old flat
theming.

## Following the system at runtime

Four pieces:

1. **The mode** (`ui::theme`): `set_system_dark_mode(Some(false) | Some(true) | None)` pins light, pins
   dark, or hands the decision back to the system. Windows raises `WindowEvent::ThemeChanged` (macOS too;
   X11/Wayland never do) and winia records that as an OBSERVATION — `note_platform_theme` — not as a pin:
   pinning from the event would freeze the app on the first report and ignore every later system change.
   Resolution order is pinned mode → platform report → `dark_light::detect()`.
2. **The window's theme cell** (`WindowTheme`): what the declaring tree provides — the INTENT to resolve
   (`ThemeSpec`: `Auto` follows, `Fixed(palette)` is an application's own choice), the TYPOGRAPHY and the
   DIRECTION — plus the palette the intent last resolved to, shared between the `Window` node that manages
   the window and the window itself. The node samples all of it (`current_theme_spec()`,
   `WiniaTheme::typography()`, `WiniaTheme::direction()`) EVERY frame and publishes it, so an application
   may switch which theme node wraps a window; the window's content wrapper composes under the cell's
   already-resolved palette (a Material scheme is not free to build, so an idle frame must not rebuild
   one), and a tracked read of the mode keeps the wrapper's composer awake enough to be woken at all when
   the mode changes.
3. **The epoch** (`system_theme_epoch`): how many times the system theme may have changed. Per WINDOW
   state, deliberately: one process-wide "pending" flag is consumed by whichever window renders first,
   which left every other window — and every sub-window of a tree that switched its own theme node — on its
   old palette.
4. **The frame step** (`PerWindow::refresh_theme`): re-resolve if the epoch or the intent moved, and when
   the palette is not what the window has drawn, set the window's own snapshot (the surface clear color),
   dirty the window's composer and every popup's, and request a redraw. No component looks at the theme by
   itself — they resolved their colors when they composed — so the tree has to run again.

Two events feed it: a platform `ThemeChanged` (recorded, then every window is asked for a frame) and an
application's `set_system_dark_mode` (bumps the epoch and wakes every composer that follows the system,
through the reactive mode value).

## The bugs this shape exists to prevent

Both were measured, and both are pinned by tests:

- **The palette pinned in the content closure.** The first version captured the resolved `ThemeColors` into
  the window's content closure, which runs every frame, so every frame re-provided the palette sampled when
  the window was created: the surface behind the tree flipped while every component kept its startup
  colors (dark-scheme fills on a freshly light page). Hence a cell that is re-resolved, not a value.
- **The intent sampled once.** Sampling only where the window is created left `PerWindow` with an immutable
  spec: a sub-window whose declaring tree switched from `light` to `dark` kept the palette it started with,
  and a window opened through the public API with no theme was pinned to light. Hence a cell the declaring
  tree PUBLISHES to every frame (and `None` on `open_window_with_title` now means `Auto`, i.e. follow).
- **One global pending flag.** It made only the first-rendering window follow. Hence the epoch, which each
  window compares against its own applied state.
- **Typography and direction re-provided as defaults.** The wrapper pushed `Typography::default()` and LTR
  on every frame, so a type scale (or an RTL direction) set around a `Window` node survived exactly one
  frame — and nothing would have reported it, the window just looks default. The cell carries both now, and
  a change in either reads as "what was drawn is wrong" even when the palette did not move.

## What follows and what does not

| | A theme change reaches it |
|---|---|
| The window's tree (`Auto` window) | yes — the cell re-resolves, `refresh_theme` re-runs the composers |
| The window's own snapshot (surface clear color) | yes — same step |
| A popup that is ALREADY OPEN | yes — the declaring tree re-runs and hands it a fresh `CompositionLocal` snapshot (its own composer is dirtied too). `an_open_popup_follows_the_theme` pins this. |
| Other windows (multi-window apps) | yes — each window keeps its own epoch, so a window that has not re-resolved yet is not skipped |
| Every window, when the platform event arrives | yes — the handler asks all of them for a frame |
| Typography / direction set around the `Window` node | yes — the declaring tree publishes both every frame (`a_window_content_follows_published_typography_and_direction`) |
| A `Fixed` window (an app that pinned a theme) | no, by design: it re-resolves the same palette and stops before dirtying anything |

## Tests

- `ui::theme::tests::theme_spec_resolves_auto_per_call_and_fixed_never` — the spec's semantics.
- `ui::theme::tests::a_theme_provide_records_its_spec_for_its_content_only` — the innermost spec wins and
  the enclosing one comes back (with `dark` inside `auto`: `light` would be indistinguishable from the
  fallback).
- `ui::theme::tests::a_theme_change_needs_the_subtree_dirty` — a mode change alone changes nothing until
  the composer is told (the middle step asserts exactly that).
- `ui::theme::tests::a_window_content_follows_its_cell` — the window shape, end to end at the composer
  level, through BOTH routes: the system epoch and a published intent.
- `ui::theme::tests::a_window_content_follows_published_typography_and_direction` — the type scale and
  direction a window was declared under reach its content, and a published change reads as "re-run the
  tree" with no color change at all.
- `app::window_theme_tests::a_theme_change_reaches_every_window` — two windows, one epoch: both follow; a
  pinned window does nothing and an idle one does no work (including a published type scale).
- `ui_test.rs::theme_follows_the_windows_own_switch` — a real window, its own Light/Dark switch, pixels.
- `ui_test.rs::an_open_popup_follows_the_theme` — a popup left open across the switch.
- `ui::pixel_line_parsing` (in `tests/ui/mod.rs`) — the text pixel read: frame coordinates are physical
  pixels, not bytes (parsing them as bytes rejected every frame whose point sat past 255, i.e. a 175%/200%
  scaled display), and both miss forms are "no color" while still naming the frame.

The mode is process-global, so the tests that move it serialize on `ui::theme::theme_mode_test_lock()`.
