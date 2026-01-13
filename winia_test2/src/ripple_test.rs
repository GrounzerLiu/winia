use winia::app::WindowContext;
use winia::ui::{ripple, RippleProps, Item, flex, ColumnPropsTrait, rectangle, RectanglePropsTrait, Color, RipplePropsTrait};

pub fn ripple_test(w: &WindowContext) -> Item {
    flex(
        w.column_props(),
        vec![
            rectangle(
                w.rectangle_props(Color::TRANSPARENT)
                 .size(100.0, 100.0)
                 .background(
                     ripple(w.ripple_props())
                 )
            )
        ],
    )
}