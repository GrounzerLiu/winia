use clonelet::clone;
use crate::{bind_properties, define_props};
use crate::shared::{SharedDerived, SharedDerivedBool, SharedF32};
use crate::ui::{Alignment, HorizontalAlignment, Item, Orientation};
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, MeasureMode, PhysicalX};

define_props! {
    ScrollAreaPropsTrait;
    scroll_area_props;
    ScrollAreakProps {
        horizontal_scrollable: SharedDerivedBool,
        vertical_scrollable: SharedDerivedBool,
    }
}

impl ScrollAreakProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self {
            item_props,
            horizontal_scrollable: false.into(),
            vertical_scrollable: true.into(),
        }
    }
}

pub fn scroll_area(
    props: ScrollAreakProps,
    child: Item,
) -> Item {
    let children = vec![child];
    let item = Item::new(
        ItemKind::Container,
        item_event(&props),
        props.item_props,
        children,
    );
    bind_properties!(
        item,
        props.horizontal_scrollable,
        props.vertical_scrollable
    );
    item
}

fn item_event(props: &ScrollAreakProps) -> ItemEvent {
    let x_offset = SharedF32::new(0.0);
    let y_offset = SharedF32::new(0.0);
    ItemEvent::new()
        .set_measure({
            clone!(
                props.horizontal_scrollable,
                props.vertical_scrollable
            );
            move |item, width_mode, height_mode| {
                let padding_v = item.get_padding(Orientation::Vertical);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let h_scrollable = horizontal_scrollable.get();
                let v_scrollable = vertical_scrollable.get();
                let mut child_width = 0_f32;
                let mut child_height = 0_f32;
                {
                    let children = item.children().lock();
                    if let Some(child) = children.first() {
                        let max_width = if h_scrollable {
                            f32::INFINITY
                        } else {
                            width_mode.value() - padding_h
                        };
                        let max_height = if v_scrollable {
                            f32::INFINITY
                        } else {
                            height_mode.value() - padding_v
                        };
                        let width = child.data().props().width.get();
                        let height = child.data().props().height.get();
                        child.data().measure(
                            width.create_measure_mode(max_width),
                            height.create_measure_mode(max_height),
                        );
                        child_width = child.data().measure_frame.width;
                        child_height = child.data().measure_frame.height;
                    }
                }
                
                match width_mode {
                    MeasureMode::Specified(width) => {
                        item.measure_frame.width = item.clamp_width(width);
                    }
                    MeasureMode::Unspecified(width) => {
                        item.measure_frame.width =
                            item.clamp_width((child_width + padding_h).min(width));
                    }
                }
                
                match height_mode {
                    MeasureMode::Specified(height) => {
                        item.measure_frame.height = item.clamp_height(height);
                    }
                    MeasureMode::Unspecified(height) => {
                        item.measure_frame.height =
                            item.clamp_height((child_height + padding_v).min(height));
                    }
                }
            }
        })
        .set_layout({
            clone!(props.padding, props.layout_direction);
            move |item, width, height| {
                let padding_start = padding.start.get();
                let padding_end = padding.end.get();
                let padding_top = padding.top.get();
                let padding_bottom = padding.bottom.get();
                let layout_direction = layout_direction.get();

                let children = item.children();
                {
                    let children = children.lock();
                    for child in children.iter() {
                        let mut child_data = child.data();
                        let alignment = child_data
                            .props()
                            .get_custom::<SharedDerived<Alignment>>("align_self")
                            .cloned()
                            .unwrap_or_else(|| Alignment::top_start().into())
                            .get();

                        let measure_frame = &child_data.measure_frame;
                        let child_width = measure_frame.width;
                        let child_height = measure_frame.height;

                        let x = match alignment.horizontal() {
                            HorizontalAlignment::Start => padding_start,
                            HorizontalAlignment::Center => (width - child_width) / 2.0,
                            HorizontalAlignment::End => width - child_width - padding_end,
                        };
                        let y = match alignment.vertical() {
                            crate::ui::VerticalAlignment::Top => padding_top,
                            crate::ui::VerticalAlignment::Center => (height - child_height) / 2.0,
                            crate::ui::VerticalAlignment::Bottom => {
                                height - child_height - padding_bottom
                            }
                        };
                        child_data.dispatch_layout(
                            x.physical_x(layout_direction, width, child_width),
                            y,
                            child_width,
                            child_height,
                        );
                    }
                }
            }
        })
}