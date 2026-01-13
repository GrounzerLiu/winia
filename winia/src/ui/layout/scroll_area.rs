use crate::app::WindowContext;
use crate::core::next_id;
use crate::shared::{SharedDerived, SharedDerivedBool, SharedF32, SharedSource};
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, MeasureMode, MouseScrollDelta, MouseWheel, PhysicalX};
use crate::ui::{Item, Orientation};
use clonelet::clone;
use proc_macro::ItemProps;
use std::time::Instant;
use winit::event::TouchPhase;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScrollMode {
    Disabled,
    Start,
    End,
}

#[derive(Copy, Clone, Debug)]
pub struct ScrollState {
    /// The offset of the scroll area from the start.
    offset: f32,
    viewport_size: f32,
    content_size: f32,
}
impl ScrollState {
    fn new(viewport_size: f32, content_size: f32) -> Self {
        Self {
            offset: 0.0,
            viewport_size,
            content_size,
        }
    }

    pub fn new_shared() -> SharedSource<ScrollState> {
        SharedSource::new(ScrollState{
            offset: 0.0,
            viewport_size: 0.0,
            content_size: 0.0,
        })
    }

    pub fn offset(&self) -> f32 {
        self.offset
    }

    pub fn set_offset(&mut self, offset: f32) {
        let min_offset = self.viewport_size - self.content_size;
        self.offset = offset.clamp(min_offset.min(0.0), 0.0);
    }

    pub fn viewport_size(&self) -> f32 {
        self.viewport_size
    }

    pub fn content_size(&self) -> f32 {
        self.content_size
    }

    fn set_viewport_size(&mut self, viewport_size: f32) {
        self.viewport_size = viewport_size.clamp(0.0, f32::MAX);
    }

    fn set_content_size(&mut self, content_size: f32) {
        self.content_size = content_size.clamp(0.0, f32::MAX);
    }
}

#[derive(ItemProps)]
pub struct ScrollAreaProps {
    pub item_props: ItemProps,
    pub horizontal_scrollable: SharedDerivedBool,
    pub vertical_scrollable: SharedDerivedBool,
    pub horizontal_scroll_state: SharedDerived<ScrollState>,
    pub vertical_scroll_state: SharedDerived<ScrollState>,
    #[not_shared]
    pub on_horizontal_scroll_state_change: SharedSource<Option<Box<dyn Fn(ScrollState)>>>,
    #[not_shared]
    pub on_vertical_scroll_state_change: SharedSource<Option<Box<dyn Fn(ScrollState)>>>,
}

impl ScrollAreaProps {
    pub fn new(item_props: ItemProps) -> Self {
        Self {
            item_props,
            horizontal_scrollable: SharedDerivedBool::new_derived(false),
            vertical_scrollable: SharedDerivedBool::new_derived(false),
            horizontal_scroll_state: ScrollState::new(0.0, 0.0).into(),
            vertical_scroll_state: ScrollState::new(0.0, 0.0).into(),
            on_horizontal_scroll_state_change: SharedSource::new(None),
            on_vertical_scroll_state_change: SharedSource::new(None),
        }.name("ScrollArea")
    }

    pub fn on_horizontal_scroll_state_change(
        self,
        callback: impl Fn(ScrollState) + 'static,
    ) -> Self {
        let callback_box: Box<dyn Fn(ScrollState)> = Box::new(callback);
        self.on_horizontal_scroll_state_change.set(Some(callback_box));
        self
    }
    pub fn on_vertical_scroll_state_change(
        self,
        callback: impl Fn(ScrollState) + 'static,
    ) -> Self {
        let callback_box: Box<dyn Fn(ScrollState)> = Box::new(callback);
        self.on_vertical_scroll_state_change.set(Some(callback_box));
        self
    }
}

pub fn scroll_area(props: ScrollAreaProps, child: Item) -> Item {
    Item::new(ItemKind::Container, item_event(&props), props, vec![child])
}

