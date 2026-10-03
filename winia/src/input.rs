//! 输入子系统——指针事件、手势识别（对标 Compose pointerInput 语义）
pub(crate) mod gesture;
pub mod events;
pub use events::{
    KbEvent, KbEventType, PenKind, PointerButton, PointerEvent, PointerEventType, PointerKind,
};
