use skia_safe::Color;
use crate::shared::{Shared, SharedColor, SharedF32};
use crate::ui::app::AppContext;
use crate::ui::item::ItemEvent;

#[derive(Clone)]
struct RippleProperty {
    foreground_color: SharedColor,
    foreground_opacity: SharedF32,
    background_color: SharedColor,
    background_opacity: SharedF32,
}

pub struct Ripple {
    property: Shared<RippleProperty>
}

impl Ripple{
    // pub fn new(app_context: AppContext) -> Self {
    //     let property = Shared::new(RippleProperty {
    //         foreground_color: Color::TRANSPARENT.into(),
    //         foreground_opacity: 0.0.into(),
    //         background_color: Color::TRANSPARENT.into(),
    //         background_opacity: 0.0.into(),
    //     });
    //
    //     let item_event = ItemEvent::new()
    //         .draw(move |item, canvas|{
    //
    //         })
    //
    // }
}
