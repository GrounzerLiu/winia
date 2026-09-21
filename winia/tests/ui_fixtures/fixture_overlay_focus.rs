//! UI-test fixture: pointer events inside popup content — a tap on a button in a popup must not
//! steal the keyboard from the field beside it, and a popup node's own `on_press` must fire.
//!
//! Drives `ui_test.rs`'s `clicking_an_overlay_button_does_not_steal_focus` and
//! `an_overlay_press_zone_receives_the_press_gesture`: a page button opens an `AlertDialog`
//! holding a text field, a press-only zone and an action button. The field's text, the press
//! count and the dialog's open state are printed on the page for the tests to poll.

use winia::prelude::*;

#[composable]
fn overlay_focus_fixture(ctx: &mut ComposeCtx) {
    let open = ctx.remember(|| false);
    let field = ctx.remember(|| TextFieldValue::new(""));
    let clicks = ctx.remember(|| 0u32);
    let presses = ctx.remember(|| 0u32);
    let field_text = field.get().text;
    let is_open = open.get();
    let click_count = clicks.get();
    let press_count = presses.get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("Overlay focus fixture").font_size(20.0).build(ctx);
            Button::text()
                .on_click({
                    let o = open.clone();
                    move || o.set(true)
                })
                .modifier(Modifier::new().test_tag("open-dialog"))
                .build(ctx, |ctx| {
                    Text::new("Open dialog").build(ctx);
                });
            Text::new(format!("dialog-open: {}", if is_open { "yes" } else { "no" })).build(ctx);
            Text::new(format!("dialog-field: {field_text}")).build(ctx);
            Text::new(format!("action-clicks: {click_count}")).build(ctx);
            Text::new(format!("presses: {press_count}")).build(ctx);

            AlertDialog::new(is_open)
                .on_dismiss_request({
                    let o = open.clone();
                    move || o.set(false)
                })
                .title(|ctx| {
                    Text::new("Focus test").build(ctx);
                })
                .text({
                    let f = field.clone();
                    let p = presses.clone();
                    move |ctx| {
                        Column::new()
                            .spacing(8.0)
                            .build(ctx, |ctx| {
                                TextField::new(f.clone())
                                    .outlined()
                                    .modifier(Modifier::new().test_tag("dialog-field"))
                                    .build(ctx);
                                // Press-only zone: no `on_click`, no clickable, not focusable —
                                // `on_press` is its ONLY channel, so a moving press count proves
                                // the popup dispatched the press gesture (what the main tree does
                                // in `gesture_down`) rather than a click.
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .test_tag("dialog-press-zone")
                                            .fill_max_width()
                                            .height(40.0)
                                            .background(
                                                Color::from_argb(255, 200, 200, 210),
                                                Shape::rounded(4.0),
                                            )
                                            .on_press({
                                                let p = p.clone();
                                                move |_| p.update(|v| *v += 1)
                                            }),
                                    )
                                    .build(ctx, |ctx| {
                                        Text::new("Press zone").build(ctx);
                                    });
                            });
                    }
                })
                .confirm_button({
                    let c = clicks.clone();
                    move |ctx| {
                        // A button WITH `on_click` is a real clickable/focusable (Compose's
                        // confirmButton has a handler too), and its click count goes onto the
                        // page so the test can prove the tap landed.
                        Button::new()
                            .on_click({
                                let c = c.clone();
                                move || c.update(|v| *v += 1)
                            })
                            .modifier(Modifier::new().test_tag("dialog-action"))
                            .build(ctx, |ctx| {
                                Text::new("Action").build(ctx);
                            });
                    }
                })
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 460.0)
                .title("Overlay Focus Fixture")
                .build(ctx, |ctx| overlay_focus_fixture(ctx));
        });
    });
}
