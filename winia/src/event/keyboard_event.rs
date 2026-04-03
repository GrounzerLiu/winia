use crate::event::{DeviceId, KeyEvent};

#[derive(Debug, Clone, PartialEq)]
pub struct KeyboardInput {
    pub device_id: Option<DeviceId>,
    pub key_event: KeyEvent,
    pub is_synthetic: bool,
}