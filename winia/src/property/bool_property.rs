use std::ops::{BitAnd, BitOr};
use crate::property::{Gettable, Property};
use crate::core::RefClone;
pub type BoolProperty = Property<bool>;

macro_rules! p_op_p{
    ($op:ident, $op_fn:ident, $lhs:ty, $rhs:ty) => {
        impl $op<$rhs> for $lhs {
            type Output = BoolProperty;

            fn $op_fn(self, rhs: $rhs) -> Self::Output {
                let lhs = self.ref_clone();
                let rhs_clone = rhs.ref_clone();
                let mut output = BoolProperty::from_dynamic(Box::new(move || {
                    lhs.get().$op_fn(rhs_clone.get())
                }));
                output.observe(self);
                output.observe(rhs);
                output
            }
        }
    };
}

macro_rules! p_op_v{
    ($op:ident, $op_fn:ident, $p:ty) => {
        impl $op<bool> for $p {
            type Output = BoolProperty;

            fn $op_fn(self, rhs: bool) -> Self::Output {
                let lhs = self.ref_clone();
                let mut output = BoolProperty::from_dynamic(Box::new(move || {
                    lhs.get().$op_fn(rhs)
                }));
                output.observe(self);
                output
            }
        }
    };
}

macro_rules! v_op_p{
    ($op:ident, $op_fn:ident, $p:ty) => {
        impl $op<&BoolProperty> for $p {
            type Output = BoolProperty;

            fn $op_fn(self, rhs: &BoolProperty) -> Self::Output {
                let rhs_clone = rhs.ref_clone();
                let mut output = BoolProperty::from_dynamic(Box::new(move || {
                    self.$op_fn(rhs_clone.get())
                }));
                output.observe(rhs);
                output
            }
        }
    };
}

p_op_p!(BitAnd, bitand, BoolProperty, BoolProperty);
p_op_p!(BitAnd, bitand, BoolProperty, &BoolProperty);
p_op_p!(BitAnd, bitand, &BoolProperty, BoolProperty);
p_op_p!(BitAnd, bitand, &BoolProperty, &BoolProperty);
p_op_p!(BitAnd, bitand, BoolProperty, &mut BoolProperty);
p_op_p!(BitAnd, bitand, &mut BoolProperty, BoolProperty);
p_op_p!(BitAnd, bitand, &mut BoolProperty, &mut BoolProperty);
p_op_p!(BitAnd, bitand, &mut BoolProperty, &BoolProperty);
p_op_p!(BitAnd, bitand, &BoolProperty, &mut BoolProperty);

p_op_p!(BitOr, bitor, BoolProperty, BoolProperty);
p_op_p!(BitOr, bitor, BoolProperty, &BoolProperty);
p_op_p!(BitOr, bitor, &BoolProperty, BoolProperty);
p_op_p!(BitOr, bitor, &BoolProperty, &BoolProperty);
p_op_p!(BitOr, bitor, BoolProperty, &mut BoolProperty);
p_op_p!(BitOr, bitor, &mut BoolProperty, BoolProperty);
p_op_p!(BitOr, bitor, &mut BoolProperty, &mut BoolProperty);
p_op_p!(BitOr, bitor, &mut BoolProperty, &BoolProperty);
p_op_p!(BitOr, bitor, &BoolProperty, &mut BoolProperty);


p_op_v!(BitAnd, bitand, BoolProperty);
p_op_v!(BitAnd, bitand, &BoolProperty);
p_op_v!(BitAnd, bitand, &mut BoolProperty);

p_op_v!(BitOr, bitor, BoolProperty);
p_op_v!(BitOr, bitor, &BoolProperty);
p_op_v!(BitOr, bitor, &mut BoolProperty);

v_op_p!(BitAnd, bitand, bool);
v_op_p!(BitOr, bitor, bool);