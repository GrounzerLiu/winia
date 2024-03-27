use std::rc::Rc;
use std::sync::Mutex;
use skia_safe::Color;

use crate::property::SharedProperty;
use crate::ui::Item;
use crate::widget::Rectangle;

pub type ItemObject= Option<Rc<Mutex<Item>>>;

pub type ItemProperty = SharedProperty<Option<Item>>;

impl ItemProperty{
    pub fn none() -> Self{
        Self::from_value(None)
    }
}

impl From<Item> for ItemProperty{
    fn from(item: Item) -> Self {
        Self::from(item)
    }
}

impl From<Rectangle> for ItemProperty{
    fn from(rectangle: Rectangle) -> Self {
        let item = rectangle.item();
        Self::from(Some(item))
    }
}