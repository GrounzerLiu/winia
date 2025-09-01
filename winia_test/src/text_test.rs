use clonelet::clone;
use std::time::Duration;
use winia::{exclude_target, skia_safe};
use winia::shared::{Gettable, Settable, Shared, SharedColor, SharedDrawable, SharedF32, SharedText};
use winia::skia_safe::Color;
use winia::text::{TextStyle, Typeface};
use winia::ui::Item;
use winia::ui::animation::AnimationExt;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, FilledTextFieldExt, RectangleExt, TextExt};
use winia::ui::item::{Alignment, Size};
use winia::ui::layout::{ColumnExt, RowExt};

pub fn text_test(w: &WindowContext) -> Item {
    let text = SharedText::from("");
    {
        let mut text = text.lock();
        text.append_str_with_style("Bold ", TextStyle::Bold, false);
        text.append_str_with_style("Italic ", TextStyle::Italic, false);
        text.append_str_with_style("Underline ", TextStyle::Underline, false);
        text.append_str_with_style("Strikethrough ", TextStyle::Strikethrough, false);
        text.append_str_with_style("FontSize ", TextStyle::FontSize(36.0), false);
        text.append_str_with_style("BackgroundColor ", TextStyle::BackgroundColor(Color::from_rgb(255, 0, 0)), false);
        text.append_str_with_style("TextColor ", TextStyle::TextColor(Color::from_rgb(0, 255, 0)), false);
        text.append_str_with_style("Weight ", TextStyle::Weight(skia_safe::font_style::Weight::from(2000)), false);
        text.append_str_with_style("Tracking ", TextStyle::Tracking(5.0), false);
        text.append_str_with_style("Subscript ", TextStyle::Subscript, false);
        text.append_str_with_style("Superscript ", TextStyle::Superscript, false);
        text.append_str_with_style("Typeface ", TextStyle::Typeface(Typeface::Family("Courier New".to_string())), false);
        text.append_str_with_style("Image ", TextStyle::Image(
            {
                let image = SharedDrawable::from_file("/home/grounzer/Downloads/check_box_selected.svg").unwrap();
                image.lock().set_color(Some(Color::BLUE));
                image
            }
        ), false);
    }
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
        w.text(&text).item().margin_top(36).width(Size::Fill)
        //     w.row(
        //     w.text("Text with style Text with style").item()
        // ).item()
            + w.filled_text_field(&text).item().margin(16)
            + w.filled_text_field("").item().margin(16),
    )
    .item()
}
