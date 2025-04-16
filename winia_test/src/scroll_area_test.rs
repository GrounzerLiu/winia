use winia::shared::Children;
use winia::skia_safe::gradient_shader::GradientShaderColors;
use winia::skia_safe::{Color, Shader, TileMode};
use winia::ui::Item;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::RectangleExt;
use winia::ui::item::Size;
use winia::ui::layout::{ColumnExt, ScrollAreaExt, StackExt};

pub fn scroll_area_test(w: &WindowContext, _: &WindowAttr) -> Item {
    w.scroll_area(
        Children::new()
            + w.column(
            Children::new()
                    + w.rectangle(Color::RED).item().size(Size::Fill, 100)
                    + w.rectangle(Color::BLUE).item().size(Size::Fill, 100)
                    + w.rectangle(Color::GREEN).item().size(Size::Fill, 100)
                    + w.rectangle(Color::YELLOW).item().size(Size::Fill, 100)
                    + w.rectangle(Color::WHITE).item().size(Size::Fill, 100)
                    + w.rectangle(Color::BLACK).item().size(Size::Fill, 100)
                    + w.rectangle(Color::GRAY).item().size(Size::Fill, 100)
                    + w.rectangle(Color::CYAN).item().size(Size::Fill, 100)
                    + w.rectangle(Color::MAGENTA).item().size(Size::Fill, 100),
            )
            .size(2000, Size::Auto),
    )
        .horizontal_scrollable(true)
        .vertical_scrollable(true)
        .item()
    // .padding_start(200.0)
    // .padding_top(200.0)
}
