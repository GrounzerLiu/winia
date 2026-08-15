use crate::corner_rounding::CornerRounding;
use crate::cubic::{Cubic, PointTransformer};
use crate::feature::{Feature, FeatureTrait};
use crate::point::{Point, interpolate};
use crate::utils::{
    DISTANCE_EPSILON, FLOAT_PI, convex, direction_vector, distance, distance_squared,
    radial_to_cartesian, require,
};

pub struct RoundedPolygonBuilder<'a> {
    features: Option<Vec<Feature>>,
    num_vertices: Option<usize>,
    radius: Option<f32>,
    vertices: Option<&'a [f32]>,
    center_x: Option<f32>,
    center_y: Option<f32>,
    rounding: Option<CornerRounding>,
    per_vertex_rounding: Option<Vec<CornerRounding>>,
}

impl<'a> RoundedPolygonBuilder<'a> {
    pub fn from_num_vertices(num_vertices: usize) -> Self {
        RoundedPolygonBuilder {
            features: None,
            num_vertices: Some(num_vertices),
            radius: None,
            vertices: None,
            center_x: None,
            center_y: None,
            rounding: None,
            per_vertex_rounding: None,
        }
    }
    pub fn from_vertices(vertices: &'a [f32]) -> Self {
        RoundedPolygonBuilder {
            features: None,
            num_vertices: None,
            radius: None,
            vertices: Some(vertices),
            center_x: None,
            center_y: None,
            rounding: None,
            per_vertex_rounding: None,
        }
    }

