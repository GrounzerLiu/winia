use std::collections::HashMap;
use skia_safe::Color;
use crate::core::RefClone;
use crate::property::{Gettable, Property};
use crate::ui::item::DisplayParameter;

pub enum Value<T> {
    Static(T),
    Dynamic(Property<DisplayParameter>),
}

impl<T> Value<T> {
    pub fn from_static(value: T) -> Self {
        Self::Static(value)
    }

    pub fn from_dynamic(property: Property<DisplayParameter>) -> Self {
        Self::Dynamic(property)
    }
}

impl From<f32> for Value<f32> {
    fn from(value: f32) -> Self {
        Self::from_static(value)
    }
}

impl From<Color> for Value<Color> {
    fn from(value: Color) -> Self {
        Self::from_static(value)
    }
}

impl From<Property<DisplayParameter>> for Value<f32> {
    fn from(property: Property<DisplayParameter>) -> Self {
        Self::from_dynamic(property)
    }
}

impl From<Property<DisplayParameter>> for Value<Color> {
    fn from(property: Property<DisplayParameter>) -> Self {
        Self::from_dynamic(property)
    }
}

pub struct ParameterOption {
    width: Option<(Value<f32>, Value<f32>)>,
    height: Option<(Value<f32>, Value<f32>)>,
    parent_x: Option<(Value<f32>, Value<f32>)>,
    parent_y: Option<(Value<f32>, Value<f32>)>,
    relative_x: Option<(Value<f32>, Value<f32>)>,
    relative_y: Option<(Value<f32>, Value<f32>)>,
    opacity: Option<(Value<f32>, Value<f32>)>,
    rotation: Option<(Value<f32>, Value<f32>)>,
    float_options: HashMap<String, (Value<f32>, Value<f32>)>,
    color_options: HashMap<String, (Value<Color>, Value<Color>)>,
}

impl Default for ParameterOption {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! impl_fn {
    ($name:ident, $getter:ident) => {
        impl ParameterOption {
            pub fn $name(mut self, from: impl Into<Value<f32>>, to: impl Into<Value<f32>>) -> Self {
                self.$name = Some((from.into(), to.into()));
                self
            }
            pub(crate) fn $getter(&self) -> Option<(f32, f32)> {
                let (from, to) = self.$name.as_ref()?;
                Some((
                    match from{
                        Value::Static(v) => *v,
                        Value::Dynamic(v) => v.get().$name(),
                    }, 
                    match to {
                      Value::Static(v) => *v,
                      Value::Dynamic(v) => v.get().$name(),
                    }))
            }
        }
    }
}

impl_fn!(width, get_width);
impl_fn!(height, get_height);
impl_fn!(parent_x, get_parent_x);
impl_fn!(parent_y, get_parent_y);
impl_fn!(relative_x, get_relative_x);
impl_fn!(relative_y, get_relative_y);
impl_fn!(opacity, get_opacity);
impl_fn!(rotation, get_rotation);


impl ParameterOption {
    pub fn new() -> Self {
        Self {
            width: None,
            height: None,
            parent_x: None,
            parent_y: None,
            relative_x: None,
            relative_y: None,
            opacity: None,
            rotation: None,
            float_options: HashMap::new(),
            color_options: HashMap::new(),
        }
    }

    pub fn float_option(mut self, name: &str, from: impl Into<Value<f32>>, to: impl Into<Value<f32>>) -> Self {
        self.float_options.insert(name.to_string(), (from.into(), to.into()));
        self
    }

    pub fn color_option(mut self, name: &str, from: impl Into<Value<Color>>, to: impl Into<Value<Color>>) -> Self {
        self.color_options.insert(name.to_string(), (from.into(), to.into()));
        self
    }

    pub(crate) fn get_float_option(&self, name: &str) -> Option<(f32, f32)> {
        let (from, to) = self.float_options.get(name)?;
        Some(
            (
                match from {
                    Value::Static(v) => *v,
                    Value::Dynamic(v) => *v.get().get_float_param(name)?
                },
                match to {
                    Value::Static(v) => *v,
                    Value::Dynamic(v) => *v.get().get_float_param(name)?
                }
            )
        )
    }

    pub(crate) fn get_color_option(&self, name: &str) -> Option<(Color, Color)> {
        let (from, to) = self.color_options.get(name)?;
        Some((
            match from {
                Value::Static(v) => *v,
                Value::Dynamic(v) => *v.get().get_color_param(name)?
            },
            match to {
                Value::Static(v) => *v,
                Value::Dynamic(v) => *v.get().get_color_param(name)?
            }
        ))
    }
}