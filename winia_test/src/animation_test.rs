use std::time::Duration;
use clonelet::clone;
use winia::exclude_target;
use winia::shared::{Gettable, Settable, Shared};
use winia::skia_safe::Color;
use winia::ui::animation::AnimationExt;
use winia::ui::animation::interpolator::Linear;
use winia::ui::app::WindowContext;
use winia::ui::component::RectangleExt;
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::StackExt;

pub fn animation_test(w: &WindowContext) -> Item {
    let color = Shared::from_static(Color::RED);
    let offset = Shared::from_static(0.0);
    let size = Shared::from_static(Size::Fixed(50.0));
    
    let radius = Shared::from_static(0.0);
    let state = Shared::from_static(false);
    w.stack(
        w.rectangle(&color)
            .radius(&radius)
            .item()
            .offset_x(&offset)
            .offset_y(&offset)
            .size(&size, &size)
            .on_click({
                clone!(w, color, offset, size, radius, state);
                move |_| {
                    w.animate(exclude_target!()).transformation({
                        clone!(color, offset, size, radius, state);
                        move || {
                            if state.get() {
                                state.set(false);
                                color.set(Color::RED);
                                offset.set(0.0);
                                size.set(Size::Fixed(50.0));
                                radius.set(0.0);
                            } else {
                                state.set(true);
                                color.set(Color::BLUE);
                                offset.set(100.0);
                                size.set(Size::Fixed(100.0));
                                radius.set(50.0);
                            }
                        }
                    }).interpolator(Linear::new()).duration(Duration::from_millis(500)).start()
                }
            })
    ).item()
}