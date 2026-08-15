use crate::feature::FeatureTrait;
use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};

/// Utility struct to fix invalid [`RoundedPolygon`]s that would otherwise break [`Morph`](crate::morph::Morph)s in
/// one way or another, as [`RoundedPolygon`] assumes correct input. Correct input means:
///
/// - Closed geometry
/// - Clockwise orientation of points
/// - No self-intersections
/// - No holes
/// - Single polygon
pub struct PolygonValidator;

impl PolygonValidator {
    // TODO: b/372000685 b/372003785 b/372004969
    // Update docs when other validations are implemented
    /// Validates whether this [`RoundedPolygon`]’s orientation is clockwise and fixes it if necessary.
    ///
    /// `polygon` — the [`RoundedPolygon`] to validate.
    ///
    /// Returns a new [`RoundedPolygon`] with fixed orientation, or the same [`RoundedPolygon`] as
    /// given when it was already valid.
    pub fn fix(polygon: RoundedPolygon) -> RoundedPolygon {
        let mut result = polygon;

        if !Self::is_cw_oriented(&result) {
            result = Self::fix_cw_orientation(&result);
        }

        result
    }

    fn is_cw_oriented(polygon: &RoundedPolygon) -> bool {
        let mut signed_area = 0.0;

        for cubic in polygon.cubics.iter() {
            signed_area += (cubic.anchor_1_x() - cubic.anchor_0_x())
                * (cubic.anchor_1_y() + cubic.anchor_0_y())
        }

        signed_area < 0.0
    }

    fn fix_cw_orientation(polygon: &RoundedPolygon) -> RoundedPolygon {
        let mut reversed_features = Vec::with_capacity(polygon.features.len());
        // Persist first feature to stay a Corner
        reversed_features.push(polygon.features[0].reversed().clone());
        for feature in polygon.features.iter().skip(1).rev() {
            reversed_features.push(feature.reversed());
        }

        RoundedPolygon::new(reversed_features, polygon.center)
    }
}

#[cfg(test)]
mod polygon_validation_tests {
    use crate::corner_rounding::CornerRounding;
    use crate::polygon_validation::PolygonValidator;
    use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
    use crate::tests::assert_polygons_equalish;

    const PENTAGON_POINTS: [f32; 10] = [0.2, 0.0, 0.8, 0.0, 1.0, 0.6, 0.5, 1.0, 0.0, 0.6];
    const REVERSE_ORIENTED_PENTAGON_POINTS: [f32; 10] =
        [0.2, 0.0, 0.0, 0.6, 0.5, 1.0, 1.0, 0.6, 0.8, 0.0];

    #[test]
    fn does_not_fix_valid_sharp_polygon() {
        stays_unchanged(RoundedPolygonBuilder::from_num_vertices(5).build());
    }
    #[test]
    fn does_not_fix_valid_rounded_polygon() {
        stays_unchanged(
            RoundedPolygonBuilder::from_num_vertices(5)
                .rounding(CornerRounding::new(0.5, None))
                .build(),
        );
    }
    #[test]
    fn fies_anti_clockwise_oriented_polygon_sharp() {
        let valid = RoundedPolygonBuilder::from_vertices(&PENTAGON_POINTS).build();

        let broken = RoundedPolygonBuilder::from_vertices(&REVERSE_ORIENTED_PENTAGON_POINTS)
            .build();

        fixes(broken, &valid);
    }
    #[test]
    fn fies_anti_clockwise_oriented_rounded_polygon() {
        let valid = RoundedPolygonBuilder::from_vertices(&PENTAGON_POINTS)
            .rounding(CornerRounding::new(0.5, None))
            .build();

        let broken = RoundedPolygonBuilder::from_vertices(&REVERSE_ORIENTED_PENTAGON_POINTS)
            .rounding(CornerRounding::new(0.5, None))
            .build();

        fixes(broken, &valid);
    }
    fn stays_unchanged(polygon: RoundedPolygon) {
        let copy = RoundedPolygon::from_polygon(&polygon);
        let fixed_polygon = PolygonValidator::fix(polygon);

        assert_polygons_equalish(&copy, &fixed_polygon);
    }
    fn fixes(broken: RoundedPolygon, expected: &RoundedPolygon) {
        let fixed = PolygonValidator::fix(broken);

        assert_polygons_equalish(expected, &fixed);
    }
}