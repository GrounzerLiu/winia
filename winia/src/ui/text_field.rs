//! TextField — 文本输入组件（对齐 Jetpack Compose BasicTextField）
//!
//! 参考：旧版 winia v1 的 text_field 实现

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
    font_size: Option<crate::unit::TextUnit>,
    /// 是否启用（禁用：不聚焦不响应键盘，视觉 50% alpha——对标 Compose enabled）
    enabled: bool,
    /// 只读（可聚焦/选中，不可编辑——编辑键吞掉不生效，导航键保留）
    read_only: bool,
    /// 占位文字（值空时灰色显示——简化版；Compose 是 @Composable 参数）
    placeholder: Option<String>,
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
            font_size: None,
            enabled: true,
            read_only: false,
            placeholder: None,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 设置字号（支持 .sp() / .px() / f32，默认 14.sp()）
    pub fn font_size(mut self, size: impl Into<crate::unit::TextUnit>) -> Self {
        self.font_size = Some(size.into());
        self
    }

    /// 启用状态（禁用：不聚焦不响应键盘，视觉 50% alpha）
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// 只读（可聚焦/选中，不可编辑——编辑键吞掉不生效，导航键保留）
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// 占位文字（值空时灰色显示——对标 Compose placeholder，简化版）
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();
        let current = self.value.get();
        let content = current.text.clone();

        let theme = crate::ui::theme::WiniaTheme::colors();
        let font_size = self.font_size
            .unwrap_or(crate::unit::TextUnit::Sp(crate::unit::Sp(14.0)))
            .to_logical_px();
        // 禁用：文字 50% alpha（对标 Compose disabled 内容色）
        let color = if self.enabled {
            theme.on_surface
        } else {
            crate::modifier::Color::from_argb(
                (theme.on_surface.a as f32 * 0.5) as u8,
                theme.on_surface.r, theme.on_surface.g, theme.on_surface.b,
            )
        };

        // 占位文字：值空时显示（灰色），否则正常内容
        let show_placeholder = content.is_empty() && self.placeholder.is_some();
        let display_content = if show_placeholder {
            self.placeholder.as_deref().unwrap_or("").to_string()
        } else {
            content
        };
        let display_color = if show_placeholder {
            crate::modifier::Color::from_argb(
                (theme.on_surface_variant.a as f32 * 0.7) as u8,
                theme.on_surface_variant.r, theme.on_surface_variant.g, theme.on_surface_variant.b,
            )
        } else {
            color
        };

        // 光标闪烁状态（旧版风格）
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

        // 键盘事件处理（read_only：编辑键吞掉不生效；enabled=false：不注册）
        let value = self.value.clone();
        let on_change = std::sync::Arc::new(std::sync::Mutex::new(self.on_value_change));
        let kb_handler = {
            let v = value.clone();
            let cb = on_change.clone();
            let read_only = self.read_only;
            move |e: &crate::modifier::KbEvent| -> bool {
                if e.event_type != crate::modifier::KbEventType::KeyDown { return false; }
                let key = &e.key;
                // 导航键（只读时仍允许——对标 Compose readOnly 可选中）
                let is_nav = matches!(key,
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowLeft)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowRight)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::Home)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::End)
                );
                // 只读：编辑键直接消耗（不修改值）
                if read_only && !is_nav {
                    return true;
                }
                let mut val = v.get();
                let shift = e.is_shift_pressed;
                match key {
                    winit::keyboard::Key::Named(named) => match named {
                        winit::keyboard::NamedKey::Backspace => {
                            if val.selection.start != val.selection.end {
                                // 有选区：删除选区
                                let s = val.selection.start.min(val.selection.end);
                                let e = val.selection.start.max(val.selection.end);
                                val.text.replace_range(s..e, "");
                                val.selection = s..s;
                                v.set(val.clone());
                            } else if val.selection.start > 0 {
                                let prev = val.text.as_str()
                                    .grapheme_indices(true)
                                    .map(|(i, _)| i)
                                    .rev()
                                    .find(|&pos| pos < val.selection.start)
                                    .unwrap_or(0);
                                let range = prev..val.selection.start;
                                val.text.replace_range(range.clone(), "");
                                val.selection = prev..prev;
                                v.set(val.clone());
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Delete => {
                            if val.selection.start != val.selection.end {
                                let s = val.selection.start.min(val.selection.end);
                                let e = val.selection.start.max(val.selection.end);
                                val.text.replace_range(s..e, "");
                                val.selection = s..s;
                                v.set(val.clone());
                            } else if val.selection.start < val.text.len() {
                                let next = val.text.as_str()
                                    .grapheme_indices(true)
                                    .map(|(i, _)| i)
                                    .find(|&pos| pos > val.selection.start)
                                    .unwrap_or(val.text.len());
                                let range = val.selection.start..next;
                                val.text.replace_range(range.clone(), "");
                                val.selection = val.selection.start..val.selection.start;
                                v.set(val.clone());
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
                            let prev = val.text.as_str()
                                .grapheme_indices(true)
                                .map(|(i, _)| i)
                                .rev()
                                .find(|&pos| pos < val.selection.start);
                            if let Some(prev) = prev {
                                if shift {
                                    val.selection = prev..val.selection.end.max(prev);
                                } else {
                                    val.selection = prev..prev;
                                }
                                v.set(val.clone());
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            let next = val.text.as_str()
                                .grapheme_indices(true)
                                .map(|(i, _)| i)
                                .find(|&pos| pos > val.selection.start);
                            if let Some(next) = next {
                                if next <= val.text.len() {
                                    if shift {
                                        val.selection = val.selection.start.min(next)..next;
                                    } else {
                                        val.selection = next..next;
                                    }
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
            .padding(8.0)
            .text_content(
                display_content,
                font_size,
                display_color,
                crate::ui::text::FontWeight::NORMAL,
                crate::ui::text::FontSlant::Upright,
                usize::MAX, // unlimited lines
                crate::ui::TextAlign::Left,
                crate::ui::TextOverflow::Clip,
                true, // allow text wrapping
            );
        // 禁用：不聚焦不响应键盘（Compose disabled 语义）；否则聚焦 + 键盘
        let modifier = if self.enabled {
            modifier.focusable().on_key_event(kb_handler)
        } else {
            modifier
        };

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
        // IME 预输入回调（旧版风格——直接修改 text 内容）
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
        // 同步 selection_range 到节点（渲染高亮选区用）
        let sel = if current.selection.start != current.selection.end {
            Some(current.selection.start.min(current.selection.end)..current.selection.start.max(current.selection.end))
        } else { None };
        ctx.sync_selection_range(sel);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::modifier::ModifierElement;

    fn find_text_content(modifier: &Modifier) -> Option<String> {
        modifier.elements().iter().find_map(|el| {
            if let ModifierElement::TextContent { content, .. } = el {
                Some(content.clone())
            } else {
                None
            }
        })
    }

    fn has_focusable(modifier: &Modifier) -> bool {
        modifier.elements().iter().any(|el| matches!(el, ModifierElement::Focusable))
    }

    /// 构建 TextField 并取叶子节点 modifier
    fn build_field(field: TextField) -> Modifier {
        // build 内 tokio::spawn 光标闪烁——需要 runtime 上下文
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = rt.enter();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            field.build(ctx);
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        nodes[root].modifier.clone()
    }

    #[test]
    fn disabled_field_has_no_focusable() {
        let value = State::new(TextFieldValue::new("hi"));
        let m = build_field(TextField::new(value.clone(), |_| {}).enabled(false));
        assert!(!has_focusable(&m), "禁用字段不应可聚焦");
        // 禁用字段仍渲染文本（文字 50% alpha 在 build 内处理）
        assert!(find_text_content(&m).is_some(), "禁用字段仍显示内容");
    }

    #[test]
    fn enabled_field_has_focusable() {
        let value = State::new(TextFieldValue::new("hi"));
        let m = build_field(TextField::new(value.clone(), |_| {}));
        assert!(has_focusable(&m), "启用字段应可聚焦");
    }

    #[test]
    fn placeholder_shown_when_empty() {
        let value = State::new(TextFieldValue::new(""));
        let m = build_field(TextField::new(value.clone(), |_| {}).placeholder("请输入"));
        let content = find_text_content(&m).unwrap_or_default();
        assert_eq!(content, "请输入", "空值显示 placeholder");
    }

    #[test]
    fn placeholder_hidden_when_has_content() {
        let value = State::new(TextFieldValue::new("已有内容"));
        let m = build_field(TextField::new(value.clone(), |_| {}).placeholder("请输入"));
        let content = find_text_content(&m).unwrap_or_default();
        assert_eq!(content, "已有内容", "有值时显示真实内容");
    }
}
