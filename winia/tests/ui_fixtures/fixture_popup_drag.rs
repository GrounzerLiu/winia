//! UI-test fixture: a drag inside popup content reports the same coordinates as a drag in the main
//! tree.
//!
//! Drives `ui_test.rs`'s `a_drag_inside_a_popup_reaches_the_same_value_as_in_the_main_tree`: two
//! identical sliders — one on the page, one in a `Popup` — are dragged to the same point *relative
//! to their own track*, and the page prints both values so the test can compare them.
//!
//! Two deliberate properties of the scenario:
//! - The popup is offset horizontally. At screen origin (0, 0) a layer-vs-scene coordinate mistake
//!   is invisible, which is how the bug this guards against survived.
//! - The popup is `dismiss_on_outside(false)`: a dismissing popup CONSUMES the press that closes it
//!   (see docs/ui-testing.md), so the page-side drag of the same scenario would never reach its
//!   slider — and a drag that re-registers the overlay mid-gesture can be cut short.

use letclone::clone;
use winia::prelude::*;
use winia::ui::{Popup, PopupPosition};

#[composable]
fn popup_drag_fixture(ctx: &mut ComposeCtx) {
    let main_v = ctx.remember(|| 0.0f32);
    let popup_v = ctx.remember(|| 0.0f32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new(format!("main: {:.2}", main_v.get())).build(ctx);
            Slider::new(main_v.get())
                .value_range(0.0, 1.0)
                .on_value_change({
                    clone!(main_v);
                    move |nv| main_v.set(nv)
                })
                .modifier(Modifier::new().test_tag("main-slider"))
                .build(ctx);
            Text::new(format!("popup: {:.2}", popup_v.get())).build(ctx);

            Popup::new(true)
                .position(PopupPosition::BottomLeft)
                .offset(160.0, 6.0)
                .dismiss_on_outside(false)
                .build(ctx, {
                    let v = popup_v.clone();
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

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
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
