use std::rc::Rc;
use std::sync::Mutex;
use crate::property::Property;
use crate::ui::Item;

pub type ItemObject= Option<Rc<Mutex<Item>>>;

pub type ItemProperty = Property<Option<Item>>;

impl ItemProperty{
    pub fn none() -> Self{
        Self::from_static(None)
    }
}

impl From<Item> for ItemProperty{
    fn from(item: Item) -> Self {
        Self::from_static(Some(item))
    }
}