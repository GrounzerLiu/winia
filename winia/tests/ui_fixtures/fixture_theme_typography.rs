//! UI-test fixture: a window whose declaring tree sets a TYPE SCALE and an RTL direction.
//!
//! Drives `the_window_content_composes_under_the_declared_typography_and_direction` in `ui_test.rs`. A
//! window's content is composed by the WINDOW, not by the tree that declares it, so the typography and
//! direction in force where the `Window` node sits have to be published into the window's theme cell and
//! re-provided per frame. If that publish is dropped, nothing errors: the content quietly composes under
//! the defaults (14 px, LTR), which is what this fixture makes observable — a mirrored `Row` and a list item
//! whose height follows `body_large`.

use winia::prelude::*;

#[composable]
fn theme_typography_fixture(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new()
            .fill_max_size()
            .background(WiniaTheme::colors().surface, Shape::Rectangle)
            .padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            // RTL: the first child lands on the RIGHT, in window coordinates.
            Row::new()
                .spacing(8.0)
                .build(ctx, |ctx| {
                    for tag in ["first", "second"] {
                        Text::new(tag)
                            .modifier(Modifier::new()
                                .size(70.0, 24.0)
                                .test_tag(format!("ttyp-{tag}")))
                            .build(ctx);
                    }
                });
            // The headline is styled with `ListItemDefaults::headline_style()` — `Typography::body_large` —
            // so the TEXT node's height follows the declared scale. (The item's own height does not: it is a
            // constant per line count, which is why the assertion is on the text.)
            ListItem::new(|ctx| {
                Text::new("Headline")
                    .modifier(Modifier::new().test_tag("ttyp-headline"))
                    .build(ctx);
            })
            .modifier(Modifier::new().test_tag("ttyp-item"))
            .build(ctx);
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        // The type scale must nest INSIDE the theme node that sets the palette: every theme entry point
        // provides a typography of its own, so `with_typography` after it is what survives.
        WiniaTheme::with_theme_and_direction(WiniaTheme::colors(), LayoutDirection::Rtl, ctx, |ctx| {
            WiniaTheme::with_typography(large_type(), ctx, |ctx| {
                Window::new()
                    .size(320.0, 220.0)
                    .title("Theme Typography Fixture")
                    .build(ctx, |ctx| theme_typography_fixture(ctx));
            });
        });
    });
}

/// `body_large` — what a `ListItem` headline uses — at roughly double the Material default, so its height
/// cannot be confused with the default scale's.
fn large_type() -> Typography {
    let mut typography = Typography::default();
    typography.body_large = TextStyle::new().font_size(32.0).line_height(40.0);
    typography
}
