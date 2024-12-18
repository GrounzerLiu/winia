use std::ops::Not;
use std::time::Duration;
use winia::core::RefClone;
use winia::event_loop::EventLoop;
use winia::func;
use winia::property::{Children, F32Property, Gettable, Property, Settable, SizeProperty};
use winia::skia_safe::{Color, FontMgr};
use winia::skia_safe::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextAlign, TextStyle};
use winia::ui::app::{run_app, AppContext, AppProperty, UserEvent};
use winia::ui::item::{Gravity, InnerPosition, Size};
use winia::ui::layout::{AlignContent, AlignItems, ColumnExt, FlexDirection, FlexExt, FlexGrow, FlexWrap, JustifyContent, StackExt};
use winia::ui::component::{RectangleExt, TextBlockExt};
use winia::ui::{App, Item};
use winia::ui::animation::{AnimationExt, Target};

// #[cfg(not(target_os = "android"))]
fn main() {
    run_app(
        App::new(main_ui)
            .title("Example")
            .preferred_size(800, 600)
    );
    // run_app(
    //     App::new(|app, property| {
    //         app.flex(Children::new() +
    //             app.text_block("Hello, مرحبا بك في سكيا world!").color(Color::WHITE).item()
    //         ).wrap(FlexWrap::Wrap).item()
    //     })
    //         .title("Example")
    //         .preferred_size(800, 600)
    // );
}

fn flex_test_ui(app: AppContext, property: AppProperty) -> Item {
    let size = SizeProperty::from(Size::Fixed(150.0));
    app.flex(Children::new() +
        app.column(Children::new() +
            app.rectangle()
                .color(Color::YELLOW)
                .item()
                .width(Size::Fixed(50.0))
                .height(Size::Fixed(50.0)) +
            app.rectangle()
                .color(Color::GREEN)
                .item()
                .width(Size::Fixed(50.0))
                .height(Size::Fixed(50.0)) +
            app.rectangle()
                .color(Color::BLUE)
                .item()
                .width(Size::Fixed(50.0))
                .height(Size::Fixed(50.0)) +
            app.rectangle()
                .color(Color::YELLOW)
                .item()
                .width(Size::Fixed(50.0))
                .height(Size::Fixed(50.0)) +
            app.rectangle()
                .color(Color::GREEN)
                .item()
                .width(Size::Fixed(50.0))
                .height(Size::Fixed(50.0))
        ) +
        app.rectangle()
            .color(Color::RED)
            .item()
            .height(&size)
            .width(Size::Fixed(250.0)) +
        app.rectangle()
            .color(Color::GREEN)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::BLUE)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::RED)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::GREEN)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::BLUE)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::RED)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::GREEN)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::BLUE)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::RED)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::GREEN)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::BLUE)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::RED)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::GREEN)
            .item()
            .width(&size)
            .height(&size) +
        app.rectangle()
            .color(Color::BLUE)
            .item()
            .width(&size)
            .height(&size)
    )
        .direction(FlexDirection::Horizontal)
        .wrap(FlexWrap::Wrap)
        .justify_content(JustifyContent::Start)
        .align_items(AlignItems::Start)
        .align_content(AlignContent::Start)
        // .main_axis_gap(10.0)
        // .cross_axis_gap(10.0)
        .item()
        .on_click(func!(|app,size|, move|_|{
            if let Size::Fixed(size_value) = size.get() {
                if size_value > 150.0 {
                    app.animate(Target::Exclusion(Vec::new()))
                        .transformation(func!(|size|,move|| {
                            size.set(Size::Fixed(150.0));
                        })).duration(Duration::from_millis(500)).start();
                }else {
                    app.animate(Target::Exclusion(Vec::new()))
                        .transformation(func!(|size|,move|| {
                            size.set(Size::Fixed(200.0));
                        })).duration(Duration::from_millis(500)).start();
                }
            }
        }))
}

fn text_test_ui(app: AppContext, property: AppProperty) -> Item {
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum. Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo. Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit, sed quia non numquam eius modi tempora incidunt ut labore et dolore magnam aliquam quaerat voluptatem. Ut enim ad minima veniam, quis nostrum exercitationem ullam corporis suscipit laboriosam, nisi ut aliquid ex ea commodi consequatur? Quis autem vel eum iure reprehenderit qui in ea voluptate velit esse quam nihil molestiae consequatur, vel illum qui dolorem eum fugiat quo voluptas nulla pariatur?
";
    app.stack(Children::new() +
        app.text_block(text)
            .color(Color::WHITE)
            .item()
            .width(Size::Expanded)
    )
        .item()
}