fn item_event(props: &ScrollAreaProps) -> ItemEvent {
    let v_speed = SharedF32::new(0.0);
    v_speed.subscribe(next_id(), {
        let e = props.window_context.event_loop_proxy().clone();
        let updater = props.item_updater.clone();
        move ||{
            e.request_update_layout();
            updater.lock().request_update();
        }
    });
    let h_speed = SharedF32::new(0.0);
    h_speed.subscribe(next_id(), {
        let e = props.window_context.event_loop_proxy().clone();
        let updater = props.item_updater.clone();
        move ||{
            e.request_update_layout();
            updater.lock().request_update();
        }
    });
    let last_calculated_v_speed_time = SharedSource::new(Instant::now());
    let last_calculated_h_speed_time = SharedSource::new(Instant::now());

    let padding = props.item_props.padding.clone();
    let layout_direction = props.item_props.layout_direction.clone();
    let horizontal_scrollable = props.horizontal_scrollable.clone();
    let vertical_scrollable = props.vertical_scrollable.clone();
    let horizontal_scroll_state = props.horizontal_scroll_state.clone();
    let vertical_scroll_state = props.vertical_scroll_state.clone();
    let on_horizontal_scroll_state_change = props.on_horizontal_scroll_state_change.clone();
    let on_vertical_scroll_state_change = props.on_vertical_scroll_state_change.clone();

    ItemEvent::new()
        .set_measure({
            clone!(
                props.vertical_scrollable,
                props.horizontal_scrollable,
                props.horizontal_scroll_state,
                props.vertical_scroll_state,
                props.on_horizontal_scroll_state_change,
                props.on_vertical_scroll_state_change
            );
            move |item, width_mode, height_mode| {
                let padding_v = item.get_padding(Orientation::Vertical);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let horizontal_scrollable = horizontal_scrollable.get();
                let vertical_scrollable = vertical_scrollable.get();
                let mut child_width = 0_f32;
                let mut child_height = 0_f32;
                {
                    let children = item.children().lock();
                    if let Some(child) = children.first() {
                        let max_width = if horizontal_scrollable {
                            f32::INFINITY
                        } else {
                            width_mode.value() - padding_h
                        };
                        let max_height = if vertical_scrollable {
                            f32::INFINITY
                        } else {
                            height_mode.value() - padding_v
                        };
                        let width = child.data().props().width.get();
                        let height = child.data().props().height.get();
                        child.data().dispatch_measure(
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

                let h_scroll_state = horizontal_scroll_state.lock();
                if horizontal_scrollable
                    && (h_scroll_state.viewport_size() != item.measure_frame.width - padding_h
                        || h_scroll_state.content_size() != child_width)
                {
                    let mut new_state = *h_scroll_state;
                    new_state.set_viewport_size(item.measure_frame.width - padding_h);
                    new_state.set_content_size(child_width);
                    drop(h_scroll_state);
                    let on_change = on_horizontal_scroll_state_change.lock();
                    if let Some(callback) = &*on_change {
                        callback(new_state);
                    }
                }
                let v_scroll_state = vertical_scroll_state.lock();
                if vertical_scrollable
                    && (v_scroll_state.viewport_size() != item.measure_frame.height - padding_v
                        || v_scroll_state.content_size() != child_height)
                {
                    let mut new_state = *v_scroll_state;
                    new_state.set_viewport_size(item.measure_frame.height - padding_v);
                    new_state.set_content_size(child_height);
                    drop(v_scroll_state);
                    let on_change = on_vertical_scroll_state_change.lock();
                    if let Some(callback) = &*on_change {
                        callback(new_state);
                    }
                }
            }
        })
        .set_layout({
            // clone!(
            //     // v_speed,
            //     // h_speed,
            //     // last_calculated_v_speed_time,
            //     // last_calculated_h_speed_time,
            //     props.padding,
            //     props.layout_direction,
            //     props.horizontal_scroll_state,
            //     props.vertical_scroll_state,
            //     props.horizontal_scrollable,
            //     props.vertical_scrollable,
            //     props.on_horizontal_scroll_state_change,
            //     props.on_vertical_scroll_state_change
            // );
            use |item, width, height| {
                let padding_start = padding.start.get();
                let padding_end = padding.end.get();
                let padding_top = padding.top.get();
                let padding_bottom = padding.bottom.get();
                let layout_direction = layout_direction.get();
                let horizontal_scroll_state = horizontal_scroll_state.get();
                let vertical_scroll_state = vertical_scroll_state.get();

                let children = item.children();
                if let Some(child) = children.lock().first() {
                    let mut child_data = child.data();
                    let measure_frame = &child_data.measure_frame;
                    let child_width = measure_frame.width;
                    let child_height = measure_frame.height;

                    let x = padding_start + horizontal_scroll_state.offset;
                    let y = padding_top + vertical_scroll_state.offset;
                    child_data.dispatch_layout(
                        x.physical_x(layout_direction, width, child_width),
                        y,
                        child_width,
                        child_height,
                    );
                }

/*                let h_scrollable = horizontal_scrollable.get();
                let v_scrollable = vertical_scrollable.get();
                if h_scrollable && h_speed.get().abs() > 0.0 {
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_calculated_h_speed_time.get());
                    let elapsed_secs = elapsed.as_secs_f32();
                    if elapsed_secs > 0.0 {
                        let speed = h_speed.get();
                        let d = speed * elapsed_secs;
                        let on_change = on_horizontal_scroll_state_change.lock();
                        if let Some(callback) = &*on_change {
                            let mut new_state = horizontal_scroll_state;
                            new_state.set_offset(new_state.offset() + d);
                            callback(new_state);
                        }
                        last_calculated_h_speed_time.set(now);
                    }
                }
                if v_scrollable && v_speed.get().abs() > 0.0 {
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_calculated_v_speed_time.get());
                    let elapsed_secs = elapsed.as_secs_f32();
                    if elapsed_secs > 0.0 {
                        let speed = v_speed.get();
                        let d = speed * elapsed_secs;
                        let on_change = on_vertical_scroll_state_change.lock();
                        if let Some(callback) = &*on_change {
                            let mut new_state = vertical_scroll_state;
                            new_state.set_offset(new_state.offset() + d);
                            println!("Auto scroll v by {} to offset {}", d, new_state.offset());
                            callback(new_state);
                        }
                        last_calculated_v_speed_time.set(now);
                    }
                }*/
            }
        })
        .set_mouse_wheel({
            // clone!(
            //     props.horizontal_scrollable,
            //     props.vertical_scrollable,
            //     props.horizontal_scroll_state,
            //     props.vertical_scroll_state,
            //     props.on_horizontal_scroll_state_change,
            //     props.on_vertical_scroll_state_change,
            //     // v_speed,
            //     // h_speed,
            //     // last_calculated_v_speed_time,
            //     // last_calculated_h_speed_time
            // );
            let mut current_v_speed = 0.0;
            let mut current_h_speed = 0.0;
            let mut last_x_offset = 0.0;
            let mut last_y_offset = 0.0;
            use |item, x_wheel, y_wheel| {
                let vertical_scrollable = vertical_scrollable.get();
                let horizontal_scrollable = horizontal_scrollable.get();
                let mut v_scroll_state = vertical_scroll_state.get();
                let mut h_scroll_state = horizontal_scroll_state.get();
                let (cursor_x, cursor_y) = item.window_context().cursor_position().get();
                let item_frame = item.current_frame();
                if !item_frame.contains(cursor_x, cursor_y) {
                    return (x_wheel, y_wheel);
                }

                if let Some(MouseWheel{delta: MouseScrollDelta::LogicalDelta(delta), phase,..}) = x_wheel {
                    match phase {
                        TouchPhase::Started => {
                            last_calculated_h_speed_time.set(Instant::now());
                        }
                        TouchPhase::Moved => {
                            let now = Instant::now();
                            let elapsed = now.duration_since(last_calculated_h_speed_time.get());
                            let elapsed_secs = elapsed.as_secs_f32();
                            if elapsed_secs > 0.0 {
                                current_h_speed = delta / elapsed_secs;
                                last_calculated_h_speed_time.set(now);
                            }
                        }
                        TouchPhase::Ended => {
                            last_calculated_h_speed_time.set(Instant::now());
                            h_speed.set(current_h_speed);
                        }
                        TouchPhase::Cancelled => {}
                    }
                }

                if let Some(MouseWheel{delta: MouseScrollDelta::LogicalDelta(delta), phase,..}) = y_wheel {
                    match phase {
                        TouchPhase::Started => {
                            last_calculated_v_speed_time.set(Instant::now());
                            last_y_offset = v_scroll_state.offset();
                        }
                        TouchPhase::Moved => {
                            let now = Instant::now();
                            let elapsed = now.duration_since(last_calculated_v_speed_time.get());
                            let elapsed_secs = elapsed.as_secs_f32();
                            if elapsed_secs > 0.0 {
                                let new_offset = v_scroll_state.offset();
                                let d_offset = new_offset - last_y_offset;
                                current_v_speed = d_offset / elapsed_secs;
                                last_y_offset = new_offset;
                                last_calculated_v_speed_time.set(now);
                            }
                        }
                        TouchPhase::Ended => {
                            last_calculated_v_speed_time.set(Instant::now());
                            v_speed.set(current_v_speed);
                            println!("Set v_speed to {}", current_v_speed);
                        }
                        TouchPhase::Cancelled => {}
                    }
                }

                (
                    if horizontal_scrollable {
                        match x_wheel {
                            Some(MouseWheel{delta,..}) => {
                                let offset = match delta {
                                    MouseScrollDelta::LineDelta(offset) => {
                                        offset * 20.0
                                    }
                                    MouseScrollDelta::LogicalDelta(offset) => {
                                        offset
                                    }
                                };
                                h_scroll_state.set_offset(h_scroll_state.offset() + offset);
                                let on_change = on_horizontal_scroll_state_change.lock();
                                if let Some(callback) = &*on_change {
                                    callback(h_scroll_state);
                                }
                                None
                            }
                            None => { None }
                        }
                    } else {
                        x_wheel
                    }
                    ,
                    if vertical_scrollable {
                        match y_wheel {
                            Some(MouseWheel{delta,..}) => {
                                let offset = match delta {
                                    MouseScrollDelta::LineDelta(offset) => {
                                        offset * 20.0
                                    }
                                    MouseScrollDelta::LogicalDelta(offset) => {
                                        offset
                                    }
                                };
                                v_scroll_state.set_offset(v_scroll_state.offset() + offset);
                                let on_change = on_vertical_scroll_state_change.lock();
                                if let Some(callback) = &*on_change {
                                    callback(v_scroll_state);
                                }
                                None
                            }
                            None => { None }
                        }
                    } else {
                        y_wheel
                    },
                )
            }
        })
}

pub trait DefaultScrollAreaProps {
    fn horizontal_scroll_props(&self) -> ScrollAreaProps;
    fn vertical_scroll_props(&self) -> ScrollAreaProps;
}

impl DefaultScrollAreaProps for &WindowContext {
    fn horizontal_scroll_props(&self) -> ScrollAreaProps {
        let state = ScrollState::new_shared();
        self.scroll_area_props()
            .horizontal_scrollable(true)
            .horizontal_scroll_state(&state)
            .on_horizontal_scroll_state_change({
                let state = state.clone();
                move |new_state: ScrollState| {
                    state.set(new_state);
                }
            })
    }

    fn vertical_scroll_props(&self) -> ScrollAreaProps {
        let state = ScrollState::new_shared();
        self.scroll_area_props()
            .vertical_scrollable(true)
            .vertical_scroll_state(&state)
            .on_vertical_scroll_state_change({
                let state = state.clone();
                move |new_state: ScrollState| {
                    state.set(new_state);
                }
            })
    }
}