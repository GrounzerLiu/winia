use crate::cubic::Cubic;
use crate::feature::{Feature, FeatureTrait};
use crate::feature_mapping::ProgressableFeature;
use crate::point::Point;
use crate::rounded_polygon::RoundedPolygon;
use crate::utils::{positive_modulus, require, DISTANCE_EPSILON};
use std::ops::Deref;
use std::rc::Rc;

/// A [`MeasuredCubic`] holds information about the cubic itself, the feature (if any) associated
/// with it, and the outline progress values (start and end) for the cubic. This information is
/// used to match cubics between shapes that lie at similar outline progress positions along
/// their respective shapes (after matching features and shifting).
///
/// Outline progress is a value in `[0..1)` that represents the distance traveled along the overall
/// outline path of the shape.
#[derive(Clone)]
pub struct MeasuredCubic {
    pub cubic: Cubic,
    start_outline_progress: f32,
    end_outline_progress: f32,
    measurer: Rc<dyn Measurer>,
    measured_size: f32,
}

impl MeasuredCubic {
    pub fn new(
        cubic: Cubic,
        start_outline_progress: f32,
        end_outline_progress: f32,
        measurer: Rc<dyn Measurer>,
    ) -> Self {
        require(
            end_outline_progress >= start_outline_progress,
            "end_outline_progress is expected to be equal or greater than start_outline_progress",
        );
        // let measured_size -
        MeasuredCubic {
            cubic,
            start_outline_progress,
            end_outline_progress,
            measurer: measurer.clone(),
            measured_size: measurer.measure_cubic(&cubic),
        }
    }

    pub fn start_outline_progress(&self) -> f32 {
        self.start_outline_progress
    }

    pub fn end_outline_progress(&self) -> f32 {
        self.end_outline_progress
    }

    pub fn update_progresses_range(
        &mut self,
        start_outline_progress: impl Into<Option<f32>>,
        end_outline_progress: impl Into<Option<f32>>,
    ) {
        let start_outline_progress = start_outline_progress
            .into()
            .unwrap_or(self.start_outline_progress);
        let end_outline_progress = end_outline_progress
            .into()
            .unwrap_or(self.end_outline_progress);
        require(
            end_outline_progress >= start_outline_progress,
            "end_outline_progress is expected to be equal or greater than start_outline_progress",
        );
        self.start_outline_progress = start_outline_progress;
        self.end_outline_progress = end_outline_progress;
    }

    /// Cut this [`MeasuredCubic`] into two [`MeasuredCubic`]s at the given outline progress value.
    pub fn cut_at_progress(&self, cut_outline_progress: f32) -> (MeasuredCubic, MeasuredCubic) {
        // Floating point errors further up can cause cutOutlineProgress to land just
        // slightly outside of the start/end progress for this cubic, so we limit it
        // to those bounds to avoid further errors later
        let bounded_cut_outline_progress =
            cut_outline_progress.clamp(self.start_outline_progress, self.end_outline_progress);
        let outline_progress_size = self.end_outline_progress - self.start_outline_progress;
        let progress_from_start = bounded_cut_outline_progress - self.start_outline_progress;

        // Note that in earlier parts of the computation, we have empty MeasuredCubics (cubics
        // with progressSize == 0f), but those cubics are filtered out before this method is
        // called.
        let relative_progress = progress_from_start / outline_progress_size;
        let t = self
            .measurer
            .find_cubic_cut_point(&self.cubic, relative_progress * self.measured_size);
        require(
            t >= 0.0 && t <= 1.0,
            "Cubic cut point is expected to be between 0 and 1",
        );
        // c1/c2 are the two new cubics, then we return MeasuredCubics created from them
        let (c1, c2) = self.cubic.split(t);
        (
            MeasuredCubic::new(
                c1,
                self.start_outline_progress,
                bounded_cut_outline_progress,
                self.measurer.clone(),
            ),
            MeasuredCubic::new(
                c2,
                bounded_cut_outline_progress,
                self.end_outline_progress,
                self.measurer.clone(),
            ),
        )
    }
}
pub struct MeasuredPolygon {
    measurer: Rc<dyn Measurer>,
    cubics: Vec<MeasuredCubic>,
    pub features: Vec<ProgressableFeature>,
}

