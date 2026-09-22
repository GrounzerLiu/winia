//! UI-test fixture: a single-choice and a multi-choice segmented row that print their state.
//!
//! Drives `segmented_buttons_pick_and_toggle` in `ui_test.rs`: every item carries a test tag, so the
//! test can click one item at a time and watch the printed selection — which is what a real click has
//! to get right (the row's equal-width strip and the shared borders hide nothing the unit tests do not
//! cover, but the click routing and the toggle semantics are only real here).

use letclone::clone;
use winia::prelude::*;

#[composable]
fn segmented_fixture(ctx: &mut ComposeCtx) {
    let picked = ctx.remember(|| 0usize);
    let bold = ctx.remember(|| false);
    let italic = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new(format!("picked: {}", picked.get())).build(ctx);
            SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                for i in 0..3 {
                    let p = picked.clone();
                    SegmentedButton::new(picked.get() == i, move || p.set(i))
                        .shape(SegmentedButtonDefaults::item_shape(i, 3))
                        .modifier(Modifier::new().test_tag(format!("seg-{i}")))
                        .build(ctx, |ctx| {
                            Text::new(format!("Day {i}")).build(ctx);
                        });
                }
            });

            Text::new(format!("bold: {} italic: {}", bold.get(), italic.get())).build(ctx);
            MultiChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                SegmentedButton::toggle(bold.get(), {
                    clone!(bold);
                    move |v| bold.set(v)
                })
                .shape(SegmentedButtonDefaults::item_shape(0, 2))
                .modifier(Modifier::new().test_tag("mseg-0"))
                .build(ctx, |ctx| {
                    Text::new("Bold").build(ctx);
                });
                SegmentedButton::toggle(italic.get(), {
                    clone!(italic);
                    move |v| italic.set(v)
                })
                .shape(SegmentedButtonDefaults::item_shape(1, 2))
                .modifier(Modifier::new().test_tag("mseg-1"))
                .build(ctx, |ctx| {
                    Text::new("Italic").build(ctx);
                });
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
                .size(420.0, 300.0)
                .title("Segmented Button Fixture")
                .build(ctx, |ctx| segmented_fixture(ctx));
        });
    });
}
