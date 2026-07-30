//! TextField — 文本输入组件（对齐 Jetpack Compose BasicTextField）
//!
//! 参考：D:\winia\winia\src\ui\widget\text_field.rs

use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::modifier::Modifier;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

// ═══════════════════════════════════════════════════════════
// TextFieldValue — 文本输入状态
// ═══════════════════════════════════════════════════════════

/// 文本输入状态（对齐 Compose TextFieldValue）
#[derive(Clone, PartialEq)]
pub struct TextFieldValue {
    pub text: String,
    /// 光标/选区范围（start == end 表示无选区仅光标）
    pub selection: Range<usize>,
    /// IME 组合范围（预输入文本在 text 中的字节范围，None 表示无组合）
    pub composing_range: Option<Range<usize>>,
}

impl TextFieldValue {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self { text, selection: len..len, composing_range: None }
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
                            // 按 grapheme cluster 边界移动（对齐 D:\winia）
                            let prev = val.text.as_str()
                                .grapheme_indices(true)
                                .map(|(i, _)| i)
                                .rev()
                                .find(|&pos| pos < val.selection.start);
                            if let Some(prev) = prev {
                                val.selection = prev..prev;
                                v.set(val.clone());
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            // 按 grapheme cluster 边界移动（对齐 D:\winia）
                            let next = val.text.as_str()
                                .grapheme_indices(true)
                                .map(|(i, _)| i)
                                .find(|&pos| pos > val.selection.start);
                            if let Some(next) = next {
                                if next <= val.text.len() {
                                    val.selection = next..next;
                                    v.set(val.clone());
                                }
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

        // 设置光标位置和回调（用 node_stack 直接访问，find_node_by_id 因树未建立无效）
        ctx.set_current_node_cursor_and_callback(
            current.selection.start,
            cursor_visible.get(),
            Box::new({
                let v = value.clone();
                move |idx| {
                    v.update(|val| { val.selection.start = idx; val.selection.end = idx; });
                }
            }),
        );
        // IME 预输入回调（D:\winia 风格——直接修改 text 内容）
        {
            let v = value.clone();
            ctx.set_current_node_ime_callback(Box::new(move |text, cursor| {
                let mut val = v.get();
                // 删除旧的 composing range
                if let Some(ref comp_range) = val.composing_range.clone() {
                    val.text.replace_range(comp_range.clone(), "");
                    let len = comp_range.len();
                    let shift = len.min(val.selection.start.saturating_sub(comp_range.start));
                    val.selection = (val.selection.start - shift)..(val.selection.end - shift.min(val.selection.end));
                    val.composing_range = None;
                }
                // 插入新的预输入文本
                if !text.is_empty() {
                    let pos = val.selection.start;
                    val.text.insert_str(pos, text);
                    let new_len = text.len();
                    val.composing_range = Some(pos..(pos + new_len));
                    if let Some((start, end)) = cursor {
                        let s = (pos + start).min(val.text.len());
                        let e = (pos + end).min(val.text.len());
                        val.selection = s..e;
                    } else {
                        val.selection = (pos + new_len)..(pos + new_len);
                    }
                } else {
                    val.composing_range = None;
                }
                v.set(val);
            }));
        }
        // 同步 composing_range 到节点（渲染画下划线用）
        ctx.sync_composing_range(current.composing_range.clone());
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
