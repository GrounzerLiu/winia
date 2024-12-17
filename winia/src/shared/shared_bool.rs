use std::ops::{BitAnd, BitOr};
use crate::shared::{Gettable, Shared};
use crate::core::RefClone;
pub type SharedBool = Shared<bool>;

macro_rules! p_op_p{
    ($op:ident, $op_fn:ident, $lhs:ty, $rhs:ty) => {
        impl $op<$rhs> for $lhs {
            type Output = SharedBool;

            fn $op_fn(self, rhs: $rhs) -> Self::Output {
                let lhs = self.ref_clone();
                let rhs_clone = rhs.ref_clone();
                let mut output = SharedBool::from_dynamic(Box::new(move || {
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
            type Output = SharedBool;

            fn $op_fn(self, rhs: bool) -> Self::Output {
                let lhs = self.ref_clone();
                let mut output = SharedBool::from_dynamic(Box::new(move || {
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
        impl $op<&SharedBool> for $p {
            type Output = SharedBool;

            fn $op_fn(self, rhs: &SharedBool) -> Self::Output {
                let rhs_clone = rhs.ref_clone();
                let mut output = SharedBool::from_dynamic(Box::new(move || {
                    self.$op_fn(rhs_clone.get())
                }));
                output.observe(rhs);
                output
            }
        }
    };
}

p_op_p!(BitAnd, bitand, SharedBool, SharedBool);
p_op_p!(BitAnd, bitand, SharedBool, &SharedBool);
p_op_p!(BitAnd, bitand, &SharedBool, SharedBool);
p_op_p!(BitAnd, bitand, &SharedBool, &SharedBool);
p_op_p!(BitAnd, bitand, SharedBool, &mut SharedBool);
p_op_p!(BitAnd, bitand, &mut SharedBool, SharedBool);
p_op_p!(BitAnd, bitand, &mut SharedBool, &mut SharedBool);
p_op_p!(BitAnd, bitand, &mut SharedBool, &SharedBool);
p_op_p!(BitAnd, bitand, &SharedBool, &mut SharedBool);

p_op_p!(BitOr, bitor, SharedBool, SharedBool);
p_op_p!(BitOr, bitor, SharedBool, &SharedBool);
p_op_p!(BitOr, bitor, &SharedBool, SharedBool);
p_op_p!(BitOr, bitor, &SharedBool, &SharedBool);
p_op_p!(BitOr, bitor, SharedBool, &mut SharedBool);
p_op_p!(BitOr, bitor, &mut SharedBool, SharedBool);
p_op_p!(BitOr, bitor, &mut SharedBool, &mut SharedBool);
p_op_p!(BitOr, bitor, &mut SharedBool, &SharedBool);
p_op_p!(BitOr, bitor, &SharedBool, &mut SharedBool);


p_op_v!(BitAnd, bitand, SharedBool);
p_op_v!(BitAnd, bitand, &SharedBool);
p_op_v!(BitAnd, bitand, &mut SharedBool);

p_op_v!(BitOr, bitor, SharedBool);
p_op_v!(BitOr, bitor, &SharedBool);
p_op_v!(BitOr, bitor, &mut SharedBool);

v_op_p!(BitAnd, bitand, bool);
v_op_p!(BitOr, bitor, bool);