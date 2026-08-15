use std::collections::HashSet;
use std::hash::Hash;
use crate::float_mapping::{progress_in_range, DoubleMapper};
use crate::feature::{Feature, FeatureTrait};
use crate::point::Point;
use crate::utils::{require, DISTANCE_EPSILON};

/// [`MeasuredFeatures`] contains a list of all features in a polygon along with the [0..1] progress at
/// that feature
pub type MeasuredFeatures = Vec<ProgressableFeature>;

#[derive(Clone, Debug)]
pub struct ProgressableFeature{
    pub progress: f32,
    pub feature: Feature,
}

impl Hash for ProgressableFeature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.progress.to_bits().hash(state);
        self.feature.hash(state);
    }
}

impl PartialEq for ProgressableFeature {
    fn eq(&self, other: &Self) -> bool {
        self.progress.to_bits() == other.progress.to_bits() && self.feature == other.feature
    }
}

impl Eq for ProgressableFeature {}

/// [`feature_apper`] creates a mapping between the "features" (rounded corners) of two shapes
pub fn feature_mapper(features1: &MeasuredFeatures, features2: &MeasuredFeatures) -> DoubleMapper {
    let filtered_features1: MeasuredFeatures =
        features1.iter().filter(|f1|
            matches!(f1.feature, Feature::Corner(_))
        ).cloned().collect();
    let filtered_features2: MeasuredFeatures =
        features2.iter().filter(|f2|
            matches!(f2.feature, Feature::Corner(_))
        ).cloned().collect();

    let feature_progress_mapping =
        do_mapping(&filtered_features1, &filtered_features2);
    DoubleMapper::new(&feature_progress_mapping)
}

pub struct  DistanceVertex {
    pub distance: f32,
    pub f1: ProgressableFeature,
    pub f2: ProgressableFeature,
}

/// Returns a mapping of the features between `features1` and `features2`. The return value is a
/// list of pairs in which the first element is the progress of a feature in `features1` and the
/// second element is the progress of the feature in `features2` that we mapped it to. The list is
/// sorted by the first element. To do this:
///
/// 1. Compute the distance for all pairs of features in `(features1 × features2)`.
/// 2. Sort ascending by such distance.
/// 3. Try to add them, from the smallest distance to biggest, ensuring that:
///    a) The features we are mapping haven't been mapped yet.
///    b) We are not adding a crossing in the mapping. Since the mapping is sorted by the first
///       element of each pair, this means that the second elements of each pair are monotonically
///       increasing, except maybe one time (counting all pairs of consecutive elements, and the
///       last element to first element).
fn do_mapping(
    features1: &MeasuredFeatures,
    features2: &MeasuredFeatures
) -> Vec<(f32, f32)> {
    let mut distance_vertex_list = Vec::new();
    for f1 in features1.iter() {
        for f2 in features2.iter() {
            let d = feature_dist_squared(&f1.feature, &f2.feature);
            if d != f32::MAX {
                distance_vertex_list.push(
                    DistanceVertex {
                        distance: d,
                        f1: f1.clone(),
                        f2: f2.clone(),
                    }
                );
            }
        }
    }
    distance_vertex_list.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
    if distance_vertex_list.is_empty() {
        return identity_mapping();
    }
    if distance_vertex_list.len() == 1 {
        let first = &distance_vertex_list[0];
        let f1 = first.f1.progress;
        let f2 = first.f2.progress;
        return vec![(f1, f2), ((f1 + 0.5) % 1.0, (f2 + 0.5) % 1.0)];
    }
    let mut helper = MappingHelper::new();
    for vertex in distance_vertex_list.iter() {
        helper.add_mapping(&vertex.f1, &vertex.f2);
    }
    helper.mapping
}

fn identity_mapping() -> Vec<(f32, f32)> {
    vec![(0.0, 0.0), (0.5, 0.5)]
}

struct MappingHelper {
    pub mapping: Vec<(f32, f32)>,
    used_f1: HashSet<ProgressableFeature>,
    used_f2: HashSet<ProgressableFeature>,
}
impl MappingHelper {
    pub fn new() -> Self {
        MappingHelper {
            mapping: Vec::new(),
            used_f1: HashSet::new(),
            used_f2: HashSet::new(),
        }
    }

    pub fn add_mapping(
        &mut self,
        f1: &ProgressableFeature,
        f2: &ProgressableFeature
    ) {
        if self.used_f1.contains(f1) || self.used_f2.contains(f2) {
            return;
        }
        let index = self.mapping.binary_search_by(
            |(p1, _)| p1.partial_cmp(&f1.progress).unwrap()
        );
        let insertion_index = match index {
            Ok(i) => panic!(
                "There can't be two features with the same progress"
            ),
            Err(i) => i,
        };
        let n = self.mapping.len();
        if n >= 1 {
            let (before_1, before_2) = self.mapping[(insertion_index + n - 1) % n];
            let (after_1, after_2) = self.mapping[insertion_index % n];
            if progress_distance(f1.progress, before_1) < DISTANCE_EPSILON ||
                progress_distance(f1.progress, after_1) < DISTANCE_EPSILON ||
                progress_distance(f2.progress, before_2) < DISTANCE_EPSILON ||
                progress_distance(f2.progress, after_2) < DISTANCE_EPSILON {
                return;
            }
            if n > 1 && !progress_in_range(
                f2.progress,
                before_2,
                after_2
            ) {
                return;
            }
        }
        self.mapping.insert(insertion_index, (f1.progress, f2.progress));
        self.used_f1.insert(f1.clone());
        self.used_f2.insert(f2.clone());
    }
}
/// Returns distance along overall shape between two Features on the two different shapes. This
/// information is used to determine how to map features (and the curves that make up those
/// features).
pub fn feature_dist_squared(f1: &Feature, f2: &Feature) -> f32 {
    // TODO: We might want to enable concave-convex matching in some situations. If so, the
    //  approach below will not work
    if let Feature::Corner(c1) = f1 && let Feature::Corner(c2) = f2
        && c1.convex != c2.convex {
        return f32::MAX;
    }
    (feature_representative_point(f1) - feature_representative_point(f2)).get_distance_squared()
}

