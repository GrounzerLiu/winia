use std::ops::{Add, Deref, DerefMut};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::shared::{SharedDerived, SharedSource};
use crate::ui::Item;
use crate::ui::item::ItemUpdater;

// pub type Children = SharedSource<Vec<Item>>;
#[derive(Clone)]
pub struct Children {
    parent_updater: Option<Arc<Mutex<ItemUpdater>>>,
    inner: SharedSource<Vec<Item>>,
}

impl Children {
    pub fn new() -> Self {
        Self {
            parent_updater: None,
            inner: SharedSource::new(vec![]),
        }
    }

    pub(crate) fn set_parent_updater(&mut self, parent_updater: &Arc<Mutex<ItemUpdater>>) {
        self.parent_updater = Some(parent_updater.clone());
        for item in self.inner.lock().iter_mut() {
            item.data().item_updater.lock().parent = Some(Arc::downgrade(parent_updater));
        }
    }

    pub fn add_item(&mut self, item: Item) {
        if let Some(parent_updater) = &self.parent_updater {
            item.data().item_updater.lock().parent = Some(Arc::downgrade(parent_updater));
        }
        self.write(|children|{
            if children.iter().any(|i| i.id() == item.id()) {
                return;
            }
            children.push(item);
        })
    }

    pub fn remove_by_id(&mut self, id: u32) {
        self.write(|children|{
            // children.retain(|i| i.id() != id);
            children.extract_if(.., |i| i.id() == id).for_each(|item| {
                item.data().item_updater.lock().parent = None;
                item.data().is_mounted = false;
                item.data().on_unmounted();
            })
        })
    }
}

impl Add<Item> for Children {
    type Output = Children;

    fn add(mut self, rhs: Item) -> Self::Output {
        self.add_item(rhs);
        self
    }
}

impl Deref for Children {
    type Target = SharedSource<Vec<Item>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Children {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<SharedSource<Vec<Item>>> for Children {
    fn from(value: SharedSource<Vec<Item>>) -> Self {
        Self {
            parent_updater: None,
            inner: value,
        }
    }
}

impl From<Vec<Item>> for Children {
    fn from(value: Vec<Item>) -> Self {
        Self {
            parent_updater: None,
            inner: SharedSource::new(value),
        }
    }
}

impl From<Item> for Children {
    fn from(value: Item) -> Self {
        let mut children = Children::new();
        children.add_item(value);
        children
    }
}