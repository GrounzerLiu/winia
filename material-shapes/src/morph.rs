use crate::cubic::Cubic;
use crate::feature_mapping::feature_mapper;
use crate::polygon_measure::{LengthMeasurer, MeasuredPolygon};
use crate::rounded_polygon::RoundedPolygon;
use crate::utils::{ANGLE_EPSILON, interpolate, positive_modulus, require};
use std::rc::Rc;

/// This struct is used to animate between start and end polygon objects.
///
/// Morphing between arbitrary objects can be problematic because it can be difficult to determine
/// how the points of a given shape map to the points of some other shape. [`Morph`] simplifies the
/// problem by only operating on [`RoundedPolygon`] objects, which are known to have similar,
/// contiguous structures. For one thing, the shape of a polygon is contiguous from start to end
/// (compared to an arbitrary Path object, which could have one or more `move_to` operations in the
/// shape). Also, all edges of a polygon shape are represented by [`Cubic`] objects, so the start and
/// end shapes use similar operations. Two polygon shapes then only differ in the quantity and
/// placement of their curves. The morph works by determining how to map the curves of the two shapes
/// together (based on proximity and other information, such as distance to polygon vertices and
/// concavity), and splitting curves when the shapes do not have the same number of curves or when
/// the curve placement within the shapes is very different.
pub struct Morph<'p> {
    start: &'p RoundedPolygon,
    end: &'p RoundedPolygon,
    /// The structure which holds the actual shape being morphed. It contains all cubics necessary to
    /// represent the start and end shapes (the original cubics in the shapes may be cut to align the
    /// start/end shapes), matched one to one in each Pair.
    morph_match: Vec<(Cubic, Cubic)>,
}

impl<'p> Morph<'p> {
    pub fn new(start: &'p RoundedPolygon, end: &'p RoundedPolygon) -> Self {
        let morph_match = Self::march(start, end);
        Self {
            start,
            end,
            morph_match,
        }
    }

    pub fn morph_match(&self) -> &Vec<(Cubic, Cubic)> {
        &self.morph_match
    }

    /// Calculates the axis-aligned bounds of the object.
    ///
    /// `approximate` — when `true`, uses a faster calculation to create the bounding box based on
    /// the min/max values of all anchor and control points that make up the shape. Default value is `true`.
    ///
    /// Returns the axis-aligned bounding box for this object, where the rectangle's left, top, right,
    /// and bottom values will be stored in entries 0, 1, 2, and 3, in that order.
    pub fn calculate_bounds<'b>(&self, approximate: impl Into<Option<bool>>) -> [f32; 4] {
        let approximate = approximate.into().unwrap_or(true);

