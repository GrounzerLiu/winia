use std::ops::{Deref, DerefMut};
use crate::shared::{SharedDerived, SharedDerivedF32, SharedDerivedUsize, SharedSource};
use crate::theme::shape::Corner;
use crate::ui::item::{Children, Frame, ItemEvent, ItemKind, ItemProps, LayoutDirection, MeasureMode, PhysicalX};
use crate::ui::widget::slider::slider_style::SliderStyleExt;
use crate::ui::{rectangle, stack, Alignment, Color, Item, Orientation, Radius, RectanglePropsTrait, SetColor, Size, StackProps, StackPropsTrait};
use crate::{depend, shared_derived};
use clonelet::clone;
use skia_safe::{Paint, RRect};
use proc_macro::ItemProps;
use crate::core::next_id;
use crate::event::ElementState;

#[derive(ItemProps)]
pub struct SliderProps {
    pub item_props: ItemProps,
    #[constructor]
    pub value: SharedDerivedF32,
    #[constructor]
    pub min: SharedDerivedF32,
    #[constructor]
    pub max: SharedDerivedF32,
    pub step: SharedDerivedUsize,
    #[not_shared]
    #[constructor(type = impl FnMut(f32) + 'static)]
    pub on_value_change: SharedSource<Box<dyn FnMut(f32)>>,
    #[not_shared]
    pub handle: Option<Box<dyn FnMut(&SliderProps) -> Item>>,
    #[not_shared]
    pub track: Option<Box<dyn FnMut(&SliderProps) -> Item>>,

    pub stop_indicator_size: SharedDerivedF32,
    pub stop_indicator_shape: SharedDerived<Corner>,
    pub stop_indicator_trailing_space: SharedDerivedF32,
    pub stop_indicator_color: SharedDerived<Color>,
    pub stop_indicator_color_selected: SharedDerived<Color>,

    pub active_stop_indicator_container_color: SharedDerived<Color>,
    pub active_stop_indicator_container_opacity: SharedDerivedF32,
    pub inactive_stop_indicator_container_color: SharedDerived<Color>,
    pub inactive_stop_indicator_container_opacity: SharedDerivedF32,

    pub active_track_height: SharedDerivedF32,
    pub inactive_track_height: SharedDerivedF32,
    pub active_track_shape: SharedDerived<Corner>,
    pub active_track_outer_corner_size: SharedDerived<Corner>,
    pub active_track_inner_corner_size: SharedDerived<Corner>,
    pub inactive_track_shape: SharedDerived<Corner>,
    pub active_track_color: SharedDerived<Color>,
    pub active_track_opacity: SharedDerivedF32,
    pub inactive_track_color: SharedDerived<Color>,
    pub inactive_track_opacity: SharedDerivedF32,

    pub handle_height: SharedDerivedF32,
    pub handle_width: SharedDerivedF32,
    pub handle_shape: SharedDerived<Corner>,
    pub handle_color: SharedDerived<Color>,
    pub handle_opacity: SharedDerivedF32,
    pub active_handle_color: SharedDerived<Color>,
    pub active_handle_height: SharedDerivedF32,
    pub active_handle_width: SharedDerivedF32,
    pub active_handle_shape: SharedDerived<Corner>,
    pub active_handle_leading_space: SharedDerivedF32,
    pub active_handle_trailing_space: SharedDerivedF32,
    pub active_handle_padding: SharedDerivedF32,

