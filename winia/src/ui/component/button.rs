/*use proc_macro::item;
use crate::ui::app::AppContext;
use crate::shared::{Children, SharedText};
use crate::ui::Item;

pub enum ButtonType {

}

#[derive(Clone)]
struct ButtonProperty {

}

#[item]
pub struct Button {
    item: Item,
    property: ButtonProperty,
}

impl Button {
    pub fn new(app_context: AppContext, text: impl Into<SharedText>) -> Self {
        let property = ButtonProperty {

        };
        let item = Item::new(app_context, Children::new());
        Self {
            item,
            property,
        }
    }
}

*/