use letclone::clone;
use crate::shared::SharedDerived;
use crate::ui::item::{Children, ItemKind, ItemProps, PhysicalX, SetCustomProp};
use crate::ui::{Alignment, HorizontalAlignment, Item, Orientation};
use crate::define_props;
use proc_macro::ItemProps;
use crate::event::{ItemEvent, MeasureMode};
/*define_props! {
    StackPropsTrait;
    stack_props;
    StackProps {
        alignment: SharedDerived<Alignment>
    }
}*/

#[derive(ItemProps)]
pub struct StackProps {
    pub item_props: ItemProps,
    pub alignment: SharedDerived<Alignment>,
}

impl StackProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self {
            item_props,
            alignment: Alignment::top_start().into(),
        }.name("Stack")
    }
}

pub fn stack(props: StackProps, children: impl Into<Children>) -> Item {
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        children,
    )
}

fn item_event(props: &StackProps) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            move |item, width_mode, height_mode| {
                let padding_v = item.get_padding(Orientation::Vertical);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let mut max_width = 0_f32;
                let mut max_height = 0_f32;
                let mut children = item.children().lock();
                for child in children.iter_mut() {
                    let mut child_data = child.data();
                    let child_width = child_data.props().width.get();
                    let child_height = child_data.props().height.get();
                    child_data.dispatch_measure(
                        child_width.create_measure_mode(width_mode.value() - padding_h),
                        child_height.create_measure_mode(height_mode.value() - padding_v),
                    );
                    max_width = max_width.max(child_data.measure_frame.width);
                    max_height = max_height.max(child_data.measure_frame.height);
                }
                drop(children);
                match width_mode {
                    MeasureMode::Specified(width) => {
                        item.measure_frame.width = item.clamp_width(width);
                    }
                    MeasureMode::Unspecified(width) => {
                        item.measure_frame.width =
                            item.clamp_width((max_width + padding_h).min(width));
                    }
                }
                match height_mode {
                    MeasureMode::Specified(height) => {
                        item.measure_frame.height = item.clamp_height(height);
                    }
                    MeasureMode::Unspecified(height) => {
                        item.measure_frame.height =
                            item.clamp_height((max_height + padding_v).min(height));
                    }
                }
            }
        })
        .set_layout({
            clone!(props.padding, props.layout_direction, props.alignment);
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
                            .unwrap_or_else(|| alignment.clone())
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

pub trait AlignSelf {
    fn align_self(self, align: impl Into<SharedDerived<Alignment>>) -> Self;
}

impl<T: SetCustomProp> AlignSelf for T {
    fn align_self(mut self, align: impl Into<SharedDerived<Alignment>>) -> Self {
        self.set_custom_prop("align_self", align);
        self
    }
}