impl MeasuredPolygon {
    pub fn new(
        measurer: Rc<dyn Measurer>,
        features: Vec<ProgressableFeature>,
        cubics: Vec<Cubic>,
        outline_progresses: Vec<f32>,
    ) -> Self {
        require(
            outline_progresses.len() == cubics.len() + 1,
            "Outline progress size is expected to be the cubics size + 1",
        );
        require(
            outline_progresses.first() == Some(&0f32),
            "First outline progress value is expected to be zero",
        );
        require(
            outline_progresses.last() == Some(&1f32),
            "Last outline progress value is expected to be one",
        );
        let mut measured_cubics = Vec::with_capacity(cubics.len());
        let mut start_outline_progress = 0.0;
        for (index, cubic) in cubics.into_iter().enumerate() {
            if (outline_progresses[index + 1] - outline_progresses[index]) > DISTANCE_EPSILON {
                measured_cubics.push(MeasuredCubic::new(
                    cubic,
                    start_outline_progress,
                    outline_progresses[index + 1],
                    measurer.clone(),
                ));
                start_outline_progress = outline_progresses[index + 1];
            }
        }
        let len = measured_cubics.len();
        measured_cubics[len - 1].update_progresses_range(None, 1.0);
        MeasuredPolygon {
            measurer,
            cubics: measured_cubics,
            features,
        }
    }

    /// Finds the point in the input list of [`MeasuredCubic`]s that passes the given outline progress,
    /// and generates a new [`MeasuredPolygon`] (equivalent to this) that starts at that point. This
    /// usually means cutting the cubic that crosses the outline progress (unless the cut is at one
    /// of its ends). For example, given outline progress `0.4` and measured cubics on these outline
    /// progress ranges:
    ///
    /// ```text
    /// c1 [0 -> 0.2]
    /// c2 [0.2 -> 0.5]
    /// c3 [0.5 -> 1.0]
    /// ```
    ///
    /// `c2` will be cut in two at the given outline progress, named `c2a [0.2 -> 0.4]` and
    /// `c2b [0.4 -> 0.5]`.
    ///
    /// The return then will have measured cubics `[c2b, c3, c1, c2a]`, and they will have their
    /// outline progress ranges adjusted so the new list starts at 0:
    ///
    /// ```text
    /// c2b [0 -> 0.1]
    /// c3  [0.1 -> 0.6]
    /// c1  [0.6 -> 0.8]
    /// c2a [0.8 -> 1.0]
    /// ```
    pub fn cut_and_shift(&self, cutting_point: f32) -> MeasuredPolygon {
        require(
            cutting_point >= 0.0 && cutting_point < 1.0,
            "Cutting point is expected to be between 0 and 1",
        );
        // Find the index of cubic we want to cut
        let taget_index = self
            .cubics
            .iter()
            .position(|mc| {
                cutting_point >= mc.start_outline_progress
                    && cutting_point < mc.end_outline_progress
            })
            .unwrap();
        let target = &self.cubics[taget_index];
        // Cut the target cubic.
        // b1, b2 are two resulting cubics after cut
        let (b1, b2) = target.cut_at_progress(cutting_point);

        // Construct the list of the cubics we need:
        // * The second part of the target cubic (after the cut)
        // * All cubics after the target, until the end + All cubics from the start, before the
        //   target cubic
        // * The first part of the target cubic (before the cut)
        let mut ret_cubics = vec![b2.cubic];
        let cubics_len = self.cubics.len();
        for i in 1..self.cubics.len() {
            ret_cubics.push(self.cubics[(i + taget_index) % cubics_len].cubic);
        }
        ret_cubics.push(b1.cubic);
        // Construct the array of outline progress.
        // For example, if we have 3 cubics with outline progress [0 .. 0.3], [0.3 .. 0.8] &
        // [0.8 .. 1.0], and we cut + shift at 0.6:
        // 0.  0123456789
        //     |--|--/-|-|
        // The outline progresses will start at 0 (the cutting point, that shifs to 0.0),
        // then 0.8 - 0.6 = 0.2, then 1 - 0.6 = 0.4, then 0.3 - 0.6 + 1 = 0.7,
        // then 1 (the cutting point again),
        // all together: (0.0, 0.2, 0.4, 0.7, 1.0)
        let mut ret_outline_progress = Vec::with_capacity(self.cubics.len() + 2);
        for index in 0..self.cubics.len() + 2 {
            ret_outline_progress.push(if index == 0 {
                0.0
            } else if index == self.cubics.len() + 1 {
                1.0
            } else {
                let cubic_index = (taget_index + index - 1) % cubics_len;
                positive_modulus(
                    self.cubics[cubic_index].end_outline_progress - cutting_point,
                    1.0,
                )
            });
        }

        // Shift the feature's outline progress too.
        let new_features = self
            .features
            .iter()
            .map(|f| {
                let new_progress = positive_modulus(f.progress - cutting_point, 1.0);
                ProgressableFeature {
                    feature: f.feature.clone(),
                    progress: new_progress,
                }
            })
            .collect();
        // Filter out all empty cubics (i.e. start and end anchor are (almost) the same point.)
        MeasuredPolygon::new(
            self.measurer.clone(),
            new_features,
            ret_cubics,
            ret_outline_progress,
        )
    }
    pub fn measured_polygon(
        measurer: Rc<dyn Measurer>,
        polygon: &RoundedPolygon,
    ) -> MeasuredPolygon {
        let mut cubics = vec![];
        let mut feature_to_cubic = vec![];

        // Get the cubics from the polygon, at the same time, extract the features and keep a
        // reference to the representative cubic we will use.
        for feature in polygon.features.iter() {
            let cubic_count = feature.cubics().len();
            for (cubic_index, cubic) in feature.cubics().iter().enumerate() {
                if let Feature::Corner(corner) = feature
                    && cubic_index == cubic_count / 2
                {
                    feature_to_cubic.push((feature.clone(), cubics.len()));
                }
                cubics.push(cubic.clone());
            }
        }
        // TODO(performance): Make changes to satisfy the lint warnings for unnecessary
        //  iterators creation.
        let measures = std::iter::once(0.0)
            .chain(cubics.iter().scan(0.0, |measure, cubic| {
                let measured = measurer.measure_cubic(cubic);
                require(
                    measured >= 0.0,
                    "Measured cubic is expected to be greater or equal to zero",
                );
                *measure += measured;
                Some(*measure)
            }))
            .collect::<Vec<f32>>();
        let total_measure = *measures.last().unwrap();

        // Equivalent to `measures.map { it / totalMeasure }` but without Iterator allocation.
        let outline_progresses = measures
            .iter()
            .map(|m| m / total_measure)
            .collect::<Vec<f32>>();

        let features = feature_to_cubic
            .iter()
            .map(|(feature, ix)| ProgressableFeature {
                progress: positive_modulus(
                    (outline_progresses[*ix] + outline_progresses[ix + 1]) / 2.0,
                    1.0,
                ),
                feature: feature.clone(),
            })
            .collect::<Vec<ProgressableFeature>>();
        MeasuredPolygon::new(measurer, features, cubics, outline_progresses)
    }
}

