use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;
use skia_safe::{Color, Path};
use proc_macro::item;
use crate::shared::{Children, Gettable, Settable, Shared, SharedDrawable};
use crate::ui::app::WindowContext;
use crate::ui::component::{ImageDrawable, ImageExt, RectangleExt, RippleExt, ScaleMode};
use crate::ui::Item;
use crate::ui::item::{Alignment, ItemData};
use crate::ui::layout::StackExt;

const RADIO_BUTTON_CHECKED_ICON:&[u8] = include_bytes!("assets/icon/radio_button_checked.svg");
const RADIO_BUTTON_UNCHECKED_ICON:&[u8] = include_bytes!("assets/icon/radio_button_unchecked.svg");

struct RadioButtonProperty {
    selected: Shared<bool>
}

#[item]
pub struct RadioButton {
    item: Item,
    property: Shared<RadioButtonProperty>,
}

impl RadioButton {
    pub fn new(window_context: &WindowContext) -> Self {
        let event_loop_proxy = window_context.event_loop_proxy().clone();
        let property = Shared::from(RadioButtonProperty {
            selected: Shared::from(false),
        });
        let unchecked_opacity = Shared::from(1.0);
        let unchecked_scale = Shared::from(1.0);
        let checked_opacity = Shared::from(0.0);
        let checked_scale = Shared::from(1.5);
        let item = window_context.stack(Children::new() +
            window_context.stack(Children::new()+
                window_context.image({
                    let image = ImageDrawable::from_bytes(RADIO_BUTTON_UNCHECKED_ICON, true).unwrap();
                    SharedDrawable::from_static(
                        Box::new(image)
                    )
                })
                    // .color(Some(Color::BLACK))
                    .oversize_scale_mode(ScaleMode::Stretch)
                    .oversize_scale_mode(ScaleMode::Stretch)
                    .item().size(20.0, 20.0)
                    .opacity(&unchecked_opacity)
                    .scale_x(&unchecked_scale)
                    .scale_y(&unchecked_scale)+
                window_context.image({
                    let image = ImageDrawable::from_bytes(RADIO_BUTTON_CHECKED_ICON, true).unwrap();
                    SharedDrawable::from_static(
                        Box::new(image)
                    )
                })
                    // .color(Some(Color::BLACK))
                    .oversize_scale_mode(ScaleMode::Stretch)
                    .oversize_scale_mode(ScaleMode::Stretch)
                    .item().size(20.0, 20.0)
                    .opacity(&checked_opacity)
                    .scale_x(&checked_scale)
                    .scale_y(&checked_scale)
            ).item().size(20.0, 20.0)
                .clip(false)
                .clip_shape({
                    let shape: Box<dyn Fn(&mut ItemData) -> Path + Send> = Box::new(|item_data| {
                        let display_parameter = item_data.get_display_parameter();
                        Path::circle(
                            (
                                display_parameter.width / 2.0 + display_parameter.x(),
                                display_parameter.height / 2.0 + display_parameter.y()
                            ),
                            display_parameter.width / 2.0,
                            None
                        )
                    });
                    Shared::from(shape)
                })
        ).item().align_content(Alignment::Center).size(40.0, 40.0)
            .background(window_context.ripple().item().clip(true)
                .clip_shape({
                    let shape: Box<dyn Fn(&mut ItemData) -> Path + Send> = Box::new(|item_data| {
                        let display_parameter = item_data.get_display_parameter();
                        Path::circle(
                            (
                                display_parameter.width / 2.0 + display_parameter.x(),
                                display_parameter.height / 2.0 + display_parameter.y()
                            ),
                            display_parameter.width / 2.0,
                            None
                        )
                    });
                    Shared::from(shape)
                }))
            .on_click({
                let property = property.clone();
                
                move |_|{
                    let selected = property.lock().selected.get();
                    unchecked_opacity.get_animation().map(|mut a|{
                        a.stop()
                    });
                    checked_opacity.get_animation().map(|mut a|{
                        a.stop();
                    });
                    unchecked_scale.get_animation().map(|mut a|{
                        a.stop()
                    });
                    checked_scale.get_animation().map(|mut a|{
                        a.stop()
                    });
                    if selected {
                        unchecked_opacity.animation_to_f32(1.0)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        unchecked_scale.animation_to_f32(1.0)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        checked_opacity.animation_to_f32(0.0)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        checked_scale.animation_to_f32(1.5)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        property.lock().selected.set(false);
                    } else {
                        unchecked_opacity.animation_to_f32(0.0)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        unchecked_scale.animation_to_f32(0.5)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        checked_opacity.animation_to_f32(1.0)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        checked_scale.animation_to_f32(1.0)
                            .duration(Duration::from_millis(200))
                            .start(&event_loop_proxy);
                        
                        property.lock().selected.set(true);
                    }
                }
            });
        Self { item, property }
    }
}