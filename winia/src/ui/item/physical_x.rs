use crate::ui::item::LayoutDirection;

pub trait PhysicalX {
    fn physical_x(&self, layout_direction: LayoutDirection, parent_width: f32, self_width: f32) -> f32;
}

impl PhysicalX for f32 {
    fn physical_x(&self, layout_direction: LayoutDirection, parent_width: f32, self_width: f32) -> f32 {
        match layout_direction {
            LayoutDirection::LTR => *self,
            LayoutDirection::RTL => parent_width - self_width - *self,
        }
    }
}
