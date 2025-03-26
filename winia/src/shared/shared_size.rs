use std::ops::{Add, Div, Mul, Rem, Sub};
use crate::shared::{Gettable, Shared};
use crate::ui::Unit;
use crate::ui::item::Size;

pub type SharedSize = Shared<Size>;

// macro_rules! impl_from {
//     ($($t:ty),*) => {
//         $(
//             impl From<$t> for SharedSize {
//                 fn from(size: $t) -> Self {
//                     SharedSize::from_static(Size::Fixed(size.dp()))
//                 }
//             }
//         )*
//     };
// }

// impl_from!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

impl From<Unit> for SharedSize {
    fn from(unit: Unit) -> Self {
        SharedSize::from_static(Size::Fixed(unit))
    }
}

impl From<&str> for SharedSize {
    fn from(size: &str) -> Self {
        SharedSize::from_static(size.into())
    }
}

/*impl Add<&SharedSize> for &SharedSize {
    type Output = SharedSize;

    fn add(self, rhs: &SharedSize) -> Self::Output {
        let lhs = self.clone();
        let rhs = rhs.clone();
        SharedSize::from_dynamic(&[self.clone(), rhs.clone()], move || {
            let lhs = lhs.get();
            let rhs = rhs.get();
            match (lhs, rhs) {
                (Size::Fixed(lhs), Size::Fixed(rhs)) => Size::Fixed(lhs + rhs),
                (Size::Relative(lhs), Size::Relative(rhs)) => Size::Relative(lhs + rhs),
                _ => {
                    panic!("Addition of different size types")
                }
            }
        })
    }
}*/

macro_rules! impl_op {
    ($op_trait:ident, $op_method:ident, $msg:expr) => {
        impl $op_trait<&SharedSize> for &SharedSize {
            type Output = SharedSize;

            fn $op_method(self, rhs: &SharedSize) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedSize::from_dynamic(&[self.clone(), rhs.clone()], move || {
                    let lhs = lhs.get();
                    let rhs = rhs.get();
                    match (lhs, rhs) {
                        (Size::Fixed(lhs), Size::Fixed(rhs)) => Size::Fixed(lhs.$op_method(rhs)),
                        (Size::Relative(lhs), Size::Relative(rhs)) => Size::Relative(lhs.$op_method(rhs)),
                        _ => {
                            panic!($msg)
                        }
                    }
                })
            }
        }

        impl $op_trait<SharedSize> for &SharedSize {
            type Output = SharedSize;

            fn $op_method(self, rhs: SharedSize) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedSize::from_dynamic(&[self.clone(), rhs.clone()], move || {
                    let lhs = lhs.get();
                    let rhs = rhs.get();
                    match (lhs, rhs) {
                        (Size::Fixed(lhs), Size::Fixed(rhs)) => Size::Fixed(lhs.$op_method(rhs)),
                        (Size::Relative(lhs), Size::Relative(rhs)) => Size::Relative(lhs.$op_method(rhs)),
                        _ => {
                            panic!($msg)
                        }
                    }
                })
            }
        }

        impl $op_trait<&SharedSize> for SharedSize {
            type Output = SharedSize;

            fn $op_method(self, rhs: &SharedSize) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedSize::from_dynamic(&[self.clone(), rhs.clone()], move || {
                    let lhs = lhs.get();
                    let rhs = rhs.get();
                    match (lhs, rhs) {
                        (Size::Fixed(lhs), Size::Fixed(rhs)) => Size::Fixed(lhs.$op_method(rhs)),
                        (Size::Relative(lhs), Size::Relative(rhs)) => Size::Relative(lhs.$op_method(rhs)),
                        _ => {
                            panic!($msg)
                        }
                    }
                })
            }
        }

        impl $op_trait<SharedSize> for SharedSize {
            type Output = SharedSize;

            fn $op_method(self, rhs: SharedSize) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedSize::from_dynamic(&[self.clone(), rhs.clone()], move || {
                    let lhs = lhs.get();
                    let rhs = rhs.get();
                    match (lhs, rhs) {
                        (Size::Fixed(lhs), Size::Fixed(rhs)) => Size::Fixed(lhs.$op_method(rhs)),
                        (Size::Relative(lhs), Size::Relative(rhs)) => Size::Relative(lhs.$op_method(rhs)),
                        _ => {
                            panic!($msg)
                        }
                    }
                })
            }
        }
    };
}

impl_op!(Add, add, "Addition of different size types");
impl_op!(Sub, sub, "Subtraction of different size types");
impl_op!(Mul, mul, "Multiplication of different size types");
impl_op!(Div, div, "Division of different size types");
impl_op!(Rem, rem, "Remainder of different size types");


