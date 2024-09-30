use std::rc::Rc;
use std::sync::Mutex;

use skia_safe::Canvas;
use crate::property::Gettable;

use crate::uib::{ButtonState, ImeAction, Item, MeasureMode, Pointer, PointerEvent, PointerState};
use crate::uib::MouseEvent;
use crate::uib::MulticastEvent;

pub type DrawEvent = dyn FnMut(&mut Item, &Canvas);
pub type MeasureEvent = dyn FnMut(&mut Item, MeasureMode, MeasureMode);
pub type LayoutEvent = dyn FnMut(&mut Item, f32, f32);
pub type FocusEvent = dyn FnMut(&mut Item, bool);
pub type MouseInputEvent = dyn FnMut(&mut Item, MouseEvent) -> bool;
pub type PointerInputEvent = dyn FnMut(&mut Item, PointerEvent) -> bool;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

pub struct ItemEvent {
    /// item, canvas
    pub draw_event: Rc<Mutex<DrawEvent>>,
    /// item, canvas
    pub on_draw: Rc<Mutex<DrawEvent>>,
    /// item, width_measure_mode, height_measure_mode
    pub measure_event: Rc<Mutex<MeasureEvent>>,
    /// item, relative_x, relative_y
    pub layout_event: Rc<Mutex<LayoutEvent>>,
    pub focus_event: MulticastEvent<FocusEvent>,
    /// item, mouse_event
    pub mouse_input_event: Rc<Mutex<MouseInputEvent>>,
    /// item, mouse_event
    pub on_mouse_input: MulticastEvent<MouseInputEvent>,
    pub on_pointer_input: MulticastEvent<PointerInputEvent>,
    pub orientation_changed_event: Rc<Mutex<dyn FnMut(&mut Item, Orientation)>>,
    pub on_orientation_changed: MulticastEvent<dyn FnMut(Orientation)>,
    pub on_focused: MulticastEvent<dyn FnMut(bool)>,
    // /// item, x, y
    // pub on_cursor_moved: Box<dyn Fn(&mut Item, f32, f32) -> bool>,
    // pub on_cursor_entered: Box<dyn Fn(&mut Item)>,
    // pub on_cursor_exited: Box<dyn Fn(&mut Item)>,
    // /// item, pointer_action
    // // pub on_pointer_input: Box<dyn Fn(&mut Item, PointerAction) -> bool>,
    // /// item, ime_action
    pub ime_input_event: Rc<Mutex<dyn FnMut(&mut Item, ImeAction)>>,
    pub on_ime_input: Rc<Mutex<dyn FnMut(&mut Item, ImeAction)>>,
    // /// item, device_id, key_event, is_synthetic
    // pub on_keyboard_input: Box<dyn Fn(&mut Item, DeviceId, KeyEvent, bool) -> bool>,
}

impl ItemEvent {
    /// item, canvas
    pub fn set_draw_event(mut self, draw_event: impl FnMut(&mut Item, &Canvas) + 'static) -> Self {
        self.draw_event = Rc::new(Mutex::new(draw_event));
        self
    }
    /// item, canvas
    pub fn set_on_draw(mut self, on_draw: impl FnMut(&mut Item, &Canvas) + 'static) -> Self {
        self.on_draw = Rc::new(Mutex::new(on_draw));
        self
    }
    /// item, width_measure_mode, height_measure_mode
    pub fn set_measure_event(mut self, measure_event: impl FnMut(&mut Item, MeasureMode, MeasureMode) + 'static) -> Self {
        self.measure_event = Rc::new(Mutex::new(measure_event));
        self
    }

    /// item, relative_x, relative_y
    pub fn set_layout_event(mut self, layout_event: impl FnMut(&mut Item, f32, f32) + 'static) -> Self {
        self.layout_event = Rc::new(Mutex::new(layout_event));
        self
    }

    pub fn set_focus_event(mut self, focus_event: impl FnMut(&mut Item, bool) + 'static) -> Self {
        self.focus_event = MulticastEvent::new();
        self
    }

    /// item, mouse_event
    pub fn set_mouse_input_event(mut self, mouse_input_event: impl FnMut(&mut Item, MouseEvent) -> bool + 'static) -> Self {
        self.mouse_input_event = Rc::new(Mutex::new(mouse_input_event));
        self
    }

    /// item, mouse_event
    pub fn set_on_mouse_input(mut self, on_mouse_input: impl FnMut(&mut Item, MouseEvent) -> bool + 'static) -> Self {
        self.on_mouse_input.set_event(Box::new(on_mouse_input));
        self
    }

    pub fn add_on_mouse_input(mut self, on_mouse_input: impl FnMut(&mut Item, MouseEvent) -> bool + 'static) -> Self {
        self.on_mouse_input.add_event(Box::new(on_mouse_input));
        self
    }

