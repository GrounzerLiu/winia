//! TextField — 文本输入组件（对齐 Jetpack Compose BasicTextField）
//!
//! 参考：旧版 winia v1 的 text_field 实现 + Compose Foundation 1.11.4 源码
//! （KeyCommand/KeyMapping/UndoManager 语义）

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
// UndoManager — 撤销/重做（对齐 Compose UndoManager 简化版）
// ═══════════════════════════════════════════════════════════

/// 撤销/重做（对齐 Compose UndoManager 核心语义，双栈实现：
/// - `push` 记录编辑前状态；同文本只更新 selection（光标移动合并）
/// - 新编辑清空 redo 栈（undo 后输入 → 丢弃 redo 分支）
/// - undo/redo 需要调用方传入"当前状态"（当前状态在调用方，不在栈内——
///   双栈让 redo 能精确回到 undo 前的状态）
/// - 上限 100 条，超出丢最旧）
#[derive(Default)]
struct UndoManager {
    undo_stack: Vec<(String, Range<usize>)>,
    redo_stack: Vec<(String, Range<usize>)>,
}

const UNDO_MAX_SNAPSHOTS: usize = 100;

impl UndoManager {
    fn new() -> Self {
        Self::default()
    }

    /// 编辑前调用：记录 (text, selection) 快照；新编辑丢弃 redo 分支
    fn push(&mut self, text: &str, selection: &Range<usize>) {
        if let Some(last) = self.undo_stack.last_mut() {
            if last.0 == text {
                // 同文本：只更新 selection（光标移动不产生新条目）
                last.1 = selection.clone();
                return;
            }
        }
        self.undo_stack.push((text.to_string(), selection.clone()));
        self.redo_stack.clear();
        if self.undo_stack.len() > UNDO_MAX_SNAPSHOTS {
            self.undo_stack.remove(0);
        }
    }

    /// 撤销：当前状态入 redo 栈，返回上一快照
    fn undo(&mut self, current: (String, Range<usize>)) -> Option<(String, Range<usize>)> {
        let s = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(s)
    }

    /// 重做：当前状态入 undo 栈，返回 redo 快照
    fn redo(&mut self, current: (String, Range<usize>)) -> Option<(String, Range<usize>)> {
        let s = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(s)
    }
}

// ═══════════════════════════════════════════════════════════
// 词边界（对齐 Compose TextFieldPreparedSelection 的 getWordStart/
// getWordEnd 语义——词字符 = letterOrDigit，非 UAX#29 词规则：
// 后者把 "a.b" 当一词，Compose 标点即边界）
// ═══════════════════════════════════════════════════════════

/// 词字符（对标 Compose `Character.isLetterOrDigit`）
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// 前一个词的起始字节（Compose getWordStart：
/// 先跳过 pos 前的非词字符，再跳过词字符到词首）
fn word_prev(text: &str, pos: usize) -> usize {
    let ci: Vec<(usize, char)> = text.char_indices().collect();
    let pos = pos.min(text.len());
    // 第一个字节 >= pos 的字符（pos 可能落在字符内部——从该字符起算）
    let mut idx = ci.partition_point(|(b, _)| *b < pos);
    // 跳过词尾的非词字符
    while idx > 0 && !is_word_char(ci[idx - 1].1) {
        idx -= 1;
    }
    // 跳过词字符到词首
    while idx > 0 && is_word_char(ci[idx - 1].1) {
        idx -= 1;
    }
    ci.get(idx).map(|(b, _)| *b).unwrap_or(0)
}

/// 下一个词的起始字节（Compose getWordEnd：
/// 先跳过当前词字符到词尾，再跳过空白到下一词首）
fn word_next(text: &str, pos: usize) -> usize {
    let ci: Vec<(usize, char)> = text.char_indices().collect();
    let pos = pos.min(text.len());
    let mut idx = ci.partition_point(|(b, _)| *b < pos);
    // 跳过当前词的词字符（pos 在词中间 → 词尾）
    while idx < ci.len() && is_word_char(ci[idx].1) {
        idx += 1;
    }
    // 跳过空白/标点（到下一词首）
    while idx < ci.len() && !is_word_char(ci[idx].1) {
        idx += 1;
    }
    ci.get(idx).map(|(b, _)| *b).unwrap_or(text.len())
}

