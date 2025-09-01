use crate::shared::Children;
use crate::ui::app::WindowContext;
use crate::ui::layout::{Flex, FlexDirection, FlexWrap};

pub trait RowExt {
    fn row(&self, children: impl Into<Children>) -> Flex;
}

impl RowExt for WindowContext {
    fn row(&self, children: impl Into<Children>) -> Flex {
        Flex::new(self, children)
            .flex_direction(FlexDirection::Horizontal)
            .wrap(FlexWrap::NoWrap)
    }
}
