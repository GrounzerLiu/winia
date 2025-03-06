use super::{Gettable, Shared, SharedAnimation};
use material_color_utilities::blend_cam16ucs;
use material_color_utilities::utils::argb_from_rgb;
use skia_safe::{Color, Color4f};

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
        Self::from_static(
            Color4f {
                a: color.0,
                r: color.1,
                g: color.2,
                b: color.3,
            }
            .to_color(),
        )
    }
}

impl From<&(f32, f32, f32, f32)> for SharedColor {
    fn from(color: &(f32, f32, f32, f32)) -> Self {
        Self::from_static(
            Color4f {
                a: color.0,
                r: color.1,
                g: color.2,
                b: color.3,
            }
            .to_color(),
        )
    }
}

impl From<(f32, f32, f32)> for SharedColor {
    fn from(color: (f32, f32, f32)) -> Self {
        Self::from_static(
            Color4f {
                a: 1.0,
                r: color.0,
                g: color.1,
                b: color.2,
            }
            .to_color(),
        )
    }
}

impl From<&(f32, f32, f32)> for SharedColor {
    fn from(color: &(f32, f32, f32)) -> Self {
        Self::from_static(
            Color4f {
                a: 1.0,
                r: color.0,
                g: color.1,
                b: color.2,
            }
            .to_color(),
        )
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

impl SharedColor {
    pub fn animation_to_color(&self, to: impl Into<Color>) -> SharedAnimation<Color> {
        SharedAnimation::new(
            self.clone(),
            self.get(),
            to.into(),
            Box::new(|from: &Color, to: &Color, progress: f32| {
                let from_a = from.a() as f32;
                let from_u32 = argb_from_rgb(from.r(), from.g(), from.b());
                let to_a = to.a() as f32;
                let to_u32 = argb_from_rgb(to.r(), to.g(), to.b());
                let blend_a = from_a + (to_a - from_a) * progress;
                let blend_u32 = blend_cam16ucs(from_u32, to_u32, progress as f64);
                let a = blend_a as u8;
                let r = (blend_u32 >> 16) as u8;
                let g = (blend_u32 >> 8) as u8;
                let b = blend_u32 as u8;
                Color::from_argb(a, r, g, b)
            }),
        )
    }
}
