//! UI-test fixture: the semantics snapshot as the debug channel reports it.
//!
//! Drives `semantics_are_published_per_frame_and_follow_the_state` in `ui_test.rs`.
//!
//! The model itself is unit-tested in `winia/src/semantics.rs`; what a real window adds is the
//! CHANNEL — the snapshot is published once per rendered frame, carries the overlays, and follows a
//! click. That last part is the one a screen reader depends on: an accessibility client that reads a
//! stale `selected` is worse than one that reads nothing.
//!
//! The labels are deliberately unlike each other so a test can name one element and not match
//! another by substring.

use winia::prelude::*;

#[composable]
fn semantics_fixture(ctx: &mut ComposeCtx) {
    let visible = ctx.remember(|| false);
    // A real tri-state, so the test can watch the snapshot follow a click.
    let checked = ctx.remember(|| ToggleableState::On);
    let picked = ctx.remember(|| false);
    let snackbar = ctx.remember(|| SnackbarHostState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Plain label").build(ctx);

            Button::filled()
                .on_click(|| {})
                .modifier(Modifier::new().test_tag("sem-button"))
                .build(ctx, |ctx| Text::new("Merged button").build(ctx));

            Button::filled()
                .on_click(|| {})
                .enabled(false)
                .modifier(Modifier::new().test_tag("sem-disabled"))
                .build(ctx, |ctx| Text::new("Disabled button").build(ctx));

            TriStateCheckbox::new(checked.get())
                .on_click({
                    let checked = checked.clone();
                    move || {
                        checked.update(|state| {
                            *state = match *state {
                                ToggleableState::On => ToggleableState::Off,
                                _ => ToggleableState::On,
                            }
                        })
                    }
                })
                .modifier(Modifier::new().test_tag("sem-checkbox"))
                .build(ctx);

            Switch::new(true)
                .on_checked_change(|_| {})
                .modifier(Modifier::new().test_tag("sem-switch"))
                .build(ctx, |_| {});

            RadioButton::new(picked.get())
                .on_click({
                    let picked = picked.clone();
                    move || picked.set(true)
                })
                .modifier(Modifier::new().test_tag("sem-radio"))
                .build(ctx);

            Icon::svg_path("M0 0 L24 24")
                .content_description("Described icon")
                .build(ctx);

            // A determinate progress bar reports its VALUE (what a screen reader announces as a
            // percentage), an indeterminate one reports only that it is a progress bar.
            LinearProgressIndicator::new(0.25)
                .modifier(Modifier::new().test_tag("sem-progress"))
                .build(ctx);
            LinearProgressIndicator::indeterminate()
                .modifier(Modifier::new().test_tag("sem-progress-spin"))
                .build(ctx);

            Button::text()
                .on_click({
                    let visible = visible.clone();
                    move || visible.set(true)
                })
                .modifier(Modifier::new().test_tag("sem-open-dialog"))
                .build(ctx, |ctx| Text::new("Open dialog").build(ctx));

            // A live region with an action: the message is announced unprompted, and the action has to
            // stay its OWN element. (Measured: declaring the live region on the whole bar absorbed the
            // action button, so a screen reader could hear the message but had nothing to invoke.)
            Button::text()
                .on_click({
                    let host = snackbar.clone();
                    move || {
                        host.show(
                            SnackbarData::new("Saved")
                                .action("Undo", || {})
                                .duration(SnackbarDuration::Indefinite),
                        )
                    }
                })
                .modifier(Modifier::new().test_tag("sem-show-snackbar"))
                .build(ctx, |ctx| Text::new("Show snackbar").build(ctx));

            SnackbarHost::new(snackbar.clone()).build(ctx);
        });

    AlertDialog::new(visible.get())
        .title(|ctx| Text::new("Dialog title").build(ctx))
        .text(|ctx| Text::new("Dialog body").build(ctx))
        .confirm_button(|ctx| Text::new("Confirm").build(ctx))
        .on_dismiss_request({
            let visible = visible.clone();
            move || visible.set(false)
        })
        .build(ctx);
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario
/// from `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(480.0, 620.0)
                .title("Semantics Fixture")
                .build(ctx, |ctx| semantics_fixture(ctx));
        });
    });
}
