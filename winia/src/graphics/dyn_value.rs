//! `Dyn<T>`: a value that is either static or evaluated per frame.
//!
//! One carrier for every "static or closure" parameter in the draw layer — `AxisValue` (a variable
//! font axis), `BackgroundColor`, `BorderColor` and `GraphicsLayerSpec` are all aliases of it — so
//! the read rules exist once:
//!
//! - [`Dyn::eval`] at paint time. A paint closure must not read a `State` with `get()`: paint runs
//!   with the dependency tracker off, and a `get()` there would attach the dependency to whatever
//!   slot is active the moment a paint pass runs inside a transaction.
//! - [`Dyn::same_parameter`] for the element comparison: static values compare by value, two dynamic
//!   ones are the same PARAMETER (the number is read at paint time and carried by the dependency),
//!   and a static turning into a dynamic — or back — is a CHANGE.
//!
//! The static form stores `T` inline, so a constant background/layer parameter allocates nothing.

use std::sync::Arc;

/// A value that may be evaluated per frame.
#[derive(Clone)]
pub enum Dyn<T> {
    Static(T),
    Dynamic(Arc<dyn Fn() -> T + Send + Sync>),
}

impl<T: Clone> Dyn<T> {
    /// Read the current value. Call it where the value is consumed (paint), not in the composition.
    pub fn eval(&self) -> T {
        match self {
            Dyn::Static(v) => v.clone(),
            Dyn::Dynamic(f) => f(),
        }
    }
}

impl<T> Dyn<T> {
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Dyn::Dynamic(_))
    }
}

impl<T: PartialEq> Dyn<T> {
    /// Whether two values are the same PARAMETER — see the module docs.
    pub fn same_parameter(&self, other: &Self) -> bool {
        match (self, other) {
            (Dyn::Static(a), Dyn::Static(b)) => a == b,
            (Dyn::Dynamic(_), Dyn::Dynamic(_)) => true,
            _ => false,
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Dyn<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dyn::Static(v) => f.debug_tuple("static").field(v).finish(),
            Dyn::Dynamic(_) => f.write_str("dynamic(..)"),
        }
    }
}

/// The static and closure forms of one instantiation.
///
/// Two impls per type instead of a blanket `impl<T> From<T> for Dyn<T>`: that blanket cannot coexist
/// with the closure impl (`F` could itself be `T`), and every carrier here is instantiated over a
/// handful of concrete types.
macro_rules! dyn_from_value {
    ($t:ty) => {
        impl From<$t> for Dyn<$t> {
            fn from(v: $t) -> Self {
                Dyn::Static(v)
            }
        }

        impl<F> From<F> for Dyn<$t>
        where
            F: Fn() -> $t + Send + Sync + 'static,
        {
            fn from(f: F) -> Self {
                Dyn::Dynamic(Arc::new(f))
            }
        }
    };
}

dyn_from_value!(f32);
dyn_from_value!(crate::graphics::Color);
dyn_from_value!(crate::graphics::Brush);
dyn_from_value!(crate::graphics::Shape);
dyn_from_value!(crate::graphics::GraphicsLayerParams);
dyn_from_value!(crate::graphics::ColorFilter);
dyn_from_value!(crate::graphics::ShadowParams);

/// The state forms of one instantiation: a `State`/`Animating`/`DerivedValue` reads through `get()`,
/// which is correct at the point the closure runs (paint reads it untracked, but the dependency that
/// redraws comes from the animation loop).
macro_rules! dyn_from_state {
    ($t:ty) => {
        impl From<crate::runtime::state::State<$t>> for Dyn<$t> {
            fn from(s: crate::runtime::state::State<$t>) -> Self {
                Dyn::Dynamic(Arc::new(move || s.get()))
            }
        }

        impl From<&crate::runtime::state::State<$t>> for Dyn<$t> {
            fn from(s: &crate::runtime::state::State<$t>) -> Self {
                let s = s.clone();
                Dyn::Dynamic(Arc::new(move || s.get()))
            }
        }

        impl From<crate::runtime::state::DerivedValue<$t>> for Dyn<$t> {
            fn from(d: crate::runtime::state::DerivedValue<$t>) -> Self {
                Dyn::Dynamic(Arc::new(move || d.get()))
            }
        }

        impl From<&crate::runtime::state::DerivedValue<$t>> for Dyn<$t> {
            fn from(d: &crate::runtime::state::DerivedValue<$t>) -> Self {
                let d = d.clone();
                Dyn::Dynamic(Arc::new(move || d.get()))
            }
        }
    };
}

