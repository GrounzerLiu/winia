use crate::shared::{Gettable, Settable, SharedF32};
use crate::ui::app::AppContext;
use crate::ui::item::{DisplayParameter, MouseScrollDelta, MouseWheel};
use crate::ui::theme::colors;
use skia_safe::{Canvas, RRect, Rect};

pub struct Scroller {
    app_context: AppContext,
    scroll_enabled: (bool, bool),
    scroll_range: (f32, f32),
    scroll_extent: (f32, f32),
    scroll_position: (SharedF32, SharedF32),
    scroll_speed: f32,
    scroll_position_updater: Box<dyn FnMut(f32, f32) -> (f32, f32)>,
    content_size: (f32, f32),
    thumb_opacity: SharedF32,
}

impl Scroller {
    pub fn new(
        app_context: AppContext,
        scroll_enabled: (bool, bool),
        scroll_range: (f32, f32),
        scroll_extent: (f32, f32),
        scroll_position: (f32, f32),
    ) -> Self {
        Self {
            app_context,
            scroll_enabled,
            scroll_range,
            scroll_extent,
            scroll_position: (scroll_position.0.into(), scroll_position.1.into()),
            scroll_speed: 30.0,
            scroll_position_updater: Box::new(|x, y| (x, y)),
            content_size: (0.0, 0.0),
            thumb_opacity: 1.0.into(),
        }
    }

    fn sanitize(&mut self) {
        let x = self.scroll_position.0.get();
        let y = self.scroll_position.1.get();
        if x < 0.0 {
            self.scroll_position.0.set(0.0);
        } else if self.scroll_extent.0 > self.scroll_range.0 {
            self.scroll_position.0.set(0.0);
        } else if x + self.scroll_extent.0 > self.scroll_range.0 {
            self.scroll_position
                .0
                .set(self.scroll_range.0 - self.scroll_extent.0);
        }

        if y < 0.0 {
            self.scroll_position.1.set(0.0);
        } else if self.scroll_extent.1 > self.scroll_range.1 {
            self.scroll_position.1.set(0.0);
        } else if y + self.scroll_extent.1 > self.scroll_range.1 {
            self.scroll_position
                .1
                .set(self.scroll_range.1 - self.scroll_extent.1);
        }
    }

    pub fn scroll_enabled(&self) -> (bool, bool) {
        self.scroll_enabled
    }

    pub fn set_scroll_enabled(&mut self, scroll_enabled: (bool, bool)) {
        self.scroll_enabled = scroll_enabled;
    }

    pub fn scroll_range(&self) -> (f32, f32) {
        self.scroll_range
    }

    pub fn set_scroll_range(&mut self, scroll_range: (f32, f32)) {
        self.scroll_range = scroll_range;
        self.sanitize();
    }

    pub fn scroll_extent(&self) -> (f32, f32) {
        self.scroll_extent
    }

    pub fn set_scroll_extent(&mut self, scroll_extent: (f32, f32)) {
        self.scroll_extent = scroll_extent;
        self.sanitize();
    }

    pub fn scroll_position(&self) -> (f32, f32) {
        (self.scroll_position.0.get(), self.scroll_position.1.get())
    }

    pub fn set_scroll_position_updater(
        &mut self,
        scroll_position_updater: impl FnMut(f32, f32) -> (f32, f32) + 'static,
    ) {
        self.scroll_position_updater = Box::new(scroll_position_updater);
    }

