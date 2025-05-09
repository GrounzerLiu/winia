use winia::core::get_id_by_name;
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, ButtonType, RectangleExt, ToastExt};
use winia::ui::item::Alignment;
use winia::ui::layout::{AlignSelf, ColumnExt};

pub fn layer_test(w: &WindowContext) -> Item {
    w.column(
        w.button("Show").item().on_click({
            let w = w.clone();
            move |_| {
                w.event_loop_proxy().new_layer(|w, controller| {
                    w.column(
                        w.button("Close")
                            .button_type(ButtonType::Elevated)
                            .item()
                            .on_click({
                                let controller = controller.clone();
                                move |_| {
                                    controller.remove();
                                }
                            }),
                    )
                    .item()
                    .background(w.rectangle(Color::GRAY).radius(16).item())
                    .size(300, 200)
                    .align_self(Alignment::Center)
                });
            }
        }) + w.button("Toast").item().on_click({
            let w = w.clone();
            move |_| {
                w.toast("Hello World");
            }
        }),
    )
    .item()
    .padding(16)
}
