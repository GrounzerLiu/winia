use clonelet::clone;
use std::time::Duration;
use winia::exclude_target;
use winia::shared::{Gettable, Settable, Shared, SharedColor, SharedF32, SharedText};
use winia::skia_safe::Color;
use winia::text::TextStyle;
use winia::ui::Item;
use winia::ui::animation::AnimationExt;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, FilledTextFieldExt, RectangleExt, TextExt};
use winia::ui::item::{Alignment, Size};
use winia::ui::layout::ColumnExt;

pub fn text_test(w: &WindowContext) -> Item {
    let text = SharedText::from("A simple text");
    text.lock().set_style(TextStyle::Bold, 2..8, true);
    text.lock().set_style(TextStyle::Italic, 0..2, true);
    text.lock().set_style(TextStyle::Underline, 0..2, true);
    text.lock().set_style(TextStyle::TextColor(Color::from_rgb(255, 0, 0)), 0..2, true);
    let state = Shared::from_static(false);
    let color: SharedColor = Color::WHITE.into();
    let size: SharedF32 = 16.0.into();
    w.column(
        /*        w.button("Change color").item().on_click({
            clone!(w, color, size, state, text);
            move |_| {
                w.animate(exclude_target!())
                    .transformation({
                        clone!(color, size, state, text);
                        move ||{
                            if state.get() {
                                state.set(false);
                                // text.set("A simple text");
                                color.set(Color::WHITE);
                                // size.set(16.0);
                            } else {
                                state.set(true);
                                // text.set("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut");
                                color.set(Color::from_rgb(255, 0, 255));
                                // size.set(32.0);
                            }
                        }
                    })
                    .duration(Duration::from_millis(500))
                    .start();
            }
        })
        + w.text(text)
            // .editable(false)
            .color(color)
            .font_size(size)
            .item()
            .padding(16)
            .align_content(Alignment::Center)
            .size(300, Size::Auto)
            .background(w.rectangle(Color::TRANSPARENT).outline_color(Color::WHITE).outline_width(4).item())
        +*/
        w.text(&text).item().margin_top(36)
            + w.filled_text_field("").item().margin(16)
            + w.filled_text_field("").item().margin(16),
    )
    .item()
}
