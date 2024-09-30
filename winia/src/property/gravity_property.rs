use crate::core::RefClone;
use crate::property::Property;
use crate::ui::item::Gravity;

pub type GravityProperty = Property<Gravity>;

impl Into<(GravityProperty,GravityProperty)> for Gravity {
    fn into(self) -> (GravityProperty,GravityProperty) {
        let gravity = Property::from_static(self);
        (gravity.ref_clone(),gravity)
    }
}

