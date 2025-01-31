use crate::ui::app::AppContext;
use proc_macro::Item;
use crate::shared::Children;
use crate::ui::Item;
use crate::ui::item::ItemEvent;

#[derive(Clone)]
struct ButtonProperty {

}

#[derive(Item)]
pub struct Button {
    item: Item,
    property: ButtonProperty,
}

impl Button {
    pub fn new(app_context: AppContext) -> Self {
        let property = ButtonProperty {

        };
        let item = Item::new(app_context, Children::new(), ItemEvent::new());
        Self {
            item,
            property,
        }
    }
}

