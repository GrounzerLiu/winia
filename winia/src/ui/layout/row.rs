use crate::shared::Children;
use crate::ui::app::AppContext;
use crate::ui::Item;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};

pub trait RowExt {
    fn row(&self, children: Children) -> Item;
}

impl RowExt for AppContext {
    fn row(&self, children: Children) -> Item {
        Flex::new(self.clone(), children)
            .direction(FlexDirection::Horizontal)
            .wrap(FlexWrap::NoWrap)
            .item()
    }
}