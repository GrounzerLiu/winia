mod column;
mod flex;
mod relative;
mod row;
mod scroll_area;
mod stack;
mod list;

pub use column::*;
pub use flex::*;
pub use row::*;
pub use stack::*;
// pub use relative::*;
pub use scroll_area::*;
pub use list::*;

use crate::shared::{Observable, SharedAlignment};
use crate::ui::item::{CustomProperty, ItemData};
use crate::ui::Item;

pub trait AlignSelf {
    fn align_self(self, align_self: impl Into<SharedAlignment>) -> Self;
}

pub trait GetAlignSelf {
    fn get_align_self(&self) -> Option<SharedAlignment>;
}

impl GetAlignSelf for ItemData {
    fn get_align_self(&self) -> Option<SharedAlignment> {
        if let Some(CustomProperty::Any(align_self)) = self.get_custom_property("align_self") {
            if let Some(align_self) = align_self.downcast_ref::<SharedAlignment>() {
                return Some(align_self.clone());
            }
        }
        None
    }
}

impl AlignSelf for Item {
    fn align_self(self, align_self: impl Into<SharedAlignment>) -> Self {
        let id = self.data().get_id();
        if let Some(CustomProperty::Any(align_self)) =
            self.data().get_custom_property_mut("align_self")
        {
            if let Some(align_self) = align_self.downcast_mut::<SharedAlignment>() {
                align_self.remove_observer(id);
            }
        }

        let event_loop_proxy = self.data().get_window_context().event_loop_proxy().clone();
        let mut align_self = align_self.into();
        align_self.add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_layout();
            }),
        );
        self.data()
            .custom_property("align_self", CustomProperty::Any(Box::new(align_self)));
        self
    }
}
