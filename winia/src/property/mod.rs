mod size_property;

use winit::dpi::LogicalSize;
pub use size_property::*;
mod color_property;
pub use color_property::*;
mod bool_property;
pub use bool_property::*;
mod item_property;
pub use item_property::*;
// mod alignment_property;
mod text_property;
pub use text_property::*;
mod gravity_property;
pub use gravity_property::*;
mod property;
mod children;
mod num_property;
mod inner_position_property;
pub use inner_position_property::*;

pub use num_property::*;

pub use children::*;

pub use property::*;

impl Into<Property<String>> for &str {
    fn into(self) -> Property<String> {
        Property::from_static(self.to_string())
    }
}

impl Into<Property<winit::dpi::Size>> for (usize, usize){
    fn into(self) -> Property<crate::dpi::Size> {
        Property::from_static(crate::dpi::Size::new(LogicalSize::new(self.0 as f64, self.1 as f64)))
    }
}