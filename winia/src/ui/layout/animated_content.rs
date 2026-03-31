use letclone::clone;
use proc_macro::ItemProps;
use crate::animation::AnimationExt;
use crate::app::WindowContext;
use crate::exclude_target;
use crate::shared::{SharedDerived, SharedSource};
use crate::ui::{Alignment, HorizontalAlignment, Item, Orientation};
use crate::ui::item::{ItemData, ItemEvent, ItemKind, ItemProps, MeasureMode, PhysicalX};

#[derive(ItemProps)]
pub struct AnimatedContentProps<T: PartialEq + Clone + 'static> {
    pub item_props: ItemProps,
    #[constructor]
    pub target: SharedDerived<T>,
    #[constructor(type = impl FnMut(&T, &WindowContext) -> Item + 'static)]
    pub content: SharedSource<Box<dyn FnMut(&T, &WindowContext) -> Item>>,
    pub alignment: SharedDerived<Alignment>,
}

impl<T> AnimatedContentProps<T>
where
    T: PartialEq + Clone + 'static,
{
    pub fn new(
        item_props: ItemProps,
        target: impl Into<SharedDerived<T>>,
        content: impl FnMut(&T, &WindowContext) -> Item + 'static,
    ) -> Self {
        let target = target.into();
        let content: Box<dyn FnMut(&T, &WindowContext) -> Item> = Box::new(content);
        let content = SharedSource::new(content);
        Self {
            item_props,
            target,
            content,
            alignment: Alignment::center().into(),
        }
    }
}

pub fn animated_content<T>(props: AnimatedContentProps<T>) -> Item
where
    T: PartialEq + Clone + 'static,
{
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        vec![],
    )
}

fn item_event<T>(props: &AnimatedContentProps<T>) -> ItemEvent
where
    T: PartialEq + Clone + 'static,
{
    ItemEvent::new()
        .set_measure({
            clone!(
                props.target,
                props.content
            );
            let mut last_target: Option<T> = None;
            move |item, width_mode, height_mode| {
                let mut children = item.children().clone();
                if let Some(last_target_value) = &last_target {
                    let target_value = target.get();
                    if last_target_value != &target_value {
                        {
                            let ops = children.pending_operations().clone();
                            let mut children = children.lock();
                            let len = children.len();
                            if let Some(index) = len.checked_sub(1) && let Some(old_children) = children.get(index) {
                                let animation = item.window_context.animate(exclude_target!())
                                    .duration(std::time::Duration::from_millis(300))
                                    .on_finished(move ||{
                                        ops.add_operation(move |children| {
                                            if !children.len() > 1 {
                                                children.remove(0);
                                            }
                                        });
                                    })
                                    .interpolator(Box::new(crate::animation::interpolator::Linear::new()));
                                let mut old_child_data = old_children.data();
                                old_child_data.exit_frame = Some(Box::new(|frame| {
                                    let mut exit_frame = frame.clone();
                                    exit_frame.opacity = 0.0;
                                    exit_frame.offset_x = -exit_frame.width;
                                    exit_frame
                                }));
                                old_child_data.is_exited = false;
                                drop(old_child_data);
                                animation.start();
                                item.window_context.request_update_layout();
                            }
                        }


                        let mut content_fn = content.lock();
                        let child_item = content_fn(&target_value, item.window_context());
                        let prev_right = if let Some(last) = children.lock().last() {
                            let mut last_data = last.data();
                            let current_frame = last_data.current_frame();
                            current_frame.x() + current_frame.width
                        } else {
                            0.0
                        };
                        let on_mounted: Box<dyn FnMut(&mut ItemData) + 'static> = Box::new(move |item| {
                            item.is_entered = false;
                            item.entry_frame = Some(Box::new(move |frame| {
                                let mut entry_frame = frame.clone();
                                entry_frame.opacity = 0.0;
                                entry_frame.offset_x = prev_right - frame.x();
                                entry_frame
                            }));
                            let animation = item.window_context.animate(exclude_target!())
                                .duration(std::time::Duration::from_millis(300))
                                .interpolator(Box::new(crate::animation::interpolator::Linear::new()));
                            animation.start();
                            item.window_context.request_update_layout();
                        });
                        child_item.data().on_mounted.lock().push(on_mounted);
                        children.add_item(child_item);
                        last_target = Some(target_value);
                    }
                } else {
                    let target_value = target.get();
                    let mut content_fn = content.lock();
                    let child_item = content_fn(&target_value, item.window_context());
                    children.add_item(child_item);
                    last_target = Some(target_value);
                }

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
        }).set_layout({
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
