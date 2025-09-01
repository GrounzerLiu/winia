mod animation;
pub mod interpolator;
pub use interpolator::Interpolator;
mod target;
mod local_animation;

use material_colors::blend::cam16_ucs;
use material_colors::color::Argb;
use skia_safe::Color;
pub use animation::*;
pub use target::*;
pub use local_animation::*;

pub trait Animation {
    fn interpolate_f32(&self, start: f32, end: f32) -> f32;
    fn interpolate_color(&self, start: &Color, end: &Color) -> Color;
    fn is_finished(&self) -> bool;
    fn finish(&mut self);
    /// (animatable, children_forced)
    fn animatable(&self, id: usize, forced: bool) -> (bool, bool);
    fn clone_boxed(&self) -> Box<dyn Animation>;
}

pub fn interpolate_f32(start: f32, end: f32, progress: f32, interpolator: &dyn Interpolator) -> f32
{
    let progress = progress.clamp(0.0, 1.0);
    let interpolated = interpolator.interpolate(progress);
    start + (end - start) * interpolated
}

pub fn interpolate_color(start: &Color, end: &Color, progress: f32) -> Color {
    let progress = (progress as f64).clamp(0.0, 1.0);
    let start_a = start.a() as f64;
    let start_argb = Argb::new(255, start.r(), start.g(), start.b());
    let end_a = end.a() as f64;
    let end_argb = Argb::new(255, end.r(), end.g(), end.b());
    let blend_a = start_a + (end_a - start_a) * progress;
    let blend_argb = cam16_ucs(start_argb, end_argb, progress);
    let a = blend_a as u8;
    let r = blend_argb.red;
    let g = blend_argb.green;
    let b = blend_argb.blue;
    Color::from_argb(a, r, g, b)
}