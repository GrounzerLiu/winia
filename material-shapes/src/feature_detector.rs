use crate::cubic::Cubic;
use crate::feature::Feature;
use crate::utils::{collinear_ish, RELAXED_DISTANCE_EPSILON};

/// Convert cubics to Features in a 1:1 mapping of [`Cubic::as_feature`] unless
/// - two subsequent cubics are not continuous, in which case an empty corner needs to be added in
///   between. Example for C1, C2: /C1\/C2\ -> /C\C/C\.
/// - multiple subsequent cubics can be expressed as a single feature. Example for C1, C2:
///   --C1----C2-- -> -----E----. One exception to the latter rule is for the first and last cubic,
///   which remain the same in order to persist the start position. Assumes the list of cubics is
///   continuous.
pub fn detect_features(cubics: &[Cubic]) -> Vec<Feature> {
    if cubics.is_empty() {
        return Vec::new();
    }

    let mut features: Vec<Feature> = Vec::new();
    let mut current = cubics[0];
    // Do one roundabout in which (current == last, next == first) is the last iteration.
    // Just like a snowball, subsequent cubics that align to one feature merge until
    // the streak breaks, the result is added, and a new streak starts.
    for i in 0..cubics.len() {
        let next = &cubics[(i + 1) % cubics.len()];

        if i < cubics.len() - 1 && current.aligns_ish_with(next) {
            current = Cubic::extend(&current, next);
            continue;
        }
        features.push(current.as_feature(next));
        if !current.smoothes_into_ish(next) {
            features.push(Cubic::empty(current.anchor_1_x(), current.anchor_1_y()).as_feature(next))
        }
        current = *next;
    }
    features
}

impl Cubic {
    /// Convert to [`Feature::Edge`] if this cubic describes a straight line, otherwise to a
    /// [`Feature::Corner`]. Corner convexity is determined by [`convex`](crate::utils::convex).
    pub fn as_feature(&self, next: &Cubic) -> Feature {
        if self.straight_ish() {
            Feature::edge(vec![self.clone()])
        } else {
            Feature::corner(vec![self.clone()], self.convex_to(next))
        }
    }

    /// Determine if the cubic is close to a straight line. Empty cubics don't count as `straight_ish`.
    pub fn straight_ish(&self) -> bool {
        !self.zero_length()
            && collinear_ish(
                self.anchor_0_x(),
                self.anchor_0_y(),
                self.anchor_1_x(),
                self.anchor_1_y(),
                self.control_0_x(),
                self.control_0_y(),
                RELAXED_DISTANCE_EPSILON,
            )
            && collinear_ish(
                self.anchor_0_x(),
                self.anchor_0_y(),
                self.anchor_1_x(),
                self.anchor_1_y(),
                self.control_1_x(),
                self.control_1_y(),
                RELAXED_DISTANCE_EPSILON,
            )
    }

    /// Determines if next is a smooth continuation of this cubic. Smooth meaning that the first control
    /// point of next is a reflection of this' second control point, similar to the S/s or t/T command in
    /// svg paths https://developer.mozilla.org/en-US/docs/Web/SVG/Tutorial/Paths#b%C3%A9zier_curves
    pub fn smoothes_into_ish(&self, next: &Cubic) -> bool {
        collinear_ish(
            self.control_1_x(),
            self.control_1_y(),
            next.control_0_x(),
            next.control_0_y(),
            self.anchor_1_x(),
            self.anchor_1_y(),
            RELAXED_DISTANCE_EPSILON,
        )
    }

     /// Determines if all of this' points align with next's points. For straight lines, this is the same
     /// as if next was a continuation of this.
    pub fn aligns_ish_with(&self, next: &Cubic) -> bool {
        self.straight_ish() && next.straight_ish() && self.smoothes_into_ish(next)
            || self.zero_length()
            || next.zero_length()
    }

    /// Create a new cubic by extending A to B's second anchor point
    pub fn extend(a: &Cubic, b: &Cubic) -> Cubic {
        if a.zero_length() {
            Cubic::new(
                a.anchor_0_x(),
                a.anchor_0_y(),
                b.control_0_x(),
                b.control_0_y(),
                b.control_1_x(),
                b.control_1_y(),
                b.anchor_1_x(),
                b.anchor_1_y(),
            )
        } else {
            Cubic::new(
                a.anchor_0_x(),
                a.anchor_0_y(),
                a.control_0_x(),
                a.control_0_y(),
                b.control_1_x(),
                b.control_1_y(),
                b.anchor_1_x(),
                b.anchor_1_y(),
            )
        }
    }
}

