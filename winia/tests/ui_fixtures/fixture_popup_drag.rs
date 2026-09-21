//! UI-test fixture: a drag inside popup content carries the same coordinates as a drag in the main
//! tree.
//!
//! Drives `ui_test.rs`'s `a_drag_inside_a_popup_reaches_the_same_value_as_in_the_main_tree`: a
//! slider in a `Popup` is dragged to a known point of its own track, and the page prints the value
//! so the test can check it against the dragged position — before the fix it read the position
//! shifted by the popup's screen origin.
//!
//! The popup is deliberately offset horizontally (a layer-vs-scene coordinate mistake is invisible
//! at screen origin (0, 0), which is how the bug survived), and its slider is the only gesture in
//! the scenario: a `Popup` dismisses on an outside press and CONSUMES that press, so a drag over
//! page content could not run while this popup is open.

use letclone::clone;
use winia::prelude::*;
use winia::ui::{Popup, PopupPosition};

#[composable]
fn popup_drag_fixture(ctx: &mut ComposeCtx) {
    let value = ctx.remember(|| 0.0f32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new(format!("popup: {:.2}", value.get())).build(ctx);

            Popup::new(true)
                .position(PopupPosition::BottomLeft)
                .offset(160.0, 6.0)
                .build(ctx, {
                    let v = value.clone();
                    move |ctx| {
                        Column::new()
                            .modifier(Modifier::new().test_tag("popup-body").fill_max_width())
                            .spacing(6.0)
                            .build(ctx, |ctx| {
                                Slider::new(v.get())
                                    .value_range(0.0, 1.0)
                                    .on_value_change({
                                        clone!(v);
                                        move |nv| v.set(nv)
                                    })
                                    .modifier(Modifier::new().test_tag("popup-slider"))
                                    .build(ctx);
                            });
                    }
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 400.0)
                .title("Popup Drag Fixture")
                .build(ctx, |ctx| popup_drag_fixture(ctx));
        });
    });
}
