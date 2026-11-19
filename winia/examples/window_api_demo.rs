//! Window API demo — identity, live state, capabilities, persistence and close negotiation.
//!
//! `Window` is a declaration, not a window object. Its `build` returns a `WindowId`, and every
//! live attribute is read or changed through `WindowId::state()`. The cards below exercise the
//! whole surface from ordinary windows:
//!
//! - identity: raw id, realized/focused flags, and the platform capability table;
//! - multi-window: a second `Window` declaration on desktop; the first window hides and shows it
//!   through `WindowState::set_visible`, while `ctx.windows()` keeps reporting both declarations;
//! - live state: size, position, placement, title, visibility and resizable, with buttons that
//!   queue platform writes and read the next frame's echo;
//! - close negotiation: `on_close_requested` is a notification, so the handler here vetoes by doing
//!   nothing and records the reason in the window title;
//! - persistence: `.key("window-api-demo")` gives the window a stable saveable key; the run counter
//!   is restored from disk when a store is configured.
//!
//! Run with: `cargo run -p winia --example window_api_demo`.

use letclone::clone;
use std::cell::Cell;
use std::rc::Rc;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    // Persistence is opt-in. Without a store the key still works for a close-and-reopen in the
    // same process; on desktop the example writes under the workspace target directory.
    #[cfg(not(target_os = "android"))]
    winia::runtime::saveable::persist::set_store(Some(
        winia::runtime::saveable::persist::PersistStore::open("target/window-api-demo-state"),
    ));

    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            app(ctx);
        });
    });
}

#[composable]
fn app(ctx: &mut ComposeCtx) {
    // Declare the first window first so it keeps WindowId 1; the second id is published through a
    // State because the first window's content needs it for the hide/show controls.
    let second_slot = ctx.remember(|| None::<WindowId>);
    let body_slot = second_slot.clone();
    Window::new()
        .title("Window API demo")
        .size(900.0, 760.0)
        .key("window-api-demo")
        .on_close_requested(|id, reason| {
            // A veto: doing nothing keeps the window open. The title makes the request visible.
            id.state()
                .set_title(&format!("Window API demo — {reason:?} close vetoed"));
        })
        .build(ctx, move |ctx| {
            let slot = body_slot.clone();
            settings::shell("Window API", ctx, move |ctx, padding| {
                body(ctx, padding, slot.get());
            });
        });

    // The second window is a static declaration on desktop. Android reports multi_window = false
    // (several declarations share one Activity surface), so it keeps one window there.
    #[cfg(not(target_os = "android"))]
    {
        let second = second_window(ctx);
        second_slot.set(Some(second));
    }
}

/// The second declaration. Closing it hides it rather than destroying it, so the first window can
/// show it again; the declaration stays alive and keeps its WindowId.
#[composable]
fn second_window(ctx: &mut ComposeCtx) -> WindowId {
    Window::new()
        .title("Window API demo — second window")
        .size(620.0, 480.0)
        .on_close_requested(|id, _reason| id.state().set_visible(false))
        .build(ctx, |ctx| {
            settings::shell("Second window", ctx, second_body);
        })
}