    pub value_indicator_container_color: SharedDerived<Color>,
    pub value_indicator_label_font: SharedDerived<String>,
    pub value_indicator_label_font_color: SharedDerived<Color>,
    pub value_indicator_label_line_height: SharedDerivedF32,
    pub value_indicator_label_size: SharedDerivedF32,
    pub value_indicator_label_tracking: SharedDerivedF32,
    pub value_indicator_label_weight: SharedDerivedUsize,
    pub value_indicator_active_bottom_space: SharedDerivedF32,
}
macro_rules! get_style {
    ($get_style_method:ident, $theme:ident, $item_state:ident) => {
        SharedDerived::from_fn(
            depend!($theme, $item_state),
            {
                clone!($theme, $item_state);
                move || {
                    let theme = $theme.lock();
                    theme.get_slider_style("slider_style", $item_state.get()).unwrap()
                        .$get_style_method(&theme).cloned().unwrap()
                }
            }
        )
    }
}
impl SliderProps {
    pub fn new(
        mut item_props: ItemProps,
        value: impl Into<SharedDerivedF32>,
        min: impl Into<SharedDerivedF32>,
        max: impl Into<SharedDerivedF32>,
        on_value_change: impl FnMut(f32) + 'static,
    ) -> Self {
        item_props.width = SharedDerived::new_derived(Size::Fixed(100.0));
        let theme = item_props.window_context.theme().clone();
        let item_state = item_props.item_state.clone();
        let stop_indicator_size = get_style!(get_stop_indicator_size, theme, item_state);
        let stop_indicator_shape = get_style!(get_stop_indicator_shape, theme, item_state);
        let stop_indicator_trailing_space = get_style!(get_stop_indicator_trailing_space, theme, item_state);
        let stop_indicator_color = get_style!(get_stop_indicator_color, theme, item_state);
        let stop_indicator_color_selected = get_style!(get_stop_indicator_color_selected, theme, item_state);
        let active_stop_indicator_container_color = get_style!(get_active_stop_indicator_container_color, theme, item_state);
        let active_stop_indicator_container_opacity = get_style!(get_active_stop_indicator_container_opacity, theme, item_state);
        let inactive_stop_indicator_container_color = get_style!(get_inactive_stop_indicator_container_color, theme, item_state);
        let inactive_stop_indicator_container_opacity = get_style!(get_inactive_stop_indicator_container_opacity, theme, item_state);
        let active_track_height = get_style!(get_active_track_height, theme, item_state);
        let inactive_track_height = get_style!(get_inactive_track_height, theme, item_state);
        let active_track_shape = get_style!(get_active_track_shape, theme, item_state);
        let active_track_outer_corner_size = get_style!(get_active_track_outer_corner_size, theme, item_state);
        let active_track_inner_corner_size = get_style!(get_active_track_inner_corner_size, theme, item_state);
        let inactive_track_shape = get_style!(get_inactive_track_shape, theme, item_state);
        let active_track_color = get_style!(get_active_track_color, theme, item_state);
        let active_track_opacity = get_style!(get_active_track_opacity, theme, item_state);
        let inactive_track_color = get_style!(get_inactive_track_color, theme, item_state);
        let inactive_track_opacity = get_style!(get_inactive_track_opacity, theme, item_state);
        let handle_height = get_style!(get_handle_height, theme, item_state);
        let handle_width = get_style!(get_handle_width, theme, item_state);
        let handle_shape = get_style!(get_handle_shape, theme, item_state);
        let handle_color = get_style!(get_handle_color, theme, item_state);
        let handle_opacity = get_style!(get_handle_opacity, theme, item_state);
        let active_handle_color = get_style!(get_active_handle_color, theme, item_state);
        let active_handle_height = get_style!(get_active_handle_height, theme, item_state);
        let active_handle_width = get_style!(get_active_handle_width, theme, item_state);
        let active_handle_shape = get_style!(get_active_handle_shape, theme, item_state);
        let active_handle_leading_space = get_style!(get_active_handle_leading_space, theme, item_state);
        let active_handle_trailing_space = get_style!(get_active_handle_trailing_space, theme, item_state);
        let active_handle_padding = get_style!(get_active_handle_padding, theme, item_state);
        let value_indicator_container_color = get_style!(get_value_indicator_container_color, theme, item_state);
        let value_indicator_label_font = get_style!(get_value_indicator_label_font, theme, item_state);
        let value_indicator_label_font_color = get_style!(get_value_indicator_label_font_color, theme, item_state);
        let value_indicator_label_line_height = get_style!(get_value_indicator_label_line_height, theme, item_state);
        let value_indicator_label_size = get_style!(get_value_indicator_label_size, theme, item_state);
        let value_indicator_label_tracking = get_style!(get_value_indicator_label_tracking, theme, item_state);
        let value_indicator_label_weight = get_style!(get_value_indicator_label_weight, theme, item_state);
        let value_indicator_active_bottom_space = get_style!(get_value_indicator_active_bottom_space, theme, item_state);

        Self {
            item_props,
            value: value.into(),
            min: min.into(),
            max: max.into(),
            step: 0.into(),
            on_value_change: SharedSource::new(Box::new(on_value_change)),
            handle: None,
            track: None,
            stop_indicator_size,
            stop_indicator_shape,
            stop_indicator_trailing_space,
            stop_indicator_color,
            stop_indicator_color_selected,
            active_stop_indicator_container_color,
            active_stop_indicator_container_opacity,
            inactive_stop_indicator_container_color,
            inactive_stop_indicator_container_opacity,
            active_track_height,
            inactive_track_height,
            active_track_shape,
            active_track_outer_corner_size,
            active_track_inner_corner_size,
            inactive_track_shape,
            active_track_color,
            active_track_opacity,
            inactive_track_color,
            inactive_track_opacity,
            handle_height,
            handle_width,
            handle_shape,
            handle_color,
            handle_opacity,
            active_handle_color,
            active_handle_height,
            active_handle_width,
            active_handle_shape,
            active_handle_leading_space,
            active_handle_trailing_space,
            active_handle_padding,
            value_indicator_container_color,
            value_indicator_label_font,
            value_indicator_label_font_color,
            value_indicator_label_line_height,
            value_indicator_label_size,
            value_indicator_label_tracking,
            value_indicator_label_weight,
            value_indicator_active_bottom_space,
        }
    }

    pub fn handle(
        mut self,
        handle: impl FnMut(&SliderProps) -> Item + 'static,
    ) -> Self {
        self.handle = Some(Box::new(handle));
        self
    }

    pub fn track(
        mut self,
        track: impl FnMut(&SliderProps) -> Item + 'static,
    ) -> Self {
        self.track = Some(Box::new(track));
        self
    }
}

