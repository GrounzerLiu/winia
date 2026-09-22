//! UI-test fixture: the tap family (tap / long-press) inside popup content.
//!
//! Drives `ui_test.rs`'s `a_popup_tap_zone_fires_the_tap_family_like_the_main_tree`: one tap zone on
//! the page and one inside a `Popup`, both carrying `on_tap` + `on_long_press` and a counter per
//! callback, so the test can compare what the two arenas deliver.
//!
//! The popup is `dismiss_on_outside(false)` so the page gesture is not eaten by a dismissal, and it
//! is offset so its layer origin is not (0, 0) — the same scenario shape as `fixture_popup_drag`.

use letclone::clone;
use winia::prelude::*;
use winia::ui::{Popup, PopupPosition};

#[composable]
fn popup_tap_fixture(ctx: &mut ComposeCtx) {
    let main_taps = ctx.remember(|| 0u32);
    let main_holds = ctx.remember(|| 0u32);
    let popup_taps = ctx.remember(|| 0u32);
    let popup_holds = ctx.remember(|| 0u32);
    let popup_singles = ctx.remember(|| 0u32);
    let popup_doubles = ctx.remember(|| 0u32);
    let popup_drag_taps = ctx.remember(|| 0u32);
    let popup_drag_starts = ctx.remember(|| 0u32);
    let popup_drag_ends = ctx.remember(|| 0u32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new(format!("main-taps: {}", main_taps.get())).build(ctx);
            Text::new(format!("main-holds: {}", main_holds.get())).build(ctx);
            Column::new()
                .modifier(
                    Modifier::new()
                        .test_tag("main-tap-zone")
                        .fill_max_width()
                        .height(40.0)
                        .background(Color::from_argb(255, 200, 210, 220), Shape::rounded(4.0))
                        .on_tap({
                            clone!(main_taps);
                            move |_| main_taps.update(|v| *v += 1)
                        })
                        .on_long_press({
                            clone!(main_holds);
                            move |_| main_holds.update(|v| *v += 1)
                        }),
                )
                .build(ctx, |ctx| {
                    Text::new("Main tap zone").build(ctx);
                });

            Text::new(format!("popup-taps: {}", popup_taps.get())).build(ctx);
            Text::new(format!("popup-holds: {}", popup_holds.get())).build(ctx);
            Text::new(format!("popup-singles: {}", popup_singles.get())).build(ctx);
            Text::new(format!("popup-doubles: {}", popup_doubles.get())).build(ctx);
            Text::new(format!("popup-drag-taps: {}", popup_drag_taps.get())).build(ctx);
            Text::new(format!("popup-drag-starts: {}", popup_drag_starts.get())).build(ctx);
            Text::new(format!("popup-drag-ends: {}", popup_drag_ends.get())).build(ctx);

            Popup::new(true)
                .position(PopupPosition::BottomLeft)
                .offset(160.0, 6.0)
                .dismiss_on_outside(false)
                .build(ctx, {
                    let taps = popup_taps.clone();
                    let holds = popup_holds.clone();
                    let singles = popup_singles.clone();
                    let doubles = popup_doubles.clone();
                    let drag_taps = popup_drag_taps.clone();
                    let drag_starts = popup_drag_starts.clone();
                    let drag_ends = popup_drag_ends.clone();
                    move |ctx| {
                        Column::new()
                            .modifier(Modifier::new().test_tag("popup-body").fill_max_width())
                            .build(ctx, |ctx| {
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .test_tag("popup-tap-zone")
                                            .fill_max_width()
                                            .height(40.0)
                                            .background(
                                                Color::from_argb(255, 220, 210, 200),
                                                Shape::rounded(4.0),
                                            )
                                            .on_tap({
                                                clone!(taps);
                                                move |_| taps.update(|v| *v += 1)
                                            })
                                            .on_long_press({
                                                clone!(holds);
                                                move |_| holds.update(|v| *v += 1)
                                            }),
                                    )
                                    .build(ctx, |ctx| {
                                        Text::new("Popup tap zone").build(ctx);
                                    });
                                // A zone with `on_double_tap`: its single tap must be DEFERRED to the
                                // double-tap window and then re-fired into this popup's arena (that
                                // path carries the arena in `PendingTap::overlay_id`). A fast second
                                // tap in the window turns the pair into one double tap instead.
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .test_tag("popup-double-zone")
                                            .fill_max_width()
                                            .height(40.0)
                                            .background(
                                                Color::from_argb(255, 210, 220, 210),
                                                Shape::rounded(4.0),
                                            )
                                            .on_tap({
                                                clone!(singles);
                                                move |_| singles.update(|v| *v += 1)
                                            })
                                            .on_double_tap({
                                                clone!(doubles);
                                                move |_| doubles.update(|v| *v += 1)
                                            }),
                                    )
                                    .build(ctx, |ctx| {
                                        Text::new("Popup double-tap zone").build(ctx);
                                    });
                                // A card that is BOTH tappable and draggable: until the tracker owns
                                // drag targets too, its `on_tap` was unreachable inside a popup (the
                                // overlay drag machinery owned the node and no tracker was created).
                                Column::new()
                                    .modifier(
                                        Modifier::new()
                                            .test_tag("popup-drag-zone")
                                            .fill_max_width()
                                            .height(40.0)
                                            .background(
                                                Color::from_argb(255, 220, 220, 200),
                                                Shape::rounded(4.0),
                                            )
                                            .on_tap({
                                                clone!(drag_taps);
                                                move |_| drag_taps.update(|v| *v += 1)
                                            })
                                            .on_drag_start({
                                                clone!(drag_starts);
                                                move |_| drag_starts.update(|v| *v += 1)
                                            })
                                            .on_drag_end({
                                                clone!(drag_ends);
                                                move || drag_ends.update(|v| *v += 1)
                                            }),
                                    )
                                    .build(ctx, |ctx| {
                                        Text::new("Popup drag zone").build(ctx);
                                    });
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
                .size(520.0, 460.0)
                .title("Popup Tap Fixture")
                .build(ctx, |ctx| popup_tap_fixture(ctx));
        });
    });
}
