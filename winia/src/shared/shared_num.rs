use std::ops::Add;
use std::ops::Sub;
use std::ops::Mul;
use std::ops::Div;
use std::ops::Rem;
use crate::depend;
use crate::shared::{Derived, Readable, Shared, SharedDerived, SharedSource, Source};

pub type SharedI8 = SharedSource<i8>;
pub type SharedI16 = SharedSource<i16>;
pub type SharedI32 = SharedSource<i32>;
pub type SharedI64 = SharedSource<i64>;
pub type SharedI128 = SharedSource<i128>;
pub type SharedIsize = SharedSource<isize>;
pub type SharedU8 = SharedSource<u8>;
pub type SharedU16 = SharedSource<u16>;
pub type SharedU32 = SharedSource<u32>;
pub type SharedU64 = SharedSource<u64>;
pub type SharedU128 = SharedSource<u128>;
pub type SharedUsize = SharedSource<usize>;
pub type SharedF32 = SharedSource<f32>;
pub type SharedF64 = SharedSource<f64>;

pub type SharedDerivedI8 = SharedDerived<i8>;
pub type SharedDerivedI16 = SharedDerived<i16>;
pub type SharedDerivedI32 = SharedDerived<i32>;
pub type SharedDerivedI64 = SharedDerived<i64>;
pub type SharedDerivedI128 = SharedDerived<i128>;
pub type SharedDerivedIsize = SharedDerived<isize>;
pub type SharedDerivedU8 = SharedDerived<u8>;
pub type SharedDerivedU16 = SharedDerived<u16>;
pub type SharedDerivedU32 = SharedDerived<u32>;
pub type SharedDerivedU64 = SharedDerived<u64>;
pub type SharedDerivedU128 = SharedDerived<u128>;
pub type SharedDerivedUsize = SharedDerived<usize>;
pub type SharedDerivedF32 = SharedDerived<f32>;
pub type SharedDerivedF64 = SharedDerived<f64>;

macro_rules! impl_shared_num_op {
    ($num_ty:ty, $Op:tt, $op:ident) => {
        impl<A: Readable, B: Readable> $Op<Shared<$num_ty, B>> for Shared<$num_ty, A> {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: Shared<$num_ty, B>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&lhs, &rhs),
                    move || lhs.get().$op(rhs.get()),
                )
            }
        }
        impl<A: Readable, B: Readable> $Op<&Shared<$num_ty, B>> for Shared<$num_ty, A> {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: &Shared<$num_ty, B>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&lhs, &rhs),
                    move || lhs.get().$op(rhs.get()),
                )
            }
        }

        impl<A: Readable, B: Readable> $Op<Shared<$num_ty, B>> for &Shared<$num_ty, A> {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: Shared<$num_ty, B>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&lhs, &rhs),
                    move || lhs.get().$op(rhs.get())
                )
            }
        }

        impl<A: Readable, B: Readable> $Op<&Shared<$num_ty, B>> for &Shared<$num_ty, A> {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: &Shared<$num_ty, B>) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&lhs, &rhs),
                    move || lhs.get().$op(rhs.get())
                )
            }
        }

        impl<A: Readable> $Op<$num_ty> for &Shared<$num_ty, A> {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: $num_ty) -> Self::Output {
                let lhs = self.clone();
                SharedDerived::from_fn(
                    depend!(&lhs),
                    move || lhs.get().$op(rhs)
                )
            }
        }

        impl<A: Readable> $Op<$num_ty> for Shared<$num_ty, A> {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: $num_ty) -> Self::Output {
                let lhs = self.clone();
                SharedDerived::from_fn(
                    depend!(&lhs),
                    move || lhs.get().$op(rhs)
                )
            }
        }

        impl<A: Readable> $Op<Shared<$num_ty, A>> for $num_ty {
            type Output = SharedDerived<$num_ty>;

            fn $op(self, rhs: Shared<$num_ty, A>) -> Self::Output {
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&rhs),
                    move || self.$op(rhs.get())
                )
            }
        }
    };
}
macro_rules! impl_all_op {
    ($num_ty:ty) => {
        impl_shared_num_op!($num_ty, Add, add);
        impl_shared_num_op!($num_ty, Sub, sub);
        impl_shared_num_op!($num_ty, Mul, mul);
        impl_shared_num_op!($num_ty, Div, div);
        impl_shared_num_op!($num_ty, Rem, rem);
    }
}

impl_all_op!(i8);
impl_all_op!(i16);
impl_all_op!(i32);
impl_all_op!(i64);
impl_all_op!(i128);
impl_all_op!(isize);
impl_all_op!(u8);
impl_all_op!(u16);
impl_all_op!(u32);
impl_all_op!(u64);
impl_all_op!(u128);
impl_all_op!(usize);
impl_all_op!(f32);
impl_all_op!(f64);

macro_rules! impl_shared_to {
    ($from:ty, $to:ty, $to_fn:ident) => {
        impl<T: Readable> Shared<$from, T> {
            pub fn $to_fn(&self) -> SharedDerived<$to> {
                let this = self.clone();
                SharedDerived::from_fn(depend!(this), move || this.get() as $to)
            }
        }
    };
}

macro_rules! impl_shared_to_all {
    ($from:ty, $($to:ty|$to_fn:ident),*) => {
        $(
            impl_shared_to!($from, $to, $to_fn);
        )*
    };
}
impl_shared_to_all!(i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(i16, i8|to_i8, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(i32, i8|to_i8, i16|to_i16, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(i64, i8|to_i8, i16|to_i16, i32|to_i32, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(i128, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(isize, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(u8, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(u16, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(u32, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(u64, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u128|to_u128, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(u128, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, usize|to_usize, f32|to_f32, f64|to_f64);
impl_shared_to_all!(usize, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, f32|to_f32, f64|to_f64);
impl_shared_to_all!(f32, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f64|to_f64);
impl_shared_to_all!(f64, i8|to_i8, i16|to_i16, i32|to_i32, i64|to_i64, i128|to_i128, isize|to_isize, u8|to_u8, u16|to_u16, u32|to_u32, u64|to_u64, u128|to_u128, usize|to_usize, f32|to_f32);


mod tests {
    use crate::shared::Observable;
use crate::depend;
    use crate::shared::{SharedDerived, SharedF32, SharedIsize, SharedUsize};

    #[test]
    fn test_shared_static() {
        let a: SharedUsize = 5.into();
        let b: SharedUsize = 10.into();
        let c = &a + &b;
        assert_eq!(c.get(), 15);
    }

    #[test]
    fn test_shared_dynamic() {
        let a: SharedUsize = 5.into();
        let b: SharedUsize = 10.into();
        let c = &a + &b;
        assert_eq!(c.get(), 15);
        a.set(20);
        assert_eq!(c.get(), 30);
        b.set(5);
        assert_eq!(c.get(), 25);
    }

    #[test]
    fn test_shared_mixed() {
        let a: SharedIsize = 5.into();
        let b: SharedUsize = 10.into();
        let d: SharedF32 = 2.5.into();
        let c = a.to_usize() + &b;
        let e = c.to_f32() * &d;
        assert_eq!(c.get(), 15);
        assert_eq!(e.get(), 37.5);
        a.set(20);
        assert_eq!(c.get(), 30);
        assert_eq!(e.get(), 75.0);
    }
}