    pub fn update_by_mouse_wheel(&mut self, mouse_wheel: MouseWheel) {
        match mouse_wheel.delta {
            MouseScrollDelta::LineDelta(x, y) => {
                if let Some(mut animation) = self.scroll_position.0.get_animation() {
                    animation.cancel();
                }
                if let Some(mut animation) = self.scroll_position.1.get_animation() {
                    animation.cancel();
                }
                let (x, y) =
                    (self.scroll_position_updater)(x * self.scroll_speed, y * self.scroll_speed);
                let x = self.scroll_position.0.get() - x;
                let y = self.scroll_position.1.get() - y;

                if x > 0.0 && x + self.scroll_extent.0 < self.scroll_range.0 {
                    self.scroll_position.0.set(x);
                } else if x <= 0.0 {
                    self.scroll_position.0.set(0.0);
                } else if x > 0.0 && x + self.scroll_extent.0 >= self.scroll_range.0 {
                    self.scroll_position
                        .0
                        .set(self.scroll_range.0 - self.scroll_extent.0);
                }
                if y > 0.0 && y + self.scroll_extent.1 < self.scroll_range.1 {
                    self.scroll_position.1.set(y);
                } else if y <= 0.0 {
                    self.scroll_position.1.set(0.0);
                } else if y > 0.0 && y + self.scroll_extent.1 >= self.scroll_range.1 {
                    self.scroll_position
                        .1
                        .set(self.scroll_range.1 - self.scroll_extent.1);
                }
                // self.scroll_position
                //     .0
                //     .animation_to_f32(x)
                //     .duration(Duration::from_millis(300))
                //     .interpolator(EaseOutCirc::new())
                //     .start(self.app_context.clone());
                // self.scroll_position
                //     .1
                //     .animation_to_f32(y)
                //     .duration(Duration::from_millis(300))
                //     .interpolator(EaseOutCirc::new())
                //     .start(self.app_context.clone());
            }
            MouseScrollDelta::LogicalDelta(x, y) => {
                if let Some(mut animation) = self.scroll_position.0.get_animation() {
                    animation.cancel();
                }
                if let Some(mut animation) = self.scroll_position.1.get_animation() {
                    animation.cancel();
                }
                let (x, y) = (self.scroll_position_updater)(
                    x, /* self.scroll_speed*/
                    y, /* self.scroll_speed*/
                );
                let x = self.scroll_position.0.get() - x;
                let y = self.scroll_position.1.get() - y;

                if x > 0.0 && x + self.scroll_extent.0 < self.scroll_range.0 {
                    self.scroll_position.0.set(x);
                } else if x <= 0.0 {
                    self.scroll_position.0.set(0.0);
                } else if x > 0.0 && x + self.scroll_extent.0 >= self.scroll_range.0 {
                    self.scroll_position
                        .0
                        .set(self.scroll_range.0 - self.scroll_extent.0);
                }

                if y > 0.0 && y + self.scroll_extent.1 < self.scroll_range.1 {
                    self.scroll_position.1.set(y);
                } else if y <= 0.0 {
                    self.scroll_position.1.set(0.0);
                } else if y > 0.0 && y + self.scroll_extent.1 >= self.scroll_range.1 {
                    self.scroll_position
                        .1
                        .set(self.scroll_range.1 - self.scroll_extent.1);
                }

                // self.scroll_position
                //     .0
                //     .animation_to_f32(x)
                //     .duration(Duration::from_millis(300))
                //     .interpolator(EaseOutCirc::new())
                //     .start(self.app_context.clone());
                // self.scroll_position
                //     .1
                //     .animation_to_f32(y)
                //     .duration(Duration::from_millis(300))
                //     .interpolator(EaseOutCirc::new())
                //     .start(self.app_context.clone());
            }
        }
    }

    pub fn draw(
        &mut self,
        display_parameter: &DisplayParameter,
        content_size: (f32, f32),
        canvas: &Canvas,
    ) {
        self.content_size = content_size;
        let thumb_opacity = self.thumb_opacity.get();
        let thumb_color = {
            let theme = self.app_context.theme();
            let thumb_color = theme.value().get_color(colors::ON_SURFACE).unwrap();
            thumb_color.with_a((thumb_opacity * 255.0) as u8)
        };

        // Draw horizontal thumb
        if self.scroll_extent.0 < self.scroll_range.0 / 2.0 {
            let thumb_width = self.scroll_extent.0 / self.scroll_range.0 * display_parameter.width;
            let thumb_height = 10.0;
            let thumb_x = display_parameter.x()
                + self.scroll_position.0.get() / self.scroll_range.0 * display_parameter.width;
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
        if self.scroll_extent.1 < self.scroll_range.1 / 2.0 {
            let thumb_width = 10.0;
            let thumb_height =
                self.scroll_extent.1 / self.scroll_range.1 * display_parameter.height;
            let thumb_x = display_parameter.x() + display_parameter.width - thumb_width;
            let thumb_y = display_parameter.y()
                + self.scroll_position.1.get() / self.scroll_range.1 * display_parameter.height;
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
