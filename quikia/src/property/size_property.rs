use crate::property::{Gettable, SharedProperty};

#[derive(Debug)]
pub enum Size{
    Default,
    Fill,
    Fixed(f32),
    Relative(f32),
}

impl Default for Size{
    fn default() -> Self {
        Size::Default
    }
}

impl PartialEq for Size{
    fn eq(&self, other: &Self) -> bool {
        match self{
            Size::Default => {
                match other{
                    Size::Default => true,
                    _ => false,
                }
            }
            Size::Fill => {
                match other{
                    Size::Fill => true,
                    _ => false,
                }
            }
            Size::Fixed(f) => {
                match other{
                    Size::Fixed(f2) => f == f2,
                    _ => false,
                }
            }
            Size::Relative(f) => {
                match other{
                    Size::Relative(f2) => f == f2,
                    _ => false,
                }
            }
        }
    }
}

impl Clone for Size{
    fn clone(&self) -> Self {
        match self{
            Size::Default => Size::Default,
            Size::Fill => Size::Fill,
            Size::Fixed(f) => Size::Fixed(*f),
            Size::Relative(f) => Size::Relative(*f),
        }
    }
}

impl Into<Size> for u32{
    fn into(self) -> Size{
        Size::Fixed(self as f32)
    }
}

impl Into<Size> for f32{
    fn into(self) -> Size{
        if self < 0.0{
            panic!("Size cannot be negative.")
        }
        Size::Fixed(self)
    }
}

impl Into<Size> for f64{
    fn into(self) -> Size{
        if self < 0.0{
            panic!("Size cannot be negative.")
        }
        Size::Fixed(self as f32)
    }
}

impl Into<Size> for i32{
    fn into(self) -> Size{
        if self < 0{
            panic!("Size cannot be negative.")
        }
        Size::Fixed(self as f32)
    }
}

impl Into<Size> for &str{
    fn into(self) -> Size{
        if self.ends_with("%") {
            let size = self[..self.len() - 1].parse::<f32>().unwrap() / 100.0;
            Size::Relative(size)
        }
        else{
            let size = self.parse::<f32>().unwrap();
            Size::Fixed(size)
        }
    }
}

impl Into<Size> for &Size{
    fn into(self) -> Size{
        self.clone()
    }
}

pub type SizeProperty = SharedProperty<Size>;

impl From<u32> for SizeProperty {
    fn from(size: u32) -> Self {
        SizeProperty::from_value(Size::Fixed(size as f32))
    }
}

impl From<f32> for SizeProperty {
    fn from(size: f32) -> Self {
        SizeProperty::from_value(Size::Fixed(size))
    }
}

impl From<f64> for SizeProperty {
    fn from(size: f64) -> Self {
        SizeProperty::from_value(Size::Fixed(size as f32))
    }
}

impl From<i32> for SizeProperty {
    fn from(size: i32) -> Self {
        SizeProperty::from_value(Size::Fixed(size as f32))
    }
}

impl From<&str> for SizeProperty {
    fn from(size: &str) -> Self {
        if size.ends_with("%") {
            let size = size[..size.len() - 1].parse::<f32>().unwrap() / 100.0;
            SizeProperty::from_value(Size::Relative(size))
        }
        else{
            let size = size.parse::<f32>().unwrap();
            SizeProperty::from_value(Size::Fixed(size))
        }
    }
}

impl From<&SizeProperty> for SizeProperty {
    fn from(size: &SizeProperty) -> Self {
        SizeProperty::clone(size)
    }
}

impl std::ops::Add<&SizeProperty> for &SizeProperty {
    type Output = SizeProperty;

    fn add(self, rhs: &SizeProperty) -> Self::Output {
        let lhs = self.clone();
        let rhs_clone = rhs.clone();
        let output = SizeProperty::from_generator(Box::new(move || {
            let left_value = lhs.get();
            let right_value = rhs_clone.get();
            if let Size::Fixed(left_value) = left_value{
                if let Size::Fixed(right_value) = right_value{
                    Size::Fixed(left_value+right_value)
                }
                else{
                    panic!("Cannot add a fixed size to a non-fixed size.")
                }
            }
            else{
                panic!("Cannot add a non-fixed size to a fixed size.")
            }
        }
        ));
        output.observe(self);
        output.observe(rhs);
        output
    }
}