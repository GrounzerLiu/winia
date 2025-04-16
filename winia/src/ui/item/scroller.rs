use std::collections::LinkedList;
use parking_lot::MutexGuard;
use crate::shared::{Gettable, Settable, Shared, SharedF32};
use crate::ui::app::{EventLoopProxy, WindowContext};
use crate::ui::item::{DisplayParameter, MouseScrollDelta, MouseWheel};
use crate::ui::theme::color;
use skia_safe::{Canvas, RRect, Rect};

pub struct Scroller {
    animated_f32: SharedF32,
    event_loop_proxy: EventLoopProxy,
    scroll_enabled: (bool, bool),
    mouse_scroll_speed: f32,
    x_deltas: LinkedList<f32>,
    y_deltas: LinkedList<f32>,
    thumb_opacity: SharedF32,
}

impl Scroller {
    pub fn new(
        event_loop_proxy: &EventLoopProxy,
        scroll_enabled: (bool, bool),
    ) -> Self {
        Self {
            animated_f32: 0.0.into(),
            event_loop_proxy: event_loop_proxy.clone(),
            scroll_enabled,
            mouse_scroll_speed: 30.0,
            x_deltas: LinkedList::new(),
            y_deltas: LinkedList::new(),
            thumb_opacity: 1.0.into(),
        }
    }
    
    pub fn set_scroll_enabled(
        &mut self,
        scroll_enabled: (bool, bool),
    ) {
        self.scroll_enabled = scroll_enabled;
    }
    
    pub fn x_deltas(&mut self) -> &mut LinkedList<f32> {
        &mut self.x_deltas
    }
    
    pub fn y_deltas(&mut self) -> &mut LinkedList<f32> {
        &mut self.y_deltas
    }
    
    pub fn update_by_mouse_wheel(&mut self, mouse_wheel: &MouseWheel) {
        match mouse_wheel.delta {
            MouseScrollDelta::LineDelta(x, y) => {
                if self.scroll_enabled.0 {
                    self.x_deltas.push_back(- x * self.mouse_scroll_speed);
                }
                if self.scroll_enabled.1 {
                    self.y_deltas.push_back(- y * self.mouse_scroll_speed);
                }
            }
            MouseScrollDelta::LogicalDelta(x, y) => {
                if self.scroll_enabled.0 {
                    self.x_deltas.push_back(-x);
                }
                if self.scroll_enabled.1 {
                    self.y_deltas.push_back(-y);
                }
            }
        }
    }

    pub fn draw(
        &mut self,
        window_context: &WindowContext,
        display_parameter: &DisplayParameter,
        canvas: &Canvas,
        scroll_range: (f32, f32),
        scroll_extent: (f32, f32),
        scroll_position: (f32, f32),
    ) {
        let thumb_opacity = self.thumb_opacity.get();
        let thumb_color = {
            let theme = window_context.theme();
            let thumb_color = theme.lock().get_color(color::ON_SURFACE).unwrap();
            thumb_color.with_a((thumb_opacity * 255.0) as u8)
        };

        // Draw horizontal thumb
        if scroll_extent.0 < scroll_range.0 / 2.0 {
            let thumb_width = scroll_extent.0 / scroll_range.0 * display_parameter.width;
            let thumb_height = 10.0;
            let thumb_x = display_parameter.x()
                + scroll_position.0 / scroll_range.0 * display_parameter.width;
            let thumb_y = display_parameter.y() + display_parameter.height - thumb_height;
            let thumb_rect = RRect::new_rect_xy(
                Rect::from_xywh(thumb_x, thumb_y, thumb_width, thumb_height),
                5.0,
                5.0,
            );
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(thumb_color);
            canvas.draw_rrect(thumb_rect, &paint);
        }

        // Draw vertical thumb
        if scroll_extent.1 < scroll_range.1 / 2.0 {
            let thumb_width = 10.0;
            let thumb_height =
                scroll_extent.1 / scroll_range.1 * display_parameter.height;
            let thumb_x = display_parameter.x() + display_parameter.width - thumb_width;
            let thumb_y = display_parameter.y()
                + scroll_position.1 / scroll_range.1 * display_parameter.height;
            let thumb_rect = RRect::new_rect_xy(
                Rect::from_xywh(thumb_x, thumb_y, thumb_width, thumb_height),
                5.0,
                5.0,
            );
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(thumb_color);
            canvas.draw_rrect(thumb_rect, &paint);
        }
    }
}
