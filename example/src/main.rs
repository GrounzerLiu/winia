use material_colors::color::Argb;
use material_colors::dynamic_color::Variant;
use material_colors::theme::ThemeBuilder;
use std::ops::Add;
use std::thread;
use std::time::Duration;
use winia::shared::{Children, Gettable, Settable, Shared, SharedBool, SharedF32, SharedSize, SharedText};
use winia::skia_safe::Color;
use winia::text::StyledText;
use winia::ui::animation::{AnimationExt, Target};
use winia::ui::app::{run_app, AppContext, AppProperty};
use winia::ui::component::{ImageExt, RectangleExt, RippleExt, ScaleMode, TextExt};
use winia::ui::item::{Alignment, Size};
use winia::ui::layout::FlexWrap::Wrap;
use winia::ui::layout::{AlignContent, AlignItems, ColumnExt, FlexDirection, FlexExt, FlexWrap, JustifyContent, RowExt, ScrollAreaExt, StackExt};
use winia::ui::{App, Item};
use winia::{exclude_target, func, include_target, shared};
use winia::ui::animation::interpolator::Linear;
use winia::ui::unit::Dp;

// #[cfg(not(target_os = "android"))]
fn main() {
    run_app(App::new(animation_test).title("Example").preferred_size(800, 600));
    // run_app(
    //     App::new(|app, shared| {
    //         app.flex(Children::new() +
    //             app.text_block("Hello, مرحبا بك في سكيا world!").color(Color::WHITE).item()
    //         ).wrap(FlexWrap::Wrap).item()
    //     })
    //         .title("Example")
    //         .preferred_size(800, 600)
    // );
}

pub fn animation_test(app: AppContext, property: AppProperty) -> Item {
    //let width = Shared::from(100.dp());
    app.row(Children::new() +
/*        app.rectangle(Color::TRANSPARENT)
            .radius(10.dp())
            .outline_width(5.dp())
            .outline_color(Color::BLACK)
            .item()
            .name("rectangle")
            .width(&width)
            .height(150.dp())
/*            .on_click(
                |_| {
                    let app = app.clone();
                    let width = width.clone();
                    // app.animate(exclude_target!("blue")).transformation({
                    //     let mut width = width.clone();
                    //     move || {
                    //         if width.get() == Size::Fixed(100.0) {
                    //             width.set(Size::Fixed(200.0));
                    //         } else {
                    //             width.set(Size::Fixed(100.0));
                    //         }
                    //     }
                    // }).interpolator(Box::new(Linear::new())).duration(Duration::from_millis(1000)).start();
                }
            )*/
            .on_click(
                func!(|app, width|, move |_| {
                    app.animate(exclude_target!("blue")).transformation(
                        func!(|width|, move || {
                            if width.get() == Size::Fixed(100.dp()) {
                                width.set(200.dp());
                            } else {
                                width.set(Size::Fixed(100.dp()));
                            }
                        })
                    ).interpolator(Box::new(Linear::new())).duration(Duration::from_millis(1000)).start();
                })
            )+*/
        app.rectangle(Color::BLUE)
            .radius(50.dp())
            .item()
            .name("blue")
            .width(100.dp())
            .height(100.dp())
            .elevation(3.dp()) /*+
        app.text("Hello, world!").color(Color::BLACK).item()*/
    ).name("row")
        .padding_start(16.dp())
        .padding_top(16.dp())
}