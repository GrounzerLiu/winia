//! AlertDialog demo — Material 3 alert dialogs.
//!
//! Shows: the two-action dialog (confirm + dismiss), a one-action dialog, an icon above the
//! title, a long body that stops at the 560dp maximum width, and a custom container colour.
//! The buttons really open and close, so the overlay's lifetime (and the scrim's dismissal)
//! can be driven by hand.
//!
//! Note the closure shape: a slot is `Fn`, not `FnOnce` — the overlay composes it on every
//! frame the dialog is up — so a callback that moves out of it needs a fresh clone per call,
//! which is what `clone!` inside the slot body is for.
//!
//! Run: cargo run -p winia --example alert_dialog_demo

use letclone::clone;
use winia::prelude::*;

// The shared example chrome: a top app bar with a settings button, and the bottom sheet that
// switches the theme mode and the layout direction. Every example includes this file.
#[path = "common/settings.rs"]
mod settings;

/// Which dialog is open, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Open {
    None,
    TwoAction,
    OneAction,
    WithIcon,
    LongBody,
}

#[composable]
fn alert_dialog_demo(ctx: &mut ComposeCtx) {
    let open = ctx.remember(|| Open::None);
    let theme = WiniaTheme::colors();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            // No heading of its own: the shared chrome's top app bar carries the title.
            for (label, which) in [
                ("Two actions (confirm + dismiss)", Open::TwoAction),
                ("One action", Open::OneAction),
                ("Icon above the title", Open::WithIcon),
                ("Long body (560dp maximum)", Open::LongBody),
            ] {
                Button::new()
                    .on_click({
                        clone!(open);
                        move || open.set(which)
                    })
                    .build(ctx, |ctx| {
                        Text::new(label).build(ctx);
                    });
            }
            Text::new("Clicking outside, or Escape, dismisses the dialog.")
                .font_size(12.0)
                .build(ctx);
        });

    match open.get() {
        Open::None => {}
        Open::TwoAction => {
            AlertDialog::new(true)
                .on_dismiss_request({
                    clone!(open);
                    move || open.set(Open::None)
                })
                .title(|ctx| {
                    Text::new("Discard draft?").build(ctx);
                })
                .text(|ctx| {
                    Text::new("Your draft will be deleted. This cannot be undone.").build(ctx);
                })
                .confirm_button({
                    clone!(open);
                    move |ctx| {
                        let o = open.clone();
                        Button::new()
                            .on_click(move || o.set(Open::None))
                            .build(ctx, |ctx| {
                                Text::new("Discard").build(ctx);
                            });
                    }
                })
                .dismiss_button({
                    clone!(open);
                    move |ctx| {
                        let o = open.clone();
                        Button::new()
                            .style(ButtonStyle::Text)
                            .on_click(move || o.set(Open::None))
                            .build(ctx, |ctx| {
                                Text::new("Cancel").build(ctx);
                            });
                    }
                })
                .build(ctx);
        }
        Open::OneAction => {
            AlertDialog::new(true)
                .on_dismiss_request({
                    clone!(open);
                    move || open.set(Open::None)
                })
                .title(|ctx| {
                    Text::new("Saved").build(ctx);
                })
                .text(|ctx| {
                    Text::new("Your changes are in the cloud.").build(ctx);
                })
                .confirm_button({
                    clone!(open);
                    move |ctx| {
                        let o = open.clone();
                        Button::new()
                            .on_click(move || o.set(Open::None))
                            .build(ctx, |ctx| {
                                Text::new("OK").build(ctx);
                            });
                    }
                })
                .build(ctx);
        }
        Open::WithIcon => {
            AlertDialog::new(true)
                .on_dismiss_request({
                    clone!(open);
                    move || open.set(Open::None)
                })
                .icon({
                    let accent = theme.secondary;
                    move |ctx| {
                        // A filled circle stands in for an icon font. The slot is 24dp
                        // (`AlertDialogDefaults::icon_size`).
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .size(
                                        AlertDialogDefaults::icon_size(),
                                        AlertDialogDefaults::icon_size(),
                                    )
                                    .background(accent, Shape::Circle),
                            )
                            .build(ctx, |_| {});
                    }
                })
                .title(|ctx| {
                    Text::new("Location access").build(ctx);
                })
                .text(|ctx| {
                    Text::new("Allow Winia to use your location while the app is open?").build(ctx);
                })
                .confirm_button({
                    clone!(open);
                    move |ctx| {
                        let o = open.clone();
                        Button::new()
                            .on_click(move || o.set(Open::None))
                            .build(ctx, |ctx| {
                                Text::new("Allow").build(ctx);
                            });
                    }
                })
                .dismiss_button({
                    clone!(open);
                    move |ctx| {
                        let o = open.clone();
                        Button::new()
                            .style(ButtonStyle::Text)
                            .on_click(move || o.set(Open::None))
                            .build(ctx, |ctx| {
                                Text::new("Not now").build(ctx);
                            });
                    }
                })
                .build(ctx);
        }
        Open::LongBody => {
            AlertDialog::new(true)
                .on_dismiss_request({
                    clone!(open);
                    move || open.set(Open::None)
                })
                .title(|ctx| {
                    Text::new("Terms of service").build(ctx);
                })
                .text(|ctx| {
                    Text::new(
                        "This paragraph is deliberately long, so the dialog stops at its 560dp \
                         maximum width instead of growing to the window: the content's own width \
                         is clamped into the 280..560dp range the Material 3 spec defines for \
                         dialogs, and the text wraps inside it.",
                    )
                    .build(ctx);
                })
                .confirm_button({
                    clone!(open);
                    move |ctx| {
                        let o = open.clone();
                        Button::new()
                            .on_click(move || o.set(Open::None))
                            .build(ctx, |ctx| {
                                Text::new("Accept").build(ctx);
                            });
                    }
                })
                .build(ctx);
        }
    }
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                // Wider than the dialog's 560dp maximum, so the long-body case demonstrates
                // the cap instead of matching the window by coincidence.
                .size(700.0, 620.0)
                .title("AlertDialog")
                .build(ctx, |ctx| {
                    settings::shell("AlertDialog", ctx, |ctx| alert_dialog_demo(ctx));
                });
        });
    });
}
