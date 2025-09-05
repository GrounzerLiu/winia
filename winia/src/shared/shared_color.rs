use crate::shared::{SharedDerived, SharedSource};
use skia_safe::{Color, Color4f};
use crate::depend;

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
        SharedSource::new(Color4f::new(r, g, b, a).to_color())
    }

    pub fn from_rgb_f(r: f32, g: f32, b: f32) -> Self {
        SharedSource::new(Color4f::new(r, g, b, 1.0).to_color())
    }

    pub fn set_a(&self, a: u8) -> SharedDerivedColor {
        let this = self.clone();
        SharedDerivedColor::from_fn(depend!(this), move || {
            let color = this.get();
            Color::from_argb(a, color.r(), color.g(), color.b())
        })
    }

    pub fn set_r(&self, r: u8) -> SharedDerivedColor {
        let this = self.clone();
        SharedDerivedColor::from_fn(depend!(this), move || {
            let color = this.get();
            Color::from_argb(color.a(), r, color.g(), color.b())
        })
    }

    pub fn set_g(&self, g: u8) -> SharedDerivedColor {
        let this = self.clone();
        SharedDerivedColor::from_fn(depend!(this), move || {
            let color = this.get();
            Color::from_argb(color.a(), color.r(), g, color.b())
        })
    }

    pub fn set_b(&self, b: u8) -> SharedDerivedColor {
        let this = self.clone();
        SharedDerivedColor::from_fn(depend!(this), move || {
            let color = this.get();
            Color::from_argb(color.a(), color.r(), color.g(), b)
        })
    }
}