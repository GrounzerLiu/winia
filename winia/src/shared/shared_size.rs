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
        let lhs = self.clone();
        let rhs = rhs.clone();
        SharedSize::from_dynamic(
            &[self.clone(), rhs.clone()],
            move || {
                let lhs = lhs.get();
                let rhs = rhs.get();
                match (lhs, rhs) {
                    (Size::Fixed(lhs), Size::Fixed(rhs)) => Size::Fixed(lhs + rhs),
                    (Size::Relative(lhs), Size::Relative(rhs)) => Size::Relative(lhs + rhs),
                    _ => { panic!("Addition of different size types") }
                }
            }
        )
    }
}