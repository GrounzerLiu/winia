use std::ops::{AddAssign, Deref, DerefMut, Neg, SubAssign};
use crate::ui::app::AppContext;

#[derive(Clone, Debug, PartialEq, Copy)]
pub enum Unit {
    Dp(f32),
    Px(f32),
    Sp(f32),
}

impl Unit {
    pub fn to_dp(&self, context: impl AsRef<AppContext>) -> f32 {
        let context = context.as_ref();
        match self {
            Unit::Dp(value) => *value,
            Unit::Px(value) => *value / context.scale_factor(),
            Unit::Sp(value) => (*value / context.scale_factor()) * context.font_scale(),
        }
    }
}

impl AsRef<f32> for Unit {
    fn as_ref(&self) -> &f32 {
        match self {
            Unit::Dp(value) => value,
            Unit::Px(value) => value,
            Unit::Sp(value) => value,
        }
    }
}

impl AsMut<f32> for Unit {
    fn as_mut(&mut self) -> &mut f32 {
        match self {
            Unit::Dp(value) => value,
            Unit::Px(value) => value,
            Unit::Sp(value) => value,
        }
    }
}

impl From<Unit> for f32 {
    fn from(unit: Unit) -> Self {
        *unit.as_ref()
    }
}

impl From<f32> for Unit {
    fn from(value: f32) -> Self {
        Unit::Dp(value)
    }
}

impl Deref for Unit {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl DerefMut for Unit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Unit::Dp(value) => value,
            Unit::Px(value) => value,
            Unit::Sp(value) => value,
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Unit::Dp(0.0)
    }
}

pub trait Dp {
    fn dp(&self) -> Unit;
}

pub trait Px {
    fn px(&self) -> Unit;
}

pub trait Sp {
    fn sp(&self) -> Unit;
}

macro_rules! impl_unit {
    ($($t:ty),*) => {
        $(
            impl Dp for $t {
                fn dp(&self) -> Unit {
                    Unit::Dp(*self as f32)
                }
            }

            impl Px for $t {
                fn px(&self) -> Unit {
                    Unit::Px(*self as f32)
                }
            }

            impl Sp for $t {
                fn sp(&self) -> Unit {
                    Unit::Sp(*self as f32)
                }
            }
        )*
    };
}

impl_unit!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);

macro_rules! impl_unit_op_num {
    ($op_trait:ident, $op_method:ident, $num:ty) => {
        impl $op_trait<Unit> for $num {
            type Output = Unit;

            fn $op_method(self, rhs: Unit) -> Self::Output {
                Unit::Dp((self as f32).$op_method(rhs.as_ref()))
            }
        }
        impl $op_trait<$num> for Unit {
            type Output = Unit;

            fn $op_method(self, rhs: $num) -> Self::Output {
                Unit::Dp(self.as_ref().$op_method(rhs as f32))
            }
        }
        
        impl $op_trait<Unit> for &$num {
            type Output = Unit;

            fn $op_method(self, rhs: Unit) -> Self::Output {
                Unit::Dp((*self as f32).$op_method(rhs.as_ref()))
            }
        }
        
        impl $op_trait<&$num> for Unit {
            type Output = Unit;

            fn $op_method(self, rhs: &$num) -> Self::Output {
                Unit::Dp(self.as_ref().$op_method(*rhs as f32))
            }
        }
        
        impl $op_trait<&Unit> for $num {
            type Output = Unit;

            fn $op_method(self, rhs: &Unit) -> Self::Output {
                Unit::Dp((self as f32).$op_method(rhs.as_ref()))
            }
        }
        
        impl $op_trait<$num> for &Unit {
            type Output = Unit;

            fn $op_method(self, rhs: $num) -> Self::Output {
                Unit::Dp(self.as_ref().$op_method(rhs as f32))
            }
        }
        
        impl $op_trait<&$num> for &Unit {
            type Output = Unit;

            fn $op_method(self, rhs: &$num) -> Self::Output {
                Unit::Dp(self.as_ref().$op_method(*rhs as f32))
            }
        }
        
        impl $op_trait<&Unit> for &$num {
            type Output = Unit;

            fn $op_method(self, rhs: &Unit) -> Self::Output {
                Unit::Dp((*self as f32).$op_method(rhs.as_ref()))
            }
        }
    }
}

