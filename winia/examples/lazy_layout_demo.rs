//! Lazy-list layout demo — the three cases the review fixed, with lists long enough to scroll.
//!
//! 1. RTL: Compose lays a `LazyRow` out from the RIGHT in RTL, so index 0 sits at the right edge and
//!    the items run leftward. winia's list mirrors its content coordinates (the policy mirrors the
//!    placement, the scroll modifier mirrors the translate) — the same pair of steps
//!    `reverseLayout` uses.
//! 2. An RTL COLUMN keeps its main axis (a column must not scroll upward) but mirrors its CROSS
//!    axis, so `content_padding_cross`'s "before" is the right edge and a narrow item hugs it.
//! 3. A lazy list inside a horizontally unbounded parent reports its WIDEST item instead of the
//!    `f32::MAX` sentinel (3.4e38 would poison the scroll container's content width, hit testing
//!    and any scrollbar built on it).
//!
//! Every list here holds `ITEMS` entries, far more than fits: a lazy list that cannot scroll does
//! not exercise the window, the anchor or the mirror translate.
//!
//! Run with `cargo run -p winia --example lazy_layout_demo`; the same UI is the `lazy_layout`
//! fixture scenario driven by `tests/ui_lazy_test.rs`.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Long enough to scroll in every card, small enough to stay cheap to build.
const ITEMS: usize = 200;

fn card(ctx: &mut ComposeCtx, tag: &'static str, title: &str, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .test_tag(tag)
                .background(Color::from_argb(14, 60, 90, 140), Shape::rounded(8.0))
                .padding(10.0),
        )
        .spacing(6.0)
        .build(ctx, |ctx| {
            Text::new(title)
                .modifier(Modifier::new().test_tag(format!("{tag}-title")))
                .font_size(14.0)
                .build(ctx);
            content(ctx);
        });
}

fn tile(ctx: &mut ComposeCtx, tag: String, label: String, w: f32, h: f32, color: Color) {
    Stack::new()
        .content_alignment(ContentAlignment::CENTER)
        .modifier(
            Modifier::new()
                .size(w, h)
                .test_tag(tag)
                .background(color, Shape::rounded(6.0)),
        )
        .build(ctx, |ctx| {
            Text::new(label).font_size(11.0).color(Color::WHITE).build(ctx);
        });
}

// `Color::from_argb` is not a `const fn`, so the palette is a function, not constants.
/// A reader announces a collection by the name the caller gives it; the component only declares the
/// item count ("RTL row, list, 200 items"). A container with no name of its own gets UIA's fallback,
/// which concatenates the children's names into "0 1 2 3 …".
fn list_name(label: &str) -> Modifier {
    Modifier::new().semantics(SemanticsConfig::new().content_description(label))
}

fn palette(i: usize) -> Color {
    if i % 2 == 0 {
        Color::from_argb(255, 90, 160, 220)
    } else {
        Color::from_argb(255, 120, 200, 140)
    }
}

fn accent(i: usize) -> Color {
    if i % 2 == 0 {
        Color::from_argb(255, 220, 150, 90)
    } else {
        Color::from_argb(255, 200, 110, 60)
    }
}

#[composable]
fn lazy_layout_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .spacing(14.0)
        .build(ctx, |ctx| {
            Text::new("Lazy list layout — RTL and the unbounded cross axis")
                .font_size(18.0)
                .build(ctx);

            // 1. RTL row: item 0 at the right edge at offset 0, running leftward, 200 tiles so it
            // scrolls. The modifier carries the direction, so the demo does not depend on the
            // window's local (the shell flips that too, and both routes end at the same resolution).
            card(ctx, "lazy-rtl-row-card", "1. LazyRow under RTL (scroll it)", |ctx| {
                LazyRow::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(60.0)
                            .layout_direction(LayoutDirection::Rtl)
                            .test_tag("lazy-rtl-row")
                            .then(list_name("RTL row")),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(ctx, format!("lazy-rtl-item-{i}"), format!("{i}"), 100.0, 44.0, palette(i));
                    })
                    .build(ctx);
            });

            // 2. RTL column with cross content padding: the main axis still runs downward, the cross
            // axis mirrors (start padding 16 on the right, 4 on the left), so a 120 dp item hugs the
            // right edge.
            card(ctx, "lazy-rtl-col-card", "2. LazyColumn under RTL (scroll it)", |ctx| {
                LazyColumn::new()
                    .content_padding_cross(16.0, 4.0)
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(150.0)
                            .layout_direction(LayoutDirection::Rtl)
                            .test_tag("lazy-rtl-column")
                            .then(list_name("RTL column")),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(
                            ctx,
                            format!("lazy-rtl-col-item-{i}"),
                            format!("{i}"),
                            120.0,
                            36.0,
                            palette(i),
                        );
                    })
                    .build(ctx);
            });

            // 3. Unbounded cross axis: the strip is a HORIZONTAL scroll, so the lazy column's width
            // axis has no maximum (f32::MAX). It must report 50 + 6 + 4 = 60 dp, not 3.4e38. Three
            // items are enough — this card is about measurement, not scrolling.
            card(ctx, "lazy-unbounded-card", "3. unbounded cross axis", |ctx| {
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(90.0)
                            .horizontal_scroll(ScrollState::new())
                            .test_tag("lazy-unbounded-clip")
                            .background(Color::from_argb(30, 200, 120, 90), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx| {
                        LazyColumn::new()
                            .content_padding_cross(6.0, 4.0)
                            .modifier(Modifier::new().test_tag("lazy-unbounded-column"))
                            .items(3, |i| i as u64, |ctx, i| {
                                tile(
                                    ctx,
                                    format!("lazy-unbounded-item-{i}"),
                                    format!("{i}"),
                                    50.0,
                                    30.0,
                                    accent(i),
                                );
                            })
                            .build(ctx);
                    });
            });

            // 4. The LTR control: nothing above may move a list that is not mirrored. It also shows
            //    its own `layoutInfo` — the visible range as the measure pass reported it, which is
            //    Compose's `LazyListState.layoutInfo` read from a composable.
            card(ctx, "lazy-ltr-row-card", "4. LTR control (scroll it)", |ctx| {
                let state = ctx.remember(|| LazyListState::new()).get();
                LazyRow::new()
                    .state(state.clone())
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(60.0)
                            .test_tag("lazy-ltr-row")
                            .then(list_name("LTR row")),
                    )
                    .items(ITEMS, |i| i as u64, |ctx, i| {
                        tile(ctx, format!("lazy-ltr-item-{i}"), format!("{i}"), 100.0, 44.0, accent(i));
                    })
                    .build(ctx);
                // Rendered unconditionally: the first composition runs before the first measure, so
                // the placeholder is what shows on frame one and the real range from frame two.
                let info = state.layout_info();
                let label = match (info.visible_items.first(), info.visible_items.last()) {
                    (Some(first), Some(last)) => {
                        format!("items {}-{} of {}", first.index, last.index, info.total_items)
                    }
                    _ => "items pending".to_string(),
                };
                Text::new(label)
                    .modifier(Modifier::new().test_tag("lazy-ltr-info"))
                    .font_size(11.0)
                    .build(ctx);
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 860.0)
                .title("Lazy layout demo")
                .build(ctx, |ctx| {
                    settings::shell("Lazy Layout", ctx, lazy_layout_demo);
                });
        });
    });
}
// ═══════════════════════════════════════════════════════════
// The second entry point: one list, driven by buttons
// ═══════════════════════════════════════════════════════════

