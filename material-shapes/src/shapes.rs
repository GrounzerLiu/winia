use crate::corner_rounding::CornerRounding;
use crate::point::Point;
use crate::rounded_polygon::RoundedPolygon;
use crate::utils::{interpolate, radial_to_cartesian, require, FLOAT_PI, TWO_PI};

pub struct CircleBuilder {
    num_vertices: Option<usize>,
    radius: Option<f32>,
    center_x: Option<f32>,
    center_y: Option<f32>,
}

impl CircleBuilder {
    pub fn new() -> Self {
        Self {
            num_vertices: None,
            radius: None,
            center_x: None,
            center_y: None,
        }
    }

    pub fn num_vertices(mut self, num_vertices: usize) -> Self {
        self.num_vertices = Some(num_vertices);
        self
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

    pub fn build(self) -> RoundedPolygon {
        RoundedPolygon::circle(self.num_vertices, self.radius, self.center_x, self.center_y)
    }
}

pub struct RectangleBuilder {
    width: Option<f32>,
    height: Option<f32>,
    rounding: Option<CornerRounding>,
    per_vertex_rounding: Option<Vec<CornerRounding>>,
    center_x: Option<f32>,
    center_y: Option<f32>,
}

impl RectangleBuilder {
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            rounding: None,
            per_vertex_rounding: None,
            center_x: None,
            center_y: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
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

    pub fn center_x(mut self, center_x: f32) -> Self {
        self.center_x = Some(center_x);
        self
    }

    pub fn center_y(mut self, center_y: f32) -> Self {
        self.center_y = Some(center_y);
        self
    }

