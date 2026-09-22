//! UI-test fixture: a window that follows a theme switch the application makes itself.
//!
//! Drives `theme_follows_the_windows_own_switch` in `ui_test.rs`. No node in the layout tree names a
//! color — a theme-derived color is resolved when the node is built, and the debug tree prints every
//! background as `bg(<dynamic>)` — so the only way to see a theme change from a test is to read the
//! frame. Hence this fixture: a full-window surface painted from the theme, and two buttons that pin
//! light and dark through `set_system_dark_mode` (the application's own switch, never the OS setting).

use winia::prelude::*;

#[composable]
fn theme_follow_fixture(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new()
            .fill_max_size()
            .background(WiniaTheme::colors().surface, Shape::Rectangle))
        .build(ctx, |ctx| {
            Row::new()
                .spacing(8.0)
                .modifier(Modifier::new().padding(16.0))
                .build(ctx, |ctx| {
                    for (label, tag, mode) in [
                        ("Light", "theme-light", Some(false)),
                        ("Dark", "theme-dark", Some(true)),
                    ] {
                        Button::text()
                            .on_click(move || winia::ui::set_system_dark_mode(mode))
                            .modifier(Modifier::new().test_tag(tag))
                            .build(ctx, |ctx| {
                                Text::new(label).build(ctx);
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
                .size(320.0, 220.0)
                .title("Theme Follow Fixture")
                .build(ctx, |ctx| theme_follow_fixture(ctx));
        });
    });
}
