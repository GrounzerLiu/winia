//! Pointer and keyboard event vocabulary.
//!
//! These are the shapes a hit test and a key dispatch hand around; they lived in `modifier.rs`,
//! which put the input vocabulary inside the chain that reacts to it.

use winit::keyboard::Key;

#[derive(Debug, Clone)]
pub struct KbEvent {
    pub key: winit::keyboard::Key,
    pub event_type: KbEventType,
    pub is_alt_pressed: bool,
    pub is_ctrl_pressed: bool,
    pub is_shift_pressed: bool,
    pub is_meta_pressed: bool,
    pub repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbEventType {
    Unknown,
    KeyDown,
    KeyUp,
}

/// 指针按钮
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Other(u16),
}

impl PointerKind {
    pub fn from_button_source(button: &winit::event::ButtonSource) -> Self {
        match button {
            winit::event::ButtonSource::Mouse(m) => PointerKind::Mouse {
                button: match m {
                    winit::event::MouseButton::Left => PointerButton::Primary,
                    winit::event::MouseButton::Right => PointerButton::Secondary,
                    winit::event::MouseButton::Middle => PointerButton::Middle,
                    other => PointerButton::Other(*other as u16),
                },
            },
            winit::event::ButtonSource::Touch { finger_id, force } => PointerKind::Touch {
                finger_id: finger_id.into_raw() as u64,
                force: force.map(|f| f.normalized(None) as f32),
            },
            winit::event::ButtonSource::TabletTool { kind, data, .. } => PointerKind::Pen {
                kind: match kind {
                    winit::event::TabletToolKind::Eraser => PenKind::Eraser,
                    _ => PenKind::Stylus,
                },
                pressure: data.force.map(|f| f.normalized(None) as f32),
            },
            _ => PointerKind::Mouse { button: PointerButton::Primary },
        }
    }
}

/// 指针事件类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEventType {
    Down,
    Up,
    Move,
    Scroll { delta: f32, is_vertical: bool },
}

/// 指针类型（对齐 Compose PointerType）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerKind {
    Mouse { button: PointerButton },
    Touch { finger_id: u64, force: Option<f32> },
    Pen { kind: PenKind, pressure: Option<f32> },
}

/// 触控笔类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenKind {
    Stylus,
    Eraser,
    Unknown,
}

/// 指针事件
#[derive(Debug, Clone)]
pub struct PointerEvent {
    pub event_type: PointerEventType,
    pub position: (f32, f32),
    pub scene_position: (f32, f32),
    pub kind: PointerKind,
    pub is_alt_pressed: bool,
    pub is_ctrl_pressed: bool,
    pub is_shift_pressed: bool,
    pub is_meta_pressed: bool,
}
