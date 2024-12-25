/*use skia_safe::Color;
use proc_macro::RefClone;
use crate::shared::{Shared, SharedColor};
use crate::ui::app::AppContext;

#[derive(RefClone)]
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
*/