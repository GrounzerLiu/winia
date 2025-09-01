use std::time::Duration;
use crate::Children;
use winia::children;
use winia::core::get_id_by_name;
use winia::shared::SharedF32;
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, ButtonType, RectangleExt, ToastExt};
use winia::ui::item::{Alignment, Size};
use winia::ui::layout::{AlignItems, AlignSelf, ColumnExt, JustifyContent, StackExt};

pub fn layer_test(w: &WindowContext) -> Item {
    w.column(children!(
        w.button("Show").item().on_click({
            let w = w.clone();
            move |_| {
                w.event_loop_proxy().new_layer(|w, controller| {
                    let blur = SharedF32::from_static(0.0);
                    blur.animation_to_f32(5.0).duration(Duration::from_millis(500)).start(w.event_loop_proxy());
                    w.column(
                        w.stack(w.button("Close").item().on_click({
                            let controller = controller.clone();
                            move |_| {
                                controller.remove();
                            }
                        }))
                        .item()
                        .size(400, 400),
                    )
                    .justify_content(JustifyContent::Center)
                    .align_items(AlignItems::Center)
                    .item()
                    .size(Size::Fill, Size::Fill)
                    .enable_background_blur(true)
                    .blur(&blur)
                    .align_self(Alignment::Center)
                });
            }
        }),
        w.button("Toast").item().on_click({
            let w = w.clone();
            move |_| {
                w.toast("Hello World");
            }
        }),
    ))
    .item()
    .padding(16)
}
