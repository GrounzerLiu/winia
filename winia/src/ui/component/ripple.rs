use crate::impl_property_redraw;
use crate::shared::{Children, Gettable, Observable, Settable, Shared, SharedColor, SharedF32};
use crate::ui::app::AppContext;
use crate::ui::item::{DisplayParameter, ItemEvent, PointerState};
use crate::ui::theme::colors;
use crate::ui::theme::colors::parse_color;
use crate::ui::Item;
use skia_safe::{Color, Path, Rect};
use std::time::Duration;
use skia_safe::gradient_shader::GradientShaderColors;
use toml::Value;
use proc_macro::item;

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
        let on_primary_color = app_context
            .theme
            .value()
            .get_color(colors::ON_PRIMARY)
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

        let ripple_position: Shared<(f32, f32)> = Shared::from_static((0.0, 0.0));
        let mut ripple_radius: Shared<f32> = Shared::from_static(0.0);
        ripple_radius.add_observer(
            0,
            Box::new({
                let app_context = app_context.clone();
                move || {
                    app_context.request_redraw();
                }
            }),
        );
        let item_event = ItemEvent::new()
            .draw({
                let ripple_position = ripple_position.clone();
                let ripple_radius = ripple_radius.clone();
                let property = property.clone();
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
                    let radius = ((width.powi(2) + height.powi(2)).sqrt()) / 2.0;
                    canvas.draw_circle((center_x, center_y), radius, &paint);

                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    let foreground_color = property.color.get();
                    let foreground_opacity = property.foreground_opacity.get();
                    let ripple_position = ripple_position.get();
                    let ripple_radius = ripple_radius.get();
                    let colors = [
                        foreground_color.with_a((foreground_opacity.clamp(0.0, 1.0) * 255.0) as u8),
                        Color::TRANSPARENT,
                    ];
                    let pos = [(1.0 - (30.0 / ripple_radius)).clamp(0.0, 1.1), 1.0];
                    let pos_ref: &[f32] = &pos;
                    let gradient_colors = GradientShaderColors::Colors(&colors);
                    let shader = skia_safe::Shader::radial_gradient(
                        ripple_position,
                        ripple_radius + 30.0,
                        gradient_colors,
                        pos_ref,
                        skia_safe::TileMode::Clamp,
                        None,
                        None,
                    );
                    paint.set_shader(shader);
                    canvas.draw_paint(&paint);
                }
            })
            .pointer_input({
                let property = property.clone();
                let mut ripple_position = ripple_position.clone();
                let mut ripple_radius = ripple_radius.clone();
                move |item, event| {
                    let mut property = property.value();
                    match event.pointer_state {
                        PointerState::Started => {
                            property
                                .foreground_opacity
                                .get_animation()
                                .map(|animation| {
                                    animation.stop();
                                });
                            property.foreground_opacity.set(0.1);
                            ripple_radius.get_animation().map(|animation| {
                                animation.stop();
                            });
                            ripple_position.set((event.x, event.y));
                            let display_parameter = item.get_display_parameter();
                            let width = display_parameter.width;
                            let height = display_parameter.height;
                            let radius = (width.powi(2) + height.powi(2)).sqrt();
                            ripple_radius.set(0.0);
                            ripple_radius
                                .animation_to_f32(radius)
                                .duration(Duration::from_millis(500))
                                .start(item.get_app_context());
                        }
                        PointerState::Ended => {
                            property
                                .foreground_opacity
                                .get_animation()
                                .map(|animation| {
                                    animation.stop();
                                });
                            property
                                .foreground_opacity
                                .animation_to_f32(0.0)
                                .duration(Duration::from_millis(500))
                                .start(item.get_app_context());
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
                        .map(|animation| {
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
