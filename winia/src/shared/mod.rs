mod shared_size;

use winit::dpi::LogicalSize;
pub use shared_size::*;
mod shared_color;
pub use shared_color::*;
mod shared_bool;
pub use shared_bool::*;
mod shared_item;
pub use shared_item::*;
// mod alignment_property;
mod shared_text;
pub use shared_text::*;
mod shared_alignment;
pub use shared_alignment::*;
mod shared;
mod children;
mod shared_num;
mod shared_inner_position;
mod shared_un_send;
mod shared_drawable;

pub use shared_inner_position::*;

pub use shared_num::*;

pub use children::*;

pub use shared::*;

pub use shared_un_send::*;

pub use shared_drawable::*;

impl Into<Shared<String>> for &str {
    fn into(self) -> Shared<String> {
        Shared::from_static(self.to_string())
    }
}

impl Into<Shared<winit::dpi::Size>> for (usize, usize){
    fn into(self) -> Shared<crate::dpi::Size> {
        Shared::from_static(crate::dpi::Size::new(LogicalSize::new(self.0 as f64, self.1 as f64)))
    }
}
