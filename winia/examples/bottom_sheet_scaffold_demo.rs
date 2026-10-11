//! BottomSheetScaffold demo — a draggable docked sheet (against Compose's BottomSheetScaffold).
//!
//! What it shows:
//! - A permanently docked sheet peeking 96 dp (the component's default `SCAFFOLD_SHEET_PEEK_HEIGHT` is 56 dp;
//!   this demo raises it so there is something to read).
//! - Dragging up expands it, dragging down returns it to the peek. The WHEEL drives it too, because M3's
//!   nested-scroll connection accepts `NestedScrollSource.UserInput` and a wheel is dispatched as UserInput.
//! - Top corners of 28 dp (`SCAFFOLD_SHEET_SHAPE_RADIUS`, M3's `CornerExtraLargeTop`) with square bottom ones.
//! - The drag handle: 32x4 with 22 dp of vertical padding, painted `on_surface_variant`.
//!
//! Run: cargo run -p winia --example bottom_sheet_scaffold_demo --features debug-server

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

use std::sync::Arc;

#[derive(Clone)]
struct ScaffoldItem {
    id: u64,
    title: String,
}

#[composable]
fn scaffold_demo(ctx: &mut ComposeCtx) {
    // 外部 SheetState 可用于程序化控制（按钮展开/收起）
    // Compose's `rememberStandardBottomSheetState`: a standard (docked) sheet skips the hidden state, so a
    // downward drag stops at the peek instead of taking the panel off the screen.
    let scaffold_state = ctx
        .remember(|| {
            let s = SheetState::new(SheetValue::PartiallyExpanded);
            s.set_skip_hidden_state(true);
            s
        })
        .get();
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    // The `snackbar` slot, which Compose places from the sheet's own offset: on the panel's top edge while it
    // is partially expanded (and until it has travelled past the midpoint while animating), on the window's
    // bottom edge otherwise. Shown by a button so both placements can be watched.
    let snack_visible = ctx.remember(|| false);
    let items = ctx
        .remember(|| {
            let v: Vec<ScaffoldItem> = (0..120)
                .map(|i| ScaffoldItem { id: i as u64, title: format!("Scaffold 列表项 {}", i) })
                .collect();
            Arc::new(v)
        })
        .get();

    BottomSheetScaffold::new()
        .sheet_state(scaffold_state.clone())
        .sheet_peek_height(96.dp())
        .sheet_drag_handle(true)
        // The `snackbar` slot. Its placement is the sheet's, not the caller's: on the panel's top edge while the
        // sheet is partially expanded, on the window's bottom edge otherwise — and, while the sheet animates,
        // where the panel actually IS, so it rides that edge instead of jumping ahead of it.
        .snackbar({
            clone!(snack_visible);
            move |ctx| {
                if !snack_visible.get() {
                    return;
                }
                let colors = WiniaTheme::colors();
                let hide = snack_visible.clone();
                // The margins are on an OUTER Column and the card is an inner one, rather than both on one
                // modifier chain: `padding(16).size(…)` in a single chain reports the size WITHOUT the padding
                // (`Modifier::size` wins), and the scaffold places the slot by its measured height, so the card
                // would end up up to 60 px too low and could spill past the window's bottom edge. Nested, the
                // outer Column measures the card plus its own padding and the card sits exactly on the edge the
                // rule computes. (The chain behaviour is a deviation from Compose's `padding().size()`; see the
                // ignored test in `bottom_sheet_scaffold.rs`.)
                Stack::new()
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .build(ctx, move |ctx| {
                        Stack::new()
                            .modifier(Modifier::new().fill_max_width())
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
                                        Text::new("已保存到草稿")
                                            .font_size(13.0)
                                            .color(colors.on_surface)
                                            .build(ctx);
                                        Button::text()
                                            .on_click({
                                                clone!(hide);
                                                move || hide.set(false)
                                            })
                                            .build(ctx, |ctx| Text::new("撤销").build(ctx));
                                    });
                            });
                    });
            }
        })
        .build(
            ctx,
            {
                clone!(scaffold_state, lazy_state, items);
                move |ctx| {
                    // Fixed header + LazyColumn list (virtualized, 120 items).
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Text::new("底部片内容（上拖展开，内部可单独滚动）").font_size(16.0).build(ctx);
                            // Fixed-height viewport (sheet height is anchor-driven;
                            // give the list a fixed height to enable virtualization).
                            // height 620 makes content ≈ viewport 720 → is_full fires
                            // on expand → corner 28→0 animation visible.
                            Stack::new()
                                .modifier(Modifier::new().fill_max_width().height(620.0).clip(Shape::RoundedRect { corner_radius: 12.0 }))
                                .build(ctx, |ctx| {
                                    LazyColumn::new()
                                        .state(lazy_state.clone())
                                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                                        .items_from(items.clone(), |it: &ScaffoldItem| it.id, move |ctx, _i, it| {
                                            // The row fill and its text come from the theme, like the modal
                                            // sheet's demo: a hardcoded light grey looked right in light mode
                                            // and glaring in dark mode (the sheet's own container is
                                            // `surface_container_low`, so its rows use the next step up).
                                            let colors = WiniaTheme::colors();
                                            Text::new(it.title.clone())
                                                .font_size(13.0)
                                                .color(colors.on_surface)
                                                .modifier(
                                                    Modifier::new()
                                                        .fill_max_width()
                                                        .padding(10.0)
                                                        .background(
                                                            colors.surface_container_high,
                                                            Shape::RoundedRect { corner_radius: 10.0 },
                                                        ),
                                                )
                                                .build(ctx);
                                        })
                                        .build(ctx);
                                });
                            Row::new().spacing(8.0).build(ctx, |ctx| {
                                Button::text()
                                    .on_click({ clone!(scaffold_state); move || scaffold_state.expand() })
                                    .build(ctx, |ctx| Text::new("展开").build(ctx));
                                Button::text()
                                    .on_click({ clone!(scaffold_state); move || scaffold_state.partial_expand() })
                                    .build(ctx, |ctx| Text::new("收起").build(ctx));
                                Button::text()
                                    .on_click({ clone!(lazy_state); move || lazy_state.animate_scroll_to_item(80, 0.0) })
                                    .build(ctx, |ctx| Text::new("跳 80").build(ctx));
                            });
                        });
                }
            },
            {
                clone!(snack_visible);
                move |ctx| {
                // 主内容
                Column::new()
                    .modifier(Modifier::new().fill_max_size().padding(16.0))
                    .spacing(12.0)
                    .build(ctx, |ctx| {
                        Text::new("常驻底部片，peek 96dp，上拖显示更多内容（对标 Compose Standard BottomSheet）")
                            .font_size(12.0)
                            .color(WiniaTheme::colors().on_surface_variant)
                            .build(ctx);
                        Text::new("主内容区域（可放地图/列表）")
                            .font_size(14.0)
                            .build(ctx);
                        // The `snackbar` slot is the scaffold's, so this only flips visibility: where the card
                        // ends up is the sheet's business (see the slot above).
                        Button::text()
                            .on_click(move || snack_visible.set(true))
                            .build(ctx, |ctx| Text::new("显示提示").build(ctx));
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
                .title("BottomSheetScaffold 演示")
                .build(ctx, |ctx| {
                    settings::shell("BottomSheetScaffold 演示", ctx, |ctx| scaffold_demo(ctx));
                });
        });
    });
}
