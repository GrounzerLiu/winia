//! UI-test fixture: a `LazyColumn` fed by a `StateList`.
//!
//! Drives `a_state_list_drives_a_lazy_column_through_real_mutations` in `ui_test.rs`.
//!
//! The unit tests in `winia/src/core/state_list.rs` prove the collection's own contract (identity
//! comparison, cheap snapshots, notification). What a real window adds is the integration: the list
//! hands its snapshot to `LazyColumn::items_from` without copying elements, and a mutation reaches the
//! rendered rows — including the row COUNT, which is what a caller cannot fake with a fixed-size loop.

use winia::prelude::*;

#[composable]
fn state_list_fixture(ctx: &mut ComposeCtx) {
    // Remembered like any other handle (LazyListState, SheetState): the slot holds it stably across
    // frames and get() hands the Clone-able handle out. The collection owns its own observable, so
    // this is NOT a State<StateList<..>> — mutating the handle is what notifies.
    let rows: StateList<i32> = ctx.remember(|| StateList::from_vec(vec![1, 2, 3])).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(8.0))
        .build(ctx, |ctx| {
            Text::new(format!("count: {}", rows.len()))
                .font_size(18.0)
                .modifier(Modifier::new().test_tag("sl-count"))
                .build(ctx);

            Row::new()
                .modifier(Modifier::new().height(32.0))
                .build(ctx, |ctx| {
                    Button::text()
                        .on_click({
                            let rows = rows.clone();
                            move || {
                                // The next id is one past the largest present, so a removed-in-the-
                                // middle list still gets fresh keys.
                                let next = rows.snapshot().iter().copied().max().unwrap_or(0) + 1;
                                rows.push(next);
                            }
                        })
                        .modifier(Modifier::new().test_tag("sl-push"))
                        .build(ctx, |ctx| Text::new("push").build(ctx));

                    Button::text()
                        .on_click({
                            let rows = rows.clone();
                            move || {
                                rows.pop();
                            }
                        })
                        .modifier(Modifier::new().test_tag("sl-pop"))
                        .build(ctx, |ctx| Text::new("pop").build(ctx));

                    Button::text()
                        .on_click({
                            let rows = rows.clone();
                            move || {
                                rows.remove(0);
                            }
                        })
                        .modifier(Modifier::new().test_tag("sl-drop-first"))
                        .build(ctx, |ctx| Text::new("drop first").build(ctx));
                });

            LazyColumn::new()
                .modifier(Modifier::new().fill_max_size().test_tag("sl-list"))
                .spacing(2.0)
                .items_from(
                    rows.snapshot(),
                    |id| *id as u64,
                    |ctx, _index, id| {
                        Text::new(format!("row {id}"))
                            .font_size(14.0)
                            .modifier(Modifier::new()
                                .fill_max_width()
                                .height(28.0)
                                .test_tag(format!("sl-row-{id}")))
                            .build(ctx);
                    },
                )
                .build(ctx);
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
                .size(420.0, 520.0)
                .title("StateList Fixture")
                .build(ctx, |ctx| state_list_fixture(ctx));
        });
    });
}
