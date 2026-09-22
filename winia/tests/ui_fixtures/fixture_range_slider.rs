//! UI-test fixture: a `RangeSlider` whose thumbs are dragged in a real window.
//!
//! Drives `range_slider_drags_the_thumb_the_press_resolved` in `ui_test.rs`. The page prints both
//! ends of the range, so the test can press near one thumb and check that THAT thumb moved and the
//! other stayed — the press-resolution rule is what a real gesture has to get right, and the unit
//! tests only exercise it against a synthetic track width.

use letclone::clone;
use winia::prelude::*;

#[composable]
fn range_slider_fixture(ctx: &mut ComposeCtx) {
    let range = ctx.remember(|| RangeValue::new(0.2, 0.8));

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new(format!("range-start: {:.2}", range.get().start)).build(ctx);
            Text::new(format!("range-end: {:.2}", range.get().end)).build(ctx);
            RangeSlider::new(range.get())
                .value_range(0.0, 1.0)
                .on_value_change({
                    clone!(range);
                    move |v: RangeValue| range.set(v)
                })
                .modifier(Modifier::new().test_tag("range-slider"))
                .build(ctx);
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(520.0, 400.0)
                .title("Range Slider Fixture")
                .build(ctx, |ctx| range_slider_fixture(ctx));
        });
    });
}
