//! UI test fixture: `SwipeToDismissBox` rows inside a scrolling list.
//!
//! Drives the swipe cases in `ui_test.rs`. The list is the point: a press inside a row is
//! claimed by BOTH the row's drag and the list's scroll, and the finger's dominant axis decides which
//! one keeps the gesture (`app::gesture_move`). So "a vertical drag scrolls the list" and "a
//! horizontal drag dismisses the row" are the two halves of one arbitration, and neither can be
//! proven without the other.
//!
//! Rows are KEYED by their item id (`LazyColumn::items_from`) and a dismissal removes the item — the
//! way a real list uses the component, so the row's own state follows its ITEM rather than a list
//! position. The status line carries everything the tests assert on: how many rows are left, how far
//! the list is scrolled, and what the last `on_dismiss` reported.

use std::sync::Arc;
use winia::prelude::*;

/// Rows in the list. Ten rows of 48 px in a 240 px viewport leave 240 px of scroll range — enough
/// that a vertical drag is never mistaken for having hit the end.
const ROWS: u64 = 10;

/// The row that refuses the leftward direction (`enable_dismiss_from_end_to_start(false)`). It is the
/// second row, so it is on screen without scrolling first — and it is also the row that moves up into
/// the first slot when the first row is dismissed (`the_row_that_moves_into_a_dismissed_slot_is_live`).
const DISABLED_ROW: u64 = 1;

/// Height of a row, in px.
const ROW_HEIGHT: f32 = 48.0;

#[derive(Clone, PartialEq)]
struct Row {
    id: u64,
    label: String,
}

fn make_rows() -> Vec<Row> {
    (0..ROWS)
        .map(|id| Row { id, label: format!("row {id}") })
        .collect()
}

/// The scenario UI: a keyed lazy list of swipe rows that remove themselves.
#[composable]
fn swipe_dismiss_ui(ctx: &mut ComposeCtx) {
    // The handle-object idiom: `remember` keeps ONE `LazyListState` alive across frames, `.get()`
    // takes it out. Plain values need no such treatment — `remember` already returns their `State`.
    let list = ctx.remember(|| LazyListState::new()).get();
    let rows: State<Arc<Vec<Row>>> = ctx.remember(|| Arc::new(make_rows()));
    let last: State<String> = ctx.remember(|| "none".to_string());

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(12.0))
        .build(ctx, |ctx| {
            // The status line is composed in a scope of its own: it reads the item list and the
            // scroll offset, and without the scope every scroll frame would re-compose the list.
            let status_rows = rows.clone();
            let status_last = last.clone();
            let status_list = list.clone();
            Stack::new().build(ctx, move |ctx| {
                Text::new(format!(
                    "count: {}  voffset: {:.0}  last: {}",
                    status_rows.get().len(),
                    status_list.offset.get(),
                    status_last.get()
                ))
                .font_size(14.0)
                .modifier(Modifier::new().test_tag("sd-status"))
                .build(ctx);
            });

            let list_rows = rows.clone();
            LazyColumn::new()
                .state(list.clone())
                .modifier(Modifier::new().size(360.0, 240.0).clip(Shape::Rectangle))
                .items_from(
                    list_rows.get(),
                    |row| row.id,
                    move |ctx, _index, row| {
                        let id = row.id;
                        let label = row.label.clone();
                        // The state is owned HERE, in the item's own composition scope: that is how a
                        // caller drives or observes a row (`SwipeToDismissBox::state`). The box's
                        // built-in state works in a keyed list too; owning it makes the row's
                        // identity explicit and matches what the demo does.
                        let box_state = ctx.remember(|| SwipeToDismissBoxState::default()).get();
                        let remove_rows = rows.clone();
                        let dismiss_last = last.clone();
                        let mut box_ = SwipeToDismissBox::new()
                            .background({
                                let panel = WiniaTheme::colors().error_container;
                                move |ctx| {
                                    Stack::new()
                                        .modifier(Modifier::new().fill_max_size().background(
                                            panel,
                                            Shape::Rectangle,
                                        ))
                                        .build(ctx, |ctx| {
                                            Text::new("dismiss").font_size(14.0).build(ctx);
                                        });
                                }
                            })
                            .on_dismiss(move |direction| {
                                // Where a real list drops the item. The remaining rows keep their own
                                // state, because the list is keyed by item id.
                                remove_rows.update(|v| {
                                    let kept: Vec<Row> =
                                        v.iter().filter(|r| r.id != id).cloned().collect();
                                    *v = Arc::new(kept);
                                });
                                let way = match direction {
                                    SwipeToDismissBoxValue::StartToEnd => "right",
                                    SwipeToDismissBoxValue::EndToStart => "left",
                                    SwipeToDismissBoxValue::Settled => "settled",
                                };
                                dismiss_last.set(format!("{id} {way}"));
                            });
                        if id == DISABLED_ROW {
                            box_ = box_.enable_dismiss_from_end_to_start(false);
                        }
                        box_ = box_.state(box_state);
                        box_.modifier(Modifier::new().fill_max_width().height(ROW_HEIGHT))
                            .build(ctx, move |ctx| {
                                Text::new(label.clone())
                                    .font_size(16.0)
                                    .modifier(
                                        Modifier::new()
                                            .fill_max_size()
                                            .padding(12.0)
                                            .background(WiniaTheme::colors().surface, Shape::Rectangle)
                                            .test_tag(format!("sd-row-{id}")),
                                    )
                                    .build(ctx);
                            });
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
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 520.0)
                .title("ui fixture: swipe dismiss")
                .build(ctx, |ctx| swipe_dismiss_ui(ctx));
        });
    });
}
