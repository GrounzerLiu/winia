use winia::shared::Children;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{ImageExt, ScaleMode};
use winia::ui::Item;
use winia::ui::item::Size;
use winia::ui::layout::{ColumnExt, ScrollAreaExt, StackExt};

pub fn image_test(w: &WindowContext, _: &WindowAttr) -> Item {
    w.scroll_area(
        w.column(Children::new() +
            w.image("https://www.rust-lang.org/logos/rust-logo-512x512.png")
                .item()
                .size(200, 200) +
            w.image("https://www.rust-lang.org/logos/rust-logo-512x512.png")
                .item()
                .size(200, 200) +
            w.image("/home/grounzer/Pictures/Screenshot_20241104_163414.png")
                .oversize_scale_mode(ScaleMode::Contain)
                .item()
                .width(Size::Fill)
        )
    ).item()
}