/// 行首（`\n` 近似——无布局信息时使用；与 caret_prev_line 同语义）
fn line_start(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let pos = pos.min(bytes.len());
    bytes[..pos]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// 行尾（不含换行；pos 在行末换行符前——对齐 Compose LINE_END 语义）
fn line_end(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let pos = pos.min(bytes.len());
    bytes[pos..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| pos + i)
        .unwrap_or(bytes.len())
}

// ═══════════════════════════════════════════════════════════
// 剪贴板（arboard——系统剪贴板；每次操作临时打开，无长驻句柄）
// ═══════════════════════════════════════════════════════════

fn clipboard_get_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn clipboard_set_text(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}

// ═══════════════════════════════════════════════════════════
// TextField 容器视觉（对齐 material3 TextField/OutlinedTextField 外观）
// ═══════════════════════════════════════════════════════════

/// 容器变体（对齐 material3 TextField（Filled）/ OutlinedTextField）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFieldVariant {
    /// 填充容器 + 底部指示线（M3 FilledTextField——top 4dp 圆角）
    Filled,
    /// 无填充 + 边框（M3 OutlinedTextField——四角 4dp 圆角）
    Outlined,
}

/// M3 TextField 状态色集合（对齐 `TextFieldDefaults.colors` 的默认值；
/// 状态优先级 disabled > error > focused > unfocused）
#[derive(Debug, Clone)]
pub struct TextFieldColors {
    pub text: crate::modifier::Color,
    pub disabled_text: crate::modifier::Color,
    /// 容器背景（Filled 用；Outlined 为透明）
    pub container: crate::modifier::Color,
    /// 光标（正常；error 时用 error_cursor）
    pub cursor: crate::modifier::Color,
    pub error_cursor: crate::modifier::Color,
    /// 指示线/边框
    pub indicator_focused: crate::modifier::Color,
    pub indicator_unfocused: crate::modifier::Color,
    pub indicator_disabled: crate::modifier::Color,
    pub indicator_error: crate::modifier::Color,
    /// label
    pub label_focused: crate::modifier::Color,
    pub label_unfocused: crate::modifier::Color,
    pub label_disabled: crate::modifier::Color,
    pub label_error: crate::modifier::Color,
    /// placeholder
    pub placeholder: crate::modifier::Color,
    pub disabled_placeholder: crate::modifier::Color,
    /// 支持文本
    pub supporting: crate::modifier::Color,
    pub disabled_supporting: crate::modifier::Color,
    pub error_supporting: crate::modifier::Color,
}

/// M3 默认禁用降级：基色 × alpha（disabled 色 = onSurface @ alpha）
fn alpha(c: crate::modifier::Color, a: f32) -> crate::modifier::Color {
    crate::modifier::Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b)
}

impl TextFieldColors {
    /// M3 FilledTextField 默认色（defaultTextFieldColors——1.4.0 tokens）：
    /// text onSurface、container surfaceContainerHighest、indicator/label 按状态、
    /// disabled 全系 onSurface@38%、cursor primary（error → error）
    pub fn filled_from_theme(theme: &crate::ui::theme::ThemeColors) -> Self {
        Self {
            text: theme.on_surface,
            disabled_text: alpha(theme.on_surface, 0.38),
            container: theme.surface_container_highest,
            cursor: theme.primary,
            error_cursor: theme.error,
            indicator_focused: theme.primary,
            indicator_unfocused: theme.on_surface_variant,
            indicator_disabled: alpha(theme.on_surface, 0.38),
            indicator_error: theme.error,
            label_focused: theme.primary,
            label_unfocused: theme.on_surface_variant,
            label_disabled: alpha(theme.on_surface, 0.38),
            label_error: theme.error,
            placeholder: theme.on_surface_variant,
            disabled_placeholder: alpha(theme.on_surface, 0.38),
            supporting: theme.on_surface_variant,
            disabled_supporting: alpha(theme.on_surface, 0.38),
            error_supporting: theme.error,
        }
    }

    /// M3 OutlinedTextField 默认色（defaultOutlinedTextFieldColors）：
    /// 容器透明、边框 unfocused outline / disabled onSurface@12%、其余同 Filled
    pub fn outlined_from_theme(theme: &crate::ui::theme::ThemeColors) -> Self {
        let mut c = Self::filled_from_theme(theme);
        c.container = crate::modifier::Color::TRANSPARENT;
        c.indicator_unfocused = theme.outline;
        c.indicator_disabled = alpha(theme.on_surface, 0.12);
        c
    }

    /// 状态解析（优先级 disabled > error > focused > unfocused——M3 同款）
    pub fn indicator_color(&self, enabled: bool, is_error: bool, focused: bool) -> crate::modifier::Color {
        if !enabled { self.indicator_disabled }
        else if is_error { self.indicator_error }
        else if focused { self.indicator_focused }
        else { self.indicator_unfocused }
    }

