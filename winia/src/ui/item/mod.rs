mod display_parameter;
mod logical_x;
mod item;
mod size;
mod inner_position;

pub use item::*;

pub use display_parameter::*;
pub use logical_x::*;
pub use size::*;
pub use inner_position::*;


#[macro_export]
macro_rules! impl_property_redraw {
    ($struct_name:ident, $property_name:ident, $property_type:ty) => {
        impl $struct_name {
            pub fn $property_name(self, $property_name: impl Into<$property_type>) -> Self {
                let id = self.item.data().get_id();
                let mut property = self.property.value();
                property.$property_name.remove_observer(id);
                let app_context = self.item.data().get_app_context();
                property.$property_name = $property_name.into();
                property.$property_name.add_observer(
                    id,
                    Box::new(move || {
                        app_context.request_redraw();
                    }),
                );
                drop(property);
                self
            }
        }
    }
}

#[macro_export]
macro_rules! impl_property_layout {
    ($struct_name:ident, $property_name:ident, $property_type:ty) => {
        impl $struct_name {
            pub fn $property_name(self, $property_name: impl Into<$property_type>) -> Self {
                let id = self.item.data().get_id();
                let mut property = self.property.value();
                property.$property_name.remove_observer(id);
                let app_context = self.item.data().get_app_context();
                property.$property_name = $property_name.into();
                property.$property_name.add_observer(
                    id,
                    Box::new(move || {
                        app_context.request_layout();
                    }),
                );
                drop(property);
                self
            }
        }
    }
}