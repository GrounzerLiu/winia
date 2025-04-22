use crate::shared::Children;
use crate::ui::app::WindowContext;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};
use crate::ui::Item;

pub trait ColumnExt {
    fn column(&self, children: impl Into<Children>) -> Flex;
}

impl ColumnExt for WindowContext {
    fn column(&self, children: impl Into<Children>) -> Flex {
        Flex::new(self, children)
            .flex_direction(FlexDirection::Vertical)
            .wrap(FlexWrap::NoWrap)
    }
}
