use letclone::clone;
use crate::Radius;
use winia::ui::RectanglePropsTrait;
use winia::ui::Color;
use crate::rectangle;
use winia::app::WindowContext;
use winia::core::next_id;
use winia::shared::SharedSource;
use winia::ui::{flex, slider, stack, ColumnPropsTrait, Item, SliderPropsTrait, StackPropsTrait};

pub fn slider_test(w: &WindowContext) -> Item {
    let value1 = SharedSource::new(50.0);
    let value2 = SharedSource::new(25.0);
    value1.subscribe(
        next_id(),
        {
            let value1 = value1.clone();
            move || {
                println!("Slider 1 Value: {}", value1.get());
            }
        }
    );
    flex(
        w.column_props(),
        vec![
            slider(
                w.slider_props(
                    &value1,
                    0.0,
                    100.0,
                    {
                        clone!(value1);
                        move |value| {
                            value1.set(value);
                        }
                    }
                )
            ),
            slider(
                w.slider_props(
                    &value2,
                    0.0,
                    50.0,
                    {
                        clone!(value2);
                        move |value| {
                            value2.set(value);
                        }
                    }
                )
                .background(
                    rectangle(
                        w.rectangle_props(Color::GRAY)
                        .radius(
                            Radius::new().all(0.0)
                        )
                    )
                )
            ),
        ]
    )
}