    pub fn from_features(features: Vec<Feature>) -> Self {
        RoundedPolygonBuilder {
            features: Some(features),
            num_vertices: None,
            radius: None,
            vertices: None,
            center_x: None,
            center_y: None,
            rounding: None,
            per_vertex_rounding: None,
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn center_x(mut self, center_x: f32) -> Self {
        self.center_x = Some(center_x);
        self
    }

    pub fn center_y(mut self, center_y: f32) -> Self {
        self.center_y = Some(center_y);
        self
    }

    pub fn rounding(mut self, rounding: CornerRounding) -> Self {
        self.rounding = Some(rounding);
        self
    }

    pub fn per_vertex_rounding(mut self, per_vertex_rounding: Vec<CornerRounding>) -> Self {
        self.per_vertex_rounding = Some(per_vertex_rounding);
        self
    }

    pub fn build(self) -> RoundedPolygon {
        if let Some(features) = self.features {
            RoundedPolygon::from_features(features, self.center_x, self.center_y)
        } else if let Some(num_vertices) = self.num_vertices {
            RoundedPolygon::from_num_vertices(
                num_vertices,
                self.radius,
                self.center_x,
                self.center_y,
                self.rounding,
                self.per_vertex_rounding,
            )
        } else if let Some(vertices) = self.vertices {
            RoundedPolygon::from_vertices(
                vertices,
                self.rounding,
                self.per_vertex_rounding,
                self.center_x,
                self.center_y,
            )
        } else {
            panic!(
                "RoundedPolygonBuilder requires either a feature, number of vertices, or vertices to build a RoundedPolygon."
            )
        }
    }
}

/// The [`RoundedPolygon`] struct allows simple construction of polygonal shapes with optional
/// rounding at the vertices. Polygons can be constructed with either the number of vertices desired
/// or an ordered list of vertices.
pub struct RoundedPolygon {
    pub features: Vec<Feature>,
    /// A flattened version of the [`Feature`]s, as a List<Cubic>.
    pub cubics: Vec<Cubic>,
    pub center: Point,
}

impl RoundedPolygon {
    pub fn new(features: Vec<Feature>, center: Point) -> Self {
        let mut cubics: Vec<Cubic> = Vec::new();
        let mut first_cubic: Option<Cubic> = None;
        let mut last_cubic: Option<Cubic> = None;
        let mut first_feature_split_start: Option<Vec<Cubic>> = None;
        let mut first_feature_split_end: Option<Vec<Cubic>> = None;
        if features.len() > 0 && features[0].cubics().len() == 3 {
            let center_cubic = features[0].cubics()[1].clone();
            let (start, end) = center_cubic.split(0.5);
            first_feature_split_start = Some(vec![features[0].cubics()[0].clone(), start]);
            first_feature_split_end = Some(vec![end, features[0].cubics()[2].clone()]);
        }

        for i in 0..=features.len() {
            let feature_cubics = if i == 0
                && let Some(first_feature_split_end) = first_feature_split_end.clone()
            {
                first_feature_split_end
            } else if i == features.len() {
                if let Some(start) = first_feature_split_start.clone() {
                    start
                } else {
                    break;
                }
            } else {
                features[i].cubics().clone()
            };
            for cubic in feature_cubics {
                if !cubic.zero_length() {
                    if let Some(last_cubic) = last_cubic {
                        cubics.push(last_cubic);
                    }
                    last_cubic = Some(cubic);
                    if first_cubic.is_none() {
                        first_cubic = Some(cubic);
                    }
                } else {
                    if let Some(last_cubic) = &mut last_cubic {
                        last_cubic.points[6] = cubic.anchor_1_x();
                        last_cubic.points[7] = cubic.anchor_1_y();
                    }
                }
            }
        }
        if let Some(last_cubic) = last_cubic
            && let Some(first_cubic) = first_cubic
        {
            cubics.push(Cubic::new(
                last_cubic.anchor_0_x(),
                last_cubic.anchor_0_y(),
                last_cubic.control_0_x(),
                last_cubic.control_0_y(),
                last_cubic.control_1_x(),
                last_cubic.control_1_y(),
                first_cubic.anchor_0_x(),
                first_cubic.anchor_0_y(),
            ));
        } else {
            cubics.push(Cubic::new(
                center.0, center.1, center.0, center.1, center.0, center.1, center.0, center.1,
            ))
        }

        {
            let mut prev_cubic = cubics.last().unwrap();
            for cubic in &cubics {
                if (cubic.anchor_0_x() - prev_cubic.anchor_1_x()).abs() > DISTANCE_EPSILON
                    || (cubic.anchor_0_y() - prev_cubic.anchor_1_y()).abs() > DISTANCE_EPSILON
                {
                    panic!(
                        "RoundedPolygon must be contiguous, with the anchor points of all curves matching the anchor points of the preceding and succeeding cubics"
                    )
                }
                prev_cubic = cubic;
            }
        }

        RoundedPolygon {
            features,
            cubics,
            center,
        }
    }

    /// Constructs a [`RoundedPolygon`] with the specified number of vertices. These vertices are
    /// positioned on a virtual circle around a given center, with each vertex at a distance of
    /// `radius` from the center, equally spaced (with equal angles between them). If no `radius` is
    /// supplied, the shape will be created with a default radius of `1.0`, resulting in a shape whose
    /// vertices lie on a unit circle, with width/height of `2.0`. That default polygon will probably
    /// need to be rescaled using [`transformed`](RoundedPolygon::transformed) to the appropriate size for the UI in which it will be drawn.
    ///
    /// The `rounding` and `per_vertex_rounding` parameters are optional. If not supplied, the result
    /// will be a regular polygon with straight edges and unrounded corners.
    ///
    /// `num_vertices` — the number of vertices in this polygon.
    /// `radius` — the radius of the polygon, in pixels. Determines the initial size of the object, but
    ///     it can be transformed later by using [`transformed`](RoundedPolygon::transformed).
    /// `center_x` — the X coordinate of the center of the polygon. Default is `0.0`.
    /// `center_y` — the Y coordinate of the center of the polygon. Default is `0.0`.
    /// `rounding` — the [`CornerRounding`] properties of all vertices. If some vertices should have
    ///     different rounding properties, use `per_vertex_rounding` instead. Default is
    ///     [`CornerRounding::UNROUNDED`], meaning the polygon uses the vertices themselves without curves.
    /// `per_vertex_rounding` — the [`CornerRounding`] properties of every vertex. If not `None`, it
    ///     must have `num_vertices` elements. If `None`, the polygon uses the `rounding` parameter for
    ///     every vertex. Default is `None`.
    ///
    /// # Panics
    /// - If `per_vertex_rounding` is not `None` and its length is not equal to `num_vertices`.
    /// - If `num_vertices` is less than 3.
    pub fn from_num_vertices(
        num_vertices: usize,
        radius: impl Into<Option<f32>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
        rounding: impl Into<Option<CornerRounding>>,
        per_vertex_rounding: impl Into<Option<Vec<CornerRounding>>>,
    ) -> RoundedPolygon {
        let center_x = center_x.into();
        let center_y = center_y.into();
        RoundedPolygon::from_vertices(
            &vertices_from_num_verts(
                num_vertices,
                radius.into().unwrap_or(1.0),
                center_x.unwrap_or(0.0),
                center_y.unwrap_or(0.0),
            ),
            rounding,
            per_vertex_rounding,
            center_x,
            center_y,
        )
    }

    /// Takes the vertices (either supplied or calculated, depending on the constructor called),
    /// plus [`CornerRounding`] parameters, and creates the actual [`RoundedPolygon`] shape, rounding
    /// around the vertices (or not) as specified. The result is a list of `Cubic` curves which
    /// represent the geometry of the final shape.
    ///
    /// `vertices` — the list of vertices in this polygon specified as pairs of x/y coordinates in
    ///     this slice. This should be an ordered list (with the outline of the shape going from each
    ///     vertex to the next in order of this list); otherwise the results will be undefined.
    /// `rounding` — the [`CornerRounding`] properties of all vertices. If some vertices should have
    ///     different rounding properties, then use `per_vertex_rounding` instead. Default is
    ///     [`CornerRounding::UNROUNDED`], meaning the polygon uses the vertices themselves and not
    ///     curves around them.
    /// `per_vertex_rounding` — the [`CornerRounding`] properties of all vertices. If not `None`,
    ///     it must have the same length as `vertices`. If `None`, the polygon uses `rounding` for
    ///     every vertex. Default is `None`.
    /// `center_x` — the X coordinate of the center of the polygon. Default is `0.0`.
    /// `center_y` — the Y coordinate of the center of the polygon. Default is `0.0`.
    ///
    /// # Panics
    /// - If the number of vertices is less than 3 (i.e., `vertices` has less than 6 floats).
    /// - If `per_vertex_rounding` is not `None` and its length does not match the number of vertices.
    // TODO(performance): Update the map calls to more efficient code that doesn't allocate Iterators
    //  unnecessarily.
    pub fn from_vertices(
        vertices: &[f32],
        rounding: impl Into<Option<CornerRounding>>,
        per_vertex_rounding: impl Into<Option<Vec<CornerRounding>>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> RoundedPolygon {
        let rounding = rounding.into().unwrap_or(CornerRounding::UNROUNDED);
        let per_vertex_rounding = per_vertex_rounding.into();
        let center_x = center_x.into().unwrap_or(f32::MIN);
        let center_y = center_y.into().unwrap_or(f32::MIN);
        if vertices.len() < 6 {
            panic!("Polygon must have at least 3 vertices");
        }
        if vertices.len() % 2 == 1 {
            panic!("The vertices array should have even size");
        }
        if let Some(per_vertex_rounding) = &per_vertex_rounding
            && per_vertex_rounding.len() * 2 != vertices.len()
        {
            panic!(
                "per_vertex_rounding list should be either `Option::None` or the same length as the number of vertices (vertices.len() / 2)."
            );
        }
        let mut corners = Vec::new();
        let n = vertices.len() / 2;
        let mut rounded_corners = Vec::new();
        for i in 0..n {
            let vtx_rounding = if let Some(per_vertex_rounding) = &per_vertex_rounding {
                per_vertex_rounding[i].clone()
            } else {
                rounding
            };
            let prev_index = ((i + n - 1) % n) * 2;
            let next_index = ((i + 1) % n) * 2;
            rounded_corners.push(RoundedCorner::new(
                Point(vertices[prev_index], vertices[prev_index + 1]),
                Point(vertices[i * 2], vertices[i * 2 + 1]),
                Point(vertices[next_index], vertices[next_index + 1]),
                vtx_rounding,
            ));
        }
        // For each side, check if we have enough space to do the cuts needed, and if not split
        // the available space, first for round cuts, then for smoothing if there is space left.
        // Each element in this list is a pair, that represent how much we can do of the cut for
        // the given side (side i goes from corner i to corner i+1), the elements of the pair are:
        // first is how much we can use of expectedRoundCut, second how much of expectedCut
        let cut_adjust: Vec<(f32, f32)> = (0..n)
            .map(|ix| {
                let expected_round_cut = rounded_corners[ix].expected_round_cut
                    + rounded_corners[(ix + 1) % n].expected_round_cut;
                let expected_cut = rounded_corners[ix].expected_cut()
                    + rounded_corners[(ix + 1) % n].expected_cut();
                let vtx_x = vertices[ix * 2];
                let vtx_y = vertices[ix * 2 + 1];
                let next_vtx_x = vertices[((ix + 1) % n) * 2];
                let next_vtx_y = vertices[((ix + 1) % n) * 2 + 1];
                let side_size = distance(vtx_x - next_vtx_x, vtx_y - next_vtx_y);

                // Check expectedRoundCut first, and ensure we fulfill rounding needs first for
                // both corners before using space for smoothing
                if expected_round_cut > side_size {
                    // Not enough room for fully rounding, see how much we can actually do.
                    (side_size / expected_round_cut, 0.0)
                } else if expected_cut > side_size {
                    (
                        1.0,
                        (side_size - expected_round_cut) / (expected_cut - expected_round_cut),
                    )
                } else {
                    (1.0, 1.0)
                }
            })
            .collect();
        // Create and store list of Béziers for each [potentially] rounded corner
        for i in 0..n {
            // allowedCuts[0] is for the side from the previous corner to this one,
            // allowedCuts[1] is for the side from this corner to the next one.
            let mut allowed_cuts = Vec::with_capacity(2);
            for delta in 0..=1 {
                let (round_cut_ratio, cut_ratio) = cut_adjust[(i + n - 1 + delta) % n];
                allowed_cuts.push(
                    rounded_corners[i].expected_round_cut * round_cut_ratio
                        + (rounded_corners[i].expected_cut()
                            - rounded_corners[i].expected_round_cut)
                            * cut_ratio,
                );
            }
            let cubics = rounded_corners[i].get_cubics(allowed_cuts[0], allowed_cuts[1]);
            corners.push(cubics);
            // corners.push(rounded_corners[i].get_cubics(allowed_cuts[0], allowed_cuts[1]))
        }
        // Finally, store the calculated cubics. This includes all of the rounded corners
        // from above, along with new cubics representing the edges between those corners.
        let mut temp_features: Vec<Feature> = Vec::new();
        for i in 0..n {
            // Note that these indices are for pairs of values (points), they need to be
            // doubled to access the xy values in the vertices float array
            let prev_vtx_index = (i + n - 1) % n;
            let next_vtx_index = (i + 1) % n;
            let curr_vertex = Point(vertices[i * 2], vertices[i * 2 + 1]);
            let prev_vertex = Point(
                vertices[prev_vtx_index * 2],
                vertices[prev_vtx_index * 2 + 1],
            );
            let next_vertex = Point(
                vertices[next_vtx_index * 2],
                vertices[next_vtx_index * 2 + 1],
            );
            let convex = convex(prev_vertex, curr_vertex, next_vertex);
            temp_features.push(Feature::corner(corners[i].clone(), convex));
            temp_features.push(Feature::edge(vec![Cubic::straight_line(
                corners[i].last().unwrap().anchor_1_x(),
                corners[i].last().unwrap().anchor_1_y(),
                corners[(i + 1) % n].first().unwrap().anchor_0_x(),
                corners[(i + 1) % n].first().unwrap().anchor_0_y(),
            )]))
        }
        let (cx, cy) = if center_x == f32::MIN || center_y == f32::MIN {
            let center = Self::calculate_center(vertices);
            (center.0, center.1)
        } else {
            (center_x, center_y)
        };
        RoundedPolygon::from_features(temp_features, cx, cy)
    }

    /// Constructs a [`RoundedPolygon`] from a list of [`Feature`] objects that define the polygon's
    /// shape and curves. By specifying the features directly, the summarization of [`Cubic`] objects
    /// to curves can be precisely controlled. This affects [`Morph`](crate::morph::Morph)’s default mapping, as curves
    /// with the same type (convex or concave) are mapped with each other. For example, if you have a
    /// convex curve in your start polygon, [`Morph`](crate::morph::Morph) will map it to another convex curve in the end polygon.
    ///
    /// The `center_x` and `center_y` parameters are optional. If not supplied, they will be estimated
    /// by calculating the average of all cubic anchor points.
    ///
    /// `features` — the [`Feature`]s that describe the characteristics of each outline segment of the polygon.
    /// `center_x` — the X coordinate of the center of the polygon. If `None`, the center will be averaged.
    /// `center_y` — the Y coordinate of the center of the polygon. If `None`, the center will be averaged.
    ///
    /// # Panics
    /// - If `features` contains fewer than 2 elements or does not describe a closed shape.
    fn from_features(
        features: Vec<Feature>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> RoundedPolygon {
        require(features.len() >= 2, "Polygon must have at least 2 features");
        let mut vertices = vec![];
        for feature in &features {
            for cubic in feature.cubics() {
                vertices.push(cubic.anchor_0_x());
                vertices.push(cubic.anchor_0_y());
            }
        }
        let center_x = center_x.into();
        let center_y = center_y.into();
        let cx = if let Some(cx) = center_x {
            cx
        } else {
            Self::calculate_center(&vertices).0
        };
        let cy = if let Some(cy) = center_y {
            cy
        } else {
            Self::calculate_center(&vertices).1
        };
        RoundedPolygon::new(features, Point(cx, cy))
    }

    /** Creates a copy of the given [RoundedPolygon] */
    pub fn from_polygon(source: &RoundedPolygon) -> RoundedPolygon {
        RoundedPolygon::new(source.features.clone(), source.center.clone())
    }

    /// Calculates an estimated center position for the polygon and returns it. This function should
    /// only be called if the center is not already calculated or provided. The [`RoundedPolygon`]
    /// constructor which takes `num_vertices` calculates its own center, since it knows exactly where
    /// it is centered, at `(0, 0)`.
    ///
    /// Note that this center will be transformed whenever the shape itself is transformed. Any
    /// transforms that occur before the center is calculated will be taken into account automatically,
    /// since the center calculation is an average of the current location of all cubic anchor points.
    fn calculate_center(vertices: &[f32]) -> Point {
        let mut cumulative_x = 0.0_f32;
        let mut cumulative_y = 0.0_f32;
        let mut index = 0;
        while index < vertices.len() {
            cumulative_x += vertices[index];
            index += 1;
            cumulative_y += vertices[index];
            index += 1;
        }
        Point(
            cumulative_x / (vertices.len() as f32 / 2.0),
            cumulative_y / (vertices.len() as f32 / 2.0),
        )
    }

    pub fn center_x(&self) -> f32 {
        self.center.0
    }
    pub fn center_y(&self) -> f32 {
        self.center.1
    }

    /// Transforms (scales/translates/etc.) this [`RoundedPolygon`] with the given [`PointTransformer`]
    /// and returns a new [`RoundedPolygon`]. This is a low-level API and there should be more
    /// platform-idiomatic ways to transform a [`RoundedPolygon`] provided by the platform-specific wrapper.
    ///
    /// `f` — the [`PointTransformer`] used to transform this [`RoundedPolygon`].
    pub fn transformed(&self, f: &PointTransformer) -> RoundedPolygon {
        let center = self.center.transformed(f);
        RoundedPolygon::from_features(
            self.features
                .iter()
                .map(|feature| feature.transformed(f))
                .collect(),
            center.x(),
            center.y(),
        )
    }
    /// Creates a new [`RoundedPolygon`], moving and resizing this one, so it's completely inside the
    /// `(0, 0) -> (1, 1)` square, centered if there extra space in one direction
    pub fn normalized(&self) -> RoundedPolygon {
        let bounds = self.calculate_bounds(None);
        let width = bounds[2] - bounds[0];
        let height = bounds[3] - bounds[1];
        let side = width.max(height);
        let offset_x = (side - width) / 2.0 - bounds[0];
        let offset_y = (side - height) / 2.0 - bounds[1];
        self.transformed(&move |x, y| Point((x + offset_x) / side, (y + offset_y) / side))
    }
    /// Like [`calculate_bounds`](Self::calculate_bounds), this function calculates the axis-aligned bounds of the object and
    /// returns that rectangle. But this function determines the max dimension of the shape (by
    /// calculating the distance from its center to the start and midpoint of each curve) and returns
    /// a square which can be used to hold the object in any rotation. This function can be used, for
    /// example, to calculate the max size of a UI element meant to hold this shape in any rotation.
    ///
    /// `bounds` — a buffer to hold the results. If not supplied, a temporary buffer will be created.
    ///
    /// Returns the axis-aligned max bounding box for this object, where the rectangle's left, top,
    /// right, and bottom values are stored in entries 0, 1, 2, and 3, in that order.
    pub fn calculate_max_bounds<'a>(&self) -> [f32; 4] {
        let mut max_dist_squared = 0.0_f32;
        for cubic in &self.cubics {
            let anchor_distance = distance_squared(
                cubic.anchor_0_x() - self.center_x(),
                cubic.anchor_0_y() - self.center_y(),
            );
            let middle_point = cubic.point_on_curve(0.5);
            let middle_distance = distance_squared(
                middle_point.0 - self.center_x(),
                middle_point.1 - self.center_y(),
            );
            max_dist_squared = max_dist_squared.max(anchor_distance.max(middle_distance));
        }
        let distance = max_dist_squared.sqrt();

        [
            self.center_x() - distance,
            self.center_y() - distance,
            self.center_x() + distance,
            self.center_y() + distance,
        ]
    }
    /// Calculates the axis-aligned bounds of the object.
    ///
    /// `approximate` — when `true`, uses a faster calculation to create the bounding box based on
    /// the min/max values of all anchor and control points that make up the shape. Default is `true`.
    ///
    /// Returns the axis-aligned bounding box for this object, where the rectangle's left, top, right,
    /// and bottom values are stored in entries 0, 1, 2, and 3, in that order.
    pub fn calculate_bounds(&self, approximate: impl Into<Option<bool>>) -> [f32; 4] {
        let approximate = approximate.into().unwrap_or(true);
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        let mut temp_bounds = [0.0; 4];
        for cubic in &self.cubics {
            cubic.calculate_bounds(&mut temp_bounds, approximate);
            min_x = min_x.min(temp_bounds[0]);
            min_y = min_y.min(temp_bounds[1]);
            max_x = max_x.max(temp_bounds[2]);
            max_y = max_y.max(temp_bounds[3]);
        }
        [min_x, min_y, max_x, max_y]
    }
}

/// Private utility struct that holds information about each corner in a polygon. The shape of the
/// corner can be obtained by calling [`get_cubics`](RoundedCorner::get_cubics), which returns a list of curves representing
/// the corner geometry. The shape of the corner depends on the `rounding` constructor parameter.
///
/// If `rounding` is `None`, there is no rounding; the corner will simply be a single point at `p1`.
/// This point will be represented by a [`Cubic`] of length 0 at that point.
///
/// If `rounding` is `Some`, the corner will be rounded either with a curve approximating a circular
/// arc of the radius specified in `rounding`, or with three curves if `rounding` has a nonzero
/// smoothing parameter. These three curves are a circular arc in the middle and two symmetrical
/// flanking curves on either side. The smoothing parameter determines the curvature of the flanking
/// curves.
///
/// This is a struct because we usually need to do the work in two steps and prefer to keep state
/// between them: first we determine how much we want to cut to comply with the parameters, then we
/// are given how much we can actually cut (because of space restrictions outside this corner).
///
/// `p0` — the vertex before the one being rounded.  
/// `p1` — the vertex of this rounded corner.  
/// `p2` — the vertex after the one being rounded.  
/// `rounding` — the optional parameters specifying how this corner should be rounded.
struct RoundedCorner {
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub rounding: Option<CornerRounding>,
    pub d1: Point,
    pub d2: Point,
    pub corner_radius: f32,
    pub smoothing: f32,
    pub cos_angle: f32,
    pub sin_angle: f32,
    pub expected_round_cut: f32,
    pub center: Point,
}

impl RoundedCorner {
    pub fn new(
        p0: Point,
        p1: Point,
        p2: Point,
        rounding: impl Into<Option<CornerRounding>>,
    ) -> RoundedCorner {
        let rounding = rounding.into();
        let v01 = p0 - p1;
        let v21 = p2 - p1;
        let d01 = v01.get_distance();
        let d21 = v21.get_distance();
        if d01 > 0.0 && d21 > 0.0 {
            let d1 = v01 / d01;
            let d2 = v21 / d21;
            let corner_radius = rounding.as_ref().map_or(0.0, |r| r.radius);
            let cos_angle = d1.dot_product(d2);
            let sin_angle = (1.0 - cos_angle * cos_angle).sqrt();
            Self {
                p0,
                p1,
                p2,
                rounding,
                d1,
                d2,
                corner_radius,
                smoothing: rounding.as_ref().map_or(0.0, |r| r.smoothing),
                cos_angle,
                sin_angle,
                expected_round_cut: if sin_angle > 1e-3 {
                    corner_radius * (cos_angle + 1.0) / sin_angle
                } else {
                    0.0
                },
                center: Point(0.0, 0.0),
            }
        } else {
            Self {
                p0,
                p1,
                p2,
                rounding,
                d1: Point(0.0, 0.0),
                d2: Point(0.0, 0.0),
                corner_radius: 0.0,
                smoothing: 0.0,
                cos_angle: 0.0,
                sin_angle: 0.0,
                expected_round_cut: 0.0,
                center: Point(0.0, 0.0),
            }
        }
    }

    pub fn expected_cut(&self) -> f32 {
        (1.0 + self.smoothing) * self.expected_round_cut
    }

    pub fn get_cubics(
        &mut self,
        allowed_cut0: f32,
        allowed_cut1: impl Into<Option<f32>>,
    ) -> Vec<Cubic> {
        let allowed_cut1 = allowed_cut1.into().unwrap_or(allowed_cut0);
        // We use the minimum of both cuts to determine the radius, but if there is more space
        // in one side we can use it for smoothing.
        let allowed_cut = allowed_cut0.min(allowed_cut1);
        // Nothing to do, just use lines, or a point
        if self.expected_round_cut < DISTANCE_EPSILON
            || allowed_cut < DISTANCE_EPSILON
            || self.corner_radius < DISTANCE_EPSILON
        {
            self.center = self.p1;
            return vec![Cubic::straight_line(
                self.p1.x(),
                self.p1.y(),
                self.p1.x(),
                self.p1.y(),
            )];
        };
        // How much of the cut is required for the rounding part.
        let actual_round_cut = allowed_cut.min(self.expected_round_cut);
        // We have two smoothing values, one for each side of the vertex
        // Space is used for rounding values first. If there is space left over, then we
        // apply smoothing, if it was requested
        let actual_smoothing0 = self.calculate_actual_smoothing_values(allowed_cut0);
        let actual_smoothing1 = self.calculate_actual_smoothing_values(allowed_cut1);
        // Scale the radius if needed
        let actual_r = self.corner_radius * actual_round_cut / self.expected_round_cut;
        // Distance from the corner (p1) to the center
        let center_distance = (actual_r * actual_r + actual_round_cut * actual_round_cut).sqrt();
        // Center of the arc we will use for rounding
        self.center = self.p1 + ((self.d1 + self.d2) / 2.0).get_direction() * center_distance;
        let circle_intersection0 = self.p1 + self.d1 * actual_round_cut;
        let circle_intersection2 = self.p1 + self.d2 * actual_round_cut;
        let flanking0 = Self::compute_flanking_curve(
            actual_round_cut,
            actual_smoothing0,
            self.p1,
            self.p0,
            circle_intersection0,
            circle_intersection2,
            self.center,
            actual_r,
        );
        let flanking2 = Self::compute_flanking_curve(
            actual_round_cut,
            actual_smoothing1,
            self.p1,
            self.p2,
            circle_intersection2,
            circle_intersection0,
            self.center,
            actual_r,
        )
        .reverse();
        let c = Cubic::circular_arc(
            self.center.x(),
            self.center.y(),
            flanking0.anchor_1_x(),
            flanking0.anchor_1_y(),
            flanking2.anchor_0_x(),
            flanking2.anchor_0_y(),
        );
        vec![
            flanking0,
            Cubic::circular_arc(
                self.center.x(),
                self.center.y(),
                flanking0.anchor_1_x(),
                flanking0.anchor_1_y(),
                flanking2.anchor_0_x(),
                flanking2.anchor_0_y(),
            ),
            flanking2,
        ]
    }
    /// Computes a Bézier curve to connect the linear segment defined by `corner` and `side_start`
    /// with the circular segment defined by `circle_center`, `circle_segment_intersection`,
    /// `other_circle_segment_intersection`, and `actual_r`. The Bezier will start at the linear
    /// segment and end on the circular segment.
    ///
    /// `actual_round_cut` — how much we are cutting off the corner to add the circular segment (before smoothing, which may cut more).
    /// `actual_smoothing_values` — how much we want to smooth (the smooth parameter, adjusted down if there is not enough room).
    /// `corner` — the point at which the linear side ends.
    /// `side_start` — the point at which the linear side starts.
    /// `circle_segment_intersection` — the point at which the linear side and the circle intersect.
    /// `other_circle_segment_intersection` — the point at which the opposing linear side and the circle intersect.
    /// `circle_center` — the center of the circle.
    /// `actual_r` — the radius of the circle.
    ///
    /// Returns a Bezier cubic curve that connects from the (cut) linear side to the (cut) circular
    /// segment in a smooth way.
    fn compute_flanking_curve(
        actual_round_cut: f32,
        actual_smoothing_values: f32,
        corner: Point,
        side_start: Point,
        circle_segment_intersection: Point,
        other_circle_segment_intersection: Point,
        circle_center: Point,
        actual_r: f32,
    ) -> Cubic {
        // sideStart is the anchor, 'anchor' is actual control point
        let side_direction = (side_start - corner).get_direction();
        let curve_start =
            corner + side_direction * actual_round_cut * (1.0 + actual_smoothing_values);
        // We use an approximation to cut a part of the circle section proportional to 1 - smooth,
        // When smooth = 0, we take the full section, when smooth = 1, we take nothing.
        // TODO: revisit this, it can be problematic as it approaches 180 degrees
        let p = interpolate(
            circle_segment_intersection,
            (circle_segment_intersection + other_circle_segment_intersection) / 2.0,
            actual_smoothing_values,
        );
        // The flanking curve ends on the circle
        let curve_end = circle_center
            + direction_vector(p.x() - circle_center.x(), p.y() - circle_center.y()) * actual_r;
        // The anchor on the circle segment side is in the intersection between the tangent to the
        // circle in the circle/flanking curve boundary and the linear segment.
        let circle_tangent = (curve_end - circle_center).rotate_90();
        let anchor_end =
            Self::line_intersection(side_start, side_direction, curve_end, circle_tangent)
                .unwrap_or(circle_segment_intersection);
        // From what remains, we pick a point for the start anchor.
        // 2/3 seems to come from design tools?
        let anchor_start = (curve_start + anchor_end * 2.0) / 3.0;
        Cubic::from_points(curve_start, anchor_start, anchor_end, curve_end)
    }

    /// Returns the intersection point of the two lines d0->d1 and p0->p1, or null if the lines do
    /// not intersect
    fn line_intersection(p0: Point, d0: Point, p1: Point, d1: Point) -> Option<Point> {
        let rotated_d1 = d1.rotate_90();
        let den = d0.dot_product(rotated_d1);
        if den.abs() < DISTANCE_EPSILON {
            return None;
        }
        let num = (p1 - p0).dot_product(rotated_d1);
        // Also check the relative value. This is equivalent to abs(den/num) < DistanceEpsilon,
        // but avoid doing a division
        if (num.abs() * DISTANCE_EPSILON) > den.abs() {
            return None;
        }
        let k = num / den;
        Some(p0 + d0 * k)
    }

    fn calculate_actual_smoothing_values(&self, allowed_cut: f32) -> f32 {
        if allowed_cut > self.expected_cut() {
            self.smoothing
        } else if allowed_cut > self.expected_round_cut {
            self.smoothing * (allowed_cut - self.expected_round_cut)
                / (self.expected_cut() - self.expected_round_cut)
        } else {
            0.0
        }
    }
}

fn vertices_from_num_verts(
    num_vertices: usize,
    radius: f32,
    center_x: f32,
    center_y: f32,
) -> Vec<f32> {
    let mut result: Vec<f32> = Vec::with_capacity(num_vertices * 2);
    for i in 0..num_vertices {
        let vertex = radial_to_cartesian(
            radius,
            FLOAT_PI / num_vertices as f32 * 2.0 * i as f32,
            None,
        ) + Point(center_x, center_y);
        result.push(vertex.x());
        result.push(vertex.y());
    }
    result
}

#[cfg(test)]
mod rounded_polygon_tests {
    use crate::assert_panic;
    use crate::corner_rounding::CornerRounding;
    use crate::cubic::Cubic;
    use crate::feature::{Feature, FeatureFactory, FeatureTrait};
    use crate::point::Point;
    use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
    use crate::tests::{assert_floats_equalish, assert_in_bounds, assert_polygons_equalish};
    use lazy_static::lazy_static;
    use std::ops::MulAssign;

    lazy_static! {
        static ref ROUNDING: CornerRounding = CornerRounding::new(0.1, None);
        static ref PER_VTX_ROUNDED: Vec<CornerRounding> = vec![
            ROUNDING.clone(),
            ROUNDING.clone(),
            ROUNDING.clone(),
            ROUNDING.clone()
        ];
    }

    #[test]
    fn num_verts_constructor_test() {
        assert_panic!({ RoundedPolygonBuilder::from_num_vertices(2).build() });

        let square = RoundedPolygonBuilder::from_num_vertices(4).build();
        let mut min = Point(-1.0, -1.0);
        let mut max = Point(1.0, 1.0);
        assert_in_bounds(&square.cubics, min, max);

        let double_square = RoundedPolygonBuilder::from_num_vertices(4)
            .radius(2.0)
            .build();
        min *= 2.0;
        max *= 2.0;
        assert_in_bounds(&double_square.cubics, min, max);

        let square_rounded = RoundedPolygonBuilder::from_num_vertices(4)
            .rounding(ROUNDING.clone())
            .build();
        min = Point(-1.0, -1.0);
        max = Point(1.0, 1.0);
        assert_in_bounds(&square_rounded.cubics, min, max);

        let square_pv_rounded = RoundedPolygonBuilder::from_num_vertices(4)
            .per_vertex_rounding(PER_VTX_ROUNDED.clone())
            .build();
        min = Point(-1.0, -1.0);
        max = Point(1.0, 1.0);
        assert_in_bounds(&square_pv_rounded.cubics, min, max);
    }

    #[test]
    fn vertices_constructor_test() {
        let p0 = Point(1.0, 0.0);
        let p1 = Point(0.0, 1.0);
        let p2 = Point(-1.0, 0.0);
        let p3 = Point(0.0, -1.0);
        let verts = vec![
            p0.x(),
            p0.y(),
            p1.x(),
            p1.y(),
            p2.x(),
            p2.y(),
            p3.x(),
            p3.y(),
        ];

        assert_panic!({
            RoundedPolygon::from_vertices(
                &vec![p0.x(), p0.y(), p1.x(), p1.y()],
                None,
                None,
                None,
                None,
            )
        });

        let manual_square = RoundedPolygonBuilder::from_vertices(&verts).build();
        let mut min = Point(-1.0, -1.0);
        let mut max = Point(1.0, 1.0);
        assert_in_bounds(&manual_square.cubics, min, max);

        let offset = Point(1.0, 2.0);
        let offset_verts = [
            p0.x() + offset.x(),
            p0.y() + offset.y(),
            p1.x() + offset.x(),
            p1.y() + offset.y(),
            p2.x() + offset.x(),
            p2.y() + offset.y(),
            p3.x() + offset.x(),
            p3.y() + offset.y(),
        ];
        let manual_square_offset = RoundedPolygonBuilder::from_vertices(&offset_verts)
            .center_x(offset.x())
            .center_y(offset.y())
            .build();
        min = Point(0.0, 1.0);
        max = Point(2.0, 3.0);
        assert_in_bounds(&manual_square_offset.cubics, min, max);

        let manual_square_rounded = RoundedPolygonBuilder::from_vertices(&verts)
            .rounding(ROUNDING.clone())
            .build();
        min = Point(-1.0, -1.0);
        max = Point(1.0, 1.0);
        assert_in_bounds(&manual_square_rounded.cubics, min, max);

        let manual_square_pv_rounded = RoundedPolygonBuilder::from_vertices(&verts)
            .per_vertex_rounding(PER_VTX_ROUNDED.clone())
            .build();
        min = Point(-1.0, -1.0);
        max = Point(1.0, 1.0);
        assert_in_bounds(&manual_square_pv_rounded.cubics, min, max);
    }

    #[test]
    fn features_constructor_panic_for_too_few_features() {
        assert_panic!({ RoundedPolygon::from_features(vec![], None, None) });
        let corner = Feature::corner(vec![Cubic::empty(0.0, 0.0)], true);
        assert_panic!({ RoundedPolygon::from_features(vec![corner], None, None) });
    }

    #[test]
    fn features_constructor_panic_for_non_continuous_features() {
        let cubic1 = Cubic::straight_line(0.0, 0.0, 1.0, 0.0);
        let cubic2 = Cubic::straight_line(10.0, 10.0, 20.0, 20.0);
        assert_panic!({
            RoundedPolygonBuilder::from_features(vec![
                FeatureFactory::build_edge(cubic1),
                FeatureFactory::build_edge(cubic2),
            ])
            .build()
        });
    }

    #[test]
    fn features_constructor_reconstructs_square() {
        let base = RoundedPolygon::rectangle(None, None, None, None, None, None);
        let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
        assert_polygons_equalish(&base, &actual);
    }

    #[test]
    fn features_constructor_reconstructs_rounded_square() {
        let base =
            RoundedPolygon::rectangle(None, None, CornerRounding::new(0.5, 0.2), None, None, None);
        let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
        assert_polygons_equalish(&base, &actual);
    }
    #[test]
    fn features_constructor_reconstructs_circles() {
        for i in 3..=20 {
            let base = RoundedPolygon::circle(i, None, None, None);
            let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
            assert_polygons_equalish(&base, &actual);
        }
    }

    #[test]
    fn features_constructor_reconstructs_starts() {
        for i in 3..=20 {
            let base = RoundedPolygon::star(i, None, None, None, None, None, None, None);
            let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
            assert_polygons_equalish(&base, &actual);
        }
    }

    #[test]
    fn features_constructor_reconstructs_rounded_stars() {
        for i in 3..=20 {
            let base = RoundedPolygon::star(
                i,
                None,
                None,
                CornerRounding::new(0.5, 0.2),
                None,
                None,
                None,
                None,
            );
            let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
            assert_polygons_equalish(&base, &actual);
        }
    }

    #[test]
    fn features_constructor_reconstructs_pill() {
        let base = RoundedPolygon::pill(None, None, None, None, None);
        let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
        assert_polygons_equalish(&base, &actual);
    }
    #[test]
    fn features_constructor_reconstructs_pill_star() {
        let base = RoundedPolygon::pill_star(
            None,
            None,
            None,
            None,
            CornerRounding::new(0.5, 0.2),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let actual = RoundedPolygonBuilder::from_features(base.features.clone()).build();
        assert_polygons_equalish(&base, &actual);
    }

    #[test]
    fn compute_center_test() {
        let polygon =
            RoundedPolygonBuilder::from_vertices(&vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0])
                .build();

        assert_floats_equalish(0.5, polygon.center_x(), 1e-4, None);
        assert_floats_equalish(0.5, polygon.center_y(), 1e-4, None);
    }

    fn points_to_floats(points: &[Point]) -> Vec<f32> {
        let mut result: Vec<f32> = Vec::with_capacity(points.len() * 2);
        for point in points {
            result.push(point.x());
            result.push(point.y());
        }
        result
    }

    #[test]
    fn rounding_space_usage_test() {
        let p0 = Point(0.0, 0.0);
        let p1 = Point(1.0, 0.0);
        let p2 = Point(0.5, 1.0);
        let pv_rounding = vec![
            CornerRounding::new(1.0, 0.0),
            CornerRounding::new(1.0, 1.0),
            CornerRounding::UNROUNDED,
        ];
        let polygon = RoundedPolygonBuilder::from_vertices(&points_to_floats(&[p0, p1, p2]))
            .per_vertex_rounding(pv_rounding)
            .build();

        // Since there is not enough room in the p0 -> p1 side even for the roundings, we shouldn't
        // take smoothing into account, so the corners should end in the middle point.
        let lower_edge_feature = polygon
            .features
            .iter()
            .find(|f| matches!(f, Feature::Edge { .. }))
            .unwrap();
        assert_eq!(1, lower_edge_feature.cubics().len());

        let lower_edge = &lower_edge_feature.cubics()[0];
        assert_eq!(0.5, lower_edge.anchor_0_x());
        assert_eq!(0.0, lower_edge.anchor_0_y());
        assert_eq!(0.5, lower_edge.anchor_1_x());
        assert_eq!(0.0, lower_edge.anchor_1_y());
    }

    /*
     * In the following tests, we check how much was cut for the top left (vertex 0) and bottom
     * left corner (vertex 3).
     * In particular, both vertex are competing for space in the left side.
     *
     *   Vertex 0            Vertex 1
     *      *---------------------*
     *      |                     |
     *      *---------------------*
     *   Vertex 3            Vertex 2
     */
    const POINTS: usize = 20;

    #[test]
    fn uneven_radius_test() {
        // Vertex 3 has the default 0.5 radius, 0 smoothing.
        // Vertex 0 has smoothing 0, and radius varying from 0 to 0.5.
        for i in 0..=POINTS {
            let smooth = i as f32 / POINTS as f32;
            do_uneven_smooth_test(
                CornerRounding::new(0.4, smooth),
                0.4 * (1.0 + smooth),
                (0.4 * (1.0 + smooth)).min(0.5),
                0.5,
                None,
            );
        }
    }

    #[test]
    fn uneven_smoothing_test2() {
        // Vertex 3 has 0.2f radius and 0.2f smoothing, so it takes at most 0.4f
        // Vertex 0 has 0.4f radius and smoothing varies from 0 to 1, when it reaches 0.5 it starts
        // competing with vertex 3 for space.
        for i in 0..=POINTS {
            let smooth = i as f32 / POINTS as f32;

            let smooth_wanted_v0 = 0.4 * smooth;
            let smooth_wanted_v3 = 0.2;

            // There is 0.4f room for smoothing
            let factor = (0.4 / (smooth_wanted_v0 + smooth_wanted_v3)).min(1.0);
            do_uneven_smooth_test(
                CornerRounding::new(0.4, smooth),
                0.4 * (1.0 + smooth),
                0.4 + factor * smooth_wanted_v0,
                0.2 + factor * smooth_wanted_v3,
                CornerRounding::new(0.2, 1.0),
            );
        }
    }

    #[test]
    fn uneven_smoothing_test3() {
        // Vertex 3 has 0.6f radius.
        // Vertex 0 has 0.4f radius and smoothing varies from 0 to 1. There is no room for smoothing
        // on the segment between these vertices, but vertex 0 can still have smoothing on the top
        // side.
        for i in 0..=POINTS {
            let smooth = i as f32 / POINTS as f32;

            do_uneven_smooth_test(
                CornerRounding::new(0.4, smooth),
                0.4 * (1.0 + smooth),
                0.4,
                0.6,
                CornerRounding::new(0.6, 0.0),
            );
        }
    }

    #[test]
    fn creating_full_size_test() {
        let radius = 400.0;
        let inner_radius_factor = 0.35;
        let inner_radius = radius * inner_radius_factor;
        let rounding_factor = 0.32;

        let full_size_shape = RoundedPolygon::star(
            4,
            radius,
            inner_radius,
            CornerRounding::new(radius * rounding_factor, None),
            CornerRounding::new(radius * rounding_factor, None),
            None,
            radius,
            radius,
        );
        let full_size_shape = full_size_shape
            .transformed(&move |x, y| Point((x - radius) / radius, (y - radius) / radius));

        let canonical_shape = RoundedPolygon::star(
            4,
            1.0,
            inner_radius_factor,
            CornerRounding::new(rounding_factor, None),
            CornerRounding::new(rounding_factor, None),
            None,
            None,
            None,
        );
        let cubics = &canonical_shape.cubics;
        let cubics1 = &full_size_shape.cubics;
        assert_eq!(cubics.len(), cubics1.len());
        for (cubic, cubic1) in cubics.iter().zip(cubics1.iter()) {
            assert_floats_equalish(cubic.anchor_0_x(), cubic1.anchor_0_x(), None, None);
            assert_floats_equalish(cubic.anchor_0_y(), cubic1.anchor_0_y(), None, None);
            assert_floats_equalish(cubic.anchor_1_x(), cubic1.anchor_1_x(), None, None);
            assert_floats_equalish(cubic.anchor_1_y(), cubic1.anchor_1_y(), None, None);
            assert_floats_equalish(cubic.control_0_x(), cubic1.control_0_x(), None, None);
            assert_floats_equalish(cubic.control_0_y(), cubic1.control_0_y(), None, None);
            assert_floats_equalish(cubic.control_1_x(), cubic1.control_1_x(), None, None);
            assert_floats_equalish(cubic.control_1_y(), cubic1.control_1_y(), None, None);
        }
    }

    fn do_uneven_smooth_test(
        rounding0: CornerRounding,
        expected_v0sx: f32,
        expected_v0sy: f32,
        expected_v3sy: f32,
        rounding3: impl Into<Option<CornerRounding>>,
    ) {
        let rounding3 = rounding3.into().unwrap_or(CornerRounding::new(0.5, 0.0));
        let p0 = Point(0.0, 0.0);
        let p1 = Point(5.0, 0.0);
        let p2 = Point(5.0, 1.0);
        let p3 = Point(0.0, 1.0);

        let pv_rounding = vec![
            rounding0,
            CornerRounding::UNROUNDED,
            CornerRounding::UNROUNDED,
            rounding3,
        ];
        let polygon = RoundedPolygon::from_vertices(
            &points_to_floats(&[p0, p1, p2, p3]),
            None,
            pv_rounding,
            None,
            None,
        );
        let edges: Vec<&Feature> = polygon
            .features
            .iter()
            .filter(|f| matches!(f, Feature::Edge { .. }))
            .collect();
        let e01 = edges[0];
        let e30 = edges[3];
        let msg = format!("r0 = {}, r3 = {}", show(&rounding0), show(&rounding3));
        assert_floats_equalish(
            expected_v0sx,
            e01.cubics()[0].anchor_0_x(),
            None,
            msg.as_str(),
        );
        assert_floats_equalish(
            expected_v0sy,
            e30.cubics()[0].anchor_1_y(),
            None,
            msg.as_str(),
        );
        assert_floats_equalish(
            expected_v3sy,
            1.0 - e30.cubics()[0].anchor_0_y(),
            None,
            msg.as_str(),
        );
    }

    fn show(cr: &CornerRounding) -> String {
        format!("(r={}, s={})", cr.radius, cr.smoothing)
    }
}

#[cfg(test)]
mod polygon_tests {
    use crate::assert_equalish;
    use crate::corner_rounding::CornerRounding;
    use crate::cubic::Cubic;
    use crate::feature::FeatureTrait;
    use crate::point::Point;
    use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
    use crate::tests::{
        assert_cubic_lists_equalish, assert_cubics_equalish, assert_in_bounds,
        assert_points_equalish, identity_transform, scale_transform, translate_transform,
    };
    use lazy_static::lazy_static;

    lazy_static! {
        static ref SQUARE: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(4).build();
        static ref ROUNDED_SQUARE: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(4)
            .rounding(CornerRounding::new(0.2, None))
            .build();
        static ref PENTAGON: RoundedPolygon = RoundedPolygonBuilder::from_num_vertices(5).build();
    }

    #[test]
    fn construction_test() {
        // We can't be too specific on how exactly the square is constructed, but
        // we can at least test whether all points are within the unit square
        let mut min = Point(-1.0, -1.0);
        let mut max = Point(1.0, 1.0);
        assert_in_bounds(&SQUARE.cubics, min, max);

        let double_square = RoundedPolygonBuilder::from_num_vertices(4)
            .radius(2.0)
            .build();
        min *= 2.0;
        max *= 2.0;
        assert_in_bounds(&double_square.cubics, min, max);

        let offset_square = RoundedPolygonBuilder::from_num_vertices(4)
            .center_x(1.0)
            .center_y(2.0)
            .build();
        min = Point(0.0, 1.0);
        max = Point(2.0, 3.0);
        assert_in_bounds(&offset_square.cubics, min, max);

        let square_copy = RoundedPolygon::from_polygon(&SQUARE);
        min = Point(-1.0, -1.0);
        max = Point(1.0, 1.0);
        assert_in_bounds(&square_copy.cubics, min, max);

        let p0 = Point(1.0, 0.0);
        let p1 = Point(0.0, 1.0);
        let p2 = Point(-1.0, 0.0);
        let p3 = Point(0.0, -1.0);
        let manual_square = RoundedPolygonBuilder::from_vertices(&vec![
            p0.x(),
            p0.y(),
            p1.x(),
            p1.y(),
            p2.x(),
            p2.y(),
            p3.x(),
            p3.y(),
        ])
        .build();
        min = Point(-1.0, -1.0);
        max = Point(1.0, 1.0);
        assert_in_bounds(&manual_square.cubics, min, max);

        let offset = Point(1.0, 2.0);
        let p0_offset = p0 + offset;
        let p1_offset = p1 + offset;
        let p2_offset = p2 + offset;
        let p3_offset = p3 + offset;
        let manual_square_offset = RoundedPolygonBuilder::from_vertices(&vec![
            p0_offset.x(),
            p0_offset.y(),
            p1_offset.x(),
            p1_offset.y(),
            p2_offset.x(),
            p2_offset.y(),
            p3_offset.x(),
            p3_offset.y(),
        ])
        .center_x(offset.x())
        .center_y(offset.y())
        .build();
        min = Point(0.0, 1.0);
        max = Point(2.0, 3.0);
        assert_in_bounds(&manual_square_offset.cubics, min, max);
    }
    #[test]
    fn bounds_test() {
        let bounds = SQUARE.calculate_bounds(None);
        assert_equalish!(-1.0, bounds[0]); // Left
        assert_equalish!(-1.0, bounds[1]); // Top
        assert_equalish!(1.0, bounds[2]); // Right
        assert_equalish!(1.0, bounds[3]); // Bottom

        let better_bounds = SQUARE.calculate_bounds(false);
        assert_equalish!(-1.0, better_bounds[0]); // Left
        assert_equalish!(-1.0, better_bounds[1]); // Top
        assert_equalish!(1.0, better_bounds[2]); // Right
        assert_equalish!(1.0, better_bounds[3]); // Bottom

        // roundedSquare's approximate bounds will be larger due to control points
        let bounds = ROUNDED_SQUARE.calculate_bounds(None);
        let better_bounds = ROUNDED_SQUARE.calculate_bounds(false);
        assert!(
            better_bounds[2] - better_bounds[0] < bounds[2] - bounds[0],
            "bounds {}, {}, {}, {}, betterBounds = {}, {}, {}, {}",
            bounds[0],
            bounds[1],
            bounds[2],
            bounds[3],
            better_bounds[0],
            better_bounds[1],
            better_bounds[2],
            better_bounds[3],
        );

        let bounds = PENTAGON.calculate_bounds(None);
        let max_bounds = PENTAGON.calculate_max_bounds();
        assert!(max_bounds[2] - max_bounds[0] > bounds[2] - bounds[0]);
    }
    #[test]
    fn center_test() {
        assert_points_equalish(Point(0.0, 0.0), Point(SQUARE.center_x(), SQUARE.center_y()));
    }
    #[test]
    fn transform_test() {
        // First, make sure the shape doesn't change when transformed by the identity
        let square_copy = SQUARE.transformed(&identity_transform());
        let n = SQUARE.cubics.len();

        assert_eq!(n, square_copy.cubics.len());
        for i in 0..n {
            assert_cubics_equalish(&SQUARE.cubics[i], &square_copy.cubics[i]);
        }

        // Now create a function which translates points by (1, 2) and make sure
        // the shape is translated similarly by it
        let offset = Point(1.0, 2.0);
        let square_cubics = &SQUARE.cubics;
        let translator = translate_transform(offset.x(), offset.y());
        let translated_square_cubics = SQUARE.transformed(&translator).cubics;

        for i in 0..square_cubics.len() {
            assert_points_equalish(
                Point(square_cubics[i].anchor_0_x(), square_cubics[i].anchor_0_y()) + offset,
                Point(
                    translated_square_cubics[i].anchor_0_x(),
                    translated_square_cubics[i].anchor_0_y(),
                ),
            );
            assert_points_equalish(
                Point(square_cubics[i].control_0_x(), square_cubics[i].control_0_y()) + offset,
                Point(
                    translated_square_cubics[i].control_0_x(),
                    translated_square_cubics[i].control_0_y(),
                ),
            );
            assert_points_equalish(
                Point(square_cubics[i].control_1_x(), square_cubics[i].control_1_y()) + offset,
                Point(
                    translated_square_cubics[i].control_1_x(),
                    translated_square_cubics[i].control_1_y(),
                ),
            );
            assert_points_equalish(
                Point(square_cubics[i].anchor_1_x(), square_cubics[i].anchor_1_y()) + offset,
                Point(
                    translated_square_cubics[i].anchor_1_x(),
                    translated_square_cubics[i].anchor_1_y(),
                ),
            );
        }
    }
    #[test]
    fn features_test() {
        let square_features = SQUARE.features.clone();

        // Verify that cubics of polygon == nonzero cubics of features of that polygon
        // Note the Equalish test since some points may be adjusted in conversion from raw
        // cubics in the feature to the cubics list for the shape
        let cubics = square_features
            .iter()
            .flat_map(|f| f.cubics().clone())
            .collect::<Vec<Cubic>>();
        {
            let nonzero_cubics = nonzero_cubics(&cubics);
            assert_cubic_lists_equalish(&SQUARE.cubics, &nonzero_cubics);
        }

        // Same as the first polygon test, but with a copy of that polygon
        let square_copy =
            RoundedPolygonBuilder::from_vertices(&vec![1.0, 0.0, 0.0, 1.0, -1.0, 0.0, 0.0, -1.0])
                .build();
        let square_copy_features = square_copy.features.clone();
        let cubics = square_copy_features
            .iter()
            .flat_map(|f| f.cubics().clone())
            .collect::<Vec<Cubic>>();
        let nonzero_cubics = nonzero_cubics(&cubics);
        assert_cubic_lists_equalish(&square_copy.cubics, &nonzero_cubics);
    }
    #[test]
    fn empty_polygon_test() {
        let poly = RoundedPolygonBuilder::from_num_vertices(6)
            .radius(0.0)
            .rounding(CornerRounding::new(0.1, None))
            .build();
        assert_eq!(1, poly.cubics.len());

        let still_empty = poly.transformed(&scale_transform(10.0, 20.0));
        assert_eq!(1, still_empty.cubics.len());
        assert!(still_empty.cubics[0].zero_length());
    }
    #[test]
    fn empty_side_test() {
        let poly1 = RoundedPolygonBuilder::from_vertices(
            &vec![0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0], // Triangle with one point repeated
        )
        .build();
        let poly2 = RoundedPolygonBuilder::from_vertices(
            &vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0], // Triangle
        )
        .build();
        assert_cubic_lists_equalish(&poly1.cubics, &poly2.cubics);
    }
    fn nonzero_cubics(original: &[Cubic]) -> Vec<Cubic> {
        let mut result: Vec<Cubic> = Vec::new();
        for cubic in original {
            if !cubic.zero_length() {
                result.push(cubic.clone());
            }
        }
        result
    }
}
