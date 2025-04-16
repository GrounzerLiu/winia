use winia::core::get_id_by_name;
use winia::skia_safe::Color;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, ButtonType, RectangleExt, ToastExt};
use winia::ui::Item;
use winia::ui::item::Alignment;
use winia::ui::layout::{AlignSelf, ColumnExt};

pub fn layer_test(w: &WindowContext, _: &WindowAttr) -> Item {
    w.column(
        w.button("Show").item()
            .on_click({
                let w = w.clone();
                move |_| {
                    w.event_loop_proxy().new_layer(
                      |w, _attr| {
                          w.column(
                              w.button("Close").button_type(ButtonType::Elevated).item()
                                  .on_click({
                                        let w = w.clone();
                                        move |_| {
                                            w.event_loop_proxy().remove_layer(get_id_by_name("layer").unwrap())
                                        }
                                  })
                          ).background(
                              w.rectangle(Color::GRAY).radius(16).item()
                          ).size(300, 200)
                              .name("layer")
                              .align_self(Alignment::Center)
                      }
                    );
                }
            })
        + w.button("Toast").item()
            .on_click({
                let w = w.clone();
                move |_| {
                    w.toast("Hello World");
                }
            })
    ).padding(16)
}