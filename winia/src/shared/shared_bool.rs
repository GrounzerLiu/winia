use crate::shared::{Gettable, Shared};
use std::ops::{BitAnd, BitOr};
pub type SharedBool = Shared<bool>;

macro_rules! p_op_p {
    ($op:ident, $op_fn:ident, $lhs:ty, $rhs:ty) => {
        impl $op<$rhs> for $lhs {
            type Output = SharedBool;

            fn $op_fn(self, rhs: $rhs) -> Self::Output {
                let lhs = self.clone();
                let rhs_clone = rhs.clone();
                SharedBool::from_dynamic([self.as_ref().into(), rhs.as_ref().into()].into(), move || {
                    lhs.get().$op_fn(rhs_clone.get())
                })
            }
        }
    };
}

macro_rules! p_op_v {
    ($op:ident, $op_fn:ident, $p:ty) => {
        impl $op<bool> for $p {
            type Output = SharedBool;

            fn $op_fn(self, rhs: bool) -> Self::Output {
                let lhs = self.clone();
                SharedBool::from_dynamic([self.as_ref().into()].into(), move || lhs.get().$op_fn(rhs))
            }
        }
    };
}

macro_rules! v_op_p {
    ($op:ident, $op_fn:ident, $p:ty) => {
        impl $op<&SharedBool> for $p {
            type Output = SharedBool;

            fn $op_fn(self, rhs: &SharedBool) -> Self::Output {
                let rhs_clone = rhs.clone();
                SharedBool::from_dynamic([rhs.as_ref().into()].into(), move || self.$op_fn(rhs_clone.get()))
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
