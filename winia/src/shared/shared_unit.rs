use std::ops::{Add, AddAssign, Div, Mul, Rem, Sub};
use crate::shared::{Gettable, Shared, SharedF32};
use crate::ui::Unit;

pub type SharedUnit = Shared<Unit>;
macro_rules! impl_u_op_num {
    ($op_trait:ident, $op_method:ident, $num:ty) => {
        impl $op_trait<$num> for SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: $num) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs))
            }
        }

        impl $op_trait<SharedUnit> for $num {
            type Output = SharedUnit;

            fn $op_method(self, rhs: SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[rhs.clone()], move || lhs.$op_method(rhs.get()))
            }
        }

        impl $op_trait<&SharedUnit> for $num {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[rhs.clone()], move || lhs.$op_method(rhs.get()))
            }
        }

        impl $op_trait<$num> for &SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: $num) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs))
            }
        }

        impl $op_trait<&$num> for SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &$num) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs))
            }
        }

        impl $op_trait<&$num> for &SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &$num) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs))
            }
        }

        impl $op_trait<&SharedUnit> for &$num {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[rhs.clone()], move || lhs.$op_method(rhs.get()))
            }
        }
    };
}

macro_rules! impl_all_op_for_unit_and_num {
    ($($t:ty),*) => {
        $(
            impl_u_op_num!(Add, add, $t);
            impl_u_op_num!(Sub, sub, $t);
            impl_u_op_num!(Mul, mul, $t);
            impl_u_op_num!(Div, div, $t);
            impl_u_op_num!(Rem, rem, $t);
        )*
    };
}

impl_all_op_for_unit_and_num!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);

macro_rules! impl_u_op_s {
    ($op_trait:ident, $op_method:ident, $t:ty) => {
        impl $op_trait<Shared<$t>> for SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: Shared<$t>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<SharedUnit> for Shared<$t> {
            type Output = SharedUnit;

            fn $op_method(self, rhs: SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<&Shared<$t>> for SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &Shared<$t>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<SharedUnit> for &Shared<$t> {
            type Output = SharedUnit;

            fn $op_method(self, rhs: SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<Shared<$t>> for &SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: Shared<$t>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<&SharedUnit> for Shared<$t> {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<&Shared<$t>> for &SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &Shared<$t>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }

        impl $op_trait<&SharedUnit> for &Shared<$t> {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                let lhs_clone = lhs.clone();
                let rhs_clone = rhs.clone();
                let output = Shared::from_dynamic(&[lhs_clone], move || lhs.get().$op_method(rhs.get()));
                output.observe(rhs_clone);
                output
            }
        }
    }
}

macro_rules! impl_all_op_for_unit_and_shared {
    ($($t:ty),*) => {
        $(
            impl_u_op_s!(Add, add, $t);
            impl_u_op_s!(Sub, sub, $t);
            impl_u_op_s!(Mul, mul, $t);
            impl_u_op_s!(Div, div, $t);
            impl_u_op_s!(Rem, rem, $t);
        )*
    };
}

impl_all_op_for_unit_and_shared!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);


macro_rules! impl_u_op_u {
    ($op_trait:ident, $op_method:ident) => {
        impl $op_trait<SharedUnit> for SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs.get()))
            }
        }

        impl $op_trait<&SharedUnit> for SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs.get()))
            }
        }
        
        impl $op_trait<SharedUnit> for &SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs.get()))
            }
        }
        
        impl $op_trait<&SharedUnit> for &SharedUnit {
            type Output = SharedUnit;

            fn $op_method(self, rhs: &SharedUnit) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_method(rhs.get()))
            }
        }
    }
}

impl_u_op_u!(Add, add);
impl_u_op_u!(Sub, sub);
impl_u_op_u!(Mul, mul);
impl_u_op_u!(Div, div);
impl_u_op_u!(Rem, rem);