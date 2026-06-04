use crate::event::{ItemEvent, MeasureMode, MouseScrollDelta, MouseWheel};
use crate::shared::{SharedDerived, SharedDerivedWVec, SharedSource};
use crate::ui::item::{Children, ItemKind, ItemProps, PhysicalX};
use crate::ui::{Item, LazyListState, Orientation};
use proc_macro::ItemProps;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDirection {
    TopToBottom,
    BottomToTop,
    StartToEnd,
    EndToStart,
}

#[derive(ItemProps)]
pub struct LazyListProps<T: Send + Sync + 'static> {
    pub item_props: ItemProps,
    pub direction: SharedDerived<ListDirection>,
    pub list_state: SharedSource<LazyListState>,
    #[constructor]
    pub list_items: SharedDerivedWVec<T>,
    #[constructor(type = impl Fn(usize, &T) -> Item + Send + Sync + 'static)]
    pub item_generator: SharedSource<Box<dyn Fn(usize, &T) -> Item + Send + Sync + 'static>>,
    #[not_shared]
    pub estimated_item_height: SharedSource<f32>,
    #[not_shared]
    pub buffer_size: SharedSource<usize>,
    #[not_shared]
    pub on_list_state_change: SharedSource<Option<Box<dyn Fn(LazyListState) + Send + Sync + 'static>>>,
}

impl<T: Send + Sync + 'static> LazyListProps<T> {
    pub fn new(
        item_props: ItemProps,
        list_items: impl Into<SharedDerivedWVec<T>>,
        item_generator: impl Fn(usize, &T) -> Item + Send + Sync + 'static,
    ) -> Self {
        Self {
            item_props,
            direction: ListDirection::TopToBottom.into(),
            list_state: SharedSource::new(LazyListState::new()),
            list_items: list_items.into(),
            item_generator: SharedSource::new(Box::new(item_generator)),
            estimated_item_height: SharedSource::new(40.0),
            buffer_size: SharedSource::new(3),
            on_list_state_change: SharedSource::new(None),
        }
    }

    pub fn on_list_state_change(
        self,
        callback: impl Fn(LazyListState) + Send + Sync + 'static,
    ) -> Self {
        let callback_box: Box<dyn Fn(LazyListState) + Send + Sync + 'static> = Box::new(callback);
        self.on_list_state_change.set(Some(callback_box));
        self
    }

    pub fn set_estimated_item_height(self, height: f32) -> Self {
        self.estimated_item_height.set(height);
        self
    }

    pub fn set_buffer_size(self, size: usize) -> Self {
        self.buffer_size.set(size);
        self
    }
}

pub fn lazy_list<T: Send + Sync + 'static>(
    props: LazyListProps<T>,
) -> Item {
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        Children::new(),
    )
}

