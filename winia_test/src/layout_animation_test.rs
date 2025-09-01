use std::ops::Add;
use clonelet::clone;
use rand::Rng;
use winia::shared::Children;
use winia::skia_safe::Color;
use winia::ui::app::WindowContext;
use winia::ui::component::{ButtonExt, RectangleExt};
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::{ColumnExt, FlexWrap, RowExt};

pub fn layout_animation_test(w: &WindowContext) -> Item {
    let items = 
        w.rectangle(Color::RED).item().size(100,100)
        + w.rectangle(Color::GREEN).item().size(100,100)
        + w.rectangle(Color::YELLOW).item().size(100,100);
    w.column(
        w.row(
            w.button("Add").item().on_click({
                clone!(w, mut items);
                move |_| {
                    let mut rng = rand::rng();
                    let random_color = Color::from_rgb(
                        rng.random_range(0..= 255),
                        rng.random_range(0..= 255),
                        rng.random_range(0..= 255)
                    );
                    items.insert_with_animation(
                        1,
                        w.rectangle(random_color).item()
                            .size(
                                rng.random_range(50..= 200),
                                rng.random_range(50..= 200)
                            )
                    );
                }
            })
            + w.button("Remove").item().on_click({
                clone!(w, mut items);
                let mut remove_items: Vec<usize> = Vec::new();
                move |_| {
                    if !items.is_empty() {
                        // let id= items.lock().get(1).map(|item| item.data().get_id());
                        // if let Some(id) = id {
                        //     items.remove_with_animation(id);
                        // }
                        for i in 1..items.len() {
                            let id = items.lock().get(i).map(|item| item.data().get_id());
                            if let Some(id) = id {
                                if !remove_items.contains(&id) {
                                    items.remove_with_animation(id);
                                    remove_items.push(id);
                                    break; // Remove only one item at a time
                                }
                            }
                        }
                    }
                }
            })
        ).item()
        + w.row(&items).wrap(FlexWrap::Wrap).item().width(Size::Fill)
    ).item()
}