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
            std::time::Duration::from_millis(500),
            EaseOutCubic::new(),
        ))
}

fn item(ctx: &mut ComposeCtx, n: usize) {
    Stack::new()
        .modifier(
            Modifier::new()
                .size(88.0, 64.0)
                .animate_item_with(item_spec())
                .test_tag(format!("item-{n}"))
                .background(
                    Color::from_argb(255, 92, 126, 196),
                    Shape::rounded(10.0),
                ),
        )
        .content_alignment(ContentAlignment::CENTER)
        .build(ctx, |ctx| {
            Text::new(format!("#{n}"))
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
                    let count = ctx.remember(|| 4usize);
                    Column::new()
                        .modifier(Modifier::new().fill_max_size().padding(24.0))
                        .spacing(16.0)
                        .build(ctx, |ctx| {
                            Text::new("Item animation")
                                .font_size(24.0)
                                .build(ctx);
                            Text::new("place, fade in, and remove an item")
                                .font_size(14.0)
                                .build(ctx);
                            Row::new()
                                .spacing(8.0)
                                .build(ctx, |ctx| {
                                    Button::new()
                                        .modifier(Modifier::new().test_tag("add-item"))
                                        .on_click({
                                            let count = count.clone();
                                            move || count.set(count.get() + 1)
                                        })
                                        .build(ctx, |ctx| Text::new("add item").build(ctx));
                                    Button::new()
                                        .modifier(Modifier::new().test_tag("remove-item"))
                                        .on_click({
                                            let count = count.clone();
                                            move || count.set(count.get().saturating_sub(1))
                                        })
                                        .build(ctx, |ctx| Text::new("remove item").build(ctx));
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
                                            let total = count.get();
                                            ctx.keyed_range(
                                                "item-animation-demo",
                                                total,
                                                |ctx, n| {
                                                    item(ctx, n);
                                                },
                                            );
                                        });
                                });
                        });
                });
        });
    });
}
