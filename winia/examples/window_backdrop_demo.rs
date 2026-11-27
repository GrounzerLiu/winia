//! Backdrop demo — the Windows system backdrop request on its own.
//!
//! The window is created transparent and undecorated so the material has a chance to reach the
//! client area. The buttons write `DWMWA_SYSTEMBACKDROP_TYPE` and read it back; the status line
//! reports exactly what the platform did. Nothing else is implied: the request never changes
//! transparency or decorations, and turning decorations back on confines the same material to the
//! title bar.
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
        .decorations(false)
        .backdrop(WindowBackdrop::Mica)
        .build(ctx, |ctx| {
            let id = ctx.window_id();
            let state = id.state();
            let caps = id.capabilities();
            let decorations = state.decorations();
            let requested = state.backdrop();
            let status = state.backdrop_status();
            Card::new().build(ctx, |ctx| {
                Column::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().padding(20.0))
                    .build(ctx, |ctx| {
                        Text::new("System backdrop").font_size(22.0).build(ctx);
                        Text::new(
                            "Each button writes DWMWA_SYSTEMBACKDROP_TYPE and reads the value \
                             back. The request is one axis only: it does not touch transparency, \
                             decorations, the frame, or the surface, so the material shows only \
                             where the client area does not paint an opaque pixel.",
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
                            Text::new("Decorations").font_size(14.0).build(ctx);
                            Switch::new(decorations)
                                .on_checked_change({
                                    clone!(state);
                                    move |on| state.set_decorations(on)
                                })
                                .build(ctx, |_| {});
                        });
                        Text::new(
                            "Transparency is a creation-time option, so this window was built \
                             with transparent(true). With decorations off the whole-window \
                             materials can reach the client area; switch decorations on and the \
                             same request is confined to the title bar.",
                        )
                        .font_size(14.0)
                        .build(ctx);
                    });
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
