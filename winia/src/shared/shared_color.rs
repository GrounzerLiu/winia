use std::time::{Duration, Instant};
use skia_safe::{Color, Color4f};
use material_color_utilities::blend_cam16ucs;
use material_color_utilities::utils::argb_from_rgb;
use crate::ui::animation::interpolator::Interpolator;
use crate::ui::animation::interpolator::Linear;
use crate::ui::app::AppContext;
use super::{Gettable, Settable, Shared, SharedAnimation};

pub type SharedColor = Shared<Color>;

impl From<u32> for SharedColor {
    fn from(color: u32) -> Self {
        Self::from_static(Color::from(color))
    }
}

impl From<&u32> for SharedColor {
    fn from(color: &u32) -> Self {
        let color = *color;
        Self::from_static(Color::from(color))
    }
}

impl From<(u8, u8, u8, u8)> for SharedColor {
    fn from(color: (u8, u8, u8, u8)) -> Self {
        Self::from_static(Color::from_argb(color.0, color.1, color.2, color.3))
    }
}

impl From<&(u8, u8, u8, u8)> for SharedColor {
    fn from(color: &(u8, u8, u8, u8)) -> Self {
        Self::from_static(Color::from_argb(color.0, color.1, color.2, color.3))
    }
}

impl From<(u8, u8, u8)> for SharedColor {
    fn from(color: (u8, u8, u8)) -> Self {
        Self::from_static(Color::from_argb(255, color.0, color.1, color.2))
    }
}

impl From<&(u8, u8, u8)> for SharedColor {
    fn from(color: &(u8, u8, u8)) -> Self {
        Self::from_static(Color::from_argb(255, color.0, color.1, color.2))
    }
}

impl From<(f32, f32, f32, f32)> for SharedColor {
    fn from(color: (f32, f32, f32, f32)) -> Self {
        Self::from_static(Color4f{a: color.0, r: color.1, g: color.2, b: color.3}.to_color())
    }
}

impl From<&(f32, f32, f32, f32)> for SharedColor {
    fn from(color: &(f32, f32, f32, f32)) -> Self {
        Self::from_static(Color4f{a: color.0, r: color.1, g: color.2, b: color.3}.to_color())
    }
}

impl From<(f32, f32, f32)> for SharedColor {
    fn from(color: (f32, f32, f32)) -> Self {
        Self::from_static(Color4f{a: 1.0, r: color.0, g: color.1, b: color.2}.to_color())
    }
}

impl From<&(f32, f32, f32)> for SharedColor {
    fn from(color: &(f32, f32, f32)) -> Self {
        Self::from_static(Color4f{a: 1.0, r: color.0, g: color.1, b: color.2}.to_color())
    }
}

impl From<Color4f> for SharedColor {
    fn from(color: Color4f) -> Self {
        Self::from_static(color.to_color())
    }
}

impl From<&Color4f> for SharedColor {
    fn from(color: &Color4f) -> Self {
        let color = *color;
        Self::from_static(color.to_color())
    }
}


pub struct ColorAnimation{
    color: SharedColor,
    from: Color,
    to: Color,
    duration: Duration,
    start_time: Instant,
    interpolator: Box<dyn Interpolator>,
}

impl ColorAnimation {
    pub(crate) fn new(color: SharedColor, from: Color, to: Color, duration: Duration, interpolator: Box<dyn Interpolator>) -> Self {
        Self {
            color,
            from,
            to,
            duration,
            start_time: Instant::now(),
            interpolator,
        }
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn interpolator(mut self, interpolator: impl Interpolator + 'static) -> Self {
        self.interpolator = Box::new(interpolator);
        self
    }
}

impl SharedAnimation for ColorAnimation{
    fn start(mut self, app_context: &AppContext){
        self.start_time = Instant::now();
        app_context.shared_animations.value().push(Box::new(self));
        app_context.request_redraw();
    }
    fn is_finished(&self) -> bool{
        self.start_time.elapsed() >= self.duration
    }
    fn update(&mut self){
        if self.is_finished(){
            return;
        }
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0);
        let interpolated = self.interpolator.interpolate(progress);
        let from_a = self.from.a() as f32;
        let from_u32 = argb_from_rgb(self.from.r(), self.from.g(), self.from.b());
        let to_a = self.to.a() as f32;
        let to_u32 = argb_from_rgb(self.to.r(), self.to.g(), self.to.b());
        let blend_a = from_a + (to_a - from_a) * interpolated;
        let blend_u32 = blend_cam16ucs(from_u32, to_u32, interpolated as f64);
        let a = blend_a as u8;
        let r = (blend_u32 >> 16) as u8;
        let g = (blend_u32 >> 8) as u8;
        let b = blend_u32 as u8;
        let color = Color::from_argb(a, r, g, b);
        self.color.set(color);
    }
}


impl SharedColor{
    pub fn animation_to_color(&self, to: impl Into<Color>) -> ColorAnimation{
        ColorAnimation::new(
            self.clone(),
            self.get(),
            to.into(),
            Duration::from_millis(1000),
            Box::new(Linear::new()),
        )
    }
}