fn calculate_progress(value: f32, min: f32, max: f32) -> f32 {
    if max - min == 0.0 {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

pub fn slider(mut props: SliderProps) -> Item {
    let w = props.window_context.clone();
    let handle = props.handle.take().map_or(
        {
            rectangle(
                w.rectangle_props(&props.handle_color)
                 .height(shared_derived!(props.handle_height|| {
                        Size::Fixed(handle_height.get())
                    }))
                 .width(shared_derived!(props.handle_width|| {
                        Size::Fixed(handle_width.get())
                    }))
                 .opacity(&props.handle_opacity)
                 .radius(
                     Radius::new()
                         .top_start(shared_derived!(props.handle_shape|| {
                                handle_shape.lock().top_start
                            }))
                         .top_end(shared_derived!(props.handle_shape|| {
                                handle_shape.lock().top_end
                            }))
                         .bottom_start(shared_derived!(props.handle_shape|| {
                                handle_shape.lock().bottom_start
                            }))
                         .bottom_end(shared_derived!(props.handle_shape|| {
                                handle_shape.lock().bottom_end
                            }))
                 )
            )
        },
        |mut handle| {
            handle.deref_mut()(&props)
        },
    );
    let track = props.track.take().map_or(
        {
            track(&props)
            // rectangle(w.rectangle_props(Color::RED))
        },
        |mut track| {
            track.deref_mut()(&props)
        },
    );
    let pressed = SharedSource::new(false);
    let item_event = ItemEvent::new()
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
        }).set_layout({
        clone!(
            props.padding,
            props.layout_direction,
            props.value,
            props.min,
            props.max,
            props.active_handle_trailing_space,
            props.active_handle_leading_space,
        );
        move |item, width, height| {
            let padding_start = padding.start.get();
            let padding_end = padding.end.get();
            let padding_top = padding.top.get();
            let padding_bottom = padding.bottom.get();
            let layout_direction = layout_direction.get();

            let children = item.children().lock();
            let handle = &children[0];
            let mut handle_data = handle.data();
            let handle_width = handle_data.measure_frame.width;
            let handle_height = handle_data.measure_frame.height;
            let progress = calculate_progress(
                value.get(),
                min.get(),
                max.get(),
            );
            let handle_leading_space = active_handle_leading_space.get();
            let handle_trailing_space = active_handle_trailing_space.get();
            let handle_track_space = width - padding_start - padding_end - handle_width
                - handle_leading_space
                - handle_trailing_space;
            let handle_x = padding_start + handle_leading_space + handle_track_space * progress;
            let handle_y = padding_top + (height - padding_top - padding_bottom - handle_height) / 2.0;
            handle_data.dispatch_layout(
                handle_x.physical_x(layout_direction, width, handle_width),
                handle_y,
                handle_width,
                handle_height,
            );
            drop(handle_data);

            let track = &children[1];
            let mut track_data = track.data();
            let track_width = width - padding_start - padding_end;
            let track_height = track_data.measure_frame.height;
            let track_x = padding_start;
            let track_y = padding_top + (height - padding_top - padding_bottom - track_height) / 2.0;
            track_data.dispatch_layout(
                track_x.physical_x(layout_direction, width, track_width),
                track_y,
                track_width,
                track_height,
            );
        }
    })
        .set_pointer_button({
            clone!(
                pressed,
                props.value,
                props.min,
                props.max,
                props.on_value_change,
                props.handle_width,
                props.active_handle_trailing_space,
                props.active_handle_leading_space,
            );
            move |item, pointer_button| {
                if pointer_button.primary && item.enable.get() {
                    let current_frame = item.current_frame();
                    let width = current_frame.width();
                    let padding_start = item.padding.start.get();
                    let padding_end = item.padding.end.get();
                    let handle_width = handle_width.get();
                    let leading_space = current_frame.get_float_param("handle_leading_space").unwrap_or(active_handle_leading_space.get());
                    let trailing_space = current_frame.get_float_param("handle_trailing_space").unwrap_or(active_handle_trailing_space.get());
                    let available_width = width - padding_start - padding_end - handle_width - leading_space - trailing_space;
                    let offset_x = pointer_button.position.x - current_frame.x() - padding_start - leading_space - (handle_width / 2.0);
                    let progress = (offset_x / available_width).clamp(0.0, 1.0);
                    let new_value = min.get() + (max.get() - min.get()) * progress;
                    let mut on_value_change = on_value_change.lock();
                    on_value_change(new_value);
                    match pointer_button.state {
                        ElementState::Pressed => {
                            pressed.set(true);
                            return true;
                        }
                        ElementState::Released => {
                            pressed.set(false);
                            return true;
                        }
                    }
                }
                false
            }
        })
        .set_pointer_moved({
            clone!(
                props.value,
                props.min,
                props.max,
                props.on_value_change,
                props.handle_width,
                props.active_handle_trailing_space,
                props.active_handle_leading_space,
                pressed
            );
            move |item, pointer_moved| {
                if !pointer_moved.primary || !item.enable.get() || !pressed.get() {
                    return;
                }
                let current_frame = item.current_frame();
                let width = current_frame.width();
                let padding_start = item.padding.start.get();
                let padding_end = item.padding.end.get();
                let handle_width = current_frame.get_float_param("handle_width").unwrap_or(handle_width.get());
                let leading_space = current_frame.get_float_param("handle_leading_space").unwrap_or(active_handle_leading_space.get());
                let trailing_space = current_frame.get_float_param("handle_trailing_space").unwrap_or(active_handle_trailing_space.get());
                let available_width = width - padding_start - padding_end - handle_width - leading_space - trailing_space;
                let offset_x = pointer_moved.position.x - current_frame.x() - padding_start - leading_space - (handle_width / 2.0);
                let progress = (offset_x / available_width).clamp(0.0, 1.0);
                let new_value = min.get() + (max.get() - min.get()) * progress;
                let mut on_value_change = on_value_change.lock();
                on_value_change(new_value);
            }
        });
    Item::new(
        ItemKind::Container,
        item_event,
        props,
        Children::from(vec![
            handle,
            track,
        ]),
    )
}

