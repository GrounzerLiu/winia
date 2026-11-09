//! Alert dialog demo — the M3 alert dialog, `BasicAlertDialog`, and the four `DialogProperties`
//! switches.
//!
//! Every dialog here is composed EVERY frame with `visible` as an argument, never wrapped in an
//! `if`: an overlay is released by its owner recording `active = false`, and an owner that simply
//! stops composing it counts as "skipped" and is kept, so the `if` form pins the dialog on screen.
//! See `docs/alert-dialog.md` ("Composing it").
//!
//! Try: open the two-action dialog and press Escape / click the scrim; check the "dismiss on
//! outside" and "dismiss on Escape" switches suppress those; turn `focusable` off and notice Tab
//! stops moving within the dialog; open the basic dialog, which brings its own content.
//!
//! Run with: `cargo run -p winia --example alert_dialog_demo`.

use letclone::clone;
use std::sync::Arc;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Which dialog the page currently has open. One value rather than a bool per dialog, so opening
/// one closes the other the way a modal stack would.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Alert,
    Basic,
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 900.0)
                .title("Alert dialog demo")
                .build(ctx, |ctx| {
                    settings::shell("Alert dialog", ctx, |ctx| demo_body(ctx));
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx) {
    // `ctx.remember(|| value)` already returns a `State`, so there is no nested handle here — see
    // docs/state-handles.md section 7.4.
    let open = ctx.remember(|| Open::None);
    let outside = ctx.remember(|| true);
    let escape = ctx.remember(|| true);
    let focusable = ctx.remember(|| true);

    let scroll = ctx.remember(ScrollState::new).get();
    Column::new()
        .spacing(16.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .vertical_scroll(scroll)
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            Text::new("Alert dialog").font_size(22.0).build(ctx);
            Text::new(
                "The M3 alert dialog puts an optional icon above a title, a body, and one or two \
                 text buttons. The action row flips its layout direction so the confirm action \
                 reads after the dismiss one in a row and above it once the row wraps.",
            )
            .font_size(14.0)
            .build(ctx);

            control_panel(
                ctx,
                open.clone(),
                outside.clone(),
                escape.clone(),
                focusable.clone(),
            );

            Text::new("DialogProperties").font_size(18.0).build(ctx);
            Text::new(
                "These three are Compose's cross-platform DialogProperties fields, flat on the \
                 builder. `dismiss_on_outside` and `dismiss_on_back_press` only suppress the \
                 gesture: the dialog stays up and the caller decides what to do.",
            )
            .font_size(14.0)
            .build(ctx);
        });

    // Both dialogs are composed unconditionally; `Open::None`/`Open::Alert`/`Open::Basic` decides
    // which one is visible. Composing them inside `if` blocks would pin whichever one appeared.
    alert_dialog(
        ctx,
        open.clone(),
        outside.clone(),
        escape.clone(),
        focusable.clone(),
    );
    basic_dialog(ctx, open);
}

/// The switches plus the two "open" buttons, laid out as a `FlowRow` so a narrow window wraps them
/// instead of pushing the tail off the card.
#[composable]
fn control_panel(
    ctx: &mut ComposeCtx,
    open: State<Open>,
    outside: State<bool>,
    escape: State<bool>,
    focusable: State<bool>,
) {
    Stack::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .background(
                    WiniaTheme::colors().surface_container_high,
                    Shape::rounded(12.0),
                )
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            Column::new().spacing(12.0).build(ctx, |ctx| {
                Text::new("Controls").font_size(16.0).build(ctx);
                FlowRow::new()
                    .main_spacing(8.0)
                    .cross_spacing(8.0)
                    .build(ctx, |ctx| {
                        Button::new()
                            .modifier(Modifier::new().test_tag("open-alert"))
                            .on_click({
                                clone!(open);
                                move || open.set(Open::Alert)
                            })
                            .build(ctx, |ctx| {
                                Text::new("Alert dialog").build(ctx);
                            });
                        Button::new()
                            .style(ButtonStyle::Tonal)
                            .modifier(Modifier::new().test_tag("open-basic"))
                            .on_click({
                                clone!(open);
                                move || open.set(Open::Basic)
                            })
                            .build(ctx, |ctx| {
                                Text::new("Basic dialog").build(ctx);
                            });
                    });
                FlowRow::new()
                    .main_spacing(8.0)
                    .cross_spacing(8.0)
                    .build(ctx, |ctx| {
                        toggle_row(
                            ctx,
                            "dismiss on outside",
                            "toggle-outside",
                            outside,
                            "outside-state",
                        );
                        toggle_row(
                            ctx,
                            "dismiss on Escape",
                            "toggle-escape",
                            escape,
                            "escape-state",
                        );
                        toggle_row(ctx, "focusable", "toggle-focus", focusable, "focus-state");
                    });
            });
        });
}

/// A checkbox plus its current value, so the demo shows the state it is driving.
#[composable]
fn toggle_row(
    ctx: &mut ComposeCtx,
    label: &'static str,
    tag: &'static str,
    value: State<bool>,
    state_tag: &'static str,
) {
    Row::new()
        .spacing(6.0)
        .modifier(Modifier::new().test_tag(tag))
        .build(ctx, |ctx| {
            let now = value.get();
            Checkbox::new(now)
                .on_checked_change({
                    clone!(value);
                    move |next| value.set(next)
                })
                .build(ctx);
            Text::new(label).font_size(14.0).build(ctx);
            Text::new(if now { "on" } else { "off" })
                .font_size(14.0)
                .modifier(Modifier::new().test_tag(state_tag))
                .build(ctx);
        });
}