pub fn feature_representative_point(feature: &Feature) -> Point {
    let cubics = feature.cubics();
    let first = cubics.first().unwrap();
    let last = cubics.last().unwrap();
    let x = (first.anchor_0_x() + last.anchor_1_x()) / 2.0;
    let y = (first.anchor_0_y() + last.anchor_1_y()) / 2.0;
    Point(x, y)
}

fn validate_progresses(p: &Vec<f32>) {
    let mut prev = *p.last().unwrap();
    let mut wraps = 0;
    for i in 0..p.len() {
        let curr = &p[i];
        require(
            *curr >= 0.0 && *curr < 1.0,
            format!("FeatureMapping - Progress outside of range: {:?}", p)
        );
        require(
            progress_distance(*curr, prev) > DISTANCE_EPSILON,
            format!("FeatureMapping - Progress repeats a value: {:?}", p)
        );
        if curr < &prev {
            wraps += 1;
            require(
                wraps <= 1,
                format!("FeatureMapping - Progress wraps more than once: {:?}", p)
            );
        }
        prev = *curr;
    }
}

fn progress_distance(p1: f32, p2: f32) -> f32 {
    let d = (p1 - p2).abs();
    d.min(1.0 - d)
}

#[cfg(test)]
mod feature_mapping_tests {
    use std::rc::Rc;
    use lazy_static::lazy_static;
    use crate::assert_equalish;
    use crate::corner_rounding::CornerRounding;
    use crate::feature_mapping::{do_mapping, feature_dist_squared};
    use crate::polygon_measure::{LengthMeasurer, MeasuredPolygon};
    use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
    use crate::shapes::StarBuilder;

    lazy_static!(
        static ref TRIANGLE_WITH_ROUNDINGS: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(3)
        .rounding(CornerRounding::new(0.2, None)).build();
        static ref TRIANGLE: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(3).build();
        static ref SQUARE: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(4).build();
    );

    #[test]
    fn feature_mapping_triangle() {
        verify_mapping(
            &TRIANGLE_WITH_ROUNDINGS,
            &TRIANGLE,
            |distances| {
                distances.iter().for_each(|&d| assert!(d < 0.1f32));
            }
        );
    }

    #[test]
    fn feature_mapping_triangle_to_square() {
        verify_mapping(
            &TRIANGLE,
            &SQUARE,
            |distances| {
                // We have one exact match (both have points at 0 degrees), and 2 close ones
                assert_eq!(3, distances.len());
                assert_equalish!(distances[0], distances[1]);
                assert!(distances[0] < 0.3f32);
                assert!(distances[2] < 1e-6f32);
            }
        );
    }

    #[test]
    fn feature_mapping_square_to_triangle() {
        verify_mapping(
            &SQUARE,
            &TRIANGLE,
            |distances| {
                // We have one exact match (both have points at 0 degrees), and 2 close ones
                assert_eq!(3, distances.len());
                assert_equalish!(distances[0], distances[1]);
                assert!(distances[0] < 0.3f32);
                assert!(distances[2] < 1e-6f32);
            }
        );
    }

    #[test]
    fn feature_mapping_does_not_crash() {
        // Verify that complicated shapes can be matched (this used to crash before).
        let checkmark = RoundedPolygonBuilder::from_vertices(
            &vec![
                400.0,
                -304.0,
                240.0,
                -464.0,
                296.0,
                -520.0,
                400.0,
                -416.0,
                664.0,
                -680.0,
                720.0,
                -624.0,
                400.0,
                -304.0,
            ]
        ).build().normalized();
        let very_sunny = StarBuilder::new(8)
            .inner_radius(0.65)
            .rounding(CornerRounding::new(0.15, None))
            .build()
            .normalized();
        verify_mapping(&checkmark, &very_sunny, |distances| {
            // Most vertices on the checkmark map to a feature in the second shape.
            assert!(distances.len() >= 6);

            // And they are close enough
            assert!(distances[0] < 0.15f32);
        });
    }

    fn verify_mapping(
        p1: &RoundedPolygon,
        p2: &RoundedPolygon,
        validator: impl Fn(&[f32])
    ) {
        let measurer = Rc::new(LengthMeasurer::new());
        let f1 = MeasuredPolygon::measured_polygon(measurer.clone(), p1).features;
        let f2 = MeasuredPolygon::measured_polygon(measurer.clone(), p2).features;

        // Maps progress in p1 to progress in p2
        let map = do_mapping(&f1, &f2);

        // See which features where actually mapped and the distance between their representative
        // points
        let mut distances = Vec::new();
        for (progress1, progress2) in map.iter() {
            let feature1 = f1.iter().find(|f| f.progress == *progress1).unwrap();
            let feature2 = f2.iter().find(|f| f.progress == *progress2).unwrap();
            distances.push(feature_dist_squared(&feature1.feature, &feature2.feature));
        }
        distances.sort_by(|a, b| b.partial_cmp(a).unwrap());
        validator(&distances);
    }
}