use crate::shared::{Children, Gettable, Settable, Shared, SharedDrawable};
use crate::ui::app::{EventLoopProxy, WindowContext};
use crate::ui::component::{ImageDrawable, ImageExt, RectangleExt, RippleExt, ScaleMode};
use crate::ui::item::{Alignment, ItemData};
use crate::ui::layout::StackExt;
use crate::ui::theme::color;
use crate::ui::Item;
use clonelet::clone;
use parking_lot::Mutex;
use proc_macro::item;
use skia_safe::{Color, Path};
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

const RADIO_BUTTON_CHECKED_ICON: &[u8] = include_bytes!("assets/icon/radio_button_checked.svg");
const RADIO_BUTTON_UNCHECKED_ICON: &[u8] = include_bytes!("assets/icon/radio_button_unchecked.svg");

#[derive(Clone)]
struct RadioProperty<T> {
    value: T,
    selected_value: Shared<T>,
}

#[item(value: T, selected_value: impl Into<Shared<T>>, on_selected: Option<Box<dyn FnMut(&Shared<T>, T)>>)]
pub struct Radio<T: Clone + PartialEq + Send + 'static> {
    item: Item,
    property: Shared<RadioProperty<T>>,
}

impl<T: Clone + PartialEq + Send + 'static> Radio<T> {
    pub fn new(
        window_context: &WindowContext,
        value: T,
        selected_value: impl Into<Shared<T>>,
        mut on_selected: Option<Box<dyn FnMut(&Shared<T>, T)>>,
    ) -> Self {
        if on_selected.is_none() {
            let os: Box<dyn FnMut(&Shared<T>, T)> = Box::new(move |selected_value, value| {
                selected_value.set(value);
            });
            on_selected = Some(os);
        }
        let w = window_context;
        let event_loop_proxy = window_context.event_loop_proxy().clone();
        let property = Shared::from(RadioProperty {
            value: value.clone(),
            selected_value: selected_value.into(),
        });
        let unchecked_opacity = Shared::from(1.0);
        let unchecked_scale = Shared::from(1.0);
        let checked_opacity = Shared::from(0.0);
        let checked_scale = Shared::from(1.5);
        let item = window_context
            .stack(
                Children::new()
                    + w.stack(
                        Children::new()
                            + w.image({
                                let image =
                                    ImageDrawable::from_bytes(RADIO_BUTTON_UNCHECKED_ICON, true)
                                        .unwrap();
                                SharedDrawable::from_static(Box::new(image))
                            })
                            .color(Shared::<Option<Color>>::from_dynamic(
                                [w.theme().into()].into(),
                                {
                                    let theme = w.theme().clone();
                                    move || theme.lock().get_color(color::ON_SURFACE_VARIANT)
                                },
                            ))
                            .oversize_scale_mode(ScaleMode::Stretch)
                            .oversize_scale_mode(ScaleMode::Stretch)
                            .item()
                            .size(20.0, 20.0)
                            .opacity(&unchecked_opacity)
                            .scale_x(&unchecked_scale)
                            .scale_y(&unchecked_scale)
                            + w.image({
                                let image =
                                    ImageDrawable::from_bytes(RADIO_BUTTON_CHECKED_ICON, true)
                                        .unwrap();
                                SharedDrawable::from_static(Box::new(image))
                            })
                            .color(Shared::<Option<Color>>::from_dynamic(
                                [w.theme().into()].into(),
                                {
                                    let theme = w.theme().clone();
                                    move || theme.lock().get_color(color::PRIMARY)
                                },
                            ))
                            .oversize_scale_mode(ScaleMode::Stretch)
                            .oversize_scale_mode(ScaleMode::Stretch)
                            .item()
                            .size(20.0, 20.0)
                            .opacity(&checked_opacity)
                            .scale_x(&checked_scale)
                            .scale_y(&checked_scale),
                    )
                    .item()
                    .size(20.0, 20.0)
                    .clip(true)
                    .clip_shape({
                        let shape: Box<dyn Fn(&mut ItemData) -> Path + Send> =
                            Box::new(|item_data| {
                                let display_parameter = item_data.get_display_parameter();
                                Path::circle(
                                    (
                                        display_parameter.width / 2.0 + display_parameter.x(),
                                        display_parameter.height / 2.0 + display_parameter.y(),
                                    ),
                                    display_parameter.width / 2.0,
                                    None,
                                )
                            });
                        Shared::from(shape)
                    }),
            )
            .item()
            .align_content(Alignment::Center)
            .size(40.0, 40.0)
            .background(window_context.ripple().item().clip(true).clip_shape({
                let shape: Box<dyn Fn(&mut ItemData) -> Path + Send> = Box::new(|item_data| {
                    let display_parameter = item_data.get_display_parameter();
                    Path::circle(
                        (
                            display_parameter.width / 2.0 + display_parameter.x(),
                            display_parameter.height / 2.0 + display_parameter.y(),
                        ),
                        display_parameter.width / 2.0,
                        None,
                    )
                });
                Shared::from(shape)
            }))
            .on_click({
                clone!(
                    property,
                    // event_loop_proxy,
                    // unchecked_opacity,
                    // unchecked_scale,
                    // checked_opacity,
                    // checked_scale
                );
                move |_| {
                    let property = property.lock();
                    let value = property.value.clone();
                    if let Some(on_selected) = &mut on_selected {
                        on_selected(&property.selected_value, value);
                    }
                    // property.selected_value.set(value);
                    // animate(
                    //     true,
                    //     &event_loop_proxy,
                    //     &unchecked_opacity,
                    //     &unchecked_scale,
                    //     &checked_opacity,
                    //     &checked_scale
                    // );
                }
            });

        let value = property.lock().value.clone();
        property
            .lock()
            .selected_value
            .add_specific_observer(item.data().get_id(), {
                clone!(
                    event_loop_proxy,
                    unchecked_opacity,
                    unchecked_scale,
                    checked_opacity,
                    checked_scale
                );
                let mut last_selected = false;
                move |selected_value| {
                    let selected = (*selected_value).eq(&value);
                    if selected == last_selected {
                        return;
                    }
                    last_selected = selected;
                    if selected {
                        animate(
                            true,
                            &event_loop_proxy,
                            &unchecked_opacity,
                            &unchecked_scale,
                            &checked_opacity,
                            &checked_scale,
                        );
                    } else {
                        animate(
                            false,
                            &event_loop_proxy,
                            &unchecked_opacity,
                            &unchecked_scale,
                            &checked_opacity,
                            &checked_scale,
                        );
                    }
                }
            });

        property.lock().selected_value.notify();

        Self { item, property }
    }
}

fn animate(
    selected: bool,
    event_loop_proxy: &EventLoopProxy,
    unchecked_opacity: &Shared<f32>,
    unchecked_scale: &Shared<f32>,
    checked_opacity: &Shared<f32>,
    checked_scale: &Shared<f32>,
) {
    if let Some(mut a) = unchecked_opacity.get_animation() {
        a.stop()
    }
    if let Some(mut a) = checked_opacity.get_animation() {
        a.stop();
    }
    if let Some(mut a) = unchecked_scale.get_animation() {
        a.stop()
    }
    if let Some(mut a) = checked_scale.get_animation() {
        a.stop()
    }
    if selected {
        unchecked_opacity
            .animation_to_f32(0.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        unchecked_scale
            .animation_to_f32(0.5)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        checked_opacity
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        checked_scale
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);
    } else {
        unchecked_opacity
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        unchecked_scale
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        checked_opacity
            .animation_to_f32(0.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        checked_scale
            .animation_to_f32(1.5)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);
    }
}
