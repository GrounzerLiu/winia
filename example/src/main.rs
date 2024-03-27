use quikia::app::{run_app, SharedApp};
use quikia::{Color, stack};
use quikia::dpi::{LogicalSize, Size};
#[cfg(target_os = "android")]
use quikia::platform::android::activity::AndroidApp;
use quikia::property::{FloatProperty, Gettable, ItemProperty};
use quikia::theme::material_theme;
use quikia::ui::{ButtonState, Children, Gravity, Item};
use quikia::widget::{RectangleExt, Stack, StackExt};
use quikia::window::WindowBuilder;

macro_rules! str {
    ($($x:tt{$y:ident})*) => {
        {  
            let mut s = String::new();
             $(
                s.push_str($x);
                s.push_str($y.to_string().as_str());
            )*
            s
        }
    };
}

#[cfg(not(target_os = "android"))]
fn main() {
    let window_builder = WindowBuilder::new()
        .with_title("Hello, world!")
        .with_inner_size(Size::Logical(LogicalSize::new(800.0, 600.0)))
        .with_min_inner_size(Size::Logical(LogicalSize::new(400.0, 300.0)));
    run_app(window_builder, material_theme(Color::BLUE, true), main_ui);
}

fn main_ui(app: SharedApp) -> Item {
    let offset_x = FloatProperty::from(0.0);
    let offset_y = FloatProperty::from(0.0);
    let children = Children::new();
    let children_manager = children.manager();
    stack!(
        stack!(
            app.stack(children)
            .item()
            .add_child(
                app.rectangle()
                .color(Color::YELLOW)
                .item()
                .width(300)
                .height(300)
            )
            .width(300)
            .height(300)
            // app.rectangle()
            // .color(Color::RED)
            // .item()
            // .width(100.0)
            // .height(100.0)
            // .margin_start(10.0)
            // .margin_top(20.0)
            // .margin_end(0)
            // .offset_x(&offset_x)
            // .offset_y(&offset_y)
            // .on_click(|pointer|{
            //     // println!("Hello, world! {:?}", pointer);
            // })
            // .on_mouse_input({
            //     let mut offset_x = offset_x.clone();
            //     let mut offset_y = offset_y.clone();
            //     let mut last_x = FloatProperty::from(0.0);
            //     let mut last_y = FloatProperty::from(0.0);
            //     let app = app.clone();
            //     move|item, event|{
            //         match event.state{
            //             ButtonState::Pressed => {
            //                 last_x.set_value(event.x);
            //                 last_y.set_value(event.y);
            //             }
            //             ButtonState::Moved => {
            //                 let dx = event.x - last_x.get();
            //                 let dy = event.y - last_y.get();
            //                 offset_x.set_value(offset_x.get() + dx);
            //                 offset_y.set_value(offset_y.get() + dy);
            //                 last_x.set_value(event.x);
            //                 last_y.set_value(event.y);
            //                 app.request_layout();
            //             }
            //             ButtonState::Released => {
            //                 
            //             }
            //         }
            //     false
            // }})
            // .tag("rect")
            app.rectangle()
            .color(Color::BLUE)
            .item()
            .width(100.0)
            .height(100.0)
            .on_click({
                let app = app.clone();
                let mut children_manager = children_manager.clone();
                move|_|{
                    if children_manager.len()==0{
                        children_manager.add(app.rectangle().color(Color::RED).item().width(200).height(200));
                    }
                    else{
                        children_manager.clear();
                    }
                    app.request_layout();
                }
            })
        ).item()
        .width(600)
        .height(600)
        .background(app.rectangle().color(Color::GREEN))
        .margin_start(20.0)
        .margin_top(10.0)
        .horizontal_gravity(Gravity::End)
        .vertical_gravity(Gravity::Center)
    ).item()
}

/*#[cfg(target_os = "android")]

use quikia::platform::android::activity::AndroidApp;*/

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: AndroidApp) {
    let window_builder = WindowBuilder::new()
        .with_title("Hello, world!")
        .with_inner_size(Size::Logical(LogicalSize::new(800.0, 600.0)));
    run_app(app, window_builder, Box::new(MainPage::new()));
}

#[cfg(target_os = "android")]
fn main() {}