/// How many items the driven list starts with, and how many a button adds or removes.
const INITIAL: usize = 50;
const STEP: usize = 5;
/// Pages in the pager row.
const PAGES: usize = 5;

/// A small toolbar button: the tag is what a test clicks.
fn action(ctx: &mut ComposeCtx, tag: &'static str, label: &str, on_click: impl Fn() + Send + Sync + 'static) {
    Button::new()
        .modifier(Modifier::new().test_tag(tag))
        .on_click(on_click)
        .build(ctx, |ctx| Text::new(label).font_size(12.0).build(ctx));
}

/// A compact page that exercises the REST of the API surface, with a button per behaviour:
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
                Row::new().spacing(6.0).build(ctx, |ctx| {
                    let (data_add, next_key_add) = (data.clone(), next_key.clone());
                    action(ctx, "lazy-add-head", "add 5", move || {
                        let mut next: Vec<u64> = data_add.get().as_ref().clone();
                        let mut key = next_key_add.get();
                        for _ in 0..STEP {
                            next.insert(0, key);
                            key += 1;
                        }
                        next_key_add.set(key);
                        data_add.set(std::sync::Arc::new(next));
                    });
                    let data_remove = data.clone();
                    action(ctx, "lazy-remove-head", "remove 5", move || {
                        let mut next: Vec<u64> = data_remove.get().as_ref().clone();
                        let keep = next.len().saturating_sub(STEP);
                        next.truncate(keep);
                        data_remove.set(std::sync::Arc::new(next));
                    });
                    let jump = main_state.clone();
                    action(ctx, "lazy-jump-25", "jump 25", move || jump.scroll_to_item(25, 0.0));
                    let animate = main_state.clone();
                    action(ctx, "lazy-animate-25", "animate 25", move || {
                        animate.animate_scroll_to_item(25, 0.0)
                    });
                });
                // Seven buttons do not fit one 440 dp row, and a button outside the card cannot be
                // clicked by coordinates — which is exactly how the UI test drives this page.
                Row::new().spacing(6.0).build(ctx, |ctx| {
                    let fling = main_state.clone();
                    action(ctx, "lazy-fling", "fling", move || fling.fling(1500.0));
                    let flip = reverse.clone();
                    action(ctx, "lazy-flip-reverse", "reverse", move || flip.set(!flip.get()));
                    let lock = locked.clone();
                    action(ctx, "lazy-toggle-lock", "lock scroll", move || lock.set(!lock.get()));
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
                Row::new().spacing(6.0).build(ctx, |ctx| {
                    let next_page = page.clone();
                    let pager = pager_state.clone();
                    action(ctx, "lazy-next-page", "next page", move || {
                        let target = (next_page.get() + 1).min(PAGES - 1);
                        next_page.set(target);
                        pager.scroll_to_item(target, 0.0);
                    });
                    let reset_page = page.clone();
                    let pager = pager_state.clone();
                    action(ctx, "lazy-first-page", "first page", move || {
                        reset_page.set(0);
                        pager.scroll_to_item(0, 0.0);
                    });
                    Text::new(format!("page {} of {}", page.get() + 1, PAGES))
                        .modifier(Modifier::new().test_tag("lazy-page-info"))
                        .font_size(11.0)
                        .build(ctx);
                });
            });
        });
}

/// Entry point for the `lazy_ops` fixture scenario.
pub fn ops_main() {
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
