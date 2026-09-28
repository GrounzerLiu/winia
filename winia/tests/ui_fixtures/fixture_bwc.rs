//! UI-test fixture: `BoxWithConstraints` with a parent whose width cap changes on demand.
//!
//! Drives `box_with_constraints_*` in `ui_test.rs`. The point is the subcomposition: the box's
//! content is composed DURING measurement, with the constraints that measurement just computed, so
//! the text it prints and the size the box takes must both follow the parent's cap — on the very
//! first frame, and again after a change. A frame-lagged implementation would print the previous
//! cap (and, before the facility existed, would print nothing measurable at all on frame one).
//!
//! The box carries a test tag so the test can read its own measured size out of the tree. The content
//! is plain text (width driven by the string), which keeps that number independent of the parent's
//! cap: what the cap decides is what the CONTENT is told, and that is what the test asserts on.

use letclone::clone;
use winia::prelude::*;

#[composable]
fn bwc_fixture(ctx: &mut ComposeCtx) {
    // The parent's cap. The content fills what it is given, so the box's measured width equals the
    // cap — 200 wide, then 120 after a click.
    let cap = ctx.remember(|| 200.0f32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Button::text()
                    .on_click({
                        clone!(cap);
                        move || cap.set(120.0)
                    })
                    .modifier(Modifier::new().test_tag("bwc-narrow"))
                    .build(ctx, |ctx| Text::new("narrow").build(ctx));
                Button::text()
                    .on_click({
                        clone!(cap);
                        move || cap.set(200.0)
                    })
                    .modifier(Modifier::new().test_tag("bwc-wide"))
                    .build(ctx, |ctx| Text::new("wide").build(ctx));
            });

            // The box: the parent caps its width, and it composes its content with that cap.
            BoxWithConstraints::new()
                .modifier(Modifier::new().max_width(cap.get()).test_tag("bwc-box"))
                .build(ctx, |ctx, scope| {
                    let msg = if !scope.is_measured() {
                        "BWC-not-measured".to_string()
                    } else {
                        format!("BWC max {}", scope.max_width() as i32)
                    };
                    Text::new(msg).build(ctx);
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
                .size(420.0, 300.0)
                .title("BoxWithConstraints Fixture")
                .build(ctx, |ctx| bwc_fixture(ctx));
        });
    });
}
