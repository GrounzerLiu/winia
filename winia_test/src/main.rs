use winia::cpu::SoftSkiaWindow;
use winia::gl::GlSkiaWindow;
use winia::shared::Children;
use winia::skia_safe::Color;
use winia::ui::{App, Item};
use winia::ui::app::{WindowAttr, run_app, WindowContext};
use winia::ui::component::{ButtonExt, ImageExt, RectangleExt, RippleExt, ScaleMode, TextExt};
use winia::ui::component::divider::DividerExt;
use winia::ui::item::{Alignment, Size};
use winia::ui::layout::{ColumnExt, ListExt, ScrollAreaExt, StackExt};
use winia::ui::theme::color;
use winia::vulkan::VulkanSkiaWindow;
use crate::blur_test::blur_test;
use crate::button_test::button_test;
use crate::checkbox_test::checkbox_test;
use crate::flex_test::flex_test;
use crate::focus_test::focus_test;
use crate::image_test::image_test;
use crate::layer_test::layer_test;
use crate::list_test::list_test;
use crate::progress_indicator_test::progress_indicator_test;
use crate::radio_test::radio_test;
use crate::rectangle_test::rectangle_test;
use crate::ripple_test::ripple_test;
use crate::scroll_area_test::scroll_area_test;
use crate::slider_test::slider_test;
use crate::switch_test::switch_test;
use crate::text_test::text_test;

mod image_test;
mod scroll_area_test;
mod list_test;
mod button_test;
mod layer_test;
mod ripple_test;
mod text_test;
mod slider_test;
mod flex_test;
mod rectangle_test;
mod focus_test;
mod blur_test;
mod radio_test;
mod checkbox_test;
mod switch_test;
mod progress_indicator_test;

fn main() {
    run_app(App::new(
        main_ui,
        WindowAttr::default(),
        Some(Box::new(
            |window| {
                Box::new(GlSkiaWindow::new(window, None))
            }
        )),
    ));
}

fn main_ui(w: &WindowContext) -> Item {
    let items: Vec<(fn(&WindowContext) -> Item, &str)> = vec![
        (button_test, "Button"),
        (blur_test, "Blur"),
        (checkbox_test, "Checkbox"),
        (flex_test, "Flex"),
        // (focus_test, "Focus"),
        (image_test, "Image"),
        (layer_test, "Layer"),
        (list_test, "List"),
        (progress_indicator_test, "Progress Indicator"),
        (radio_test, "Radio"),
        (rectangle_test, "Rectangle"),
        (ripple_test, "Ripple"),
        (scroll_area_test, "Scroll Area"),
        (slider_test, "Slider"),
        (switch_test, "Switch"),
        (text_test, "Text"),
    ];
    w.list(
        items,
        |w, items, i| {
            let w_clone = w.clone();
            let items_clone = items.clone();
            let items = items.lock();
            let item = items[i];
            w.column(
                w.text(item.1.to_string())
                    .editable(false)
                    .item().size(Size::Fill, 50)
                    .padding_start(16)
                    .align_content(Alignment::CenterStart)
                    .on_click(move |_|{
                        let items = items_clone.lock();
                        let item_generator = items[i].0;
                        let title = items[i].1;
                        w_clone.event_loop_proxy().new_window(
                            item_generator,
                            WindowAttr::default()
                                .title(title)
                        )
                    })
                    .background(w.ripple().item())
                + w.divider().item().width(Size::Fill)
            ).item()
        }
    ).item()
}

#[cfg(test)]
mod test {
    use winia::shared::{Gettable, Settable, SharedF32};

    #[test]
    fn shared_test() {
        let a = SharedF32::from_static(2.0);
        let b = SharedF32::from_static(2.0);
        a.add_specific_observer(b.id(), {
            let b = b.clone();
            move |a| {
                b.try_set_static(*a);
            }
        });
        b.add_specific_observer(a.id(), {
            let a = a.clone();
            move |b| {
                a.try_set_static(*b);
            }
        });

        a.set(3.0);
        assert_eq!(a.get(), 3.0);
        assert_eq!(b.get(), 3.0);

        b.set(4.0);
        assert_eq!(a.get(), 4.0);
        assert_eq!(b.get(), 4.0);
    }
}