fn main_ui(app: AppContext, property: AppProperty) -> Item {
    let size = SizeProperty::from(100.0);
    let offset = F32Property::from(0.0);
    let margin_start = F32Property::from(150.0);
    let margin_top = F32Property::from(50.0);
    let vertical_gravity = Property::from(Gravity::End);
    let horizontal_gravity = Property::from(Gravity::End);

    // let txt_file = "/home/grounzer/Downloads/long.txt";
    // let long_text = std::fs::read_to_string(txt_file).unwrap();
    //

    let text = "Hello, world!";
    app.stack(Children::new() +
        app.stack(Children::new() +
            app.rectangle()
                .color(Color::BLUE)
                .item().width(Size::Fixed(100.0)).height(Size::Fixed(100.0))
                // .skew_x(1.0)
                // .rotation(45.0)
                // .opacity(0.5)
                .name("blue_rect")
                .on_click(func!(|property|, move|_|{
                    println!("Blue rectangle clicked");
                    property.title().set("Blue rectangle clicked".to_string());
                    property.maximized().set(property.maximized().get().not())
                })) +

            app.flex(Children::new() +
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()+
                app.text_block(text).item()
            )
                .direction(FlexDirection::Horizontal)
                .wrap(FlexWrap::Wrap).item()
                .width(Size::Fixed(400.0))+

            app.rectangle()
                .color(Color::WHITE).item()
                .width(&size).height(&size)
                .offset_x(&offset)
                .offset_y(&offset)
                .margin_start(&margin_start)
                .margin_top(&margin_top)
                .name("red_rect")
                .enable_background_blur(true)
                .opacity(0.5)
                // .rotation(-30.0)
                // .rotation_center_x(InnerPosition::End(0.0))
                // .rotation_center_y(InnerPosition::End(0.0))
                .on_click(func!(|app, size, margin_start, margin_top, offset|, move|_|{
                    app.animate(Target::Exclusion(Vec::new()))
                    .transformation(func!(|size, margin_start, margin_top, offset|,move|| {
                        if let Size::Expanded = size.get() {
                            size.set(Size::Fixed(100.0));
                            margin_start.set(150.0);
                            margin_top.set(50.0);
                            offset.set(0.0);
                        } else {
                            size.set(Size::Expanded);
                            margin_start.set(0.0);
                            margin_top.set(0.0);
                            offset.set(-50.0);
                        }
                    })).duration(Duration::from_secs(5)).start();
                }))
        )
            .item()
            .width(Size::Fixed(400.0))
            .height(Size::Fixed(400.0))
            .horizontal_gravity(&horizontal_gravity)
            .vertical_gravity(&vertical_gravity)
            .background(
                app.rectangle()
                    .color(Color::GREEN)
                    .item()
            )
            .on_click(func!(|app, horizontal_gravity, vertical_gravity|, move|_|{
                app.animate(Target::Exclusion(Vec::new()))
                    .transformation(func!(|horizontal_gravity, vertical_gravity|,move|| {
                    if horizontal_gravity.get() == Gravity::Start && vertical_gravity.get() == Gravity::Start {
                        horizontal_gravity.set(Gravity::End);
                    }else if horizontal_gravity.get() == Gravity::End && vertical_gravity.get() == Gravity::Start {
                        vertical_gravity.set(Gravity::End);
                    }else if horizontal_gravity.get() == Gravity::End && vertical_gravity.get() == Gravity::End {
                        horizontal_gravity.set(Gravity::Start);
                    }else {
                        vertical_gravity.set(Gravity::Start);
                    }
            })).duration(Duration::from_secs(1)).start();
        }))
    )
        .horizontal_gravity(&horizontal_gravity)
        .vertical_gravity(&vertical_gravity)
        .item()
        .width(Size::Expanded)
        .height(Size::Expanded)
        .padding_start(10.0)
        .padding_top(10.0)
        .name("root")
}
