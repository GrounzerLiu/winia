use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use skia_safe::{BlurStyle, Canvas, Color, MaskFilter, Paint, Path, Point, Rect, RRect, Vector};
use crate::app::{SharedApp, ThemeColor};
use crate::ui::item::Item;
use crate::ui::{ItemEvent, LayoutDirection, MeasureMode};
use crate::ui::additional_property::{ShadowBlur, ShadowColor, ShadowOffsetX, ShadowOffsetY};
use crate::property::{BoolProperty, ColorProperty, FloatProperty, Gettable, Observable, Observer};

struct RectangleProperties {
    color: ColorProperty,
    use_smooth_corners: BoolProperty,
    radius_start_top: FloatProperty,
    radius_end_top: FloatProperty,
    radius_start_bottom: FloatProperty,
    radius_end_bottom: FloatProperty,
}

pub struct Rectangle {
    item: Item,
    properties: Arc<Mutex<RectangleProperties>>,
}

impl Rectangle {
    pub fn new(app: SharedApp) -> Self {
        let properties = Arc::new(Mutex::new(RectangleProperties {
            color: ColorProperty::from_value(Color::TRANSPARENT),
            use_smooth_corners: BoolProperty::from_value(false),
            radius_start_top: FloatProperty::from_value(0.0),
            radius_end_top: FloatProperty::from_value(0.0),
            radius_start_bottom: FloatProperty::from_value(0.0),
            radius_end_bottom: FloatProperty::from_value(0.0),
        }));

        let mut item = Item::new(
            app,
            ItemEvent::default()

                .set_on_draw({
                    let properties = properties.clone();
                    move |item, canvas| {
                        if let Some(background) = item.get_background().lock().as_mut() {
                            background.draw(canvas);
                        }

                        let layout_params = item.get_layout_params();

                        let layout_direction = item.get_layout_direction().get();

                        let x = match layout_direction {
                            LayoutDirection::LeftToRight => {
                                layout_params.x + layout_params.padding_start
                            }
                            LayoutDirection::RightToLeft => {
                                layout_params.x + layout_params.padding_end
                            }
                        };

                        let y = layout_params.y + layout_params.padding_top;

                        let width = layout_params.width - layout_params.padding_start - layout_params.padding_end;
                        let height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom;

                        let rect = Rect::from_xywh(x, y, width, height);

                        let properties = properties.lock().unwrap();
                        let use_smooth_corners = properties.use_smooth_corners.get();

                        let color = *layout_params.get_color_param("color").unwrap_or(&(properties.color.get()));
                        let radius_start_top = *layout_params.get_float_param("radius_start_top").unwrap_or(&(properties.radius_start_top.get()));
                        let radius_end_top = *layout_params.get_float_param("radius_end_top").unwrap_or(&(properties.radius_end_top.get()));
                        let radius_start_bottom = *layout_params.get_float_param("radius_start_bottom").unwrap_or(&(properties.radius_start_bottom.get()));
                        let radius_end_bottom = *layout_params.get_float_param("radius_end_bottom").unwrap_or(&(properties.radius_end_bottom.get()));

                        let radius_left_top = match layout_direction {
                            LayoutDirection::LeftToRight => {
                                radius_start_top
                            }
                            LayoutDirection::RightToLeft => {
                                radius_end_top
                            }
                        };

                        let radius_right_top = match layout_direction {
                            LayoutDirection::LeftToRight => {
                                radius_end_top
                            }
                            LayoutDirection::RightToLeft => {
                                radius_start_top
                            }
                        };

                        let radius_right_bottom = match layout_direction {
                            LayoutDirection::LeftToRight => {
                                radius_end_bottom
                            }
                            LayoutDirection::RightToLeft => {
                                radius_start_bottom
                            }
                        };

                        let radius_left_bottom = match layout_direction {
                            LayoutDirection::LeftToRight => {
                                radius_start_bottom
                            }
                            LayoutDirection::RightToLeft => {
                                radius_end_bottom
                            }
                        };

                        let shadow_color = *layout_params.get_color_param("shadow_color").unwrap();
                        let shadow_offset_x = *layout_params.get_float_param("shadow_offset_x").unwrap();
                        let shadow_offset_y = *layout_params.get_float_param("shadow_offset_y").unwrap();
                        let shadow_blur = *layout_params.get_float_param("shadow_blur").unwrap();

                        let mut shadow_paint = Paint::default();
                        shadow_paint.set_anti_alias(true);
                        shadow_paint.set_color(shadow_color);
                        shadow_paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, shadow_blur, true));

                        let shadow_rect = Rect::from_xywh(rect.left + shadow_offset_x, rect.top + shadow_offset_y, rect.width(), rect.height());

                        draw_round_rect(canvas, use_smooth_corners, shadow_rect, radius_left_top, radius_right_top, radius_right_bottom, radius_left_bottom, &shadow_paint);

                        draw_round_rect(canvas, use_smooth_corners, rect, radius_left_top, radius_right_top, radius_right_bottom, radius_left_bottom, &Paint::default().set_anti_alias(true).set_color(color));

                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.draw(canvas);
                        }
                    }
                })

                .set_on_measure({
                    let properties = properties.clone();
                    move |item, width_measure_mode, height_measure_mode| {
                        let mut layout_params = item.get_layout_params().clone();
                        layout_params.init_from_item(item);

                        layout_params.set_color_param("color", properties.lock().unwrap().color.get());
                        layout_params.set_float_param("radius_start_top", properties.lock().unwrap().radius_start_top.get());
                        layout_params.set_float_param("radius_end_top", properties.lock().unwrap().radius_end_top.get());
                        layout_params.set_float_param("radius_start_bottom", properties.lock().unwrap().radius_start_bottom.get());
                        layout_params.set_float_param("radius_end_bottom", properties.lock().unwrap().radius_end_bottom.get());

                        match width_measure_mode {
                            MeasureMode::Specified(width) => {
                                layout_params.width = width + layout_params.padding_start + layout_params.padding_end;
                            }
                            MeasureMode::Unspecified(_) => {
                                layout_params.width = layout_params.padding_start + layout_params.padding_end;
                            }
                        }
                        layout_params.width = layout_params.width.max(item.get_min_width().get()).min(item.get_max_width().get());
                        match height_measure_mode {
                            MeasureMode::Specified(height) => {
                                layout_params.height = height + layout_params.padding_top + layout_params.padding_bottom;
                            }
                            MeasureMode::Unspecified(_) => {
                                layout_params.height = layout_params.padding_top + layout_params.padding_bottom;
                            }
                        }

                        layout_params.height = layout_params.height.max(item.get_min_height().get()).min(item.get_max_height().get());

                        if let Some(shadow_color) = item.get_shadow_color() {
                            layout_params.set_color_param("shadow_color", shadow_color.get());
                        } else {
                            layout_params.set_color_param("shadow_color", Color::from_argb(0x66, 0, 0, 0));
                        }

                        if let Some(shadow_offset_x) = item.get_shadow_offset_x() {
                            layout_params.set_float_param("shadow_offset_x", shadow_offset_x.get());
                        } else {
                            layout_params.set_float_param("shadow_offset_x", 0.0);
                        }

                        if let Some(shadow_offset_y) = item.get_shadow_offset_y() {
                            layout_params.set_float_param("shadow_offset_y", shadow_offset_y.get());
                        } else {
                            layout_params.set_float_param("shadow_offset_y", 6.0);
                        }

                        if let Some(shadow_blur) = item.get_shadow_blur() {
                            layout_params.set_float_param("shadow_blur", shadow_blur.get());
                        } else {
                            layout_params.set_float_param("shadow_blur", 4.0);
                        }


                        if let Some(background) = item.get_background().lock().as_mut() {
                            background.measure(
                                MeasureMode::Specified(layout_params.width),
                                MeasureMode::Specified(layout_params.height),
                            );
                        }

                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.measure(
                                MeasureMode::Specified(layout_params.width),
                                MeasureMode::Specified(layout_params.height),
                            );
                        }

                        item.set_layout_params(&layout_params);
                    }
                })

                .set_on_layout(
                    |item, x, y| {
                        let mut layout_params = item.get_layout_params().clone();
                        layout_params.x = x;
                        layout_params.y = y;
                        item.set_layout_params(&layout_params);
                        if let Some(background) = item.get_background().lock().as_mut() {
                            background.layout(layout_params.width, layout_params.height);
                        }
                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.layout(layout_params.width, layout_params.height);
                        }
                    }
                )
        );

        Rectangle {
            item,
            properties,
        }
    }

    pub fn color(self, color: impl Into<ColorProperty>) -> Self {
        let color = color.into();
        let app = self.item.get_app();
        color.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        self.properties.lock().unwrap().color = color;
        self
    }

    pub fn radius_start_top(self, radius: impl Into<FloatProperty>) -> Self {
        let radius = radius.into();
        let app = self.item.get_app();
        radius.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        self.properties.lock().unwrap().radius_start_top = radius;
        self
    }

    pub fn radius_end_top(self, radius: impl Into<FloatProperty>) -> Self {
        let radius = radius.into();
        let app = self.item.get_app();
        radius.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout()
                }
            )
        );
        self.properties.lock().unwrap().radius_end_top = radius;
        self
    }

    pub fn radius_start_bottom(self, radius: impl Into<FloatProperty>) -> Self {
        let radius = radius.into();
        let app = self.item.get_app();
        radius.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        self.properties.lock().unwrap().radius_start_bottom = radius;
        self
    }

    pub fn radius_end_bottom(self, radius: impl Into<FloatProperty>) -> Self {
        let radius = radius.into();
        let app = self.item.get_app();
        radius.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        self.properties.lock().unwrap().radius_end_bottom = radius;
        self
    }

    pub fn radius(self, radius: impl Into<FloatProperty>) -> Self {
        let radius = radius.into();
        let app = self.item.get_app();
        radius.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        let mut properties = self.properties.lock().unwrap();
        properties.radius_start_top = radius.clone();
        properties.radius_end_top = radius.clone();
        properties.radius_start_bottom = radius.clone();
        properties.radius_end_bottom = radius;
        drop(properties);
        self
    }

    pub fn use_smooth_corners(self, use_smooth_corners: impl Into<BoolProperty>) -> Self {
        let use_smooth_corners = use_smooth_corners.into();
        let app = self.item.get_app();
        use_smooth_corners.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        self.properties.lock().unwrap().use_smooth_corners = use_smooth_corners;
        self
    }

    pub fn unwrap(self) -> Item {
        self.item
    }
}

