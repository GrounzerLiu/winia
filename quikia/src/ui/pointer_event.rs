use winit::dpi::LogicalPosition;
use winit::event::{DeviceId, Force, MouseButton, TouchPhase};
use crate::ui::ButtonState;

#[derive(Clone, Hash, Copy, Debug)]
pub enum Pointer {
    Cursor { mouse_button: MouseButton },
    Touch { id: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerState {
    Pressed,
    Moved,
    Released,
    Cancelled,
}

#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    device_id: DeviceId,
    state: PointerState,
    pointer: Pointer,
    x: f32,
    y: f32,
}