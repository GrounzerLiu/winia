use crate::event::{DeviceId, MouseScrollDelta, TouchPhase};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseWheel {
    pub device_id: Option<DeviceId>,
    pub delta: MouseScrollDelta,
    pub phase: TouchPhase,
}