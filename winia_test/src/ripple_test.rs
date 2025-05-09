use winia::shared::Children;
use winia::skia_safe::Color;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{RectangleExt, RippleExt};
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::{FlexExt, FlexWrap, RowExt, ScrollAreaExt};

pub fn ripple_test(w: &WindowContext) -> Item {
    w.row(
        w.rectangle(Color::BLUE).item().size(100, 100).foreground(
            w.ripple().item()
        ).margin_end(8)
        + w.rectangle(Color::BLUE).item().size(100, 100).foreground(
            w.ripple().borderless(true).item()
        )
    ).item()
}