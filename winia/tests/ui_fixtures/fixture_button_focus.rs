//! UI-test fixture: a Filled and a Text button, both focusable, for the focus-ring test.
//!
//! `a_text_button_shows_the_same_focus_ring_as_a_filled_one` drives it with Tab and reads pixels just outside
//! each pill: M3 draws the ring for every button (the `Surface` carries the same focus indication), and winia's
//! Text style swaps its background for a transparent `clip(shape)`, which is where the ring's shape comes from.

use winia::prelude::*;

#[composable]
fn button_focus_fixture(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new().padding(32.0))
        .spacing(24.0)
        .build(ctx, |ctx| {
            Button::filled()
                .on_click(|| {})
                .modifier(Modifier::new().test_tag("btn-filled"))
                .build(ctx, |ctx| Text::new("Filled").build(ctx));
            Button::text()
                .on_click(|| {})
                .modifier(Modifier::new().test_tag("btn-text"))
                .build(ctx, |ctx| Text::new("Text").build(ctx));
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 300.0)
                .title("button-focus")
                .build(ctx, |ctx| button_focus_fixture(ctx));
        });
    });
}
