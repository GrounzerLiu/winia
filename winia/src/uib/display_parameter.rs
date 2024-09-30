use std::collections::{HashMap, HashSet};
use skia_safe::Color;
use material_color_utilities::blend_cam16ucs;
use crate::property::Gettable;
use crate::uib::{Edges, Item, LayoutDirection};

#[derive(Clone, Debug)]
pub struct DisplayParameter {
    pub width: f32,
    pub height: f32,
    pub parent_x: f32,
    pub parent_y: f32,
    pub relative_x: f32,
    pub relative_y: f32,
    pub padding_start: f32,
    pub padding_top: f32,
    pub padding_end: f32,
    pub padding_bottom: f32,
    pub margin_start: f32,
    pub margin_top: f32,
    pub margin_end: f32,
    pub margin_bottom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub float_params: HashMap<String, f32>,
    pub color_params: HashMap<String, Color>,
}

impl DisplayParameter {
    pub fn x(&self) -> f32 {
        self.parent_x + self.relative_x + self.offset_x
    }

    pub fn y(&self) -> f32 {
        self.parent_y + self.relative_y + self.offset_y
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x() && x <= self.x() + self.width && y >= self.y() && y <= self.y() + self.height
    }

    pub fn set_float_param(&mut self, key: impl Into<String>, value: f32) {
        self.float_params.insert(key.into(), value);
    }

    pub fn get_float_param(&self, key: impl Into<String>) -> Option<&f32> {
        self.float_params.get(&key.into())
    }

    pub fn set_color_param(&mut self, key: impl Into<String>, value: Color) {
        self.color_params.insert(key.into(), value);
    }

    pub fn get_color_param(&self, key: impl Into<String>) -> Option<&Color> {
        self.color_params.get(&key.into())
    }

    pub fn init_from_item(&mut self, item: &Item) {
        self.padding_start = item.get_padding_start().get();
        self.padding_top = item.get_padding_top().get();
        self.padding_end = item.get_padding_end().get();
        self.padding_bottom = item.get_padding_bottom().get();
        self.margin_start = item.get_margin_start().get();
        self.margin_top = item.get_margin_top().get();
        self.margin_end = item.get_margin_end().get();
        self.margin_bottom = item.get_margin_bottom().get();
        self.offset_x = item.get_offset_x().get();
        self.offset_y = item.get_offset_y().get();
        self.max_width = item.get_max_width().get();
        self.max_height = item.get_max_height().get();
        self.min_width = item.get_min_width().get();
        self.min_height = item.get_min_height().get();
    }

    pub fn copy_from(&mut self, display_parameter: &DisplayParameter) {
        self.width = display_parameter.width;
        self.height = display_parameter.height;
        self.parent_x = display_parameter.parent_x;
        self.parent_y = display_parameter.parent_y;
        self.relative_x = display_parameter.relative_x;
        self.relative_y = display_parameter.relative_y;
        self.padding_start = display_parameter.padding_start;
        self.padding_top = display_parameter.padding_top;
        self.padding_end = display_parameter.padding_end;
        self.padding_bottom = display_parameter.padding_bottom;
        self.margin_start = display_parameter.margin_start;
        self.margin_top = display_parameter.margin_top;
        self.margin_end = display_parameter.margin_end;
        self.margin_bottom = display_parameter.margin_bottom;
        self.offset_x = display_parameter.offset_x;
        self.offset_y = display_parameter.offset_y;
        self.max_width = display_parameter.max_width;
        self.max_height = display_parameter.max_height;
        self.min_width = display_parameter.min_width;
        self.min_height = display_parameter.min_height;
        self.float_params = display_parameter.float_params.clone();
        self.color_params = display_parameter.color_params.clone();
    }
}

impl Default for DisplayParameter {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            parent_x: 0.0,
            parent_y: 0.0,
            relative_x: 0.0,
            relative_y: 0.0,
            padding_start: 0.0,
            padding_top: 0.0,
            padding_end: 0.0,
            padding_bottom: 0.0,
            margin_start: 0.0,
            margin_top: 0.0,
            margin_end: 0.0,
            margin_bottom: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
            min_width: 0.0,
            min_height: 0.0,
            float_params: HashMap::new(),
            color_params: HashMap::new(),
        }
    }
}

