use std::sync::{Arc, Mutex};
use skia_safe::{Color, Rect};
use crate::core::RefClone;
use crate::property::{Children, ColorProperty, Gettable};
use crate::ui::app::AppContext;
use crate::ui::Item;
use crate::ui::item::{InnerPosition, ItemEvent, MeasureMode, Orientation};

struct RectangleProperty {
    color: ColorProperty,
}
impl RefClone for RectangleProperty {
    fn ref_clone(&self) -> Self {
        Self {
            color: self.color.ref_clone()
        }
    }
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
            .measure(|item, orientation, mode| {
                let min_size = item.get_min_size(orientation);
                let max_size = item.get_max_size(orientation);
                let measure_parameter = item.get_measure_parameter();
                match mode {
                    MeasureMode::Specified(size) => {
                        measure_parameter.set_size(orientation, size.clamp(min_size, max_size));
                    }
                    MeasureMode::Unspecified(_) => {
                        measure_parameter.set_size(orientation, min_size);
                    }
                }
            })
            .layout({
                move |item, relative_x, relative_y, width, height| {
                    let horizontal_padding = item.get_padding(Orientation::Horizontal);
                    let vertical_padding = item.get_padding(Orientation::Vertical);
                    item.layout_layers(width - horizontal_padding, height - vertical_padding);
                }
            })
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
                    let opacity = item.get_opacity().get();
                    let rect = Rect::from_xywh(
                        display_parameter.x() + padding_left,
                        display_parameter.y() + padding_top,
                        display_parameter.width() - padding_left - padding_right,
                        display_parameter.height() - padding_top - padding_bottom,
                    );
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color(color.with_a((opacity.clamp(0.0, 1.0) * 255.0) as u8));
                    canvas.draw_rect(rect, &paint);
                }
            });
        let item = Item::new(app_context, Children::new(), item_event);
        Self {
            item,
            rectangle_property,
        }
    }

    pub fn color(self, color: impl Into<ColorProperty>) -> Self {
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
        Rectangle::new(self.ref_clone())
    }
}