#[cfg(test)]
mod feature_detector_tests {
    use crate::corner_rounding::CornerRounding;
    use crate::cubic::Cubic;
    use crate::feature::Feature;
    use crate::feature_detector::detect_features;
    use crate::point::Point;
    use crate::rounded_polygon::RoundedPolygonBuilder;
    use crate::shapes::PillStarBuilder;
    use crate::tests::assert_points_equalish;

    #[test]
    fn recognizes_straightness() {
        assert!(Cubic::straight_line(0.0, 0.0, 1.0, 0.0).straight_ish());
    }
    #[test]
    fn recognizes_straightness_ish() {
        let slightly_not_straight_cubic = Cubic::new(
            323.508, 201.759, 320.0, 197.0, 317.35, 192.008, 311.193, 182.227,
        );
        assert!(!slightly_not_straight_cubic.straight_ish());
    }
    #[test]
    fn recognizes_curvature() {
        let round_cubic = Cubic::new(0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0);
        assert!(!round_cubic.straight_ish());
    }
    #[test]
    fn recognizes_nonsmoothness_for_curved_cubic() {
        let base_cubic = Cubic::new(0.0, 0.0, 0.0, 10.0, 10.0, 10.0, 10.0, 0.0);
        let nonsmooth_continuation = Cubic::new(10.0, 0.0, 15.0, -10.0, 20.0, -10.0, 20.0, 0.0);

        assert!(!base_cubic.smoothes_into_ish(&nonsmooth_continuation));
    }
    #[test]
    fn recognizes_smoothness_for_straight_cubic() {
        let base_cubic = Cubic::straight_line(0.0, 0.0, 10.0, 0.0);
        let smooth_continuation = Cubic::straight_line(10.0, 0.0, 20.0, 0.0);
        assert!(base_cubic.smoothes_into_ish(&smooth_continuation));
    }
    #[test]
    fn recognizes_smoothness_within_relative_tolerance() {
        // These two cubics are from the edge of an imported shape. Even though they don't
        // count as smooth within the absolute distance epsilon, relatively seen they should count.
        let base_cubic = Cubic::new(
            323.508, 201.759, 317.35, 192.008, 311.193, 182.227, 305.008, 172.475,
        );
        let smooth_continuation = Cubic::new(
            305.008, 172.475, 290.812, 149.962, 276.617, 127.42, 262.422, 104.907,
        );

        assert!(base_cubic.smoothes_into_ish(&smooth_continuation));
    }
    #[test]
    fn empty_cubics_are_straight_ish() {
        assert!(!Cubic::empty(10.0, 10.0).straight_ish());
    }
    #[test]
    fn recognizes_alignment_for_straight_lines() {
        let base_cubic = Cubic::straight_line(0.0, 0.0, 10.0, 0.0);
        let smooth_continuation = Cubic::straight_line(10.0, 0.0, 20.0, 0.0);

        assert!(base_cubic.aligns_ish_with(&smooth_continuation));
    }
    #[test]
    fn recognizes_alignment_within_relative_tolerance() {
        // These two cubics are from the edge of an imported shape. Even though the second edge
        // is very small within the given scale, it is not empty. However, even the length of
        // 0.027 is so relatively tiny in the given range of coordinates, that it should be seen as
        // an empty cubic. Therefore, the second can be seen as an extend of the first.
        let base_cubic = Cubic::new(
            323.508, 201.759, 317.35, 192.008, 311.193, 182.227, 305.035, 172.475,
        );
        let smooth_continuation = Cubic::straight_line(305.035, 172.475, 305.008, 172.475);

        assert!(base_cubic.aligns_ish_with(&smooth_continuation));
    }
    #[test]
    fn includes_alignment_for_empty_cubics() {
        let base_cubic = Cubic::straight_line(0.0, 0.0, 10.0, 0.0);
        let empty_cubic = Cubic::empty(10.0, 0.0);

        assert!(base_cubic.aligns_ish_with(&empty_cubic));
        assert!(empty_cubic.aligns_ish_with(&base_cubic));
    }
    #[test]
    fn converts_straight_cubic_to_edge() {
        let cubic = Cubic::straight_line(0.0, 0.0, 10.0, 0.0);
        let following_cubic = Cubic::straight_line(10.0, 0.0, 20.0, 0.0);

        let converted = cubic.as_feature(&following_cubic);
        let expected = Feature::edge(vec![cubic]);

        match converted.clone() {
            Feature::Edge(_) => {
                assert_eq!(expected, converted);
            }
            _ => panic!("Expected Feature::Edge"),
        }
    }
    #[test]
    fn converts_curved_cubic_to_corner() {
        let cubic = Cubic::new(0.0, 0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0);
        let following_cubic = Cubic::new(1.0, 0.0, 1.5, 1.5, 1.5, 1.5, 2.0, 0.0);

        let converted = cubic.as_feature(&following_cubic);
        let expected = Feature::corner(vec![cubic], false);

        match converted.clone() {
            Feature::Corner(_) => {
                assert_eq!(expected, converted);
            }
            _ => panic!("Expected Feature::Corner"),
        }
    }
    #[test]
    fn converts_empty_cubic_to_corner() {
        let cubic = Cubic::empty(1.0, 0.0);
        let following_cubic = Cubic::new(1.0, 0.0, 1.5, 1.5, 1.5, 1.5, 2.0, 0.0);

        let converted = cubic.as_feature(&following_cubic);
        let expected = Feature::corner(vec![cubic], false);

        match converted.clone() {
            Feature::Corner(_) => {
                assert_eq!(expected, converted);
            }
            _ => panic!("Expected Feature::Corner"),
        }
    }