macro_rules! impl_all_op_for_unit_and_num {
    ($($t:ty),*) => {
        $(
            impl_unit_op_num!(Add, add, $t);
            impl_unit_op_num!(Sub, sub, $t);
            impl_unit_op_num!(Mul, mul, $t);
            impl_unit_op_num!(Div, div, $t);
            impl_unit_op_num!(Rem, rem, $t);
        )*
    };
}

use std::ops::{Add, Sub, Mul, Div, Rem};
impl_all_op_for_unit_and_num!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);

macro_rules! impl_unit_op_unit {
    ($op_trait:ident, $op_method:ident, $msg:expr) => {
        impl $op_trait<Unit> for Unit {
            type Output = Unit;

            fn $op_method(self, rhs: Unit) -> Self::Output {
                match (self, rhs) {
                    (Unit::Dp(lhs), Unit::Dp(rhs)) => Unit::Dp(lhs.$op_method(rhs)),
                    (Unit::Px(lhs), Unit::Px(rhs)) => Unit::Px(lhs.$op_method(rhs)),
                    (Unit::Sp(lhs), Unit::Sp(rhs)) => Unit::Sp(lhs.$op_method(rhs)),
                    _ => panic!($msg),
                }
            }
        }

        impl $op_trait<&Unit> for Unit {
            type Output = Unit;

            fn $op_method(self, rhs: &Unit) -> Self::Output {
                match (self, rhs) {
                    (Unit::Dp(lhs), Unit::Dp(rhs)) => Unit::Dp(lhs.$op_method(*rhs)),
                    (Unit::Px(lhs), Unit::Px(rhs)) => Unit::Px(lhs.$op_method(*rhs)),
                    (Unit::Sp(lhs), Unit::Sp(rhs)) => Unit::Sp(lhs.$op_method(*rhs)),
                    _ => panic!($msg),
                }
            }
        }

        impl $op_trait<Unit> for &Unit {
            type Output = Unit;

            fn $op_method(self, rhs: Unit) -> Self::Output {
                match (self, rhs) {
                    (Unit::Dp(lhs), Unit::Dp(rhs)) => Unit::Dp((*lhs).$op_method(rhs)),
                    (Unit::Px(lhs), Unit::Px(rhs)) => Unit::Px((*lhs).$op_method(rhs)),
                    (Unit::Sp(lhs), Unit::Sp(rhs)) => Unit::Sp((*lhs).$op_method(rhs)),
                    _ => panic!($msg),
                }
            }
        }

        impl $op_trait<&Unit> for &Unit {
            type Output = Unit;

            fn $op_method(self, rhs: &Unit) -> Self::Output {
                match (self, rhs) {
                    (Unit::Dp(lhs), Unit::Dp(rhs)) => Unit::Dp((*lhs).$op_method(*rhs)),
                    (Unit::Px(lhs), Unit::Px(rhs)) => Unit::Px((*lhs).$op_method(*rhs)),
                    (Unit::Sp(lhs), Unit::Sp(rhs)) => Unit::Sp((*lhs).$op_method(*rhs)),
                    _ => panic!($msg),
                }
            }
        }
    }
}

impl_unit_op_unit!(Add, add, "Addition of different unit types");
impl_unit_op_unit!(Sub, sub, "Subtraction of different unit types");
impl_unit_op_unit!(Mul, mul, "Multiplication of different unit types");
impl_unit_op_unit!(Div, div, "Division of different unit types");
impl_unit_op_unit!(Rem, rem, "Remainder of different unit types");

impl AddAssign for Unit {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl AddAssign<&Self> for Unit {
    fn add_assign(&mut self, rhs: &Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Unit {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl SubAssign<&Self> for Unit {
    fn sub_assign(&mut self, rhs: &Self) {
        *self = *self - rhs;
    }
}

impl Neg for Unit {
    type Output = Unit;

    fn neg(self) -> Self::Output {
        match self {
            Unit::Dp(value) => Unit::Dp(-value),
            Unit::Px(value) => Unit::Px(-value),
            Unit::Sp(value) => Unit::Sp(-value),
        }
    }
}