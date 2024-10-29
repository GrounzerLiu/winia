use crate::core::RefClone;
use crate::property::Children;
use crate::ui::app::AppContext;
use crate::ui::Item;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};

pub trait ColumnExt {
    fn column(&self, children: Children) -> Item;
}

impl ColumnExt for AppContext {
    fn column(&self, children: Children) -> Item {
        Flex::new(self.ref_clone(), children)
            .direction(FlexDirection::Vertical)
            .wrap(FlexWrap::NoWrap)
            .item()
    }
}