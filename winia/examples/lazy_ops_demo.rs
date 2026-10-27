//! Lazy-list API demo — one list, driven by buttons.
//!
//! A button per behaviour: add 5 / remove 5 at the head (`items_from` with stable keys), jump to 25
//! (`scroll_to_item`), animate to 25 (`animate_scroll_to_item`), fling, reverse (`reverse_layout`),
//! lock scroll (`user_scroll(false)`) and page navigation on a pager-shaped row (`fill_items` +
//! `snap_paging` + the snap tunables). The list itself uses a `sticky_header`, both content paddings,
//! `spacing` and `beyond_bounds`, and the page prints what the list reports every frame — the
//! visible range from `layout_info`, the anchor and its offset, the scroll flags, the viewport size —
//! so `tests/ui_lazy_test.rs` can assert behaviour instead of pixels.
//!
//! Run with `cargo run -p winia --example lazy_ops_demo`; the same UI is the `lazy_ops` fixture
//! scenario driven by `tests/ui_lazy_test.rs`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;
#[path = "common/lazy_kit.rs"]
mod kit;

use kit::{accent, action, card, list_name, palette, tile, INITIAL, PAGES, STEP};

/// `items_from` with stable keys (add/remove at the head), `scroll_to_item`, `animate_scroll_to_item`,
/// `fling`, `reverse_layout`, `user_scroll(false)`, `sticky_header`, both paddings, `spacing`,
/// `beyond_bounds`, and a pager row using `fill_items` + the four `snap_*` knobs.
///
/// It is a separate scenario (`lazy_ops`) rather than more cards on the long page: the UI test clicks
/// these buttons, and a button below the fold cannot be clicked by coordinates.
#[composable]
fn lazy_ops_demo(ctx: &mut ComposeCtx) {
    let page_scroll = ctx.remember(|| ScrollState::new()).get();
    // The data is the source of truth: keys are the item ids, so an insert at the head moves the
    // indices and leaves the anchor on the same ITEM.
    let data = ctx.remember(|| std::sync::Arc::new((0..INITIAL as u64).collect::<Vec<u64>>()));
    let next_key = ctx.remember(|| INITIAL as u64);
    let main_state = ctx.remember(|| LazyListState::new()).get();
    let reverse = ctx.remember(|| false);
    let locked = ctx.remember(|| false);
    let pager_state = ctx.remember(|| LazyListState::new()).get();
    let page = ctx.remember(|| 0usize);

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(page_scroll),
        )
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("Lazy API surface, driven by buttons").font_size(18.0).build(ctx);

            card(ctx, "lazy-ops-card", "1. data, jumps, fling, reverse, lock", |ctx| {
                // A Flow, not a Row: seven buttons never fit one line, and a button outside the card
                // cannot be clicked by coordinates (which is how the UI test drives this page).
                FlowRow::new()
                    .main_spacing(6.0)
                    .cross_spacing(6.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                    action(ctx, "lazy-add-head", "add 5", {
                        clone!(data);
                        clone!(next_key);
                        move || {
                            let mut next: Vec<u64> = data.get().as_ref().clone();
                            let mut key = next_key.get();
                            for _ in 0..STEP {
                                next.insert(0, key);
                                key += 1;
                            }
                            next_key.set(key);
                            data.set(std::sync::Arc::new(next));
                        }
                    });
                    action(ctx, "lazy-remove-head", "remove 5", {
                        clone!(data);
                        move || {
                            let mut next: Vec<u64> = data.get().as_ref().clone();
                            let keep = next.len().saturating_sub(STEP);
                            next.truncate(keep);
                            data.set(std::sync::Arc::new(next));
                        }
                    });
                    action(ctx, "lazy-jump-25", "jump 25", {
                        clone!(main_state);
                        move || main_state.scroll_to_item(25, 0.0)
                    });
                    action(ctx, "lazy-animate-25", "animate 25", {
                        clone!(main_state);
                        move || main_state.animate_scroll_to_item(25, 0.0)
                    });
                });
                // A Flow, not a Row: seven buttons never fit one line, and a button outside the card
                // cannot be clicked by coordinates (which is how the UI test drives this page).
                FlowRow::new()
                    .main_spacing(6.0)
                    .cross_spacing(6.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                    action(ctx, "lazy-fling", "fling", {
                        clone!(main_state);
                        move || main_state.fling(1500.0)
                    });
                    action(ctx, "lazy-flip-reverse", "reverse", {
                        clone!(reverse);
                        move || reverse.set(!reverse.get())
                    });
                    action(ctx, "lazy-toggle-lock", "lock scroll", {
                        clone!(locked);
                        move || locked.set(!locked.get())
                    });
                });
                // What the list reports: the visible range from `layout_info`, the rest from the state.
                let info = main_state.layout_info();
                let range = match (info.visible_items.first(), info.visible_items.last()) {
                    (Some(first), Some(last)) => format!("{}-{}", first.index, last.index),
                    _ => "pending".to_string(),
                };
                Text::new(format!(
                    "items {range} of {} | first {} @{:.0} | offset {:.0} | forward {} backward {} | viewport {:.0} | reverse {} lock {}",
                    info.total_items,
                    main_state.first_visible(),
                    main_state.first_visible_offset(),
                    main_state.offset(),
                    main_state.can_scroll_forward(),
                    main_state.can_scroll_backward(),
                    main_state.viewport_main_size(),
                    reverse.get(),
                    locked.get(),
                ))
                .modifier(Modifier::new().test_tag("lazy-main-info"))
                .font_size(11.0)
                .build(ctx);
            });

            card(ctx, "lazy-main-card", "2. LazyColumn — keys, sticky, paddings", |ctx| {
                LazyColumn::new()
                    .state(main_state.clone())
                    .content_padding(8.0, 8.0)
                    .content_padding_cross(12.0, 4.0)
                    .spacing(6.0)
                    .beyond_bounds(2)
                    .reverse_layout(reverse.get())
                    .user_scroll(!locked.get())
                    .sticky_header(u64::MAX, |ctx| {
                        tile(
                            ctx,
                            "lazy-main-header".to_string(),
                            "sticky".to_string(),
                            120.0,
                            28.0,
                            Color::from_argb(255, 90, 120, 200),
                        );
                    })
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(200.0)
                            .test_tag("lazy-main-list")
                            .then(list_name("main list")),
                    )
                    .items_from(data.get(), |key| *key, |ctx, _index, key| {
                        tile(
                            ctx,
                            format!("lazy-main-item-{key}"),
                            format!("{key}"),
                            120.0,
                            32.0,
                            palette(*key as usize),
                        );
                    })
                    .build(ctx);
            });

            card(ctx, "lazy-pager-card", "3. pager row — fill_items, snap_paging", |ctx| {
                LazyRow::new()
                    .state(pager_state.clone())
                    .fill_items(true)
                    .snap_paging(true)
                    .snap_positional_threshold(0.3)
                    .snap_max_pages(2)
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(80.0)
                            .test_tag("lazy-pager")
                            .then(list_name("pager")),
                    )
                    .items(PAGES, |i| 9000 + i as u64, |ctx, i| {
                        tile(
                            ctx,
                            format!("lazy-page-{i}"),
                            format!("page {i}"),
                            120.0,
                            60.0,
                            accent(i),
                        );
                    })
                    .build(ctx);
                // A Flow, not a Row: seven buttons never fit one line, and a button outside the card
                // cannot be clicked by coordinates (which is how the UI test drives this page).
                FlowRow::new()
                    .main_spacing(6.0)
                    .cross_spacing(6.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                    action(ctx, "lazy-next-page", "next page", {
                        clone!(page);
                        clone!(pager_state);
                        move || {
                            let target = (page.get() + 1).min(PAGES - 1);
                            page.set(target);
                            pager_state.scroll_to_item(target, 0.0);
                        }
                    });
                    action(ctx, "lazy-first-page", "first page", {
                        clone!(page);
                        clone!(pager_state);
                        move || {
                            page.set(0);
                            pager_state.scroll_to_item(0, 0.0);
                        }
                    });
                    Text::new(format!("page {} of {}", page.get() + 1, PAGES))
                        .modifier(Modifier::new().test_tag("lazy-page-info"))
                        .font_size(11.0)
                        .build(ctx);
                });
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 900.0)
                .title("Lazy ops")
                .build(ctx, |ctx| {
                    settings::shell("Lazy Ops", ctx, lazy_ops_demo);
                });
        });
    });
}
