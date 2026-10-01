//! AlertDialog demo — Material 3 alert dialogs.
//!
//! Shows: the two-action dialog (confirm + dismiss), a one-action dialog, an icon above the
//! title, a long body that stops at the 560dp maximum width, and a case that exercises the
//! `DialogProperties` knobs with a custom shape and colour and both dismissal routes off.
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

/// Which dialog is open, if any. Every case shares ONE dialog; this only picks its content.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Open {
    None,
    TwoAction,
    OneAction,
    WithIcon,
    LongBody,
    /// `DialogProperties` knobs: a square container, a custom colour, and both dismissal routes off.
    Custom,
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
                ("Custom shape, colour, non-dismissible", Open::Custom),
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

    // ONE dialog, composed every frame, with `visible` carrying whether it is up.
    //
    // This is the contract, and breaking it is invisible until a button stops working. An overlay is
    // released by its owner recording `active = false`; a registrar that simply STOPS composing it is
    // treated as "skipped" and KEPT (`composer.rs::record_overlay_active`, `app.rs::sync_overlays`).
    // A `match` that only composes the dialog in the open case therefore pins it on screen: Escape
    // still closed it (that path calls `begin_overlay_close` directly), while Discard and Cancel ran
    // their handlers and set the state to `Open::None` — and left the dialog sitting there. Measured
    // with probe prints in the confirm slot: the click FIRED, and `overlays` stayed at 1.
    let current = open.get();
    let custom = current == Open::Custom;
    let accent = theme.secondary;

    // The icon and the dismiss button are OPTIONAL slots, and the difference is not cosmetic: the
    // content asks `slots.icon.is_some()` to decide whether the title is centred or start-aligned, and
    // it composes the icon's wrapper — padding bottom and all — whenever the slot is present. Attaching
    // `.icon(..)` unconditionally and early-returning inside the closure therefore left a 16dp gap and
    // a centred title in the four cases that have no icon, and the `Stack` wrapper is not something the
    // closure declining to draw can undo. So the slot is attached only where it is filled.
    let mut dialog = AlertDialog::new(current != Open::None)
        .on_dismiss_request({
            clone!(open);
            move || open.set(Open::None)
        })
        .shape(if custom {
            Shape::rounded(8.0)
        } else {
            AlertDialogDefaults::shape()
        })
        .container_color(if custom {
            theme.secondary_container
        } else {
            AlertDialogDefaults::container_color(&theme)
        })
        .title_content_color(if custom {
            theme.on_secondary_container
        } else {
            AlertDialogDefaults::title_color(&theme)
        })
        .text_content_color(if custom {
            theme.on_secondary_container
        } else {
            AlertDialogDefaults::text_color(&theme)
        })
        // Both `DialogProperties` dismissal flags, off only in the custom case. Escape is still
        // SWALLOWED either way — the page behind must not react to a key this dialog kept.
        .dismiss_on_outside(!custom)
        .dismiss_on_back_press(!custom)
        .focusable(!custom)
        .title(move |ctx| {
            let title = match current {
                Open::OneAction => "Saved",
                Open::WithIcon => "Location access",
                Open::LongBody => "Terms of service",
                Open::Custom => "Dismissible only by its button",
                _ => "Discard draft?",
            };
            Text::new(title).build(ctx);
        })
        .text(move |ctx| {
            let body = match current {
                Open::OneAction => "Your changes are in the cloud.",
                Open::WithIcon => "Allow Winia to use your location while the app is open?",
                Open::LongBody => {
                    "This paragraph is deliberately long, so the dialog stops at its 560dp maximum \
                     width instead of growing to the window: the content's own width is clamped into \
                     the 280..560dp range the Material 3 spec defines for dialogs, and the text wraps \
                     inside it."
                }
                Open::Custom => {
                    "Neither the scrim nor Escape closes this one — both DialogProperties flags are \
                     off, and the dialog does not take the keyboard either, so pressing Tab leaves \
                     focus where it was."
                }
                _ => "Your draft will be deleted. This cannot be undone.",
            };
            Text::new(body).build(ctx);
        });

    if current == Open::WithIcon {
        // A filled circle stands in for an icon font. The slot is 24dp
        // (`AlertDialogDefaults::icon_size`).
        dialog = dialog.icon(move |ctx| {
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
        });
    }

    if current != Open::OneAction {
        // Compose's one-action overload has no dismissButton at all (`AlertDialog.kt:76-108`), so the
        // "One action" case below must not carry a Cancel next to its Ok.
        dialog = dialog.dismiss_button({
            clone!(open);
            move |ctx| {
                let label = if current == Open::WithIcon {
                    "Not now"
                } else {
                    "Cancel"
                };
                let o = open.clone();
                // The action row is a TEXT BUTTON, never a filled one. The basic dialog's anatomy on the
                // specs page calls it "Button label text" (the full-screen variant says "Text button"),
                // and its colour role is Primary for the label. `AlertDialogImpl` says the same: it
                // provides `ActionLabelTextColor` to the row and notes that a TextButton "will not
                // consume this provided content color value, and will use their own defined or default
                // colors" (`AlertDialog.kt:283-288`).
                Button::new()
                    .style(ButtonStyle::Text)
                    .on_click(move || o.set(Open::None))
                    .build(ctx, |ctx| {
                        Text::new(label).build(ctx);
                    });
            }
        });
    }

    dialog
        .confirm_button({
            clone!(open);
            move |ctx| {
                let label = match current {
                    Open::OneAction => "Ok",
                    Open::WithIcon => "Allow",
                    Open::LongBody => "Accept",
                    Open::Custom => "Close it",
                    _ => "Discard",
                };
                let o = open.clone();
                // The same TEXT BUTTON rule as the dismiss action above.
                Button::new()
                    .style(ButtonStyle::Text)
                    .on_click(move || o.set(Open::None))
                    .build(ctx, |ctx| {
                        Text::new(label).build(ctx);
                    });
            }
        })
        .build(ctx);
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
