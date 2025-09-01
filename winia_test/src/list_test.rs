use clonelet::clone;
use winia::collection::WVec;
use winia::core::next_id;
use winia::shared::{Gettable, Shared};
use winia::ui::app::WindowContext;
use winia::ui::component::{ButtonExt, TextExt};
use winia::ui::item::Size;
use winia::ui::layout::{ColumnExt, ListExt, RowExt};
use winia::ui::Item;

pub fn list_test(w: &WindowContext) -> Item {
    let mut strings = WVec::new();
    for i in 0..100 {
        strings.push(format!("Item {}", i));
    }
    let shared_strings = Shared::from_static(strings);
    w.column(
        w.button("Add").item()
            .on_click({
                clone!(shared_strings);
                move |_| {
                    shared_strings.insert(0, format!("Item {}", next_id()));
                }
            })
        + w.list(
            shared_strings,
            |w, items, i| {
                let items_clone = items.clone();
                let items = items.lock();
                let item = items.get(i.get()).unwrap().clone();
                w.text(item.clone())
                    .editable(false)
                    .item().size(Size::Fill, 50)
                    .on_click(move |_|{
                        // println!("Clicked on item {}, value: {}", i.get(), item);
                        {
                            let mut items = items_clone.lock();
                            items.remove(i.get());
                        }
                        items_clone.notify();
                    })
                    // .background(w.ripple().item())
            }
        ).item()
    ).item()
}