use crate::shared::{Gettable, Shared, SharedAnimation};

macro_rules! p_op_v {
    ($op:ident, $op_fn:ident, $l:ty, $r:ty, $out:ty) => {
        impl std::ops::$op<$r> for $l {
            type Output = $out;

            fn $op_fn(self, rhs: $r) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                Shared::from_dynamic(&[lhs.clone()], move || lhs.get().$op_fn(rhs))
            }
        }
    };
}

macro_rules! v_op_p {
    ($op:ident, $op_fn:ident, $l:ty, $r:ty, $out:ty) => {
        impl std::ops::$op<$r> for $l {
            type Output = $out;

            fn $op_fn(self, rhs: $r) -> Self::Output {
                let lhs = self.clone();
                let rhs_clone = rhs.clone();
                Shared::from_dynamic(&[rhs_clone.clone()], move || lhs.$op_fn(rhs_clone.get()))
            }
        }
    };
}

macro_rules! p_op_p {
    ($op:ident, $op_fn:ident, $l:ty, $r:ty, $out:ty) => {
        impl std::ops::$op<$r> for $l {
            type Output = $out;

            fn $op_fn(self, rhs: $r) -> Self::Output {
                let lhs = self.clone();
                let rhs_clone = rhs.clone();
                Shared::from_dynamic(&[lhs.clone(), rhs_clone.clone()], move || {
                    lhs.get().$op_fn(rhs_clone.get())
                })
            }
        }
    };
}

pub type SharedI8 = Shared<i8>;
pub type SharedI16 = Shared<i16>;
pub type SharedI32 = Shared<i32>;
pub type SharedI64 = Shared<i64>;
pub type SharedI128 = Shared<i128>;
pub type SharedIsize = Shared<isize>;
pub type SharedU8 = Shared<u8>;
pub type SharedU16 = Shared<u16>;
pub type SharedU32 = Shared<u32>;
pub type SharedU64 = Shared<u64>;
pub type SharedU128 = Shared<u128>;
pub type SharedUsize = Shared<usize>;
pub type SharedF32 = Shared<f32>;
pub type SharedF64 = Shared<f64>;

macro_rules! impl_p_op_v {
    ($l:ty, $r:ty, $out:ty) => {
        p_op_v!(Add, add, $l, $r, $out);
        p_op_v!(Sub, sub, $l, $r, $out);
        p_op_v!(Mul, mul, $l, $r, $out);
        p_op_v!(Div, div, $l, $r, $out);
        p_op_v!(Rem, rem, $l, $r, $out);
    };
}

macro_rules! impl_v_op_p {
    ($l:ty, $r:ty, $out:ty) => {
        v_op_p!(Add, add, $l, $r, $out);
        v_op_p!(Sub, sub, $l, $r, $out);
        v_op_p!(Mul, mul, $l, $r, $out);
        v_op_p!(Div, div, $l, $r, $out);
        v_op_p!(Rem, rem, $l, $r, $out);
    };
}

macro_rules! impl_p_op_p {
    ($l:ty, $r:ty, $out:ty) => {
        p_op_p!(Add, add, $l, $r, $out);
        p_op_p!(Sub, sub, $l, $r, $out);
        p_op_p!(Mul, mul, $l, $r, $out);
        p_op_p!(Div, div, $l, $r, $out);
        p_op_p!(Rem, rem, $l, $r, $out);
    };
}

macro_rules! impl_property {
    ($property: ident, $generic: ty) => {
        impl_p_op_v!($property, $generic, $property);
        impl_p_op_v!(&$property, $generic, $property);
        impl_p_op_v!(&mut $property, $generic, $property);

        impl_v_op_p!($generic, $property, $property);
        impl_v_op_p!($generic, &$property, $property);
        impl_v_op_p!($generic, &mut $property, $property);

        impl_p_op_p!($property, $property, $property);
        impl_p_op_p!(&$property, $property, $property);
        impl_p_op_p!($property, &$property, $property);
        impl_p_op_p!(&$property, &$property, $property);
        impl_p_op_p!(&mut $property, $property, $property);
        impl_p_op_p!($property, &mut $property, $property);
        impl_p_op_p!(&mut $property, &$property, $property);
        impl_p_op_p!(&$property, &mut $property, $property);
        impl_p_op_p!(&mut $property, &mut $property, $property);
    };
}

impl_property!(SharedI8, i8);
impl_property!(SharedI16, i16);
impl_property!(SharedI32, i32);
impl_property!(SharedI64, i64);
impl_property!(SharedI128, i128);
impl_property!(SharedIsize, isize);
impl_property!(SharedU8, u8);
impl_property!(SharedU16, u16);
impl_property!(SharedU32, u32);
impl_property!(SharedU64, u64);
impl_property!(SharedU128, u128);
impl_property!(SharedUsize, usize);
impl_property!(SharedF32, f32);
impl_property!(SharedF64, f64);

macro_rules! into_type {
    ($from: ty, $to: ident) => {
        impl Shared<$from> {
            pub fn $to(&self) -> Shared<$to> {
                let self_clone = self.clone();
                Shared::from_dynamic(&[self.clone()], move || self_clone.get() as $to)
            }
        }
    };
}

macro_rules! impl_into_type{
    ($from: ty $(, $to: ident)*) => {
        $(into_type!($from, $to);)*
    }
}

impl_into_type!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(i16, i8, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(i32, i8, i16, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(i64, i8, i16, i32, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(i128, i8, i16, i32, i64, isize, u8, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(isize, i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(u8, i8, i16, i32, i64, i128, isize, u16, u32, u64, u128, usize, f32, f64);
impl_into_type!(u16, i8, i16, i32, i64, i128, isize, u8, u32, u64, u128, usize, f32, f64);
impl_into_type!(u32, i8, i16, i32, i64, i128, isize, u8, u16, u64, u128, usize, f32, f64);
impl_into_type!(u64, i8, i16, i32, i64, i128, isize, u8, u16, u32, u128, usize, f32, f64);
impl_into_type!(u128, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, usize, f32, f64);
impl_into_type!(usize, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, f32, f64);
impl_into_type!(f32, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f64);
impl_into_type!(f64, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32);

impl SharedF32 {
    pub fn animation_to_f32(&self, to: impl Into<f32>) -> SharedAnimation<f32> {
        SharedAnimation::new(self.clone(), self.get(), to.into(), |from, to, progress| {
            from + (to - from) * progress
        })
    }
}
