//! TextField — 文本输入组件（对齐 Jetpack Compose BasicTextField）
//!
//! 参考：D:\winia\winia\src\ui\widget\text_field.rs

use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::modifier::Modifier;
use std::ops::Range;

// ═══════════════════════════════════════════════════════════
// TextFieldValue — 文本输入状态
// ═══════════════════════════════════════════════════════════

/// 文本输入状态（对齐 Compose TextFieldValue）
#[derive(Clone, PartialEq)]
pub struct TextFieldValue {
    pub text: String,
    /// 光标/选区范围（start == end 表示无选区仅光标）
    pub selection: Range<usize>,
}

impl TextFieldValue {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self { text, selection: len..len }
    }
}

/// 文本变更描述
#[derive(Clone)]
pub enum TextChange {
    Inserted { index: usize, text: String },
    Deleted { range: Range<usize> },
}

impl TextChange {
    pub fn apply_to(&self, value: &mut TextFieldValue) {
        match self {
            TextChange::Inserted { index, text } => {
                value.text.insert_str(*index, text);
                value.selection = (index + text.len())..(index + text.len());
            }
            TextChange::Deleted { range } => {
                value.text.drain(range.clone());
                value.selection = range.start..range.start;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// TextField — composable widget
// ═══════════════════════════════════════════════════════════

pub struct TextField {
    value: State<TextFieldValue>,
    on_value_change: Box<dyn Fn(TextFieldValue) + Send + Sync>,
    modifier: Modifier,
}

impl TextField {
    pub fn new(
        value: State<TextFieldValue>,
        on_value_change: impl Fn(TextFieldValue) + Send + Sync + 'static,
    ) -> Self {
        Self {
            value,
            on_value_change: Box::new(on_value_change),
            modifier: Modifier::new(),
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();
        let current = self.value.get();
        let content = current.text.clone();

        let theme = crate::ui::theme::WiniaTheme::colors();
        let font_size = 14.0;
        let color = theme.on_surface;

        // 光标闪烁状态（D:\winia 风格）
        let cursor_visible = ctx.remember(|| true);
        let cv = cursor_visible.clone();
        let blink_started = ctx.remember(|| false);
        if !blink_started.get() {
            blink_started.set(true);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    cv.update(|v| *v = !*v);
                }
            });
        }

        // 键盘事件处理
        let value = self.value.clone();
        let on_change = std::sync::Arc::new(std::sync::Mutex::new(self.on_value_change));
        let kb_handler = {
            let v = value.clone();
            let cb = on_change.clone();
            move |e: &crate::modifier::KbEvent| -> bool {
                if e.event_type != crate::modifier::KbEventType::KeyDown { return false; }
                let mut val = v.get();
                let key = &e.key;
                match key {
                    winit::keyboard::Key::Named(named) => match named {
                        winit::keyboard::NamedKey::Backspace => {
                            if val.selection.start == val.selection.end && val.selection.start > 0 {
                                let change = TextChange::Deleted { range: (val.selection.start - 1)..val.selection.start };
                                change.apply_to(&mut val);
                                v.set(val.clone());
                                if let Ok(cb) = cb.lock() { cb(val); }
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Delete => {
                            if val.selection.start == val.selection.end && val.selection.start < val.text.len() {
                                let change = TextChange::Deleted { range: val.selection.start..(val.selection.start + 1) };
                                change.apply_to(&mut val);
                                v.set(val.clone());
                                if let Ok(cb) = cb.lock() { cb(val); }
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Enter => {
                            let change = TextChange::Inserted { index: val.selection.start, text: "\n".into() };
                            change.apply_to(&mut val);
                            v.set(val.clone());
                            if let Ok(cb) = cb.lock() { cb(val); }
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowLeft => {
                            if val.selection.start > 0 {
                                val.selection = (val.selection.start - 1)..(val.selection.start - 1);
                                v.set(val.clone());
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            eprintln!("[kb] ArrowRight");
                            if val.selection.start < val.text.len() {
                                val.selection = (val.selection.start + 1)..(val.selection.start + 1);
                                v.set(val.clone());
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Home => {
                            val.selection = 0..0;
                            v.set(val.clone());
                            return true;
                        }
                        winit::keyboard::NamedKey::End => {
                            let len = val.text.len();
                            val.selection = len..len;
                            v.set(val.clone());
                            return true;
                        }
                        _ => {}
                    },
                    winit::keyboard::Key::Character(c) => {
                        if c.is_empty() { return false; }
                        let change = TextChange::Inserted { index: val.selection.start, text: c.to_string() };
                        change.apply_to(&mut val);
                        v.set(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                        return true;
                    }
                    _ => {}
                }
                false
            }
        };

        let modifier = self.modifier
            .focusable()
            .padding(8.0)
            .push(crate::modifier::ModifierElement::TextContent {
                content,
                font_size,
                color,
                font_weight: crate::ui::text::FontWeight::NORMAL,
                font_style: crate::ui::text::FontSlant::Upright,
                max_lines: usize::MAX, // unlimited lines
                align: crate::ui::TextAlign::Left,
                overflow: crate::ui::TextOverflow::Clip,
                soft_wrap: true, // allow text wrapping
            })
            .on_key_event(kb_handler);

        ctx.start_leaf(key, modifier);

        // 设置光标位置到节点
        ctx.set_current_node_cursor(current.selection.start, cursor_visible.get());
        // 设置点击回调和更新 value.selection/光标
        if let Some(node_id) = ctx.current_node_id() {
            if let Some(root) = ctx.layout_root() {
                if let Some(node) = crate::layout::node::find_node_by_id(root, node_id) {
                    let v = value.clone();
                    node.cursor_callback.borrow_mut().replace(Box::new(move |idx| {
                        eprintln!("[cursor_callback] idx={}", idx);
                        v.update(|val| { val.selection.start = idx; val.selection.end = idx; });
                    }));
                }
            }
        }
        ctx.end_node();
    }
}

impl Default for TextField {
    fn default() -> Self {
        Self::new(
            State::new(TextFieldValue::new("")),
            |_| {},
        )
    }
}
