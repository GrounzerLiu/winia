use winit::dpi::LogicalPosition;
use winit::event::{DeviceId, Force, MouseButton, TouchPhase};
use crate::uib::ButtonState;

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
    pub device_id: DeviceId,
    pub state: PointerState,
    pub pointer: Pointer,
    pub x: f32,
    pub y: f32,
}