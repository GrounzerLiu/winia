use crate::shared::{Children, Gettable, Shared, SharedColor};
use crate::ui::app::AppContext;
use crate::ui::Item;
use proc_macro::item;
use skia_safe::Rect;

#[derive(Clone)]
struct RectangleProperty {
    color: SharedColor,
}

#[item(color: impl Into<SharedColor>)]
pub struct Rectangle {
    item: Item,
    property: Shared<RectangleProperty>,
}

impl Rectangle {
    pub fn new(app_context: AppContext, color: impl Into<SharedColor>) -> Self {
        let property = Shared::new(RectangleProperty {
            color: color.into(),
        });

        let item = Item::new(app_context, Children::new());
        item.data().set_draw({
            let property = property.clone();
            move |item, canvas| {
                let rectangle_property = property.value();
                let display_parameter = item.get_display_parameter().clone();
                let padding_left = item.get_padding_left();
                let padding_top = item.get_padding_top().get();
                let padding_right = item.get_padding_right();
                let padding_bottom = item.get_padding_bottom().get();

                let color = rectangle_property.color.get();
                let rect = Rect::from_xywh(
                    display_parameter.x() + padding_left,
                    display_parameter.y() + padding_top,
                    display_parameter.width - padding_left - padding_right,
                    display_parameter.height - padding_top - padding_bottom,
                );
                let mut paint = skia_safe::Paint::default();
                paint.set_anti_alias(true);
                paint.set_color(color);
                canvas.draw_rect(rect, &paint);
            }
        });
        Self {
            item,
            property,
        }
    }

    pub fn color(self, color: impl Into<SharedColor>) -> Self {
        let mut rectangle_property = self.property.value();
        rectangle_property.color = color.into();
        drop(rectangle_property);
        self
    }
}
