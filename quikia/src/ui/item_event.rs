use skia_safe::Canvas;
use winit::event::{DeviceId, KeyEvent, MouseButton};
use crate::ui::{ButtonState, ImeAction, Item, MeasureMode, Pointer};
use crate::property::Gettable;
use crate::ui::mouse_event::MouseEvent;


pub struct ItemEvent {
    /// item, canvas
    pub draw_event: Box<dyn Fn(&mut Item, &Canvas)>,
    /// item, canvas
    pub on_draw: Box<dyn Fn(&mut Item, &Canvas)>,
    /// item, width_measure_mode, height_measure_mode
    pub measure_event: Box<dyn Fn(&mut Item, MeasureMode, MeasureMode)>,
    /// item, relative_x, relative_y
    pub layout_event: Box<dyn Fn(&mut Item, f32, f32)>,
    /// item, mouse_event
    pub mouse_input_event: Box<dyn Fn(&mut Item, MouseEvent) -> bool>,
    /// item, mouse_event
    pub on_mouse_input: Box<dyn Fn(&mut Item, MouseEvent) -> bool>,
    /// item, x, y
    pub on_cursor_moved: Box<dyn Fn(&mut Item, f32, f32) -> bool>,
    pub on_cursor_entered: Box<dyn Fn(&mut Item)>,
    pub on_cursor_exited: Box<dyn Fn(&mut Item)>,
    /// item, pointer_action
    // pub on_pointer_input: Box<dyn Fn(&mut Item, PointerAction) -> bool>,
    /// item, ime_action
    pub on_ime_input: Box<dyn Fn(&mut Item, ImeAction) -> bool>,
    /// item, device_id, key_event, is_synthetic
    pub on_keyboard_input: Box<dyn Fn(&mut Item, DeviceId, KeyEvent, bool) -> bool>,
}

impl ItemEvent {
    /// item, canvas
    pub fn set_draw_event(mut self, draw_event: impl Fn(&mut Item, &Canvas) + 'static) -> Self {
        self.draw_event = Box::new(draw_event);
        self
    }
    /// item, canvas
    pub fn set_on_draw(mut self, on_draw: impl Fn(&mut Item, &Canvas) + 'static) -> Self {
        self.on_draw = Box::new(on_draw);
        self
    }
    /// item, width_measure_mode, height_measure_mode
    pub fn set_measure_event(mut self, measure_event: impl Fn(&mut Item, MeasureMode, MeasureMode) + 'static) -> Self {
        self.measure_event = Box::new(measure_event);
        self
    }

    /// item, relative_x, relative_y
    pub fn set_layout_event(mut self, layout_event: impl Fn(&mut Item, f32, f32) + 'static) -> Self {
        self.layout_event = Box::new(layout_event);
        self
    }

    /// item, mouse_event
    pub fn set_mouse_input_event(mut self, mouse_input_event: impl Fn(&mut Item, MouseEvent) -> bool + 'static) -> Self {
        self.mouse_input_event = Box::new(mouse_input_event);
        self
    }

    /// item, mouse_event
    pub fn set_on_mouse_input(mut self, on_mouse_input: impl Fn(&mut Item, MouseEvent) -> bool + 'static) -> Self {
        self.on_mouse_input = Box::new(on_mouse_input);
        self
    }

    /// item, x, y
    pub fn set_on_cursor_moved(mut self, on_cursor_moved: impl Fn(&mut Item, f32, f32) -> bool + 'static) -> Self {
        self.on_cursor_moved = Box::new(on_cursor_moved);
        self
    }

    pub fn set_on_cursor_entered(mut self, on_cursor_entered: impl Fn(&mut Item) + 'static) -> Self {
        self.on_cursor_entered = Box::new(on_cursor_entered);
        self
    }

    pub fn set_on_cursor_exited(mut self, on_cursor_exited: impl Fn(&mut Item) + 'static) -> Self {
        self.on_cursor_exited = Box::new(on_cursor_exited);
        self
    }

    /// item, pointer_action
    // pub fn set_on_pointer_input(mut self, on_pointer_input: impl Fn(&mut Item, PointerAction) -> bool + 'static) -> Self {
    //     self.on_pointer_input = Box::new(on_pointer_input);
    //     self
    // }

