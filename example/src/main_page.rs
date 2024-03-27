/*use std::thread;
use std::time::Duration;
use quikia::animation::{Animation, AnimationExt};
use quikia::app::{Page, SharedApp, ThemeColor};
use quikia::{Color, flex_layout, stack};
use quikia::component::Button;
//use quikia::{clonify, Color, row, scroller, text_block};
use quikia::ui::{Gravity, Image, Item, LayoutDirection, Rectangle, RectangleExt, Ripple, TextBlock};
use quikia::ui::additional_property::{ShadowBlur, ShadowColor, ShadowOffsetY};
use quikia::layout::FlexLayout;
use quikia::property::{BoolProperty, ColorProperty, FloatProperty, Gettable, GravityProperty, SizeProperty};
use quikia::property::Size::{Fill, Fixed};

// macro_rules! repeat {
//     ($item:expr; 0) => {};
//     ($item:expr; $count:expr) => {
//         $item
//         repeat!($item; $count - 1)
//     };
// }
pub struct MainPage {
    rectangle1_active: BoolProperty,
    width: SizeProperty,
    color: ColorProperty,
    radius: FloatProperty,
    gravity: GravityProperty,
}

impl MainPage {
    pub fn new() -> Self {
        Self {
            rectangle1_active: BoolProperty::from_value(true),
            width: SizeProperty::from_value(Fixed(100.0)),
            color: Color::BLUE.into(),
            radius: FloatProperty::from_value(100.0),
            gravity: Gravity::Start.into(),
        }
    }
}

impl Page for MainPage {
    fn build(&mut self, app: SharedApp) -> Item {
        let c = app.lock().unwrap().theme().get_color(ThemeColor::Primary);
        let primary = app.lock().unwrap().theme().get_color(ThemeColor::Primary);
        let secondary = app.lock().unwrap().theme().get_color(ThemeColor::Secondary);
        let tertiary = app.lock().unwrap().theme().get_color(ThemeColor::Tertiary);
        let on_surface = app.lock().unwrap().theme().get_color(ThemeColor::OnSurface);
        let background = app.lock().unwrap().theme().get_color(ThemeColor::Background);

        let mut shadow_offset_y = FloatProperty::from_value(5.0);
        let mut shadow_blur = FloatProperty::from_value(5.0);

        let size = self.width.clone();
        let app_clone = app.clone();
        flex_layout!(
            app.rectangle()
            .color(primary)
            .unwrap()
            .width(100)
            .height(100)
        )
            .unwrap()
            .background(app.rectangle().color(background))

/*        let mut children = vec![];
        children.push(
            app.rectangle()
                .color(tertiary)
                .radius(20.0)
                .unwrap()
                .width(&size)
                .height(&size)
                .on_click(
                {
                    let app = app.clone();
                    let size = size.clone();
                    move || {
                        if size.get() == Fixed(100.0) {
                            app.animation({
                                let size = size.clone();
                                move||{
                                    size.set_value(Fixed(200.0));
                                }}).duration(Duration::from_millis(500)).start();
                        }
                        else {
                            app.animation({
                                let size = size.clone();
                                move||{
                                    size.set_value(Fixed(100.0));
                                }}).duration(Duration::from_millis(500)).start();
                        }
                    }
                }
            )
        );
        for i in 0..5000 {
            children.push(
                app.rectangle()
                    .color(tertiary)
                    .radius(20.0)
                    .unwrap()
                    .width(&size)
                    .height(&size)
            );
        }

        FlexLayout::new(app.clone(),children).unwrap()*/

/*        flex_layout!(
                // Ripple::new().unwrap().width(100).height(100)
                
                // Button::new()
                //     .unwrap()
                //     .on_click(|| {
                //         println!("Hello World");
                //     })
                
                // Image::new()
                //     .source("https://www.rust-lang.org/logos/rust-logo-512x512.png")
                //     .item()
                //     .width(100)
                //     .height(100)
                
                // TextBlock::new()
                //     .text("Hello, world!")
                //     .color(on_surface)
                //     .unwrap()
                //
                // TextBlock::new()
                //     .text("Hello, world!")
                //     .color(on_surface)
                //     .unwrap()
                
                // Rectangle::new()
                //     .color(primary)
                //     .radius(50.0)
                //     .unwrap()
                //     .width(100)
                //     .height(100)
                //     .shadow_color(0x66000000)
                //     .shadow_offset_y(&shadow_offset_y)
                //     .shadow_blur(&shadow_blur)
                //     .on_cursor_entered({
                //         let shadow_offset_y = shadow_offset_y.clone();
                //         let shadow_blur = shadow_blur.clone();
                //         move || {
                //             Animation::new({
                //                 // let shadow_offset_y = shadow_offset_y.clone();
                //                 // let shadow_blur = shadow_blur.clone();
                //                 move||{
                //                     // shadow_offset_y.set_value(8.0);
                //                     // shadow_blur.set_value(8.0);
                //                 }}).duration(Duration::from_millis(500)).start();
                //     }})
                //     // .on_cursor_exited({
                //     //     let shadow_offset_y = shadow_offset_y.clone();
                //     //     let shadow_blur = shadow_blur.clone();
                //     //     move || {
                //     //         Animation::new({
                //     //             let shadow_offset_y = shadow_offset_y.clone();
                //     //             let shadow_blur = shadow_blur.clone();
                //     //             move||{
                //     //                 shadow_offset_y.set_value(5.0);
                //     //                 shadow_blur.set_value(5.0);
                //     //             }}).duration(Duration::from_millis(500)).start();
                //     // }})
                
                Rectangle::new()
                    .color(secondary)
                    .radius(60.0)
                    .unwrap()
                    .width(&self.width)
                    .height(150)
                    .on_click({
                let width = self.width.clone();
                move || {
                    if width.get() == Fixed(200.0) {
                        Animation::new({
                            let width = width.clone();
                            move||{
                                width.set_value(Fixed(300.0));
                            }}).duration(Duration::from_millis(500)).start();
                    }
                    else {
                        Animation::new({
                            let width = width.clone();
                            move||{
                                width.set_value(Fixed(200.0));
                            }}).duration(Duration::from_millis(500)).start();
                    }
                }
                    })  
                
                Rectangle::new()
                    .color(tertiary)
                    .radius(16.0)
                    .unwrap()
                    .width(56)
                    .height(56)
                    .margin_end(20)
                
                Rectangle::new()
                    .color(tertiary)
                    .radius(16.0)
                    .unwrap()
                    .width(56)
                    .height(56)
                
                Rectangle::new()
                    .color(tertiary)
                    .radius(40.0)
                    .unwrap()
                    .width(100)
                    .height(100)
                
                Rectangle::new()
                    .color(tertiary)
                    .radius(40.0)
                    .unwrap()
                    .width(100)
                    .height(100)
            
                // repeat!(Rectangle::new().color(tertiary).radius(40.0).unwrap().width(100).height(100); 100)
            ).unwrap()
            .width(Fill)
            .height(Fill)
            .on_click(move || {
                println!("Hello, world!");
                let layout_direction = app.layout_direction();
                app.set_layout_direction(
                    match layout_direction {
                        LayoutDirection::LeftToRight => LayoutDirection::RightToLeft,
                        LayoutDirection::RightToLeft => LayoutDirection::LeftToRight,
                    }
                );
                app.request_rebuild();
            })
*/    }
}
*/