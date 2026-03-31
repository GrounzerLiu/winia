use winia::animation::AnimationExt;
use winia::app::WindowContext;
use winia::{closure_use, exclude_target};
use winia::shared::{SharedBool, SharedSize};
use winia::ui::{flex, rectangle, Item, RectanglePropsTrait, RowPropsTrait, Size};

#[closure_use]
pub fn animation_test(w: &WindowContext) -> Item {
    let w = w.clone();
    let r1_size = SharedSize::new(Size::Fixed(100.0));
    let r2_size = SharedSize::new(Size::Fixed(100.0));
    let is_r1_expanded = SharedBool::new(false);
    let is_r2_expanded = SharedBool::new(false);
    flex(
        w.row_props(),
        vec![
            rectangle(
                w.rectangle_props(winia::ui::Color::RED)
                    .size(&r1_size, &r1_size)
                    .on_click(move |_| {
                        let is_expanded = is_r1_expanded.get();
                        w.animate(exclude_target!())
                            .transformation(move ||{
                                if is_expanded {
                                    r1_size.set(Size::Fixed(100.0));
                                } else {
                                    r1_size.set(Size::Fixed(200.0));
                                }
                            })
                            .duration(std::time::Duration::from_millis(300))
                            .start();
                        is_r1_expanded.set(!is_expanded);
                    })
            ),
            rectangle(
                w.rectangle_props(winia::ui::Color::BLUE)
                    .size(&r2_size, &r2_size)
                    .on_click(move |_| {
                        let is_expanded = is_r2_expanded.get();
                        w.animate(exclude_target!())
                            .transformation(move ||{
                                if is_expanded {
                                    r2_size.set(Size::Fixed(100.0));
                                } else {
                                    r2_size.set(Size::Fixed(200.0));
                                }
                            })
                            .duration(std::time::Duration::from_millis(3000))
                            .start();
                        is_r2_expanded.set(!is_expanded);
                    })
            ),
        ]
    )
}