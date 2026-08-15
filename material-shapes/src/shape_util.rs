use crate::cubic::Cubic;
use crate::morph::Morph;
use crate::rounded_polygon::RoundedPolygon;
use skia_safe::{Matrix, Path, PathBuilder};
use std::f32::consts::PI;

pub trait PolygonToPath {
    fn to_path(
        &self,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
    ) -> Path;
    fn apply_to_path_builder(
        &self,
        path: &mut PathBuilder,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
    );
}

impl PolygonToPath for RoundedPolygon {
    fn to_path(
        &self,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
    ) -> Path {
        let mut path = PathBuilder::new();
        self.apply_to_path_builder(&mut path, start_angle, repeat_path, close_path);
        path.detach()
    }

    fn apply_to_path_builder(
        &self,
        path: &mut PathBuilder,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
    ) {
        let start_angle = start_angle.into().unwrap_or(270);
        let repeat_path = repeat_path.into().unwrap_or(false);
        let close_path = close_path.into().unwrap_or(true);
        path_from_cubics(
            path,
            start_angle,
            repeat_path,
            close_path,
            &self.cubics,
            self.center.x(),
            self.center.y(),
        )
    }
}

pub trait MorphToPath {
    fn to_path(
        &self,
        progress: f32,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
        rotation_pivot_x: impl Into<Option<f32>>,
        rotation_pivot_y: impl Into<Option<f32>>,
    ) -> Path;
    fn apply_to_path_builder(
        &self,
        path: &mut PathBuilder,
        progress: f32,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
        rotation_pivot_x: impl Into<Option<f32>>,
        rotation_pivot_y: impl Into<Option<f32>>,
    );
}

impl MorphToPath for Morph<'_> {
    fn to_path(
        &self,
        progress: f32,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
        rotation_pivot_x: impl Into<Option<f32>>,
        rotation_pivot_y: impl Into<Option<f32>>,
    ) -> Path {
        let mut path = PathBuilder::new();
        self.apply_to_path_builder(
            &mut path,
            progress,
            start_angle,
            repeat_path,
            close_path,
            rotation_pivot_x,
            rotation_pivot_y,
        );
        path.detach()
    }

    fn apply_to_path_builder(
        &self,
        path: &mut PathBuilder,
        progress: f32,
        start_angle: impl Into<Option<isize>>,
        repeat_path: impl Into<Option<bool>>,
        close_path: impl Into<Option<bool>>,
        rotation_pivot_x: impl Into<Option<f32>>,
        rotation_pivot_y: impl Into<Option<f32>>,
    ) {
        let start_angle = start_angle.into().unwrap_or(270);
        let repeat_path = repeat_path.into().unwrap_or(false);
        let close_path = close_path.into().unwrap_or(true);
        let rotation_pivot_x = rotation_pivot_x.into().unwrap_or(0.0);
        let rotation_pivot_y = rotation_pivot_y.into().unwrap_or(0.0);
        let cubics = self.as_cubics(progress);
        path_from_cubics(
            path,
            start_angle,
            repeat_path,
            close_path,
            &cubics,
            rotation_pivot_x,
            rotation_pivot_y,
        )
    }
}

fn path_from_cubics(
    path: &mut PathBuilder,
    start_angle: isize,
    repeat_path: bool,
    close_path: bool,
    cubics: &[Cubic],
    rotation_pivot_x: f32,
    rotation_pivot_y: f32,
) {
    let mut first = true;
    let mut first_cubic: Option<&Cubic> = None;
    path.reset();
    for cubic in cubics {
        if first {
            path.move_to((cubic.anchor_0_x(), cubic.anchor_0_y()));
            if start_angle != 0 {
                first_cubic = Some(cubic);
            }
            first = false;
        }
        path.cubic_to(
            (cubic.control_0_x(), cubic.control_0_y()),
            (cubic.control_1_x(), cubic.control_1_y()),
            (cubic.anchor_1_x(), cubic.anchor_1_y()),
        );
    }
    if repeat_path {
        let mut first_in_repeat = true;
        for cubic in cubics {
            if first_in_repeat {
                path.line_to((cubic.anchor_0_x(), cubic.anchor_0_y()));
                first_in_repeat = false;
            }
            path.cubic_to(
                (cubic.control_0_x(), cubic.control_0_y()),
                (cubic.control_1_x(), cubic.control_1_y()),
                (cubic.anchor_1_x(), cubic.anchor_1_y()),
            );
        }
    }
    if close_path {
        path.close();
    }
    if start_angle != 0
        && let Some(first_cubic) = first_cubic
    {
        let angle_to_first_cubic = randians_to_degrees(
            (cubics[0].anchor_0_y() - rotation_pivot_y)
                .atan2(cubics[0].anchor_0_x() - rotation_pivot_x),
        );
        // Rotate the Path to to start from the given angle.
        path.transform(&Matrix::rotate_deg(angle_to_first_cubic));
    }
}

fn randians_to_degrees(radians: f32) -> f32 {
    radians * 180.0 / PI
}