    #[test]
    fn reconstructs_pill_star() {
        let original_polygon = PillStarBuilder::new().build();
        let split_cubics: Vec<Cubic> = original_polygon
            .cubics
            .iter()
            .flat_map(|cubic| {
                let (first_half, second_half) = cubic.split(0.5);
                vec![first_half, second_half]
            })
            .collect();

        let created_polygon = RoundedPolygonBuilder::from_features(detect_features(&split_cubics))
            .center_x(original_polygon.center_x())
            .center_y(original_polygon.center_y())
            .build();

        // It's okay if the cubics' control points aren't the same, as long as the shape is the same
        assert_eq!(original_polygon.cubics.len(), created_polygon.cubics.len());
        created_polygon
            .cubics
            .iter()
            .enumerate()
            .for_each(|(i, new)| {
                let original = &original_polygon.cubics[i];

                // pillStar has no roundings, so the created cubics shouldn't be as well
                assert!(new.straight_ish());
                assert!(original.straight_ish());

                assert_points_equalish(
                    Point(new.anchor_0_x(), new.anchor_0_y()),
                    Point(original.anchor_0_x(), original.anchor_0_y()),
                );
                assert_points_equalish(
                    Point(new.anchor_1_x(), new.anchor_1_y()),
                    Point(original.anchor_1_x(), original.anchor_1_y()),
                );
            });

        // The order of the features can be different, as long as they describe the same shape
        assert_eq!(
            original_polygon.features.len(),
            created_polygon.features.len()
        );
        assert_eq!(
            original_polygon
                .features
                .iter()
                .filter(|f| matches!(f, Feature::Corner(_)))
                .count(),
            created_polygon
                .features
                .iter()
                .filter(|f| matches!(f, Feature::Corner(_)))
                .count(),
        );
        assert_eq!(
            original_polygon
                .features
                .iter()
                .filter(|f| matches!(f, Feature::Edge(_)))
                .count(),
            created_polygon
                .features
                .iter()
                .filter(|f| matches!(f, Feature::Edge(_)))
                .count(),
        );
        assert!(created_polygon.features.windows(2).all(|pair| {
            (matches!(pair[0], Feature::Edge(_)) && matches!(pair[1], Feature::Corner(_)))
                || (matches!(pair[0], Feature::Corner(_)) && matches!(pair[1], Feature::Edge(_)))
        }));
        assert!(
            created_polygon
                .features
                .iter()
                .filter_map(|f| {
                    if let Feature::Corner(corner) = f {
                        Some(corner)
                    } else {
                        None
                    }
                })
                .all(|corner| corner.cubics.len() == 1
                    && corner.cubics.first().unwrap().zero_length())
        );
    }

    #[test]
    fn reconstructs_rounded_pill_star_close_enough() {
        // This test aims to ensure that our distance epsilon is not set too high that
        // the roundings of pill star gets pointy as they are small in the [0,1] space
        let original_polygon = PillStarBuilder::new()
            .rounding(CornerRounding::new(0.2, None))
            .build();
        let created_polygon =
            RoundedPolygonBuilder::from_features(detect_features(&original_polygon.cubics))
                .center_x(original_polygon.center_x())
                .center_y(original_polygon.center_y())
                .build();

        assert_eq!(original_polygon.cubics.len(), created_polygon.cubics.len());
        // Allow up to one difference...
        assert_eq!(
            (original_polygon.features.len() as isize - created_polygon.features.len() as isize)
                .abs(),
            1
        );
        // ...as long as the edge - corner pattern persists
        assert!(created_polygon.features.windows(2).all(|pair| {
            (matches!(pair[0], Feature::Edge(_)) && matches!(pair[1], Feature::Corner(_)))
                || (matches!(pair[0], Feature::Corner(_)) && matches!(pair[1], Feature::Edge(_)))
        }));
    }
}