/// Compose's two-action `AlertDialog`: icon, title, text, confirm + dismiss.
#[composable]
fn alert_dialog(
    ctx: &mut ComposeCtx,
    open: State<Open>,
    outside: State<bool>,
    escape: State<bool>,
    focusable: State<bool>,
) {
    // One `Arc` shared by the dismissal handler and both buttons. The slot closures are `Fn` (the
    // dialog is recomposed every frame it is up), so each one clones the handle rather than moving
    // it, and `letclone::clone!` cannot be used inside them for that reason.
    let dismiss: Arc<dyn Fn() + Send + Sync> = {
        let open = open.clone();
        Arc::new(move || open.set(Open::None))
    };
    let dismiss_for_confirm = Arc::clone(&dismiss);
    let dismiss_for_cancel = Arc::clone(&dismiss);
    let dismiss_for_request = Arc::clone(&dismiss);

    AlertDialog::new(open.get() == Open::Alert)
        .on_dismiss_request(move || dismiss_for_request())
        .dismiss_on_outside(outside.get())
        .dismiss_on_back_press(escape.get())
        .focusable(focusable.get())
        .icon(|ctx| {
            // Material Symbols `delete` at the 24dp token size.
            Icon::svg_path(
                "M6,19c0,1.1,0.9,2,2,2h8c1.1,0,2-0.9,2-2V7H6v12zM19,4h-3.5l-1-1h-5l-1,1H5v2h14V4z",
            )
            .size(AlertDialogDefaults::icon_size())
            .build(ctx);
        })
        .title(|ctx| {
            Text::new("Discard draft?").build(ctx);
        })
        .text(|ctx| {
            Text::new("Your draft will be deleted. This cannot be undone.").build(ctx);
        })
        .confirm_button(move |ctx| {
            let dismiss = Arc::clone(&dismiss_for_confirm);
            Button::new()
                .style(ButtonStyle::Text)
                .modifier(Modifier::new().test_tag("dialog-confirm"))
                .on_click(move || dismiss())
                .build(ctx, |ctx| {
                    Text::new("Discard").build(ctx);
                });
        })
        .dismiss_button(move |ctx| {
            let dismiss = Arc::clone(&dismiss_for_cancel);
            Button::new()
                .style(ButtonStyle::Text)
                .modifier(Modifier::new().test_tag("dialog-dismiss"))
                .on_click(move || dismiss())
                .build(ctx, |ctx| {
                    Text::new("Cancel").build(ctx);
                });
        })
        .build(ctx);
}

/// Compose's `BasicAlertDialog`: the caller brings the content and its styling, and the dialog
/// supplies the surface, the width range and the behaviour. `content_padding(0.0)` is how a caller
/// whose content IS the surface opts out of the 24dp inset.
#[composable]
fn basic_dialog(ctx: &mut ComposeCtx, open: State<Open>) {
    // Same shape as the alert dialog above: one shared handle, cloned per `Fn` slot.
    let close: Arc<dyn Fn() + Send + Sync> = {
        let open = open.clone();
        Arc::new(move || open.set(Open::None))
    };
    let close_for_request = Arc::clone(&close);
    let close_for_button = Arc::clone(&close);
    BasicAlertDialog::new(open.get() == Open::Basic)
        .on_dismiss_request(move || close_for_request())
        .content_padding(0.0)
        .content(move |ctx| {
            let close = Arc::clone(&close_for_button);
            let theme = WiniaTheme::colors();
            Column::new()
                .spacing(0.0)
                .modifier(
                    Modifier::new()
                        .background(theme.surface_container_high, Shape::rounded(28.0))
                        .clip(Shape::rounded(28.0)),
                )
                .build(ctx, |ctx| {
                    Column::new()
                        .spacing(12.0)
                        .modifier(Modifier::new().padding(24.0))
                        .build(ctx, |ctx| {
                            Text::new("Basic dialog").font_size(20.0).build(ctx);
                            Text::new(
                                "This one brings its own surface: the calendar strip below is \
                                 content, not a slot, and the dialog only supplies the width \
                                 range and the dismissal.",
                            )
                            .font_size(14.0)
                            .build(ctx);
                        });
                    // A full-bleed band, which is what `content_padding(0.0)` is for: it reaches
                    // the container edges instead of stopping at the 24dp inset.
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .background(theme.secondary_container, Shape::Rectangle)
                                .padding(16.0),
                        )
                        .build(ctx, |ctx| {
                            Text::new("Full-bleed content")
                                .font_size(14.0)
                                .color(theme.on_secondary_container)
                                .build(ctx);
                        });
                    Row::new()
                        .spacing(8.0)
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .build(ctx, |ctx| {
                            Button::new()
                                .style(ButtonStyle::Text)
                                .modifier(Modifier::new().test_tag("basic-close"))
                                .on_click(move || close())
                                .build(ctx, |ctx| {
                                    Text::new("Close").build(ctx);
                                });
                        });
                });
        })
        .build(ctx);
}
