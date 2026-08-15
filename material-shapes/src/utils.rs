use crate::point::Point;
/// These epsilon values are used internally to determine when two points are the same, within some reasonable roundoff error. The distance epsilon is smaller, with the intention that the roundoff should not be larger than a pixel on any reasonable sized display.
pub const DISTANCE_EPSILON: f32 = 1e-4f32;
pub const ANGLE_EPSILON: f32 = 1e-6f32;
pub const ZERO: Point = Point(0.0, 0.0);
pub const RELAXED_DISTANCE_EPSILON: f32 = 5e-3f32;
pub const FLOAT_PI: f32 = std::f32::consts::PI;
pub const TWO_PI: f32 = std::f32::consts::PI * 2.0;

pub fn require(b: bool, message: impl AsRef<str>) {
    let message = message.as_ref();
    if !b {
        panic!("{}", message);
    }
}

pub fn distance(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}
pub fn distance_squared(x: f32, y: f32) -> f32 {
    x * x + y * y
}
pub fn direction_vector(x: f32, y: f32) -> Point {
    let d = distance(x, y);
    require(d > 0.0, "Required direction greater than zero");
    Point(x / d, y / d)
}
pub fn direction_vector_from_angle(angle_radians: f32) -> Point {
    Point(angle_radians.cos(), angle_radians.sin())
}
pub fn angle(x: f32, y: f32) -> f32 {
    (y.atan2(x) + TWO_PI) % TWO_PI
}
pub fn radial_to_cartesian(radius: f32, angle_radians: f32, center: impl Into<Option<Point>>) -> Point {
    let center = center.into();
    direction_vector_from_angle(angle_radians) * radius + center.unwrap_or(ZERO)
}
pub fn square(x: f32) -> f32 {
    x * x
}
/// Linearly interpolate between start and stop with fraction between them.
pub fn interpolate(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

/// Similar to num % mod, but ensures the result is always positive. For example: 4 % 3 =
/// positive_modulo(4, 3) = 1, but: -4 % 3 = -1 positive_modulo(-4, 3) = 2

pub fn positive_modulus(num: f32, modulus: f32) -> f32 {
    ((num % modulus) + modulus) % modulus
}
/// Returns whether C is on the line defined by the two points AB
pub fn collinear_ish(
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
    cx: f32,
    cy: f32,
    tolerance: impl Into<Option<f32>>,
) -> bool {
    // The dot product of a perpendicular angle is 0. By rotating one of the vectors,
    // we save the calculations to convert the dot product to degrees afterwards.
    let ab = Point(bx - ax, by - ay).rotate_90();
    let ac = Point(cx - ax, cy - ay);
    let dot_product = ab.dot_product(ac).abs();
    let tolerance = tolerance.into().unwrap_or(DISTANCE_EPSILON);
    let relative_tolerance = tolerance * ab.get_distance() * ac.get_distance();
    dot_product < tolerance || dot_product < relative_tolerance
}

/// Approximates whether corner at this vertex is concave or convex, based on the relationship of the
/// prev->curr/curr->next vectors.
pub fn convex(previous: Point, current: Point, next: Point) -> bool {
    // TODO: b/369320447 - This is a fast, but not reliable calculation.
    (current - previous).clockwise(&(next - current))
}

/*
 * Does a ternary search in [v0..v1] to find the parameter that minimizes the given function.
 * Stops when the search space size is reduced below the given tolerance.
 *
 * NTS: Does it make sense to split the function f in 2, one to generate a candidate, of a custom
 * type T (i.e. (Float) -> T), and one to evaluate it ( (T) -> Float )?
 */
pub fn find_minimum(
    v0: f32,
    v1: f32,
    tolerance: impl Into<Option<f32>>,
    f: impl FindMinimumFunction,
) -> f32 {
    let tolerance = tolerance.into().unwrap_or(1e-3f32);
    let mut a = v0;
    let mut b = v1;
    while b - a > tolerance {
        let c1 = (2.0 * a + b) / 3.0;
        let c2 = (2.0 * b + a) / 3.0;
        if f.invoke(c1) < f.invoke(c2) {
            b = c2;
        } else {
            a = c1;
        }
    }
    (a + b) / 2.0
}

/// A functional interface for computing a Float value when finding the minimum at [`find_minimum`].
pub trait FindMinimumFunction {
    fn invoke(&self, value: f32) -> f32;
}