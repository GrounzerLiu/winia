//! Native popup demo — the same Popup content hosted in-window or in a child native window.
//!
//! Run with: `cargo run -p winia --example native_popup_demo`.

use winia::prelude::*;

fn main() {
    let smoke = std::env::args().any(|argument| argument == "--smoke");
    let smoke_host = if std::env::args().any(|argument| argument == "--inwindow") {
        PopupHost::InWindow
    } else {
        PopupHost::Native
    };
    winia::run_app!(move |ctx| {
        WiniaTheme::auto(ctx, |ctx| app(ctx, smoke, smoke_host));
    });
}

#[composable]
fn app(ctx: &mut ComposeCtx, smoke: bool, smoke_host: PopupHost) {
    Window::new()
        .title("Native popup demo")
        .size(720.0, 520.0)
        .build(ctx, move |ctx| {
            let open = ctx.remember(|| false);
            let host = ctx.remember(|| PopupHost::Native);
            let open_native = open.clone();
            let host_native = host.clone();
            let open_in_window = open.clone();
            let host_in_window = host.clone();
            let dismiss = open.clone();
            let close = open.clone();
            let current_host = host.get();
            let visible = open.get();
            if smoke {
                let open = open.clone();
                let host = host.clone();
                winia::effect::LaunchedEffect::new(()).build(ctx, move |_| async move {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    host.set(smoke_host);
                    open.set(true);
                    tokio::time::sleep(std::time::Duration::from_millis(5000)).await;
                    open.set(false);
                });
            }

            Column::new()
                .spacing(16.0)
                .modifier(Modifier::new().fill_max_size().padding(24.0))
                .build(ctx, |ctx| {
                    Text::new("Choose a host, then click outside or press Escape to dismiss.")
                        .font_size(16.0)
                        .build(ctx);
                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        Button::filled()
                            .modifier(Modifier::new().test_tag("native-popup-open"))
                            .on_click(move || {
                                host_native.set(PopupHost::Native);
                                open_native.set(true);
                            })
                            .build(ctx, |ctx| Text::new("Native popup").build(ctx));
                        Button::outlined()
                            .on_click(move || {
                                host_in_window.set(PopupHost::InWindow);
                                open_in_window.set(true);
                            })
                            .build(ctx, |ctx| Text::new("In-window popup").build(ctx));
                    });
                    let anchor = ctx.prev_sibling_slot_key();
                    let dismiss_for_request = dismiss.clone();
                    Popup::new(visible)
                        .host(current_host)
                        .anchor_slot(anchor)
                        .position(PopupPosition::BottomStart)
                        .on_dismiss_request(move || dismiss_for_request.set(false))
                        .build(ctx, move |ctx| {
                            let close = close.clone();
                            let theme = WiniaTheme::colors();
                            Column::new()
                                .spacing(12.0)
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
                                    Text::new("Popup content")
                                        .modifier(Modifier::new().test_tag("native-popup-content"))
                                        .font_size(18.0)
                                        .build(ctx);
                                    Text::new(match current_host {
                                        PopupHost::Native => "Hosted by Winit WindowType::Popup.",
                                        PopupHost::InWindow => {
                                            "Hosted by the owner window overlay layer."
                                        }
                                    })
                                    .font_size(14.0)
                                    .build(ctx);
                                    Button::filled()
                                        .modifier(Modifier::new().test_tag("native-popup-close"))
                                        .on_click(move || close.set(false))
                                        .build(ctx, |ctx| Text::new("Close").build(ctx));
                                });
                        });
                });
        });
}