#[composable]
fn body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding, second: Option<WindowId>) {
    let id = ctx.window_id();
    let state = id.state();
    let caps = id.capabilities();
    let refresh = ctx.remember(|| 0u32);
    let second_visible = second.map(|window| window.state().visible());
    let scroll = ctx.remember(ScrollState::new).get();
    let runs = winia::runtime::saveable::remember_saveable::<i64>(ctx, "runs", || 0);

    // Count once per window content, not once per recomposition.
    let counted = ctx.remember(|| Rc::new(Cell::new(false))).get();
    if !counted.get() {
        counted.set(true);
        let runs = runs.clone();
        winia::effect::side_effect(ctx, move || runs.set(runs.get() + 1));
    }

    // Reading the tick is what refreshes the readouts after a button changes a window attribute.
    let _ = refresh.get();

    let size = state.size();
    let position = state.position();
    let placement = state.placement();
    let title = state.title();
    let resizable = state.resizable();
    let visible = state.visible();
    let windows = ctx.windows();
    let window_list = windows
        .iter()
        .map(|window| window.raw().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let second_status = match (second, second_visible) {
        (None, _) => "not declared on this platform".to_string(),
        (Some(window), Some(true)) => format!("WindowId {} visible", window.raw()),
        (Some(window), Some(false)) => format!("WindowId {} hidden", window.raw()),
        (Some(window), None) => format!("WindowId {}", window.raw()),
    };

    Column::new()
        .spacing(16.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding_sides(scaffold.start, scaffold.top, scaffold.end, scaffold.bottom)
                .vertical_scroll(scroll)
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            Text::new(
                "This window is a declaration. Everything below is read from the WindowId it returned, \
                 and every write is queued to the next frame rather than touching the OS from compose.",
            )
            .font_size(14.0)
            .build(ctx);

            card(ctx, "Identity", |ctx| {
                metric(ctx, "WindowId", format!("{}", id.raw()));
                metric(ctx, "realized", format!("{}", id.is_realized()));
                metric(ctx, "focused", format!("{}", id.is_focused()));
                metric(ctx, "ctx.windows()", format!("{} window(s): {window_list}", windows.len()));
                metric(
                    ctx,
                    "capabilities",
                    format!(
                        "position={} placement={} multi_window={} native_close={}",
                        caps.position, caps.placement, caps.multi_window, caps.native_close
                    ),
                );
            });

            card(ctx, "Multi-window", |ctx| {
                metric(ctx, "multi_window capability", caps.multi_window.to_string());
                metric(ctx, "second window", second_status);
                metric(ctx, "declared windows", format!("{}: {window_list}", windows.len()));
                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Button::filled()
                        .enabled(second.is_some() && second_visible != Some(true))
                        .on_click({
                            clone!(refresh);
                            move || {
                                if let Some(window) = second {
                                    window.state().set_visible(true);
                                    refresh.set(refresh.get() + 1);
                                }
                            }
                        })
                        .build(ctx, |ctx| Text::new("Show second window").build(ctx));
                    Button::outlined()
                        .enabled(second_visible == Some(true))
                        .on_click({
                            clone!(refresh);
                            move || {
                                if let Some(window) = second {
                                    window.state().set_visible(false);
                                    refresh.set(refresh.get() + 1);
                                }
                            }
                        })
                        .build(ctx, |ctx| Text::new("Hide second window").build(ctx));
                });
                Text::new(if !caps.multi_window {
                    "This platform reports multi_window = false: several declarations share one Activity surface, so the demo keeps a single window."
                } else if second_visible == Some(true) {
                    "The second window is visible. Hiding it is a live-state write, not a teardown; the declaration and its WindowId stay."
                } else {
                    "The second window is hidden. Showing it maps the same window again; ctx.windows() still reports both declarations."
                })
                .font_size(14.0)
                .build(ctx);
            });

            card(ctx, "Live state", |ctx| {
                metric(ctx, "size", format!("{:.1} x {:.1}", size.0, size.1));
                metric(ctx, "position", format!("{position:?}"));
                metric(ctx, "placement", format!("{placement:?}"));
                metric(ctx, "title", title.clone());
                metric(ctx, "resizable", format!("{resizable}"));
                metric(ctx, "visible", format!("{visible}"));

                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Button::filled()
                        .on_click({
                            clone!(state, refresh);
                            move || {
                                state.set_title("Window API demo — retitled");
                                refresh.set(refresh.get() + 1);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Retitle").build(ctx));
                    Button::filled_tonal()
                        .on_click({
                            clone!(state, refresh);
                            move || {
                                state.set_size(size.0 + 80.0, size.1 + 40.0);
                                refresh.set(refresh.get() + 1);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Grow").build(ctx));
                    Button::outlined()
                        .on_click({
                            clone!(state, refresh);
                            move || {
                                state.set_resizable(!resizable);
                                refresh.set(refresh.get() + 1);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Toggle resizable").build(ctx));
                    Button::outlined()
                        .enabled(caps.placement)
                        .on_click({
                            clone!(state, refresh);
                            move || {
                                let next = match placement {
                                    WindowPlacement::Fullscreen => WindowPlacement::Windowed,
                                    _ => WindowPlacement::Fullscreen,
                                };
                                state.set_placement(next);
                                refresh.set(refresh.get() + 1);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Toggle fullscreen").build(ctx));
                });
            });

            card(ctx, "Close negotiation", |ctx| {
                Text::new(
                    "id.close() sends a request. The handler above neither destroys nor hides the \
                     window, so the request is vetoed; it writes the reason into the OS title instead. \
                     id.destroy() would skip the handler and close this window.",
                )
                .font_size(14.0)
                .build(ctx);
                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Button::text()
                        .on_click({
                            clone!(id);
                            move || id.close()
                        })
                        .build(ctx, |ctx| Text::new("Request close (vetoed)").build(ctx));
                    Button::outlined()
                        .on_click({
                            clone!(id);
                            move || id.destroy()
                        })
                        .build(ctx, |ctx| Text::new("Destroy this window").build(ctx));
                });
                Text::new(
                    "Destroy skips the handler. When this was the last window, the default Exit \
                     policy also ends the process and flushes the saved state.",
                )
                .font_size(14.0)
                .build(ctx);
            });

            card(ctx, "Persistence", |ctx| {
                metric(ctx, "saveable key", "window-api-demo".to_string());
                metric(ctx, "runs", runs.get().to_string());
                Text::new(
                    "The run counter is stored through .key(..); the store is configured in main \
                     on desktop. Removing .key keeps the state in memory only.",
                )
                .font_size(14.0)
                .build(ctx);
            });
        });
}

#[composable]
fn second_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    let id = ctx.window_id();
    let state = id.state();
    let caps = id.capabilities();
    let windows = ctx.windows();
    let size = state.size();
    let title = state.title();
    let list = windows
        .iter()
        .map(|window| window.raw().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Column::new()
        .spacing(16.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding_sides(scaffold.start, scaffold.top, scaffold.end, scaffold.bottom)
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            card(ctx, "Second window", |ctx| {
                metric(ctx, "WindowId", id.raw().to_string());
                metric(ctx, "size", format!("{:.1} x {:.1}", size.0, size.1));
                metric(ctx, "title", title);
                metric(ctx, "ctx.windows()", format!("{}: {list}", windows.len()));
                metric(
                    ctx,
                    "multi_window capability",
                    caps.multi_window.to_string(),
                );
                Button::filled()
                    .on_click({
                        clone!(id);
                        move || id.state().set_visible(false)
                    })
                    .build(ctx, |ctx| Text::new("Hide this window").build(ctx));
                Text::new(
                    "The first window's Multi-window card can show it again. Hiding keeps the declaration and the WindowId.",
                )
                .font_size(14.0)
                .build(ctx);
            });
        });
}

#[composable]
fn card(ctx: &mut ComposeCtx, title: &'static str, content: impl FnOnce(&mut ComposeCtx)) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(10.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new(title).font_size(18.0).build(ctx);
                content(ctx);
            });
    });
}

#[composable]
fn metric(ctx: &mut ComposeCtx, label: &'static str, value: String) {
    Row::new().spacing(8.0).build(ctx, |ctx| {
        Text::new(label).font_size(13.0).build(ctx);
        Text::new(value).font_size(13.0).build(ctx);
    });
}
