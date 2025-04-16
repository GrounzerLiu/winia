use winia::skia_safe::Color;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::RectangleExt;
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::{FlexExt, ScrollAreaExt};

pub fn flex_test(w: &WindowContext, _attr: &WindowAttr) -> Item {
    w.scroll_area(
        w.flex(
            w.rectangle(Color::RED).item().size(Size::Fill,100)
        ).item().padding(16)
    ).item()
}