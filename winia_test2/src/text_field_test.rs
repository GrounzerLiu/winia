use winia::app::WindowContext;
use winia::clone;
use winia::shared::{SharedDrawable, SharedText};
use winia::text::TextAttribute;
use winia::ui::{flex, text_field, Color, ColumnPropsTrait, Item, Size, TextFieldPropsTrait};

pub fn text_field_test(w: &WindowContext) -> Item {
    // let lorem_ipsum = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
    // let lorem_ipsum = "संस्कृत भाषा सुंदर है";
    let lorem_ipsum = "AB";
    let text = SharedText::from(lorem_ipsum);
    // text.lock().append_str(" Additional text.");
    text.lock().append_str_with_attr(
        "CDEF",
        TextAttribute::Placeholder(SharedDrawable::from_file("/home/grounzer/Downloads/ttt.png").unwrap()),
        false
    );
    text.lock().append_str("G我H");
    flex(
        w.column_props(),
        vec![
            text_field(
                w.text_field_props(
                    &text,
                    {
                        clone!(text);
                        move |text_change| {
                            text_change.apply_to(&text);
                        }
                    }
                )
                    .clipped(false)
                 .color(Color::GREEN)
                 .width(Size::Auto)
            ),
        ],
    )
}

