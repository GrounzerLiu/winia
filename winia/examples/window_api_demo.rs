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
    let show_second = ctx.remember(|| false);
    let body_show = show_second.clone();
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
            let show = body_show.clone();
            settings::shell("Window API", ctx, move |ctx, padding| {
                body(ctx, padding, show);
            });
        });

    // The second window is a root declaration behind a State flag. Android reports
    // multi_window = false, so it keeps one window there.
    #[cfg(not(target_os = "android"))]
    if show_second.get() {
        second_window(ctx, show_second.clone());
    }
}

#[composable]
fn second_window(ctx: &mut ComposeCtx, show_second: State<bool>) {
    let close_show = show_second.clone();
    let body_show = show_second.clone();
    Window::new()
        .title("Window API demo — second window")
        .size(620.0, 480.0)
        .on_close_requested(move |_id, _reason| close_show.set(false))
        .build(ctx, move |ctx| {
            let show = body_show.clone();
            settings::shell("Second window", ctx, move |ctx, padding| {
                second_body(ctx, padding, show);
            });
        });
}

#[composable]
fn body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding, show_second: State<bool>) {
    let id = ctx.window_id();
    let state = id.state();
    let caps = id.capabilities();
    let refresh = ctx.remember(|| 0u32);
    let multi_open = show_second.get();
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
    let second_status = if !caps.multi_window {
        "not available on this platform".to_string()
    } else if multi_open {
        "declared".to_string()
    } else {
        "not declared".to_string()
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
                        .enabled(caps.multi_window && !multi_open)
                        .on_click({
                            clone!(show_second, refresh);
                            move || {
                                show_second.set(true);
                                refresh.set(refresh.get() + 1);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Open second window").build(ctx));
                    Button::outlined()
                        .enabled(multi_open)
                        .on_click({
                            clone!(show_second, refresh);
                            move || {
                                show_second.set(false);
                                refresh.set(refresh.get() + 1);
                            }
                        })
                        .build(ctx, |ctx| Text::new("Close second window").build(ctx));
                });
                Text::new(if !caps.multi_window {
                    "This platform reports multi_window = false: several declarations share one Activity surface, so the demo keeps a single window."
                } else if multi_open {
                    "The second window is declared. Close removes the declaration; the root composer then closes the platform window."
                } else {
                    "Open adds a root Window declaration; ctx.windows() then reports both live ids."
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
fn second_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding, show_second: State<bool>) {
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
                        clone!(show_second);
                        move || show_second.set(false)
                    })
                    .build(ctx, |ctx| Text::new("Close this window").build(ctx));
                Text::new(
                    "Closing clears the declaration flag. The root composer removes this declaration and closes the platform window; the first window can open a fresh one.",
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
