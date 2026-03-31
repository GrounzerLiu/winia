use material_colors::blend::cam16_ucs;
use material_colors::color::Argb;
use skia_safe::{named_transfer_fn, Color4f, ColorSpace as SkiaColorSpace};
use skia_safe::named_primaries::CicpId;

#[derive(Copy, Debug, Clone, PartialEq)]
pub enum ColorSpace {
    Srgb,
    LinearSrgb,
}

impl From<ColorSpace> for SkiaColorSpace {
    fn from(value: ColorSpace) -> Self {
        match value {
            ColorSpace::Srgb => SkiaColorSpace::new_srgb(),
            ColorSpace::LinearSrgb => SkiaColorSpace::new_srgb_linear(),
        }
    }
}

#[derive(Copy, Debug, Clone, PartialEq)]
pub enum Color {
    ByteARGB(u8, u8, u8, u8),
    FloatARGB(f32, f32, f32, f32, Option<ColorSpace>),
}

impl Color {
    fn get_u8_argb(&self) -> (u8, u8, u8, u8) {
        match self {
            Color::ByteARGB(a, r, g, b) => (*a, *r, *g, *b),
            Color::FloatARGB(a, r, g, b, _) => (
                (a.clamp(0.0, 1.0) * 255.0) as u8,
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
            ),
        }
    }
    pub fn interpolate(&self, to: &Color, progress: f32) -> Color {
        // let progress = (progress as f64).clamp(0.0, 1.0);
        // let start_a = start.a() as f64;
        // let start_argb = Argb::new(255, start.r(), start.g(), start.b());
        // let end_a = end.a() as f64;
        // let end_argb = Argb::new(255, end.r(), end.g(), end.b());
        // let blend_a = start_a + (end_a - start_a) * progress;
        // let blend_argb = cam16_ucs(start_argb, end_argb, progress);
        // let a = blend_a as u8;
        // let r = blend_argb.red;
        // let g = blend_argb.green;
        // let b = blend_argb.blue;
        // Color::from_argb(a, r, g, b)
        if progress <= 0.0 {
            return *self;
        } else if progress >= 1.0 {
            return *to;
        }
        let (sa, sr, sg, sb) = self.get_u8_argb();
        let (ea, er, eg, eb) = to.get_u8_argb();
        let start_argb = Argb::new(255, sr, sg, sb);
        let end_argb = Argb::new(255, er, eg, eb);
        let blend_a = sa as f64 + (ea as f64 - sa as f64) * (progress as f64);
        let blend_argb = cam16_ucs(start_argb, end_argb, progress as f64);
        let a = blend_a as u8;
        let r = blend_argb.red;
        let g = blend_argb.green;
        let b = blend_argb.blue;
        Color::from_argb(a, r, g, b)
    }

    pub fn a(&self) -> u8 {
        match self {
            Color::ByteARGB(a, _, _, _) => *a,
            Color::FloatARGB(a, _, _, _, _) => (a.clamp(0.0, 1.0) * 255.0) as u8,
        }
    }
    pub fn r(&self) -> u8 {
        match self {
            Color::ByteARGB(_, r, _, _) => *r,
            Color::FloatARGB(_, r, _, _, _) => (r.clamp(0.0, 1.0) * 255.0) as u8,
        }
    }
    pub fn g(&self) -> u8 {
        match self {
            Color::ByteARGB(_, _, g, _) => *g,
            Color::FloatARGB(_, _, g, _, _) => (g.clamp(0.0, 1.0) * 255.0) as u8,
        }
    }
    pub fn b(&self) -> u8 {
        match self {
            Color::ByteARGB(_, _, _, b) => *b,
            Color::FloatARGB(_, _, _, b, _) => (b.clamp(0.0, 1.0) * 255.0) as u8,
        }
    }
    pub fn a_f(&self) -> f32 {
        match self {
            Color::ByteARGB(a, _, _, _) => (*a as f32) / 255.0,
            Color::FloatARGB(a, _, _, _, _) => *a,
        }
    }
    pub fn r_f(&self) -> f32 {
        match self {
            Color::ByteARGB(_, r, _, _) => (*r as f32) / 255.0,
            Color::FloatARGB(_, r, _, _, _) => *r,
        }
    }
    pub fn g_f(&self) -> f32 {
        match self {
            Color::ByteARGB(_, _, g, _) => (*g as f32) / 255.0,
            Color::FloatARGB(_, _, g, _, _) => *g,
        }
    }
    pub fn b_f(&self) -> f32 {
        match self {
            Color::ByteARGB(_, _, _, b) => (*b as f32) / 255.0,
            Color::FloatARGB(_, _, _, b, _) => *b,
        }
    }
}

impl From<u32> for Color {
    fn from(value: u32) -> Self {
        Color::from_u32(value)
    }
}

impl From<skia_safe::Color> for Color {
    fn from(value: skia_safe::Color) -> Self {
        let a = value.a();
        let r = value.r();
        let g = value.g();
        let b = value.b();
        Color::ByteARGB(a, r, g, b)
    }
}

impl From<Color4f> for Color {
    fn from(value: Color4f) -> Self {
        Color::FloatARGB(value.a, value.r, value.g, value.b, None)
    }
}

impl From<&Color> for Color {
    fn from(value: &Color) -> Self {
        *value
    }
}

impl From<&skia_safe::Color> for Color {
    fn from(value: &skia_safe::Color) -> Self {
        Color::from(*value)
    }
}

impl From<&Color4f> for Color {
    fn from(value: &Color4f) -> Self {
        Color::from(*value)
    }
}

