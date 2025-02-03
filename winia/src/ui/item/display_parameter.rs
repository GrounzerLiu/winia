use skia_safe::Color;
use std::collections::HashMap;
use crate::ui::item::Orientation;

#[derive(Debug)]
pub struct DisplayParameter {
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
    
    pub fn set_scale(&mut self, x: f32, y: f32){
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
}

impl Default for DisplayParameter {
    fn default() -> Self {
        Self {
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

impl Clone for DisplayParameter {
    fn clone(&self) -> Self {
        Self {
            parent_x: self.parent_x,
            parent_y: self.parent_y,
            width: self.width,
            height: self.height,
            relative_x: self.relative_x,
            relative_y: self.relative_y,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            opacity: self.opacity,
            rotation: self.rotation,
            rotation_center_x: self.rotation_center_x,
            rotation_center_y: self.rotation_center_y,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            scale_center_x: self.scale_center_x,
            scale_center_y: self.scale_center_y,
            skew_x: self.skew_x,
            skew_y: self.skew_y,
            skew_center_x: self.skew_center_x,
            skew_center_y: self.skew_center_y,
            float_params: self.float_params.clone(),
            color_params: self.color_params.clone(),
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
    let color = color.clone();
    let a = color.a();
    let r = color.r();
    let g = color.g();
    let b = color.b();
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn f32_near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

// pub fn parameter_interpolate(start: &DisplayParameter, end: &DisplayParameter, progress: f32) -> DisplayParameter {
//     if progress <= 0.0 {
//         return start.clone();
//     } else if progress >= 1.0 {
//         return end.clone();
//     }
//     let mut display_parameter = end.clone();
//     display_parameter.width = start.width + (end.width - start.width) * progress;
//     display_parameter.height = start.height + (end.height - start.height) * progress;
//     display_parameter.relative_x = start.relative_x + (end.relative_x - start.relative_x) * progress;
//     display_parameter.relative_y = start.relative_y + (end.relative_y - start.relative_y) * progress;
//     for (key, value) in &start.float_params {
//         let end_value = end.float_params.get(key);
//         if let Some(end_value) = end_value {
//             display_parameter.float_params.insert(key.clone(), value + (end_value - value) * progress);
//         } else {
//             display_parameter.float_params.insert(key.clone(), *value);
//         }
//     }
//     for (key, value) in &start.color_params {
//         let end_value = end.color_params.get(key);
//         if let Some(end_value) = end_value {
//             let interpolated_color = blend_cam16ucs(color_to_argb(value), color_to_argb(end_value), progress as f64);
//             display_parameter.color_params.insert(key.clone(), Color::from(interpolated_color));
//         } else {
//             display_parameter.color_params.insert(key.clone(), *value);
//         }
//     }
//     display_parameter