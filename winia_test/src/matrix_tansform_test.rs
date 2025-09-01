use crate::Children;
use clonelet::clone;
use std::time::Duration;
use winia::shared::{Settable, SharedF32};
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::animation::AnimationExt;
use winia::ui::animation::interpolator::Linear;
use winia::ui::app::WindowContext;
use winia::ui::component::RectangleExt;
use winia::ui::item::InnerPosition;
use winia::ui::layout::{ColumnExt, RowExt, ScrollAreaExt};
use winia::{children, exclude_target};

pub fn matrix_transform_test(w: &WindowContext) -> Item {
    let scale = SharedF32::from_static(1.0);
    let rotation = SharedF32::from_static(0.0);
    w.scroll_area(
        w.column(children!(
            w.row(children!(
                w.rectangle(Color::RED)
                    .item()
                    .opacity(0.0)
                    .size(100.0, 100.0),
                w.rectangle(Color::RED)
                    .item()
                    .opacity(0.3)
                    .size(100.0, 100.0),
                w.rectangle(Color::RED)
                    .item()
                    .opacity(0.6)
                    .size(100.0, 100.0) // + w.rectangle(Color::RED).item().opacity(1.0).scale(1.5, 1.5).size(100.0, 100.0)
            ))
            .item(),
            w.row(children!(
                w.rectangle(Color::RED).item().size(100.0, 100.0),
                w.rectangle(Color::RED)
                    .item()
                    .opacity(0.3)
                    .size(100.0, 100.0),
                w.rectangle(Color::RED)
                    .item()
                    .opacity(0.6)
                    .scale(1.5, 1.5)
                    .size(100.0, 100.0), // + w.rectangle(Color::RED).item().skew_x(-0.5).skew_y(0.5).skew_center_x(InnerPosition::Start(0.0)).skew_center_y(InnerPosition::Start(0.0))/*.rotation(45)*//*.scale(1.5, 1.5)*/.rotation_center(InnerPosition::End(0.0),InnerPosition::End(0.0)).size(100.0, 100.0)
                w.rectangle(Color::RED)
                    .item()
                    .size(100, 100)
                    .scale(&scale, &scale)
                    .rotation(&rotation)
                    .on_click({
                        let mut flag = true;
                        clone!(w, scale, rotation);
                        move |_| {
                            if flag {
                                w.animate(exclude_target!())
                                    .transformation({
                                        clone!(scale, rotation);
                                        move || {
                                            scale.set(3.5);
                                            rotation.set(225.0);
                                        }
                                    })
                                    .interpolator(Linear::new())
                                    .duration(Duration::from_millis(500))
                                    .start();
                            } else {
                                w.animate(exclude_target!())
                                    .transformation({
                                        clone!(scale, rotation);
                                        move || {
                                            scale.set(1.0);
                                            rotation.set(0.0);
                                        }
                                    })
                                    .interpolator(Linear::new())
                                    .duration(Duration::from_millis(500))
                                    .start();
                            }
                            flag = !flag
                        }
                    })
            ))
            .item()
        ))
        .item(),
    )
    .item()
}
