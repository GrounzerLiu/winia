use crate::shared::Shared;
use crate::ui::item::Gravity;

pub type SharedGravity = Shared<Gravity>;

impl Into<(SharedGravity, SharedGravity)> for Gravity {
    fn into(self) -> (SharedGravity, SharedGravity) {
        let gravity = Shared::from_static(self);
        (gravity.clone(),gravity)
    }
}

