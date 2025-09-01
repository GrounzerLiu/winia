use crate::Children;
use winia::icon::Outlined;
use winia::icon::{Rounded, Sharp};
use winia::shared::{Settable, SharedF32};
use winia::skia_safe::Color;
use winia::ui::app::WindowContext;
use winia::ui::component::{IconExt, SliderExt};
use winia::ui::item::Size;
use winia::ui::layout::ColumnExt;
use winia::ui::Item;
use winia::children;

pub fn icon_test(w: &WindowContext) -> Item {
    let fill = SharedF32::from_static(0.0);
    let weight = SharedF32::from_static(400.0);
    let grade = SharedF32::from_static(0.0);
    let optical_size = SharedF32::from_static(20.0);
    w.column(children!(
        w.icon(Outlined::FACE, Color::BLACK)
            .fill(&fill)
            .weight(&weight)
            .grade(&grade)
            .optical_size(&optical_size)
            .item()
            .size(96, 96),
        w.icon(Rounded::FACE, Color::BLACK)
            .fill(&fill)
            .weight(&weight)
            .grade(&grade)
            .optical_size(&optical_size)
            .item()
            .size(96, 96),
        w.icon(Sharp::FACE, Color::BLACK)
            .fill(&fill)
            .weight(&weight)
            .grade(&grade)
            .optical_size(&optical_size)
            .item()
            .size(96, 96),
        w.slider(0.0, 1.0, &fill, {
            let fill = fill.clone();
            move |v| {
                fill.set(v);
            }
        })
        .item()
        .width(Size::Fill),
        w.slider(100.0, 700.0, &weight, {
            let weight = weight.clone();
            move |v| {
                weight.set(v);
            }
        })
        .item()
        .width(Size::Fill),
        w.slider(-25.0, 200.0, &grade, {
            let grade = grade.clone();
            move |v| {
                grade.set(v);
            }
        })
        .item()
        .width(Size::Fill),
        w.slider(20.0, 48.0, &optical_size, {
            let optical_size = optical_size.clone();
            move |v| {
                optical_size.set(v);
            }
        })
        .item()
        .width(Size::Fill),
    ))
    .item()
}
