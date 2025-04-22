use std::time::Duration;
use winia::exclude_target;
use winia::shared::{Settable, Shared};
use winia::skia_safe::Color;
use winia::ui::animation::AnimationExt;
use winia::ui::Item;
use winia::ui::app::{EventLoopProxy, WindowAttr, WindowContext};
use winia::ui::component::divider::DividerExt;
use winia::ui::component::{RadioGroupExt, RectangleExt, TextExt};
use winia::ui::item::Size;
use winia::ui::layout::{
    AlignContent, AlignItems, ColumnExt, FlexDirection, FlexExt, FlexGrow, FlexWrap,
    JustifyContent, RowExt, ScrollAreaExt, StackExt,
};
use winia::ui::theme::color;

pub fn flex_test(w: &WindowContext) -> Item {
    fn flex_direction_example(w: &WindowContext) -> Item {
        let outline_color = w.theme().lock().get_color(color::ON_SURFACE).unwrap();
        let direction = Shared::from_static(FlexDirection::Horizontal);
        let wrap = Shared::from_static(FlexWrap::NoWrap);
        let justify_content = Shared::from_static(JustifyContent::Start);
        let align_items = Shared::from_static(AlignItems::Start);
        let align_content = Shared::from_static(AlignContent::Start);
        fn on_selected<T:Clone + Send + 'static>(
            event_loop_proxy: &EventLoopProxy
        ) -> impl FnMut(&Shared<T>, T) + Clone + 'static {
            let event_loop_proxy = event_loop_proxy.clone();
            move |s, v| {
                event_loop_proxy
                    .animate(exclude_target!())
                    .transformation({
                        let s = s.clone();
                        move || {
                            s.set(v.clone());
                        }
                    })
                    .duration(Duration::from_millis(500))
                    .start();
            }
        }
        w.column(
            w.text("FlexDirection").item()
                + w.row(w.radio_group(
                    &direction,
                    &[
                        (FlexDirection::Horizontal, "Horizontal".into()),
                        (FlexDirection::HorizontalReverse, "HorizontalReverse".into()),
                        (FlexDirection::Vertical, "Vertical".into()),
                        (FlexDirection::VerticalReverse, "VerticalReverse".into()),
                    ],
                    on_selected(w.event_loop_proxy())
                ))
                .item()
                + w.text("FlexWrap").item()
                + w.row(w.radio_group(
                    &wrap,
                    &[
                        (FlexWrap::NoWrap, "NoWrap".into()),
                        (FlexWrap::Wrap, "Wrap".into()),
                        (FlexWrap::WrapReverse, "WrapReverse".into()),
                    ],
                    on_selected(w.event_loop_proxy())
                ))
                .item()
                + w.text("JustifyContent").item()
                + w.row(w.radio_group(
                    &justify_content,
                    &[
                        (JustifyContent::Start, "Start".into()),
                        (JustifyContent::Center, "Center".into()),
                        (JustifyContent::End, "End".into()),
                        (JustifyContent::SpaceBetween, "SpaceBetween".into()),
                        (JustifyContent::SpaceAround, "SpaceAround".into()),
                        (JustifyContent::SpaceEvenly, "SpaceEvenly".into()),
                    ],
                    on_selected(w.event_loop_proxy())
                ))
                .item()
                + w.text("AlignItems").item()
                + w.row(w.radio_group(
                    &align_items,
                    &[
                        (AlignItems::Start, "Start".into()),
                        (AlignItems::Center, "Center".into()),
                        (AlignItems::End, "End".into()),
                        (AlignItems::Stretch, "Stretch".into()),
                        (AlignItems::Baseline, "Baseline".into()),
                    ],
                    on_selected(w.event_loop_proxy())
                ))
                .item()
                + w.text("AlignContent").item()
                + w.row(w.radio_group(
                    &align_content,
                    &[
                        (AlignContent::Start, "Start".into()),
                        (AlignContent::Center, "Center".into()),
                        (AlignContent::End, "End".into()),
                        (AlignContent::SpaceBetween, "SpaceBetween".into()),
                        (AlignContent::SpaceAround, "SpaceAround".into()),
                        (AlignContent::SpaceEvenly, "SpaceEvenly".into()),
                        (AlignContent::Stretch, "Stretch".into()),
                    ],
                    on_selected(w.event_loop_proxy())
                ))
                .item()
                + w.flex(
                    w.text("Text").font_size(16).item().size(100, 110).background(w.rectangle(Color::RED).item())
                        + w.text("Text").font_size(20).item().size(80, 50).background(w.rectangle(Color::GREEN).item())
                        + w.text("Text").font_size(12).item().size(120, 80).background(w.rectangle(Color::BLUE).item())
                        + w.text("Text").font_size(18).item().size(90, 100).background(w.rectangle(Color::YELLOW).item())
                        + w.text("Text").font_size(26).item().size(50, 120).background(w.rectangle(Color::CYAN).item())
                        + w.text("Text").font_size(12).item().size(110, 90).background(w.rectangle(Color::MAGENTA).item())
                +                    w.text("Text").font_size(16).item().size(100, 110).background(w.rectangle(Color::RED).item())
                        + w.text("Text").font_size(20).item().size(80, 50).background(w.rectangle(Color::GREEN).item())
                        + w.text("Text").font_size(12).item().size(120, 80).background(w.rectangle(Color::BLUE).item())
                        + w.text("Text").font_size(18).item().size(90, 100).background(w.rectangle(Color::YELLOW).item())
                        + w.text("Text").font_size(26).item().size(50, 120).background(w.rectangle(Color::CYAN).item())
                        + w.text("Text").font_size(12).item().size(110, 90).background(w.rectangle(Color::MAGENTA).item())
                )
                .flex_direction(direction)
                .wrap(wrap)
                .justify_content(justify_content)
                .align_items(align_items)
                .align_content(align_content)
                .item()
                .height(400)
                .padding(16)
                .width(Size::Fill)
                .background(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(outline_color)
                        .outline_width(1)
                        .item(),
                ),
        )
        .item().width(Size::Fill)
    }

    fn flex_wrap_example(w: &WindowContext, wrap: FlexWrap) -> Item {
        let outline_color = w.theme().lock().get_color(color::ON_SURFACE).unwrap();
        w.column(
            w.text(format!("FlexWrap::{:?}", wrap)).item()
                + w.flex(
                    w.rectangle(Color::RED).item().size(100, 100)
                        + w.rectangle(Color::GREEN).item().size(100, 100)
                        + w.rectangle(Color::BLUE).item().size(100, 100)
                        + w.rectangle(Color::YELLOW).item().size(100, 100)
                        + w.rectangle(Color::CYAN).item().size(100, 100)
                        + w.rectangle(Color::MAGENTA).item().size(100, 100)
                        + w.rectangle(Color::WHITE).item().size(100, 100),
                )
                .wrap(wrap)
                .item()
                .padding(16)
                .width(Size::Fill)
                .background(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(outline_color)
                        .outline_width(1)
                        .item(),
                ),
        )
        .item()
    }

    fn justify_content_example(w: &WindowContext, justify_content: JustifyContent) -> Item {
        let outline_color = w.theme().lock().get_color(color::ON_SURFACE).unwrap();
        w.column(
            w.text(format!("JustifyContent::{:?}", justify_content))
                .item()
                + w.flex(
                    w.rectangle(Color::RED).item().size(50, 100)
                        + w.rectangle(Color::GREEN).item().size(200, 100)
                        + w.rectangle(Color::BLUE).item().size(80, 100)
                        + w.rectangle(Color::YELLOW).item().size(20, 100),
                )
                .justify_content(justify_content)
                .item()
                .padding(16)
                .width(Size::Fill)
                .background(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(outline_color)
                        .outline_width(1)
                        .item(),
                ),
        )
        .item()
    }

    fn align_items_example(w: &WindowContext, align_items: AlignItems) -> Item {
        let outline_color = w.theme().lock().get_color(color::ON_SURFACE).unwrap();
        w.column(
            w.text(format!("AlignItems::{:?}", align_items)).item()
                + w.flex(
                    w.text("Text")
                        .font_size(10)
                        .item()
                        .size(100, 100)
                        .background(w.rectangle(Color::RED).item())
                        + w.text("Text")
                            .font_size(20)
                            .item()
                            .size(100, 50)
                            .background(w.rectangle(Color::GREEN).item())
                        + w.text("Text")
                            .font_size(30)
                            .item()
                            .size(100, 150)
                            .background(w.rectangle(Color::BLUE).item())
                        + w.text("Text")
                            .font_size(16)
                            .item()
                            .size(100, 90)
                            .background(w.rectangle(Color::YELLOW).item()),
                )
                .align_items(align_items)
                .item()
                .padding(16)
                .width(Size::Fill)
                .background(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(outline_color)
                        .outline_width(1)
                        .item(),
                ),
        )
        .item()
    }

    fn align_content(w: &WindowContext, align_content: AlignContent) -> Item {
        let outline_color = w.theme().lock().get_color(color::ON_SURFACE).unwrap();
        w.column(
            w.text(format!("AlignContent::{:?}", align_content)).item()
                + w.flex(
                    w.rectangle(Color::RED).item().size(100, 100)
                        + w.rectangle(Color::GREEN).item().size(100, 100)
                        + w.rectangle(Color::BLUE).item().size(100, 100)
                        + w.rectangle(Color::YELLOW).item().size(100, 100)
                        + w.rectangle(Color::CYAN).item().size(100, 100)
                        + w.rectangle(Color::MAGENTA).item().size(100, 100)
                        + w.rectangle(Color::WHITE).item().size(100, 100)
                        + w.rectangle(Color::BLACK).item().size(100, 100)
                        + w.rectangle(Color::GRAY).item().size(100, 100)
                        + w.rectangle(Color::RED).item().size(100, 100)
                        + w.rectangle(Color::GREEN).item().size(100, 100)
                        + w.rectangle(Color::BLUE).item().size(100, 100)
                        + w.rectangle(Color::YELLOW).item().size(100, 100)
                        + w.rectangle(Color::CYAN).item().size(100, 100)
                        + w.rectangle(Color::MAGENTA).item().size(100, 100)
                        + w.rectangle(Color::WHITE).item().size(100, 100)
                        + w.rectangle(Color::BLACK).item().size(100, 100)
                        + w.rectangle(Color::GRAY).item().size(100, 100),
                )
                .align_content(align_content)
                .wrap(FlexWrap::Wrap)
                .item()
                .padding(16)
                .width(Size::Fill)
                .height(400)
                .background(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(outline_color)
                        .outline_width(1)
                        .item(),
                ),
        )
        .item()
    }

    fn flex_grow_example(w: &WindowContext) -> Item {
        let outline_color = w.theme().lock().get_color(color::ON_SURFACE).unwrap();
        w.column(
            w.text(format!("FlexGrow")).item()
                + w.flex(
                    w.rectangle(Color::RED).item().size(100, 100)
                        + w.rectangle(Color::GREEN).item().size(100, 100)
                        .flex_grow(2)
                        + w.rectangle(Color::BLUE).item().size(100, 100)
                        + w.rectangle(Color::YELLOW)
                            .item()
                            .size(100, 100)
                            .flex_grow(1),
                )
                .item()
                .padding(16)
                .width(Size::Fill)
                .background(
                    w.rectangle(Color::TRANSPARENT)
                        .outline_color(outline_color)
                        .outline_width(1)
                        .item(),
                ),
        )
        .item()
    }

    w.scroll_area(
        w.column(
            flex_direction_example(w)
                // + flex_wrap_example(w, FlexWrap::NoWrap)
                // + flex_wrap_example(w, FlexWrap::Wrap)
                // + flex_wrap_example(w, FlexWrap::WrapReverse)
                // + justify_content_example(w, JustifyContent::Start)
                // + justify_content_example(w, JustifyContent::Center)
                // + justify_content_example(w, JustifyContent::End)
                // + justify_content_example(w, JustifyContent::SpaceBetween)
                // + justify_content_example(w, JustifyContent::SpaceAround)
                // + justify_content_example(w, JustifyContent::SpaceEvenly)
                // + align_items_example(w, AlignItems::Start)
                // + align_items_example(w, AlignItems::Center)
                // + align_items_example(w, AlignItems::End)
                // + align_items_example(w, AlignItems::Stretch)
                // + align_items_example(w, AlignItems::Baseline)
                // + align_content(w, AlignContent::Start)
                // + align_content(w, AlignContent::Center)
                // + align_content(w, AlignContent::End)
                // + align_content(w, AlignContent::SpaceBetween)
                // + align_content(w, AlignContent::SpaceAround)
                // + align_content(w, AlignContent::SpaceEvenly)
                // + align_content(w, AlignContent::Stretch)
                + flex_grow_example(w),
        )
        .item(),
    )
    .item()
}
