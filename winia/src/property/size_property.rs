use crate::core::RefClone;
use crate::property::{Gettable, Property};
use crate::ui::item::Size;

pub type SizeProperty = Property<Size>;

impl From<u32> for SizeProperty {
    fn from(size: u32) -> Self {
        SizeProperty::from_static(Size::Fixed(size as f32))
    }
}

impl From<f32> for SizeProperty {
    fn from(size: f32) -> Self {
        SizeProperty::from_static(Size::Fixed(size))
    }
}

impl From<f64> for SizeProperty {
    fn from(size: f64) -> Self {
        SizeProperty::from_static(Size::Fixed(size as f32))
    }
}

impl From<i32> for SizeProperty {
    fn from(size: i32) -> Self {
        SizeProperty::from_static(Size::Fixed(size as f32))
    }
}

impl From<&str> for SizeProperty {
    fn from(size: &str) -> Self {
        if let Some(size) = size.strip_suffix('%') {
            let size = size.parse::<f32>().unwrap() / 100.0;
            SizeProperty::from_static(Size::Relative(size))
        } else {
            let size = size.parse::<f32>().unwrap();
            SizeProperty::from_static(Size::Fixed(size))
        }
    }
}

impl std::ops::Add<&SizeProperty> for &SizeProperty {
    type Output = SizeProperty;

    fn add(self, rhs: &SizeProperty) -> Self::Output {
        let lhs_clone = self.ref_clone();
        let rhs_clone = rhs.ref_clone();
        SizeProperty::from_dynamic(move || {
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