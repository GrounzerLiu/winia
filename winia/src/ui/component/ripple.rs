use std::collections::HashSet;
use crate::impl_property_redraw;
use crate::shared::{Children, Gettable, Observable, Settable, Shared, SharedAnimationTrait, SharedBool, SharedColor, SharedF32};
use crate::ui::app::AppContext;
use crate::ui::item::{DisplayParameter, ItemEvent, Pointer, PointerState};
use crate::ui::theme::colors;
use crate::ui::theme::colors::parse_color;
use crate::ui::Item;
use skia_safe::{Color, Paint, Path, Rect};
use std::time::Duration;
use skia_safe::gradient_shader::GradientShaderColors;
use toml::Value;
use proc_macro::item;

struct Layer{
    pub is_ended: bool,
    pub is_finished: SharedBool,
    pub center: (f32, f32),
    pub degree: SharedF32,
    pub opacity: SharedF32,
}

#[derive(Clone)]
struct RippleProperty {
    color: SharedColor,
    borderless: Shared<bool>,
    foreground_opacity: SharedF32,
    background_opacity: SharedF32,
}

#[item]
pub struct Ripple {
    item: Item,
    property: Shared<RippleProperty>,
}

impl Ripple {
    pub fn new(app_context: AppContext) -> Self {
        let primary_color = app_context
            .theme
            .value()
            .get_color(colors::PRIMARY)
            .unwrap();
        let property = Shared::new(RippleProperty {
            color: primary_color.clone().into(),
            borderless: true.into(),
            foreground_opacity: {
                let mut opacity: SharedF32 = 0.0.into();
                opacity.add_observer(
                    0,
                    Box::new({
                        let app_context = app_context.clone();
                        move || {
                            app_context.request_redraw();
                        }
                    }),
                );
                opacity
            },
            background_opacity: {
                let mut opacity: SharedF32 = 0.0.into();
                opacity.add_observer(
                    0,
                    Box::new({
                        let app_context = app_context.clone();
                        move || {
                            app_context.request_redraw();
                        }
                    }),
                );
                opacity
            },
        });
        let layers: Shared<Vec<Layer>> = Vec::new().into();
        let item_event = ItemEvent::new()
            .draw({
                let property = property.clone();
                let layers = layers.clone();
                move |item, canvas| {
                    let property = property.value();
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    let background_color = property.color.get();
                    let background_opacity = property.background_opacity.get();
                    paint.set_color(background_color.with_a((background_opacity.clamp(0.0, 1.0) * 255.0) as u8));
                    let display_parameter = item.get_display_parameter();
                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let width = display_parameter.width;
                    let height = display_parameter.height;
                    let center_x = x + width / 2.0;
                    let center_y = y + height / 2.0;
                    let radius = ((width.powi(2) + height.powi(2)).sqrt());
                    canvas.draw_circle((center_x, center_y), radius / 2.0, &paint);

                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    let foreground_color = property.color.get();
                    let mut layers = layers.value();
                    layers.retain(|layer| !layer.is_finished.get());
                    for layer in layers.iter() {
                        let opacity = layer.opacity.get();
                        let degree = layer.degree.get();
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        let color = foreground_color.with_a((opacity.clamp(0.0, 1.0) * 255.0) as u8);
                        let radius = radius * degree;
                        paint.set_color(color);
                        canvas.draw_circle(layer.center, radius, &paint);
                    }
                }
            })
            .pointer_input({
                let app_context = app_context.clone();
                let mut down_pointers: HashSet<Pointer> = HashSet::new();
                move |item, event| {
                    match event.pointer_state {
                        PointerState::Started => {
                            fn add_observer(app_context: AppContext, shared: &mut SharedF32) {
                                shared.add_observer(
                                    0,
                                    Box::new({
                                        let app_context = app_context.clone();
                                        move || {
                                            app_context.request_redraw();
                                        }
                                    }),
                                );
                            }

                            if !down_pointers.is_empty() {
                                return;
                            }

                            down_pointers.insert(event.pointer);

                            let mut degree = SharedF32::new(0.0);
                            let mut opacity = SharedF32::new(0.1);
                            add_observer(app_context.clone(), &mut degree);
                            add_observer(app_context.clone(), &mut opacity);
                            degree.animation_to_f32(1.0)
                                .duration(Duration::from_millis(300))
                                .start(app_context.clone());
                            let layer = Layer {
                                is_ended: false,
                                is_finished: false.into(),
                                center: (event.x, event.y),
                                degree,
                                opacity,
                            };
                            layers.value().push(layer);
                        }
                        PointerState::Ended => {
                            down_pointers.remove(&event.pointer);
                            if !down_pointers.is_empty() {
                                return;
                            }
                            let mut layers = layers.value();
                            for layer in layers.iter_mut() {
                                if layer.is_ended {
                                    continue;
                                }
                                layer.is_ended = true;
                                let mut is_finished = layer.is_finished.clone();
                                if let Some(animation) = layer.degree.get_animation() {
                                    if !animation.is_finished() {
                                        let opacity = layer.opacity.clone();
                                        let app_context = app_context.clone();
                                        animation.on_finish(
                                            move || {
                                                let mut is_finished = is_finished.clone();
                                                opacity.animation_to_f32(0.0)
                                                    .duration(Duration::from_millis(300))
                                                    .on_finish(
                                                        move || {
                                                            is_finished.set(true);
                                                        },
                                                    )
                                                    .start(app_context.clone());
                                            }
                                        );
                                    } else {
                                        layer.opacity.animation_to_f32(0.0)
                                            .duration(Duration::from_millis(300))
                                            .on_finish(
                                                move || {
                                                    is_finished.set(true);
                                                },
                                            )
                                            .start(app_context.clone());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            })
            .on_hover({
                let property = property.clone();
                move |item, is_hovered| {
                    let mut property = property.value();
                    property
                        .background_opacity
                        .get_animation()
                        .map(|mut animation| {
                            animation.stop();
                        });
                    property
                        .background_opacity
                        .animation_to_f32(if is_hovered { 0.08 } else { 0.0 })
                        .duration(Duration::from_millis(300))
                        .start(item.get_app_context());
                }
            });

        Self {
            item: Item::new(app_context, Children::new(), item_event).clip(true),
            property,
        }
    }
    ///```toml
    /// [ripple]
    /// color = "primary"
    /// # color = "#ff0000"
    /// # color = "0xff0000"
    /// borderless = true
    /// ```
    pub fn from_toml(app_context: AppContext, string: &str) -> Self {
        let mut ripple = Ripple::new(app_context);
        let toml: Value = toml::from_str(string).unwrap_or_else(|err| {
            panic!("Failed to parse toml: {}", err);
        });

        if let Some(Value::Table(table)) = toml.get("ripple") {
            if let Some(Value::String(color)) = table.get("color") {
                if let Some(color) = parse_color(color.as_str()) {
                    ripple = ripple.color(color);
                } else {
                    let theme = ripple.item.get_app_context().theme();
                    if let Some(color) = theme.value().get_color(color) {
                        ripple = ripple.color(color);
                    };
                }
            }
            if let Some(Value::Boolean(borderless)) = table.get("borderless") {
                ripple = ripple.borderless(*borderless);
            }
        }

        ripple
    }

    pub fn borderless(self, borderless: impl Into<Shared<bool>>) -> Self {
        {
            let mut property = self.property.value();
            property.borderless = borderless.into();
            let app_context = self.item.get_app_context();
            let mut clip_shape = self.item.get_clip_shape();
            property
                .borderless
                .add_specific_observer(self.item.get_id(), move |borderless| {
                    if *borderless {
                        clip_shape.set_static(Box::new(|display_parameter: DisplayParameter| {
                            let x = display_parameter.x();
                            let y = display_parameter.y();
                            let width = display_parameter.width;
                            let height = display_parameter.height;
                            let center_x = x + width / 2.0;
                            let center_y = y + height / 2.0;
                            let radius = ((width.powi(2) + height.powi(2)).sqrt()) / 2.0;
                            Path::circle((center_x, center_y), radius, None)
                        }))
                    } else {
                        clip_shape.set_static(Box::new(|display_parameter: DisplayParameter| {
                            let x = display_parameter.x();
                            let y = display_parameter.y();
                            let width = display_parameter.width;
                            let height = display_parameter.height;
                            Path::rect(Rect::from_xywh(x, y, width, height), None)
                        }));
                    }
                    app_context.request_layout();
                });
            property.borderless.notify();
        }
        self
    }
}

impl_property_redraw!(Ripple, color, SharedColor);
