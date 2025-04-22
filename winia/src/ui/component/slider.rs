use crate::shared::{Children, Gettable, Settable, Shared, SharedColor, SharedF32};
use crate::ui::app::WindowContext;
use crate::ui::item::{DisplayParameter, HorizontalAlignment, LogicalX, MouseScrollDelta, Size};
use crate::ui::theme::color;
use crate::ui::Item;
use crate::{impl_property_layout, impl_property_redraw};
use proc_macro::item;
use skia_safe::{Canvas, Color, Paint, RRect, Rect, Shader, Vector};
use std::ops::{Add, Div, Mul, Sub};

#[derive(Clone)]
struct SliderProperty {
    value: Shared<f32>,
    max: Shared<f32>,
    min: Shared<f32>,
    division: Shared<f32>,
}

#[item(min: impl Into<SharedF32>,
        max: impl Into<SharedF32>,
        value: impl Into<SharedF32>)]
pub struct Slider {
    item: Item,
    property: Shared<SliderProperty>,
}

struct Handle {
    color: Color,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Handle {
    pub fn new() -> Self {
        Handle {
            color: Color::TRANSPARENT,
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 44.0,
        }
    }
    pub fn set_center_x(&mut self, x: f32) {
        self.x = x - self.width / 2.0;
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    pub fn set_center_y(&mut self, y: f32) {
        self.y = y - self.height / 2.0;
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    pub fn draw(&self, canvas: &Canvas, paint: &mut Paint) {
        paint.set_color(self.color);
        let width = self.width - 12.0;
        let radius = width / 2.0;
        let radius = Vector::new(radius, radius);
        let rect = RRect::new_rect_radii(
            Rect::from_xywh(self.x + 6.0, self.y, width, self.height),
            &[radius, radius, radius, radius],
        );
        canvas.draw_rrect(rect, paint);
    }

    pub fn from_parameter(display_parameter: &DisplayParameter) -> Self {
        let color = display_parameter
            .get_color_param("handle_color")
            .unwrap_or(Color::TRANSPARENT);
        let width = display_parameter
            .get_float_param("handle_width")
            .unwrap_or(16.0);
        let height = display_parameter
            .get_float_param("handle_height")
            .unwrap_or(44.0);
        let x = display_parameter.get_float_param("handle_x").unwrap_or(0.0);
        let y = display_parameter.get_float_param("handle_y").unwrap_or(0.0);
        Handle {
            color,
            x,
            y,
            width,
            height,
        }
    }

    pub fn save(&self, display_parameter: &mut DisplayParameter) {
        display_parameter.set_color_param("handle_color", self.color);
        display_parameter.set_float_param("handle_width", self.width);
        display_parameter.set_float_param("handle_height", self.height);
        display_parameter.set_float_param("handle_x", self.x);
        display_parameter.set_float_param("handle_y", self.y);
    }
}

struct Track {
    active: bool,
    color: Color,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Track {
    pub fn new(active: bool) -> Self {
        Track {
            active,
            color: Color::TRANSPARENT,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 16.0,
        }
    }

    pub fn set_center_x(&mut self, x: f32) {
        self.x = x - self.width / 2.0;
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    pub fn set_center_y(&mut self, y: f32) {
        self.y = y - self.height / 2.0;
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    pub fn draw(&self, canvas: &Canvas, paint: &mut Paint) {
        if self.width <= 0.0 {
            return;
        }
        paint.set_color(self.color);
        let radius = self.height / 2.0;
        let radius = if self.active {
            [
                Vector::new(radius, radius),
                Vector::new(4.0, 4.0),
                Vector::new(4.0, 4.0),
                Vector::new(radius, radius),
            ]
        } else {
            [
                Vector::new(4.0, 4.0),
                Vector::new(radius, radius),
                Vector::new(radius, radius),
                Vector::new(4.0, 4.0),
            ]
        };
        let rect = RRect::new_rect_radii(
            Rect::from_xywh(self.x, self.y, self.width, self.height),
            &radius,
        );
        canvas.draw_rrect(rect, paint);
    }

    pub fn from_parameter(active: bool, display_parameter: &DisplayParameter) -> Self {
        let active_str = if active { "active" } else { "inactive" };
        let color = display_parameter
            .get_color_param(&format!("{}_track_color", active_str))
            .unwrap_or(Color::TRANSPARENT);
        let width = display_parameter
            .get_float_param(&format!("{}_track_width", active_str))
            .unwrap_or(0.0);
        let height = display_parameter
            .get_float_param(&format!("{}_track_height", active_str))
            .unwrap_or(16.0);
        let x = display_parameter
            .get_float_param(&format!("{}_track_x", active_str))
            .unwrap_or(0.0);
        let y = display_parameter
            .get_float_param(&format!("{}_track_y", active_str))
            .unwrap_or(0.0);
        Track {
            active,
            color,
            x,
            y,
            width,
            height,
        }
    }

    pub fn save(&self, display_parameter: &mut DisplayParameter) {
        let active_str = if self.active { "active" } else { "inactive" };
        display_parameter.set_color_param(format!("{}_track_color", active_str), self.color);
        display_parameter.set_float_param(format!("{}_track_width", active_str), self.width);
        display_parameter.set_float_param(format!("{}_track_height", active_str), self.height);
        display_parameter.set_float_param(format!("{}_track_x", active_str), self.x);
        display_parameter.set_float_param(format!("{}_track_y", active_str), self.y);
    }
}

impl_property_layout!(Slider, min, SharedF32);
impl_property_layout!(Slider, max, SharedF32);
impl_property_layout!(Slider, value, SharedF32);

impl Slider {
    pub fn new(
        window_context: &WindowContext,
        min: impl Into<SharedF32>,
        max: impl Into<SharedF32>,
        value: impl Into<SharedF32>,
    ) -> Self {
        let (active_color, inactive_color) = {
            let theme = window_context.theme().lock();
            (
                theme.get_color(color::PRIMARY).unwrap(),
                theme.get_color(color::SECONDARY_CONTAINER).unwrap(),
            )
        };
        let item = Item::new(window_context, Children::new())
            .width(Size::Fill)
            .height(Size::Fixed(44.0));
        let id = item.data().get_id();
        let event_loop_proxy = window_context.event_loop_proxy().clone();
        let property = Shared::from(SliderProperty {
            value: value.into().layout_when_changed(&event_loop_proxy, id),
            max: max.into().layout_when_changed(&event_loop_proxy, id),
            min: min.into().layout_when_changed(&event_loop_proxy, id),
            division: Shared::from(1.0),
        });

        {
            let property = property.lock();
            let value = property.value.clone();
            let max = property.max.clone();
            let min = property.min.clone();
            value.set_filter({
                let max = max.clone();
                let min = min.clone();
                move |value| {
                    let max = max.get();
                    let min = min.get();
                    Some(value.clamp(min, max))
                }
            });
        }

        item.data()
            .set_layout({
                let property = property.clone();
                move |item, width, height| {
                    let property = property.lock();
                    let padding_start = item.get_padding_start().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_bottom = item.get_padding_bottom().get();

                    let align_content = item.get_align_content().get();
                    let y = match align_content.to_horizontal_alignment() {
                        HorizontalAlignment::Start => padding_top,
                        HorizontalAlignment::Center => {
                            (padding_top + height - padding_bottom) / 2.0
                        }
                        HorizontalAlignment::End => height - padding_bottom,
                    };

                    let layout_direction = item.get_layout_direction().get();
                    let mut x = match align_content.to_horizontal_alignment() {
                        HorizontalAlignment::Start => {
                            LogicalX::new(layout_direction, padding_start, width)
                        }
                        HorizontalAlignment::Center => LogicalX::new(
                            layout_direction,
                            (padding_start + width - padding_end) / 2.0,
                            width,
                        ),
                        HorizontalAlignment::End => {
                            LogicalX::new(layout_direction, width - padding_end, width)
                        }
                    };

                    let value = property.value.get();
                    let max = property.max.get();
                    let min = property.min.get();
                    let percent = (value - min) / (max - min);

                    let slider_width = width - padding_start - padding_end;

                    let active_track_width = slider_width * percent - 16.0 / 2.0;
                    let handle_width = 16.0;
                    let inactive_track_width = slider_width - active_track_width - handle_width;

                    let mut active_track = Track::new(true);
                    active_track.x = x.physical_value(active_track_width);
                    active_track.set_center_y(y + 44.0 / 2.0);
                    active_track.width = active_track_width;
                    active_track.color = active_color;
                    x += active_track_width;

                    let mut handle = Handle::new();
                    x += handle_width / 2.0;
                    handle.set_center_x(x.physical_value(handle_width));
                    handle.y = y;
                    handle.width = handle_width;
                    handle.color = active_color;

                    let mut inactive_track = Track::new(false);
                    x += handle_width / 2.0;
                    inactive_track.x = x.physical_value(inactive_track_width);
                    inactive_track.set_center_y(y + 44.0 / 2.0);
                    inactive_track.width = inactive_track_width;
                    inactive_track.color = inactive_color;

                    let target_parameter = item.get_target_parameter();
                    active_track.save(target_parameter);
                    inactive_track.save(target_parameter);
                    handle.save(target_parameter);
                }
            })
            .set_draw({
                // let property = property.clone();
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(skia_safe::paint::Style::Fill);
                move |item, canvas| {
                    let display_parameter = item.get_display_parameter();
                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let mut active_track = Track::from_parameter(true, &display_parameter);
                    active_track.x += x;
                    active_track.y += y;
                    let mut inactive_track = Track::from_parameter(false, &display_parameter);
                    inactive_track.x += x;
                    inactive_track.y += y;
                    let mut handle = Handle::from_parameter(&display_parameter);
                    handle.x += x;
                    handle.y += y;

                    active_track.draw(canvas, &mut paint);
                    inactive_track.draw(canvas, &mut paint);
                    handle.draw(canvas, &mut paint);
                }
            })
            .set_pointer_input({
                let property = property.clone();
                move |item, input| {
                    let property = property.lock();
                    let display_parameter = item.get_display_parameter();
                    let x = input.x - display_parameter.x();
                    let width = display_parameter.width;
                    let percent = x / width;
                    let min = property.min.get();
                    let max = property.max.get();
                    let value = min + (max - min) * percent;
                    if value < min {
                        property.value.set(min);
                    } else if value > max {
                        property.value.set(max);
                    } else {
                        property.value.set(value);
                    }
                    property.value.notify();
                }
            }).set_mouse_wheel({
                let property = property.clone();
                move |item, input| {
                    let property = property.lock();
                    let min = property.min.get();
                    let max = property.max.get();
                    let offset = (max - min) / 100.0;
                    let offset = match input.delta {
                        MouseScrollDelta::LineDelta(_, y) => { 
                            if y > 0.0 {
                                1.0 * offset
                            } else {
                                -1.0 * offset
                            }
                        }
                        MouseScrollDelta::LogicalDelta(_, y) => {
                            if y > 0.0 {
                                -1.0 * offset
                            } else {
                                1.0 * offset
                            }
                        }
                    };
                    let value = property.value.get();
                    property.value.set(value + offset);
                    true
                }
        });

        Self { item, property }
    }
}