        let bounds_start = self.start.calculate_bounds(approximate);
        let bounds_end = self.end.calculate_bounds(approximate);
        [
            bounds_start[0].min(bounds_end[0]),
            bounds_start[1].min(bounds_end[1]),
            bounds_start[2].max(bounds_end[2]),
            bounds_start[3].max(bounds_end[3]),
        ]
    }

    /// Like [`calculate_bounds`](Self::calculate_bounds), this function calculates the axis-aligned
    /// bounds of the object and returns that rectangle. But this function determines the max dimension
    /// of the shape (by calculating the distance from its center to the start and midpoint of each
    /// curve) and returns a square which can be used to hold the object in any rotation. This
    /// function can be used, for example, to calculate the max size of a UI element meant to hold
    /// this shape in any rotation.
    ///
    /// Returns the axis-aligned max bounding box for this object, where the rectangle's left, top,
    /// right, and bottom values will be stored in entries 0, 1, 2, and 3, in that order.
    pub fn calculate_max_bounds(&self) -> [f32; 4] {
        let start_bounds = self.start.calculate_max_bounds();
        let end_bounds = self.end.calculate_max_bounds();
        [
            start_bounds[0].min(end_bounds[0]),
            start_bounds[1].min(end_bounds[1]),
            start_bounds[2].max(end_bounds[2]),
            start_bounds[3].max(end_bounds[3]),
        ]
    }

    /// Returns a representation of the morph object at a given `progress` value as a list of [`Cubic`].
    /// Note that this function creates a new list, so there is some overhead.
    ///
    /// `progress` — a value from `0.0` to `1.0` that determines the morph's current shape, between
    /// the start and end shapes provided at construction time. A value of `0.0` results in the start
    /// shape, a value of `1.0` results in the end shape, and any value in between results in a shape
    /// which is a linear interpolation between those two shapes. The range is generally `[0..1]` and
    /// values outside it could result in undefined shapes, but values close to (but outside) the range
    /// can be used to get an exaggerated effect (e.g., for a bounce or overshoot animation).
    pub fn as_cubics(&self, progress: f32) -> Vec<Cubic> {
        let mut result = vec![];
        // The first/last mechanism here ensures that the final anchor point in the shape
        // exactly matches the first anchor point. There can be rendering artifacts introduced
        // by those points being slightly off, even by much less than a pixel
        let mut first_cubic: Option<Cubic> = None;
        let mut last_cubic: Option<Cubic> = None;
        for i in 0..self.morph_match.len() {
            let cubic = {
                let mut coordinates = [0.0f32; 8];
                for j in 0..8 {
                    coordinates[j] = interpolate(
                        self.morph_match[i].0.points[j],
                        self.morph_match[i].1.points[j],
                        progress,
                    )
                }
                Cubic::from_array(&coordinates)
            };
            if first_cubic.is_none() {
                first_cubic = Some(cubic.clone());
            }
            if let Some(last_cubic) = last_cubic {
                result.push(last_cubic);
            }
            last_cubic = Some(cubic);
        }
        if let (Some(first_cubic), Some(last_cubic)) = (first_cubic, last_cubic) {
            result.push(Cubic::from_array(&[
                last_cubic.anchor_0_x(),
                last_cubic.anchor_0_y(),
                last_cubic.control_0_x(),
                last_cubic.control_0_y(),
                last_cubic.control_1_x(),
                last_cubic.control_1_y(),
                first_cubic.anchor_0_x(),
                first_cubic.anchor_0_y(),
            ]));
        }
        result
    }

    /// Returns a representation of the morph object at a given [`progress`] value, iterating over the
    /// cubics and calling the callback. This function is faster than [`as_cubics`], since it doesn't
    /// allocate new `Cubic` instances, but it reuses the same [`Cubic`] instance during iteration.
    ///
    /// `progress` — a value from `0.0` to `1.0` that determines the morph's current shape, between
    /// the start and end shapes provided at construction time. A value of `0.0` results in the start
    /// shape, a value of `1.0` results in the end shape, and any value in between results in a shape
    /// which is a linear interpolation between those two shapes. The range is generally `[0..1]` and
    /// values outside it could result in undefined shapes, but values close to (but outside) the range
    /// can be used to get an exaggerated effect (e.g., for a bounce or overshoot animation).
    ///
    /// `callback` — the function to be called for each `Cubic`.
    fn for_each_cubic<F>(&self, progress: f32, callback: F)
    where
        F: Fn(&Cubic),
    {
        let mut mutable_cubic = Cubic::from_array(&[0.0; 8]);
        for i in 0..self.morph_match.len() {
            mutable_cubic.interpolate(&self.morph_match[i].0, &self.morph_match[i].1, progress);
            callback(&mutable_cubic);
        }
    }

    /// [`match`], called at [`Morph`] construction time, creates the structure used to animate between
    /// the start and end shapes. The technique is to match geometry (curves) between the shapes
    /// when and where possible, and to create new/placeholder curves when necessary (when one of
    /// the shapes has more curves than the other). The result is a list of pairs of [`Cubic`]
    /// curves. Those curves are the matched pairs: the first of each pair holds the geometry of
    /// the start shape, the second holds the geometry for the end shape. Changing the progress
    /// of a [`Morph`] object simply interpolates between all pairs of curves for the morph shape.
    ///
    /// Curves on both shapes are matched by running the [`Measurer`](crate::polygon_measure::Measurer) to determine where the points
    /// are in each shape (proportionally, along the outline), and then running [`feature_mapper`]
    /// which decides how to map (match) all of the curves with each other.
    fn march(p1: &RoundedPolygon, p2: &RoundedPolygon) -> Vec<(Cubic, Cubic)> {
        // Measure polygons, returns lists of measured cubics for each polygon, which
        // we then use to match start/end curves
        let measurer = Rc::new(LengthMeasurer::new());
        let measured_polygon1 = MeasuredPolygon::measured_polygon(measurer.clone(), p1);
        let measured_polygon2 = MeasuredPolygon::measured_polygon(measurer.clone(), p2);

        // features1 and 2 will contain the list of corners (just the inner circular curve)
        // along with the progress at the middle of those corners. These measurement values
        // are then used to compare and match between the two polygons
        let features1 = &measured_polygon1.features;
        let features2 = &measured_polygon2.features;

        // Map features: doubleMapper is the result of mapping the features in each shape to the
        // closest feature in the other shape.
        // Given a progress in one of the shapes it can be used to find the corresponding
        // progress in the other shape (in both directions)
        let double_mapper = feature_mapper(features1, features2);

        // cut point on poly2 is the mapping of the 0 point on poly1
        let polygon_2_cut_point = double_mapper.map(0.0);

        // Cut and rotate.
        // Polygons start at progress 0, and the featureMapper has decided that we want to match
        // progress 0 in the first polygon to `polygon2CutPoint` on the second polygon.
        // So we need to cut the second polygon there and "rotate it", so as we walk through
        // both polygons we can find the matching.
        // The resulting bs1/2 are MeasuredPolygons, whose MeasuredCubics start from
        // outlineProgress=0 and increasing until outlineProgress=1
        let bs1 = measured_polygon1;
        let bs2 = measured_polygon2.cut_and_shift(polygon_2_cut_point);

        // Match
        // Now we can compare the two lists of measured cubics and create a list of pairs
        // of cubics [ret], which are the start/end curves that represent the Morph object
        // and the start and end shapes, and which can be interpolated to animate the
        // between those shapes.
        let mut ret = vec![];
        // i1/i2 are the indices of the current cubic on the start (1) and end (2) shapes
        let mut i1 = 0;
        let mut i2 = 0;
        // b1, b2 are the current measured cubic for each polygon
        let mut ob1 = bs1.get(i1).cloned();
        i1 += 1;
        let mut ob2 = bs2.get(i2).cloned();
        i2 += 1;
        // Iterate until all curves are accounted for and matched
        while let (Some(b1), Some(b2)) = (&ob1, &ob2) {
            // Progresses are in shape1's perspective
            // b1a, b2a are ending progress values of current measured cubics in [0,1] range
            let b1a = if i1 == bs1.len() {
                1.0
            } else {
                b1.end_outline_progress()
            };
            let b2a = if i2 == bs2.len() {
                1.0
            } else {
                double_mapper.map_back(positive_modulus(
                    b2.end_outline_progress() + polygon_2_cut_point,
                    1.0,
                ))
            };
            let min_b = b1a.min(b2a);
            // min b is the progress at which the curve that ends first ends.
            // If both curves ends roughly there, no cutting is needed, we have a match.
            // If one curve extends beyond, we need to cut it.
            let (seg1, new_b1) = if b1a > min_b + ANGLE_EPSILON {
                let (first, second) = b1.cut_at_progress(min_b);
                (first, Some(second))
            } else {
                let bs1_current = bs1.get(i1);
                i1 += 1;
                (b1.clone(), bs1_current.cloned())
            };
            let (seg2, new_b2) = if b2a > min_b + ANGLE_EPSILON {
                let (first, second) = b2.cut_at_progress(positive_modulus(
                    double_mapper.map(min_b) - polygon_2_cut_point,
                    1.0,
                ));
                (first, Some(second))
            } else {
                let bs2_current = bs2.get(i2);
                i2 += 1;
                (b2.clone(), bs2_current.cloned())
            };
            ret.push((seg1.cubic, seg2.cubic));
            ob1 = new_b1;
            ob2 = new_b2;
        }
        require(
            ob1.is_none() && ob2.is_none(),
            "Expected both Polygon's Cubic to be fully matched",
        );
        ret
    }
}

