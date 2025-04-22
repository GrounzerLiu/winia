use clonelet::clone;
use winia::shared::{Gettable, Shared, SharedText};
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::{RectangleExt, TextExt};
use winia::ui::item::Alignment;
use winia::ui::layout::ColumnExt;

pub fn focus_test(w: &WindowContext) -> Item {
    let red_focused = Shared::from_static(false);
    let green_focused = Shared::from_static(false);
    let blue_focused = Shared::from_static(false);
    w.column(
        w.text("Text 1")
            .item()
            .size(100, 100)
            .focusable(true)
            .focused(&red_focused)
            .focused_when_clicked(true)
            // .foreground(
            //     w.text(SharedText::from_dynamic(
            //         [red_focused.as_ref().into()].into(),
            //         {
            //             let red_focused = red_focused.clone();
            //             move ||{
            //                 red_focused.lock().to_string().into()
            //             }
            //         }
            //     )).item().align_content(Alignment::Center)
            // )
            .on_click({
                clone!(red_focused, green_focused, blue_focused);
                move |_| {
                    println!("Red {:?}", red_focused.get());
                    println!("Green {:?}", green_focused.get());
                    println!("Blue {:?}", blue_focused.get());
                }
            })
        + w.text("Text 2")
            .item()
            .size(100, 100)
            .focusable(true)
            .focused(&green_focused)
            .focused_when_clicked(true)
            // .foreground(
            //     w.text(SharedText::from_dynamic(
            //         [green_focused.as_ref().into()].into(),
            //         {
            //             let green_focused = green_focused.clone();
            //             move ||{
            //                 green_focused.lock().to_string().into()
            //             }
            //         }
            //     )).item().align_content(Alignment::Center)
            // )
            .on_click({
                clone!(red_focused, green_focused, blue_focused);
                move |_| {
                    println!("Red {:?}", red_focused.get());
                    println!("Green {:?}", green_focused.get());
                    println!("Blue {:?}", blue_focused.get());
                }
            })
        + w.text("Text 3")
            .item()
            .size(100, 100)
            .focusable(true)
            .focused(&blue_focused)
            .focused_when_clicked(true)
            // .foreground(
            //     w.text(SharedText::from_dynamic(
            //         [blue_focused.as_ref().into()].into(),
            //         {
            //             let blue_focused = blue_focused.clone();
            //             move ||{
            //                 blue_focused.lock().to_string().into()
            //             }
            //         }
            //     )).item().align_content(Alignment::Center)
            // )
            .on_click({
                clone!(red_focused, green_focused, blue_focused);
                move |_| {
                    println!("Red {:?}", red_focused.get());
                    println!("Green {:?}", green_focused.get());
                    println!("Blue {:?}", blue_focused.get());
                }
            })
    ).item()
}
