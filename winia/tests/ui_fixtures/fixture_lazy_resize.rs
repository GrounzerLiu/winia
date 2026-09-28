//! UI-test fixture: a `LazyColumn` that fills the window, for the resize convergence proof.
//!
//! Drives `a_resize_frame_covers_its_new_viewport_within_one_frame` in `ui_test.rs`.
//!
//! The unit tests in `winia/src/ui/lazy_column.rs` prove the window coverage inside a bare `Composer`.
//! What a real window adds is the frame handler's own convergence
//! (`PerWindow::recompose_layout_render`), which no unit test can reach — and the observable for it is
//! the `fp` debug command (compose+layout rounds per rendered frame), because a tree query is answered
//! after the frames in question have already passed.
//!
//! Rows are FIXED height (48px): the only thing that changes across a resize is the viewport, so a
//! window that comes up short is short because of the resize and nothing else.

use std::sync::Arc;
use winia::prelude::*;

#[composable]
fn lazy_resize_fixture(ctx: &mut ComposeCtx) {
    let items: Arc<Vec<u64>> = Arc::new((0..200).collect());
    LazyColumn::new()
        .modifier(Modifier::new().fill_max_size().test_tag("lr-list"))
        .items_from(
            items,
            |id| *id,
            |ctx, _index, id| {
                Text::new(format!("Item {id}"))
                    .font_size(14.0)
                    .modifier(Modifier::new()
                        .fill_max_width()
                        .height(48.0)
                        .test_tag(format!("lr-row-{id}")))
                    .build(ctx);
            },
        )
        .build(ctx);
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(400.0, 300.0)
                .title("Lazy Resize Fixture")
                .build(ctx, |ctx| lazy_resize_fixture(ctx));
        });
    });
}
