use clonelet::clone;
use winia::exclude_target;
use winia::shared::{Children, Gettable, Settable, SharedBool, SharedDrawable};
use winia::skia_safe::Color;
use winia::ui::Item;
use winia::ui::animation::AnimationExt;
use winia::ui::app::{WindowAttr, WindowContext};
use winia::ui::component::{Drawable, ImageDrawable, ImageExt, ScaleMode};
use winia::ui::item::Size;
use winia::ui::layout::{ColumnExt, ScrollAreaExt, StackExt};

pub fn image_test(w: &WindowContext) -> Item {
    let image = SharedDrawable::from("/home/grounzer/Pictures/Screenshot_20241104_163414.png");
    let b = SharedBool::from(true);
    w.scroll_area(
        w.column(
            w.image("https://www.rust-lang.org/logos/rust-logo-512x512.png")
                .item()
                .size(200, 200)
                + w.image("https://www.rust-lang.org/logos/rust-logo-512x512.png")
                    .item()
                    .size(200, 200)
                + w.image("/home/grounzer/Pictures/Screenshot_20241104_163414.png")
                    .oversize_scale_mode(ScaleMode::Stretch)
                    .item()
                    .size(400, 400)
                + w.image(&image)
                    .undersize_scale_mode(ScaleMode::Stretch)
                .oversize_scale_mode(ScaleMode::Stretch)
                    .item()
                    .size(400, 200)
                .on_click({
                    clone!(w, image, b);
                    move |_| {
                        w.animate(exclude_target!())
                            .transformation({
                                clone!(image, b);
                                move || {
                                    if b.get() {
                                        image.set({ 
                                            let mut i = ImageDrawable::from_file("/home/grounzer/RustroverProjects/winia/example/add.svg").unwrap();
                                            i.set_color(Some(Color::WHITE));
                                            i
                                        });
                                    } else { 
                                        image.set(ImageDrawable::from_file("/home/grounzer/Pictures/Screenshot_20241104_163414.png").unwrap());
                                    }
                                    b.set(!b.get());
                                }
                            })
                            .duration(std::time::Duration::from_millis(500))
                            .start();
                    }
                }),
        ).item(),
    )
    .item()
}