dyn_from_state!(f32);
dyn_from_state!(crate::graphics::Color);
dyn_from_state!(crate::graphics::Brush);
dyn_from_state!(crate::graphics::Shape);
dyn_from_state!(crate::graphics::ColorFilter);
dyn_from_state!(crate::graphics::ShadowParams);

impl From<crate::runtime::state::Animating<f32>> for Dyn<f32> {
    fn from(s: crate::runtime::state::Animating<f32>) -> Self {
        Dyn::Dynamic(Arc::new(move || s.get()))
    }
}

impl From<&crate::runtime::state::Animating<f32>> for Dyn<f32> {
    fn from(s: &crate::runtime::state::Animating<f32>) -> Self {
        let s = s.clone();
        Dyn::Dynamic(Arc::new(move || s.get()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::Color;
    use crate::runtime::state::State;

    /// A constant is stored inline — no closure, no allocation.
    #[test]
    fn a_static_value_is_stored_inline() {
        let d: Dyn<f32> = 4.0f32.into();
        assert!(matches!(d, Dyn::Static(4.0)));
        assert!(!d.is_dynamic());
        assert_eq!(d.eval(), 4.0);

        let c: Dyn<Color> = Color::from_argb(255, 1, 2, 3).into();
        assert!(matches!(c, Dyn::Static(_)));
        assert_eq!(c.eval(), Color::from_argb(255, 1, 2, 3));
    }

    /// A state or a closure is evaluated on every read.
    #[test]
    fn a_dynamic_value_tracks_its_source() {
        let s = State::new(1.0f32);
        let d = Dyn::from(&s);
        assert!(d.is_dynamic());
        assert_eq!(d.eval(), 1.0);
        s.set(3.0);
        assert_eq!(d.eval(), 3.0);

        assert_eq!(Dyn::from(|| 7.0f32).eval(), 7.0);
    }

    /// Two dynamics are the same PARAMETER; a static turning into a dynamic (or back) is a change.
    #[test]
    fn same_parameter_separates_parameters_from_values() {
        let a = Dyn::from(State::new(1.0f32));
        let b = Dyn::from(State::new(2.0f32));
        assert!(a.same_parameter(&b), "two dynamic values are the same request");

        let literal = Dyn::from(1.0f32);
        assert!(!a.same_parameter(&literal), "a literal and a state are different requests");
        assert!(!literal.same_parameter(&a), "and the comparison is symmetric");
        assert!(literal.same_parameter(&Dyn::from(1.0f32)));
        assert!(!literal.same_parameter(&Dyn::from(2.0f32)));
    }

    /// A shape carrier: inline when literal, live when driven by a state.
    #[test]
    fn a_shape_carrier_follows_its_state() {
        use crate::graphics::Shape;
        let s = State::new(Shape::rounded(4.0));
        let d = Dyn::from(&s);
        assert!(d.is_dynamic());
        assert_eq!(d.eval(), Shape::rounded(4.0));
        s.set(Shape::rounded(8.0));
        assert_eq!(d.eval(), Shape::rounded(8.0));

        let literal: Dyn<Shape> = Shape::Rectangle.into();
        assert!(matches!(literal, Dyn::Static(_)));
        assert!(!literal.same_parameter(&d), "a literal and a state are different requests");
        assert!(d.same_parameter(&Dyn::from(&State::new(Shape::rounded(1.0)))));
    }
}