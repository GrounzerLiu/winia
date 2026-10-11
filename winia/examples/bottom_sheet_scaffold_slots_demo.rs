//! BottomSheetScaffold's own slots — `topBar` and `snackbar` — outside the shared settings shell.
//!
//! Every other sheet demo runs inside `common/settings.rs`'s shell, which supplies the page's top bar itself, so
//! the scaffold's `top_bar` slot had no demo at all. This one is standalone for exactly that reason:
//!
//! - `top_bar` is measured FIRST and the body gets what is left below it, which is Compose's
//!   `bodyConstraints = looseConstraints.copy(maxHeight = layoutHeight - topBarHeight)`
//!   (`BottomSheetScaffold.kt:427-431`).
//! - `snackbar`'s placement is the sheet's, not the caller's: on the panel's top edge while the sheet is
//!   partially expanded, on the window's bottom edge otherwise — and, while the sheet animates, wherever the
//!   panel actually is, so the card rides that edge instead of jumping ahead of it
//!   (`BottomSheetScaffold.kt:441-446`).
//!
//! Run: cargo run -p winia --example bottom_sheet_scaffold_slots_demo --features debug-server

use letclone::clone;
use std::sync::Arc;
use winia::prelude::*;

#[composable]
fn slots_demo(ctx: &mut ComposeCtx) {
    let state = ctx
        .remember(|| {
            let s = SheetState::new(SheetValue::PartiallyExpanded);
            s.set_skip_hidden_state(true);
            s
        })
        .get();
    let list = ctx.remember(|| LazyListState::new()).get();
    let snack_visible = ctx.remember(|| false);
    let snack_for_bar = snack_visible.clone();
    let rows: Arc<Vec<(u64, String)>> = Arc::new((0..40).map(|i| (i as u64, format!("row {i}"))).collect());

    BottomSheetScaffold::new()
        .sheet_state(state.clone())
        .sheet_peek_height(120.dp())
        .sheet_drag_handle(true)
        // The scaffold's own bar: the body below it starts at the bar's height, so the two never overlap.
        .top_bar(move |ctx| {
            // A per-call clone: the bar's closure is `Fn`, so the captured handle cannot be moved into the
            // button.
            let snack = snack_for_bar.clone();
            Row::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(64.0)
                        .background(WiniaTheme::colors().surface_container, Shape::Rectangle)
                        .padding_horizontal(16.0),
                )
                .alignment(Alignment::Center)
                .spacing(12.0)
                .build(ctx, |ctx| {
                    Text::new("scaffold topBar").font_size(16.0).build(ctx);
                    Button::text()
                        .on_click(move || snack.set(true))
                        .build(ctx, |ctx| Text::new("snackbar").build(ctx));
                });
        })
        .snackbar({
            clone!(snack_visible);
            move |ctx| {
                if !snack_visible.get() {
                    return;
                }
                let colors = WiniaTheme::colors();
                let hide = snack_visible.clone();
                // The margins sit on the outer node and the card on an inner one: a single chain of
                // `padding(16).size(…)` reports the size WITHOUT the padding here (`Modifier::size` wins), and
                // the scaffold places this slot by its measured height — see `docs/modifier-node.md`, item 5 of
                // its known-limitations section.
                Stack::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .build(ctx, move |ctx| {
                        Row::new()
                            .modifier(
                                Modifier::new()
                                    .fill_max_width()
                                    .background(colors.surface_container_highest, Shape::rounded(8.0))
                                    .padding(12.0),
                            )
                            .alignment(Alignment::Center)
                            .spacing(12.0)
                            .build(ctx, |ctx| {
                                Text::new("the card rides the panel's top edge")
                                    .font_size(13.0)
                                    .color(colors.on_surface)
                                    .build(ctx);
                                Button::text()
                                    .on_click({
                                        clone!(hide);
                                        move || hide.set(false)
                                    })
                                    .build(ctx, |ctx| Text::new("dismiss").build(ctx));
                            });
                    });
            }
        })
        .build(
            ctx,
            {
                let rows = rows.clone();
                move |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Text::new("docked sheet").font_size(16.0).build(ctx);
                            Stack::new()
                                .modifier(Modifier::new().fill_max_width().height(360.0))
                                .build(ctx, |ctx| {
                                    LazyColumn::new()
                                        .state(list.clone())
                                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                                        .spacing(4.0)
                                        .items_from(rows.clone(), |(id, _)| *id, |ctx, _i, (_, label)| {
                                            Text::new(label.clone())
                                                .modifier(Modifier::new().fill_max_width().height(40.0))
                                                .build(ctx);
                                        })
                                        .build(ctx);
                                });
                        });
                }
            },
            {
                clone!(state);
                move |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_size().padding(16.0))
                        .spacing(12.0)
                        .build(ctx, |ctx| {
                            Text::new("The body starts below the bar and reserves one peek at the bottom.")
                                .font_size(12.0)
                                .color(WiniaTheme::colors().on_surface_variant)
                                .build(ctx);
                            Row::new().spacing(8.0).build(ctx, |ctx| {
                                Button::text()
                                    .on_click({
                                        clone!(state);
                                        move || state.expand()
                                    })
                                    .build(ctx, |ctx| Text::new("expand").build(ctx));
                                Button::text()
                                    .on_click({
                                        clone!(state);
                                        move || state.partial_expand()
                                    })
                                    .build(ctx, |ctx| Text::new("collapse").build(ctx));
                            });
                        });
                }
            },
        );
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 720.0)
                .title("BottomSheetScaffold slots")
                .build(ctx, |ctx| slots_demo(ctx));
        });
    });
}
