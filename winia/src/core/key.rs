//! 键盘事件类型，对齐 Jetpack Compose 的 `androidx.compose.ui.input.key`

use crate::modifier::Modifier;
use crate::modifier::ModifierElement;
use std::sync::Arc;
use winit::keyboard::Key as WinitKey;

/// 键盘事件（对齐 Compose KeyEvent）
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// 按键（winit 定义，含 NamedKey 和 Character 等）
    pub key: WinitKey,
    /// 事件类型
    pub event_type: KeyEventType,
    /// 修饰键
    pub is_alt_pressed: bool,
    pub is_ctrl_pressed: bool,
    pub is_shift_pressed: bool,
    pub is_meta_pressed: bool,
}

/// 键盘事件类型（对齐 Compose KeyEventType）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventType {
    Unknown,
    KeyDown,
    KeyUp,
}

// ── Modifier 扩展 ──

pub trait ModifierKeyEventExt {
    /// 按键事件：事件从焦点节点向上冒泡（onKeyEvent）。
    fn on_key_event(self, handler: impl Fn(&KeyEvent) -> bool + Send + Sync + 'static) -> Self;
    /// 预拦截按键事件：事件从根向下分派（onPreviewKeyEvent）。
    fn on_pre_key_event(self, handler: impl Fn(&KeyEvent) -> bool + Send + Sync + 'static) -> Self;
}

impl ModifierKeyEventExt for Modifier {
    fn on_key_event(self, handler: impl Fn(&KeyEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::KeyEvent {
            on_key: Some(Arc::new(handler)),
            on_pre_key: None,
        })
    }

    fn on_pre_key_event(self, handler: impl Fn(&KeyEvent) -> bool + Send + Sync + 'static) -> Self {
        self.push(ModifierElement::KeyEvent {
            on_key: None,
            on_pre_key: Some(Arc::new(handler)),
        })
    }
}
