use std::rc::Rc;
use std::sync::Mutex;
use crate::shared::Shared;
use crate::ui::Item;

pub type ItemObject= Option<Rc<Mutex<Item>>>;

pub type SharedItem = Shared<Option<Item>>;

impl SharedItem {
    pub fn none() -> Self{
        Self::from_static(None)
    }
}

impl From<Item> for SharedItem {
    fn from(item: Item) -> Self {
        Self::from_static(Some(item))
    }
}