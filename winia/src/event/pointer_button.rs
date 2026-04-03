use crate::event::{ButtonSource, DeviceId, ElementState};

#[derive(Clone, Debug)]
pub struct PointerButton {
    pub device_id: Option<DeviceId>,
    pub state: ElementState,
    pub x: f32,
    pub y: f32,
    pub primary: bool,
    pub button: ButtonSource,
}