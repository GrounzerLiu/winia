use std::sync::Arc;
use std::time::Duration;

use crate::animation::AnimationExt;
use letclone::clone;
use parking_lot::Mutex;

use crate::animation::interpolator::EaseOutCubic;
use crate::collection::CollectionOperation;
use crate::core::next_id;
use crate::shared::{SharedDerived, SharedSource, SharedWVec};
use crate::ui::item::{
    Children, ItemData, ItemKind, ItemProps, PhysicalX,
};
use crate::ui::navigation::NavKey;
use crate::ui::{Alignment, HorizontalAlignment, Item, Orientation};
use proc_macro::ItemProps;
use crate::event::{ItemEvent, MeasureMode};

#[derive(ItemProps)]
pub struct NavDisplayProps {
    pub item_props: ItemProps,
    #[constructor]
    pub back_stack: SharedWVec<Box<dyn NavKey>>,
    #[constructor(type = impl Fn(&Box<dyn NavKey>) -> Item + 'static)]
    #[not_shared]
    pub entry_provider: SharedSource<Box<dyn Fn(&Box<dyn NavKey>) -> Item>>,
    pub alignment: SharedDerived<Alignment>,
}

impl NavDisplayProps {
    pub fn new(
        item_props: ItemProps,
        back_stack: impl Into<SharedWVec<Box<dyn NavKey>>>,
        entry_provider: impl Fn(&Box<dyn NavKey>) -> Item + 'static,
    ) -> Self {
        Self {
            item_props,
            back_stack: back_stack.into(),
            alignment: Alignment::top_start().into(),
            entry_provider: SharedSource::new(Box::new(entry_provider)),
        }
        .name("NavDisplay")
    }
}

struct NavEntry {
    route_key: &'static str,
    instance_key: String,
    item: Item,
}

pub fn nav_display(props: NavDisplayProps) -> Item {
    let children = Children::new();
    Item::new(
        ItemKind::Container,
        item_event(&props, children.clone()),
        props,
        children,
    )
}

