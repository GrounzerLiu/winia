use std::hash::Hash;
use crate::point::Point;
use crate::utils::{convex, direction_vector, distance, DISTANCE_EPSILON};

/**
 * This class holds the anchor and control point data for a single cubic Bézier curve, with anchor
 * points ([`anchor_0_x`](Cubic::anchor_0_x), [`anchor_0_y`](Cubic::anchor_0_y)) and ([`anchor_1_x`](Cubic::anchor_1_x), [`anchor_1_y`](Cubic::anchor_1_y)) at either end and control points
 * ([`control_0_x`](Cubic::control_0_x), [`control_0_y`](Cubic::control_0_y)) and ([`control_1_x`](Cubic::control_1_x), [`control_1_y`](Cubic::control_0_y)) determining the slope of the curve
 * between the anchor points.
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cubic {
    pub points: [f32; 8],
}

impl Hash for Cubic {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for point in &self.points {
            point.to_bits().hash(state);
        }
    }
}

impl Eq for Cubic {}
impl Cubic {
    pub fn new(
        anchor_0_x: f32,
        anchor_0_y: f32,
        control_0_x: f32,
        control_0_y: f32,
        control_1_x: f32,
        control_1_y: f32,
        anchor_1_x: f32,
        anchor_1_y: f32,
    ) -> Self {
        Self {
            points: [
                anchor_0_x,
                anchor_0_y,
                control_0_x,
                control_0_y,
                control_1_x,
                control_1_y,
                anchor_1_x,
                anchor_1_y,
            ],
        }
    }

    pub fn from_array(points: &[f32]) -> Self {
        assert_eq!(points.len(), 8);
        let mut arr = [0.0; 8];
        arr.copy_from_slice(&points[0..8]);
        Self { points: arr }
    }
    pub fn from_points(anchor_0: Point, control_0: Point, control_1: Point, anchor_1: Point) -> Self {
        Self::new(
            anchor_0.0,
            anchor_0.1,
            control_0.0,
            control_0.1,
            control_1.0,
            control_1.1,
            anchor_1.0,
            anchor_1.1,
        )
    }

    /// The first anchor point x coordinate
    pub fn anchor_0_x(&self) -> f32 {
        self.points[0]
    }
    /// The first anchor point y coordinate
    pub fn anchor_0_y(&self) -> f32 {
        self.points[1]
    }
    /// The first control point x coordinate
    pub fn control_0_x(&self) -> f32 {
        self.points[2]
    }
    /// The first control point y coordinate
    pub fn control_0_y(&self) -> f32 {
        self.points[3]
    }
    /// The second control point x coordinate
    pub fn control_1_x(&self) -> f32 {
        self.points[4]
    }
    /// The second control point y coordinate
    pub fn control_1_y(&self) -> f32 {
        self.points[5]
    }
    /// The second anchor point x coordinate
    pub fn anchor_1_x(&self) -> f32 {
        self.points[6]
    }
    /// The second anchor point y coordinate
    pub fn anchor_1_y(&self) -> f32 {
        self.points[7]
    }

    /// Returns a point on the curve for parameter `t`, representing the proportional distance along
    /// the curve between its starting point at `anchor_0` and ending point at `anchor_1`.
    ///
    /// `t` is the distance along the curve between the anchor points, where `0` is at `anchor_0` and
    /// `1` is at `anchor_1`.
    pub fn point_on_curve(&self, t: f32) -> Point {
        let u = 1.0 - t;
        Point(
            self.anchor_0_x() * (u * u * u)
                + self.control_0_x() * (3.0 * t * u * u)
                + self.control_1_x() * (3.0 * t * t * u)
                + self.anchor_1_x() * (t * t * t),
            self.anchor_0_y() * (u * u * u)
                + self.control_0_y() * (3.0 * t * u * u)
                + self.control_1_y() * (3.0 * t * t * u)
                + self.anchor_1_y() * (t * t * t),
        )
    }

    pub fn zero_length(&self) -> bool {
        (self.anchor_0_x() - self.anchor_1_x()).abs() < DISTANCE_EPSILON
            && (self.anchor_0_y() - self.anchor_1_y()).abs() < DISTANCE_EPSILON
    }

    pub fn convex_to(&self, next: &Cubic) -> bool {
        let prev_vertex = Point(self.anchor_0_x(), self.anchor_0_y());
        let curr_vertex = Point(self.anchor_1_x(), self.anchor_1_y());
        let next_vertex = Point(next.anchor_1_x(), next.anchor_1_y());
        convex(prev_vertex, curr_vertex, next_vertex)
    }

    pub fn zero_ish(value: f32) -> bool {
        value.abs() < DISTANCE_EPSILON
    }

    /// This function returns the true bounds of this curve, filling [bounds] with the axis-aligned
    /// bounding box values for left, top, right, and bottom, in that order.
    pub fn calculate_bounds(&self, bounds: &mut [f32; 4], approximate: impl Into<Option<bool>>) {
        let approximate = approximate.into().unwrap_or(false);
        // A curve might be of zero-length, with both anchors co-lated.
        // Just return the point itself.
        if self.zero_length() {
            bounds[0] = self.anchor_0_x();
            bounds[1] = self.anchor_0_y();
            bounds[2] = self.anchor_0_x();
            bounds[3] = self.anchor_0_y();
            return;
        }

        let mut min_x = self.anchor_0_x().min(self.anchor_1_x());
        let mut min_y = self.anchor_0_y().min(self.anchor_1_y());
        let mut max_x = self.anchor_0_x().max(self.anchor_1_x());
        let mut max_y = self.anchor_0_y().max(self.anchor_1_y());

        if approximate {
            // Approximate bounds use the bounding box of all anchors and controls
            bounds[0] = min_x.min(self.control_0_x().min(self.control_1_x()));
            bounds[1] = min_y.min(self.control_0_y().min(self.control_1_y()));
            bounds[2] = max_x.max(self.control_0_x().max(self.control_1_x()));
            bounds[3] = max_y.max(self.control_0_y().max(self.control_1_y()));
            return;
        }

        // Find the derivative, which is a quadratic Bezier. Then we can solve for t using
        // the quadratic formula
        let xa = -self.anchor_0_x() + 3.0 * self.control_0_x() - 3.0 * self.control_1_x() + self.anchor_1_x();
        let xb = 2.0 * self.anchor_0_x() - 4.0 * self.control_0_x() + 2.0 * self.control_1_x();
        let xc = -self.anchor_0_x() + self.control_0_x();

        if Self::zero_ish(xa) {
            // Try Muller's method instead; it can find a single root when a is 0
            if xb != 0.0 {
                let t = 2.0 * xc / (-2.0 * xb);
                if t >= 0.0 && t <= 1.0 {
                    let x = self.point_on_curve(t).0;
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                }
            }
        } else {
            let xs = xb * xb - 4.0 * xa * xc;
            if xs >= 0.0 {
                let t1 = (-xb + xs.sqrt()) / (2.0 * xa);
                if t1 >= 0.0 && t1 <= 1.0 {
                    let x = self.point_on_curve(t1).0;
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                }

                let t2 = (-xb - xs.sqrt()) / (2.0 * xa);
                if t2 >= 0.0 && t2 <= 1.0 {
                    let x = self.point_on_curve(t2).0;
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                }
            }
        }

        // Repeat the above for y coordinate
        let ya = -self.anchor_0_y() + 3.0 * self.control_0_y() - 3.0 * self.control_1_y() + self.anchor_1_y();
        let yb = 2.0 * self.anchor_0_y() - 4.0 * self.control_0_y() + 2.0 * self.control_1_y();
        let yc = -self.anchor_0_y() + self.control_0_y();

        if Self::zero_ish(ya) {
            if yb != 0.0 {
                let t = 2.0 * yc / (-2.0 * yb);
                if t >= 0.0 && t <= 1.0 {
                    let y = self.point_on_curve(t).1;
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        } else {
            let ys = yb * yb - 4.0 * ya * yc;
            if ys >= 0.0 {
                let t1 = (-yb + ys.sqrt()) / (2.0 * ya);
                if t1 >= 0.0 && t1 <= 1.0 {
                    let y = self.point_on_curve(t1).1;
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }

                let t2 = (-yb - ys.sqrt()) / (2.0 * ya);
                if t2 >= 0.0 && t2 <= 1.0 {
                    let y = self.point_on_curve(t2).1;
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
        bounds[0] = min_x;
        bounds[1] = min_y;
        bounds[2] = max_x;
        bounds[3] = max_y;
    }

    /// Returns two Cubics, created by splitting this curve at the given distance of `t` between
    /// the original starting and ending anchor points.
    pub fn split(&self, t: f32) -> (Cubic, Cubic) {
        let u = 1.0 - t;
        let point_on_curve = self.point_on_curve(t);

        let first = Cubic::new(
            self.anchor_0_x(),
            self.anchor_0_y(),

            self.anchor_0_x() * u + self.control_0_x() * t,
            self.anchor_0_y() * u + self.control_0_y() * t,

            self.anchor_0_x() * (u * u)
                + self.control_0_x() * (2.0 * u * t)
                + self.control_1_x() * (t * t),
            self.anchor_0_y() * (u * u)
                + self.control_0_y() * (2.0 * u * t)
                + self.control_1_y() * (t * t),

            point_on_curve.x(),
            point_on_curve.y(),
        );

        let second = Cubic::new(
            point_on_curve.x(),
            point_on_curve.y(),

            self.control_0_x() * (u * u)
                + self.control_1_x() * (2.0 * u * t)
                + self.anchor_1_x() * (t * t),
            self.control_0_y() * (u * u)
                + self.control_1_y() * (2.0 * u * t)
                + self.anchor_1_y() * (t * t),

            self.control_1_x() * u + self.anchor_1_x() * t,
            self.control_1_y() * u + self.anchor_1_y() * t,

            self.anchor_1_x(),
            self.anchor_1_y(),
        );

        (first, second)
    }

    /// Utility function to reverse the control/anchor points for this curve.
    pub fn reverse(&self) -> Cubic {
        Cubic::new(
            self.anchor_1_x(),
            self.anchor_1_y(),
            self.control_1_x(),
            self.control_1_y(),
            self.control_0_x(),
            self.control_0_y(),
            self.anchor_0_x(),
            self.anchor_0_y(),
        )
    }

    /// Transforms the points in this [`Cubic`] with the given [`PointTransformer`] and returns a new
    /// [`Cubic`].
    ///
    /// `f` is the [`PointTransformer`] used to transform this [`Cubic`].
    pub fn transformed(&self, f: &PointTransformer) -> Cubic {
        let mut new_cubic = self.clone();
        new_cubic.transform(f);
        new_cubic
    }

    /// Generates a Bézier curve that is a straight line between the given anchor points. The
    /// control points lie 1/3 of the distance from their respective anchor points.
    pub fn straight_line(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> Cubic {
        Cubic::new(
            x0,
            y0,
            crate::utils::interpolate(x0, x1, 1.0 / 3.0),
            crate::utils::interpolate(y0, y1, 1.0 / 3.0),
            crate::utils::interpolate(x0, x1, 2.0 / 3.0),
            crate::utils::interpolate(y0, y1, 2.0 / 3.0),
            x1,
            y1,
        )
    }
    /// Generates a Bézier curve that approximates a circular arc, with p0 and p1 as the starting
    /// and ending anchor points. The curve generated is the smallest of the two possible arcs
    /// around the entire 360-degree circle. Arcs of greater than 180 degrees should use more
    /// than one arc together. Note that p0 and p1 should be equidistant from the center.
    pub fn circular_arc(
        center_x: f32,
        center_y: f32,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    ) -> Cubic {
        let p0d = direction_vector(x0 - center_x, y0 - center_y);
        let p1d = direction_vector(x1 - center_x, y1 - center_y);
        let rotated_p0 = p0d.rotate_90();
        let rotated_p1 = p1d.rotate_90();
        let clockwise = rotated_p0.dot_product_xy(x1 - center_x, y1 - center_y) >= 0.0;
        let cosa = p0d.dot_product(p1d);
        if cosa > 0.999 {
            return Self::straight_line(x0, y0, x1, y1);
        }
        let k = distance(x0 - center_x, y0 - center_y) * 4.0 / 3.0 *
            ((2.0 * (1.0 - cosa)).sqrt() - (1.0 - cosa * cosa).sqrt()) / (1.0 - cosa) *
            if clockwise { 1.0 } else { -1.0 };
        Cubic::new(
            x0,
            y0,
            x0 + rotated_p0.0 * k,
            y0 + rotated_p0.1 * k,
            x1 - rotated_p1.0 * k,
            y1 - rotated_p1.1 * k,
            x1,
            y1,
        )
    }

    /// Generates an empty Cubic defined at (x0, y0)
    pub fn empty(x0: f32, y0: f32) -> Cubic {
        Cubic::new(x0, y0, x0, y0, x0, y0, x0, y0)
    }

    fn transform_one_point(
        &mut self,
        f: &PointTransformer,
        ix: usize
    ) {
        let result = f(self.points[ix], self.points[ix + 1]);
        self.points[ix] = result.0;
        self.points[ix + 1] = result.1;
    }

    pub fn transform(&mut self, f: &PointTransformer) {
        self.transform_one_point(f, 0);
        self.transform_one_point(f, 2);
        self.transform_one_point(f, 4);
        self.transform_one_point(f, 6);
    }

    pub fn interpolate(&mut self, c1: &Cubic, c2: &Cubic, progress: f32) {
        for i in 0..8 {
            self.points[i] = crate::utils::interpolate(c1.points[i], c2.points[i], progress);
        }
    }
}

// pub trait PointTransformer {
//     fn transform(&self, x: f32, y: f32) -> Point;
// }
pub type PointTransformer = dyn Fn(f32, f32) -> Point;


impl std::ops::Add for Cubic {
    type Output = Cubic;

    fn add(self, rhs: Self) -> Self::Output {
        Cubic::new(
            self.anchor_0_x() + rhs.anchor_0_x(),
            self.anchor_0_y() + rhs.anchor_0_y(),
            self.control_0_x() + rhs.control_0_x(),
            self.control_0_y() + rhs.control_0_y(),
            self.control_1_x() + rhs.control_1_x(),
            self.control_1_y() + rhs.control_1_y(),
            self.anchor_1_x() + rhs.anchor_1_x(),
            self.anchor_1_y() + rhs.anchor_1_y(),
        )
    }
}
impl std::ops::Mul<f32> for Cubic {
    type Output = Cubic;

    fn mul(self, rhs: f32) -> Self::Output {
        Cubic::new(
            self.anchor_0_x() * rhs,
            self.anchor_0_y() * rhs,
            self.control_0_x() * rhs,
            self.control_0_y() * rhs,
            self.control_1_x() * rhs,
            self.control_1_y() * rhs,
            self.anchor_1_x() * rhs,
            self.anchor_1_y() * rhs,
        )
    }
}
impl std::ops::Mul<usize> for Cubic {
    type Output = Cubic;

    fn mul(self, rhs: usize) -> Self::Output {
        let r = rhs as f32;
        self * r
    }
}
impl std::ops::Mul<isize> for Cubic {
    type Output = Cubic;

    fn mul(self, rhs: isize) -> Self::Output {
        let r = rhs as f32;
        self * r
    }
}
impl std::ops::Mul<i32> for Cubic {
    type Output = Cubic;

    fn mul(self, rhs: i32) -> Self::Output {
        let r = rhs as f32;
        self * r
    }
}
impl std::ops::Div<f32> for Cubic {
    type Output = Cubic;

    fn div(self, rhs: f32) -> Self::Output {
        self * (1.0 / rhs)
    }
}
impl std::ops::Div<usize> for Cubic {
    type Output = Cubic;

    fn div(self, rhs: usize) -> Self::Output {
        let r = rhs as f32;
        self / r
    }
}
impl std::ops::Div<isize> for Cubic {
    type Output = Cubic;
    fn div(self, rhs: isize) -> Self::Output {
        let r = rhs as f32;
        self / r
    }
}

impl std::ops::Div<i32> for Cubic {
    type Output = Cubic;
    fn div(self, rhs: i32) -> Self::Output {
        let r = rhs as f32;
        self / r
    }
}

#[cfg(test)]
mod cubic_tests {
    use std::ops::Deref;
    use lazy_static::lazy_static;
    use crate::cubic::Cubic;
    use crate::point::Point;
    use crate::tests::{identity_transform, scale_transform, translate_transform};

    const ZERO: Point = Point(0.0, 0.0);
    const P0: Point = Point(1.0, 0.0);
    const P1: Point = Point(1.0, 0.5);
    const P2: Point = Point(0.5, 1.0);
    const P3: Point = Point(0.0, 1.0);
    // fn cubic() -> Cubic {
    //     Cubic::from_points(P0, P1, P2, P3)
    // }
    lazy_static!(
        static ref CUBIC: Cubic = Cubic::from_points(P0, P1, P2, P3);
    );

    #[test]
    fn construction_test() {
        assert_eq!(Point(CUBIC.anchor_0_x(), CUBIC.anchor_0_y()), P0);
        assert_eq!(Point(CUBIC.control_0_x(), CUBIC.control_0_y()), P1);
        assert_eq!(Point(CUBIC.control_1_x(), CUBIC.control_1_y()), P2);
        assert_eq!(Point(CUBIC.anchor_1_x(), CUBIC.anchor_1_y()), P3);
    }

    #[test]
    fn circular_arc_test() {
        let arc_cubic = Cubic::circular_arc(ZERO.x(), ZERO.y(), P0.x(), P0.y(), P3.x(), P3.y());
        assert_eq!(Point(arc_cubic.anchor_0_x(), arc_cubic.anchor_0_y()), P0);
        assert_eq!(Point(arc_cubic.anchor_1_x(), arc_cubic.anchor_1_y()), P3);
    }

    #[test]
    fn div_test() {
        let mut div_cubic = *CUBIC / 1.0;
        assert_eq!(div_cubic, *CUBIC);
        div_cubic = *CUBIC / 1;
        assert_eq!(div_cubic, *CUBIC);
        div_cubic = *CUBIC / 2.0;
        assert_eq!(Point(div_cubic.anchor_0_x(), div_cubic.anchor_0_y()), P0 / 2.0);
        assert_eq!(Point(div_cubic.control_0_x(), div_cubic.control_0_y()), P1 / 2.0);
        assert_eq!(Point(div_cubic.control_1_x(), div_cubic.control_1_y()), P2 / 2.0);
        assert_eq!(Point(div_cubic.anchor_1_x(), div_cubic.anchor_1_y()), P3 / 2.0);
        div_cubic = *CUBIC / 2;
        assert_eq!(Point(div_cubic.anchor_0_x(), div_cubic.anchor_0_y()), P0 / 2.0);
        assert_eq!(Point(div_cubic.control_0_x(), div_cubic.control_0_y()), P1 / 2.0);
        assert_eq!(Point(div_cubic.control_1_x(), div_cubic.control_1_y()), P2 / 2.0);
        assert_eq!(Point(div_cubic.anchor_1_x(), div_cubic.anchor_1_y()), P3 / 2.0);
    }

    #[test]
    fn mul_test() {
        let mut mul_cubic = *CUBIC * 1.0;
        assert_eq!(Point(mul_cubic.anchor_0_x(), mul_cubic.anchor_0_y()), P0);
        assert_eq!(Point(mul_cubic.control_0_x(), mul_cubic.control_0_y()), P1);
        assert_eq!(Point(mul_cubic.control_1_x(), mul_cubic.control_1_y()), P2);
        assert_eq!(Point(mul_cubic.anchor_1_x(), mul_cubic.anchor_1_y()), P3);
        mul_cubic = *CUBIC * 1;
        assert_eq!(Point(mul_cubic.anchor_0_x(), mul_cubic.anchor_0_y()), P0);
        assert_eq!(Point(mul_cubic.control_0_x(), mul_cubic.control_0_y()), P1);
        assert_eq!(Point(mul_cubic.control_1_x(), mul_cubic.control_1_y()), P2);
        assert_eq!(Point(mul_cubic.anchor_1_x(), mul_cubic.anchor_1_y()), P3);
        mul_cubic = *CUBIC * 2.0;
        assert_eq!(Point(mul_cubic.anchor_0_x(), mul_cubic.anchor_0_y()), P0 * 2.0);
        assert_eq!(Point(mul_cubic.control_0_x(), mul_cubic.control_0_y()), P1 * 2.0);
        assert_eq!(Point(mul_cubic.control_1_x(), mul_cubic.control_1_y()), P2 * 2.0);
        assert_eq!(Point(mul_cubic.anchor_1_x(), mul_cubic.anchor_1_y()), P3 * 2.0);
        mul_cubic = *CUBIC * 2;
        assert_eq!(Point(mul_cubic.anchor_0_x(), mul_cubic.anchor_0_y()), P0 * 2.0);
        assert_eq!(Point(mul_cubic.control_0_x(), mul_cubic.control_0_y()), P1 * 2.0);
        assert_eq!(Point(mul_cubic.control_1_x(), mul_cubic.control_1_y()), P2 * 2.0);
        assert_eq!(Point(mul_cubic.anchor_1_x(), mul_cubic.anchor_1_y()), P3 * 2.0);
    }

    #[test]
    fn add_test() {
        let offset_cubic = *CUBIC * 2.0;
        let add_cubic = *CUBIC + offset_cubic;
        assert_eq!(
            Point(add_cubic.anchor_0_x(), add_cubic.anchor_0_y()),
            P0 + Point(offset_cubic.anchor_0_x(), offset_cubic.anchor_0_y())
        );
        assert_eq!(
            Point(add_cubic.control_0_x(), add_cubic.control_0_y()),
            P1 + Point(offset_cubic.control_0_x(), offset_cubic.control_0_y())
        );
        assert_eq!(
            Point(add_cubic.control_1_x(), add_cubic.control_1_y()),
            P2 + Point(offset_cubic.control_1_x(), offset_cubic.control_1_y())
        );
        assert_eq!(
            Point(add_cubic.anchor_1_x(), add_cubic.anchor_1_y()),
            P3 + Point(offset_cubic.anchor_1_x(), offset_cubic.anchor_1_y())
        );
    }

    #[test]
    fn reverse_test() {
        let reversed_cubic = CUBIC.reverse();
        assert_eq!(
            Point(reversed_cubic.anchor_0_x(), reversed_cubic.anchor_0_y()),
            P3
        );
        assert_eq!(
            Point(reversed_cubic.control_0_x(), reversed_cubic.control_0_y()),
            P2
        );
        assert_eq!(
            Point(reversed_cubic.control_1_x(), reversed_cubic.control_1_y()),
            P1
        );
        assert_eq!(
            Point(reversed_cubic.anchor_1_x(), reversed_cubic.anchor_1_y()),
            P0
        );
    }

    fn assert_between(end0: Point, end1: Point, actual: Point) {
        let min_x = end0.x().min(end1.x());
        let min_y = end0.y().min(end1.y());
        let max_x = end0.x().max(end1.x());
        let max_y = end0.y().max(end1.y());
        assert!(min_x <= actual.x());
        assert!(min_y <= actual.y());
        assert!(max_x >= actual.x());
        assert!(max_y >= actual.y());
    }

    #[test]
    fn straight_line_test() {
        let line_cubic = Cubic::straight_line(P0.x(), P0.y(), P3.x(), P3.y());
        assert_eq!(Point(line_cubic.anchor_0_x(), line_cubic.anchor_0_y()), P0);
        assert_eq!(Point(line_cubic.anchor_1_x(), line_cubic.anchor_1_y()), P3);
        assert_between(
            P0,
            P3,
            Point(line_cubic.control_0_x(), line_cubic.control_0_y())
        );
        assert_between(
            P0,
            P3,
            Point(line_cubic.control_1_x(), line_cubic.control_1_y())
        );
    }

    #[test]
    fn split_test() {
        let (split0, split1) = CUBIC.split(0.5);
        assert_eq!(
            Point(split0.anchor_0_x(), split0.anchor_0_y()),
            Point(CUBIC.anchor_0_x(), CUBIC.anchor_0_y())
        );
        assert_eq!(
            Point(split1.anchor_1_x(), split1.anchor_1_y()),
            Point(CUBIC.anchor_1_x(), CUBIC.anchor_1_y())
        );
        assert_between(
            Point(CUBIC.anchor_0_x(), CUBIC.anchor_0_y()),
            Point(CUBIC.anchor_1_x(), CUBIC.anchor_1_y()),
            Point(split0.anchor_1_x(), split0.anchor_1_y())
        );
        assert_between(
            Point(CUBIC.anchor_0_x(), CUBIC.anchor_0_y()),
            Point(CUBIC.anchor_1_x(), CUBIC.anchor_1_y()),
            Point(split1.anchor_0_x(), split1.anchor_0_y())
        );
    }
    #[test]
    fn point_on_curve_test() {
        let mut halfway = CUBIC.point_on_curve(0.5);
        assert_between(
            Point(CUBIC.anchor_0_x(), CUBIC.anchor_0_y()),
            Point(CUBIC.anchor_1_x(), CUBIC.anchor_1_y()),
            halfway,
        );
        let straight_line_cubic =
            Cubic::straight_line(P0.x(), P0.y(), P3.x(), P3.y());
        halfway = straight_line_cubic.point_on_curve(0.5);
        let computed_halfway = Point(
            P0.x() + 0.5 * (P3.x() - P0.x()),
            P0.y() + 0.5 * (P3.y() - P0.y()),
        );
        assert_eq!(halfway, computed_halfway);
    }
    #[test]
    fn transform_test() {
        let mut transform = identity_transform();
        let mut transformed_cubic = CUBIC.transformed(transform.deref());
        assert_eq!(*CUBIC, transformed_cubic);

        transform = scale_transform(3.0, 3.0);
        transformed_cubic = CUBIC.transformed(transform.deref());
        assert_eq!(*CUBIC * 3.0, transformed_cubic);

        let tx = 200.0;
        let ty = 300.0;
        let translation_vector = Point(tx, ty);
        transform = translate_transform(tx, ty);
        transformed_cubic = CUBIC.transformed(transform.deref());
        assert_eq!(
            Point(CUBIC.anchor_0_x(), CUBIC.anchor_0_y()) + translation_vector,
            Point(transformed_cubic.anchor_0_x(), transformed_cubic.anchor_0_y())
        );
        assert_eq!(
            Point(CUBIC.control_0_x(), CUBIC.control_0_y()) + translation_vector,
            Point(transformed_cubic.control_0_x(), transformed_cubic.control_0_y())
        );
        assert_eq!(
            Point(CUBIC.control_1_x(), CUBIC.control_1_y()) + translation_vector,
            Point(transformed_cubic.control_1_x(), transformed_cubic.control_1_y())
        );
        assert_eq!(
            Point(CUBIC.anchor_1_x(), CUBIC.anchor_1_y()) + translation_vector,
            Point(transformed_cubic.anchor_1_x(), transformed_cubic.anchor_1_y())
        );
    }
    #[test]
    fn empty_cubic_has_zero_length() {
        assert!(Cubic::empty(10.0, 10.0).zero_length());
    }
}