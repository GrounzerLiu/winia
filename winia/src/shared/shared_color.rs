use crate::shared::{SharedAnimation, SharedDerived, SharedSource};
use crate::ui::Color;

pub type SharedColor = SharedSource<Color>;
pub type SharedDerivedColor = SharedDerived<Color>;

impl From<u32> for SharedColor {
    fn from(value: u32) -> Self {
        SharedSource::new(Color::from(value))
    }
}

impl SharedColor {
    pub fn from_argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        SharedSource::new(Color::from_argb(a, r, g, b))
    }

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        SharedSource::new(Color::from_rgb(r, g, b))
    }

    pub fn from_argb_f(a: f32, r: f32, g: f32, b: f32) -> Self {
        SharedSource::new(Color::from_argb_f(a, r, g, b))
    }

    pub fn from_rgb_f(r: f32, g: f32, b: f32) -> Self {
        SharedSource::new(Color::from_rgb_f(r, g, b))
    }
}

impl SharedColor {
    pub fn animation_to_color(&self, to: impl Into<Color>) -> SharedAnimation<Color> {
        SharedAnimation::new(
            self.clone(),
            self.get(),
            to.into(),
            Box::new(|from: &Color, to: &Color, progress: f32| {
                from.interpolate(to, progress)
            }),
        )
    }
}