fn draw_round_rect(canvas: &Canvas, smooth: bool, rect: Rect, radius_left_top: f32, radius_right_top: f32, radius_right_bottom: f32, radius_left_bottom: f32, paint: &Paint) {
    let radius_left_top = radius_left_top.clamp(0.0, rect.width() / 2.0).clamp(0.0, rect.height() / 2.0);
    let radius_right_top = radius_right_top.clamp(0.0, rect.width() / 2.0).clamp(0.0, rect.height() / 2.0);
    let radius_right_bottom = radius_right_bottom.clamp(0.0, rect.width() / 2.0).clamp(0.0, rect.height() / 2.0);
    let radius_left_bottom = radius_left_bottom.clamp(0.0, rect.width() / 2.0).clamp(0.0, rect.height() / 2.0);

    let instant = std::time::Instant::now();
    if smooth {
        let mut path = Path::new();
        path.move_to((rect.left + radius_left_top, rect.top));
        path.line_to((rect.right - radius_right_top, rect.top));
        super_ellipse_curve(&mut path, (rect.right - radius_right_top, rect.top + radius_right_top), radius_right_top,1);
        path.line_to((rect.right, rect.bottom - radius_right_bottom));
        super_ellipse_curve(&mut path, (rect.right - radius_right_bottom, rect.bottom - radius_right_bottom), radius_right_bottom,2);
        path.line_to((rect.left + radius_left_bottom, rect.bottom));
        super_ellipse_curve(&mut path, (rect.left + radius_left_bottom, rect.bottom - radius_left_bottom), radius_left_bottom,3);
        path.line_to((rect.left, rect.top + radius_left_top));
        super_ellipse_curve(&mut path, (rect.left + radius_left_top, rect.top + radius_left_top), radius_left_top,4);
        path.close();
        canvas.draw_path(&path, paint);
    } else {
        let radii = [
            Vector::new(radius_left_top, radius_left_top),
            Vector::new(radius_right_top, radius_right_top),
            Vector::new(radius_right_bottom, radius_right_bottom),
            Vector::new(radius_left_bottom, radius_left_bottom),
        ];

        let rrect = RRect::new_rect_radii(rect, &radii);
        canvas.draw_rrect(&rrect, paint);
    }
}

fn super_ellipse_curve(path: &mut Path, center: impl Into<Point>, radius: f32, quadrant: u8) {
    let center = center.into();
    for x in 0..radius as i32 {
        let x = x as f32 + 1.0;
        let y = (radius.powi(3) - x.powi(3)).cbrt();
        match quadrant {
            1 => {
                path.line_to((center.x + x, center.y - y));
            }
            2 => {
                path.line_to((center.x + y, center.y + x));
            }
            3 => {
                path.line_to((center.x - x, center.y + y));
            }
            4 => {
                path.line_to((center.x - y, center.y -x ));
            }
            _=>{}
        }
    }
    match quadrant {
        1 => {
            path.line_to((center.x + radius, center.y));
        }
        2 => {
            path.line_to((center.x, center.y + radius));
        }
        3 => {
            path.line_to((center.x - radius, center.y));
        }
        4 => {
            path.line_to((center.x, center.y - radius));
        }
        _=>{}
    }
}

pub trait RectangleExt {
    fn rectangle(&self) -> Rectangle;
}

impl RectangleExt for SharedApp {
    fn rectangle(&self) -> Rectangle {
        Rectangle::new(self.clone())
    }
}