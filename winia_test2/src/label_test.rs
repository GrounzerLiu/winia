use winia::app::WindowContext;
use winia::text::{StyledText, TextAttribute};
use winia::ui::{flex, label, scroll_area, Color, ColumnPropsTrait, DefaultScrollAreaProps, Item, LabelPropsTrait};

pub fn label_test(w: &WindowContext) -> Item {
    scroll_area(
        w.vertical_scroll_props(),
        flex(
            w.column_props(),
            vec![
                label(w.label_props("Color")),
                label(w.label_props("Red").color(Color::RED)),
                label(w.label_props("Green").color(Color::GREEN)),
                label(w.label_props("Blue").color(Color::BLUE)),
                label(w.label_props("Font Size")),
                label(w.label_props("Size 12").font_size(12.0)),
                label(w.label_props("Size 16").font_size(16.0)),
                label(w.label_props("Size 20").font_size(20.0)),
                label(w.label_props("Size 24").font_size(24.0)),
                label(w.label_props("Max Lines")),
                label(
                    w.label_props("This is a very long text that should be truncated after two lines.")
                     .max_lines(2)
                     .width(200.0)
                     .ellipsis("...")
                ),
                label(
                    w.label_props("This is a very long text that should be truncated after three lines.")
                     .max_lines(3)
                     .width(200.0)
                     .ellipsis("...")
                ),
                {
                    let long_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                                     Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                                     Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris \
                                     nisi ut aliquip ex ea commodo consequat.";
                    let mut long_text = StyledText::from(long_text);
                    long_text.set_attr(TextAttribute::Color(Color::BLUE), 0..10, false);
                    long_text.set_attr(TextAttribute::FontSize(20.0), 19..56, false);
                    long_text.set_attr(TextAttribute::Color(Color::RED), 57..long_text.len(), false);

                    label(
                        w.label_props(long_text)
                         .width(300.0)
                            .color(Color::WHITE)
                    )
                }
            ],
        ),
    )
}