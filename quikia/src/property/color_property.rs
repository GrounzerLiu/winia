use skia_safe::{Color, Color4f};

use super::{Gettable, SharedProperty};

pub type ColorProperty = SharedProperty<Color>;

impl From<&ColorProperty> for ColorProperty {
    fn from(color: &ColorProperty) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || color.get()))
    }
}

impl From<u32> for ColorProperty {
    fn from(color: u32) -> Self {
        Self::from_generator(Box::new(move || Color::from(color)))
    }
}

impl From<&u32> for ColorProperty {
    fn from(color: &u32) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || Color::from(color)))
    }
}

impl From<(u8, u8, u8, u8)> for ColorProperty {
    fn from(color: (u8, u8, u8, u8)) -> Self {
        Self::from_generator(Box::new(move || Color::from_argb(color.0, color.1, color.2, color.3)))
    }
}

impl From<&(u8, u8, u8, u8)> for ColorProperty {
    fn from(color: &(u8, u8, u8, u8)) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || Color::from_argb(color.0, color.1, color.2, color.3)))
    }
}

impl From<(u8, u8, u8)> for ColorProperty {
    fn from(color: (u8, u8, u8)) -> Self {
        Self::from_generator(Box::new(move || Color::from_argb(255, color.0, color.1, color.2)))
    }
}

impl From<&(u8, u8, u8)> for ColorProperty {
    fn from(color: &(u8, u8, u8)) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || Color::from_argb(255, color.0, color.1, color.2)))
    }
}

impl From<(f32, f32, f32, f32)> for ColorProperty {
    fn from(color: (f32, f32, f32, f32)) -> Self {
        Self::from_generator(Box::new(move || Color4f{a: color.0, r: color.1, g: color.2, b: color.3}.to_color()))
    }
}

impl From<&(f32, f32, f32, f32)> for ColorProperty {
    fn from(color: &(f32, f32, f32, f32)) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || Color4f{a: color.0, r: color.1, g: color.2, b: color.3}.to_color()))
    }
}

impl From<(f32, f32, f32)> for ColorProperty {
    fn from(color: (f32, f32, f32)) -> Self {
        Self::from_generator(Box::new(move || Color4f{a: 1.0, r: color.0, g: color.1, b: color.2}.to_color()))
    }
}

impl From<&(f32, f32, f32)> for ColorProperty {
    fn from(color: &(f32, f32, f32)) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || Color4f{a: 1.0, r: color.0, g: color.1, b: color.2}.to_color()))
    }
}

impl From<Color4f> for ColorProperty {
    fn from(color: Color4f) -> Self {
        Self::from_generator(Box::new(move || color.to_color()))
    }
}

impl From<&Color4f> for ColorProperty {
    fn from(color: &Color4f) -> Self {
        let color = color.clone();
        Self::from_generator(Box::new(move || color.to_color()))
    }
}

