//! UI-test fixture: a popup's content follows a CALLER-side value.
//!
//! Drives `ui_test.rs`'s `popup_content_follows_a_caller_side_value`: the page button bumps a counter,
//! and a `Popup` shows that counter — computed in the CALLER's scope and captured by the content
//! closure, which is the case an overlay's own composer cannot observe. Nothing inside the popup reads
//! the counter, so only the caller's recomposition (and the overlay re-running its content) can bring
//! the new value in.
//!
//! The popup is non-modal with `dismiss_on_outside(false)` so the page button stays clickable.

use letclone::clone;
use winia::prelude::*;
use winia::ui::{Popup, PopupPosition};

#[composable]
fn popup_content_fixture(ctx: &mut ComposeCtx) {
    let n = ctx.remember(|| 0u32);
    let page_n = n.get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new(format!("page-n: {page_n}")).build(ctx);
            Button::text()
                .on_click({
                    clone!(n);
                    move || n.update(|v| *v += 1)
                })
                .modifier(Modifier::new().test_tag("bump"))
                .build(ctx, |ctx| {
                    Text::new("Bump").build(ctx);
                });

            // The value is read HERE, in the caller's scope — the popup's body never touches the
            // counter, so it can only be refreshed by the caller's recomposition reaching it.
            //
            // The text sits inside a wrapper `Column` on purpose: the caller's content is usually
            // nested in groups of its own that declare nothing, and those are exactly what a
            // root-only refresh misses (measured on `search_bar_demo`'s docked dropdown, whose
            // caller lambda is wrapped in a `Surface` + `Column`).
            let popup_n = n.get();
            Popup::new(true)
                .position(PopupPosition::TopLeft)
                .offset(40.0, 180.0)
                .dismiss_on_outside(false)
                .build(ctx, move |ctx| {
                    Column::new()
                        .modifier(Modifier::new().test_tag("popup-body"))
                        .build(ctx, |ctx| {
                            Text::new(format!("popup-n: {popup_n}")).build(ctx);
                        });
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 420.0)
                .title("Popup Content Fixture")
                .build(ctx, |ctx| popup_content_fixture(ctx));
        });
    });
}
