//! UI-test fixture: `dismiss_on_outside` decides whether an outside press closes an overlay.
//!
//! Drives `ui_test.rs`'s `a_modal_dialog_with_dismiss_on_outside_false_stays_open`: two `AlertDialog`s
//! — one with the flag off, one with the default — plus a page button that counts its clicks, so the
//! test can tell "still open" from "open and the press went through to the page".

use letclone::clone;
use winia::prelude::*;

#[composable]
fn dialog_dismiss_fixture(ctx: &mut ComposeCtx) {
    let open_a = ctx.remember(|| false);
    let open_b = ctx.remember(|| false);
    let clicks = ctx.remember(|| 0u32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Button::text()
                .on_click({
                    clone!(clicks);
                    move || clicks.update(|v| *v += 1)
                })
                .modifier(Modifier::new().test_tag("page-button"))
                .build(ctx, |ctx| {
                    Text::new("Page button").build(ctx);
                });
            Text::new(format!("page-clicks: {}", clicks.get())).build(ctx);

            Button::text()
                .on_click({
                    clone!(open_a);
                    move || open_a.set(true)
                })
                .modifier(Modifier::new().test_tag("open-a"))
                .build(ctx, |ctx| {
                    Text::new("Open (dismiss_on_outside=false)").build(ctx);
                });
            Button::text()
                .on_click({
                    clone!(open_b);
                    move || open_b.set(true)
                })
                .modifier(Modifier::new().test_tag("open-b"))
                .build(ctx, |ctx| {
                    Text::new("Open (default)").build(ctx);
                });
            Text::new(format!(
                "a: {} / b: {}",
                if open_a.get() { "open" } else { "closed" },
                if open_b.get() { "open" } else { "closed" }
            ))
            .build(ctx);

            AlertDialog::new(open_a.get())
                .dismiss_on_outside(false)
                .on_dismiss_request({
                    clone!(open_a);
                    move || open_a.set(false)
                })
                .title(|ctx| {
                    Text::new("Stays open").build(ctx);
                })
                .text(|ctx| {
                    Text::new("An outside press must not close this one.").build(ctx);
                })
                .build(ctx);

            AlertDialog::new(open_b.get())
                .on_dismiss_request({
                    clone!(open_b);
                    move || open_b.set(false)
                })
                .title(|ctx| {
                    Text::new("Closes on outside press").build(ctx);
                })
                .text(|ctx| {
                    Text::new("The default flag closes this one.").build(ctx);
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
                .size(460.0, 520.0)
                .title("Dialog Dismiss Fixture")
                .build(ctx, |ctx| dialog_dismiss_fixture(ctx));
        });
    });
}