    pub fn set_on_pointer_input(mut self, on_pointer_input: impl FnMut(&mut Item, PointerEvent) -> bool + 'static) -> Self {
        self.on_pointer_input.set_event(Box::new(on_pointer_input));
        self
    }

    pub fn add_on_pointer_input(mut self, on_pointer_input: impl FnMut(&mut Item, PointerEvent) -> bool + 'static) -> Self {
        self.on_pointer_input.add_event(Box::new(on_pointer_input));
        self
    }

    pub fn set_on_ime_input(mut self, on_ime_input: impl FnMut(&mut Item, ImeAction) + 'static) -> Self {
        self.ime_input_event = Rc::new(Mutex::new(on_ime_input));
        self
    }


    /*
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
        }*/
}

impl Default for ItemEvent {
    fn default() -> Self {
        Self {
            draw_event: Rc::new(Mutex::new(|item: &mut Item, canvas: &Canvas| {
                let layout_params = item.get_display_parameter();

                if let Some(background) = item.get_background().lock().unwrap().as_mut() {
                    {
                        let mut background_layout_params = background.get_display_parameter_mut();
                        background_layout_params.parent_x = layout_params.x();
                        background_layout_params.parent_y = layout_params.y();
                    }
                    background.draw(canvas);
                }

                item.on_draw(canvas);
                item.get_children().lock().iter_mut().for_each(|child| {
                    {
                        let mut child_layout_params = child.get_display_parameter_mut();
                        child_layout_params.parent_x = layout_params.x();
                        child_layout_params.parent_y = layout_params.y();
                    }
                    child.draw(canvas);
                });

                if let Some(foreground) = item.get_foreground().lock().unwrap().as_mut() {
                    {
                        let mut foreground_layout_params = foreground.get_display_parameter_mut();
                        foreground_layout_params.parent_x = layout_params.x();
                        foreground_layout_params.parent_y = layout_params.y();
                    }
                    foreground.draw(canvas);
                }
            })),
            on_draw: Rc::new(Mutex::new(|_: &mut Item, _: &Canvas| {})),
            measure_event: Rc::new(Mutex::new(|_: &mut Item, _: MeasureMode, _: MeasureMode| {})),
            layout_event: Rc::new(Mutex::new(|_: &mut Item, _: f32, _: f32| {})),
            focus_event: MulticastEvent::new(),
            mouse_input_event: Rc::new(Mutex::new(|item: &mut Item, event: MouseEvent| {
                let layout_params = item.get_display_parameter().clone();

                match event.state {
                    ButtonState::Pressed => {
                        if !layout_params.contains(event.x, event.y) {
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

                if let Some(foreground) = item.get_foreground().lock().unwrap().as_mut() {
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

                if item.invoke_on_pointer_input(
                    PointerEvent {
                        device_id: event.device_id,
                        x: event.x,
                        y: event.y,
                        state: match event.state {
                            ButtonState::Pressed => {
                                PointerState::Pressed
                            }
                            ButtonState::Moved => {
                                PointerState::Moved
                            }
                            ButtonState::Released => {
                                PointerState::Released
                            }
                        },
                        pointer: Pointer::Cursor { mouse_button: event.button }
                    }
                ) {
                    return true;
                }

                if let Some(background) = item.get_background().lock().unwrap().as_mut() {
                    if background.mouse_input(event) {
                        return true;
                    }
                }

                if let Some(on_click) = item.get_on_click() {
                    match event.state {
                        ButtonState::Released => {
                            if layout_params.contains(event.x, event.y) {
                                on_click(Pointer::Cursor { mouse_button: event.button });
                                return true;
                            }
                        }
                        _ => {}
                    }
                }

                false
            })),
            on_mouse_input: MulticastEvent::new(),
            on_pointer_input: MulticastEvent::new(),
            orientation_changed_event: Rc::new(Mutex::new(|item: &mut Item, orientation: Orientation| {
                item.get_children().lock().iter_mut().for_each(|child| {
                    child.orientation_changed(orientation);
                });
                item.invoke_on_orientation_changed(orientation);
            })),
            on_orientation_changed: MulticastEvent::new(),
            on_focused: MulticastEvent::new(),
            ime_input_event: Rc::new(Mutex::new(|item: &mut Item, ime_action: ImeAction| {
                if item.get_focused().get() {
                    item.invoke_on_ime_input(ime_action);
                    return;
                }
                for child in item.get_children().lock().iter_mut() {
                    child.ime_input(ime_action.clone());
                }
            })),
            on_ime_input: Rc::new(Mutex::new(|_: &mut Item, _: ImeAction| {})),
        }
    }
}