impl Deref for MeasuredPolygon {
    type Target = Vec<MeasuredCubic>;

    fn deref(&self) -> &Self::Target {
        &self.cubics
    }
}


/// Trait for measuring a cubic. Implementations can use whatever algorithm desired to produce
/// these measurement values.

pub trait Measurer {
    /// Returns size of given cubic, according to however the implementation wants to measure the
    /// size (angle, length, etc). It has to be greater or equal to 0.
    fn measure_cubic(&self, c: &Cubic) -> f32;

    /// Given a cubic and a measure that should be between 0 and the value returned by [`measure_cubic`](Self::measure_cubic)
    /// (If not, it will be capped), finds the parameter t of the cubic at which that measure is
    /// reached.
    fn find_cubic_cut_point(&self, c: &Cubic, m: f32) -> f32;
}


/// Approximates the arc lengths of cubics by splitting the arc into segments and calculating their
/// sizes. The more segments, the more accurate the result will be to the true arc length. The
/// default implementation has at least 98.5% accuracy on the case of a circular arc, which is the
/// worst case for our standard shapes.
pub struct LengthMeasurer {
    segments: usize,
}

impl LengthMeasurer {
    pub fn new() -> Self {
        LengthMeasurer { segments: 3 }
    }

    fn closest_progress_to(&self, cubic: &Cubic, threshold: f32) -> (f32, f32) {
        let mut total = 0.0;
        let mut remainder = threshold;
        let mut prev = Point(cubic.anchor_0_x(), cubic.anchor_0_y());

        for i in 1..=self.segments {
            let progress = i as f32 / self.segments as f32;
            let point = cubic.point_on_curve(progress);
            let segment = (point - prev).get_distance();

            if segment >= remainder {
                return (
                    progress - (1.0 - remainder / segment) / self.segments as f32,
                    threshold,
                );
            }

            remainder -= segment;
            total += segment;
            prev = point;
        }
        (1.0, total)
    }
}

