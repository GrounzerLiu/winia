use crate::depend;
use std::ops::{BitAnd, BitOr, Not};
use crate::shared::{SharedDerived, SharedSource};

pub type SharedBool = SharedSource<bool>;
pub type SharedDerivedBool = SharedDerived<bool>;

macro_rules! impl_not {
    ($ty:ty) => {
        impl Not for $ty {
            type Output = SharedDerivedBool;

            fn not(self) -> Self::Output {
                let val = self.clone();
                SharedDerived::from_fn(
                    depend!(&val),
                    move || !val.get(),
                )
            }
        }
    };
}

impl_not!(SharedBool);
impl_not!(&SharedBool);
impl_not!(SharedDerivedBool);
impl_not!(&SharedDerivedBool);
macro_rules! impl_op {
    ($A:ty, $B:ty) => {
        impl BitAnd<$B> for $A {
            type Output = SharedDerivedBool;

            fn bitand(self, rhs: $B) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&lhs, &rhs),
                    move || lhs.get().bitand(rhs.get()),
                )
            }
        }
        impl BitOr<$B> for $A {
            type Output = SharedDerivedBool;

            fn bitor(self, rhs: $B) -> Self::Output {
                let lhs = self.clone();
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&lhs, &rhs),
                    move || lhs.get().bitor(rhs.get()),
                )
            }
        }
    };
}

macro_rules! impl_op_all {
    ($A:ty, $B:ty) => {
        impl_op!($A, $B);
        impl_op!($A, &$B);
        impl_op!(&$A, $B);
        impl_op!(&$A, &$B);
    };
}

impl_op_all!(SharedBool, SharedBool);
impl_op_all!(SharedBool, SharedDerivedBool);
impl_op_all!(SharedDerivedBool, SharedBool);
impl_op_all!(SharedDerivedBool, SharedDerivedBool);

macro_rules! impl_op_bool {
    ($A:ty) => {
        impl BitAnd<bool> for $A {
            type Output = SharedDerivedBool;

            fn bitand(self, rhs: bool) -> Self::Output {
                let lhs = self.clone();
                SharedDerived::from_fn(
                    depend!(&lhs),
                    move || lhs.get().bitand(rhs),
                )
            }
        }
        impl BitOr<bool> for $A {
            type Output = SharedDerivedBool;

            fn bitor(self, rhs: bool) -> Self::Output {
                let lhs = self.clone();
                SharedDerived::from_fn(
                    depend!(&lhs),
                    move || lhs.get().bitor(rhs),
                )
            }
        }

        impl BitAnd<$A> for bool {
            type Output = SharedDerivedBool;

            fn bitand(self, rhs: $A) -> Self::Output {
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&rhs),
                    move || self.bitand(rhs.get()),
                )
            }
        }

        impl BitOr<$A> for bool {
            type Output = SharedDerivedBool;

            fn bitor(self, rhs: $A) -> Self::Output {
                let rhs = rhs.clone();
                SharedDerived::from_fn(
                    depend!(&rhs),
                    move || self.bitor(rhs.get()),
                )
            }
        }
    }
}

impl_op_bool!(SharedBool);
impl_op_bool!(&SharedBool);
impl_op_bool!(SharedDerivedBool);
impl_op_bool!(&SharedDerivedBool);

#[cfg(test)]
mod tests {
    use crate::shared::{Shared, SharedBool, SharedDerivedBool};

    #[test]
    fn test_not() {
        let a: SharedBool = Shared::new(true);
        let b: SharedDerivedBool = !a.clone();
        assert!(!b.get());
        a.set(false);
        assert!(b.get());

        let c: SharedDerivedBool = !&a;
        a.set(true);
        assert!(!c.get());
        a.set(false);
        assert!(c.get());

        let d: SharedDerivedBool = !b.clone();
        a.set(true);
        assert!(d.get());
        a.set(false);
        assert!(!d.get());

        let e: SharedDerivedBool = !&b;
        a.set(true);
        assert!(e.get());
        a.set(false);
        assert!(!e.get());
    }

    #[test]
    fn test_and() {
        let a: SharedBool = Shared::new(true);
        let b: SharedBool = Shared::new(false);
        let c: SharedDerivedBool = a.clone() & b.clone();
        let d: SharedDerivedBool = &a & b.clone();
        let e: SharedDerivedBool = a.clone() & &b;
        let f: SharedDerivedBool = &a & &b;
        assert!(!c.get());
        assert!(!d.get());
        assert!(!e.get());
        assert!(!f.get());
        a.set(false);
        assert!(!c.get());
        assert!(!d.get());
        assert!(!e.get());
        assert!(!f.get());
        a.set(true);
        assert!(!c.get());
        assert!(!d.get());
        assert!(!e.get());
        assert!(!f.get());
        b.set(true);
        assert!(c.get());
        assert!(d.get());
        assert!(e.get());
        assert!(f.get());
    }

    #[test]
    fn test_or() {
        let a: SharedBool = Shared::new(true);
        let b: SharedBool = Shared::new(false);
        let c: SharedDerivedBool = a.clone() | b.clone();
        let d: SharedDerivedBool = &a | b.clone();
        let e: SharedDerivedBool = a.clone() | &b;
        let f: SharedDerivedBool = &a | &b;
        assert!(c.get());
        assert!(d.get());
        assert!(e.get());
        assert!(f.get());
        a.set(false);
        assert!(!c.get());
        assert!(!d.get());
        assert!(!e.get());
        assert!(!f.get());
        a.set(true);
        assert!(c.get());
        assert!(d.get());
        assert!(e.get());
        assert!(f.get());
        b.set(true);
        assert!(c.get());
        assert!(d.get());
        assert!(e.get());
        assert!(f.get());
    }
}