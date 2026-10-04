//! Which physical axis is a container's main axis.
//!
//! A flex container and a lazy list ask the same questions of the same two markers — where a size's
//! main extent lives, where a constraint's main budget lives, how to build the values back — and
//! each adds vocabulary on top: [`super::flex::FlexAxis`] the constraint building, `LazyAxis` the
//! scroll modifier and the item constraints.
//!
//! Both used to declare their own `VerticalAxis` and `HorizontalAxis`, so the crate had two types
//! per name, and `size`/`point` took their arguments in opposite orders (`(main, cross)` in flex,
//! `(cross, main)` in the lazy list) for the same operation. The markers and the shared six
//! questions live here now; the two traits extend them.

use super::constraints::Constraints;
use crate::unit::{Offset, Size};

mod sealed {
    pub trait Sealed {}
}

/// A container's main axis, as a type-level parameter: `Column` and `LazyColumn` are
/// [`VerticalAxis`], `Row` and `LazyRow` are [`HorizontalAxis`].
///
/// Sealed — those two markers are the whole set. Every method is an associated function, so the
/// parameter monomorphises away.
pub trait Axis: sealed::Sealed + 'static {
    fn main_size(s: Size) -> f32;
    fn cross_size(s: Size) -> f32;
    fn main_max(c: &Constraints) -> f32;
    fn cross_max(c: &Constraints) -> f32;
    /// `(main, cross)` → size. The order is this way round in both traits: a caller that has a main
    /// extent and a cross extent should not have to remember which one this one wanted.
    fn size(main: f32, cross: f32) -> Size;
    /// `(main, cross)` → position, with the same argument order as [`Self::size`].
    fn point(main: f32, cross: f32) -> Offset;
}

/// `Column`, `LazyColumn`.
pub enum VerticalAxis {}

/// `Row`, `LazyRow`.
pub enum HorizontalAxis {}

impl sealed::Sealed for VerticalAxis {}
impl sealed::Sealed for HorizontalAxis {}

impl Axis for VerticalAxis {
    #[inline] fn main_size(s: Size) -> f32 { s.height }
    #[inline] fn cross_size(s: Size) -> f32 { s.width }
    #[inline] fn main_max(c: &Constraints) -> f32 { c.max_height }
    #[inline] fn cross_max(c: &Constraints) -> f32 { c.max_width }
    #[inline] fn size(main: f32, cross: f32) -> Size { Size::new(cross, main) }
    #[inline] fn point(main: f32, cross: f32) -> Offset { Offset::new(cross, main) }
}

impl Axis for HorizontalAxis {
    #[inline] fn main_size(s: Size) -> f32 { s.width }
    #[inline] fn cross_size(s: Size) -> f32 { s.height }
    #[inline] fn main_max(c: &Constraints) -> f32 { c.max_width }
    #[inline] fn cross_max(c: &Constraints) -> f32 { c.max_height }
    #[inline] fn size(main: f32, cross: f32) -> Size { Size::new(main, cross) }
    #[inline] fn point(main: f32, cross: f32) -> Offset { Offset::new(main, cross) }
}
