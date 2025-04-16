use crate::shared::Children;
use crate::ui::app::WindowContext;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};
use crate::ui::Item;

pub trait RowExt {
    fn row(&self, children: impl Into<Children>) -> Item;
}

impl RowExt for WindowContext {
    fn row(&self, children: impl Into<Children>) -> Item {
        Flex::new(self, children)
            .direction(FlexDirection::Horizontal)
            .wrap(FlexWrap::NoWrap)
            .item()
    }
}
