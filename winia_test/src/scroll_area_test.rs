use clonelet::clone;
use winia::children;
use winia::core::next_id;
use winia::shared::{Children, Settable, Shared};
use winia::skia_safe::gradient_shader::GradientShaderColors;
use winia::skia_safe::{Color, Shader, TileMode};
use winia::ui::Item;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ButtonExt, RectangleExt};
use winia::ui::item::Size;
use winia::ui::layout::{ColumnExt, FlexGrow, ScrollAreaExt, StackExt};

pub fn scroll_area_test(w: &WindowContext) -> Item {
    let scroll_position = Shared::from_static((0.0_f32, 0.0_f32));
    scroll_position.add_specific_observer(next_id(), |(x_position, y_position)| {
        // println!("scroll_position: {:?}", (x_position, y_position));
    });
    w.column(children!(
        w.button("Top").item().on_click({
            clone!(scroll_position);
            move |_| {
                scroll_position.set((0.0, 0.0));
            }
        }),
        w.button("Bottom").item().on_click({
            clone!(scroll_position);
            move |_| {
                scroll_position.set((0.0, 1.0));
            }
        }),
        w.scroll_area(
            w.column(children!(
                w.rectangle(Color::RED).item().size(Size::Fill, 100),
                w.rectangle(Color::BLUE).item().size(Size::Fill, 100),
                w.rectangle(Color::GREEN).item().size(Size::Fill, 100),
                w.rectangle(Color::YELLOW).item().size(Size::Fill, 100),
                w.scroll_area(
                    w.column(children!(
                        w.rectangle(Color::RED).item().size(Size::Fill, 50),
                        w.rectangle(Color::BLUE).item().size(Size::Fill, 50),
                        w.rectangle(Color::GREEN).item().size(Size::Fill, 50),
                        w.rectangle(Color::YELLOW).item().size(Size::Fill, 50),
                        w.rectangle(Color::WHITE).item().size(Size::Fill, 50),
                        w.rectangle(Color::BLACK).item().size(Size::Fill, 50),
                        w.rectangle(Color::GRAY).item().size(Size::Fill, 50),
                        w.rectangle(Color::CYAN).item().size(Size::Fill, 50),
                        w.rectangle(Color::MAGENTA).item().size(Size::Fill, 50),
                    ))
                    .item()
                    .size(2000, Size::Auto),
                )
                // .scroll_position(&scroll_position)
                .horizontal_scrollable(true)
                .vertical_scrollable(true)
                .item()
                .size(800, 300)
                .name("sa")
                .foreground(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(Color::RED)
                        .outline_width(2.0)
                        .item()
                ),
                w.rectangle(Color::WHITE).item().size(Size::Fill, 100),
                w.rectangle(Color::BLACK).item().size(Size::Fill, 100),
                w.rectangle(Color::GRAY).item().size(Size::Fill, 100),
                w.rectangle(Color::CYAN).item().size(Size::Fill, 100),
                w.rectangle(Color::MAGENTA).item().size(Size::Fill, 100),
            ))
            .item()
            .size(2000, Size::Auto)
            .name("pc"),
        )
        .scroll_position(&scroll_position)
        .horizontal_scrollable(true)
        .vertical_scrollable(true)
        .item()
        .height(Size::Fill)
        .name("psa"), // .height(200)
                      // .flex_grow(1)
    ))
    .item()
    .name("root")
    // .padding_start(200.0)
    // .padding_top(200.0)
}
