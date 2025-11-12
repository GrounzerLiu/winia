use crate::shared::SharedSource;
use crate::ui::Item;

pub type SharedItem = SharedSource<Option<Item>>;

impl SharedItem {
    pub fn none() -> Self {
        SharedSource::new(None)
    }
}