use winia::app::WindowContext;
use winia::ui::{flex, rectangle, BorderPosition, Color, ColumnPropsTrait, Item, Radius, RectanglePropsTrait, Size};

pub fn rectangle_test(w: &WindowContext) -> Item {
    flex(
        w.column_props()
         .size(Size::Fill, Size::Fill),
        vec![
            rectangle(
                w.rectangle_props(Color::RED)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
            ),
            rectangle(
                w.rectangle_props(Color::GREEN)
                 .radius(Radius::new().all(16))
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
            ),
            rectangle(
                w.rectangle_props(Color::BLUE)
                 .clipped(false)
                 .radius(Radius::new().all(16))
                 .is_filled(false)
                 .border_width(16)
                 .border_position(BorderPosition::Outside)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
            ),
            rectangle(
                w.rectangle_props(Color::YELLOW)
                 .clipped(true)
                 .radius(Radius::new().all(16))
                 .is_filled(false)
                 .border_width(16)
                 .border_position(BorderPosition::Inside)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
            ),
            rectangle(
                w.rectangle_props(Color::ORANGE)
                 .clipped(false)
                 .radius(Radius::new().all(50))
                 .is_filled(false)
                 .border_width(16)
                 .border_position(BorderPosition::Center)
                 .size(Size::Fixed(100.0), Size::Fixed(100.0))
            ),
        ],
    )
}