fn item_event<T: Send + Sync + 'static>(props: &LazyListProps<T>) -> ItemEvent {
    // Capture event_loop_proxy before entering closures (required for request_update_layout)
    let event_loop_proxy = props.item_props.window_context.event_loop_proxy().clone();

    ItemEvent::new()
        .set_measure({
            let list_state = props.list_state.clone();
            let list_items = props.list_items.clone();
            let item_generator = props.item_generator.clone();
            let on_list_state_change = props.on_list_state_change.clone();
            let estimated_item_height = props.estimated_item_height.clone();
            let buffer_size = props.buffer_size.clone();
            let direction = props.direction.clone();
            move |item, width_mode, height_mode| {
                let padding_v = item.get_padding(Orientation::Vertical);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let available_width = width_mode.value() - padding_h;
                let available_height = height_mode.value() - padding_v;

                let is_vertical = matches!(direction.get(), ListDirection::TopToBottom | ListDirection::BottomToTop);
                let available_primary = if is_vertical { available_height } else { available_width };
                let padding_cross = if is_vertical { padding_h } else { padding_v };

                let buffer = *buffer_size.lock();
                let est_primary = *estimated_item_height.lock();
                let current_offset = list_state.lock().offset;
                let total_count = list_items.read().len();

                if total_count == 0 {
                    *item.children().lock() = vec![];
                    let mut state = list_state.lock();
                    state.offset = 0.0;
                    state.visible_range = 0..0;
                    item.measure_frame.width = item.clamp_width(padding_h);
                    item.measure_frame.height = item.clamp_height(padding_v);
                    return;
                }

                // --- Step 1: compute target range + create+measure new items in one pass ---
                let est_start = if est_primary > 0.0 {
                    ((-current_offset / est_primary) as usize).min(total_count.saturating_sub(1))
                } else {
                    0
                };
                let target_start = est_start.saturating_sub(buffer);

                let mut accumulated_primary = 0_f32;
                let mut max_cross = 0_f32;
                let mut new_items: Vec<Item> = Vec::new();
                let mut target_end = target_start;

                for idx in target_start..total_count {
                    let item_ref = {
                        let guard = list_items.read();
                        match guard.get(idx) {
                            Some(r) => r as *const T,
                            None => break,
                        }
                    };
                    let new_item = {
                        let rf = unsafe { &*item_ref };
                        (item_generator.lock())(idx, rf)
                    };
                    {
                        let mut child_data = new_item.data();
                        let child_w = child_data.props().width.get();
                        let child_h = child_data.props().height.get();
                        child_data.dispatch_measure(
                            child_w.create_measure_mode(available_width),
                            child_h.create_measure_mode(available_height),
                        );
                        let ps = if is_vertical { child_data.measure_frame.height } else { child_data.measure_frame.width };
                        let cs = if is_vertical { child_data.measure_frame.width } else { child_data.measure_frame.height };
                        accumulated_primary += ps;
                        max_cross = max_cross.max(cs);
                    }
                    new_items.push(new_item);
                    target_end = idx + 1;
                    if accumulated_primary + current_offset > available_primary
                        && (target_end - target_start) > buffer
                    {
                        break;
                    }
                }

                // --- Step 2: replace children (new_items already fully created+measured) ---
                {
                    let updater = item.props().item_updater.clone();
                    for new_item in &new_items {
                        new_item.data().props().item_updater.lock().need_redraw = true;
                        new_item.data().props().item_updater.lock().parent = Some(Arc::downgrade(&updater));
                    }
                    updater.lock().request_update();
                }
                *item.children().lock() = new_items;

                // --- Step 3: clamp offset ---
                let count = target_end - target_start;
                let content_primary = if count > 0 {
                    let avg = accumulated_primary / count as f32;
                    accumulated_primary + (total_count - target_end) as f32 * avg + padding_cross
                } else {
                    accumulated_primary + padding_cross
                };
                {
                    let min_offset = (available_primary - content_primary).min(0.0);
                    list_state.lock().offset = current_offset.clamp(min_offset, 0.0);
                }

                // --- Step 4: update state ---
                {
                    let mut state = list_state.lock();
                    state.visible_range = target_start..target_end;
                }
                if let Some(callback) = on_list_state_change.lock().as_mut() {
                    callback(list_state.lock().clone());
                }

                // --- Step 5: set self size ---
                match width_mode {
                    MeasureMode::Specified(w) => item.measure_frame.width = item.clamp_width(w),
                    MeasureMode::Unspecified(w) => {
                        let w_val = if is_vertical { max_cross } else { accumulated_primary };
                        item.measure_frame.width = item.clamp_width((w_val + padding_h).min(w));
                    }
                }
                match height_mode {
                    MeasureMode::Specified(h) => item.measure_frame.height = item.clamp_height(h),
                    MeasureMode::Unspecified(h) => {
                        let h_val = if is_vertical { accumulated_primary } else { max_cross };
                        item.measure_frame.height = item.clamp_height((h_val + padding_v).min(h));
                    }
                }
            }
        })
        .set_layout({
            let list_state = props.list_state.clone();
            let padding = props.padding.clone();
            let layout_direction = props.layout_direction.clone();
            let direction = props.direction.clone();
            move |item, parent_width, parent_height| {
                let padding_start = padding.start.get();
                let padding_top = padding.top.get();
                let lr = layout_direction.get();
                let is_vertical = matches!(direction.get(), ListDirection::TopToBottom | ListDirection::BottomToTop);
                let offset = list_state.lock().offset;

                let children = item.children();
                let mut primary_pos = padding_top + offset;
                let cross_base = padding_start;

                for child in children.lock().iter() {
                    let mut child_data = child.data();
                    let child_width = child_data.measure_frame.width;
                    let child_height = child_data.measure_frame.height;
                    let (x, y) = if is_vertical {
                        (cross_base, primary_pos)
                    } else {
                        (primary_pos, cross_base)
                    };
                    child_data.dispatch_layout(
                        x.physical_x(lr, parent_width, child_width),
                        y,
                        child_width,
                        child_height,
                    );
                    primary_pos += if is_vertical { child_height } else { child_width };
                }
            }
        })
        .set_mouse_wheel({
            let list_state = props.list_state.clone();
            let e = event_loop_proxy.clone();
            move |_item, mouse_wheel: MouseWheel| {
                let delta = match mouse_wheel.delta {
                    MouseScrollDelta::LineDelta(_x, y) => y * 40.0,
                    MouseScrollDelta::Delta(_x, y) => y,
                };
                list_state.write(|s| s.offset += delta);
                e.request_update_layout();
                MouseScrollDelta::Delta(0.0, delta)
            }
        })
}
