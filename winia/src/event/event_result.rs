use crate::event::MouseScrollDelta;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventResult {
    Consumed,
    Ignored,
    MouseWheel(MouseScrollDelta),
}

impl EventResult {
    pub fn is_consumed(&self) -> bool {
        self == &EventResult::Consumed
    }
}