pub fn track(props: &SliderProps) -> Item {
    struct TrackLayoutContext {
        active_track_height: f32,
        inactive_track_height: f32,
        active_track_shape_top_start: f32,
        active_track_shape_top_end: f32,
        active_track_shape_bottom_start: f32,
        active_track_shape_bottom_end: f32,
        active_track_outer_corner_size_top_start: f32,
        active_track_outer_corner_size_top_end: f32,
        active_track_outer_corner_size_bottom_start: f32,
        active_track_outer_corner_size_bottom_end: f32,
        active_track_inner_corner_size_top_start: f32,
        active_track_inner_corner_size_top_end: f32,
        active_track_inner_corner_size_bottom_start: f32,
        active_track_inner_corner_size_bottom_end: f32,
        inactive_track_shape_top_start: f32,
        inactive_track_shape_top_end: f32,
        inactive_track_shape_bottom_start: f32,
        inactive_track_shape_bottom_end: f32,
        active_track_color: Color,
        active_track_opacity: f32,
        inactive_track_color: Color,
        inactive_track_opacity: f32,
        handle_width: f32,
        handle_leading_space: f32,
        handle_trailing_space: f32,
        stop_indicator_size: f32,
        stop_indicator_shape_top_start: f32,
        stop_indicator_shape_top_end: f32,
        stop_indicator_shape_bottom_start: f32,
        stop_indicator_shape_bottom_end: f32,
        stop_indicator_trailing_space: f32,
        stop_indicator_color: Color,
        stop_indicator_color_selected: Color,
        progress: f32,
    }

    impl TrackLayoutContext {
        pub fn from_shared(
            active_track_height: &SharedDerivedF32,
            inactive_track_height: &SharedDerivedF32,
            active_track_shape: &SharedDerived<Corner>,
            active_track_outer_corner_size: &SharedDerived<Corner>,
            active_track_inner_corner_size: &SharedDerived<Corner>,
            inactive_track_shape: &SharedDerived<Corner>,
            active_track_color: &SharedDerived<Color>,
            active_track_opacity: &SharedDerivedF32,
            inactive_track_color: &SharedDerived<Color>,
            inactive_track_opacity: &SharedDerivedF32,
            handle_width: &SharedDerivedF32,
            handle_leading_space: &SharedDerivedF32,
            handle_trailing_space: &SharedDerivedF32,
            stop_indicator_size: &SharedDerivedF32,
            stop_indicator_shape: &SharedDerived<Corner>,
            stop_indicator_trailing_space: &SharedDerivedF32,
            stop_indicator_color: &SharedDerived<Color>,
            stop_indicator_color_selected: &SharedDerived<Color>,
            progress: f32,
        ) -> Self {
            Self {
                active_track_height: active_track_height.get(),
                inactive_track_height: inactive_track_height.get(),
                active_track_shape_top_start: active_track_shape.get().top_start,
                active_track_shape_top_end: active_track_shape.get().top_end,
                active_track_shape_bottom_start: active_track_shape.get().bottom_start,
                active_track_shape_bottom_end: active_track_shape.get().bottom_end,
                active_track_outer_corner_size_top_start: active_track_outer_corner_size.get().top_start,
                active_track_outer_corner_size_top_end: active_track_outer_corner_size.get().top_end,
                active_track_outer_corner_size_bottom_start: active_track_outer_corner_size.get().bottom_start,
                active_track_outer_corner_size_bottom_end: active_track_outer_corner_size.get().bottom_end,
                active_track_inner_corner_size_top_start: active_track_inner_corner_size.get().top_start,
                active_track_inner_corner_size_top_end: active_track_inner_corner_size.get().top_end,
                active_track_inner_corner_size_bottom_start: active_track_inner_corner_size.get().bottom_start,
                active_track_inner_corner_size_bottom_end: active_track_inner_corner_size.get().bottom_end,
                inactive_track_shape_top_start: inactive_track_shape.get().top_start,
                inactive_track_shape_top_end: inactive_track_shape.get().top_end,
                inactive_track_shape_bottom_start: inactive_track_shape.get().bottom_start,
                inactive_track_shape_bottom_end: inactive_track_shape.get().bottom_end,
                active_track_color: active_track_color.get(),
                active_track_opacity: active_track_opacity.get(),
                inactive_track_color: inactive_track_color.get(),
                inactive_track_opacity: inactive_track_opacity.get(),
                handle_width: handle_width.get(),
                handle_leading_space: handle_leading_space.get(),
                handle_trailing_space: handle_trailing_space.get(),
                stop_indicator_size: stop_indicator_size.get(),
                stop_indicator_shape_top_start: stop_indicator_shape.get().top_start,
                stop_indicator_shape_top_end: stop_indicator_shape.get().top_end,
                stop_indicator_shape_bottom_start: stop_indicator_shape.get().bottom_start,
                stop_indicator_shape_bottom_end: stop_indicator_shape.get().bottom_end,
                stop_indicator_trailing_space: stop_indicator_trailing_space.get(),
                stop_indicator_color: stop_indicator_color.get(),
                stop_indicator_color_selected: stop_indicator_color_selected.get(),
                progress,
            }
        }

        pub fn from_frame(frame: &Frame) -> Option<Self> {
            Some(Self {
                active_track_height: frame.get_float_param("active_track_height")?,
                inactive_track_height: frame.get_float_param("inactive_track_height")?,
                active_track_shape_top_start: frame.get_float_param("active_track_shape_top_start")?,
                active_track_shape_top_end: frame.get_float_param("active_track_shape_top_end")?,
                active_track_shape_bottom_start: frame.get_float_param("active_track_shape_bottom_start")?,
                active_track_shape_bottom_end: frame.get_float_param("active_track_shape_bottom_end")?,
                active_track_outer_corner_size_top_start: frame.get_float_param("active_track_outer_corner_size_top_start")?,
                active_track_outer_corner_size_top_end: frame.get_float_param("active_track_outer_corner_size_top_end")?,
                active_track_outer_corner_size_bottom_start: frame.get_float_param("active_track_outer_corner_size_bottom_start")?,
                active_track_outer_corner_size_bottom_end: frame.get_float_param("active_track_outer_corner_size_bottom_end")?,
                active_track_inner_corner_size_top_start: frame.get_float_param("active_track_inner_corner_size_top_start")?,
                active_track_inner_corner_size_top_end: frame.get_float_param("active_track_inner_corner_size_top_end")?,
                active_track_inner_corner_size_bottom_start: frame.get_float_param("active_track_inner_corner_size_bottom_start")?,
                active_track_inner_corner_size_bottom_end: frame.get_float_param("active_track_inner_corner_size_bottom_end")?,
                inactive_track_shape_top_start: frame.get_float_param("inactive_track_shape_top_start")?,
                inactive_track_shape_top_end: frame.get_float_param("inactive_track_shape_top_end")?,
                inactive_track_shape_bottom_start: frame.get_float_param("inactive_track_shape_bottom_start")?,
                inactive_track_shape_bottom_end: frame.get_float_param("inactive_track_shape_bottom_end")?,
                active_track_color: frame.get_color_param("active_track_color")?,
                active_track_opacity: frame.get_float_param("active_track_opacity")?,
                inactive_track_color: frame.get_color_param("inactive_track_color")?,
                inactive_track_opacity: frame.get_float_param("inactive_track_opacity")?,
                handle_width: frame.get_float_param("handle_width")?,
                handle_leading_space: frame.get_float_param("handle_leading_space")?,
                handle_trailing_space: frame.get_float_param("handle_trailing_space")?,
                stop_indicator_size: frame.get_float_param("stop_indicator_size")?,
                stop_indicator_shape_top_start: frame.get_float_param("stop_indicator_shape_top_start")?,
                stop_indicator_shape_top_end: frame.get_float_param("stop_indicator_shape_top_end")?,
                stop_indicator_shape_bottom_start: frame.get_float_param("stop_indicator_shape_bottom_start")?,
                stop_indicator_shape_bottom_end: frame.get_float_param("stop_indicator_shape_bottom_end")?,
                stop_indicator_trailing_space: frame.get_float_param("stop_indicator_trailing_space")?,
                stop_indicator_color: frame.get_color_param("stop_indicator_color")?,
                stop_indicator_color_selected: frame.get_color_param("stop_indicator_color_selected")?,
                progress: frame.get_float_param("progress")?,
            })
        }

        pub fn write_to_frame(&self, frame: &mut Frame) {
            frame.set_float_param("active_track_height", self.active_track_height);
            frame.set_float_param("inactive_track_height", self.inactive_track_height);
            frame.set_float_param("active_track_shape_top_start", self.active_track_shape_top_start);
            frame.set_float_param("active_track_shape_top_end", self.active_track_shape_top_end);
            frame.set_float_param("active_track_shape_bottom_start", self.active_track_shape_bottom_start);
            frame.set_float_param("active_track_shape_bottom_end", self.active_track_shape_bottom_end);
            frame.set_float_param("active_track_outer_corner_size_top_start", self.active_track_outer_corner_size_top_start);
            frame.set_float_param("active_track_outer_corner_size_top_end", self.active_track_outer_corner_size_top_end);
            frame.set_float_param("active_track_outer_corner_size_bottom_start", self.active_track_outer_corner_size_bottom_start);
            frame.set_float_param("active_track_outer_corner_size_bottom_end", self.active_track_outer_corner_size_bottom_end);
            frame.set_float_param("active_track_inner_corner_size_top_start", self.active_track_inner_corner_size_top_start);
            frame.set_float_param("active_track_inner_corner_size_top_end", self.active_track_inner_corner_size_top_end);
            frame.set_float_param("active_track_inner_corner_size_bottom_start", self.active_track_inner_corner_size_bottom_start);
            frame.set_float_param("active_track_inner_corner_size_bottom_end", self.active_track_inner_corner_size_bottom_end);
            frame.set_float_param("inactive_track_shape_top_start", self.inactive_track_shape_top_start);
            frame.set_float_param("inactive_track_shape_top_end", self.inactive_track_shape_top_end);
            frame.set_float_param("inactive_track_shape_bottom_start", self.inactive_track_shape_bottom_start);
            frame.set_float_param("inactive_track_shape_bottom_end", self.inactive_track_shape_bottom_end);
            frame.set_color_param("active_track_color", self.active_track_color);
            frame.set_float_param("active_track_opacity", self.active_track_opacity);
            frame.set_color_param("inactive_track_color", self.inactive_track_color);
            frame.set_float_param("inactive_track_opacity", self.inactive_track_opacity);
            frame.set_float_param("handle_width", self.handle_width);
            frame.set_float_param("handle_leading_space", self.handle_leading_space);
            frame.set_float_param("handle_trailing_space", self.handle_trailing_space);
            frame.set_float_param("stop_indicator_size", self.stop_indicator_size);
            frame.set_float_param("stop_indicator_shape_top_start", self.stop_indicator_shape_top_start);
            frame.set_float_param("stop_indicator_shape_top_end", self.stop_indicator_shape_top_end);
            frame.set_float_param("stop_indicator_shape_bottom_start", self.stop_indicator_shape_bottom_start);
            frame.set_float_param("stop_indicator_shape_bottom_end", self.stop_indicator_shape_bottom_end);
            frame.set_float_param("stop_indicator_trailing_space", self.stop_indicator_trailing_space);
            frame.set_color_param("stop_indicator_color", self.stop_indicator_color);
            frame.set_color_param("stop_indicator_color_selected", self.stop_indicator_color_selected);
            frame.set_float_param("progress", self.progress);
        }
    }

    let pressed = SharedSource::new(false);
    let item_event = ItemEvent::new()
        .set_layout({
            clone!(
                props.active_track_height,
                props.inactive_track_height,
                props.active_track_shape,
                props.active_track_outer_corner_size,
                props.active_track_inner_corner_size,
                props.inactive_track_shape,
                props.active_track_color,
                props.active_track_opacity,
                props.inactive_track_color,
                props.inactive_track_opacity,
                props.handle_width,
                props.active_handle_leading_space,
                props.active_handle_trailing_space,
                props.stop_indicator_size,
                props.stop_indicator_shape,
                props.stop_indicator_trailing_space,
                props.stop_indicator_color,
                props.stop_indicator_color_selected,
                props.value,
                props.min,
                props.max,
            );
            move |item, width, height| {
                let progress = calculate_progress(
                    value.get(),
                    min.get(),
                    max.get(),
                );
                let layout_context = TrackLayoutContext::from_shared(
                    &active_track_height,
                    &inactive_track_height,
                    &active_track_shape,
                    &active_track_outer_corner_size,
                    &active_track_inner_corner_size,
                    &inactive_track_shape,
                    &active_track_color,
                    &active_track_opacity,
                    &inactive_track_color,
                    &inactive_track_opacity,
                    &handle_width,
                    &active_handle_leading_space,
                    &active_handle_trailing_space,
                    &stop_indicator_size,
                    &stop_indicator_shape,
                    &stop_indicator_trailing_space,
                    &stop_indicator_color,
                    &stop_indicator_color_selected,
                    progress,
                );
                let frame = &mut item.target_frame;
                layout_context.write_to_frame(frame);
            }
        })
        .set_draw({
            clone!();
            move |item, canvas| {
                let direction = item.layout_direction.get();
                let current_frame = item.current_frame();
                let layout_context = TrackLayoutContext::from_frame(&current_frame);
                if layout_context.is_none() {
                    return;
                }
                let layout_context = layout_context.unwrap();

                let width = current_frame.width();
                let height = current_frame.height();
                let padding_start = item.padding.start.get();
                let padding_end = item.padding.end.get();
                let remaining_width = width
                    - padding_start
                    - padding_end
                    - layout_context.handle_width
                    - layout_context.handle_leading_space
                    - layout_context.handle_trailing_space;
                // Active track
                let active_track_width = remaining_width * layout_context.progress;
                let active_track_height = layout_context.active_track_height;
                let active_track_x = padding_start;
                let active_track_y = (height - active_track_height) / 2.0;

                // Inactive track
                let inactive_track_width = remaining_width - active_track_width;
                let inactive_track_height = layout_context.inactive_track_height;
                let inactive_track_x = active_track_x
                    + active_track_width
                    + layout_context.handle_leading_space
                    + layout_context.handle_width
                    + layout_context.handle_trailing_space;
                let inactive_track_y = (height - inactive_track_height) / 2.0;

                fn physical_rrect_radii(
                    direction: LayoutDirection,
                    top_start: f32,
                    top_end: f32,
                    bottom_start: f32,
                    bottom_end: f32,
                ) -> [skia_safe::Vector; 4] {
                    match direction {
                        LayoutDirection::LTR => [
                            skia_safe::Vector::new(top_start, top_start),
                            skia_safe::Vector::new(top_end, top_end),
                            skia_safe::Vector::new(bottom_end, bottom_end),
                            skia_safe::Vector::new(bottom_start, bottom_start),
                        ],
                        LayoutDirection::RTL => [
                            skia_safe::Vector::new(top_end, top_end),
                            skia_safe::Vector::new(top_start, top_start),
                            skia_safe::Vector::new(bottom_start, bottom_start),
                            skia_safe::Vector::new(bottom_end, bottom_end),
                        ],
                    }
                }

                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                // Draw active track
                let active_track_color = layout_context.active_track_color.with_a_f(layout_context.active_track_opacity);
                paint.set_any_color(active_track_color);
                let active_track_rect = skia_safe::Rect::from_xywh(
                    active_track_x.physical_x(direction, width, active_track_width) + current_frame.x(),
                    active_track_y + current_frame.y(),
                    active_track_width,
                    active_track_height,
                );
                let max_radius = active_track_rect.width().min(active_track_rect.height()) / 2.0;
                let active_track_rrect = RRect::new_rect_radii(
                    active_track_rect,
                    &physical_rrect_radii(
                        direction,
                        layout_context.active_track_outer_corner_size_top_start.clamp(0.0, max_radius),
                        layout_context.active_track_inner_corner_size_top_end.clamp(0.0, max_radius),
                        layout_context.active_track_outer_corner_size_bottom_start.clamp(0.0, max_radius),
                        layout_context.active_track_inner_corner_size_bottom_end.clamp(0.0, max_radius),
                    ),
                );
                if active_track_width > 0.0 {
                    canvas.draw_rrect(active_track_rrect, &paint);
                }
                // Draw inactive track
                let inactive_track_color = layout_context.inactive_track_color.with_a_f(layout_context.inactive_track_opacity);
                paint.set_any_color(inactive_track_color);
                let inactive_track_rect = skia_safe::Rect::from_xywh(
                    inactive_track_x.physical_x(direction, width, inactive_track_width) + current_frame.x(),
                    inactive_track_y + current_frame.y(),
                    inactive_track_width,
                    inactive_track_height,
                );
                let max_radius = inactive_track_rect.width().min(inactive_track_rect.height()) / 2.0;
                let inactive_track_rrect = RRect::new_rect_radii(
                    inactive_track_rect,
                    &physical_rrect_radii(
                        direction,
                        layout_context.active_track_inner_corner_size_top_start.clamp(0.0, max_radius),
                        layout_context.active_track_outer_corner_size_top_end.clamp(0.0, max_radius),
                        layout_context.active_track_inner_corner_size_bottom_start.clamp(0.0, max_radius),
                        layout_context.active_track_outer_corner_size_bottom_end.clamp(0.0, max_radius),
                    ),
                );
                if inactive_track_width > 0.0 {
                    canvas.draw_rrect(inactive_track_rrect, &paint);
                }

                let stop_indicator_x = inactive_track_rect.right()
                    - layout_context.stop_indicator_trailing_space
                    - 6.0; // Center of stop indicator
                let stop_indicator_y = (height - layout_context.stop_indicator_size) / 2.0;
                let stop_indicator_rect = skia_safe::Rect::from_xywh(
                    stop_indicator_x.physical_x(direction, width, layout_context.stop_indicator_size) + current_frame.x(),
                    stop_indicator_y + current_frame.y(),
                    layout_context.stop_indicator_size,
                    layout_context.stop_indicator_size,
                );
                let max_radius = stop_indicator_rect.width().min(stop_indicator_rect.height()) / 2.0;
                let stop_indicator_rrect = RRect::new_rect_radii(
                    stop_indicator_rect,
                    &physical_rrect_radii(
                        direction,
                        layout_context.stop_indicator_shape_top_start.clamp(0.0, max_radius),
                        layout_context.stop_indicator_shape_top_end.clamp(0.0, max_radius),
                        layout_context.stop_indicator_shape_bottom_start.clamp(0.0, max_radius),
                        layout_context.stop_indicator_shape_bottom_end.clamp(0.0, max_radius),
                    ),
                );
                paint.set_any_color(layout_context.stop_indicator_color);
                canvas.draw_rrect(stop_indicator_rrect, &paint);
            }
        });

    let p =
        props.window_context.rectangle_props(Color::GREEN)
             .height(shared_derived!(props.inactive_track_height, props.active_track_height|| {
                let active_track_height = active_track_height.get();
                let inactive_track_height = inactive_track_height.get();
                Size::Fixed(active_track_height.max(inactive_track_height))
            }));
    let e = p.window_context.event_loop_proxy().clone();
    let updater = p.item_updater.clone();
    props.value.subscribe(
        next_id(),
        {
            clone!(e, updater);
            move || {
                updater.lock().request_update();
                e.request_update_layout();
            }
        },
    );
    props.min.subscribe(
        next_id(),
        {
            clone!(e, updater);
            move || {
                updater.lock().request_update();
                e.request_update_layout();
            }
        },
    );
    props.max.subscribe(
        next_id(),
        {
            clone!(e, updater);
            move || {
                updater.lock().request_update();
                e.request_update_layout();
            }
        },
    );
    Item::new(
        ItemKind::Widget,
        item_event,
        p,
        Children::new(),
    )
}

