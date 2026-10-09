//! UI-test fixture: an `AnimatedSize` whose content grows far past the container while the container
//! animates, so a pixel probe can ask whether what is drawn stays inside the container's bounds.
//!
//! Drives `animated_size_clips_the_content_it_outgrows` in `ui_test.rs`. Compose's
//! `Modifier.animateContentSize` starts with `this.clipToBounds()` (`AnimationModifier.kt:77`), so a
//! child that is momentarily wider than the animated box is clipped to it; this fixture is what makes
//! that observable — the container's rect comes from the layout tree, the pixel from the frame.
//!
//! Colours are chosen so the answer is unambiguous: the page is BLACK, the child is pure RED, the
//! button is whatever the theme paints.

use winia::prelude::*;
use winia::components::animated_size::AnimatedSize;

/// The grown child's width. Ten times the resting width, so the probe point sits well outside the
/// container for most of the animation.
const GROWN_WIDTH: f32 = 400.0;
/// The resting width: what the container starts at.
const RESTING_WIDTH: f32 = 60.0;
/// Long enough that the container is still near its resting width seconds after the click — a slow
/// animation is the measurement's whole margin.
const GROW_MS: u64 = 6000;

#[composable]
fn animated_size_overflow_fixture(ctx: &mut ComposeCtx) {
    let grown: State<bool> = ctx.remember(|| false);
    let g = grown.clone();
    Column::new()
        .modifier(Modifier::new().fill_max_size().background(Color::BLACK, Shape::Rectangle))
        .build(ctx, |ctx| {
            Button::new()
                .on_click(move || g.update(|v| *v = true))
                .modifier(Modifier::new().size(120.0, 40.0).test_tag("grow"))
                .build(ctx, |ctx| {
                    Text::new("grow").font_size(12.0).build(ctx);
                });
            AnimatedSize::new(winia::animation::TweenSpec::new(
                std::time::Duration::from_millis(GROW_MS),
                winia::animation::interpolator::Linear::new(),
            ))
            // Tagged so a test can read the CONTAINER's rect while it animates: the assertion is
            // about what is drawn outside it, and both numbers come from the same moment.
            .modifier(Modifier::new().test_tag("animated-box"))
            .build(ctx, |ctx| {
                let w = if grown.get() { GROWN_WIDTH } else { RESTING_WIDTH };
                Column::new()
                    .modifier(
                        Modifier::new()
                            .size(w, 40.0)
                            .background(Color::RED, Shape::Rectangle)
                            .test_tag("animated-child"),
                    )
                    .build(ctx, |_ctx| {});
            });
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario
/// from `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(500.0, 300.0)
                .title("AnimatedSize Overflow Fixture")
                .build(ctx, |ctx| animated_size_overflow_fixture(ctx));
        });
    });
}
