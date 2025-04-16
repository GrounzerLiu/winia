use winia::shared::{Settable, SharedColor, SharedF32};
use winia::skia_safe::Color;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, TextExt};
use winia::ui::Item;
use winia::ui::layout::ColumnExt;

pub fn text_test(w: &WindowContext, _: &WindowAttr) -> Item {
    let color:SharedColor = Color::BLACK.into();
    let size:SharedF32 = 17.0.into();
    w.column(
        w.button("Change color").item().on_click({
            let color = color.clone();
            let size = size.clone();
            move |_| {
                color.set(Color::RED);
                size.set(23.0);
            }
        })
        + w.text("text")
            .editable(false)
            .color(color)
            .font_size(size)
            .item()
    )
}