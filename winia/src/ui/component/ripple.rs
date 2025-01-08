use skia_safe::Color;
use crate::shared::{Shared, SharedColor};
use crate::ui::app::AppContext;

#[derive(Clone)]
struct RippleProperty {
    color: SharedColor
}

pub struct Ripple {
    property: Shared<RippleProperty>
}

impl Ripple{
    pub fn new(app_context: AppContext) -> Self {
        let property = Shared::new(RippleProperty {
            color: Color::TRANSPARENT.into()
        });

    }
}
