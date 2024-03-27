use crate::ui::Gravity;
use crate::property::SharedProperty;

pub type GravityProperty = SharedProperty<Gravity>;

impl Into<(GravityProperty,GravityProperty)> for Gravity {
    fn into(self) -> (GravityProperty,GravityProperty) {
        let gravity = SharedProperty::from_value(self);
        (gravity.clone(),gravity)
    }
}