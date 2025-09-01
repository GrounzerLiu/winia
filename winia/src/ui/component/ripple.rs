use crate::impl_property_redraw;
use crate::shared::{
    Children, Gettable, Observable, Settable, Shared, SharedAnimationTrait, SharedBool,
    SharedColor, SharedF32,
};
use crate::ui::app::WindowContext;
use crate::ui::item::{ItemData, Pointer, PointerState};
use crate::ui::theme::color;
use crate::ui::theme::color::parse_color;
use crate::ui::Item;
use proc_macro::item;
use skia_safe::{Paint, Path, Rect};
use std::collections::HashSet;
use std::time::Duration;
use toml::Value;

struct Layer {
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
    pub fn new(app_context: &WindowContext) -> Self {
        let event_loop_proxy = app_context.event_loop_proxy();
        let primary_color = *app_context
            .theme
            .lock()
            .get_color(color::PRIMARY)
            .unwrap();
        let property = Shared::from(RippleProperty {
            color: primary_color.into(),
            borderless: true.into(),
            foreground_opacity: {
                let mut opacity: SharedF32 = 0.0.into();
                opacity.add_observer(
                    0,
                    Box::new({
                        let event_loop_proxy = event_loop_proxy.clone();
                        move || {
                            event_loop_proxy.request_redraw();
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
                        let event_loop_proxy = event_loop_proxy.clone();
                        move || {
                            event_loop_proxy.request_redraw();
                        }
                    }),
                );
                opacity
            },
        });
        let layers: Shared<Vec<Layer>> = Vec::new().into();

        let item = Item::new(app_context, Children::new()).clip(true);
        item.data()
            .set_draw({
                let property = property.clone();
                let layers = layers.clone();
                move |item, canvas| {
                    let property = property.lock();
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    let background_color = property.color.get();
                    let background_opacity = property.background_opacity.get();
                    paint.set_color(
                        background_color.with_a((background_opacity.clamp(0.0, 1.0) * 255.0) as u8),
                    );
                    let display_parameter = item.get_display_parameter();
                    let x = display_parameter.x();
                    let y = display_parameter.y();
                    let width = display_parameter.width;
                    let height = display_parameter.height;
                    let center_x = x + width / 2.0;
                    let center_y = y + height / 2.0;
                    let radius = (width.powi(2) + height.powi(2)).sqrt();
                    canvas.draw_circle((center_x, center_y), radius / 2.0, &paint);

                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    let foreground_color = property.color.get();
                    let mut layers = layers.lock();
                    layers.retain(|layer| !layer.is_finished.get());
                    for layer in layers.iter() {
                        let opacity = layer.opacity.get();
                        let degree = layer.degree.get();
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        let color =
                            foreground_color.with_a((opacity.clamp(0.0, 1.0) * 255.0) as u8);
                        let radius = radius * degree;
                        paint.set_color(color);

                        let display_parameter = item.get_display_parameter();
                        let (center_x, center_y) = layer.center;
                        let center = (center_x + display_parameter.x(), center_y + display_parameter.y());
                        canvas.draw_circle(center, radius, &paint);
                    }
                }
            })
            .set_pointer_input({
                let app_context = app_context.clone();
                let mut down_pointers: HashSet<Pointer> = HashSet::new();
                move |item, event| match event.pointer_state {
                    PointerState::Started => {
                        fn add_observer(app_context: WindowContext, shared: &mut SharedF32) {
                            shared.add_observer(
                                0,
                                Box::new({
                                    let event_loop_proxy = app_context.event_loop_proxy().clone();
                                    move || {
                                        event_loop_proxy.request_redraw();
                                    }
                                }),
                            );
                        }

                        if !down_pointers.is_empty() {
                            return;
                        }

                        down_pointers.insert(event.pointer);

                        let mut degree = SharedF32::from(0.0);
                        let mut opacity = SharedF32::from(0.1);
                        add_observer(app_context.clone(), &mut degree);
                        add_observer(app_context.clone(), &mut opacity);
                        degree
                            .animation_to_f32(1.0)
                            .duration(Duration::from_millis(500))
                            .start(app_context.event_loop_proxy());
                        let display_parameter = item.get_display_parameter();
                        
                        let layer = Layer {
                            is_ended: false,
                            is_finished: false.into(),
                            center: (event.x - display_parameter.x(), event.y - display_parameter.y()),
                            degree,
                            opacity,
                        };
                        layers.lock().push(layer);
                    }
                    PointerState::Ended => {
                        down_pointers.remove(&event.pointer);
                        if !down_pointers.is_empty() {
                            return;
                        }
                        let mut layers = layers.lock();
                        for layer in layers.iter_mut() {
                            if layer.is_ended {
                                continue;
                            }
                            layer.is_ended = true;
                            let is_finished = layer.is_finished.clone();
                            if let Some(animation) = layer.degree.get_animation() {
                                if !animation.is_finished() {
                                    let opacity = layer.opacity.clone();
                                    let event_loop_proxy = app_context.event_loop_proxy().clone();
                                    animation.on_finish(move || {
                                        let is_finished = is_finished.clone();
                                        opacity
                                            .animation_to_f32(0.0)
                                            .duration(Duration::from_millis(500))
                                            .on_finish(move || {
                                                is_finished.set(true);
                                            })
                                            .start(&event_loop_proxy);
                                    });
                                } else {
                                    layer
                                        .opacity
                                        .animation_to_f32(0.0)
                                        .duration(Duration::from_millis(500))
                                        .on_finish(move || {
                                            is_finished.set(true);
                                        })
                                        .start(app_context.event_loop_proxy());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            })
            .set_hover_event({
                let property = property.clone();
                move |item, is_hovered| {
                    let property = property.lock();
                    if let Some(mut animation) = property
                        .background_opacity
                        .get_animation() { animation.stop(); }
                    property
                        .background_opacity
                        .animation_to_f32(if is_hovered { 0.08 } else { 0.0 })
                        .duration(Duration::from_millis(500))
                        .start(item.get_window_context().event_loop_proxy());
                }
            });

        Self { item, property }
    }
    ///```toml
    /// [ripple]
    /// color = "primary"
    /// # color = "#ff0000"
    /// # color = "0xff0000"
    /// borderless = true
    /// ```
    pub fn from_toml(window_context: &WindowContext, string: &str) -> Self {
        let mut ripple = Ripple::new(window_context);
        let toml: Value = toml::from_str(string).unwrap_or_else(|err| {
            panic!("Failed to parse toml: {}", err);
        });

        if let Some(Value::Table(table)) = toml.get("ripple") {
            if let Some(Value::String(color)) = table.get("color") {
                if let Some(color) = parse_color(color.as_str()) {
                    ripple = ripple.color(color);
                } else {
                    let theme = window_context.theme();
                    if let Some(color) = theme.lock().get_color(color) {
                        ripple = ripple.color(*color);
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
            let mut property = self.property.lock();
            property.borderless = borderless.into();
            let event_loop_proxy = self.item.data().get_window_context().event_loop_proxy().clone();
            let clip_shape = self.item.data().get_clip_shape().clone();
            property.borderless.add_specific_observer(
                self.item.data().get_id(),
                move |borderless| {
                    if *borderless {
                        clip_shape.set_static(Box::new(|item: &mut ItemData| {
                            let display_parameter = item.get_display_parameter();
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
                        clip_shape.set_static(Box::new(|item: &mut ItemData| {
                            let display_parameter = item.get_display_parameter();
                            let x = display_parameter.x();
                            let y = display_parameter.y();
                            let width = display_parameter.width;
                            let height = display_parameter.height;
                            Path::rect(Rect::from_xywh(x, y, width, height), None)
                        }));
                    }
                    event_loop_proxy.request_redraw();
                },
            );
            property.borderless.notify();
        }
        self
    }
}

impl_property_redraw!(Ripple, color, SharedColor);
