//! UI-test fixture: the WHEEL driving a docked `BottomSheetScaffold`.
//!
//! Drives `a_wheel_expands_the_docked_sheet_and_then_scrolls_its_list` in `ui_test.rs`.
//!
//! A wheel is user input in Compose — `MouseWheelScrollingLogic.dispatchMouseWheelScroll` dispatches with
//! `NestedScrollSource.UserInput`, and M3's connection accepts user input (`SheetDefaults.kt:454-470`) — so it
//! behaves exactly like a finger: the sheet expands first, and only what the sheet cannot use reaches the list.
//! This fixture exists because the sheet's UI coverage was drag-only, which is precisely how a fix that
//! rejected the wheel (the source filter in `07ff5a3`, corrected the same day) slipped past it.

use winia::prelude::*;
use std::sync::Arc;

#[composable]
fn bottom_sheet_scaffold_fixture(ctx: &mut ComposeCtx) {
    let state = ctx
        .remember(|| {
            let s = SheetState::new(SheetValue::PartiallyExpanded);
            s.set_skip_hidden_state(true);
            s
        })
        .get();
    let list = ctx.remember(|| LazyListState::new()).get();
    let rows: Arc<Vec<(u64, String)>> = Arc::new((0..120).map(|i| (i as u64, format!("scaffold row {i}"))).collect());

    BottomSheetScaffold::new()
        .sheet_state(state.clone())
        .sheet_peek_height(96.dp())
        .sheet_drag_handle(true)
        .build(
            ctx,
            {
                let rows = rows.clone();
                move |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .build(ctx, |ctx| {
                            Text::new("scaffold sheet").build(ctx);
                            Stack::new()
                                .modifier(Modifier::new().fill_max_width().height(520.0).test_tag("bss-viewport"))
                                .build(ctx, |ctx| {
                                    LazyColumn::new()
                                        .state(list.clone())
                                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                                        .items_from(rows.clone(), |(id, _)| *id, |ctx, i, (_, label)| {
                                            Text::new(label.clone())
                                                .modifier(Modifier::new().fill_max_width().height(40.0).test_tag(format!("bss-row-{i}")))
                                                .build(ctx);
                                        })
                                        .build(ctx);
                                });
                        });
                }
            },
            |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_size().padding(16.0))
                    .build(ctx, |ctx| {
                        Text::new("scaffold page").build(ctx);
                    });
            },
        );
}

pub fn main() {
    winia::run_app!(|ctx| {
        Window::new().size(480.0, 720.0).title("bss").build(ctx, |ctx| {
            bottom_sheet_scaffold_fixture(ctx);
        });
    });
}
