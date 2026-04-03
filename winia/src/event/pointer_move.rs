use crate::event::{DeviceId, PointerSource};

#[derive(Clone, Debug)]
pub struct PointerMoved {
    pub device_id: Option<DeviceId>,
    pub x: f32,
    pub y: f32,
    pub primary: bool,
    pub source: PointerSource,
}