    pub fn label_color(&self, enabled: bool, is_error: bool, focused: bool) -> crate::modifier::Color {
        if !enabled { self.label_disabled }
        else if is_error { self.label_error }
        else if focused { self.label_focused }
        else { self.label_unfocused }
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
    /// 单行模式（Enter 吞掉不换行——对标 Compose singleLine）
    single_line: bool,
    /// 最大行数（对标 Compose maxLines，默认无限）
    max_lines: usize,
    /// 最小行数（对标 Compose minLines，默认 1——空内容也占位）
    min_lines: usize,
    /// 交互源（None = build 时内部 remember——对标 Compose TextField 可选注入）
    interaction_source: Option<crate::ui::interaction::MutableInteractionSource>,
    /// 错误状态（对标 Compose isError——文字/边框错误色，供按状态取色）
    is_error: bool,
    /// 容器变体（None = 无容器视觉——裸输入；Filled/Outlined 对齐 M3）
    variant: Option<TextFieldVariant>,
    /// 状态色（None = 按 variant 从主题生成——M3 默认）
    colors: Option<TextFieldColors>,
    /// 浮动 label（聚焦或非空时悬浮到容器顶部）
    label: Option<String>,
    /// 支持文本（容器底部外侧 12sp）
    supporting_text: Option<String>,
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
            single_line: false,
            max_lines: usize::MAX,
            min_lines: 1,
            interaction_source: None,
            is_error: false,
            variant: None,
            colors: None,
            label: None,
            supporting_text: None,
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

    /// 单行模式（Enter 吞掉不换行——对标 Compose singleLine）
    pub fn single_line(mut self, single_line: bool) -> Self {
        self.single_line = single_line;
        if single_line {
            self.max_lines = 1;
        }
        self
    }

    /// 最大行数（默认无限——对标 Compose maxLines）
    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = max_lines;
        self
    }

    /// 最小行数（默认 1——空内容也占位，对标 Compose minLines）
    pub fn min_lines(mut self, min_lines: usize) -> Self {
        self.min_lines = min_lines;
        self
    }

