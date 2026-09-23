//! SwipeToDismissBox demo — a mail-style list whose rows are swiped away.
//!
//! What to check by hand:
//! - drag a row sideways: it follows the finger and reveals the "删除" panel underneath;
//! - release past ~56 px (or flick it): the row leaves and the header counts it;
//! - release earlier: the row springs back;
//! - drag a row UP/DOWN: the LIST scrolls instead of the row (the axis arbitration — a row is both an
//!   inner drag target and a scrollable child, so the direction the finger moves decides);
//! - the "重置列表" button puts the list back.
//!
//! Run: cargo run -p winia --example swipe_to_dismiss_demo

use letclone::clone;
use std::sync::Arc;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;


/// One row's data. `id` is both the list key and the identity the state is remembered against.
#[derive(Clone, PartialEq)]
struct Mail {
    id: u64,
    subject: String,
    from: String,
}

/// A freshly built list. The ids carry a generation so a reset never reuses the ids of rows that were
/// dismissed in an earlier round.
fn make_mails(generation: u64) -> Vec<Mail> {
    (0..12)
        .map(|i| Mail {
            id: generation * 100 + i,
            subject: format!("邮件主题 {i}"),
            from: format!("发件人 {}", i % 4),
        })
        .collect()
}

#[composable]
fn swipe_to_dismiss_demo(ctx: &mut ComposeCtx) {
    // Remembered handles (`.get()`): a `LazyListState` and the generation counter must survive frames.
    let list_state = ctx.remember(|| LazyListState::new()).get();
    let generation: State<u64> = ctx.remember(|| 0u64);
    // A plain value: `remember` itself is the state, nothing to unwrap.
    let items: State<Arc<Vec<Mail>>> = ctx.remember(|| Arc::new(make_mails(0)));
    let dismissed: State<i32> = ctx.remember(|| 0i32);
    let last_way: State<String> = ctx.remember(|| "—".to_string());

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            // The header is its own scope: it reads the item list, so removing a row re-composes the
            // header without touching the rows themselves.
            let header_items = items.clone();
            let header_dismissed = dismissed.clone();
            let header_way = last_way.clone();
            Stack::new().build(ctx, move |ctx| {
                Text::new(format!(
                    "剩余 {} 项 · 已删除 {} 项 · 最近方向 {}",
                    header_items.get().len(),
                    header_dismissed.get(),
                    header_way.get()
                ))
                .font_size(13.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .build(ctx);
            });

            Row::new()
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Button::text()
                        .on_click({
                            clone!(items);
                            clone!(dismissed);
                            clone!(last_way);
                            clone!(generation);
                            move || {
                                // A new generation of ids: the fresh rows then bring fresh states.
                                generation.update(|g| *g += 1);
                                items.set(Arc::new(make_mails(generation.get())));
                                dismissed.set(0);
                                last_way.set("—".to_string());
                            }
                        })
                        .build(ctx, |ctx| Text::new("重置列表").build(ctx));
                    Text::new("左右滑动删除 · 在行上上下拖动可滚动列表")
                        .font_size(12.0)
                        .color(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                });

            // A lazy, KEYED list: the item key is what keeps each row's state with its own item when
            // rows above it are removed (a plain `for` loop over the data would hand the next item the
            // departed row's state — its content parked off the row and its gestures gated off).
            let list_items = items.clone();
            LazyColumn::new()
                .state(list_state.clone())
                .modifier(Modifier::new().fill_max_width().height(360.0))
                .spacing(6.0)
                .items_from(
                    items.get(),
                    |mail| mail.id,
                    move |ctx, _index, mail| {
                        let id = mail.id;
                        let subject = mail.subject.clone();
                        let from = mail.from.clone();
                        // Owned per ITEM, inside the item's own composition scope — see the note on
                        // `SwipeToDismissBox::build`: the box's built-in state does not survive per
                        // item in a keyed lazy list, so the caller supplies it.
                        let state = ctx.remember(|| SwipeToDismissBoxState::default()).get();
                        let remove_items = items.clone();
                        let count_dismissed = dismissed.clone();
                        let record_way = last_way.clone();

                        SwipeToDismissBox::new()
                            .state(state)
                            .background({
                                let colors = WiniaTheme::colors();
                                move |ctx| {
                                    // One label per edge: a row dragged left uncovers the trailing
                                    // label, one dragged right uncovers the leading one.
                                    Row::new()
                                        .arrangement(Arrangement::SpaceBetween)
                                        .modifier(
                                            Modifier::new()
                                                .fill_max_size()
                                                .padding_horizontal(20.0)
                                                .background(colors.error_container, Shape::Rectangle),
                                        )
                                        .build(ctx, |ctx| {
                                            for _ in 0..2 {
                                                Text::new("删除")
                                                    .font_size(14.0)
                                                    .color(colors.on_error_container)
                                                    .build(ctx);
                                            }
                                        });
                                }
                            })
                            .on_dismiss(move |direction| {
                                // This is where a real list drops the item from its data source.
                                remove_items.update(|v| {
                                    let kept: Vec<Mail> =
                                        v.iter().filter(|m| m.id != id).cloned().collect();
                                    *v = Arc::new(kept);
                                });
                                count_dismissed.update(|n| *n += 1);
                                record_way.set(
                                    match direction {
                                        SwipeToDismissBoxValue::StartToEnd => "向右",
                                        SwipeToDismissBoxValue::EndToStart => "向左",
                                        SwipeToDismissBoxValue::Settled => "—",
                                    }
                                    .to_string(),
                                );
                            })
                            .modifier(Modifier::new().fill_max_width().height(64.0))
                            .build(ctx, move |ctx| {
                                // The content must be opaque: it is what hides the panel until the row
                                // is dragged off it.
                                Row::new()
                                    .alignment(Alignment::Center)
                                    .modifier(
                                        Modifier::new()
                                            .fill_max_size()
                                            .padding_horizontal(16.0)
                                            .background(
                                                WiniaTheme::colors().surface_container,
                                                Shape::Rectangle,
                                            ),
                                    )
                                    .build(ctx, |ctx| {
                                        Column::new()
                                            .spacing(2.0)
                                            .build(ctx, |ctx| {
                                                Text::new(subject.clone()).font_size(15.0).build(ctx);
                                                Text::new(from.clone())
                                                    .font_size(12.0)
                                                    .color(WiniaTheme::colors().on_surface_variant)
                                                    .build(ctx);
                                            });
                                    });
                            });
                    },
                )
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 620.0)
                .title("SwipeToDismissBox 演示")
                .build(ctx, |ctx| {
                    settings::shell("SwipeToDismissBox 演示", ctx, |ctx| swipe_to_dismiss_demo(ctx));
                });
        });
    });
}
