//! Item animation showcase: placement, fade-in and fade-out share one `animate_item` contract.
//!
//! Run with `cargo run -p winia --example item_animation_demo`.

use winia::animation::{item::ItemAnimationSpec, SpringSpec, TweenSpec};
use winia::animation::interpolator::EaseOutCubic;
use winia::prelude::*;

fn item_spec() -> ItemAnimationSpec {
    ItemAnimationSpec::new()
        .placement(SpringSpec {
            damping_ratio: 0.82,
            stiffness: 320.0,
            ..Default::default()
        })
        .fade_in(TweenSpec::new(
            std::time::Duration::from_millis(300),
            EaseOutCubic::new(),
        ))
        .fade_out(TweenSpec::new(
            std::time::Duration::from_millis(300),
            EaseOutCubic::new(),
        ))
}

fn item(ctx: &mut ComposeCtx, items: &State<Vec<u64>>, id: u64) {
    let items = items.clone();
    Stack::new()
        .modifier(
            Modifier::new()
                .size(88.0, 64.0)
                .animate_item_with(item_spec())
                .test_tag(format!("item-{id}"))
                .background(
                    Color::from_argb(255, 92, 126, 196),
                    Shape::rounded(10.0),
                )
                .clickable(move || {
                    let mut next = items.get();
                    next.retain(|value| *value != id);
                    items.set(next);
                }),
        )
        .content_alignment(ContentAlignment::CENTER)
        .build(ctx, |ctx| {
            Text::new(format!("#{id}"))
                .font_size(16.0)
                .color(Color::WHITE)
                .build(ctx);
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 640.0)
                .title("Item animation")
                .build(ctx, |ctx| {
                    let items = ctx.remember(|| (0u64..4).collect::<Vec<u64>>());
                    let next_id = ctx.remember(|| 4u64);
                    Column::new()
                        .modifier(Modifier::new().fill_max_size().padding(24.0))
                        .spacing(16.0)
                        .build(ctx, |ctx| {
                            Text::new("Item animation")
                                .font_size(24.0)
                                .build(ctx);
                            Text::new("add at the head, or tap a card to remove it")
                                .font_size(14.0)
                                .build(ctx);
                            Row::new()
                                .spacing(8.0)
                                .build(ctx, |ctx| {
                                    Button::new()
                                        .modifier(Modifier::new().test_tag("add-head"))
                                        .on_click({
                                            let items = items.clone();
                                            let next_id = next_id.clone();
                                            move || {
                                                let id = next_id.get();
                                                next_id.set(id + 1);
                                                let mut next = items.get();
                                                next.insert(0, id);
                                                items.set(next);
                                            }
                                        })
                                        .build(ctx, |ctx| Text::new("add head").build(ctx));
                                    Button::new()
                                        .modifier(Modifier::new().test_tag("add-item"))
                                        .on_click({
                                            let items = items.clone();
                                            let next_id = next_id.clone();
                                            move || {
                                                let id = next_id.get();
                                                next_id.set(id + 1);
                                                let mut next = items.get();
                                                next.push(id);
                                                items.set(next);
                                            }
                                        })
                                        .build(ctx, |ctx| Text::new("add tail").build(ctx));
                                    Button::new()
                                        .modifier(Modifier::new().test_tag("remove-item"))
                                        .on_click({
                                            let items = items.clone();
                                            move || {
                                                let mut next = items.get();
                                                next.pop();
                                                items.set(next);
                                            }
                                        })
                                        .build(ctx, |ctx| Text::new("remove tail").build(ctx));
                                });
                            AnimatedSize::default()
                                .modifier(
                                    Modifier::new().fill_max_width().test_tag("items-box"),
                                )
                                .build(ctx, |ctx| {
                                    FlowRow::new()
                                        .main_spacing(8.0)
                                        .cross_spacing(8.0)
                                        .max_items_in_row(3)
                                        .modifier(
                                            Modifier::new()
                                                .fill_max_width()
                                                .test_tag("items-flow"),
                                        )
                                        .build(ctx, |ctx| {
                                            let current = items.get();
                                            for id in current {
                                                // Slot key 0 is the "no explicit key" sentinel; item ids
                                                // start at 0, so shift the composition key.
                                                ctx.key(id + 1, |ctx| item(ctx, &items, id));
                                            }
                                        });
                                });
                        });
                });
        });
    });
}
