//! UI-test fixture: a split button — a leading action button and a trailing menu button — plus the
//! two numbers the suite watches (how often the primary action ran, and whether the menu is open).
//!
//! Drives `split_button_*` in `ui_test.rs`. The unit tests pin the token numbers and the measure
//! policy; what can only be measured here is the pair as the renderer actually lays it out (the 2 dp
//! gap, one shared height, the trailing icon nudged toward the gap) and the two buttons' clicks.

use letclone::clone;
use winia::prelude::*;

/// Material Icons "arrow_drop_down" (24 dp viewBox) — the glyph material3 puts on a split button's
/// menu trigger. Declared here because `ExposedDropdownMenuDefaults` keeps its copy private.
const ARROW_DROP_DOWN_PATH: &str = "M7 10L12 15L17 10z";

#[composable]
fn split_fixture(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0usize);
    let open = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new(format!("clicks: {}", clicks.get())).build(ctx);
            Text::new(format!("open: {}", if open.get() { "yes" } else { "no" })).build(ctx);

            SplitButtonLayout::new().build(
                ctx,
                |ctx| {
                    SplitButtonDefaults::leading_button({
                        clone!(clicks);
                        move || clicks.update(|value| *value += 1)
                    })
                    .modifier(Modifier::new().test_tag("sb-leading"))
                    .build(ctx, |ctx| {
                        Text::new("Add").build(ctx);
                    });
                },
                |ctx| {
                    // The checked form: this button owns the menu's open state, morphs to a stadium
                    // while it is open, and paints a state layer. No callback is needed — winia writes
                    // the toggled value into the state it was handed.
                    TrailingButton::checked(open.clone())
                        .modifier(Modifier::new().test_tag("sb-trailing"))
                        .build(ctx, |ctx| {
                            Icon::svg_path(ARROW_DROP_DOWN_PATH)
                                .size(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small))
                                .modifier(Modifier::new().test_tag("sb-trailing-icon"))
                                .build(ctx);
                        });
                },
            );
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
                .title("Split Button Fixture")
                .build(ctx, |ctx| split_fixture(ctx));
        });
    });
}
