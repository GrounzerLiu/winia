use proc_macro::ItemProps;
use crate::shared::SharedDerived;
use crate::ui::{Color, Item, Size};
use crate::ui::item::{Children, ItemEvent, ItemKind, ItemProps, PhysicalX};

#[derive(ItemProps)]
pub struct BadgedBoxProps {
    pub item_props: ItemProps,
}

impl BadgedBoxProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self { item_props }
    }
}

pub fn badged_box(
    badge: Item,
    icon: Item,
) -> Item {
    let props: BadgedBoxProps = badge.data().window_context.badged_box_props().clipped(false);
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        vec![icon, badge],
    )
}

fn item_event(_props: &BadgedBoxProps) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            |item, width_mode, height_mode| {
                item.measure_children(width_mode, height_mode);
                let (w, h) = if let Some(icon) = item.children().read().first() {
                    // (icon.data().measure_frame.width, icon.data().measure_frame.height)
                    let data = icon.data();
                    (data.measure_frame.width, data.measure_frame.height)
                } else {
                    (0.0, 0.0)
                };
                item.measure_frame.width = w;
                item.measure_frame.height = h;
            }
        })
        .set_layout(
            |item, width, height| {
                let layout_direction = item.layout_direction.get();
                item.children().lock().iter().enumerate().for_each(|(i, child)| {
                    let mut child_data = child.data();
                    if i == 0 {
                        let w = child_data.measure_frame.width();
                        let h = child_data.measure_frame.height();
                        child_data.dispatch_layout(0.0, 0.0, w, h);
                    } else {
                        let w = child_data.measure_frame.width;
                        let h = child_data.measure_frame.height;
                        let is_large = child_data.props().get_custom::<SharedDerived<bool>>("is_large").cloned();
                        // println!("is_large: {:?}", is_large);
                        if let Some(is_large) = is_large && is_large.get() {
                            child_data.dispatch_layout(
                                (width - 12.0).physical_x(layout_direction, width, w),
                                14.0 - h,
                                w,
                                h
                            )
                        } else {
                            child_data.dispatch_layout(
                                (width - w).physical_x(layout_direction, width, w),
                                0.0,
                                w,
                                h
                            );
                        }
                    }
                })
            }
        )
}