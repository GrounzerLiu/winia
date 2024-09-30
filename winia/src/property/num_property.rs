use std::ops::Add;
use crate::property::{Gettable, Property};
use crate::core::RefClone;

macro_rules! p_op_v {
    ($op:ident, $op_fn:ident, $l:ty, $r:ty, $out:ty) => {
        impl std::ops::$op<$r> for $l {
            type Output = $out;

            fn $op_fn(self, rhs: $r) -> Self::Output {
                let lhs = self.ref_clone();
                let rhs = rhs.clone();
                let mut output = Property::from_dynamic(Box::new(move || {
                    lhs.get().$op_fn(rhs)
                }));
                output.observe(self);
                output
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
                let rhs_clone = rhs.ref_clone();
                let mut output = Property::from_dynamic(Box::new(move || {
                    lhs.$op_fn(rhs_clone.get())
                }));
                output.observe(rhs);
                output
            }
        }
    };
}

macro_rules! p_op_p {
    ($op:ident, $op_fn:ident, $l:ty, $r:ty, $out:ty) => {
        impl std::ops::$op<$r> for $l {
            type Output = $out;

            fn $op_fn(self, rhs: $r) -> Self::Output {
                let lhs = self.ref_clone();
                let rhs_clone = rhs.ref_clone();
                let mut output = Property::from_dynamic(Box::new(move || {
                    lhs.get().$op_fn(rhs_clone.get())
                }));
                output.observe(self);
                output.observe(rhs);
                output
            }
        }
    };
}

pub type I8Property = Property<i8>;
pub type I16Property = Property<i16>;
pub type I32Property = Property<i32>;
pub type I64Property = Property<i64>;
pub type I128Property = Property<i128>;
pub type IsizeProperty = Property<isize>;
pub type U8Property = Property<u8>;
pub type U16Property = Property<u16>;
pub type U32Property = Property<u32>;
pub type U64Property = Property<u64>;
pub type U128Property = Property<u128>;
pub type UsizeProperty = Property<usize>;
pub type F32Property = Property<f32>;
pub type F64Property = Property<f64>;


macro_rules! impl_p_op_v {
    ($l:ty, $r:ty, $out:ty) => {
        p_op_v!(Add, add, $l, $r, $out);
        p_op_v!(Sub, sub, $l, $r, $out);
        p_op_v!(Mul, mul, $l, $r, $out);
        p_op_v!(Div, div, $l, $r, $out);
        p_op_v!(Rem, rem, $l, $r, $out);
    }
}

macro_rules! impl_v_op_p {
    ($l:ty, $r:ty, $out:ty) => {
        v_op_p!(Add, add, $l, $r, $out);
        v_op_p!(Sub, sub, $l, $r, $out);
        v_op_p!(Mul, mul, $l, $r, $out);
        v_op_p!(Div, div, $l, $r, $out);
        v_op_p!(Rem, rem, $l, $r, $out);
    }
}

macro_rules! impl_p_op_p {
    ($l:ty, $r:ty, $out:ty) => {
        p_op_p!(Add, add, $l, $r, $out);
        p_op_p!(Sub, sub, $l, $r, $out);
        p_op_p!(Mul, mul, $l, $r, $out);
        p_op_p!(Div, div, $l, $r, $out);
        p_op_p!(Rem, rem, $l, $r, $out);
    }
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
    }
}

impl_property!(I8Property, i8);
impl_property!(I16Property, i16);
impl_property!(I32Property, i32);
impl_property!(I64Property, i64);
impl_property!(I128Property, i128);
impl_property!(IsizeProperty, isize);
impl_property!(U8Property, u8);
impl_property!(U16Property, u16);
impl_property!(U32Property, u32);
impl_property!(U64Property, u64);
impl_property!(U128Property, u128);
impl_property!(UsizeProperty, usize);
impl_property!(F32Property, f32);
impl_property!(F64Property, f64);

macro_rules! into_type {
    ($from: ty, $to: ident) =>{
        impl Property<$from> {
            pub fn $to(&self) -> Property<$to> {
                let self_clone = self.ref_clone();
                let mut output = Property::from_dynamic(Box::new(move || {
                    self_clone.get() as $to
                }));
                output.observe(self.ref_clone());
                output
            }
        }
    }
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



