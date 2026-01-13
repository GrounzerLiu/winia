use winia::app::WindowContext;
use winia::clone;
use winia::ui::{button, flex, label, rectangle, Color, ColumnPropsTrait, Item, RectanglePropsTrait};
use winia::ui::button::ButtonPropsTrait;

pub fn button_test(w: &WindowContext) -> Item {
    let w = w.clone();
    let m = w.window_attributes().get_maximized().clone();
    flex(
        w.column_props(),
        button(
            w.button_props()
                .on_click(
                    use |_| {
                        w.event_loop_proxy().add_layer(use|w,ctr|{
                            rectangle(
                                w.rectangle_props(Color::RED)
                                    .size(100.0,100.0)
                                    .offset(200.0,200.0)
                                    .on_click(
                                        use|_| {
                                            ctr.remove();
                                        }
                                    )
                            )
                        });
                    }
                ),
            |props,_| {
                label(
                    props.text("Button")
                )
            }
        ),
    )
}