    pub fn build(self) -> RoundedPolygon {
        RoundedPolygon::rectangle(
            self.width,
            self.height,
            self.rounding,
            self.per_vertex_rounding,
            self.center_x,
            self.center_y,
        )
    }
}

pub struct StarBuilder {
    num_vertices_per_radius: usize,
    radius: Option<f32>,
    inner_radius: Option<f32>,
    rounding: Option<CornerRounding>,
    inner_rounding: Option<CornerRounding>,
    per_vertex_rounding: Option<Vec<CornerRounding>>,
    center_x: Option<f32>,
    center_y: Option<f32>,
}

impl StarBuilder {
    pub fn new(num_vertices_per_radius: usize) -> Self {
        Self {
            num_vertices_per_radius,
            radius: None,
            inner_radius: None,
            rounding: None,
            inner_rounding: None,
            per_vertex_rounding: None,
            center_x: None,
            center_y: None,
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn inner_radius(mut self, inner_radius: f32) -> Self {
        self.inner_radius = Some(inner_radius);
        self
    }

    pub fn rounding(mut self, rounding: CornerRounding) -> Self {
        self.rounding = Some(rounding);
        self
    }

    pub fn inner_rounding(mut self, inner_rounding: CornerRounding) -> Self {
        self.inner_rounding = Some(inner_rounding);
        self
    }

    pub fn per_vertex_rounding(mut self, per_vertex_rounding: Vec<CornerRounding>) -> Self {
        self.per_vertex_rounding = Some(per_vertex_rounding);
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

    pub fn build(self) -> RoundedPolygon {
        RoundedPolygon::star(
            self.num_vertices_per_radius,
            self.radius,
            self.inner_radius,
            self.rounding,
            self.inner_rounding,
            self.per_vertex_rounding,
            self.center_x,
            self.center_y,
        )
    }
}

pub struct PillBuilder {
    width: Option<f32>,
    height: Option<f32>,
    smoothing: Option<f32>,
    center_x: Option<f32>,
    center_y: Option<f32>,
}

impl PillBuilder {
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            smoothing: None,
            center_x: None,
            center_y: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn smoothing(mut self, smoothing: f32) -> Self {
        self.smoothing = Some(smoothing);
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

    pub fn build(self) -> RoundedPolygon {
        RoundedPolygon::pill(
            self.width,
            self.height,
            self.smoothing,
            self.center_x,
            self.center_y,
        )
    }
}

pub struct PillStarBuilder {
    width: Option<f32>,
    height: Option<f32>,
    num_vertices_per_radius: Option<usize>,
    inner_radius_ratio: Option<f32>,
    rounding: Option<CornerRounding>,
    inner_rounding: Option<CornerRounding>,
    per_vertex_rounding: Option<Vec<CornerRounding>>,
    vertex_spacing: Option<f32>,
    start_location: Option<f32>,
    center_x: Option<f32>,
    center_y: Option<f32>,
}

impl PillStarBuilder {
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            num_vertices_per_radius: None,
            inner_radius_ratio: None,
            rounding: None,
            inner_rounding: None,
            per_vertex_rounding: None,
            vertex_spacing: None,
            start_location: None,
            center_x: None,
            center_y: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn num_vertices_per_radius(mut self, num_vertices_per_radius: usize) -> Self {
        self.num_vertices_per_radius = Some(num_vertices_per_radius);
        self
    }

    pub fn inner_radius_ratio(mut self, inner_radius_ratio: f32) -> Self {
        self.inner_radius_ratio = Some(inner_radius_ratio);
        self
    }

    pub fn rounding(mut self, rounding: CornerRounding) -> Self {
        self.rounding = Some(rounding);
        self
    }

    pub fn inner_rounding(mut self, inner_rounding: CornerRounding) -> Self {
        self.inner_rounding = Some(inner_rounding);
        self
    }

    pub fn per_vertex_rounding(mut self, per_vertex_rounding: Vec<CornerRounding>) -> Self {
        self.per_vertex_rounding = Some(per_vertex_rounding);
        self
    }

    pub fn vertex_spacing(mut self, vertex_spacing: f32) -> Self {
        self.vertex_spacing = Some(vertex_spacing);
        self
    }

    pub fn start_location(mut self, start_location: f32) -> Self {
        self.start_location = Some(start_location);
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

    pub fn build(self) -> RoundedPolygon {
        RoundedPolygon::pill_star(
            self.width,
            self.height,
            self.num_vertices_per_radius,
            self.inner_radius_ratio,
            self.rounding,
            self.inner_rounding,
            self.per_vertex_rounding,
            self.vertex_spacing,
            self.start_location,
            self.center_x,
            self.center_y,
        )
    }
}

impl RoundedPolygon {
    /// Creates a circular shape, approximating the rounding of the shape around the underlying polygon
    /// vertices.
    ///
    /// `num_vertices` — the number of vertices in the underlying polygon used to approximate the circle. Default is `8`.
    /// `radius` — optional radius for the circle. Default is `1.0`.
    /// `center_x` — X coordinate of the optional center for the circle. Default is `0.0`.
    /// `center_y` — Y coordinate of the optional center for the circle. Default is `0.0`.
    ///
    /// # Panics
    /// - If `num_vertices` is less than 3.
    pub fn circle(
        num_vertices: impl Into<Option<usize>>,
        radius: impl Into<Option<f32>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> Self {
        let num_vertices = num_vertices.into().unwrap_or(8);
        let radius = radius.into().unwrap_or(1.0);
        let center_x = center_x.into().unwrap_or(0.0);
        let center_y = center_y.into().unwrap_or(0.0);

        if num_vertices < 3 {
            panic!("Circle must have at least three vertices");
        }

        // Half of the angle between two adjacent vertices on the polygon
        let theta = FLOAT_PI / num_vertices as f32;
        // Radius of the underlying RoundedPolygon object given the desired radius of the circle
        let polygon_radius = radius / theta.cos();
        RoundedPolygon::from_num_vertices(
            num_vertices,
            polygon_radius,
            center_x,
            center_y,
            CornerRounding::new(radius, None),
            None,
        )
    }

    /// Creates a rectangular shape with the given width/height around the given center. Optional
    /// rounding parameters can be used to create a rounded rectangle instead.
    ///
    /// As with all [`RoundedPolygon`] objects, if this shape is created with default dimensions and
    /// center, it is sized to fit within the 2×2 bounding box around a center of (0, 0) and will need
    /// to be scaled and moved using [`RoundedPolygon::transformed`] to fit the intended area in a UI.
    ///
    /// `width` — the width of the rectangle. Default is `2.0`.
    /// `height` — the height of the rectangle. Default is `2.0`.
    /// `rounding` — the [`CornerRounding`] properties of every vertex. If some vertices should have
    ///     different rounding properties, use `per_vertex_rounding` instead. Default is
    ///     [`CornerRounding::UNROUNDED`].
    /// `per_vertex_rounding` — the [`CornerRounding`] properties of every vertex. If not `None`,
    ///     it must have 4 elements for the four corners. If `None`, the polygon uses `rounding` for every vertex.
    /// `center_x` — the X coordinate of the center of the rectangle. Default is `0.0`.
    /// `center_y` — the Y coordinate of the center of the rectangle. Default is `0.0`.
    pub fn rectangle(
        width: impl Into<Option<f32>>,
        height: impl Into<Option<f32>>,
        rounding: impl Into<Option<CornerRounding>>,
        per_vertex_rounding: impl Into<Option<Vec<CornerRounding>>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> Self {
        let width = width.into().unwrap_or(2.0);
        let height = height.into().unwrap_or(2.0);
        let rounding = rounding.into().unwrap_or(CornerRounding::UNROUNDED);
        let per_vertex_rounding = per_vertex_rounding.into();
        let center_x = center_x.into().unwrap_or(0.0);
        let center_y = center_y.into().unwrap_or(0.0);

        let left = center_x - width / 2.0;
        let top = center_y - height / 2.0;
        let right = center_x + width / 2.0;
        let bottom = center_y + height / 2.0;

        RoundedPolygon::from_vertices(
            &[right, bottom, left, bottom, left, top, right, top],
            rounding,
            per_vertex_rounding,
            center_x,
            center_y,
        )
    }

    /// Creates a star polygon, which is like a regular polygon except every other vertex is on either an
    /// inner or outer radius. The two radii specified must both be nonzero. If the radii are equal, the
    /// result will be a regular (not star) polygon with twice the number of vertices specified in
    /// `num_vertices_per_radius`.
    ///
    /// `num_vertices_per_radius` — the number of vertices along each of the two radii.
    /// `radius` — outer radius for this star shape, must be greater than 0. Default is `1.0`.
    /// `inner_radius` — inner radius for this star shape, must be greater than 0 and ≤ `radius`. Equal radii produce a regular polygon with 2 × `num_vertices_per_radius` vertices. Default is `0.5`.
    /// `rounding` — the [`CornerRounding`] properties of every vertex. If some vertices should have different rounding properties, use `per_vertex_rounding` instead. Default is [`CornerRounding::UNROUNDED`].
    /// `inner_rounding` — optional rounding for the vertices on the inner radius. If `None` (default), inner vertices use `rounding` or `per_vertex_rounding`.
    /// `per_vertex_rounding` — the [`CornerRounding`] properties of every vertex. If not `None`, must have 2 × `num_vertices_per_radius` elements. If `None`, uses `rounding` for every vertex.
    /// `center_x` — X coordinate of the center of the polygon. Default is `0.0`.
    /// `center_y` — Y coordinate of the center of the polygon. Default is `0.0`.
    ///
    /// # Panics
    /// - If `radius` ≤ 0, `inner_radius` ≤ 0, or `inner_radius` > `radius`.
    pub fn star(
        num_vertices_per_radius: usize,
        radius: impl Into<Option<f32>>,
        inner_radius: impl Into<Option<f32>>,
        rounding: impl Into<Option<CornerRounding>>,
        inner_rounding: impl Into<Option<CornerRounding>>,
        per_vertex_rounding: impl Into<Option<Vec<CornerRounding>>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> Self {
        let radius = radius.into().unwrap_or(1.0);
        let inner_radius = inner_radius.into().unwrap_or(0.5);
        let rounding = rounding.into().unwrap_or(CornerRounding::UNROUNDED);
        let inner_rounding = inner_rounding.into();
        let per_vertex_rounding = per_vertex_rounding.into();
        let center_x = center_x.into().unwrap_or(0.0);
        let center_y = center_y.into().unwrap_or(0.0);

        if radius <= 0.0 || inner_radius <= 0.0 {
            panic!("Star radii must both be greater than 0");
        }
        if inner_radius > radius {
            panic!("inner_radius must be less than radius");
        }

        let mut pv_rounding = per_vertex_rounding;
        // If no per-vertex rounding supplied and caller asked for inner rounding,
        // create per-vertex rounding list based on supplied outer/inner rounding parameters
        if pv_rounding.is_none()
            && let Some(inner_rounding) = inner_rounding
        {
            let mut new_pv_rounding = Vec::with_capacity(num_vertices_per_radius * 2);
            for i in 0..num_vertices_per_radius {
                new_pv_rounding.push(rounding);
                new_pv_rounding.push(inner_rounding);
            }
            pv_rounding = Some(new_pv_rounding);
        }

        // Star polygon is just a polygon with all vertices supplied (where we generate
        // those vertices to be on the inner/outer radii)
        RoundedPolygon::from_vertices(
            &star_vertices_from_num_verts(
                num_vertices_per_radius,
                radius,
                inner_radius,
                center_x,
                center_y,
            ),
            rounding,
            pv_rounding,
            center_x,
            center_y,
        )
    }

    /// A pill shape consists of a rectangle bounded by two semicircles at either of the long ends.
    ///
    /// `width` — the width of the resulting shape.
    /// `height` — the height of the resulting shape.
    /// `smoothing` — the amount by which the arc is "smoothed" by extending the curve from the
    ///     circular arc on each endcap to the edge between the endcaps. `0.0` means only a circular arc.
    /// `center_x` — X coordinate of the center of the polygon. Default is `0.0`.
    /// `center_y` — Y coordinate of the center of the polygon. Default is `0.0`.
    ///
    /// # Panics
    /// - If `width` ≤ 0 or `height` ≤ 0.
    pub fn pill(
        width: impl Into<Option<f32>>,
        height: impl Into<Option<f32>>,
        smoothing: impl Into<Option<f32>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> Self {
        let width = width.into().unwrap_or(2.0);
        let height = height.into().unwrap_or(1.0);
        let smoothing = smoothing.into().unwrap_or(0.0);
        let center_x = center_x.into().unwrap_or(0.0);
        let center_y = center_y.into().unwrap_or(0.0);
        require(
            width > 0.0 && height > 0.0,
            "Pill shapes must have positive width and height",
        );

        let w_half = width / 2.0;
        let h_half = height / 2.0;
        RoundedPolygon::from_vertices(
            &[
                w_half + center_x,
                h_half + center_y,
                -w_half + center_x,
                h_half + center_y,
                -w_half + center_x,
                -h_half + center_y,
                w_half + center_x,
                -h_half + center_y,
            ],
            CornerRounding::new(w_half.min(h_half), smoothing),
            None,
            center_x,
            center_y,
        )
    }
    /// A pillStar shape is like a [`pill`](Self::pill) except it has inner and outer radii along its pill-shaped
    /// outline, similar to how a [`star`](Self::star) has inner and outer radii along a circular outline. The parameters
    /// are similar to those of a [`star`](Self::star) but, like a [`pill`](Self::pill), it has a `width` and `height` to determine
    /// the general shape of the underlying pill.
    ///
    /// Inner and outer vertices along the curved ends may not be evenly spaced depending on the
    /// magnitudes of `rounding`, `inner_rounding`, and `inner_radius_ratio`. The default approach
    /// (`vertex_spacing = 0.5`) spaces vertices halfway between inner and outer extremes. A value of
    /// 0 aligns inner vertices along the curved ends; a value of 1 aligns outer vertices instead.
    ///
    /// `width` — the width of the resulting shape.
    /// `height` — the height of the resulting shape.
    /// `num_vertices_per_radius` — the number of vertices along each of the two radii.
    /// `inner_radius_ratio` — inner radius ratio, must be > 0 and ≤ 1. A value of 1 is equivalent to a [`pill`](Self::pill) with more vertices. Default is 0.5.
    /// `rounding` — the [`CornerRounding`] properties of every vertex. If some vertices have different rounding, use `per_vertex_rounding`. Default is [`CornerRounding::Unrounded`].
    /// `inner_rounding` — optional rounding for vertices on the inner radius. If `None`, inner vertices use `rounding` or `per_vertex_rounding`.
    /// `per_vertex_rounding` — the [`CornerRounding`] properties of every vertex. If not `None`, must have 2 × `num_vertices_per_radius` elements. If `None`, uses `rounding` for every vertex.
    /// `vertex_spacing` — factor controlling spacing of vertices on the curved ends. 0 aligns inner vertices along the straight edges; 1 aligns outer vertices. Default is 0.5.
    /// `start_location` — value from 0 to 1 indicating where to start the underlying curves. Default is 0.0.
    /// `center_x` — X coordinate of the center. Default is 0.0.
    /// `center_y` — Y coordinate of the center. Default is 0.0.
    ///
    /// # Panics
    /// - If `width` ≤ 0, `height` ≤ 0, or `inner_radius_ratio` is not in the range (0, 1].
    pub fn pill_star(
        width: impl Into<Option<f32>>,
        height: impl Into<Option<f32>>,
        num_vertices_per_radius: impl Into<Option<usize>>,
        inner_radius_ratio: impl Into<Option<f32>>,
        rounding: impl Into<Option<CornerRounding>>,
        inner_rounding: impl Into<Option<CornerRounding>>,
        per_vertex_rounding: impl Into<Option<Vec<CornerRounding>>>,
        vertex_spacing: impl Into<Option<f32>>,
        start_location: impl Into<Option<f32>>,
        center_x: impl Into<Option<f32>>,
        center_y: impl Into<Option<f32>>,
    ) -> Self {
        let width = width.into().unwrap_or(2.0);
        let height = height.into().unwrap_or(1.0);
        let num_vertices_per_radius = num_vertices_per_radius.into().unwrap_or(8);
        let inner_radius_ratio = inner_radius_ratio.into().unwrap_or(0.5);
        let rounding = rounding.into().unwrap_or(CornerRounding::UNROUNDED);
        let inner_rounding = inner_rounding.into();
        let per_vertex_rounding = per_vertex_rounding.into();
        let vertex_spacing = vertex_spacing.into().unwrap_or(0.5);
        let start_location = start_location.into().unwrap_or(0.0);
        let center_x = center_x.into().unwrap_or(0.0);
        let center_y = center_y.into().unwrap_or(0.0);

        require(
            width > 0.0 && height > 0.0,
            "Pill shapes must have positive width and height",
        );
        require(
            inner_radius_ratio > 0.0 && inner_radius_ratio <= 1.0,
            "inner_radius must be between 0 and 1",
        );

        let mut pv_rounding = per_vertex_rounding;
        // If no per-vertex rounding supplied and caller asked for inner rounding,
        // create per-vertex rounding list based on supplied outer/inner rounding parameters
        if pv_rounding.is_none()
            && let Some(inner_rounding) = inner_rounding
        {
            let mut new_pv_rounding = Vec::with_capacity(num_vertices_per_radius * 2);
            for i in 0..num_vertices_per_radius {
                new_pv_rounding.push(rounding);
                new_pv_rounding.push(inner_rounding);
            }
        }
        RoundedPolygon::from_vertices(
            &pill_star_vertices_from_num_verts(
                num_vertices_per_radius,
                width,
                height,
                inner_radius_ratio,
                vertex_spacing,
                start_location,
                center_x,
                center_y,
            ),
            rounding,
            pv_rounding,
            center_x,
            center_y,
        )
    }
}

fn pill_star_vertices_from_num_verts(
    num_vertices_per_radius: usize,
    width: f32,
    height: f32,
    inner_radius: f32,
    vertex_spacing: f32,
    start_location: f32,
    center_x: f32,
    center_y: f32,
) -> Vec<f32> {
    // The general approach here is to get the perimeter of the underlying pill outline,
    // then the t value for each vertex as we walk that perimeter. This tells us where
    // on the outline to place that vertex, then we figure out where to place the vertex
    // depending on which "section" it is in. The possible sections are the vertical edges
    // on the sides, the circular sections on all four corners, or the horizontal edges
    // on the top and bottom. Note that either the vertical or horizontal edges will be
    // of length zero (whichever dimension is smaller gets only circular curvature for the
    // pill shape).
    let endcap_radius = width.min(height);
    let v_seg_len = (height - width).clamp(0.0, f32::MAX);
    let h_seg_len = (width - height).clamp(0.0, f32::MAX);
    let v_seg_half = v_seg_len / 2.0;
    let h_seg_half = h_seg_len / 2.0;
    // vertexSpacing is used to position the vertices on the end caps. The caller has the choice
    // of spacing the inner (0) or outer (1) vertices like those along the edges, causing the
    // other vertices to be either further apart (0) or closer (1). The default is .5, which
    // averages things. The magnitude of the inner and rounding parameters may cause the caller
    // to want a different value.
    let circle_perimeter = TWO_PI * endcap_radius * interpolate(inner_radius, 1.0, vertex_spacing);
    // perimeter is circle perimeter plus horizontal and vertical sections of inner rectangle,
    // whether either (or even both) might be of length zero.
    let perimeter = 2.0 * h_seg_len + 2.0 * v_seg_len + circle_perimeter;

    // The sections array holds the t start values of that part of the outline. We use these to
    // determine which section a given vertex lies in, based on its t value, as well as where
    // in that section it lies.
    let mut sections = [0.0_f32; 11];
    sections[0] = 0.0;
    sections[1] = v_seg_len / 2.0;
    sections[2] = sections[1] + circle_perimeter / 4.0;
    sections[3] = sections[2] + h_seg_len;
    sections[4] = sections[3] + circle_perimeter / 4.0;
    sections[5] = sections[4] + v_seg_len;
    sections[6] = sections[5] + circle_perimeter / 4.0;
    sections[7] = sections[6] + h_seg_len;
    sections[8] = sections[7] + circle_perimeter / 4.0;
    sections[9] = sections[8] + v_seg_len / 2.0;
    sections[10] = perimeter;

    // "t" is the length along the entire pill outline for a given vertex. With vertices spaced
    // evenly along this contour, we can determine for any vertex where it should lie.
    let t_per_vertex = perimeter / (2.0 * num_vertices_per_radius as f32);
    // separate iteration for inner vs outer, unlike the other shapes, because
    // the vertices can lie in different quadrants so each needs their own calculation
    let mut inner = false;
    // Increment section index as we walk around the pill contour with our increasing t values
    let mut curr_sec_index = 0_usize;
    // secStart/End are used to determine how far along a given vertex is in the section
    // in which it lands
    let mut sec_start = 0.0;
    let mut sec_end = sections[1];
    // t value is used to place each vertex. 0 is on the positive x axis,
    // moving into section 0 to begin with. startLocation, a value from 0 to 1, varies the location
    // anywhere on the perimeter of the shape
    let mut t = start_location * perimeter;
    // The list of vertices to be returned
    let mut result = vec![0.0_f32; num_vertices_per_radius * 4];
    let mut array_index = 0;
    let rect_br = Point(h_seg_half, v_seg_half);
    let rect_bl = Point(-h_seg_half, v_seg_half);
    let rect_tl = Point(-h_seg_half, -v_seg_half);
    let rect_tr = Point(h_seg_half, -v_seg_half);
    // Each iteration through this loop uses the next t value as we walk around the shape
    for i in 0..num_vertices_per_radius * 2 {
        // t could start (and end) after 0; extra boundedT logic makes sure it does the right
        // thing when crossing the boundar past 0 again
        let bounded_t = t % perimeter;
        if bounded_t < sec_start {
            curr_sec_index = 0;
        }
        while bounded_t >= sections[(curr_sec_index + 1) % sections.len()] {
            curr_sec_index = (curr_sec_index + 1) % sections.len();
            sec_start = sections[curr_sec_index];
            sec_end = sections[(curr_sec_index + 1) % sections.len()];
        }
        // find t in section and its proportion of that section's total length
        let t_in_section = bounded_t - sec_start;
        let t_proportion = t_in_section / (sec_end - sec_start);

        // The vertex placement in a section varies depending on whether it is on one of the
        // semicircle endcaps or along one of the straight edges. For the endcaps, we use
        // tProportion to get the angle along that circular cap and add
        // the starting angle for that section. For the edges we use a straight linear calculation
        // given tProportion and the start/end t values for that edge.
        let curr_radius = if inner {
            endcap_radius * inner_radius
        } else {
            endcap_radius
        };
        let vertex = match curr_sec_index {
            0 => Point(curr_radius, t_proportion * v_seg_half),
            1 => radial_to_cartesian(curr_radius, t_proportion * FLOAT_PI / 2.0, None) + rect_br,
            2 => Point(h_seg_half - t_proportion * h_seg_len, curr_radius),
            3 => {
                radial_to_cartesian(
                    curr_radius,
                    FLOAT_PI / 2.0 + (t_proportion * FLOAT_PI / 2.0),
                    None,
                ) + rect_bl
            }
            4 => Point(-curr_radius, v_seg_half - t_proportion * v_seg_len),
            5 => {
                radial_to_cartesian(
                    curr_radius,
                    FLOAT_PI + (t_proportion * FLOAT_PI / 2.0),
                    None,
                ) + rect_tl
            }
            6 => Point(-h_seg_half + t_proportion * h_seg_len, -curr_radius),
            7 => {
                radial_to_cartesian(
                    curr_radius,
                    FLOAT_PI * 1.5 + (t_proportion * FLOAT_PI / 2.0),
                    None,
                ) + rect_tr
            }
            _ => Point(curr_radius, -v_seg_half + t_proportion * v_seg_half),
        };
        result[array_index] = vertex.x() + center_x;
        array_index += 1;
        result[array_index] = vertex.y() + center_y;
        array_index += 1;
        t += t_per_vertex;
        inner = !inner;
    }
    result
}

fn star_vertices_from_num_verts(
    num_vertices_per_radius: usize,
    radius: f32,
    inner_radius: f32,
    center_x: f32,
    center_y: f32,
) -> Vec<f32> {
    let mut result = vec![0.0_f32; num_vertices_per_radius * 4];
    let mut array_index = 0;
    for i in 0..num_vertices_per_radius {
        let mut vertex = radial_to_cartesian(
            radius,
            FLOAT_PI / num_vertices_per_radius as f32 * 2.0 * i as f32,
            None,
        );
        result[array_index] = vertex.x() + center_x;
        array_index += 1;
        result[array_index] = vertex.y() + center_y;
        array_index += 1;
        vertex = radial_to_cartesian(
            inner_radius,
            FLOAT_PI / num_vertices_per_radius as f32 * (2.0 * i as f32 + 1.0),
            None,
        );
        result[array_index] = vertex.x() + center_x;
        array_index += 1;
        result[array_index] = vertex.y() + center_y;
        array_index += 1;
    }
    result
}

#[cfg(test)]
mod shapes_tests {
    use crate::corner_rounding::CornerRounding;
    use crate::cubic::Cubic;
    use crate::point::Point;
    use crate::shapes::{CircleBuilder, StarBuilder};
    use crate::tests::{assert_floats_equalish, assert_in_bounds};
    use crate::{assert_equalish, assert_panic};

    const ZERO: Point = Point(0.0, 0.0);
    const EPSILON: f32 = 0.01;

    fn distance(start: Point, end: Point) -> f32 {
        let vector = end - start;
        (vector.x() * vector.x() + vector.y() * vector.y()).sqrt()
    }

    /**
     * Test that the given point is radius distance away from [center]. If two radii are provided it
     * is sufficient to lie on either one (used for testing points on stars).
     */
    fn assert_point_on_raddi(
        point: Point,
        radius1: f32,
        radius2: impl Into<Option<f32>>,
        center: impl Into<Option<Point>>,
    ) {
        let radius2 = radius2.into().unwrap_or(radius1);
        let center = center.into().unwrap_or(ZERO);
        let dist = distance(center, point);
        let result1 = std::panic::catch_unwind(|| {
            assert_equalish!(dist, radius1, EPSILON);
        });
        let result2 = std::panic::catch_unwind(|| {
            assert_equalish!(dist, radius2, EPSILON);
        });
        assert!(
            result1.is_ok() || result2.is_ok(),
            "Point {:?} not on either radius {} or {}",
            point,
            radius1,
            radius2
        );
    }

    fn assert_cubic_on_raddi(
        cubic: &Cubic,
        radius1: f32,
        radius2: impl Into<Option<f32>>,
        center: impl Into<Option<Point>>,
    ) {
        let radius2 = radius2.into().unwrap_or(radius1);
        let center = center.into().unwrap_or(ZERO);
        assert_point_on_raddi(
            Point(cubic.anchor_0_x(), cubic.anchor_0_y()),
            radius1,
            radius2,
            center,
        );
        assert_point_on_raddi(
            Point(cubic.anchor_1_x(), cubic.anchor_1_y()),
            radius1,
            radius2,
            center,
        );
    }

    /**
     * Tests points along the curve of the cubic by comparing the distance from that point to the
     * center, compared to the requested radius. The test is very lenient since the Circle shape is
     * only a 4x cubic approximation of the circle and varies from the true circle.
     */
    fn assert_circular_cubic(cubic: &Cubic, radius: f32, center: Point) {
        let mut t = 0.0_f32;
        while t <= 1.0 {
            let point_on_curve = cubic.point_on_curve(t);
            let distance_to_point = distance(center, point_on_curve);
            assert_floats_equalish(distance_to_point, radius, EPSILON, None);
            t += 0.1;
        }
    }

    fn assert_circle_shape(
        shape: &Vec<Cubic>,
        radius: impl Into<Option<f32>>,
        center: impl Into<Option<Point>>,
    ) {
        let radius = radius.into().unwrap_or(1.0);
        let center = center.into().unwrap_or(ZERO);
        for cubic in shape.iter() {
            assert_circular_cubic(cubic, radius, center);
        }
    }

    macro_rules! assert_circle_shape {
        ($shape:expr, $radius:expr, $center:expr) => {
            assert_circle_shape($shape, $radius, $center);
        };
        ($shape:expr, $radius:expr) => {
            assert_circle_shape($shape, $radius, None);
        };
        ($shape:expr) => {
            assert_circle_shape($shape, None, None);
        };
    }

    #[test]
    fn circle_test() {
        assert_panic!({
            CircleBuilder::new().num_vertices(2).build();
        });

        let circle = CircleBuilder::new().build();
        assert_circle_shape!(&circle.cubics);

        let simple_circle = CircleBuilder::new().num_vertices(3).build();
        assert_circle_shape!(&simple_circle.cubics);

        let complex_circle = CircleBuilder::new().num_vertices(20).build();
        assert_circle_shape!(&complex_circle.cubics);

        let big_circle = CircleBuilder::new().radius(3.0).build();
        assert_circle_shape!(&big_circle.cubics, 3.0);

        let center = Point(1.0, 2.0);
        let offset_circle = CircleBuilder::new()
            .center_x(center.x())
            .center_y(center.y())
            .build();
        assert_circle_shape!(&offset_circle.cubics, None, center);
    }

    /**
     * Stars are complicated. For the unrounded version, we can check whether the vertices are the
     * right distance from the center. For the rounded versions, just check that the shape is within
     * the appropriate bounds.
     */
    #[test]
    fn star_test() {
        let mut star = StarBuilder::new(4).inner_radius(0.5).build();
        let mut shape = &star.cubics;
        let mut radius = 1.0;
        let mut inner_radius = 0.5;
        for cubic in shape.iter() {
            assert_cubic_on_raddi(cubic, radius, inner_radius, None);
        }

        let center = Point(1.0, 2.0);
        star = StarBuilder::new(4)
            .inner_radius(0.5)
            .center_x(center.x())
            .center_y(center.y())
            .build();
        shape = &star.cubics;
        for cubic in shape.iter() {
            assert_cubic_on_raddi(cubic, radius, inner_radius, center);
        }

        radius = 4.0;
        inner_radius = 2.0;
        star = StarBuilder::new(4)
            .radius(radius)
            .inner_radius(inner_radius)
            .build();
        shape = &star.cubics;
        for cubic in shape.iter() {
            assert_cubic_on_raddi(cubic, radius, inner_radius, None);
        }
    }

    #[test]
    fn rounded_star_test() {
        let rounding = CornerRounding::new(0.1, None);
        let inner_rounding = CornerRounding::new(0.2, None);
        let per_vtx_rounded = vec![
            rounding,
            inner_rounding,
            rounding,
            inner_rounding,
            rounding,
            inner_rounding,
            rounding,
            inner_rounding,
        ];

        // let mut star = RoundedPolygon::star(4, None, 0.5, rounding, None, None, None, None);
        let mut star = StarBuilder::new(4)
            .inner_radius(0.5)
            .rounding(rounding)
            .build();
        let min = Point(-1.0, -1.0);
        let max = Point(1.0, 1.0);
        assert_in_bounds(&star.cubics, min, max);

        star = StarBuilder::new(4)
            .inner_radius(0.5)
            .inner_rounding(inner_rounding)
            .build();
        assert_in_bounds(&star.cubics, min, max);

        star = StarBuilder::new(4)
            .inner_radius(0.5)
            .rounding(rounding)
            .inner_rounding(inner_rounding)
            .build();
        assert_in_bounds(&star.cubics, min, max);

        star = StarBuilder::new(4)
            .inner_radius(0.5)
            .per_vertex_rounding(per_vtx_rounded.clone())
            .build();
        assert_in_bounds(&star.cubics, min, max);

        assert_panic!({
            let per_vtx_rounded = vec![
                rounding,
                inner_rounding,
                rounding,
                inner_rounding,
                rounding,
                inner_rounding,
                rounding,
                inner_rounding,
            ];
            let star = StarBuilder::new(6)
                .inner_radius(0.5)
                .per_vertex_rounding(per_vtx_rounded)
                .build();
        });
    }
}
