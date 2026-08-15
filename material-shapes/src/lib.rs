mod corner_rounding;
mod cubic;
mod feature;
mod feature_detector;
mod feature_mapping;
mod feature_serializer;
mod float_mapping;
mod point;
mod polygon_measure;
mod rounded_polygon;
mod shapes;
mod utils;
mod polygon_validation;
mod morph;
mod material_shapes;
mod matrix;
mod offset;
mod rect;

pub use material_shapes::*;
pub use polygon_measure::*;
pub use rounded_polygon::*;
pub use shapes::*;
pub use morph::*;

#[cfg(feature = "shape_util")]
mod shape_util;
#[cfg(feature = "shape_util")]
pub use shape_util::*;

#[cfg(test)]
pub mod tests {
    use crate::cubic::{Cubic, PointTransformer};
    use crate::feature::{Feature, FeatureTrait};
    use crate::point::Point;
    use crate::rounded_polygon::RoundedPolygon;

    pub const EPSILON: f32 = 1e-4f32;

    pub fn identity_transform() -> Box<PointTransformer> {
        Box::new(|x: f32, y: f32| -> Point { Point(x, y) })
    }

    pub fn scale_transform(sx: f32, sy: f32) -> Box<PointTransformer> {
        Box::new(move |x: f32, y: f32| -> Point { Point(x * sx, y * sy) })
    }

    pub fn translate_transform(tx: f32, ty: f32) -> Box<PointTransformer> {
        Box::new(move |x: f32, y: f32| -> Point { Point(x + tx, y + ty) })
    }

    pub fn assert_floats_equalish<'a>(
        expected: f32,
        actual: f32,
        epsilon: impl Into<Option<f32>>,
        msg: impl Into<Option<&'a str>>,
    ) {
        // let msg = format!("{} vs. {}", expected, actual);
        let default_msg = format!("{:?} vs. {:?}", expected, actual);
        assert!(
            equalish(expected, actual, epsilon.into().unwrap_or(EPSILON)),
            "{}",
            msg.into().unwrap_or(&default_msg)
        );
    }

    pub fn assert_points_equalish(expected: Point, actual: Point) {
        let msg = format!("{:?} vs. {:?}", expected, actual);
        assert!(equalish(expected.x(), actual.x(), EPSILON), "{}", msg);
        assert!(equalish(expected.y(), actual.y(), EPSILON), "{}", msg);
    }

    pub fn equalish(f0: f32, f1: f32, epsilon: f32) -> bool {
        (f0 - f1).abs() < epsilon
    }

    pub fn points_equalish(expected: Point, actual: Point) -> bool {
        equalish(expected.x(), actual.x(), EPSILON) && equalish(expected.y(), actual.y(), EPSILON)
    }

    pub fn cubics_equalish(expected: &Cubic, actual: &Cubic) -> bool {
        points_equalish(
            Point(expected.anchor_0_x(), expected.anchor_0_y()),
            Point(actual.anchor_0_x(), actual.anchor_0_y()),
        ) && points_equalish(
            Point(expected.control_0_x(), expected.control_0_y()),
            Point(actual.control_0_x(), actual.control_0_y()),
        ) && points_equalish(
            Point(expected.control_1_x(), expected.control_1_y()),
            Point(actual.control_1_x(), actual.control_1_y()),
        ) && points_equalish(
            Point(expected.anchor_1_x(), expected.anchor_1_y()),
            Point(actual.anchor_1_x(), actual.anchor_1_y()),
        )
    }

    pub fn assert_cubics_equalish(expected: &Cubic, actual: &Cubic) {
        assert_points_equalish(
            Point(expected.anchor_0_x(), expected.anchor_0_y()),
            Point(actual.anchor_0_x(), actual.anchor_0_y()),
        );
        assert_points_equalish(
            Point(expected.control_0_x(), expected.control_0_y()),
            Point(actual.control_0_x(), actual.control_0_y()),
        );
        assert_points_equalish(
            Point(expected.control_1_x(), expected.control_1_y()),
            Point(actual.control_1_x(), actual.control_1_y()),
        );
        assert_points_equalish(
            Point(expected.anchor_1_x(), expected.anchor_1_y()),
            Point(actual.anchor_1_x(), actual.anchor_1_y()),
        );
    }

    pub fn assert_cubic_lists_equalish(expected: &Vec<Cubic>, actual: &Vec<Cubic>) {
        assert_eq!(expected.len(), actual.len());
        for i in 0..expected.len() {
            assert_cubics_equalish(&expected[i], &actual[i]);
        }
    }

    pub fn assert_features_equalish(expected: &Feature, actual: &Feature) {
        assert_cubic_lists_equalish(expected.cubics(), actual.cubics());
        assert!(match (expected, actual) {
            (Feature::Corner(c1), Feature::Corner(c2)) => c1.convex == c2.convex,
            (Feature::Edge(_), Feature::Edge(_)) => true,
            _ => false,
        });
    }

    pub fn assert_polygons_equalish(expected: &RoundedPolygon, actual: &RoundedPolygon) {
        assert_cubic_lists_equalish(&expected.cubics, &actual.cubics);
        assert_eq!(expected.features.len(), actual.features.len());
        for i in 0..expected.features.len() {
            assert_features_equalish(&expected.features[i], &actual.features[i]);
        }
    }

    pub fn assert_point_greaterish(expected: Point, actual: Point) {
        assert!(actual.x() >= expected.x() - EPSILON);
        assert!(actual.y() >= expected.y() - EPSILON);
    }

    pub fn assert_point_lessish(expected: Point, actual: Point) {
        assert!(actual.x() <= expected.x() + EPSILON);
        assert!(actual.y() <= expected.y() + EPSILON);
    }

    pub fn assert_in_bounds(shape: &Vec<Cubic>, min_point: Point, max_point: Point) {
        for cubic in shape.iter() {
            assert_point_greaterish(min_point, Point(cubic.anchor_0_x(), cubic.anchor_0_y()));
            assert_point_lessish(max_point, Point(cubic.anchor_0_x(), cubic.anchor_0_y()));
            assert_point_greaterish(min_point, Point(cubic.control_0_x(), cubic.control_0_y()));
            assert_point_lessish(max_point, Point(cubic.control_0_x(), cubic.control_0_y()));
            assert_point_greaterish(min_point, Point(cubic.control_1_x(), cubic.control_1_y()));
            assert_point_lessish(max_point, Point(cubic.control_1_x(), cubic.control_1_y()));
            assert_point_greaterish(min_point, Point(cubic.anchor_1_x(), cubic.anchor_1_y()));
            assert_point_lessish(max_point, Point(cubic.anchor_1_x(), cubic.anchor_1_y()));
        }
    }

    #[macro_export]
    macro_rules! assert_equalish {
        ($expected:expr, $actual:expr) => {
            $crate::tests::assert_floats_equalish($expected, $actual, None, None);
        };
        ($expected:expr, $actual:expr, $epsilon:expr) => {
            $crate::tests::assert_floats_equalish($expected, $actual, $epsilon, None);
        };
        ($expected:expr, $actual:expr, $epsilon:expr, $msg:expr) => {
            $crate::tests::assert_floats_equalish($expected, $actual, $epsilon, $msg);
        };
    }
}

#[macro_export]
macro_rules! assert_panic {
    ($code:block) => {{
        let old_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_info| {}));
        let result = std::panic::catch_unwind(|| $code);
        std::panic::set_hook(old_hook);
        assert!(
            result.is_err(),
            "Expected panic, but code executed without panicking"
        );
    }};
}
