use crate::property::SharedProperty;
use crate::property::Gettable;

pub type FloatProperty = SharedProperty<f32>;

impl FloatProperty{
    pub fn from_f32(value: f32) -> Self {
        Self::from_value(value)
    }

    pub fn from_f64(value: f64) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_u32(value: u32) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_u64(value: u64) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_u16(value: u16) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_u8(value: u8) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_usize(value: usize) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_i32(value: i32) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_i64(value: i64) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_i16(value: i16) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_i8(value: i8) -> Self {
        Self::from_value(value as f32)
    }

    pub fn from_isize(value: isize) -> Self {
        Self::from_value(value as f32)
    }
}

impl From<&FloatProperty> for FloatProperty{
    fn from(value: &FloatProperty) -> Self {
        value.clone()
    }
}


impl From<i32> for FloatProperty{
    fn from(value: i32) -> Self {
        Self::from_i32(value)
    }
}

impl From<i64> for FloatProperty{
    fn from(value: i64) -> Self {
        Self::from_i64(value)
    }
}

macro_rules! p_op_p {
    ($op:ident, $op_fn:ident) => {
        impl std::ops::$op<&FloatProperty> for &FloatProperty{
            type Output = FloatProperty;

            fn $op_fn(self, rhs: &FloatProperty) -> Self::Output {
                let lhs = self.clone();
                let rhs_clone = rhs.clone();
                let output = FloatProperty::from_generator(Box::new(move || lhs.get().$op_fn(rhs_clone.get())));
                output.observe(self);
                output.observe(rhs);
                output
            }
        }
    };
}

p_op_p!(Add, add);
p_op_p!(Sub, sub);
p_op_p!(Mul, mul);
p_op_p!(Div, div);
p_op_p!(Rem, rem);

macro_rules! p_op_v {
    ($op:ident, $op_fn:ident,$other_type:ty) => {
        impl std::ops::$op<$other_type> for &FloatProperty{
            type Output = FloatProperty;

            fn $op_fn(self, rhs: $other_type) -> Self::Output {
                let lhs = self.clone();
                let rhs = *rhs;
                let output = FloatProperty::from_generator(Box::new(move || lhs.get().$op_fn(rhs as f32)));
                output.observe(self);
                output
            }
        }

        impl std::ops::$op<&FloatProperty> for $other_type{
            type Output = FloatProperty;

            fn $op_fn(self, rhs: &FloatProperty) -> Self::Output {
                rhs.$op_fn(self)
            }
        }
    };
}

macro_rules! generate_op {
    ($other_type:ty) => {
        p_op_v!(Add, add, $other_type);
        p_op_v!(Sub, sub, $other_type);
        p_op_v!(Mul, mul, $other_type);
        p_op_v!(Div, div, $other_type);
        p_op_v!(Rem, rem, $other_type);
    };
}

generate_op!(&f32);
generate_op!(&f64);
generate_op!(&u32);
generate_op!(&u64);
generate_op!(&u16);
generate_op!(&u8);
generate_op!(&usize);
generate_op!(&i32);
generate_op!(&i64);
generate_op!(&i16);
generate_op!(&i8);
generate_op!(&isize);