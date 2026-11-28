//! Popup frosted-glass demo: `Modifier::backdrop_blur` over the window's own content.
//!
//! A popup hosted in the window overlay layer (`PopupHost::InWindow`) is painted on the same
//! surface as the window content, so a node asking for `backdrop_blur` samples what was already
//! drawn behind it. That is the cross-platform way to get a frosted popup: no platform material and
//! no second window. `PopupHost::Native` gets a real child window instead, which cannot sample the
//! owner's pixels, so this demo stays in-window on purpose.
//!
//! Run with: `cargo run -p winia --example popup_blur_demo`.

use letclone::clone;
use winia::prelude::*;

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            app(ctx);
        });
    });
}

/// Colour bands behind the popup: structure is what makes a blur visible.
const BANDS: [Color; 8] = [
    Color::from_argb(255, 0xE5, 0x39, 0x35),
    Color::from_argb(255, 0xFB, 0x8C, 0x00),
    Color::from_argb(255, 0xFD, 0xD8, 0x35),
    Color::from_argb(255, 0x43, 0xA0, 0x47),
    Color::from_argb(255, 0x1E, 0x88, 0xE5),
    Color::from_argb(255, 0x39, 0x49, 0xAB),
    Color::from_argb(255, 0x8E, 0x24, 0xAA),
    Color::from_argb(255, 0xD8, 0x1B, 0x60),
];

#[composable]
fn app(ctx: &mut ComposeCtx) {
    Window::new()
        .title("Popup blur demo")
        .size(760.0, 560.0)
        .build(ctx, |ctx| {
            let open = ctx.remember(|| false);
            let blur = ctx.remember(|| true);
            let open_button = open.clone();
            let dismiss = open.clone();
            let blur_popup = blur.clone();
            let blur_popup_switch = blur.clone();
            let visible = open.get();
            Stack::new()
                .modifier(Modifier::new().fill_max_size())
                .build(ctx, |ctx| {
                    // The backdrop: horizontal bands, so the blur is obvious.
                    Column::new()
                        .modifier(Modifier::new().fill_max_size())
                        .build(ctx, |ctx| {
                            for band in BANDS {
                                Row::new()
                                    .modifier(
                                        Modifier::new()
                                            .fill_max_width()
                                            .height(70.0)
                                            .background(band, Shape::Rectangle),
                                    )
                                    .build(ctx, |_| {});
                            }
                        });
                    Column::new()
                        .spacing(12.0)
                        .modifier(Modifier::new().padding(24.0))
                        .build(ctx, |ctx| {
                            Text::new("Popup blur").font_size(22.0).build(ctx);
                            Text::new(
                                "This popup is hosted in the window overlay layer, so a node with \
                                 backdrop_blur samples the bands painted behind it. Nothing leaves \
                                 the window and no platform material is involved.",
                            )
                            .font_size(14.0)
                            .build(ctx);
                            Row::new().spacing(16.0).build(ctx, |ctx| {
                                Button::filled()
                                    .modifier(Modifier::new().test_tag("popup-blur-open"))
                                    .on_click(move || open_button.set(true))
                                    .build(ctx, |ctx| Text::new("Open frosted popup").build(ctx));
                            });
                            let anchor = ctx.prev_sibling_slot_key();
                            Popup::new(visible)
                                .host(PopupHost::InWindow)
                                .anchor_slot(anchor)
                                .position(PopupPosition::BottomStart)
                                .enter_animation(Some(OverlayAnimSpec::default_enter()))
                                .exit_animation(Some(OverlayAnimSpec::default_exit()))
                                .on_dismiss_request(move || dismiss.set(false))
                                .build(ctx, move |ctx| {
                                    let radius = if blur_popup.get() { 18.0 } else { 0.0 };
                                    Column::new()
                                        .spacing(10.0)
                                        .modifier(
                                            Modifier::new()
                                                .width(320.0)
                                                .backdrop_blur(radius)
                                                .background(
                                                    Color::from_argb(150, 16, 16, 20),
                                                    Shape::rounded(16.0),
                                                )
                                                .padding(20.0),
                                        )
                                        .build(ctx, |ctx| {
                                            Text::new("Frosted popup")
                                                .modifier(
                                                    Modifier::new().test_tag("popup-blur-content"),
                                                )
                                                .font_size(18.0)
                                                .build(ctx);
                                            Text::new(
                                                "Turn Blur off to see the same translucent panel \
                                                 without the backdrop sample.",
                                            )
                                            .font_size(13.0)
                                            .build(ctx);
                                            Row::new().spacing(8.0).build(ctx, |ctx| {
                                                Text::new("Blur").font_size(13.0).build(ctx);
                                                Switch::new(blur_popup.get())
                                                    .on_checked_change({
                                                        clone!(blur_popup_switch);
                                                        move |on| blur_popup_switch.set(on)
                                                    })
                                                    .build(ctx, |_| {});
                                            });
                                        });
                                });
                        });
                });
        });
}
