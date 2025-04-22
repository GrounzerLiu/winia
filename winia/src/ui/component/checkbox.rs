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

const CHECKBOX_SELECTED_ICON: &[u8] = include_bytes!("assets/icon/check_box_selected.svg");
const CHECKBOX_UNSELECTED_ICON: &[u8] = include_bytes!("assets/icon/check_box_unselected.svg");

// #[derive(Clone)]
// struct CheckboxProperty<T> {
//     value: T,
//     selected_value: Shared<T>,
// }

#[item(selected: impl Into<Shared<bool>>)]
pub struct Checkbox {
    item: Item,
    // property: Shared<CheckboxProperty<T>>,
}

impl Checkbox {
    pub fn new(
        window_context: &WindowContext,
        selected: impl Into<Shared<bool>>,
    ) -> Self {
        let w = window_context;
        let event_loop_proxy = window_context.event_loop_proxy().clone();
        // let property = Shared::from(CheckboxProperty {
        //     value: value.clone(),
        //     selected_value: selected_value.into(),
        // });
        let selected = selected.into();
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
                                   move || theme.lock().get_color(color::ON_SURFACE_VARIANT)
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
                                   move || theme.lock().get_color(color::PRIMARY)
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
                       // .clip(true)
                       // .clip_shape({
                       //     let shape: Box<dyn Fn(&mut ItemData) -> Path + Send> =
                       //         Box::new(|item_data| {
                       //             let display_parameter = item_data.get_display_parameter();
                       //             Path::circle(
                       //                 (
                       //                     display_parameter.width / 2.0 + display_parameter.x(),
                       //                     display_parameter.height / 2.0 + display_parameter.y(),
                       //                 ),
                       //                 display_parameter.width / 2.0,
                       //                 None,
                       //             )
                       //         });
                       //     Shared::from(shape)
                       // }),
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
                let selected = selected.clone();
                move |_| {
                    let selected_value = selected.get();
                    selected.set(!selected_value);
                }
            });


        selected.add_specific_observer(item.data().get_id(), {
                clone!(
                    event_loop_proxy,
                    unselected_opacity,
                    unselected_scale,
                    selected_opacity,
                    selected_scale
                );
                let mut last_selected = false;
                move |selected| {
                    if *selected == last_selected {
                        return;
                    }
                    last_selected = *selected;
                    if *selected {
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

        selected.notify();

        Self { item }
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
