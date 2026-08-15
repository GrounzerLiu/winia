/// [`CornerRounding`](CornerRounding) defines the amount and quality of rounding applied at a vertex of a shape.
///
/// [`radius`](CornerRounding::radius) defines the radius of the circle that forms the basis of the rounding
/// at the corner. [`smoothing`](CornerRounding::smoothing) controls how much the curve is extended from the
/// circular arc toward the edges between adjacent vertices.
///
/// Each corner of a shape can be thought of as one of the following:
///
/// - **Unrounded**: the corner has a radius of `0` and no smoothing.
/// - **Rounded with a circular arc only**: smoothing is `0`. In this case, the rounding follows an
///   approximated circular arc between the edges connected to adjacent vertices.
/// - **Rounded with three curves**: an inner circular arc plus two symmetric flanking curves.
///   The flanking curves determine the transition from the inner arc to the edges. A smoothing
///   value of `0` produces a purely circular arc, while a value of `1` maximizes the flanking
///   curves such that they meet at the center with no inner circular arc.
///
/// # Fields
///
/// - `radius`: A value greater than or equal to `0` representing the radius of the circle defining
///   the inner rounding arc. A value of `0` indicates a sharp (unrounded) corner. This radius is an
///   absolute size and should be chosen relative to the coordinate space of the shape. If the shape
///   is transformed, the radius is scaled accordingly.
/// - `smoothing`: Controls how much the rounding curve extends from the inner circular arc toward
///   the edges between vertices. A value of `0` disables smoothing and uses only a circular arc. A
///   value of `1` removes the central circular arc entirely, causing the flanking curves to meet
///   at the center.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CornerRounding {
    ///   A value greater than or equal to `0` representing the radius of the circle defining
    ///   the inner rounding arc. A value of `0` indicates a sharp (unrounded) corner. This radius is an
    ///   absolute size and should be chosen relative to the coordinate space of the shape. If the shape
    ///   is transformed, the radius is scaled accordingly.
    pub radius: f32,
    ///   Controls how much the rounding curve extends from the inner circular arc toward
    ///   the edges between vertices. A value of `0` disables smoothing and uses only a circular arc. A
    ///   value of `1` removes the central circular arc entirely, causing the flanking curves to meet
    ///   at the center.
    pub smoothing: f32,
}

impl CornerRounding {
    /// [`UNROUNDED`] has a rounding radius of zero, producing a sharp corner at a vertex.
    pub const UNROUNDED: Self = Self {
        radius: 0.0,
        smoothing: 0.0,
    };

    pub fn new(radius: impl Into<Option<f32>>, smoothing: impl Into<Option<f32>>) -> Self {
        Self {
            radius: radius.into().unwrap_or(0.0),
            smoothing: smoothing.into().unwrap_or(0.0),
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn smoothing(mut self, smoothing: f32) -> Self {
        self.smoothing = smoothing;
        self
    }
}

#[cfg(test)]
mod corner_rounding_tests {
    use crate::corner_rounding::CornerRounding;

    #[test]
    fn corner_rounding_test() {
        let default_corner = CornerRounding::default();
        assert_eq!(default_corner.radius, 0.0);
        assert_eq!(default_corner.smoothing, 0.0);

        let unrounded = CornerRounding::UNROUNDED;
        assert_eq!(unrounded.radius, 0.0);
        assert_eq!(unrounded.smoothing, 0.0);

        let rounded = CornerRounding::new(5.0, None);
        assert_eq!(rounded.radius, 5.0);
        assert_eq!(rounded.smoothing, 0.0);

        let smoothed = CornerRounding::new(None, 0.5);
        assert_eq!(smoothed.radius, 0.0);
        assert_eq!(smoothed.smoothing, 0.5);

        let rounded_and_smoothed = CornerRounding::new(5.0, 0.5);
        assert_eq!(rounded_and_smoothed.radius, 5.0);
        assert_eq!(rounded_and_smoothed.smoothing, 0.5);
    }
}
