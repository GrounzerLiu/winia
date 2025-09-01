use crate::core::next_id;
use crate::shared::{Gettable, Settable, Shared, SharedBool, SharedColor, SharedF32};
use crate::ui::animation::{AnimationExt, Target};
use crate::ui::app::WindowContext;
use crate::ui::component::RectangleExt;
use crate::ui::item::{Alignment, ItemState, Pointer, PointerState, Size};
use crate::ui::layout::StackExt;
use crate::ui::theme::color;
use crate::ui::Item;
use clonelet::clone;
use material_colors::blend::cam16_ucs;
use material_colors::color::Argb;
use proc_macro::item;
use skia_safe::Color;
use std::ops::Not;
use std::time::{Duration, Instant};
use winit::event::MouseButton;
use winit::keyboard::{Key, NamedKey};

#[item(selected: impl Into<Shared<bool>>)]
pub struct Switch {
    item: Item,
    // property: Shared<CheckboxProperty<T>>,
}

fn interpolate_f32(start: f32, end: f32, progress: f32) -> f32 {
    start + (end - start) * progress
}

fn interpolate_color(start: &Color, end: &Color, progress: f32) -> Color {
    let progress = progress as f64;
    let start_a = start.a() as f64;
    let start_argb = Argb::new(255, start.r(), start.g(), start.b());
    let end_a = end.a() as f64;
    let end_argb = Argb::new(255, end.r(), end.g(), end.b());
    let blend_a = start_a + (end_a - start_a) * progress;
    let blend_argb = cam16_ucs(start_argb, end_argb, progress);
    let a = blend_a as u8;
    let r = blend_argb.red;
    let g = blend_argb.green;
    let b = blend_argb.blue;
    Color::from_argb(a, r, g, b)
}
impl Switch {
    pub fn new(
        window_context: &WindowContext,
        selected: impl Into<Shared<bool>>,
    ) -> Self {
        let w = window_context;
        let selected = selected.into();
        let theme = w.theme().clone();
        let handle_start_color = *theme.lock().get_color(color::OUTLINE).unwrap();
        let handle_end_color = *theme.lock().get_color(color::ON_PRIMARY).unwrap();
        let track_start_color = *theme.lock().get_color(color::SURFACE_CONTAINER_HIGHEST).unwrap();
        let track_end_color = *theme.lock().get_color(color::PRIMARY).unwrap();
        let track_outline_start_color = *theme.lock().get_color(color::OUTLINE).unwrap();
        let track_outline_end_color = *theme.lock().get_color(color::PRIMARY).unwrap();
        let handle_color = Shared::from_static(handle_start_color);
        let handle_size = Shared::from_static(Size::Fixed(16.0));
        let handle_pressed_size = 28.0;
        let handle_start_size = 16.0;
        let handle_end_size = 24.0;
        let handle_x = Shared::from_static(2.0);
        let track_color = Shared::from_static(track_start_color);
        let track_outline_color = Shared::from_static(track_outline_start_color);
        
        let focus_indicator_color = SharedColor::from(Color::TRANSPARENT);
        
        let progress = SharedF32::from_static(0.0);
        let pressed = SharedBool::from_static(false);
        progress.add_specific_observer(
            next_id(),
            {
                clone!(handle_color, handle_size, track_color, track_outline_color, handle_x, pressed);
                move |v| {
                    let progress = *v;
                    // println!("progress: {}", progress);
                    handle_color.set(interpolate_color(&handle_start_color, &handle_end_color, progress));
                    if !pressed.get() {
                        handle_size.set(Size::Fixed(interpolate_f32(handle_start_size, handle_end_size, progress)));
                    }
                    track_color.set(interpolate_color(&track_start_color, &track_end_color, progress));
                    track_outline_color.set(interpolate_color(&track_outline_start_color, &track_outline_end_color, progress));
                    handle_x.set(interpolate_f32(2.0, 22.0, progress));
                }
            }
        );
        
        let item = w.stack(
            w.stack(
                w.rectangle(&handle_color).radius(
                    SharedF32::from_dynamic(
                        [handle_size.to_observable()].into(),
                        {
                            let handle_size = handle_size.clone();
                            move || {
                                if let Size::Fixed(size) = handle_size.get() {
                                    size / 2.0
                                } else {
                                    0.0
                                }
                            }
                        }
                    )
                ).item().size(&handle_size, &handle_size)
            ).item().align_content(Alignment::Center).size(28, 28).margin_start(&handle_x)
        ).item().clip(false)
            .align_content(Alignment::CenterStart).size(52, 32)
            .background(
                w.rectangle(&track_color).outline_color(&track_outline_color).outline_width(2).radius(18).item()
            )
            .foreground(
                w.rectangle(Color::TRANSPARENT).outline_color(&focus_indicator_color).outline_width(3).outline_offset(5).radius(18).item()
            );
        
        let item_id = item.data().get_id();

        selected.add_specific_observer(
            item_id,
            {
                let event_loop_proxy = w.event_loop_proxy().clone();
                clone!(progress);
                move |selected| {
                    if *selected {
                        event_loop_proxy.animate(Target::Inclusion(vec![item_id]))
                                        .transformation({
                                            clone!(progress);
                                            move || {
                                                progress.set(1.0);
                                            }
                                        })
                                        .duration(Duration::from_millis(300))
                                        .start();
                    } else {
                        event_loop_proxy.animate(Target::Inclusion(vec![item_id]))
                                        .transformation({
                                            clone!(progress);
                                            move || {
                                                progress.set(0.0);
                                            }
                                        })
                                        .duration(Duration::from_millis(300))
                                        .start();
                    }
                }
            }
        );
        
        item.data().set_pointer_input({
            clone!(progress, w, selected);
            let mut started_x = 0.0;
            let mut started_progress = 0.0;
            let mut started_instant = Instant::now();
            move |item, input|{
                if let Pointer::Mouse { button: MouseButton::Left} = input.pointer {
                    match input.pointer_state {
                        PointerState::Started => {
                            started_x = input.x;
                            started_progress = progress.get();
                            started_instant = Instant::now();
                            pressed.set(true);
                            w.animate(Target::Inclusion(vec![item_id]))
                             .transformation({
                                 let handle_size = handle_size.clone();
                                 move || {
                                     handle_size.set(Size::Fixed(handle_pressed_size));
                                 }
                             })
                             .duration(Duration::from_millis(300))
                             .start();
                        }
                        PointerState::Moved => {
                            let delta_x = input.x - started_x;
                            progress.set(
                                (started_progress + (delta_x / 34.0)).clamp(0.0, 1.0)
                            )
                        }
                        _ => {
                            pressed.set(false);
                            let click = started_instant.elapsed().as_millis() < 200;
                            if click {
                                if selected.get() {
                                    selected.set(false);
                                } else {
                                    selected.set(true);
                                }
                            } else {
                                let progress_value = progress.get();
                                if progress_value > 0.5 {
                                    selected.set(true);
                                } else {
                                    selected.set(false);
                                }
                            }
                        }
                    }
                }
            }
        }).set_focus_next(|item|{
            if !item.get_enabled().get() {
                return true;
            }
            let focused = item.get_focused();
            if !focused.get() {
                focused.set(true);
                false
            } else {
                true
            }
        }).set_keyboard_input({
            clone!(selected);
            move |item, input| {
                if item.get_focused().get() {
                    match input.key_event.logical_key { 
                        Key::Named(NamedKey::Enter) => {
                            if input.key_event.state.is_pressed() {
                                selected.set(selected.get().not());
                            }
                            true
                        }
                        Key::Named(NamedKey::Escape) => {
                            if input.key_event.state.is_pressed() {
                                item.get_focused().set(false);
                            }
                            true
                        }
                        _=> {
                            false
                        }
                    }
                } else { 
                    false
                }
            }
        });
        
        item.data().get_state().add_specific_observer(
            item_id,
            {
                let focus_indicator_color = focus_indicator_color.clone();
                let theme = theme.clone();
                move |state| {
                    if let ItemState::Focused = state {
                        focus_indicator_color.set(
                            *theme.lock().get_color(color::SECONDARY).unwrap()
                        );
                    } else {
                        focus_indicator_color.set(
                            Color::TRANSPARENT
                        );
                    }
                }
            }
        );
        
        selected.notify();
        Self {
            item,
            // property: property.clone(),
        }
    }
}