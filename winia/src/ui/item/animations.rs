use std::collections::HashMap;
use crate::animation::LayoutAnimation;
use crate::ui::Color;

pub type AnimationOption = Option<(f32, f32, LayoutAnimation)>;
#[derive(Default)]
pub struct Animations {
    // (start, end, animation)
    pub width: AnimationOption,
    pub height: AnimationOption,
    pub relative_x: AnimationOption,
    pub relative_y: AnimationOption,
    pub offset_x: AnimationOption,
    pub offset_y: AnimationOption,
    pub opacity: AnimationOption,
    pub rotation: AnimationOption,
    pub rotation_center_x: AnimationOption,
    pub rotation_center_y: AnimationOption,
    pub scale_x: AnimationOption,
    pub scale_y: AnimationOption,
    pub scale_center_x: AnimationOption,
    pub scale_center_y: AnimationOption,
    pub skew_x: AnimationOption,
    pub skew_y: AnimationOption,
    pub skew_center_x: AnimationOption,
    pub skew_center_y: AnimationOption,
    pub float_params: HashMap<String, (f32, f32, LayoutAnimation)>,
    pub color_params: HashMap<String, (Color, Color, LayoutAnimation)>,
}

impl Animations {
    pub fn is_animating(&self) -> bool {
        self.width.is_some()
            || self.height.is_some()
            || self.relative_x.is_some()
            || self.relative_y.is_some()
            || self.offset_x.is_some()
            || self.offset_y.is_some()
            || self.opacity.is_some()
            || self.rotation.is_some()
            || self.rotation_center_x.is_some()
            || self.rotation_center_y.is_some()
            || self.scale_x.is_some()
            || self.scale_y.is_some()
            || self.scale_center_x.is_some()
            || self.scale_center_y.is_some()
            || self.skew_x.is_some()
            || self.skew_y.is_some()
            || self.skew_center_x.is_some()
            || self.skew_center_y.is_some()
            || !self.float_params.is_empty()
            || !self.color_params.is_empty()
    }
}
#[macro_export]
macro_rules! override_animation {
    ($animation:ident, $recorded_frame:ident, $target_frame:ident, $self_:ident, $name:ident) => {{
        let recorded = $recorded_frame.$name;
        let target = $target_frame.$name;
        if (recorded-target).abs() > 0.1
            && $self_
                .animations
                .$name
                .as_ref()
                .map_or(true, |(_, end, _)| *end != target)
        {
            $self_.animations.$name = Some((recorded, target, $animation.clone()));
        }
    }};
}
#[macro_export]
macro_rules! override_animations {
    ($animation:ident, $recorded_frame:ident, $target_frame:ident, $self_:ident, $($name:ident),+) => {
        $(
            override_animation!($animation, $recorded_frame, $target_frame, $self_, $name);
        )+
    }
}

#[macro_export]
macro_rules! calculate_animation_value {
    ($name:ident, $s:ident, $display_parameter:ident) => {
        let p = {
            if let Some((start, end, animation)) = &$s.animations.$name {
                Some((start, end, animation.clone()))
            } else {
                None
            }
        };
        if let Some((start, end, animation)) = p {
            if !animation.is_finished() {
                $display_parameter.$name =
                    animation.interpolate_f32(*start, *end);
            } else {
                $display_parameter.$name = *end;
                $s.animations.$name = None;
            }
        }
    };
}
