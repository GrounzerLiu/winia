# Window insets and pull-to-refresh

The last two Compose foundations ported: WindowInsets with its padding modifiers, and material3's
PullToRefreshBox.

## Window insets

The window declares how much of its surface system chrome takes:

    WindowInsets {
        status_bars, navigation_bars, ime, display_cutout, system_gestures, tappable_elements,
    }

each an Insets { left, top, right, bottom } in logical pixels. Every window owns a
WindowInsetsController (provided by the Window component); writing it re-measures only the nodes
whose inset padding asked for it.

Modifiers: window_insets_padding(kind) / consume_window_insets(kind), plus the Compose shortcuts
status_bars_padding, navigation_bars_padding, system_bars_padding, ime_padding, display_cutout_padding,
system_gestures_padding, tappable_element_padding, safe_drawing_padding, safe_gestures_padding,
safe_content_padding and mandatory_system_gestures_padding.

Consumption is per source and accumulates down the measure tree: a node that applies systemBars
padding consumes the status and navigation sources for its subtree, so a nested navigationBars_padding
pads by zero. A consume-only element marks the insets without padding by them.

### A custom window decoration instead of the Windows status bar

Windows has no status bar. The intended pattern is a transparent window decoration that replaces the
native title bar and plays Android's status-bar role:

1. the decoration component (inside the window content) draws the bar and knows its height h;
2. it publishes that height: insets::set_status_bars(Insets::new(0.0, h, 0.0, 0.0)) — or
   WindowInsetsController::set_status_bar_insets;
3. the page's root modifier keeps content clear of it with system_bars_padding(), and a
   navigationBars_padding() does the same for a bottom bar;
4. a platform backend can push the full set at once with WindowInsetsController::set when the OS
   reports real values (Android, or a future winit inset event).

The controller is a per-window State, so writing it is idempotent and a write from an event handler
schedules the frame that applies the new padding. The free `set_status_bars` / `set_navigation_bars`
/ `set_ime` helpers resolve the ENCLOSING controller from the composition-local stack, so they only
target the window while composition is running; a button handler or position callback runs outside
that stack and would silently write the process-wide fallback controller instead. Capture the
controller during composition and keep it for the handler:

    let insets = winia::window::insets::current();          // while composing
    Button::new().on_click(move || insets.set_ime_insets(Insets::new(0.0, 0.0, 0.0, 240.0)))

NavigationRail's WindowInsets is now the same foundation Insets type.

## PullToRefreshBox

    PullToRefreshBox::new(is_refreshing, on_refresh)
        .modifier(Modifier::new().size(320.0, 200.0))
        .build(ctx, |ctx| { /* a scrollable child */ });

The box attaches a NestedScrollConnection to itself. When the inner scrollable cannot consume a drag
(it is at the top), the leftover arrives in on_post_scroll and becomes a pull: the content translates
down and a material3 LoadingIndicator morphs as the pull approaches the threshold (the determinate
form, whose progress is offset / PULL_THRESHOLD). Crossing PULL_THRESHOLD (64 px of content offset;
the drag is damped by 0.5) runs on_refresh once, rests the content at REFRESHING_OFFSET and switches
the indicator to the contained indeterminate LoadingIndicator — the same configuration the
LoadingIndicator demo uses, which runs the seven-shape morph loop plus its own infinite 360-degree
rotation (4666 ms) — until is_refreshing turns false, when it settles back. The indicator is 48 px
and sits 8 px above the content so it never crowds the list. The demo's refresh lasts 5 s so the
loop is easy to watch.

Implementation note: PullToRefreshBox::build declares ctx.changed(&self.is_refreshing). Without that
the component group Skips when the caller flips is_refreshing and the determinate glyph stays frozen
in place of the rotating one. A
pull that falls short settles back as soon as the drag pauses; the nested-scroll protocol has no
release event, so the return is a delayed tween re-armed on every delta rather than a release hook.

Scope: drag input only (a wheel is not a pull); the indicator is the LoadingIndicator component, not
material3's exact PullToRefreshDefaults indicator; there is no fling/release trigger separate from the
threshold.

## Demo

`cargo run --example insets_pull_refresh_demo` is the end-to-end version: an app-owned Surface
decoration publishes the status-bar inset, a material3 NavigationBar publishes the navigation-bar
inset, a simulated keyboard publishes the IME inset, and a Card + 24 ListItems pull to refresh through
the LoadingIndicator. It uses the component library throughout (Surface, Card, ListItem, NavigationBar,
Button, LoadingIndicator).