impl Measurer for LengthMeasurer {
    fn measure_cubic(&self, c: &Cubic) -> f32 {
        self.closest_progress_to(c, f32::MAX).1
    }

    fn find_cubic_cut_point(&self, c: &Cubic, m: f32) -> f32 {
        self.closest_progress_to(c, m).0
    }
}

#[cfg(test)]
mod polygon_measure_tests {
    use crate::assert_equalish;
    use crate::corner_rounding::CornerRounding;
    use crate::polygon_measure::{LengthMeasurer, MeasuredPolygon, Measurer};
    use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
    use crate::tests::assert_floats_equalish;
    use std::f32::consts::PI;
    use std::rc::Rc;
    use crate::cubic::Cubic;
    use crate::feature::FeatureFactory;

    macro_rules! irregular_polygon_measure {
        ($polygon:expr) => {
            irregular_polygon_measure($polygon, |_| {})
        };
        ($polygon:expr, $extra_checks:expr) => {
            irregular_polygon_measure($polygon, $extra_checks)
        };
    }

    macro_rules! regular_polygon_measure {
        ($sides:expr) => {
            regular_polygon_measure($sides, None)
        };
        ($sides:expr, $rounding:expr) => {
            regular_polygon_measure($sides, $rounding)
        };
    }

    fn measurer() -> Rc<dyn Measurer> {
        Rc::new(LengthMeasurer::new())
    }

    #[test]
    fn measure_sharp_triangle() {
        regular_polygon_measure!(3)
    }

    #[test]
    fn measure_sharp_pentagon() {
        regular_polygon_measure!(5)
    }
    #[test]
    fn measure_sharp_octagon() {
        regular_polygon_measure!(8)
    }
    #[test]
    fn measure_sharp_dodecagon() {
        regular_polygon_measure!(12)
    }
    #[test]
    fn measure_sharp_icosagon() {
        regular_polygon_measure!(20)
    }

    #[test]
    fn measure_circle() {
        // White box test: As the length measurer approximates arcs by linear segments,
        // this test validates if the chosen segment count approximates the arc length up to
        // an error of 1.5% from the true length
        let vertices = 4;
        let polygon = RoundedPolygon::circle(vertices, None, None, None);

        let actual_length: f32 = polygon
            .cubics
            .iter()
            .map(|cubic| LengthMeasurer::new().measure_cubic(cubic))
            .sum();
        let expected_length = 2.0 * PI;

        assert_equalish!(expected_length, actual_length, 0.015 * expected_length);
    }

    #[test]
    fn irregular_triangle_angle_measure() {
        irregular_polygon_measure!(
            &RoundedPolygonBuilder::from_vertices(&vec![0.0, -1.0, 1.0, 1.0, 0.0, 0.5, -1.0, 1.0,])
                .per_vertex_rounding(vec![
                    CornerRounding::new(0.2, 0.5),
                    CornerRounding::new(0.2, 0.5),
                    CornerRounding::new(0.4, 0.0),
                    CornerRounding::new(0.2, 0.5),
                ])
                .build()
        )
    }

    #[test]
    fn quarter_angle_measure() {
        irregular_polygon_measure!(
            &RoundedPolygonBuilder::from_vertices(&vec![
                -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0,
            ])
            .per_vertex_rounding(vec![
                CornerRounding::UNROUNDED,
                CornerRounding::UNROUNDED,
                CornerRounding::new(0.5, 0.5),
                CornerRounding::UNROUNDED,
            ])
            .build()
        )
    }

