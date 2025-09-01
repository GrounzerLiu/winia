use crate::Children;
use clonelet::clone;
use winia::children;
use winia::shared::{Settable, SharedF32};
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::app::WindowContext;
use winia::ui::component::RectangleExt;
use winia::ui::layout::{FlexExt, FlexWrap, StackExt};

pub fn blur_test(w: &WindowContext) -> Item {
    let margin_start = SharedF32::from_static(0.0);
    let margin_top = SharedF32::from_static(0.0);
    w.stack(children!(
        w.flex(children!(
            w.rectangle(Color::RED).item().size(100, 100),
            w.rectangle(Color::GREEN).item().size(100, 100),
            w.rectangle(Color::BLUE).item().size(100, 100),
            w.rectangle(Color::YELLOW).item().size(100, 100),
            w.rectangle(Color::CYAN).item().size(100, 100),
            w.rectangle(Color::MAGENTA).item().size(100, 100),
            w.rectangle(Color::WHITE).item().size(100, 100),
            w.rectangle(Color::BLACK).item().size(100, 100),
            w.rectangle(Color::GRAY).item().size(100, 100),
            w.rectangle(Color::GREEN).item().size(100, 100),
            w.rectangle(Color::BLUE).item().size(100, 100),
            w.rectangle(Color::YELLOW).item().size(100, 100),
            w.rectangle(Color::CYAN).item().size(100, 100),
            w.rectangle(Color::MAGENTA).item().size(100, 100),
            w.rectangle(Color::WHITE).item().size(100, 100),
            w.rectangle(Color::BLACK).item().size(100, 100),
            w.rectangle(Color::GRAY).item().size(100, 100)
        ))
        .wrap(FlexWrap::Wrap)
        .item(),
        w.rectangle(Color::from_argb(100, 255, 255, 255))
            .item()
            .size(100, 100)
            .enable_background_blur(true)
            .margin_start(&margin_start)
            .margin_top(&margin_top)
    ))
    .item()
    .on_cursor_move({
        clone!(margin_start, margin_top);
        move |x, y| {
            margin_start.set(x - 50.0);
            margin_top.set(y - 50.0);
        }
    })
}
