//! UI-test fixture: a tap that must survive its own popup moving under the finger.
//!
//! Drives `ui_test.rs`'s `a_tap_survives_its_own_popup_moving`. The popup's tap zone moves the popup
//! 300 px to the right when it is PRESSED, so the overlay travels between the press and the release
//! while the pointer stays put. The gesture measures displacement against the overlay's arena origin:
//! with the LIVE origin the popup's own motion lands in that displacement, crosses the 8 px tap slop
//! and cancels the tap; with the origin frozen at press time the pointer is correctly seen as
//! stationary and the tap fires. The expanded `SearchBar` is the real case — its panel slides for
//! `SEARCH_BAR_EXPAND_MS` and is pressable while it moves.
//!
//! The motion is triggered by the press (not by a timer or an animation) on purpose: a test that has
//! to hold the finger down long enough for an animation would either exceed the long-press threshold
//! (500 ms, turning the gesture into a hold) or race the popup with its own tree queries.

use letclone::clone;
use winia::prelude::*;
use winia::ui::{Popup, PopupPosition};

/// How far the popup jumps, in logical px — far beyond the 8 px tap slop.
const JUMP: f32 = 300.0;

#[composable]
fn popup_slide_tap_fixture(ctx: &mut ComposeCtx) {
    let jumped = ctx.remember(|| false);
    let taps = ctx.remember(|| 0u32);
    let jumped_now = jumped.get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new(format!("taps: {}", taps.get())).build(ctx);

            Popup::new(true)
                .position(PopupPosition::TopLeft)
                .offset(40.0 + if jumped_now { JUMP } else { 0.0 }, 200.0)
                .dismiss_on_outside(false)
                .build(ctx, {
                    let t = taps.clone();
                    let j = jumped.clone();
                    move |ctx| {
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .test_tag("popup-tap-zone")
                                    .fill_max_width()
                                    .height(60.0)
                                    .background(Color::from_argb(255, 200, 210, 220), Shape::rounded(4.0))
                                    .on_press({
                                        clone!(j);
                                        move |_| j.set(true)
                                    })
                                    .on_tap({
                                        clone!(t);
                                        move |_| t.update(|v| *v += 1)
                                    }),
                            )
                            .build(ctx, |ctx| {
                                Text::new("Popup tap zone").build(ctx);
                            });
                    }
                });
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario
/// from `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 420.0)
                .title("Popup Slide Tap Fixture")
                .build(ctx, |ctx| popup_slide_tap_fixture(ctx));
        });
    });
}
