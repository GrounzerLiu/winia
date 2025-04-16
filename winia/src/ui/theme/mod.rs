pub mod theme;

use skia_safe::Color;
use std::marker::PhantomData;
pub use theme::*;
pub mod color;
mod material_theme;
pub mod styles;
pub mod elevation;
pub mod shape;
pub mod typescale;

pub use material_theme::*;
use proc_macro::style;

#[style]
pub struct Shape {
    top_start: f32,
    top_end: f32,
    bottom_end: f32,
    bottom_start: f32,
}


pub struct StyleProperty<'t, T> {
    theme: &'t mut Theme,
    key: String,
    _marker: PhantomData<T>,
}

impl<'t, T> StyleProperty<'t, T> {
    pub fn new(theme: &'t mut Theme, key: impl Into<String>) -> Self {
        StyleProperty { theme, key: key.into() , _marker: PhantomData }
    }
}


pub trait Access<T: Clone> {
    fn get(&self) -> Option<T>;
    fn set(&mut self, value: impl Into<Value<T>>);
}

impl Access<f32> for StyleProperty<'_, f32> {
    fn get(&self) -> Option<f32> {
        self.theme.get_dimension(&self.key)
    }

    fn set(&mut self, value: impl Into<Value<f32>>) {
        self.theme.set_dimension(&self.key, value);
    }
}

impl Access<Color> for StyleProperty<'_, Color> {
    fn get(&self) -> Option<Color> {
        self.theme.get_color(&self.key)
    }

    fn set(&mut self, value: impl Into<Value<Color>>) {
        self.theme.set_color(&self.key, value);
    }
}

impl Access<bool> for StyleProperty<'_, bool> {
    fn get(&self) -> Option<bool> {
        self.theme.get_bool(&self.key)
    }

    fn set(&mut self, value: impl Into<Value<bool>>) {
        self.theme.set_bool(&self.key, value);
    }
}