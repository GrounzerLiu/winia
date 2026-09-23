//! UI-test fixture: the drag routing inside a `ModalBottomSheet` — list scroll vs panel drag.
//!
//! Drives `bottom_sheet_drag_routing_keeps_the_list_in_charge_of_its_own_scroll` in `ui_test.rs`.
//!
//! The rule is Compose M3's (`ConsumeSwipeWithinBottomSheetBoundsNestedScrollConnection`): an UPWARD delta
//! is taken by the sheet first (expands-first) and only the leftover reaches the list; a DOWNWARD delta
//! belongs to the list while it can scroll, and to the sheet once it cannot — so dragging down with the
//! list at its top collapses the sheet. That IS the dismissal gesture, not a bug (checked against the
//! androidx source), and it is worth a test because the arbitration is three-way — an inner drag component
//! beats a scroll, a scroll beats the panel's `on_drag` — and gets quietly wrong when the hit test changes.

use winia::prelude::*;
use std::sync::Arc;

#[composable]
fn bottom_sheet_fixture(ctx: &mut ComposeCtx) {
    let visible = ctx.remember(|| false);
    let rows: Arc<Vec<(u64, String)>> =
        Arc::new((0..40).map(|i| (i as u64, format!("row {i}"))).collect());
    let state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Button::text()
                .on_click({
                    let v = visible.clone();
                    move || v.set(true)
                })
                .modifier(Modifier::new().test_tag("bs-open"))
                .build(ctx, |ctx| Text::new("open sheet").build(ctx));
        });

    ModalBottomSheet::new(visible.get())
        .on_dismiss_request({
            let v = visible.clone();
            move || v.set(false)
        })
        .build(ctx, {
            let rows = rows.clone();
            move |ctx| {
                Column::new()
                    .modifier(Modifier::new()
                        .fill_max_width()
                        .padding(16.0)
                        .test_tag("bs-content"))
                    .build(ctx, |ctx| {
                        Text::new("sheet header").build(ctx);
                        // Inside a bounded, clipped box: a LazyColumn given unbounded height from the sheet's
                        // offset container lays out no items at all (the demo's shape, which works). The
                        // geometry (560-tall window, 520-tall list, a footer) mirrors `bottom_sheet_demo` so
                        // the sheet's PartiallyExpanded anchor is a real one — a panel shorter than half the
                        // window collapses partial and expanded into one, and the interaction under test
                        // (expands-first) disappears with it.
                        Stack::new()
                            .modifier(Modifier::new()
                                .fill_max_width()
                                // Taller than the window on purpose: the panel then reaches the window
                                // height when expanded, which is when WINIA drops its top corners (its own rule, not M3's).
                                .height(520.0)
                                .clip(Shape::RoundedRect { corner_radius: 12.0 }))
                            .build(ctx, |ctx| {
                                LazyColumn::new()
                                    .state(state.clone())
                                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                                    .spacing(4.0)
                                    .items_from(
                                        rows.clone(),
                                        |(id, _)| *id,
                                        |ctx, _i, (_, label)| {
                                            Text::new(label.clone())
                                                .modifier(Modifier::new()
                                                    .fill_max_width()
                                                    .height(40.0)
                                                    .background(
                                                        WiniaTheme::colors().surface_container_high,
                                                        Shape::Rectangle,
                                                    )
                                                    .test_tag(format!("bs-{}", label.replace(' ', "-"))))
                                                .build(ctx);
                                        },
                                    )
                                    .build(ctx);
                            });
                        Text::new("sheet footer")
                            .modifier(Modifier::new().test_tag("bs-footer"))
                            .build(ctx);
                    });
            }
        });
}

/// Scenario entry: `fixture_all` (the single fixture binary) calls this after selecting the scenario from
/// `argv[1]`. It starts the event loop and never returns.
pub fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 560.0)
                .title("Bottom Sheet Fixture")
                .build(ctx, |ctx| bottom_sheet_fixture(ctx));
        });
    });
}
