use winia::app::WindowContext;
use winia::shared::{Shared, SharedF32, SharedSource, SharedU8, SharedUsize};
use winia::ui::{animated_content, button, flex, label, rectangle, Alignment, AnimatedContentPropsTrait, Color, ColumnPropsTrait, Item, RectanglePropsTrait, Size};
use winia::ui::button::ButtonPropsTrait;

pub fn animated_content_test(w: &WindowContext) -> Item {
    let p:SharedUsize = SharedUsize::new(0);

    flex(
        w.column_props()
            .size(Size::Fill, Size::Fill),
        vec![
            button(
                w.button_props()
                    .on_click({
                        let p = p.clone();
                        move |_| {
                            let next = (p.get() + 1) % 3;
                            p.set(next);
                        }
                    }),
                |props,_| {
                    label(props.text("Button"))
                }
            ),
            animated_content(
                w.animated_content_props(
                    p,
                    |p: &usize, w|{
                        if *p == 0 {
                            rectangle(
                                w.rectangle_props(Color::RED)
                                    .size(Size::Fixed(100.0), Size::Fixed(100.0))
                            )
                        } else if *p == 1 {
                            rectangle(
                                w.rectangle_props(Color::GREEN)
                                    .size(Size::Fixed(150.0), Size::Fixed(150.0))
                            )
                        } else {
                            rectangle(
                                w.rectangle_props(Color::BLUE)
                                    .size(Size::Fixed(200.0), Size::Fixed(200.0))
                            )
                        }
                    }
                ).size(Size::Fill, Size::Fill)
                    .alignment(Alignment::top_start())
            )
        ],
    )
}