#![allow(dead_code)]
use std::f32::consts::PI;
use std::sync::OnceLock;
use lazy_static::lazy_static;
use crate::corner_rounding::CornerRounding;
use crate::matrix::Matrix;
use crate::offset::Offset;
use crate::rounded_polygon::{RoundedPolygon, RoundedPolygonBuilder};
use crate::shapes::{CircleBuilder, RectangleBuilder, StarBuilder};

lazy_static!(
    static ref ROTATE_NEG_45: Matrix = {
        let mut matrix = Matrix::default();
        matrix.rotate_z(-45.0);
        matrix
    };
    static ref ROTATE_NEG_90: Matrix = {
        let mut matrix = Matrix::default();
        matrix.rotate_z(-90.0);
        matrix
    };
    static ref ROTATE_NEG_135: Matrix = {
        let mut matrix = Matrix::default();
        matrix.rotate_z(-135.0);
        matrix
    };
);

// static CIRCLE: OnceLock<RoundedPolygon> = OnceLock::new();
// static SQUARE: OnceLock<RoundedPolygon> = OnceLock::new();
// static SLANTED: OnceLock<RoundedPolygon> = OnceLock::new();
macro_rules! define_shape {
    ($name:ident, $fn_name:ident, $shape_expr:expr) => {
        static $name: OnceLock<RoundedPolygon> = OnceLock::new();
        impl MaterialShapes {
            pub fn $fn_name() -> &'static RoundedPolygon {
                $name.get_or_init(|| $shape_expr)
            }
        }
    };
}

define_shape!(CIRCLE, circle, circle(None));
define_shape!(SQUARE, square, square());
define_shape!(SLANTED, slanted, slanted());
define_shape!(ARCH, arch, arch());
define_shape!(FAN, fan, fan());
define_shape!(ARROW, arrow, arrow());
define_shape!(SEMI_CIRCLE, semi_circle, semi_circle());
define_shape!(OVAL, oval, oval());
define_shape!(PILL, pill, pill());
define_shape!(TRIANGLE, triangle, triangle());
define_shape!(DIAMOND, diamond, diamond());
define_shape!(CLAM_SHELL, clam_shell, clam_shell());
define_shape!(PENTAGON, pentagon, pentagon());
define_shape!(GEM, gem, gem());
define_shape!(SUNNY, sunny, sunny());
define_shape!(VERY_SUNNY, very_sunny, very_sunny());
define_shape!(COOKIE_4_SIDED, cookie_4_sided, cookie_4());
define_shape!(COOKIE_6_SIDED, cookie_6_sided, cookie_6());
define_shape!(COOKIE_7_SIDED, cookie_7_sided, cookie_7());
define_shape!(COOKIE_9_SIDED, cookie_9_sided, cookie_9());
define_shape!(COOKIE_12_SIDED, cookie_12_sided, cookie_12());
define_shape!(GHOSTISH, ghostish, ghostish());
define_shape!(CLOVER4, clover_4_leaf, clover_4());
define_shape!(CLOVER8, clover_8_leaf, clover_8());
define_shape!(BURST, burst, burst());
define_shape!(SOFT_BURST, soft_burst, soft_burst());
define_shape!(BOOM, boom, boom());
define_shape!(SOFT_BOOM, soft_boom, soft_boom());
define_shape!(FLOWER, flower, flower());
define_shape!(PUFFY, puffy, puffy());
define_shape!(PUFFY_DIAMOND, puffy_diamond, puffy_diamond());
define_shape!(PIXEL_CIRCLE, pixel_circle, pixel_circle());
define_shape!(PIXEL_TRIANGLE, pixel_triangle, pixel_triangle());
define_shape!(BUN, bun, bun());
define_shape!(HEART, heart, heart());

