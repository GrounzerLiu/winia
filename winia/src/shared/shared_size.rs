use crate::core::RefClone;
use crate::shared::{Gettable, Shared};
use crate::ui::item::Size;

pub type SharedSize = Shared<Size>;

impl From<u32> for SharedSize {
    fn from(size: u32) -> Self {
        SharedSize::from_static(Size::Fixed(size as f32))
    }
}

impl From<f32> for SharedSize {
    fn from(size: f32) -> Self {
        SharedSize::from_static(Size::Fixed(size))
    }
}

impl From<f64> for SharedSize {
    fn from(size: f64) -> Self {
        SharedSize::from_static(Size::Fixed(size as f32))
    }
}

impl From<i32> for SharedSize {
    fn from(size: i32) -> Self {
        SharedSize::from_static(Size::Fixed(size as f32))
    }
}

impl From<&str> for SharedSize {
    fn from(size: &str) -> Self {
        if let Some(size) = size.strip_suffix('%') {
            let size = size.parse::<f32>().unwrap() / 100.0;
            SharedSize::from_static(Size::Relative(size))
        } else {
            let size = size.parse::<f32>().unwrap();
            SharedSize::from_static(Size::Fixed(size))
        }
    }
}

impl std::ops::Add<&SharedSize> for &SharedSize {
    type Output = SharedSize;

    fn add(self, rhs: &SharedSize) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        SharedSize::from_dynamic(move || {
            if let Size::Fixed(lhs) = lhs_clone.get() {
                if let Size::Fixed(rhs) = rhs_clone.get() {
                    Size::Fixed(lhs + rhs)
                } else {
                    panic!("Cannot add fixed size with non-fixed size.")
                }
            } else if let Size::Relative(lhs) = lhs_clone.get() {
                if let Size::Relative(rhs) = rhs_clone.get() {
                    Size::Relative(lhs + rhs)
                } else {
                    panic!("Cannot add relative size with non-relative size.")
                }
            } else {
                panic!("\"Add\" operation is not supported for \"Compact\" or \"Expanded\" size.")
            }
        })
    }
}