use std::ops::Not;
use winia::core::RefClone;
use winia::func;
use winia::property::{Children, Gettable, Settable, SizeProperty};
use winia::skia_safe::Color;
use winia::ui::app::{run_app, AppContext, AppProperty};
use winia::ui::item::{Gravity, InnerPosition, Size};
use winia::ui::layout::StackExt;
use winia::ui::widget::RectangleExt;
use winia::ui::{App, Item};

fn main() {
    run_app(
        App::new(main_ui)
            .title("Example")
            .preferred_size(800, 600)
    );
}

fn main_ui(app: AppContext, property: AppProperty) -> Item {
    let width = SizeProperty::from(300.0);
    
    app.stack(Children::new() +
        app.stack(Children::new() +
            app.rectangle()
                .color(Color::RED).item()
                .width(&width).height(Size::Relative(0.5))
                .name("red_rect")
                .opacity(0.5)
                .rotation(-30.0)
                .rotation_center_x(InnerPosition::End(0.0))
                .rotation_center_y(InnerPosition::End(0.0))
                .on_click(func!(|width, property|, move|_|{
                    println!("Red rectangle clicked");
                    width.set(Size::Fixed(500.0));
                    property.title().set("Red rectangle clicked".to_string());
                    property.maximized().set(property.maximized().get().not())
                }))+
            app.rectangle()
                .color(Color::BLUE)
                .item().width(Size::Fixed(100.0)).height(Size::Fixed(100.0))
                .skew_x(1.0)
                .rotation(45.0)
                .opacity(0.5)
                .name("blue_rect")
                .on_click(func!(|property|, move|_|{
                    println!("Blue rectangle clicked");
                    property.title().set("Blue rectangle clicked".to_string());
                    property.maximized().set(property.maximized().get().not())
                }))
        ).item()
            .width(Size::Fixed(400.0))
            .height(Size::Fixed(400.0))
            .background(
                app.rectangle()
                    .color(Color::WHITE)
                    .item()
            )
            .vertical_gravity(Gravity::End)
            .horizontal_gravity(Gravity::End)
    )
        .horizontal_gravity(Gravity::End)
        .vertical_gravity(Gravity::End)
        .item()
        .width(Size::Expanded)
        .height(Size::Expanded)
        .padding_start(10.0)
        .padding_top(10.0)
        .name("root")
        .on_click(|_| {
            println!("Root item clicked");
        })
}