mod display_parameter;
mod inner_position;
mod item;
mod logical_x;
mod scroller;
mod size;

pub use item::*;

pub use display_parameter::*;
pub use inner_position::*;
pub use logical_x::*;
pub use scroller::*;
pub use size::*;

#[macro_export]
macro_rules! impl_property_redraw {
    ($struct_name:ident, $property_name:ident, $property_type:ty) => {
        impl $struct_name {
            pub fn $property_name(self, $property_name: impl Into<$property_type>) -> Self {
                let id = self.item.data().get_id();
                let mut property = self.property.value();
                property.$property_name.remove_observer(id);
                let event_loop_proxy = self.item.data().get_app_context().event_loop_proxy();
                property.$property_name = $property_name.into();
                property.$property_name.add_observer(
                    id,
                    Box::new(move || {
                        event_loop_proxy.request_redraw();
                    }),
                );
                drop(property);
                self
            }
        }
    };
}

#[macro_export]
macro_rules! impl_property_layout {
    ($struct_name:ident, $property_name:ident, $property_type:ty) => {
        impl $struct_name {
            pub fn $property_name(self, $property_name: impl Into<$property_type>) -> Self {
                use $crate::shared::Observable;
                let id = self.item.data().get_id();
                let mut property = self.property.value();
                property.$property_name.remove_observer(id);
                let event_loop_proxy = self.item.data().get_app_context().event_loop_proxy();
                property.$property_name = $property_name.into();
                property.$property_name.add_observer(
                    id,
                    Box::new(move || {
                        event_loop_proxy.request_layout();
                    }),
                );
                drop(property);
                self
            }
        }
    };
}