impl From<&DisplayParameter> for DisplayParameter {
    fn from(display_parameter: &DisplayParameter) -> Self {
        display_parameter.clone()
    }
}

pub(super) struct AnimationDisplayParameter {
    width: Option<f32>,
    height: Option<f32>,
    relative_x: Option<f32>,
    relative_y: Option<f32>,
    float_params: HashMap<String, f32>,
    color_params: HashMap<String, Color>,
}


fn color_to_argb(color: &Color) -> u32 {
    let color = color.clone();
    let a = color.a();
    let r = color.r();
    let g = color.g();
    let b = color.b();
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

impl AnimationDisplayParameter {
    pub fn interpolate(&self, other: &DisplayParameter, progress: f32) -> DisplayParameter {
        let mut display_parameter = other.clone();
        if let Some(width) = self.width {
            display_parameter.width = width + (other.width - width) * progress;
        }
        if let Some(height) = self.height {
            display_parameter.height = height + (other.height - height) * progress;
        }
        if let Some(relative_x) = self.relative_x {
            display_parameter.relative_x = relative_x + (other.relative_x - relative_x) * progress;
        }
        if let Some(relative_y) = self.relative_y {
            display_parameter.relative_y = relative_y + (other.relative_y - relative_y) * progress;
        }
        for (key, value) in &self.float_params {
            let other_value = other.float_params.get(key);
            if let Some(other_value) = other_value {
                display_parameter.float_params.insert(key.clone(), value + (other_value - value) * progress);
            } else {
                display_parameter.float_params.insert(key.clone(), *value);
            }
        }
        for (key, value) in &self.color_params {
            let other_value = other.color_params.get(key);
            if let Some(other_value) = other_value {
                let interpolated_color = blend_cam16ucs(color_to_argb(value), color_to_argb(other_value), progress as f64);
                display_parameter.color_params.insert(key.clone(), Color::from(interpolated_color));
            } else {
                display_parameter.color_params.insert(key.clone(), *value);
            }
        }
        display_parameter
    }
}

fn f32_near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

impl DisplayParameter {
    /// Returns true if the two display parameters are different.
    /// But only compares the width, height, relative_x, relative_y, float_params, and color_params.
    /// Because these will directly affect the layout of the item.
    /// This function is only used to implement the animation system.
    pub(crate) fn different_from(&self, other: &Self) -> Option<AnimationDisplayParameter> {
        let mut animation_display_parameter = AnimationDisplayParameter {
            width: None,
            height: None,
            relative_x: None,
            relative_y: None,
            float_params: HashMap::new(),
            color_params: HashMap::new(),
        };

        let mut is_different = false;

        if !f32_near(self.width, other.width) {
            animation_display_parameter.width = Some(self.width);
            is_different = true;
        }
        if !f32_near(self.height, other.height) {
            animation_display_parameter.height = Some(self.height);
            is_different = true;
        }
        if !f32_near(self.relative_x, other.relative_x) {
            animation_display_parameter.relative_x = Some(self.relative_x);
            is_different = true;
        }
        if !f32_near(self.relative_y, other.relative_y) {
            animation_display_parameter.relative_y = Some(self.relative_y);
            is_different = true;
        }
        {
            let self_keys = self.float_params.keys().collect::<HashSet<&String>>();
            let other_keys = other.float_params.keys().collect::<HashSet<&String>>();
            let intersection = self_keys.intersection(&other_keys).collect::<HashSet<&&String>>();
            for key in intersection {
                let self_value = self.float_params.get(*key).unwrap();
                let other_value = other.float_params.get(*key).unwrap();
                if !f32_near(*self_value, *other_value) {
                    animation_display_parameter.float_params.insert((*key).clone(), *self_value);
                    is_different = true;
                }
            }
        }
        {
            let self_keys = self.color_params.keys().collect::<HashSet<&String>>();
            let other_keys = other.color_params.keys().collect::<HashSet<&String>>();
            let intersection = self_keys.intersection(&other_keys).collect::<HashSet<&&String>>();
            for key in intersection {
                let self_value = self.color_params.get(*key).unwrap();
                let other_value = other.color_params.get(*key).unwrap();
                if self_value != other_value {
                    animation_display_parameter.color_params.insert((*key).clone(), self_value.clone());
                    is_different = true;
                }
            }
        }
        if is_different {
            Some(animation_display_parameter)
        } else {
            None
        }
    }
}