#[cfg(test)]
mod morph_tests {
    use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
    use lazy_static::lazy_static;
    use crate::cubic::Cubic;
    use crate::MaterialShapes;
    use crate::morph::Morph;
    use crate::tests::cubics_equalish;

    const RADIUS: f32 = 50.0;
    lazy_static! {
        static ref POLY1: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(3)
            .center_x(0.5)
            .center_y(0.5)
            .build();
        static ref POLY2: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(4)
            .center_x(0.5)
            .center_y(0.5)
            .build();
        static ref MORPH11: Morph<'static> = Morph::new(&POLY1, &POLY1);
        static ref MORPH12: Morph<'static> = Morph::new(&POLY1, &POLY2);
    }

    /**
    * Simple test to verify that a Morph with the same start and end shape has curves equivalent to
    * those in that shape.
    */
    #[test]
    fn cubics_test() {
        let p1_cubics = &POLY1.cubics;
        let cubics11 = MORPH11.as_cubics(0.0);
        assert!(cubics11.len() > 0);
        // The structure of a morph and its component shapes may not match exactly, because morph
        // calculations may optimize some of the zero-length curves out. But in general, every
        // curve in the morph *should* exist somewhere in the shape it is based on, so we
        // do an exhaustive search for such existence. Note that this assertion only works because
        // we constructed the Morph from/to the same shape. A Morph between different shapes
        // may not have the curves replicated exactly.
        for morph_cubic in cubics11.iter() {
            let mut matched = false;
            for p1_cubic in p1_cubics.iter() {
                if cubics_equalish(morph_cubic, p1_cubic) {
                    matched = true;
                    continue;
                }
            }
            assert!(matched);
        }
    }

    #[test]
    fn morph_test() {
        let shape1 = MaterialShapes::sunny();
        let shape2 = MaterialShapes::heart();

        let morph = Morph::new(&shape1, &shape2);
        let shape_at_50 = morph.as_cubics(0.5);
        print_cubics(&shape_at_50);
    }

    fn print_cubics(cubics: &Vec<Cubic>) {
        for cubic in cubics.iter() {
            println!(
                "[{}]",
                cubic.points.iter().map(|v| format!("{:.4}", v)).collect::<Vec<String>>().join(",\t")
            )
        }
    }
}
