use crate::shared::{Children, Gettable, Settable, Shared, SharedF32};
use crate::ui::animation::interpolator::EaseInCubic;
use crate::ui::app::WindowContext;
use crate::ui::item::LogicalX;
use crate::ui::theme::color;
use crate::ui::Item;
use clonelet::clone;
use proc_macro::item;
use skia_safe::paint::Style;
use skia_safe::{Color, Paint, RRect, Rect, Vector};
use std::time::Duration;
use strum_macros::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display)]
pub enum ProgressIndicatorType {
    #[default]
    Circular,
    Linear,
}

pub struct ProgressIndicatorProperty {}

#[item(
    progress_indicator_type: ProgressIndicatorType,
    progress: impl Into<SharedF32>,
    is_determinate: impl Into<Shared<bool>>
)]
pub struct ProgressIndicator {
    item: Item,
}

impl ProgressIndicator {
    pub fn new(
        window_context: &WindowContext,
        progress_indicator_type: ProgressIndicatorType,
        progress: impl Into<SharedF32>,
        is_determinate: impl Into<Shared<bool>>,
    ) -> ProgressIndicator {
        let w = window_context;
        let event_loop_proxy = window_context.event_loop_proxy().clone();
        let mut item = Item::new(w, Children::new());
        let id = item.data().get_id();
        let progress = progress
            .into()
            .layout_when_changed(w.event_loop_proxy(), id);

        let start_position = SharedF32::from(0.0).redraw_when_changed(w.event_loop_proxy(), id);
        let end_position = SharedF32::from(0.0).redraw_when_changed(w.event_loop_proxy(), id);

        let is_determinate = is_determinate.into();
        is_determinate.add_specific_observer(id, {
            clone!(event_loop_proxy, start_position, end_position);
            move |is_determinate| {
                if *is_determinate {
                    if let Some(mut a) = start_position.get_animation() {
                        a.stop()
                    }
                    if let Some(mut a) = end_position.get_animation() {
                        a.stop()
                    }
                } else if progress_indicator_type == ProgressIndicatorType::Circular {
                    if start_position.get_animation().is_none() {
                        start_position.set(0.0);
                        start_position
                            .animation_to_f32(1.0)
                            .enable_repeat()
                            .duration(Duration::from_millis(1000))
                            .start(&event_loop_proxy);
                    }
                    if end_position.get_animation().is_none() {
                        end_position.set(0.0);
                        end_position
                            .animation_to_f32(1.6)
                            .enable_repeat()
                            .duration(Duration::from_millis(3000))
                            .start(&event_loop_proxy);
                    }
                } else {
                    if start_position.get_animation().is_none() {
                        start_position.set(0.0);
                        start_position
                            .animation_to_f32(1.0)
                            .enable_repeat()
                            .duration(Duration::from_millis(1500))
                            .interpolator(EaseInCubic::new())
                            .start(&event_loop_proxy);
                    }
                    if end_position.get_animation().is_none() {
                        end_position.set(0.0);
                        end_position
                            .animation_to_f32(1.0)
                            .enable_repeat()
                            .duration(Duration::from_millis(1500))
                            // .interpolator(EaseOutCubic::new())
                            .start(&event_loop_proxy);
                    }
                }
            }
        });

        item = if progress_indicator_type == ProgressIndicatorType::Circular {
            item.size(40, 40)
        } else {
            item.height(4).min_width(100)
        };

        item.data()
            .set_layout({
                clone!(progress, is_determinate, start_position, end_position);
                move |item, width, height| {
                    let theme = item.get_window_context().theme().lock();
                    let active_indicator_color = *theme.get_color(color::PRIMARY).unwrap();
                    let track_color = *theme.get_color(color::SECONDARY_CONTAINER).unwrap();
                    let stop_indicator_color = *theme.get_color(color::PRIMARY).unwrap();
                    drop(theme);
                    let padding_start = item.get_padding_start().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_bottom = item.get_padding_bottom().get();
                    let direction = item.get_layout_direction().get();
                    let target_parameter = item.get_target_parameter();
                    if progress_indicator_type == ProgressIndicatorType::Circular {
                        target_parameter.set_float_param(
                            "indicator_x",
                            LogicalX::new(direction, padding_start, width).physical_value(40.0),
                        );
                        target_parameter.set_float_param("indicator_y", padding_top);
                        target_parameter.set_float_param("indicator_width", 40.0);
                        target_parameter.set_float_param("indicator_height", 40.0);
                    } else {
                        target_parameter.set_float_param(
                            "indicator_x",
                            LogicalX::new(direction, padding_start, width).physical_value(0.0),
                        );
                        target_parameter.set_float_param("indicator_y", padding_top);
                        target_parameter.set_float_param(
                            "indicator_width",
                            width - padding_start - padding_end,
                        );
                        target_parameter.set_float_param("indicator_height", 4.0)
                    }

                    if is_determinate.get() {
                        start_position.set(0.0);
                        end_position.set(progress.get().clamp(0.0, 1.0));
                    }
                    target_parameter.set_float_param("start_position", start_position.get());
                    target_parameter.set_float_param("end_position", end_position.get());
                    target_parameter
                        .set_color_param("active_indicator_color", active_indicator_color);
                    target_parameter.set_color_param("track_color", track_color);
                    target_parameter.set_color_param("stop_indicator_color", stop_indicator_color);
                }
            })
            .set_draw({
                clone!(is_determinate, start_position, end_position);
                let mut paint = Paint::default();
                paint.set_anti_alias(true);
                paint.set_style(Style::Stroke);
                paint.set_stroke_width(4.0);
                paint.set_stroke_cap(skia_bindings::SkPaint_Cap::Round);
                move |item, canvas| {
                    let display_parameter = item.get_display_parameter();
                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let indicator_x = display_parameter
                        .get_float_param("indicator_x")
                        .unwrap_or(0.0);
                    let indicator_y = display_parameter
                        .get_float_param("indicator_y")
                        .unwrap_or(0.0);
                    let indicator_width = display_parameter
                        .get_float_param("indicator_width")
                        .unwrap_or(0.0);
                    let indicator_height = display_parameter
                        .get_float_param("indicator_height")
                        .unwrap_or(0.0);
                    let start_position_value = display_parameter
                        .get_float_param("start_position")
                        .unwrap_or(0.0);
                    let end_position_value = display_parameter
                        .get_float_param("end_position")
                        .unwrap_or(0.0);
                    let active_indicator_color = display_parameter
                        .get_color_param("active_indicator_color")
                        .unwrap_or(Color::TRANSPARENT);
                    let track_color = display_parameter
                        .get_color_param("track_color")
                        .unwrap_or(Color::TRANSPARENT);
                    let stop_indicator_color = display_parameter
                        .get_color_param("stop_indicator_color")
                        .unwrap_or(Color::TRANSPARENT);

                    if progress_indicator_type == ProgressIndicatorType::Circular {
                        paint.set_style(Style::Stroke);
                        let oval = Rect::from_xywh(
                            x + indicator_x + 2.0,
                            y + indicator_y + 2.0,
                            indicator_width - 4.0,
                            indicator_height - 4.0,
                        );
                        if is_determinate.get() {
                            {
                                // Track
                                let (start_angle, sweep_angle) = if end_position_value == 0.0 {
                                    (0.0, 360.0)
                                } else {
                                    (
                                        end_position_value * 360.0 - 90.0 + 22.92,
                                        (1.0 - end_position_value) * 360.0 - 45.84,
                                    )
                                };
                                if sweep_angle > 0.0 {
                                    paint.set_color(track_color);
                                    canvas.draw_arc(oval, start_angle, sweep_angle, false, &paint);
                                }
                            }
                            {
                                // Indicator
                                let start_angle = start_position_value * 360.0 - 90.0;
                                let sweep_angle =
                                    (end_position_value - start_position_value) * 360.0;
                                paint.set_color(active_indicator_color);
                                canvas.draw_arc(oval, start_angle, sweep_angle, false, &paint);
                            }
                        } else {
                            let start_position_value = start_position.get();
                            let end_position_value = end_position.get();

                            let start_angle = start_position_value * 360.0 - 90.0;
                            let sweep_angle = if end_position_value < 0.8 {
                                end_position_value * 360.0 + 15.0
                            } else {
                                (1.6 - end_position_value) * 360.0 + 15.0
                            };

                            paint.set_color(active_indicator_color);
                            canvas.draw_arc(oval, start_angle, sweep_angle, false, &paint);
                        }
                    } else {
                        paint.set_style(Style::Fill);
                        let x = x + indicator_x;
                        let y = y + indicator_y;
                        let radius = Vector::new(2.0, 2.0);
                        let radius: [Vector; 4] = [radius, radius, radius, radius];
                        if is_determinate.get() {
                            {
                                let not_zero = end_position_value > 0.0;
                                let left = x
                                    + indicator_width * end_position_value
                                    + if not_zero { 4.0 } else { 0.0 };
                                let top = y;
                                let right = left + indicator_width * (1.0 - end_position_value)
                                    - if not_zero { 4.0 } else { 0.0 };
                                let bottom = y + indicator_height;
                                if right > left {
                                    let rect = Rect::from_ltrb(left, top, right, bottom);
                                    let rrect = RRect::new_rect_radii(rect, &radius);
                                    paint.set_color(track_color);
                                    canvas.draw_rrect(rrect, &paint);
                                }

                                if is_determinate.get() {
                                    paint.set_color(stop_indicator_color);
                                    canvas.draw_circle(
                                        (x + indicator_width - 2.0, y + indicator_height / 2.0),
                                        2.0,
                                        &paint,
                                    );
                                }
                            }

                            {
                                let rect = Rect::from_xywh(
                                    x + indicator_width * start_position_value,
                                    y,
                                    indicator_width * (end_position_value - start_position_value),
                                    indicator_height,
                                );
                                let rrect = RRect::new_rect_radii(rect, &radius);
                                paint.set_color(active_indicator_color);
                                canvas.draw_rrect(rrect, &paint);
                            }
                        } else {
                            let start_position_value = start_position.get();
                            let end_position_value = end_position.get();
                            {
                                let rect = Rect::from_xywh(
                                    x,
                                    y,
                                    indicator_width * start_position_value
                                        - if start_position_value < 1.0 { 4.0 } else { 0.0 },
                                    indicator_height,
                                );
                                if rect.width() > 0.0 {
                                    let rrect = RRect::new_rect_radii(rect, &radius);
                                    paint.set_color(track_color);
                                    canvas.draw_rrect(rrect, &paint);
                                }
                            }

                            let end_pos =
                                (
                                    start_position_value 
                                        + if end_position_value < 0.5 {
                                            end_position_value
                                        } else {
                                            1.0 - end_position_value
                                        } 
                                    + 0.1
                                ).clamp(0.0, 1.0);

                            {
                                let not_zero = end_pos > 0.0;
                                let left = x
                                    + indicator_width * end_pos
                                    + if not_zero { 4.0 } else { 0.0 };
                                let top = y;
                                let right = left + indicator_width * (1.0 - end_pos)
                                    - if not_zero { 4.0 } else { 0.0 };
                                let bottom = y + indicator_height;
                                if right > left {
                                    let rect = Rect::from_ltrb(left, top, right, bottom);
                                    let rrect = RRect::new_rect_radii(rect, &radius);
                                    paint.set_color(track_color);
                                    canvas.draw_rrect(rrect, &paint);
                                }
                            }

                            {
                                let rect = Rect::from_xywh(
                                    x + indicator_width * start_position_value,
                                    y,
                                    indicator_width * (end_pos - start_position_value),
                                    indicator_height,
                                );
                                let rrect = RRect::new_rect_radii(rect, &radius);
                                paint.set_color(active_indicator_color);
                                canvas.draw_rrect(rrect, &paint);
                            }
                        }
                    }
                }
            });

        is_determinate.notify();

        Self { item }
    }
}