pub mod slider_style {
    use crate::theme::shape::Corner;
    use crate::theme::{color, shape, StateStyles, ThemeValue};
    use crate::ui::Color;
    use proc_macro::style;

    #[style]
    pub struct SliderStyle {
        stop_indicator_size: f32,
        stop_indicator_shape: Corner,
        stop_indicator_trailing_space: f32,
        stop_indicator_color: Color,
        stop_indicator_color_selected: Color,

        active_stop_indicator_container_color: Color,
        active_stop_indicator_container_opacity: f32,
        inactive_stop_indicator_container_color: Color,
        inactive_stop_indicator_container_opacity: f32,

        active_track_height: f32,
        inactive_track_height: f32,
        active_track_shape: Corner,
        active_track_outer_corner_size: Corner,
        active_track_inner_corner_size: Corner,
        inactive_track_shape: Corner,
        active_track_color: Color,
        active_track_opacity: f32,
        inactive_track_color: Color,
        inactive_track_opacity: f32,

        handle_height: f32,
        handle_width: f32,
        handle_shape: Corner,
        handle_color: Color,
        handle_opacity: f32,
        active_handle_color: Color,
        active_handle_height: f32,
        active_handle_width: f32,
        active_handle_shape: Corner,
        active_handle_leading_space: f32,
        active_handle_trailing_space: f32,
        active_handle_padding: f32,

