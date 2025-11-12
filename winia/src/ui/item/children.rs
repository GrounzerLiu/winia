use std::ops::Add;
use crate::shared::{SharedDerived, SharedSource};
use crate::ui::Item;

pub type Children = SharedSource<Vec<Item>>;
pub type ChildrenDerived = SharedDerived<Vec<Item>>;
impl Children {
    pub fn empty() -> Self {
        SharedSource::new(Vec::new())
    }

    pub fn add_item(&mut self, item: Item) {
        let mut items = self.lock();
        if items.iter().any(|i| i.id() == item.id()) {
            return;
        }
        items.push(item);
    }

    pub fn remove_by_id(&mut self, id: u32) {
        let mut items = self.lock();
        items.retain(|i| i.id() != id);
    }
}

impl Add<Item> for Children {
    type Output = Children;

    fn add(mut self, rhs: Item) -> Self::Output {
        self.add_item(rhs);
        self
    }
}

impl From<Item> for ChildrenDerived {
    fn from(item: Item) -> Self {
        Children::new(vec![item]).into()
    }
}