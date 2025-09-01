use crate::shared::{Children, Gettable, Settable, Shared, SharedDrawable};
use crate::ui::app::{EventLoopProxy, WindowContext};
use crate::ui::component::{ImageDrawable, ImageExt, RippleExt, ScaleMode};
use crate::ui::item::{Alignment, ItemData};
use crate::ui::layout::StackExt;
use crate::ui::theme::color;
use crate::ui::Item;
use clonelet::clone;
use proc_macro::item;
use skia_safe::{Color, Path};
use std::time::Duration;

const CHECKBOX_SELECTED_ICON: &[u8] = include_bytes!("assets/icon/check_box_selected.svg");
const CHECKBOX_UNSELECTED_ICON: &[u8] = include_bytes!("assets/icon/check_box_unselected.svg");

#[derive(Clone)]
struct CheckboxProperty<> {
    on_checked_changed: Shared<Box<dyn Fn(bool) + Send>>,
}

#[item(selected: impl Into<Shared<bool>>)]
pub struct Checkbox {
    item: Item,
    property: Shared<CheckboxProperty>,
}
impl Checkbox {
    pub fn on_checked_changed<F: Fn(bool) + Send + 'static>(
        self,
        on_checked_changed: F,
    ) -> Self {
        self.property.lock().on_checked_changed = Shared::from_static(Box::new(on_checked_changed));
        self
    }
}

impl Checkbox {
    pub fn new(
        window_context: &WindowContext,
        checked: impl Into<Shared<bool>>,
    ) -> Self {
        let w = window_context;
        let checked = checked.into();
        let event_loop_proxy = window_context.event_loop_proxy().clone();
        let property = Shared::from(CheckboxProperty {
            on_checked_changed: Shared::from_static(Box::new({
                let checked = checked.clone();
                move |it: bool| {
                    checked.set(it);
                }
            }))
        });
        let unselected_opacity = Shared::from(1.0);
        let unselected_scale = Shared::from(1.0);
        let selected_opacity = Shared::from(0.0);
        let selected_scale = Shared::from(1.5);
        let item = window_context
            .stack(
                Children::new()
                    + w.stack(
                    Children::new()
                        + w.image({
                        let image =
                            ImageDrawable::from_bytes(CHECKBOX_UNSELECTED_ICON, true)
                                .unwrap();
                        SharedDrawable::from_static(Box::new(image))
                    })
                           .color(Shared::<Option<Color>>::from_dynamic(
                               [w.theme().into()].into(),
                               {
                                   let theme = w.theme().clone();
                                   move || theme.lock().get_color(color::ON_SURFACE_VARIANT).map(|color| *color)
                               },
                           ))
                           .oversize_scale_mode(ScaleMode::Stretch)
                           .oversize_scale_mode(ScaleMode::Stretch)
                           .item()
                           .size(20.0, 20.0)
                           .opacity(&unselected_opacity)
                           .scale_x(&unselected_scale)
                           .scale_y(&unselected_scale)
                        + w.image({
                        let image =
                            ImageDrawable::from_bytes(CHECKBOX_SELECTED_ICON, true)
                                .unwrap();
                        SharedDrawable::from_static(Box::new(image))
                    })
                           .color(Shared::<Option<Color>>::from_dynamic(
                               [w.theme().into()].into(),
                               {
                                   let theme = w.theme().clone();
                                   move || theme.lock().get_color(color::PRIMARY).map(|color| *color)
                               },
                           ))
                           .oversize_scale_mode(ScaleMode::Stretch)
                           .oversize_scale_mode(ScaleMode::Stretch)
                           .item()
                           .size(20.0, 20.0)
                           .opacity(&selected_opacity)
                           .scale_x(&selected_scale)
                           .scale_y(&selected_scale),
                )
                       .item()
                       .size(20.0, 20.0)
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
                let checked = checked.clone();
                let property = property.clone();
                move |_| {
                    let mut checked_value = checked.get();
                    checked_value = !checked_value;
                    let on_checked_changed = property.lock().on_checked_changed.clone();
                    on_checked_changed.lock()(checked_value);
                }
            });


        checked.add_specific_observer(item.data().get_id(), {
                clone!(
                    event_loop_proxy,
                    unselected_opacity,
                    unselected_scale,
                    selected_opacity,
                    selected_scale
                );
                let mut last_checked = false;
                move |checked| {
                    if *checked == last_checked {
                        return;
                    }
                    last_checked = *checked;
                    if *checked {
                        animate(
                            true,
                            &event_loop_proxy,
                            &unselected_opacity,
                            &unselected_scale,
                            &selected_opacity,
                            &selected_scale,
                        );
                    } else {
                        animate(
                            false,
                            &event_loop_proxy,
                            &unselected_opacity,
                            &unselected_scale,
                            &selected_opacity,
                            &selected_scale,
                        );
                    }
                }
            });

        checked.notify();

        Self { item, property }
    }
}

fn animate(
    selected: bool,
    event_loop_proxy: &EventLoopProxy,
    unselected_opacity: &Shared<f32>,
    unselected_scale: &Shared<f32>,
    selected_opacity: &Shared<f32>,
    selected_scale: &Shared<f32>,
) {
    if let Some(mut a) = unselected_opacity.get_animation() {
        a.stop()
    }
    if let Some(mut a) = selected_opacity.get_animation() {
        a.stop();
    }
    if let Some(mut a) = unselected_scale.get_animation() {
        a.stop()
    }
    if let Some(mut a) = selected_scale.get_animation() {
        a.stop()
    }
    if selected {
        unselected_opacity
            .animation_to_f32(0.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        unselected_scale
            .animation_to_f32(0.5)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        selected_opacity
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        selected_scale
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);
    } else {
        unselected_opacity
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        unselected_scale
            .animation_to_f32(1.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        selected_opacity
            .animation_to_f32(0.0)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);

        selected_scale
            .animation_to_f32(1.5)
            .duration(Duration::from_millis(200))
            .start(event_loop_proxy);
    }
}