impl Color {
    pub fn to_skia_color(&self) -> skia_safe::Color {
        let (a, r, g, b) = self.get_u8_argb();
        skia_safe::Color::from_argb(a, r, g, b)
    }

    pub fn to_color4f(&self) -> Color4f {
        match self {
            Color::ByteARGB(a, r, g, b) => Color4f::new(
                (*r as f32) / 255.0,
                (*g as f32) / 255.0,
                (*b as f32) / 255.0,
                (*a as f32) / 255.0,
            ),
            Color::FloatARGB(a, r, g, b, _) => Color4f::new(*r, *g, *b, *a),
        }
    }
}
impl Color {
    pub fn from_argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Color::ByteARGB(a, r, g, b)
    }

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color::ByteARGB(255, r, g, b)
    }

    pub fn from_u32(value: u32) -> Self {
        let a = ((value >> 24) & 0xFF) as u8;
        let r = ((value >> 16) & 0xFF) as u8;
        let g = ((value >> 8) & 0xFF) as u8;
        let b = (value & 0xFF) as u8;
        Color::ByteARGB(a, r, g, b)
    }

    pub fn from_argb_f(a: f32, r: f32, g: f32, b: f32) -> Self {
        Color::FloatARGB(a, r, g, b, None)
    }

    pub fn from_argb_f_with_colorspace(
        a: f32,
        r: f32,
        g: f32,
        b: f32,
        color_space: ColorSpace,
    ) -> Self {
        Color::FloatARGB(a, r, g, b, Some(color_space))
    }

    pub fn from_rgb_f(r: f32, g: f32, b: f32) -> Self {
        Color::FloatARGB(1.0, r, g, b, None)
    }

    pub fn from_rgb_f_with_colorspace(r: f32, g: f32, b: f32, color_space: ColorSpace) -> Self {
        Color::FloatARGB(1.0, r, g, b, Some(color_space))
    }

    pub fn with_a(self, a: u8) -> Self {
        match self {
            Color::ByteARGB(_, r, g, b) => Color::ByteARGB(a, r, g, b),
            Color::FloatARGB(_, r, g, b, color_space) => {
                let a_f = (a as f32) / 255.0;
                Color::FloatARGB(a_f, r, g, b, color_space)
            }
        }
    }

    pub fn with_a_f(self, a: f32) -> Self {
        match self {
            Color::ByteARGB(_, r, g, b) => {
                let a_u8 = (a.clamp(0.0, 1.0) * 255.0) as u8;
                Color::ByteARGB(a_u8, r, g, b)
            }
            Color::FloatARGB(_, r, g, b, color_space) => {
                Color::FloatARGB(a.clamp(0.0, 1.0), r, g, b, color_space)
            }
        }
    }

    pub const TRANSPARENT: Color = Color::ByteARGB(0, 0, 0, 0);
    pub const BLACK: Color = Color::ByteARGB(255, 0, 0, 0);
    pub const WHITE: Color = Color::ByteARGB(255, 255, 255, 255);

    pub const RED: Color = Color::ByteARGB(255, 255, 0, 0);
    pub const GREEN: Color = Color::ByteARGB(255, 0, 255, 0);
    pub const BLUE: Color = Color::ByteARGB(255, 0, 0, 255);

    pub const YELLOW: Color = Color::ByteARGB(255, 255, 255, 0);
    pub const CYAN: Color = Color::ByteARGB(255, 0, 255, 255);
    pub const MAGENTA: Color = Color::ByteARGB(255, 255, 0, 255);
    pub const ORANGE: Color = Color::ByteARGB(255, 255, 165, 0);
    pub const PURPLE: Color = Color::ByteARGB(255, 128, 0, 128);
    pub const PINK: Color = Color::ByteARGB(255, 255, 192, 203);
    pub const BROWN: Color = Color::ByteARGB(255, 165, 42, 42);
    pub const GRAY: Color = Color::ByteARGB(255, 128, 128, 128);
    pub const LIGHT_GRAY: Color = Color::ByteARGB(255, 192, 192, 192);
    pub const DARK_GRAY: Color = Color::ByteARGB(255, 64, 64, 64);

    pub const LIME: Color = Color::ByteARGB(255, 50, 205, 50);
    pub const TEAL: Color = Color::ByteARGB(255, 0, 128, 128);
    pub const NAVY: Color = Color::ByteARGB(255, 0, 0, 128);
    pub const OLIVE: Color = Color::ByteARGB(255, 128, 128, 0);
    pub const MAROON: Color = Color::ByteARGB(255, 128, 0, 0);
    pub const SILVER: Color = Color::ByteARGB(255, 192, 192, 192);
    pub const GOLD: Color = Color::ByteARGB(255, 255, 215, 0);
}

pub trait SetColor {
    fn set_any_color(&mut self, color: impl Into<Color>) -> &mut Self;
}

impl SetColor for skia_safe::Paint {
    fn set_any_color(&mut self, color: impl Into<Color>) -> &mut Self {
        let color: Color = color.into();
        match color {
            Color::ByteARGB(a, r, g, b) => {
                self.set_color(skia_safe::Color::from_argb(a, r, g, b));
            }
            Color::FloatARGB(a, r, g, b, color_space) => {
                let color_space: Option<SkiaColorSpace> = color_space.map(|cs| cs.into());
                // self.set_color4f(Color4f::new(r, g, b, a), &color_space);
                self.set_color4f(Color4f::new(r, g, b, a), &color_space);

            }
        }
        self
    }
}
