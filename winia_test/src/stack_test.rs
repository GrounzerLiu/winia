use crate::Children;
use winia::children;
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::RectangleExt;
use winia::ui::layout::StackExt;

pub fn stack_test(w: &WindowContext) -> Item {
    w.stack(children!(
        w.rectangle(Color::RED).item().size(150, 150),
        w.rectangle(Color::BLUE).item().size(100, 100),
        w.rectangle(Color::GREEN).item().size(50, 50)
    ))
    .item()
}
