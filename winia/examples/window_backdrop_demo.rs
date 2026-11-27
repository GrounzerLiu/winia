//! Backdrop demo — the Windows system backdrop request on its own.
//!
//! The window is created transparent and keeps its decorations, and the content paints no
//! background of its own: only text and controls are drawn, so the material has the client area to
//! show through. The buttons write `DWMWA_SYSTEMBACKDROP_TYPE` and read it back; the status line
//! reports exactly what the platform did. Nothing else is implied: the request never changes
//! transparency or decorations, and the switch flips the decorations at runtime so both cases can
//! be compared on the same request.
//!
//! Run with: `cargo run -p winia --example window_backdrop_demo`.

use letclone::clone;
use winia::prelude::*;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            app(ctx);
        });
    });
}

#[composable]
fn app(ctx: &mut ComposeCtx) {
    Window::new()
        .title("Window backdrop demo")
        .size(760.0, 560.0)
        .transparent(true)
        // Windows composites a client per pixel only while the window is undecorated, and the
        // mode is fixed at creation. Start frameless, then add the title bar afterwards so the
        // material can still reach the client area.
        .decorations(false)
        .backdrop(WindowBackdrop::Mica)
        .build(ctx, |ctx| {
            let id = ctx.window_id();
            let state = id.state();
            let caps = id.capabilities();
            let popup_open = ctx.remember(|| false);
            // One-shot: enable the title bar after this first composition.
            let decorated = ctx.remember(|| std::rc::Rc::new(std::cell::Cell::new(false))).get();
            if !decorated.get() {
                decorated.set(true);
                winia::effect::side_effect(ctx, {
                    clone!(state);
                    move || state.set_decorations(true)
                });
            }
            let decorations = state.decorations();
            let requested = state.backdrop();
            let status = state.backdrop_status();
            // No card and no panel: anything opaque would cover the material this demo exists to
            // show. Only the text and the controls are painted.
            Column::new()
                .spacing(12.0)
                .modifier(Modifier::new().fill_max_size().padding(24.0))
                .build(ctx, |ctx| {
                    Text::new("System backdrop").font_size(22.0).build(ctx);
                    Text::new(
                        "Each button writes DWMWA_SYSTEMBACKDROP_TYPE and reads the value back. \
                         The request is one axis only: it does not touch transparency, \
                         decorations, the frame, or the surface, so the material shows only where \
                         the client area does not paint an opaque pixel.",
                    )
                    .font_size(14.0)
                    .build(ctx);
                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        for choice in [
                            WindowBackdrop::None,
                            WindowBackdrop::Auto,
                            WindowBackdrop::Mica,
                            WindowBackdrop::Acrylic,
                            WindowBackdrop::MicaAlt,
                        ] {
                            Button::outlined()
                                .on_click({
                                    clone!(state);
                                    move || state.set_backdrop(choice)
                                })
                                .build(ctx, |ctx| Text::new(backdrop_label(choice)).build(ctx));
                        }
                    });
                    Text::new(format!("requested {requested:?}  ·  status {status}"))
                        .font_size(14.0)
                        .build(ctx);
                    Text::new(format!("backdrop capability: {}", caps.backdrop))
                        .font_size(14.0)
                        .build(ctx);
                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        Button::filled()
                            .modifier(Modifier::new().test_tag("backdrop-popup-open"))
                            .on_click({
                                clone!(popup_open);
                                move || popup_open.set(true)
                            })
                            .build(ctx, |ctx| Text::new("Open native popup").build(ctx));
                    });
                    let anchor = ctx.prev_sibling_slot_key();
                    let dismiss = popup_open.clone();
                    let close = popup_open.clone();
                    Popup::new(popup_open.get())
                        .host(PopupHost::Native)
                        .anchor_slot(anchor)
                        .position(PopupPosition::BottomStart)
                        .enter_animation(Some(OverlayAnimSpec::default_enter()))
                        .exit_animation(Some(OverlayAnimSpec::default_exit()))
                        .on_dismiss_request(move || dismiss.set(false))
                        .build(ctx, move |ctx| {
                            let close = close.clone();
                            let theme = WiniaTheme::colors();
                            Column::new()
                                .spacing(8.0)
                                .modifier(
                                    Modifier::new()
                                        .width(280.0)
                                        .background(
                                            theme.surface_container_high,
                                            Shape::rounded(12.0),
                                        )
                                        .padding(16.0),
                                )
                                .build(ctx, |ctx| {
                                    Text::new("Native popup").font_size(16.0).build(ctx);
                                    Text::new(
                                        "This content lives in a child winit popup window, so it \
                                         can extend past the owner's client area.",
                                    )
                                    .font_size(13.0)
                                    .build(ctx);
                                    Button::filled()
                                        .on_click(move || close.set(false))
                                        .build(ctx, |ctx| Text::new("Close").build(ctx));
                                });
                        });
                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        Text::new("Decorations").font_size(14.0).build(ctx);
                        Switch::new(decorations)
                            .on_checked_change({
                                clone!(state);
                                move |on| state.set_decorations(on)
                            })
                            .build(ctx, |_| {});
                    });
                    Text::new(
                        "Transparency is a creation-time option, so this window was built with \
                         transparent(true). Windows composites a client area per pixel only while \
                         the window is undecorated: with decorations on, the client is drawn opaque \
                         and the material is limited to the title bar.",
                    )
                    .font_size(14.0)
                    .build(ctx);
                });
        });
}

fn backdrop_label(backdrop: WindowBackdrop) -> &'static str {
    match backdrop {
        WindowBackdrop::None => "None",
        WindowBackdrop::Auto => "Auto",
        WindowBackdrop::Mica => "Mica",
        WindowBackdrop::Acrylic => "Acrylic",
        WindowBackdrop::MicaAlt => "Mica Alt",
    }
}