    /// 注入交互源（hoist——TextField 的 focus 状态发射到此源；
    /// 不传则内部 remember 一个）
    pub fn interaction_source(mut self, source: crate::ui::interaction::MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// 错误状态（对标 Compose isError——错误时文字/边框用 error 色）
    pub fn is_error(mut self, is_error: bool) -> Self {
        self.is_error = is_error;
        self
    }

    /// M3 Filled 容器（填充 + 底部指示线——对齐 material3 TextField 默认）
    pub fn filled(mut self) -> Self {
        self.variant = Some(TextFieldVariant::Filled);
        self
    }

    /// M3 Outlined 容器（边框——对齐 material3 OutlinedTextField）
    pub fn outlined(mut self) -> Self {
        self.variant = Some(TextFieldVariant::Outlined);
        self
    }

    /// 状态色（None = 按 variant 从主题生成 M3 默认色）
    pub fn colors(mut self, colors: TextFieldColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 浮动 label（聚焦或非空时悬浮到容器顶部，12sp；空且未聚焦时
    /// 展开占据输入位，16sp——对齐 M3 label 两态）
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// 支持文本（容器底部外侧 12sp——对齐 M3 supportingText；错误时 error 色）
    pub fn supporting_text(mut self, text: impl Into<String>) -> Self {
        self.supporting_text = Some(text.into());
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
        // 外观状态（M3 容器）——focused 从交互源组合期读取
        let visual = self.variant;
        let colors = self.colors.clone().unwrap_or_else(|| {
            match visual.unwrap_or(TextFieldVariant::Filled) {
                TextFieldVariant::Filled => TextFieldColors::filled_from_theme(&theme),
                TextFieldVariant::Outlined => TextFieldColors::outlined_from_theme(&theme),
            }
        });
        // focused：enabled 且有交互源时读（禁用态无交互源——直接 false）
        // 交互源提前解析（聚焦状态读取 + focusable 共用——避免 build 开头
        // 读到 None 而 focused 恒 false：外部注入或内部 remember）
        let interaction = if self.enabled {
            Some(self.interaction_source.clone()
                .unwrap_or_else(|| ctx.remember(|| crate::ui::interaction::MutableInteractionSource::new()).get()))
        } else {
            None
        };
        let focused = interaction.as_ref().map(|s| s.is_focused()).unwrap_or(false);
        // 悬浮 label：聚焦或文本非空（对齐 M3：Focused / UnfocusedNotEmpty）
        let label_float = self.label.is_some() && (focused || !content.is_empty());
        // 状态优先级 disabled > error > focused > unfocused
        let disabled = !self.enabled;
        let color = if disabled {
            colors.disabled_text
        } else if self.is_error {
            colors.text
        } else {
            colors.text
        };

        // 占位/展开 label（对齐 M3 TextFieldImpl placeholderAlpha 语义）：
        // - 聚焦 + 空：placeholder 显示（label 悬浮）
        // - 未聚焦 + 空 + 有 label：placeholder 隐藏（展开 label 占据输入位）
        // - 空 + 无 label：placeholder 显示
        // - 非空：隐藏
        let show_placeholder = content.is_empty()
            && self.placeholder.is_some()
            && (focused || self.label.is_none());
        let display_content = if show_placeholder {
            self.placeholder.as_deref().unwrap_or("").to_string()
        } else {
            content
        };
        let display_color = if show_placeholder {
            if disabled { colors.disabled_placeholder } else { colors.placeholder }
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
        // UndoManager（组合点 remember——跨帧持久，键位处理共享）
        let undo = ctx.remember(|| std::sync::Arc::new(parking_lot::Mutex::new(UndoManager::new()))).get();
        let kb_handler = {
            let v = value.clone();
            let cb = on_change.clone();
            let undo = undo.clone();
            let read_only = self.read_only;
            let single_line = self.single_line;
            move |e: &crate::modifier::KbEvent| -> bool {
                if e.event_type != crate::modifier::KbEventType::KeyDown { return false; }
                let key = &e.key;
                // 桌面修饰键：Ctrl（macOS 用 Cmd——Compose commonKeyMapping 同款）
                let ctrl = e.is_ctrl_pressed || e.is_meta_pressed;
                let shift = e.is_shift_pressed;
                let alt = e.is_alt_pressed;
                // 导航键（只读时仍允许——对标 Compose readOnly 可选中；
                // Tab 必须放行——否则键盘焦点无法移出）
                let is_nav = matches!(key,
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowLeft)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowRight)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowUp)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowDown)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::Home)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::End)
                    | winit::keyboard::Key::Named(winit::keyboard::NamedKey::Tab)
                );
                // 剪贴板/全选/撤销重做（editsText=false 命令——read_only 也允许）
                // Ctrl+A 全选
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("a")) {
                    let mut val = v.get();
                    undo.lock().push(&val.text, &val.selection);
                    val.selection = 0..val.text.len();
                    v.set(val);
                    return true;
                }
                // Ctrl+C 复制（read_only 可复制——Compose COPY editsText=false）
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                    let val = v.get();
                    let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                    if s != e {
                        clipboard_set_text(&val.text[s..e]);
                    }
                    return true;
                }
                // 只读：编辑类键（含 Ctrl+Z/V/X/词删除）直接消耗；导航保留
                if read_only && !is_nav {
                    return true;
                }
                // Ctrl+Z 撤销 / Ctrl+Shift+Z、Ctrl+Y 重做
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("z")) {
                    let mut val = v.get();
                    let cur = (val.text.clone(), val.selection.clone());
                    let restored = if shift || e.is_meta_pressed {
                        undo.lock().redo(cur)
                    } else {
                        undo.lock().undo(cur)
                    };
                    if let Some((text, sel)) = restored {
                        val.text = text;
                        val.selection = sel;
                        v.set(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                    }
                    return true;
                }
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("y")) {
                    let mut val = v.get();
                    let cur = (val.text.clone(), val.selection.clone());
                    if let Some((text, sel)) = undo.lock().redo(cur) {
                        val.text = text;
                        val.selection = sel;
                        v.set(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                    }
                    return true;
                }
                // Ctrl+X 剪切
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                    let mut val = v.get();
                    undo.lock().push(&val.text, &val.selection);
                    let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                    if s != e {
                        clipboard_set_text(&val.text[s..e]);
                        val.text.replace_range(s..e, "");
                        val.selection = s..s;
                        v.set(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                    }
                    return true;
                }
                // Ctrl+V 粘贴（单行：换行符替换为空格——Compose 单行语义）
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                    let Some(clip) = clipboard_get_text() else { return true; };
                    let clip = if single_line { clip.replace(['\n', '\r'], " ") } else { clip };
                    let mut val = v.get();
                    undo.lock().push(&val.text, &val.selection);
                    let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                    val.text.replace_range(s..e, &clip);
                    let caret = s + clip.len();
                    val.selection = caret..caret;
                    v.set(val.clone());
                    if let Ok(cb) = cb.lock() { cb(val); }
                    return true;
                }
                let mut val = v.get();
                // 每次键处理前快照（对齐 Compose forceNextSnapshot：
                // 编辑与移动都会记录——同文本合并 selection）
                undo.lock().push(&val.text, &val.selection);
                // 词级移动（Ctrl+←/→；Shift 扩展选区）
                match key {
                    winit::keyboard::Key::Named(named) if ctrl => match named {
                        winit::keyboard::NamedKey::ArrowLeft => {
                            let target = word_prev(&val.text, val.selection.start);
                            if shift {
                                val.selection = val.selection.start.min(target)..val.selection.end.max(target);
                            } else {
                                val.selection = target..target;
                            }
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            let target = word_next(&val.text, val.selection.start);
                            if shift {
                                val.selection = val.selection.start.min(target)..val.selection.end.max(target);
                            } else {
                                val.selection = target..target;
                            }
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::Backspace => {
                            if val.selection.start != val.selection.end {
                                let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                                val.text.replace_range(s..e, "");
                                val.selection = s..s;
                            } else {
                                let prev = word_prev(&val.text, val.selection.start);
                                if prev < val.selection.start {
                                    val.text.replace_range(prev..val.selection.start, "");
                                    val.selection = prev..prev;
                                }
                            }
                            v.set(val.clone());
                            if let Ok(cb) = cb.lock() { cb(val); }
                            return true;
                        }
                        winit::keyboard::NamedKey::Delete => {
                            if val.selection.start != val.selection.end {
                                let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                                val.text.replace_range(s..e, "");
                                val.selection = s..s;
                            } else {
                                let next = word_next(&val.text, val.selection.start);
                                if next > val.selection.start {
                                    val.text.replace_range(val.selection.start..next, "");
                                    val.selection = val.selection.start..val.selection.start;
                                }
                            }
                            v.set(val.clone());
                            if let Ok(cb) = cb.lock() { cb(val); }
                            return true;
                        }
                        winit::keyboard::NamedKey::Home => {
                            // Ctrl+Home：文首（对齐 PREV_PARAGRAPH 近似）
                            val.selection = 0..0;
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::End => {
                            val.selection = val.text.len()..val.text.len();
                            v.set(val);
                            return true;
                        }
                        _ => {}
                    },
                    _ => {}
                }
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
                                if let Ok(cb) = cb.lock() { cb(val); }
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
                                if let Ok(cb) = cb.lock() { cb(val); }
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
                                if let Ok(cb) = cb.lock() { cb(val); }
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
                                if let Ok(cb) = cb.lock() { cb(val); }
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Enter => {
                            // 单行模式：Enter 吞掉不换行（对标 Compose singleLine）
                            if single_line {
                                return true;
                            }
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
                                v.set(val);
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
                                    v.set(val);
                                }
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Home => {
                            // 行首（对齐 Compose LINE_START——无修饰 Home）
                            let target = line_start(&val.text, val.selection.start);
                            if shift {
                                val.selection = val.selection.start.min(target)..val.selection.end.max(target);
                            } else {
                                val.selection = target..target;
                            }
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::End => {
                            // 行尾（对齐 Compose LINE_END）
                            let target = line_end(&val.text, val.selection.start);
                            if shift {
                                val.selection = val.selection.start.min(target)..val.selection.end.max(target);
                            } else {
                                val.selection = target..target;
                            }
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowUp => {
                            // 单行：放行给框架的方向键焦点导航（对标 Compose
                            // 单行输入框 Up/Down 不移动光标，焦点可移出）
                            if single_line { return false; }
                            // 无布局信息时的近似：移到上一行行首（\n 分隔）
                            let target = caret_prev_line(&val.text, val.selection.start);
                            if shift {
                                val.selection = target..val.selection.end.max(target);
                            } else {
                                val.selection = target..target;
                            }
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowDown => {
                            if single_line { return false; }
                            // 近似：移到下一行行首；已是最后一行则保持原位
                            let target = caret_next_line(&val.text, val.selection.start);
                            if shift {
                                val.selection = val.selection.start.min(target)..target;
                            } else {
                                val.selection = target..target;
                            }
                            v.set(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::Tab => {
                            // 放行给框架焦点导航（对齐 Compose 单行 TAB 不消费）
                            return false;
                        }
                        _ => {}
                    },
                    winit::keyboard::Key::Character(c) => {
                        // Ctrl/Alt 组合键不插入字符（未映射命令放行——Compose 语义）
                        if ctrl || alt { return false; }
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

        // M3 容器视觉 + 文本内容。padding 按变体（M3 specs）：
        // - Filled + label：顶 24（悬浮 label 区 8..24）+ 底 8（Filled 上下
        //   padding 8dp 规格，文本从 label 下开始）
        // - Outlined + label：四边 16（label 跨边框不占容器内空间——
        //   Outlined 无上下 padding 规格，文本垂直居中）
        // - 无 label：四边 16（文本垂直居中近似）
        // - 无容器视觉：保持 8（向后兼容）
        let modifier = self.modifier;
        let modifier = if let Some(variant) = visual {
            let (pad_h, pad_top, pad_bottom) = match (variant, self.label.is_some()) {
                (TextFieldVariant::Filled, true) => (16.0, 24.0, 8.0),
                _ => (16.0, 16.0, 16.0),
            };
            let shape = crate::modifier::Shape::RoundedRect {
                corner_radius: 4.0,
            };
            // label 悬浮动画（0 = 展开（输入位 16sp），1 = 悬浮（顶部 12sp）——
            // M3 FastSpatial 150ms 近似；渲染端按 progress 插值位置/字号）
            let label_progress = self.label.as_ref().map(|_| {
                ctx.animate_float_as_state(
                    if label_float { 1.0 } else { 0.0 },
                    crate::animation::AnimationSpec::Tween(
                        crate::animation::TweenSpec::new(
                            std::time::Duration::from_millis(150),
                            // M3 FastSpatial 强调加速曲线近似
                            crate::animation::interpolator::EaseInOutCubic::new(),
                        )
                    ),
                )
            });
            // label 绘制参数：悬浮 12sp（顶部）/ 展开 16sp（输入位）
            let label_visual = match (self.label.as_ref(), &label_progress) {
                (Some(l), Some(progress)) => Some(crate::modifier::LabelVisual {
                    content: l.clone(),
                    font_size: if label_float { 12.0 } else { 16.0 },
                    color: colors.label_color(self.enabled, self.is_error, focused),
                    progress: progress.clone(),
                }),
                _ => None,
            };
            // 支持文本：容器底部外侧 12sp（M3 supporting 色）
            let supporting_visual = self.supporting_text.as_ref().map(|s| crate::modifier::SupportingVisual {
                content: s.clone(),
                font_size: 12.0,
                color: if disabled { colors.disabled_supporting }
                    else if self.is_error { colors.error_supporting }
                    else { colors.supporting },
            });
            modifier
                .padding_horizontal(pad_h)
                .padding_top(pad_top)
                .padding_bottom(pad_bottom)
                .min_width(280.0)
                .min_height(56.0)
                .text_field_visual(
                    variant,
                    shape,
                    colors.clone(),
                    self.enabled,
                    focused,
                    self.is_error,
                    if self.is_error { colors.error_cursor } else { colors.cursor },
                    label_visual,
                    supporting_visual,
                )
        } else {
            modifier.padding(8.0)
        };
        let modifier = modifier
            .text_content(
                display_content,
                font_size,
                display_color,
                crate::ui::text::FontWeight::NORMAL,
                crate::ui::text::FontSlant::Upright,
                if self.single_line { 1 } else { self.max_lines }, // maxLines（singleLine → 1）
                crate::ui::TextAlign::Left,
                crate::ui::TextOverflow::Clip,
                true, // allow text wrapping
            );
        // 支持文本：容器下方 +4dp 间距 + 12sp 行高（约 20px）——
        // 动态高度保证下方元素不重叠；基础高度 = max(容器最小 56, 内容) + 20
        let modifier = if self.supporting_text.is_some() {
            let v = value.clone();
            let line_h = font_size * 1.4;
            let (pt, pb) = modifier.get_padding_vertical();
            let pad_y = pt + pb;
            let supporting_h = 4.0 + 16.0;
            let min_h = 56.0 + supporting_h;
            modifier.height(move || {
                let text = v.get().text;
                let base = if text.is_empty() {
                    min_h
                } else {
                    ((text.matches('\n').count() as f32 + 1.0) * line_h + pad_y).max(56.0) + supporting_h
                };
                base
            })
        } else {
            modifier
        };
        // minLines：高度至少 min_lines 行——动态高度闭包（内容变化时重测）。
        // ⚠ 非空时不能返回 0（0 是合法固定尺寸 → tighten_height(0) → 节点高度 0
        // → 输入后整个 TextField 消失）。按显式换行数 × 行高近似——折行
        // （无 \n 的长文本自动换行）高度不精确，会裁剪——精确需容器 policy。
        // ⚠ 必须加 padding：measure_node 把 padding 从约束中扣除（内尺寸），
        // 动态高度返回的是外尺寸——不加 pad 时内高 = 外高 - pad，3 行文本
        // （~50.4）超出内高（42.8）→ 末行溢出与下一元素重叠（实测 bug）
        let modifier = if self.min_lines > 1 {
            let v = value.clone();
            let line_h = font_size * 1.4;
            let (pt, pb) = modifier.get_padding_vertical();
            let pad_y = pt + pb;
            let min_h = self.min_lines as f32 * line_h + pad_y;
            modifier.height(move || {
                let text = v.get().text;
                if text.is_empty() {
                    min_h
                } else {
                    (text.matches('\n').count() as f32 + 1.0) * line_h + pad_y
                }
            })
        } else {
            modifier
        };
        // 禁用：不聚焦不响应键盘（Compose disabled 语义）；否则聚焦 + 键盘
        let modifier = if let Some(interaction) = interaction {
            // 点击聚焦由组件自己请求（对标 Compose BasicTextField：点击 requestFocus；
            // 框架层 clickable/focusable 点击不自动聚焦——Button 等组件点击不抢焦点）
            let fr = ctx.remember(|| crate::modifier::FocusRequester::new()).get();
            let fr_click = fr.clone();
            modifier
                .focusable_with_source(&interaction)
                .focus_requester(&fr)
                .on_press(move |_| { fr_click.request_focus(); })
                .on_key_event(kb_handler)
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
        // 焦点环颜色：主题 primary（组合期捕获——渲染期 CompositionLocal 已退出）
        ctx.set_current_node_focus_color(crate::ui::theme::WiniaTheme::colors().primary);
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

/// Up 键光标目标：上一行行首（`\n` 分隔的近似——无布局信息时使用；
/// 首行返回 0）。按字节定位 `\n`（ASCII）——pos 非字符边界也不会 panic。
fn caret_prev_line(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let pos = pos.min(bytes.len());
    bytes[..pos]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// Down 键光标目标：下一行行首；已是最后一行则保持原位。
/// 按字节定位 `\n`（ASCII）——pos 非字符边界也不会 panic。
fn caret_next_line(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let pos = pos.min(bytes.len());
    bytes[pos..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|i| pos + i + 1)
        .unwrap_or(pos)
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
        modifier.elements().iter().any(|el| matches!(el, ModifierElement::Focusable { .. }))
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

    #[test]
    fn min_lines_height_grows_with_content() {
        // 回归：min_lines 动态高度非空时不能返回 0（节点消失 bug）
        let value = State::new(TextFieldValue::new(""));
        let mut composer = Composer::new();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        composer.compose(|ctx| {
            TextField::new(value.clone(), |_| {})
                .min_lines(3)
                .modifier(Modifier::new().width(300.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let h0 = composer.arena_nodes()[root].measured_size.height;
        // min_h 58.8 被 build 内 padding(8) 扣减 → 内容区 42.8（近似 3 行）
        assert!(h0 > 30.0, "空内容高度 = min_lines 占位（实际 {h0}）");

        // 输入 2 行（显式换行）→ 高度按行数增长（非 0——修复前输入后消失）
        value.set(TextFieldValue::new("a\nb"));
        // 重新组合（消费 pending + 同 key 复用节点 → dirty → 重测）
        composer.compose(|ctx| {
            TextField::new(value.clone(), |_| {})
                .min_lines(3)
                .modifier(Modifier::new().width(300.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let h1 = composer.arena_nodes()[root].measured_size.height;
        assert!(h1 > 0.0, "输入后高度必须 > 0（修复前为 0——TextField 消失）");
        assert!(h1 < h0, "2 行高度 < 3 行占位（{h1} < {h0}）");
    }

    #[test]
    fn caret_up_down_line_targets() {
        let text = "ab\ncd\nef";
        // 首行 → 0；第二行中部 → 上一行行首 3；第三行中部 → 上一行行首 6
        assert_eq!(caret_prev_line(text, 1), 0);
        assert_eq!(caret_prev_line(text, 4), 3);
        assert_eq!(caret_prev_line(text, 7), 6);
        // 恰在行首 → 保持（上一行行首即自身）
        assert_eq!(caret_prev_line(text, 3), 3);
        // 第一行 → 下一行行首；最后一行 → 末尾；末尾后不越界
        assert_eq!(caret_next_line(text, 1), 3);
        assert_eq!(caret_next_line(text, 4), 6);
        assert_eq!(caret_next_line(text, 6), 6, "末行 Down 保持原位");
        assert_eq!(caret_next_line(text, text.len() + 10), text.len(), "越界钳制到末尾");
        // 非字符边界 pos 不 panic（按字节查找 \n）
        let cjk = "a\n中b";
        assert_eq!(caret_prev_line(cjk, 3), 2, "‘中’字内部 pos 回退到上一行行首");
        assert_eq!(caret_next_line(cjk, 3), 3, "‘中’字内部 pos 无下一行则保持");
    }

    // ── 词边界（Ctrl+←/→、Ctrl+Backspace/Delete）──

    #[test]
    fn word_boundaries_prev_next() {
        // 词中间 → 词首；词首 → 前一词首；文首 → 0
        assert_eq!(word_prev("hello world", 7), 6, "world 中间 → world 词首");
        assert_eq!(word_prev("hello world", 6), 0, "world 词首 → hello 词首");
        assert_eq!(word_prev("hello world", 0), 0, "文首保持");
        // 下一个词首；末尾 → len
        assert_eq!(word_next("hello world", 3), 6, "hello 中间 → world 词首");
        assert_eq!(word_next("hello world", 6), 11, "world 词首 → 文末");
        assert_eq!(word_next("hello world", 11), 11, "文末保持");
        // 空白中间 → 就近词边界
        assert_eq!(word_prev("hello world", 5), 0, "空白中间 → 前一词首");
        // 标点分词（Compose wordChar 语义——标点不是词字符，是边界）
        assert_eq!(word_next("a.b", 1), 2, "点后 'b' 是下一个词");
        assert_eq!(word_prev("a.b", 2), 0, "b 前 → a 词首");
        // 中文连续汉字 = 一个词段（letterOrDigit 语义——与 Compose 一致）
        assert_eq!(word_next("你好世界", 0), 12, "4 汉字 × 3 字节 = 12（整体一词）");
        assert_eq!(word_prev("你好世界", 8), 0);
        // pos 落在多字节字符内部——按字符起算（不 panic）
        let cjk = "ab中cd";
        assert_eq!(word_next(cjk, 4), 7, "‘中’字内部 → 跳过中字后（cd 之后）");
        assert_eq!(word_prev(cjk, 4), 0, "‘中’字内部 → 前一词 'ab' 的词首 0");
        // 纯标点文本
        assert_eq!(word_next("!!!", 0), 3, "纯标点 → 文末");
        assert_eq!(word_prev("!!!", 3), 0);
    }

    #[test]
    fn line_start_end_targets() {
        // 行首：pos 之前最后一个 \n 之后
        assert_eq!(line_start("ab\ncd", 3), 3);
        assert_eq!(line_start("ab\ncd", 4), 3, "第二行中部 → 行首 3");
        assert_eq!(line_start("ab\ncd", 0), 0);
        // 行尾：不含换行符（对齐 Compose LINE_END——光标停在 \n 前）
        assert_eq!(line_end("ab\ncd", 0), 2);
        assert_eq!(line_end("ab\ncd", 2), 2, "恰在行尾 → 保持");
        assert_eq!(line_end("ab\ncd", 3), 5, "第二行中部 → 文尾 5（末行无 \n）");
        assert_eq!(line_end("ab", 0), 2, "无换行 → 文尾");
        // 非字符边界 pos 不 panic（按字节查找 \n）
        let cjk = "a\n中b";
        assert_eq!(line_end(cjk, 3), 6, "‘中’字内部 → 第二行行尾 6");
        assert_eq!(line_start(cjk, 4), 2, "‘中’字内部 → 第二行行首 2");
    }

    // ── UndoManager（Ctrl+Z / Ctrl+Shift+Z）──

    #[test]
    fn undo_redo_basic_flow() {
        let mut um = UndoManager::new();
        // 每次编辑前 push 当前状态（与 kb_handler 调用方式一致）
        um.push("", &(0..0));   // 输入 'h' 前
        um.push("h", &(1..1));  // 输入 'e' 前
        um.push("he", &(2..2)); // 输入 'x' 前
        // 当前状态 "hex"
        let (t1, s1) = um.undo(("hex".into(), 3..3)).expect("undo1");
        assert_eq!((t1.as_str(), s1.start), ("he", 2));
        let (t2, s2) = um.undo((t1.clone(), s1.clone())).expect("undo2");
        assert_eq!((t2.as_str(), s2.start), ("h", 1));
        // redo 精确回到 undo 前的状态（当前状态入 redo 栈）
        let (t3, _) = um.redo((t2.clone(), s2.clone())).expect("redo1");
        assert_eq!(t3, "he");
        let (t4, _) = um.redo((t3.clone(), s1)).expect("redo2");
        assert_eq!(t4, "hex", "redo 回到当前状态（含未入栈的最新编辑）");
        assert!(um.redo(("hex".into(), 3..3)).is_none(), "已回最新，无 redo");
        // 栈底无 undo
        let mut um2 = UndoManager::new();
        assert!(um2.undo(("x".into(), 1..1)).is_none(), "无快照时 undo 为 None");
    }

    #[test]
    fn undo_merges_same_text_selection() {
        // 对齐 Compose：同文本只更新 selection（光标移动不产生新条目）
        let mut um = UndoManager::new();
        um.push("abc", &(1..1));
        um.push("abc", &(2..2)); // 同文本 → 合并
        um.push("abc", &(3..3));
        assert_eq!(um.undo_stack.len(), 1, "三次同文本快照合并为一条");
        assert_eq!(um.undo_stack[0].1, 3..3, "selection 取最新");
        // 文本变化产生新条目；undo 回退文本与 selection
        um.push("abc", &(3..3)); // 输入 'd' 前（与 base 同文本 → 合并）
        let (t, s) = um.undo(("abcd".into(), 4..4)).unwrap();
        assert_eq!((t.as_str(), s.clone()), ("abc", 3..3));
        // 快照耗尽（同文本合并只有一条）
        assert!(um.undo((t, s)).is_none(), "合并后仅一条快照");
    }

    #[test]
    fn undo_truncates_redo_branch_on_new_edit() {
        let mut um = UndoManager::new();
        um.push("a", &(1..1));
        um.push("ab", &(2..2));
        let _ = um.undo(("ab".into(), 2..2)); // 回到 "a"
        // 新编辑（undo 后输入 'c' → 当前 "a"）→ redo 分支丢弃
        um.push("ac", &(2..2));
        assert!(um.redo_stack.is_empty(), "undo 后编辑丢弃 redo 分支");
        assert_eq!(um.undo_stack.len(), 2);
    }
}
