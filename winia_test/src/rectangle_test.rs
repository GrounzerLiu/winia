use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{RectangleExt, TextExt};
use winia::ui::layout::{ColumnExt, ScrollAreaExt};

pub fn rectangle_test(w: &WindowContext) -> Item {
    w.scroll_area(
        w.column(
            w.text("Rectangle").font_size(16).item()
            + w.rectangle(Color::RED)
                .item()
                .size(100, 32)
                .margin_bottom(16)
            + w.text("Rectangle with outline").item()
            + w.rectangle(Color::RED)
                .outline_width(4)
                .outline_color(Color::BLUE)
                .item()
                .size(100, 100)
                .margin_bottom(16)
            + w.text("Rectangle with shadow").item()
            + w.rectangle(Color::RED).item().size(100, 100).elevation(2)
            + w.text("Rectangle with rounded corners").item()
            + w.rectangle(Color::RED)
                .radius(10)
                .item()
                .size(100, 100)
                .margin_bottom(16)
        ).item().padding_start(16),
    )
    .item()
}
