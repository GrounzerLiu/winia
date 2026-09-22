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

Three pieces, none of them sufficient alone:

1. **The mode** (`ui::theme`): `set_system_dark_mode(Some(false) | Some(true) | None)` pins light, pins
   dark, or hands the decision back to the system. Windows raises `WindowEvent::ThemeChanged` (macOS too;
   X11/Wayland never do) and winia records that as an OBSERVATION — `note_platform_theme` — not as a pin:
   pinning from the event would freeze the app on the first report and ignore every later system change.
   Resolution order is pinned mode → platform report → `dark_light::detect()`.
2. **The window's spec** (`ThemeSpec`): a window's content closure runs on every frame, so what it carries
   is the INTENT — `Auto` (re-resolve per frame) or `Fixed(palette)` (an application's own choice). A
   window samples it where it is created (`Window::build` → `current_theme_spec()`, i.e. the innermost
   theme node in scope) and re-provides it every frame.
3. **The invalidation** (`app::apply_system_theme`): the app loop drains a pending theme change immediately
   before the frame's recomposition, refreshes the window's own snapshot (the color the surface is cleared
   with — this is what makes the backdrop flip) and calls `mark_content_dirty()` on the window's composer
   and every popup's.

## The bug this shape exists to prevent

The first version captured the resolved `ThemeColors` in the window's content closure and re-provided it
every frame. Everything else was right — the platform event arrived, the mode was stored, the composers
were dirtied, the tree recomposed — and the window still never changed color, because the wrapper handed
the tree the palette sampled at window creation. Measured in the demo: the surface behind the tree flipped
while every component kept its startup colors. Hence a spec (intent) rather than a palette, and hence
`a_window_content_re_resolves_the_theme_it_sampled` plus a real-window UI test
(`theme_follows_the_windows_own_switch`, via the debug server's `px` pixel read) — falsified by putting the
captured palette back, which fails it with "the light theme must paint a light surface, luma=18.9".

## What follows and what does not

| | A theme change reaches it |
|---|---|
| The window's tree (`Auto` window) | yes — the wrapper re-resolves, `mark_content_dirty` re-runs the groups |
| The window's own snapshot (surface clear color) | yes — from the window's spec |
| A `Fixed` window (an app that pinned a theme) | no, by design: `ThemeSpec::Fixed` re-provides its palette |
| **A popup that is already OPEN** | **no** — it composes under a `CompositionLocal` snapshot captured when it opened, and that snapshot holds the palette as a value. Reopening it picks up the new theme. |
| Typography / direction overridden by `with_typography` / `with_theme_and_direction` | no — the window's per-frame wrapper re-provides the defaults for both (pre-existing; unrelated to the theme mode) |

## Tests

- `ui::theme::tests::theme_spec_resolves_auto_per_call_and_fixed_never` — the spec's semantics.
- `ui::theme::tests::a_theme_provide_records_its_spec_for_its_content_only` — the innermost spec wins and
  the enclosing one comes back.
- `ui::theme::tests::a_theme_change_needs_the_subtree_dirty` — a mode change alone changes nothing until
  the composer is told (the middle step asserts exactly that).
- `ui::theme::tests::a_window_content_re_resolves_the_theme_it_sampled` — the window shape, end to end at
  the composer level: sampling, per-frame re-resolution, and the pixel flip.
- `ui_test.rs::theme_follows_the_windows_own_switch` — a real window, its own Light/Dark switch, pixels.

The mode is process-global, so those tests serialize on `THEME_TEST_SERIAL`.