    #[test]
    fn hour_class_measure() {
        // Regression test: Legacy measurer (AngleMeasurer) would skip the diagonal sides
        // as they are 0 degrees from the center.
        let unit = 1.0_f32;
        let coordinates = vec![
            // lower glass
            0.0,
            0.0,
            unit,
            unit,
            -unit,
            unit,

            // upper glass
            0.0,
            0.0,
            -unit,
            -unit,
            unit,
            -unit,
        ];

        let diagonal = (unit * unit + unit * unit).sqrt();
        let horizontal = 2.0 * unit;
        let total = 4.0 * diagonal + 2.0 * horizontal;

        let polygon = RoundedPolygonBuilder::from_vertices(&coordinates).build();
        custom_polygon_measure(
            &polygon,
            &vec![
                diagonal / total,
                horizontal / total,
                diagonal / total,
                diagonal / total,
                horizontal / total,
                diagonal / total,
            ],
        );
    }

    #[test]
    fn handles_empty_feature_last() {
        let triangle = RoundedPolygonBuilder::from_features(vec![
            FeatureFactory::build_convex_corner(vec![Cubic::straight_line(0.0, 0.0, 1.0, 1.0)]),
            FeatureFactory::build_convex_corner(vec![Cubic::straight_line(1.0, 1.0, 1.0, 0.0)]),
            FeatureFactory::build_convex_corner(vec![Cubic::straight_line(1.0, 0.0, 0.0, 0.0)]),
            // Empty feature at the end.
            FeatureFactory::build_convex_corner(vec![Cubic::straight_line(0.0, 0.0, 0.0, 0.0)]),
        ]).build();
        irregular_polygon_measure!(&triangle);
    }

    fn regular_polygon_measure(sides: usize, rounding: impl Into<Option<CornerRounding>>) {
        let rounding = rounding.into().unwrap_or(CornerRounding::UNROUNDED);
        irregular_polygon_measure(
            &RoundedPolygon::from_num_vertices(sides, None, None, None, rounding, None),
            |measured_polygon| {
                assert_eq!(sides, measured_polygon.len());
                measured_polygon
                    .iter()
                    .enumerate()
                    .for_each(|(index, measured_cubic)| {
                        assert_floats_equalish(
                            index as f32 / sides as f32,
                            measured_cubic.start_outline_progress,
                            None,
                            None,
                        );
                    })
            },
        )
    }

    fn custom_polygon_measure(
        polygon: &RoundedPolygon,
        progresses: &[f32],
    ) {
        irregular_polygon_measure(polygon, |measured_polygon| {
            assert_eq!(measured_polygon.len(), progresses.len());

            measured_polygon.iter().enumerate().for_each(|(index, measured_cubic)| {
                assert_equalish!(
                    progresses[index],
                    measured_cubic.end_outline_progress - measured_cubic.start_outline_progress
                );
            });
        });
    }

    fn irregular_polygon_measure(
        polygon: &RoundedPolygon,
        extra_checks: impl Fn(&MeasuredPolygon),
    ) {
        let measured_polygon = MeasuredPolygon::measured_polygon(measurer().clone(), polygon);

        // assert_eq!(0.0 , measured_polygon.first().unwrap().start_outline_progress);
        // assert_eq!(1.0 , measured_polygon.last().unwrap().end_outline_progress);
        assert_floats_equalish(
            0.0,
            measured_polygon.first().unwrap().start_outline_progress,
            None,
            None,
        );
        assert_floats_equalish(
            1.0,
            measured_polygon.last().unwrap().end_outline_progress,
            None,
            None,
        );

        for (index, measured_cubic) in measured_polygon.iter().enumerate() {
            if index > 0 {
                let prev = &measured_polygon[index - 1];
                // assert_eq!(
                //     prev.end_outline_progress,
                //     measured_cubic.start_outline_progress
                // );
                assert_floats_equalish(
                    prev.end_outline_progress,
                    measured_cubic.start_outline_progress,
                    None,
                    None,
                );
            }
            assert!(measured_cubic.end_outline_progress >= measured_cubic.start_outline_progress);
        }

        measured_polygon
            .features
            .iter()
            .enumerate()
            .for_each(|(index, progressable_feature)| {
                assert!(
                    progressable_feature.progress >= 0.0 && progressable_feature.progress < 1.0,
                    "Feature {} has invalid progress: {}",
                    index,
                    progressable_feature.progress
                );
            });
        extra_checks(&measured_polygon);
    }
}
