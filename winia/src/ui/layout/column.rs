use crate::shared::Children;
use crate::ui::app::AppContext;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};
use crate::ui::Item;

pub trait ColumnExt {
    fn column(&self, children: Children) -> Item;
}

impl ColumnExt for AppContext {
    fn column(&self, children: Children) -> Item {
        Flex::new(self.clone(), children)
            .direction(FlexDirection::Vertical)
            .wrap(FlexWrap::NoWrap)
            .item()
    }
}