        value_indicator_container_color: Color,
        value_indicator_label_font: String,
        value_indicator_label_font_color: Color,
        value_indicator_label_line_height: f32,
        value_indicator_label_size: f32,
        value_indicator_label_tracking: f32,
        value_indicator_label_weight: usize,
        value_indicator_active_bottom_space: f32,
    }

    impl Default for SliderStyle {
        fn default() -> Self {
            Self {
                stop_indicator_size: 4.0.into(),
                stop_indicator_shape: shape::corner::FULL.into(),
                stop_indicator_trailing_space: 4.0.into(),
                stop_indicator_color: color::ON_SECONDARY_CONTAINER.into(),
                stop_indicator_color_selected: color::ON_PRIMARY.into(),

                active_stop_indicator_container_color: color::ON_PRIMARY.into(),
                active_stop_indicator_container_opacity: 1.0.into(),
                inactive_stop_indicator_container_color: color::SECONDARY_CONTAINER.into(),
                inactive_stop_indicator_container_opacity: 1.0.into(),

                active_track_height: 16.0.into(),
                inactive_track_height: 16.0.into(),
                active_track_shape: shape::corner::FULL.into(),
                active_track_outer_corner_size: shape::corner::FULL.into(),
                active_track_inner_corner_size: Corner {
                    top_start: 2.0,
                    top_end: 2.0,
                    bottom_start: 2.0,
                    bottom_end: 2.0,
                }.into(),
                inactive_track_shape: shape::corner::FULL.into(),
                active_track_color: color::PRIMARY.into(),
                active_track_opacity: 1.0.into(),
                inactive_track_color: color::SECONDARY_CONTAINER.into(),
                inactive_track_opacity: 1.0.into(),

                handle_height: 44.0.into(),
                handle_width: 4.0.into(),
                handle_shape: shape::corner::FULL.into(),
                handle_color: color::PRIMARY.into(),
                handle_opacity: 1.0.into(),
                active_handle_color: color::ON_PRIMARY.into(),
                active_handle_height: 44.0.into(),
                active_handle_width: 4.0.into(),
                active_handle_shape: shape::corner::FULL.into(),
                active_handle_leading_space: 6.0.into(),
                active_handle_trailing_space: 6.0.into(),
                active_handle_padding: 2.0.into(),

                value_indicator_container_color: color::INVERSE_SURFACE.into(),
                value_indicator_label_font: ThemeValue::Direct("Roboto".to_string()),
                value_indicator_label_font_color: color::INVERSE_ON_SURFACE.into(),
                value_indicator_label_line_height: 20.0.into(),
                value_indicator_label_size: 14.0.into(),
                value_indicator_label_tracking: 0.5.into(),
                value_indicator_label_weight: ThemeValue::Direct(400),
                value_indicator_active_bottom_space: 12.0.into(),
            }
        }
    }

    pub fn slider_style() -> StateStyles<SliderStyle> {
        StateStyles::enabled(SliderStyle::default())
            .disabled(|style| {
                style.active_stop_indicator_container_color = color::INVERSE_ON_SURFACE.into();
                style.inactive_stop_indicator_container_color = color::INVERSE_ON_SURFACE.into();

                style.active_track_color = color::ON_SURFACE.into();
                style.active_track_opacity = 0.38.into();
                style.inactive_track_color = color::ON_SURFACE.into();
                style.inactive_track_opacity = 0.12.into();

                style.handle_color = color::ON_SURFACE.into();
                style.handle_opacity = 0.38.into();
                style.handle_width = 4.0.into();
            })
            .hovered(|style| {
                style.handle_width = 4.0.into();
            })
            .focused(|style| {
                style.handle_width = 2.0.into();
            })
            .pressed(|style| {
                style.handle_width = 2.0.into();
            })
    }

    pub fn apply_slider_style(theme: &mut crate::theme::Theme) {
        theme.set_style(
            "slider_style",
            Box::new(slider_style()),
        );
    }
}

