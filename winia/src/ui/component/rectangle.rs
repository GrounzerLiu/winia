use crate::shared::{Children, Gettable, SharedColor};
use crate::ui::app::AppContext;
use crate::ui::item::{ItemEvent, Orientation};
use crate::ui::Item;
use skia_safe::{Color, Rect};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct RectangleProperty {
    color: SharedColor,
}
pub struct Rectangle {
    item: Item,
    rectangle_property: Arc<Mutex<RectangleProperty>>,
}

impl Rectangle {
    pub fn new(app_context: AppContext) -> Self {
        let rectangle_property = Arc::new(Mutex::new(RectangleProperty {
            color: Color::TRANSPARENT.into(),
        }));
        let item_event = ItemEvent::new()
            .draw({
                let rectangle_property = rectangle_property.clone();
                move |item, canvas| {
                    let rectangle_property = rectangle_property.lock().unwrap();
                    let display_parameter = item.get_display_parameter().clone();
                    let padding_left = item.get_padding_left();
                    let padding_top = item.get_padding_top().get();
                    let padding_right = item.get_padding_right();
                    let padding_bottom = item.get_padding_bottom().get();

                    let color = rectangle_property.color.get();
                    let opacity = item.get_opacity().get().clamp(0.0, 1.0);
                    let rect = Rect::from_xywh(
                        display_parameter.x() + padding_left,
                        display_parameter.y() + padding_top,
                        display_parameter.width - padding_left - padding_right,
                        display_parameter.height - padding_top - padding_bottom,
                    );
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    let alpha = color.a();
                    let new_alpha = (alpha as f32 * opacity) as u8;
                    let new_color = Color::from_argb(new_alpha, color.r(), color.g(), color.b());
                    paint.set_color(new_color);
                    canvas.draw_rect(rect, &paint);
                }
            });
        let item = Item::new(app_context, Children::new(), item_event);
        Self {
            item,
            rectangle_property,
        }
    }

    pub fn color(self, color: impl Into<SharedColor>) -> Self {
        let mut rectangle_property = self.rectangle_property.lock().unwrap();
        rectangle_property.color = color.into();
        drop(rectangle_property);
        self
    }

    pub fn item(self) -> Item {
        self.item
    }
}

pub trait RectangleExt {
    fn rectangle(&self) -> Rectangle;
}

impl RectangleExt for AppContext {
    fn rectangle(&self) -> Rectangle {
        Rectangle::new(self.clone())
    }
}