fn circle(num_vertices: impl Into<Option<usize>>) -> RoundedPolygon {
    let num_vertices = num_vertices.into().unwrap_or(10);
    CircleBuilder::new().num_vertices(num_vertices).build()
}
fn square() -> RoundedPolygon {
    RectangleBuilder::new().width(1.0).height(1.0).rounding(MaterialShapes::CORNER_ROUND_30).build()
}
fn slanted() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.926, 0.970), CornerRounding::new(0.189, 0.811)),
            PointNRound::new(Offset::new(-0.021, 0.967), CornerRounding::new(0.187, 0.057))
        ],
        2,
        None,
        None
    )
}
fn arch() -> RoundedPolygon {
    RoundedPolygonBuilder::from_num_vertices(4)
        .per_vertex_rounding(
            vec![
                MaterialShapes::CORNER_ROUND_100,
                MaterialShapes::CORNER_ROUND_100,
                MaterialShapes::CORNER_ROUND_20,
                MaterialShapes::CORNER_ROUND_20,
            ]
        )
        .build()
        .transformed_by_matrix(*ROTATE_NEG_135)
}
fn fan() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(1.004, 1.000), CornerRounding::new(0.148, 0.417)),
            PointNRound::new(Offset::new(0.000, 1.000), CornerRounding::new(0.151, None)),
            PointNRound::new(Offset::new(0.000, -0.003), CornerRounding::new(0.148, None)),
            PointNRound::new(Offset::new(0.978, 0.020), CornerRounding::new(0.803, None))
        ],
        1,
        None,
        None,
    )
}
fn arrow() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.892), CornerRounding::new(0.313, None)),
            PointNRound::new(Offset::new(-0.216, 1.050), CornerRounding::new(0.207, None)),
            PointNRound::new(Offset::new(0.499, -0.160), CornerRounding::new(0.215, 1.000)),
            PointNRound::new(Offset::new(1.225, 1.060), CornerRounding::new(0.211, None)),
        ],
        1,
        None,
        None,
    )
}
fn semi_circle() -> RoundedPolygon {
    RectangleBuilder::new()
        .width(1.6)
        .height(1.0)
        .per_vertex_rounding(
            vec![
                MaterialShapes::CORNER_ROUND_20,
                MaterialShapes::CORNER_ROUND_20,
                MaterialShapes::CORNER_ROUND_100,
                MaterialShapes::CORNER_ROUND_100,
            ]
        )
        .build()
}
fn oval() -> RoundedPolygon {
    let mut m = Matrix::default();
    m.scale(1.0, 0.64, 1.0);
    CircleBuilder::new()
        .build()
        .transformed_by_matrix(m)
        .transformed_by_matrix(*ROTATE_NEG_45)
}
fn pill() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.961, 0.039), CornerRounding::new(0.426, None)),
            PointNRound::new(Offset::new(1.001, 0.428), None),
            PointNRound::new(Offset::new(1.000, 0.609), CornerRounding::new(1.000, None)),
        ],
        2,
        None,
        true,
    )
}
fn triangle() -> RoundedPolygon {
    RoundedPolygonBuilder::from_num_vertices(3)
        .rounding(MaterialShapes::CORNER_ROUND_20)
        .build()
        .transformed_by_matrix(*ROTATE_NEG_90)
}
fn diamond() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 1.096), CornerRounding::new(0.151, 0.524)),
            PointNRound::new(Offset::new(0.040, 0.500), CornerRounding::new(0.159, None)),
        ],
        2,
        None,
        None
    )
}
fn clam_shell() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.171, 0.841), CornerRounding::new(0.159, None)),
            PointNRound::new(Offset::new(-0.020, 0.500), CornerRounding::new(0.140, None)),
            PointNRound::new(Offset::new(0.170, 0.159), CornerRounding::new(0.159, None)),
        ],
        2,
        None,
        None,
    )
}
fn pentagon() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, -0.009), CornerRounding::new(0.172, None)),
            PointNRound::new(Offset::new(1.030, 0.365), CornerRounding::new(0.164, None)),
            PointNRound::new(Offset::new(0.828, 0.970), CornerRounding::new(0.169, None)),
        ],
        1,
        None,
        true,
    )
}
fn gem() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.499, 1.023), CornerRounding::new(0.241, 0.778)),
            PointNRound::new(Offset::new(-0.005, 0.792), CornerRounding::new(0.208, None)),
            PointNRound::new(Offset::new(0.073, 0.258), CornerRounding::new(0.228, None)),
            PointNRound::new(Offset::new(0.433, -0.000), CornerRounding::new(0.491, None)),
        ],
        1,
        None,
        true,
    )
}
fn sunny() -> RoundedPolygon {
    StarBuilder::new(8)
        .inner_radius(0.8)
        .rounding(MaterialShapes::CORNER_ROUND_15)
        .build()
}
fn very_sunny() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 1.080), CornerRounding::new(0.085, None)),
            PointNRound::new(Offset::new(0.358, 0.843), CornerRounding::new(0.085, None)),
        ],
        8,
        None,
        false,
    )
}
fn cookie_4() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(1.237, 1.236), CornerRounding::new(0.258, None)),
            PointNRound::new(Offset::new(0.500, 0.918), CornerRounding::new(0.233, None)),
        ],
        4,
        None,
        None,
    )
}
fn cookie_6() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.723, 0.884), CornerRounding::new(0.394, None)),
            PointNRound::new(Offset::new(0.500, 1.099), CornerRounding::new(0.398, None)),
        ],
        6,
        None,
        None,
    )
}
fn cookie_7() -> RoundedPolygon {
    StarBuilder::new(7)
        .inner_radius(0.75)
        .rounding(MaterialShapes::CORNER_ROUND_50)
        .build()
        .transformed_by_matrix(*ROTATE_NEG_90)
}
fn cookie_9() -> RoundedPolygon {
    StarBuilder::new(9)
        .inner_radius(0.8)
        .rounding(MaterialShapes::CORNER_ROUND_50)
        .build()
        .transformed_by_matrix(*ROTATE_NEG_90)
}
fn cookie_12() -> RoundedPolygon {
    StarBuilder::new(12)
        .inner_radius(0.8)
        .rounding(MaterialShapes::CORNER_ROUND_50)
        .build()
        .transformed_by_matrix(*ROTATE_NEG_90)
}
fn ghostish() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.000), CornerRounding::new(1.000, None)),
            PointNRound::new(Offset::new(1.000, 0.000), CornerRounding::new(1.000, None)),
            PointNRound::new(Offset::new(1.000, 1.140), CornerRounding::new(0.254, 0.106)),
            PointNRound::new(Offset::new(0.575, 0.906), CornerRounding::new(0.253, None)),
        ],
        1,
        None,
        true,
    )
}
fn clover_4() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.074), None),
            PointNRound::new(Offset::new(0.725, -0.099), CornerRounding::new(0.476, None)),
        ],
        4,
        None,
        true,
    )
}
fn clover_8() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.036), None),
            PointNRound::new(Offset::new(0.758, -0.101), CornerRounding::new(0.209, None)),
        ],
        8,
        None,
        None,
    )
}
fn burst() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, -0.006), CornerRounding::new(0.006, None)),
            PointNRound::new(Offset::new(0.592, 0.158), CornerRounding::new(0.006, None)),
        ],
        12,
        None,
        None,
    )
}
fn soft_burst() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.193, 0.277), CornerRounding::new(0.053, None)),
            PointNRound::new(Offset::new(0.176, 0.055), CornerRounding::new(0.053, None)),
        ],
        10,
        None,
        None,
    )
}
fn boom() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.296), CornerRounding::new(0.007, None)),
            PointNRound::new(Offset::new(0.543, -0.051), CornerRounding::new(0.007, None)),
        ],
        15,
        None,
        None,
    )
}
fn soft_boom() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.733, 0.454), None),
            PointNRound::new(Offset::new(0.839, 0.437), CornerRounding::new(0.532, None)),
            PointNRound::new(Offset::new(0.949, 0.449), CornerRounding::new(0.439, 1.000)),
            PointNRound::new(Offset::new(0.998, 0.478), CornerRounding::new(0.174, None)),
        ],
        16,
        None,
        true,
    )
}
fn flower() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.370, 0.187), None),
            PointNRound::new(Offset::new(0.416, 0.049), CornerRounding::new(0.381, None)),
            PointNRound::new(Offset::new(0.479, 0.001), CornerRounding::new(0.095, None)),
        ],
        8,
        None,
        true,
    )
}
fn puffy() -> RoundedPolygon {
    let mut m = Matrix::default();
    m.scale(1.0, 0.742, 1.0);
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.053), None),
            PointNRound::new(Offset::new(0.545, -0.040), CornerRounding::new(0.405, None)),
            PointNRound::new(Offset::new(0.670, -0.035), CornerRounding::new(0.426, None)),
            PointNRound::new(Offset::new(0.717, 0.066), CornerRounding::new(0.574, None)),
            PointNRound::new(Offset::new(0.722, 0.128), None),
            PointNRound::new(Offset::new(0.777, 0.002), CornerRounding::new(0.360, None)),
            PointNRound::new(Offset::new(0.914, 0.149), CornerRounding::new(0.660, None)),
            PointNRound::new(Offset::new(0.926, 0.289), CornerRounding::new(0.660, None)),
            PointNRound::new(Offset::new(0.881, 0.346), None),
            PointNRound::new(Offset::new(0.940, 0.344), CornerRounding::new(0.126, None)),
            PointNRound::new(Offset::new(1.003, 0.437), CornerRounding::new(0.255, None)),
        ],
        2,
        None,
        true,
    )
    .transformed_by_matrix(m)
}
fn puffy_diamond() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.870, 0.130), CornerRounding::new(0.146, None)),
            PointNRound::new(Offset::new(0.818, 0.357), None),
            PointNRound::new(Offset::new(1.000, 0.332), CornerRounding::new(0.853, None)),
        ],
        4,
        None,
        true,
    )
}
fn pixel_circle() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.000), None),
            PointNRound::new(Offset::new(0.704, 0.000), None),
            PointNRound::new(Offset::new(0.704, 0.065), None),
            PointNRound::new(Offset::new(0.843, 0.065), None),
            PointNRound::new(Offset::new(0.843, 0.148), None),
            PointNRound::new(Offset::new(0.926, 0.148), None),
            PointNRound::new(Offset::new(0.926, 0.296), None),
            PointNRound::new(Offset::new(1.000, 0.296), None),
        ],
        2,
        None,
        true,
    )
}
fn pixel_triangle() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.110, 0.500), None),
            PointNRound::new(Offset::new(0.113, 0.000), None),
            PointNRound::new(Offset::new(0.287, 0.000), None),
            PointNRound::new(Offset::new(0.287, 0.087), None),
            PointNRound::new(Offset::new(0.421, 0.087), None),
            PointNRound::new(Offset::new(0.421, 0.170), None),
            PointNRound::new(Offset::new(0.560, 0.170), None),
            PointNRound::new(Offset::new(0.560, 0.265), None),
            PointNRound::new(Offset::new(0.674, 0.265), None),
            PointNRound::new(Offset::new(0.675, 0.344), None),
            PointNRound::new(Offset::new(0.789, 0.344), None),
            PointNRound::new(Offset::new(0.789, 0.439), None),
            PointNRound::new(Offset::new(0.888, 0.439), None),
        ],
        1,
        None,
        true,
    )
}
fn bun() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.796, 0.500), None),
            PointNRound::new(Offset::new(0.853, 0.518), CornerRounding::new(1.0, None)),
            PointNRound::new(Offset::new(0.992, 0.631), CornerRounding::new(1.0, None)),
            PointNRound::new(Offset::new(0.968, 1.000), CornerRounding::new(1.0, None)),
        ],
        2,
        None,
        true,
    )
}
fn heart() -> RoundedPolygon {
    custom_polygon(
        &vec![
            PointNRound::new(Offset::new(0.500, 0.268), CornerRounding::new(0.016, None)),
            PointNRound::new(Offset::new(0.792, -0.066), CornerRounding::new(0.958, None)),
            PointNRound::new(Offset::new(1.064, 0.276), CornerRounding::new(1.000, None)),
            PointNRound::new(Offset::new(0.501, 0.946), CornerRounding::new(0.129, None)),
        ],
        1,
        None,
        true,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct PointNRound{
    pub o: Offset,
    pub r: CornerRounding,
}
impl PointNRound {
    pub fn new(o: Offset, r: impl Into<Option<CornerRounding>>) -> Self {
        Self {
            o,
            r: r.into().unwrap_or(CornerRounding::UNROUNDED),
        }
    }
}

fn do_repeat(
    points: &[PointNRound],
    reps: usize,
    center: Offset,
    mirroring: bool,
) -> Vec<PointNRound> {
    if mirroring {
        let mut result = vec![];
        let angles = points.iter().map(|point| (point.o - center).angle_degrees() ).collect::<Vec<f32>>();
        let distances = points.iter().map(|point| (point.o - center).get_distance() ).collect::<Vec<f32>>();
        let actual_reps = reps * 2;
        let section_angle = 360.0 / (actual_reps as f32);
        for it in 0..actual_reps {
            for index in 0..points.len() {
                let i = if it % 2 == 0 {
                    index
                } else {
                    points.len() - 1 - index
                };
                if i > 0 || it % 2 == 0 {
                    let a = (section_angle * it as f32 +
                        if it % 2 == 0 {
                            angles[i]
                        } else {
                            section_angle - angles[i] + 2.0 * angles[0]
                        }
                    ).to_radians();
                    let final_point = Offset::new(
                        a.cos(),
                        a.sin(),
                    ) * distances[i] + center;
                    result.push(PointNRound {
                        o: final_point,
                        r: points[i].r,
                    })
                }
            }
        }
        result
    } else {
        let np = points.len();
        (0..(np * reps)).map(|it| {
            let point = points[it % np].o.rotate_degrees((it /np) as f32 * 360.0 / reps as f32, center);
            PointNRound::new(point, points[it % np].r)
        }).collect()
    }
}

fn custom_polygon(
    pnr: &[PointNRound],
    reps: usize,
    center: impl Into<Option<Offset>>,
    mirroring: impl Into<Option<bool>>,
) -> RoundedPolygon {
    let center = center.into().unwrap_or(Offset::new(0.5, 0.5));
    let actual_points = do_repeat(
        pnr,
        reps,
        center,
        mirroring.into().unwrap_or(false),
    );
    let vertices = (0..actual_points.len() * 2).map(
        |ix| {
            let o = actual_points[ix / 2].o;
            if ix % 2 == 0 {
                o.x()
            } else {
                o.y()
            }
        }
    ).collect::<Vec<f32>>();
    let per_vertex_rounding = actual_points.iter().map(|p| p.r).collect::<Vec<CornerRounding>>();
    RoundedPolygonBuilder::from_vertices(&vertices)
        .per_vertex_rounding(per_vertex_rounding)
        .center_x(center.x())
        .center_y(center.y())
        .build()
}


pub struct MaterialShapes {}

impl MaterialShapes {
    const CORNER_ROUND_15: CornerRounding = CornerRounding {
        radius: 0.15,
        smoothing: 0.0,
    };
    const CORNER_ROUND_20: CornerRounding = CornerRounding {
        radius: 0.20,
        smoothing: 0.0,
    };
    const CORNER_ROUND_30: CornerRounding = CornerRounding {
        radius: 0.30,
        smoothing: 0.0,
    };
    const CORNER_ROUND_50: CornerRounding = CornerRounding {
        radius: 0.50,
        smoothing: 0.0,
    };
    const CORNER_ROUND_100: CornerRounding = CornerRounding {
        radius: 1.00,
        smoothing: 0.0,
    };
}


trait RotateDegrees {
    fn rotate_degrees(&self, angle: f32, center: impl Into<Option<Offset>>) -> Offset;
}
impl RotateDegrees for Offset {
    fn rotate_degrees(&self, angle: f32, center: impl Into<Option<Offset>>) -> Offset {
        let center = center.into().unwrap_or(Offset::new(0.0, 0.0));
        let a = angle.to_radians();
        let off = *self - center;
        Offset::new(
            off.x() * a.cos() - off.y() * a.sin(),
            off.x() * a.sin() + off.y() * a.cos(),
        ) + center
    }
}

trait ToRadians {
    fn to_radians(&self) -> f32;
}
impl ToRadians for f32 {
    fn to_radians(&self) -> f32 {
        self / 360.0 * 2.0 * PI
    }
}
trait AngleDegrees {
    fn angle_degrees(&self) -> f32;
}
impl AngleDegrees for Offset {
    fn angle_degrees(&self) -> f32 {
        self.y().atan2(self.x()) * 180.0 / PI
    }
}

trait Transformed {
    fn transformed_by_matrix(&self, matrix: Matrix) -> RoundedPolygon;
}

impl Transformed for RoundedPolygon {
    fn transformed_by_matrix(&self, matrix: Matrix) -> RoundedPolygon {
        self.transformed(&move |x, y| {
            matrix.map_offset(Offset::new(x, y))
        })
    }
}

