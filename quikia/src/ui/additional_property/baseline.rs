use crate::ui::{AdditionalProperty, Item};

pub trait BaseLine {
    fn get_baseline(&self) -> Option<f32>;
    fn set_baseline(&mut self, baseline: f32);
}

impl BaseLine for Item {
    fn get_baseline(&self) -> Option<f32> {
        if let Some(value) = self.get_additional_property("baseline") {
            if let AdditionalProperty::F32(value) = value {
                Some(*value)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn set_baseline(&mut self, baseline: f32) {
        self.set_additional_property("baseline", AdditionalProperty::F32(baseline));
    }
}