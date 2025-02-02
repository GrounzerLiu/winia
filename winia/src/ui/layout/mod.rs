mod stack;
mod flex;
mod row;
mod column;
mod relative;

pub use stack::*;
pub use flex::*;
pub use row::*;
pub use column::*;


use crate::shared::{Observable, SharedAlignment};
use crate::ui::Item;
use crate::ui::item::CustomProperty;

pub trait AlignSelf {
    fn align_self(self, align_self: impl Into<SharedAlignment>) -> Self;
    fn get_align_self(&self) -> Option<SharedAlignment>;
}

impl AlignSelf for Item {
    fn align_self(mut self, align_self: impl Into<SharedAlignment>) -> Self {
        let id = self.get_id();
        if let Some(CustomProperty::Any(align_self)) = self.get_custom_property_mut("align_self") {
            if let Some(align_self) = align_self.downcast_mut::<SharedAlignment>() {
                align_self.remove_observer(id);
            }
        }

        let app_context = self.get_app_context();
        let mut align_self = align_self.into();
        align_self.add_observer(
            id,
            Box::new(move || {
                app_context.request_layout();
            }),
        );
        self.custom_property("align_self", CustomProperty::Any(Box::new(align_self)))
    }

    fn get_align_self(&self) -> Option<SharedAlignment> {
        if let Some(CustomProperty::Any(align_self)) = self.get_custom_property("align_self") {
            if let Some(align_self) = align_self.downcast_ref::<SharedAlignment>() {
                return Some(align_self.clone());
            }
        }
        None
    }
}