use winit::event::{DeviceId, ElementState, MouseButton};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonState {
    Pressed,
    Moved,
    Released,
}

impl From<ElementState> for ButtonState {
    fn from(value: ElementState) -> Self {
        match value {
            ElementState::Pressed => ButtonState::Pressed,
            ElementState::Released => ButtonState::Released,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub device_id: DeviceId,
    pub state: ButtonState,
    pub button: MouseButton,
    pub x: f32,
    pub y: f32,
}