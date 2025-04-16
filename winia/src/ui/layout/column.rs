use crate::shared::Children;
use crate::ui::app::WindowContext;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};
use crate::ui::Item;

pub trait ColumnExt {
    fn column(&self, children: impl Into<Children>) -> Item;
}

impl ColumnExt for WindowContext {
    fn column(&self, children: impl Into<Children>) -> Item {
        Flex::new(self, children)
            .direction(FlexDirection::Vertical)
            .wrap(FlexWrap::NoWrap)
            .item()
    }
}
