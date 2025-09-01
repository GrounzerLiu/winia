use crate::ui::item::Orientation;
use skia_safe::Color;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DisplayParameter {
    pub visible: bool,
    pub margin_start: f32,
    pub margin_end: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub padding_start: f32,
    pub padding_end: f32,
    pub padding_top: f32,
    pub padding_bottom: f32,
    pub parent_x: f32,
    pub parent_y: f32,
    pub width: f32,
    pub height: f32,
    pub relative_x: f32,
    pub relative_y: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub rotation_center_x: f32,
    pub rotation_center_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub scale_center_x: f32,
    pub scale_center_y: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub skew_center_x: f32,
    pub skew_center_y: f32,
    pub float_params: HashMap<String, f32>,
    pub color_params: HashMap<String, Color>,
}

impl DisplayParameter {
    pub fn copy_from(&mut self, other: &DisplayParameter) {
        self.visible = other.visible;
        self.margin_start = other.margin_start;
        self.margin_end = other.margin_end;
        self.margin_top = other.margin_top;
        self.margin_bottom = other.margin_bottom;
        self.padding_start = other.padding_start;
        self.padding_end = other.padding_end;
        self.padding_top = other.padding_top;
        self.padding_bottom = other.padding_bottom;
        self.parent_x = other.parent_x;
        self.parent_y = other.parent_y;
        self.width = other.width;
        self.height = other.height;
        self.relative_x = other.relative_x;
        self.relative_y = other.relative_y;
        self.offset_x = other.offset_x;
        self.offset_y = other.offset_y;
        self.opacity = other.opacity;
        self.rotation = other.rotation;
        self.rotation_center_x = other.rotation_center_x;
        self.rotation_center_y = other.rotation_center_y;
        self.scale_x = other.scale_x;
        self.scale_y = other.scale_y;
        self.scale_center_x = other.scale_center_x;
        self.scale_center_y = other.scale_center_y;
        self.skew_x = other.skew_x;
        self.skew_y = other.skew_y;
        self.skew_center_x = other.skew_center_x;
        self.skew_center_y = other.skew_center_y;
        self.float_params = other.float_params.clone();
        self.color_params = other.color_params.clone();
    }

    pub fn x(&self) -> f32 {
        self.parent_x + self.relative_x + self.offset_x
    }

    pub fn y(&self) -> f32 {
        self.parent_y + self.relative_y + self.offset_y
    }

    pub fn is_inside(&self, x: f32, y: f32) -> bool {
        x >= self.x() && x <= self.x() + self.width && y >= self.y() && y <= self.y() + self.height
    }

    pub fn size(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.width,
            Orientation::Vertical => self.height,
        }
    }

    pub fn set_size(&mut self, orientation: Orientation, size: f32) {
        match orientation {
            Orientation::Horizontal => self.width = size,
            Orientation::Vertical => self.height = size,
        }
    }
    
    pub fn margin_horizontal(&self) -> f32 {
        self.margin_start + self.margin_end
    }
    
    pub fn margin_vertical(&self) -> f32 {
        self.margin_top + self.margin_bottom
    }
    
    pub fn padding_horizontal(&self) -> f32 {
        self.padding_start + self.padding_end
    }
    
    pub fn padding_vertical(&self) -> f32 {
        self.padding_top + self.padding_bottom
    }
    
    pub fn outer_width(&self) -> f32 {
        if self.visible {
            self.width + self.margin_horizontal() + self.padding_horizontal()
        } else {
            0.0
        }
    }
    
    pub fn outer_height(&self) -> f32 {
        if self.visible {
            self.height + self.margin_vertical() + self.padding_vertical()
        } else {
            0.0
        }
    }
    
    pub fn outer_size(&self, orientation: Orientation) -> f32 {
        match orientation {
            Orientation::Horizontal => self.outer_width(),
            Orientation::Vertical => self.outer_height(),
        }
    }

    pub fn set_parent_position(&mut self, x: f32, y: f32) {
        self.parent_x = x;
        self.parent_y = y;
    }

    pub fn set_relative_position(&mut self, x: f32, y: f32) {
        self.relative_x = x;
        self.relative_y = y;
    }

    pub fn set_offset(&mut self, x: f32, y: f32) {
        self.offset_x = x;
        self.offset_y = y;
    }

    pub fn set_rotation_center(&mut self, x: f32, y: f32) {
        self.rotation_center_x = x;
        self.rotation_center_y = y;
    }

    pub fn set_scale(&mut self, x: f32, y: f32) {
        self.scale_x = x;
        self.scale_y = y;
    }

    pub fn set_scale_center(&mut self, x: f32, y: f32) {
        self.scale_center_x = x;
        self.scale_center_y = y;
    }

    pub fn set_skew(&mut self, x: f32, y: f32) {
        self.skew_x = x;
        self.skew_y = y;
    }

    pub fn set_skew_center(&mut self, x: f32, y: f32) {
        self.skew_center_x = x;
        self.skew_center_y = y;
    }

    pub fn set_float_param(&mut self, key: impl Into<String>, value: f32) {
        self.float_params.insert(key.into(), value);
    }

    pub fn set_color_param(&mut self, key: impl Into<String>, value: Color) {
        self.color_params.insert(key.into(), value);
    }

    pub fn get_float_param(&self, key: &str) -> Option<f32> {
        self.float_params.get(key).copied()
    }

    pub fn get_color_param(&self, key: &str) -> Option<Color> {
        self.color_params.get(key).copied()
    }
    
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 && self.height <= 0.0
    }
}

impl Default for DisplayParameter {
    fn default() -> Self {
        Self {
            visible: true,
            margin_start: 0.0,
            margin_end: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            padding_start: 0.0,
            padding_end: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            parent_x: 0.0,
            parent_y: 0.0,
            width: 0.0,
            height: 0.0,
            relative_x: 0.0,
            relative_y: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            opacity: 1.0,
            rotation: 0.0,
            rotation_center_x: 0.0,
            rotation_center_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            scale_center_x: 0.0,
            scale_center_y: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            skew_center_x: 0.0,
            skew_center_y: 0.0,
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

impl AsRef<DisplayParameter> for DisplayParameter {
    fn as_ref(&self) -> &DisplayParameter {
        self
    }
}

fn color_to_argb(color: &Color) -> u32 {
    let color = *color;
    let a = color.a();
    let r = color.r();
    let g = color.g();
    let b = color.b();
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn f32_near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}