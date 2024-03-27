use std::collections::HashMap;
use skia_safe::Color;
use crate::property::Gettable;
use crate::ui::Item;

#[derive(Clone, Debug, PartialEq)]
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