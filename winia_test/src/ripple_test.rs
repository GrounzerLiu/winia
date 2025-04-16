use winia::shared::Children;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::RippleExt;
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::{FlexExt, FlexWrap, ScrollAreaExt};

pub fn ripple_test(w: &WindowContext, _: &WindowAttr) -> Item {
    w.scroll_area(
        w.flex(
            Children::from_static(
                // Vec<Item>
                (0..5000)
                    .map(|i| {
                        w.ripple().item().size(50, 50)
                    })
                    .collect::<Vec<Item>>(),
            )
        ).cross_axis_gap(10)
            .main_axis_gap(10)
            .wrap(FlexWrap::Wrap)
            .item()
            .size(Size::Fill, Size::Auto)
    ).item()
}