    /// item, ime_action
    pub fn set_on_ime_input(mut self, on_ime_input: impl Fn(&mut Item, ImeAction) -> bool + 'static) -> Self {
        self.on_ime_input = Box::new(on_ime_input);
        self
    }

    /// item, device_id, key_event, is_synthetic
    pub fn set_on_keyboard_input(mut self, on_keyboard_input: impl Fn(&mut Item, DeviceId, KeyEvent, bool) -> bool + 'static) -> Self {
        self.on_keyboard_input = Box::new(on_keyboard_input);
        self
    }
}

impl Default for ItemEvent {
    fn default() -> Self {
        Self {
            draw_event: Box::new(|item, canvas| {
                let layout_params = item.get_display_parameter().clone();

                if let Some(background) = item.get_background().lock().as_mut() {
                    let background_layout_params = background.get_display_parameter_mut();
                    background_layout_params.parent_x = layout_params.x();
                    background_layout_params.parent_y = layout_params.y();
                    background.draw(canvas);
                }

                item.on_draw(canvas);
                item.get_children().lock().iter_mut().for_each(|child| {
                    let mut child_layout_params = child.get_display_parameter_mut();
                    child_layout_params.parent_x = layout_params.x();
                    child_layout_params.parent_y = layout_params.y();
                    child.draw(canvas);
                });

                if let Some(foreground) = item.get_foreground().lock().as_mut() {
                    let mut foreground_layout_params = foreground.get_display_parameter_mut();
                    foreground_layout_params.parent_x = layout_params.x();
                    foreground_layout_params.parent_y = layout_params.y();
                    foreground.draw(canvas);
                }
            }),
            on_draw: Box::new(|_, _| {}),
            measure_event: Box::new(|_, _, _| {}),
            layout_event: Box::new(|_, _, _| {}),
            mouse_input_event: Box::new(|item, event|{
                let layout_params = item.get_display_parameter().clone();
                
                match event.state {
                    ButtonState::Pressed => {
                        if !layout_params.contains(event.x, event.y){
                            return false;
                        }
                        if item.get_pressed_pointers().iter().find(|pointer| {
                            match pointer {
                                Pointer::Cursor { mouse_button } => {
                                    *mouse_button == event.button
                                }
                                Pointer::Touch { id } => {
                                    false
                                }
                            }
                        }).is_none() {
                            item.get_pressed_pointers_mut().push(Pointer::Cursor { mouse_button: event.button });
                        }
                    }
                    ButtonState::Moved => {
                        if item.get_pressed_pointers().iter().find(|pointer| {
                            match pointer {
                                Pointer::Cursor { mouse_button } => {
                                    *mouse_button == event.button
                                }
                                Pointer::Touch { id } => {
                                    false
                                }
                            }
                        }).is_none() {
                            return false;
                        }
                    }
                    ButtonState::Released => {
                        item.get_pressed_pointers_mut().retain(|pointer| {
                            match pointer {
                                Pointer::Cursor { mouse_button } => {
                                    *mouse_button != event.button
                                }
                                Pointer::Touch { id } => {
                                    true
                                }
                            }
                        });
                    }
                }

                if let Some(foreground) = item.get_foreground().lock().as_mut() {
                    if foreground.mouse_input(event) {
                        return true;
                    }
                }

                for child in item.get_children().lock().iter_mut() {
                    if child.mouse_input(event) {
                        return true;
                    }
                }

                if item.invoke_on_mouse_input(event) {
                    return true;
                }

                if let Some(background) = item.get_background().lock().as_mut() {
                    if background.mouse_input(event) {
                        return true;
                    }
                }
                
                if let Some(onc_click) = item.get_on_click() {
                    match event.state {
                        ButtonState::Released => {
                            if layout_params.contains(event.x, event.y) {
                                onc_click(Pointer::Cursor { mouse_button: event.button });
                                return true;
                            }
                        }
                        _=>{}
                    }
                }

                false
            }),
            on_mouse_input: Box::new(|_, _| {
                false
            }),
            on_cursor_moved: Box::new(|_, _, _| {
                false
            }),
            on_cursor_entered: Box::new(|_| {}),
            on_cursor_exited: Box::new(|_| {}),
            // on_pointer_input: Box::new(|_, _| {
            //     false
            // }),
            on_ime_input: Box::new(|_, _| {
                false
            }),
            on_keyboard_input: Box::new(|_, _, _, _| {
                false
            }),
        }
    }
}