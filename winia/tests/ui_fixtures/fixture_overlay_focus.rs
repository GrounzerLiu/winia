//! UI-test fixture: focus inside popup content — a tap on a button in a popup must not steal
//! the keyboard from the field beside it.
//!
//! Drives `ui_test.rs`'s `clicking_an_overlay_button_does_not_steal_focus`: a page button opens
//! an `AlertDialog` holding a text field and an action button. The field's text and the dialog's
//! open state are printed on the page for the test to poll.

use winia::prelude::*;

#[composable]
fn overlay_focus_fixture(ctx: &mut ComposeCtx) {
    let open = ctx.remember(|| false);
    let field = ctx.remember(|| TextFieldValue::new(""));
    let clicks = ctx.remember(|| 0u32);
    let field_text = field.get().text;
    let is_open = open.get();
    let click_count = clicks.get();

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
                    move |ctx| {
                        TextField::new(f.clone())
                            .outlined()
                            .modifier(Modifier::new().test_tag("dialog-field"))
                            .build(ctx);
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
