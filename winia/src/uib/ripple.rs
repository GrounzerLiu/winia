use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use skia_safe::{Canvas, Color, Paint, Rect};
use crate::animation::Animation;
use crate::app::SharedApp;
use crate::ui::item::Item;
use crate::ui::{ItemEvent, LayoutDirection, MeasureMode, PointerAction};
use crate::property::{BoolProperty, ColorProperty, FloatProperty, Gettable, Observable, Observer};

struct RippleProperties {
    ripple_color: ColorProperty,
}

pub struct Ripple {
    item: Item,
    properties: Arc<Mutex<RippleProperties>>,
}

impl Ripple {
    pub fn new(app: SharedApp) -> Self {
        let properties = Arc::new(Mutex::new(RippleProperties {
            ripple_color: Color::from_argb(0x1F, 0xFF, 0xFF, 0xFF).into(),
        }));

        let mut ripple_radius = FloatProperty::from_value(0.0);
        let ripple_x = FloatProperty::from_value(0.0);
        let ripple_y = FloatProperty::from_value(0.0);
        let max_ripple_radius = FloatProperty::from_value(0.0);

        let mut item = Item::new(
            app,
            ItemEvent::default()

                .set_on_draw({
                    let properties = properties.clone();
                    let ripple_radius = ripple_radius.clone();
                    let ripple_x = ripple_x.clone();
                    let ripple_y = ripple_y.clone();
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

                        let color = *layout_params.get_color_param("ripple_color").unwrap_or(&properties.ripple_color.get());

                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_color(color);
                        paint.set_style(skia_safe::paint::Style::Fill);

                        let ripple_radius = *layout_params.get_float_param("ripple_radius").unwrap_or(&ripple_radius.get());
                        canvas.draw_circle((ripple_x.get(), ripple_y.get()), ripple_radius, &paint);

                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.draw(canvas);
                        }
                    }
                })

                .set_on_measure({
                    let properties = properties.clone();
                    let ripple_radius = ripple_radius.clone();
                    let mut max_ripple_radius = max_ripple_radius.clone();
                    move |item, width_measure_mode, height_measure_mode| {
                        let mut layout_params = item.get_layout_params().clone();
                        layout_params.init_from_item(item);

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

                        layout_params.set_color_param("ripple_color", properties.lock().unwrap().ripple_color.get());
                        layout_params.set_float_param("ripple_radius", ripple_radius.get());

                        max_ripple_radius.set_value((layout_params.width.powi(2) + layout_params.height.powi(2)).sqrt());

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

                .set_on_pointer_input({
                    let mut ripple_radius = ripple_radius.clone();
                    let max_ripple_radius = max_ripple_radius.clone();
                    let ripple_x = ripple_x.clone();
                    let ripple_y = ripple_y.clone();
                    move |item, pointer_action| {
                        if let PointerAction::Down { x, y, .. } = pointer_action {
                            ripple_x.set_value(x);
                            ripple_y.set_value(y);
                            ripple_radius.set_value(0.0);
                            let app = item.get_app();
                            // Animation::new({
                            //     let mut ripple_radius = ripple_radius.clone();
                            //     let max_ripple_radius = max_ripple_radius.clone();
                            //     move || {
                            //         // ripple_radius.set_value(max_ripple_radius.get());
                            //         // app.request_layout();
                            //     }
                            // }).duration(Duration::from_millis(1000)).start();
                        }
                        false
                    }
                })
        );

        Ripple {
            item,
            properties,
        }
    }

    pub fn ripple_color(self, color: impl Into<ColorProperty>) -> Self {
        let color = color.into();
        let app = self.item.get_app();
        color.add_observer(
            Observer::new_without_id(
                move || {
                    app.request_layout();
                }
            )
        );
        self.properties.lock().unwrap().ripple_color = color;
        self
    }

    pub fn unwrap(self) -> Item {
        self.item
    }
}