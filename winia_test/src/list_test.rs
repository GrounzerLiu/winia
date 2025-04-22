use winia::shared::Shared;
use winia::skia_safe::Color;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{RippleExt, TextExt};
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::ListExt;

pub fn list_test(w: &WindowContext) -> Item {
    let mut strings = Vec::new();
    for i in 0..100 {
        strings.push(format!("Item {}", i));
    }
    let shared_strings = Shared::from_static(strings);
    w.list(
        shared_strings,
        |w, items, i| {
            let items_clone = items.clone();
            let items = items.lock();
            let item = items[i].clone();
            w.text(item.clone()).color(Color::WHITE)
                .editable(false)
                .item().size(Size::Fill, 50)
                .on_click(move |_|{
                    println!("Clicked on item {}", item);
                    {
                        let mut items = items_clone.lock();
                        items.remove(i);
                    }
                    items_clone.notify();
                })
                // .background(w.ripple().item())
        }
    ).item()
}