fn item_event(props: &NavDisplayProps, children: Children) -> ItemEvent {
    let entries = Arc::new(Mutex::new(Vec::<NavEntry>::new()));
    let is_subscribed = Arc::new(Mutex::new(false));

    ItemEvent::new()
        .set_measure({
            clone!(props.back_stack, props.entry_provider, entries, is_subscribed);
            move |item, width_mode, height_mode| {
                ensure_back_stack_subscription(item, &back_stack, &is_subscribed);
                sync_entries(item, &children, &entries, &back_stack, &entry_provider);

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

fn ensure_back_stack_subscription(
    item: &mut ItemData,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    is_subscribed: &Arc<Mutex<bool>>,
) {
    let mut is_subscribed = is_subscribed.lock();
    if *is_subscribed {
        return;
    }

    let event_loop_proxy = item.window_context().event_loop_proxy().clone();
    back_stack.subscribe(next_id(), move || {
        event_loop_proxy.request_update_layout();
    });
    *is_subscribed = true;
}

fn sync_entries(
    item: &mut ItemData,
    children: &Children,
    entries: &Arc<Mutex<Vec<NavEntry>>>,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    entry_provider: &SharedSource<Box<dyn Fn(&Box<dyn NavKey>) -> Item>>,
) {
    let operations = back_stack.take_operations();
    if operations.is_empty() {
        update_entry_activation(entries);
        return;
    }

    for operation in operations {
        match operation {
            CollectionOperation::Add(index) => {
                insert_entry(item, children, entries, back_stack, entry_provider, index);
            }
            CollectionOperation::Remove(index) => {
                remove_entry(item, children, entries, index);
            }
            CollectionOperation::Update(index) => {
                update_entry(item, children, entries, back_stack, entry_provider, index);
            }
            CollectionOperation::UpdateAll | CollectionOperation::Clear => {
                rebuild_entries(item, children, entries, back_stack, entry_provider);
            }
        }
    }
    update_entry_activation(entries);
}

fn insert_entry(
    item: &mut ItemData,
    children: &Children,
    entries: &Arc<Mutex<Vec<NavEntry>>>,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    entry_provider: &SharedSource<Box<dyn Fn(&Box<dyn NavKey>) -> Item>>,
    index: usize,
) {
    let back_stack_read = back_stack.read();
    let Some(nav_key) = back_stack_read.get(index) else {
        return;
    };

    let child = {
        let entry_provider = entry_provider.lock();
        (entry_provider)(nav_key)
    };
    let entry = NavEntry {
        route_key: nav_key.nav_key(),
        instance_key: nav_key.instance_key(),
        item: child.clone(),
    };

    {
        let mut entries = entries.lock();
        entries.insert(index, entry);
    }

    let mut children = children.clone();
    animate_push(item, &mut children, index, child);
}

fn remove_entry(
    item: &mut ItemData,
    children: &Children,
    entries: &Arc<Mutex<Vec<NavEntry>>>,
    index: usize,
) {
    let removed = {
        let mut entries = entries.lock();
        if index >= entries.len() {
            return;
        }
        entries.remove(index)
    };

    let mut children = children.clone();
    animate_pop(item, &mut children, removed.item.id());
}

fn update_entry(
    item: &mut ItemData,
    children: &Children,
    entries: &Arc<Mutex<Vec<NavEntry>>>,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    entry_provider: &SharedSource<Box<dyn Fn(&Box<dyn NavKey>) -> Item>>,
    index: usize,
) {
    let back_stack_read = back_stack.read();
    let Some(nav_key) = back_stack_read.get(index) else {
        return;
    };
    let expected_route = nav_key.nav_key();
    let expected_instance = nav_key.instance_key();
    let should_replace = {
        let entries = entries.lock();
        let Some(existing) = entries.get(index) else {
            return;
        };
        existing.route_key != expected_route || existing.instance_key != expected_instance
    };

    if !should_replace {
        return;
    }

    remove_entry(item, children, entries, index);
    insert_entry(item, children, entries, back_stack, entry_provider, index);
}

fn rebuild_entries(
    item: &mut ItemData,
    children: &Children,
    entries: &Arc<Mutex<Vec<NavEntry>>>,
    back_stack: &SharedWVec<Box<dyn NavKey>>,
    entry_provider: &SharedSource<Box<dyn Fn(&Box<dyn NavKey>) -> Item>>,
) {
    {
        let mut entries = entries.lock();
        entries.clear();
    }

    let mut children = children.clone();
    children.clear();

    let back_stack = back_stack.read();
        let entry_provider = entry_provider.lock();
    for nav_key in back_stack.iter() {
        let child = (entry_provider)(nav_key);
        let entry = NavEntry {
            route_key: nav_key.nav_key(),
            instance_key: nav_key.instance_key(),
            item: child.clone(),
        };
        entries.lock().push(entry);
        children.add_item(child);
    }

    item.window_context().request_update_layout();
}

fn update_entry_activation(entries: &Arc<Mutex<Vec<NavEntry>>>) {
    let entries = entries.lock();
    let top_index = entries.len().checked_sub(1);
    for (index, entry) in entries.iter().enumerate() {
        let is_top = Some(index) == top_index;
        let mut item_data = entry.item.data();
        item_data.interaction_enabled = is_top;
        item_data.focus_enabled = is_top;
    }
}

fn animate_push(item: &mut ItemData, children: &mut Children, index: usize, new_top: Item) {
    let root_id = new_top.id();
    let animation = item
        .window_context()
        .animate(crate::include_target!(root_id))
        .duration(Duration::from_millis(320))
        .interpolator(Box::new(EaseOutCubic::new()));

    children.insert_item_with_animation(index, new_top, animation, |frame| {
        let mut entry_frame = frame.clone();
        entry_frame.offset_x += frame.width.max(1.0);
        entry_frame.opacity = 0.0;
        entry_frame
    });
    item.window_context().request_update_layout();
}

fn animate_pop(item: &mut ItemData, children: &mut Children, removed_id: u32) {
    let animation = item
        .window_context()
        .animate(crate::include_target!(removed_id))
        .duration(Duration::from_millis(260))
        .interpolator(Box::new(EaseOutCubic::new()));
    children.remove_item_with_animation(removed_id, animation, |frame| {
        let mut exit_frame = frame.clone();
        exit_frame.offset_x += frame.width.max(1.0) * 0.65;
        exit_frame.opacity = 0.0;
        exit_frame
    });
    item.window_context().request_update_layout();
}
