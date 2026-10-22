//! TextField — 文本输入组件（对齐 Jetpack Compose BasicTextField）
//!
//! 参考：旧版 winia v1 的 text_field 实现 + Compose Foundation 1.11.4 源码
//! （KeyCommand/KeyMapping/UndoManager 语义）

use crate::text::field::{
    TextFieldColors, TextFieldSlotRole, TextFieldVariant,
};
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use crate::modifier::Modifier;
use crate::composable;
use crate::components::text::{ProvideTextStyle};
use crate::text::TextStyle;
use crate::theme::WiniaTheme;
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
                // 有选区：先删选区再插入（替换语义——旧版 v1 的
                // Ime::Commit 先 Deleted 再 Inserted；选中文本输入
                // 应替换选区而非保留）
                if value.selection.start != value.selection.end {
                    let (s, e) = (
                        value.selection.start.min(value.selection.end),
                        value.selection.start.max(value.selection.end),
                    );
                    // 防御 clamp（选区可能越界——IME 删 composing 后）
                    let len = value.text.len();
                    let (s, e) = (s.min(len), e.min(len));
                    if s < e {
                        value.text.replace_range(s..e, "");
                        value.selection = s..s;
                    } else {
                        value.selection = s.min(len)..s.min(len);
                    }
                }
                // 插入位置 = 删选区后的光标（传参 index 是原 selection.start——
                // 反向选区时 > 删后长度，须以删后光标为准）
                let pos = value.selection.start.min(*index);
                value.text.insert_str(pos, text);
                value.selection = (pos + text.len())..(pos + text.len());
            }
            TextChange::Deleted { range } => {
                // 防御 clamp（§5.4）：stale selection/IME/registrar range 可能越界
                // 或落在多字节字符中间——drain 前对齐 char 边界 + 限制在文本长度内，
                // 否则字符串边界 panic
                let len = value.text.len();
                let s = value.text.floor_char_boundary(range.start.min(len));
                let e = value.text.floor_char_boundary(range.end.min(len).max(s));
                if s < e {
                    value.text.drain(s..e);
                }
                value.selection = s..s;
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
    undo_stack: Vec<(String, Range<usize>, Option<Range<usize>>)>,
    redo_stack: Vec<(String, Range<usize>, Option<Range<usize>>)>,
}

const UNDO_MAX_SNAPSHOTS: usize = 100;

impl UndoManager {
    fn new() -> Self {
        Self::default()
    }

    /// 编辑前调用：记录 (text, selection, composing_range) 快照；新编辑丢弃 redo 分支
    fn push(&mut self, text: &str, selection: &Range<usize>, composing: Option<Range<usize>>) {
        if let Some(last) = self.undo_stack.last_mut() {
            if last.0 == text {
                // 同文本：只更新 selection 与 composing（光标移动/组合态变化不产生新条目）
                last.1 = selection.clone();
                last.2 = composing;
                return;
            }
        }
        self.undo_stack.push((text.to_string(), selection.clone(), composing));
        self.redo_stack.clear();
        if self.undo_stack.len() > UNDO_MAX_SNAPSHOTS {
            self.undo_stack.remove(0);
        }
    }

    /// 撤销：当前状态入 redo 栈，返回上一快照
    fn undo(&mut self, current: (String, Range<usize>, Option<Range<usize>>)) -> Option<(String, Range<usize>, Option<Range<usize>>)> {
        let s = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(s)
    }

    /// 重做：当前状态入 undo 栈，返回 redo 快照
    fn redo(&mut self, current: (String, Range<usize>, Option<Range<usize>>)) -> Option<(String, Range<usize>, Option<Range<usize>>)> {
        let s = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(s)
    }
}

/// 结束组合（删除组合文本并收拢 selection——对齐 Compose
/// `FinishComposingTextCommand`：键盘编辑前先移除组合文本，否则
/// 拼音 Commit 时 preedit 文本残留在正文中）。clamp 防删组合后越界。
fn end_composition(val: &mut TextFieldValue) {
    if let Some(comp) = val.composing_range.clone() {
        let len = comp.len();
        val.text.replace_range(comp.clone(), "");
        let shift = len.min(val.selection.start.saturating_sub(comp.start));
        val.selection = (val.selection.start - shift)..(val.selection.end - shift.min(val.selection.end));
        val.composing_range = None;
        let text_len = val.text.len();
        val.selection = val.selection.start.min(text_len)..val.selection.end.min(text_len);
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


/// M3 默认禁用降级：基色 × alpha（disabled 色 = onSurface @ alpha）
fn alpha(c: crate::graphics::Color, a: f32) -> crate::graphics::Color {
    crate::graphics::Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b)
}

impl TextFieldColors {
    /// M3 FilledTextField 默认色（defaultTextFieldColors——1.4.0 tokens）：
    /// text onSurface、container surfaceContainerHighest、indicator/label 按状态、
    /// disabled 全系 onSurface@38%、cursor primary（error → error）
    pub fn filled_from_theme(theme: &crate::theme::ThemeColors) -> Self {
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
            leading_icon_focused: theme.on_surface_variant,
            leading_icon_disabled: alpha(theme.on_surface, 0.38),
            trailing_icon_focused: theme.on_surface_variant,
            trailing_icon_disabled: alpha(theme.on_surface, 0.38),
            trailing_icon_error: theme.error,
            affix: theme.on_surface_variant,
            disabled_affix: alpha(theme.on_surface, 0.38),
        }
    }

    /// M3 OutlinedTextField 默认色（defaultOutlinedTextFieldColors）：
    /// 容器透明、边框 unfocused outline / disabled onSurface@12%、其余同 Filled
    pub fn outlined_from_theme(theme: &crate::theme::ThemeColors) -> Self {
        let mut c = Self::filled_from_theme(theme);
        c.container = crate::graphics::Color::TRANSPARENT;
        c.indicator_unfocused = theme.outline;
        c.indicator_disabled = alpha(theme.on_surface, 0.12);
        c
    }

    /// 状态解析（优先级 disabled > error > focused > unfocused——M3 同款）
    pub fn indicator_color(&self, enabled: bool, is_error: bool, focused: bool) -> crate::graphics::Color {
        if !enabled { self.indicator_disabled }
        else if is_error { self.indicator_error }
        else if focused { self.indicator_focused }
        else { self.indicator_unfocused }
    }

    pub fn label_color(&self, enabled: bool, is_error: bool, focused: bool) -> crate::graphics::Color {
        if !enabled { self.label_disabled }
        else if is_error { self.label_error }
        else if focused { self.label_focused }
        else { self.label_unfocused }
    }
}

// ═══════════════════════════════════════════════════════════
// TextField — composable widget
// ═══════════════════════════════════════════════════════════


fn text_style_line_height(style: &TextStyle, font_size: f32) -> f32 {
    style.line_height
        .map(|height| height.to_logical_px(crate::runtime::density::current_density()))
        .unwrap_or(font_size * 1.4)
}

fn interpolate_text_style(from: &TextStyle, to: &TextStyle, progress: f32, color: crate::graphics::Color) -> TextStyle {
    let progress = progress.clamp(0.0, 1.0);
    let from_size = from.font_size.unwrap_or(16.0.into()).to_logical_px(crate::runtime::density::current_density());
    let to_size = to.font_size.unwrap_or(12.0.into()).to_logical_px(crate::runtime::density::current_density());
    let font_size = from_size + (to_size - from_size) * progress;
    let from_line_height = text_style_line_height(from, from_size);
    let to_line_height = text_style_line_height(to, to_size);
    let line_height = from_line_height + (to_line_height - from_line_height) * progress;
    let from_spacing = from.letter_spacing.unwrap_or(0.0);
    let to_spacing = to.letter_spacing.unwrap_or(0.0);

    TextStyle::new()
        .font_size(font_size)
        .line_height(line_height)
        .letter_spacing(from_spacing + (to_spacing - from_spacing) * progress)
        .font_weight(if progress < 0.5 {
            from.font_weight.unwrap_or_default()
        } else {
            to.font_weight.unwrap_or_default()
        })
        .font_style(if progress < 0.5 {
            from.font_style.unwrap_or_default()
        } else {
            to.font_style.unwrap_or_default()
        })
        .color(color)
}

/// TextField 容器布局 policy——子节点按角色定位（M3 specs）：
/// leading(12dp 左，垂直居中) | label(悬浮顶部/展开输入位，progress 插值)
/// | placeholder/prefix(输入前) | input(剩余宽) | suffix(输入后)
/// | trailing(右 12dp 居中)。图标与文本间距 16dp。
#[derive(Debug)]
pub(crate) struct TextFieldLayout {
    /// label 悬浮动画进度（0 = 展开 / 1 = 悬浮）——measure 期 peek 注册
    /// layout_dep，位置随动画每帧重测
    pub(crate) label_progress: crate::runtime::state::State<f32>,
    /// 容器视觉变体（决定悬浮 label 的锚点——M3 specs）：
    /// - Filled：label 顶在容器 8dp（悬浮区 8..24，中心 16）
    /// - Outlined：label 中心跨边框线（容器顶 0）
    pub(crate) variant: Option<TextFieldVariant>,
    /// 容器 padding（M3 specs：图标垂直居中于**容器**（56dp）——内容区
    /// 顶部即 pad_top——图标锚点 = 容器中心）
    pub(crate) pad_top: f32,
    pub(crate) pad_bottom: f32,
    /// supporting 区高（4 + body_small line height——容器 min_height 含此，居中推导须扣除）
    pub(crate) supporting_h: f32,
    /// 输入文本行高（px）——空文本输入子节点最小尺寸用。
    pub(crate) line_height: f32,
}

/// 内容区高度推导（label/图标容器居中锚点用）：
/// - 约束高有限（非滚动容器）→ 直接用
/// - 滚动容器内约束高 = f32::MAX → 用约束 min 高（modifier 层
///   min_height 经 padding offset 后 = **内容区 + supporting** min 高——
///   ⚠ 含 supporting（4+16=20），须扣除；空字段输入节点测量高 0
///   （空文本 paragraph），降级 input_size.height 会算负中心
/// - 多行：input_size.height 超过 min_height 兜底（24）时取 input 高——
///   否则 label/图标居中锚点恒为单行高，容器长高后图标/展开 label 不
///   跟随中心下移
/// - 兜底输入区高
/// The content height the field's slots centre in: the field's OWN box, not the room its parent happens to
/// have.
///
/// It used to return `constraints.max_height` whenever that was finite, which centres a slot in the SPACE
/// ABOVE the field instead of in the field — a trailing icon landed 88px into a 56px-tall field (measured:
/// slot `(260, 391, 13, 19)` against field `(16, 303, 280, 56)`), and only looked right where the parent
/// handed out an unbounded height (a scrolling column, which is every demo).
///
/// `constrain_height` honours a tight parent as well, so a fixed-height field (a 40dp pill) still centres
/// its slots in 40.
/// The colour spec for the field's focus transition: M3's `FastEffects` spring (1.0 / 3800), which is
/// what `TextFieldDefaults` passes to `animateColorAsState` (`material3/TextFieldDefaults.kt:246-248`
/// for the filled style, `:1068-1070` for the outlined one) instead of a 100 ms tween.
///
/// Recorded deviation: Compose animates `colors.containerColor(enabled, isError, focused)` there —
/// the CONTAINER. winia has no container-colour transition in this component and animates the
/// indicator/border colour instead, so the spring matches while the animated object does not
/// (`indicator_color` is not animated by M3). Named so the token is assertable.
fn text_field_colour_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::effects_fast()
}

fn text_field_content_height(constraints: &crate::layout::Constraints, supporting_h: f32, input_height: f32) -> f32 {
    constraints.constrain_height(input_height + supporting_h)
}

impl TextFieldLayout {
    pub(crate) fn new(
        label_progress: crate::runtime::state::State<f32>,
        variant: Option<TextFieldVariant>,
        pad_top: f32,
        pad_bottom: f32,
        supporting_h: f32,
        line_height: f32,
    ) -> Self {
        Self { label_progress, variant, pad_top, pad_bottom, supporting_h, line_height }
    }
}

impl crate::layout::MeasurePolicy for TextFieldLayout {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::Placement>) {
        use crate::layout::node::measure_node;
        use crate::modifier::ModifierElement;
        let role_of = |idx: usize| -> TextFieldSlotRole {
            nodes[idx].modifier.elements().iter().find_map(|el| {
                if let ModifierElement::TextFieldSlot { role } = el { Some(*role) } else { None }
            }).unwrap_or(TextFieldSlotRole::Input)
        };
        let roles: Vec<TextFieldSlotRole> = children.iter().map(|&c| role_of(c)).collect();
        let icon_c = crate::layout::Constraints::fixed(24.0, 24.0);
        let text_c = |max_w: f32| crate::layout::Constraints::new(0.0, max_w.max(0.0), 0.0, 30.0);
        // 第一轮：leading/trailing（图标）+ prefix/suffix（文本）——宽度
        // 决定 input 剩余空间
        let (mut leading_w, mut trailing_w) = (0.0f32, 0.0f32);
        let (mut prefix_w, mut suffix_w) = (0.0f32, 0.0f32);
        let mut placements: Vec<crate::layout::Placement> = Vec::new();
        for (i, &c) in children.iter().enumerate() {
            match roles[i] {
                TextFieldSlotRole::Leading => {
                    let (s, _) = measure_node(nodes, policies, c, icon_c);
                    leading_w = s.width;
                    // 贴内容区左端（容器左 padding 12 提供 M3 图标距边 12dp）
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                TextFieldSlotRole::Trailing => {
                    let (s, _) = measure_node(nodes, policies, c, icon_c);
                    trailing_w = s.width;
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                TextFieldSlotRole::Prefix => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    prefix_w = s.width;
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                TextFieldSlotRole::Suffix => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    suffix_w = s.width;
                    placements.push(crate::layout::Placement { size: s, position: crate::unit::Offset::new(0.0, 0.0) });
                }
                _ => placements.push(crate::layout::Placement { size: crate::unit::Size::ZERO, position: crate::unit::Offset::new(0.0, 0.0) }),
            }
        }
        // 第二轮：input——剩余宽 = **容器实际宽**（constraints.max_width 是
        // 可用空间上限——容器最终宽 = min_width 提升（如 280）而非 max（如
        // 380）——用 max 定位右对齐元素会越界（suffix/trailing 画到容器外）。
        // M3 间距（源码 TextFieldImpl.kt）：
        // - PrefixSuffixTextPadding = 2dp：prefix↔输入、输入↔suffix
        // - leading 后内容间距 = 4dp（startPadding = 16 - iconPadding(12)）
        // - suffix 右端 = trailing 左端（无间距）
        // - leading/trailing 垂直居中容器；prefix/suffix 与输入文本同位
        let left = if leading_w > 0.0 { leading_w + 4.0 } else { 0.0 };
        let right = if trailing_w > 0.0 { trailing_w } else { 0.0 };
        const AFFIX_GAP: f32 = 2.0; // PrefixSuffixTextPadding
        // 容器宽：min_width 兜底起步——input 测量后再按内容回算（超长输入
        // 撑宽容器）。⚠ 循环依赖：input 测量需要 input_w → 用 min 起步
        let width = constraints.min_width;
        let input_w = (width - left - prefix_w - AFFIX_GAP - suffix_w - right).max(0.0);
        let mut input_size = crate::unit::Size::ZERO;
        let mut input_pos_x = 0.0f32;
        for (i, &c) in children.iter().enumerate() {
            if roles[i] == TextFieldSlotRole::Input {
                let (s, _) = measure_node(nodes, policies, c, crate::layout::Constraints::new(0.0, input_w, 0.0, constraints.max_height));
                // ⚠ 空文本 paragraph 测量 0×0 → 输入节点尺寸 0 → render_pass1
                // `w<=0 || h<=0 return` → 光标不绘制（空字段聚焦无光标）。
                // 强制最小尺寸：高 ≥ token 行高、宽 ≥ 1（渲染进入 +
                // 光标绘制在内容起点）。
                let line_h = self.line_height;
                let s = crate::unit::Size::new(
                    if s.width < 1.0 { 1.0 } else { s.width },
                    if s.height < line_h { line_h } else { s.height },
                );
                input_size = s;
                // prefix 与输入 2dp（PrefixSuffixTextPadding）
                input_pos_x = left + prefix_w + AFFIX_GAP;
                // Bare no_container fields: container height == input height (see final
                // size below), so input y=0 is already centered. (Variants: M3 lower
                // text zone semantics, also y=0.)
                let input_y = 0.0;
                placements[i] = crate::layout::Placement { size: s, position: crate::unit::Offset::new(input_pos_x, input_y) };
                break;
            }
        }
        // 第三轮：label/placeholder 测量 + 全部定位
        let progress = self.label_progress.peek();
        // Bare-field content height: measured children max (NOT the constraint max —
        // loose parents hand down screen-sized maxes). Variants keep the
        // constraint-derived center (usually tight, M3 zone semantics).
        let bare_content_h = if self.variant.is_none() {
            let mut m = input_size.height;
            for p in placements.iter() {
                m = m.max(p.size.height);
            }
            // Policy constraints are already padding-stripped (content-area space):
            // do NOT subtract pad again (that double-counts and top-aligns).
            // Collapsed pill: min 40 → center in 40; expanded loose: min 0 → wrap.
            m.max(constraints.min_height.max(0.0))
        } else {
            0.0 // unused for variants
        };
        for (i, &c) in children.iter().enumerate() {
            match roles[i] {
                TextFieldSlotRole::Label => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    // 展开：**垂直居中于容器**（M3 specs：Label alignment
                    // (unpopulated) = vertically centered——容器中心；源码
                    // startY = CenterVertically.align(label.height, height)）；
                    // 悬浮：内容区顶部**上方**——锚点按变体：
                    // - Filled：label 顶对齐容器 8dp（内容区顶 24 → 偏移 -16）
                    // - Outlined：label 中心跨边框线（顶对齐容器 -8 → 偏移 -24）
                    // ⚠ policy 收到的是扣除 padding 后的约束——内容区顶部即
                    // 容器 padding 边界
                    let content_h = text_field_content_height(&constraints, self.supporting_h, input_size.height);
                    let container_center = (content_h + self.pad_bottom - self.pad_top) / 2.0;
                    let expanded_y = container_center - s.height / 2.0;
                    let float_y = -(s.height / 2.0)
                        - match self.variant {
                            Some(TextFieldVariant::Filled) => 8.0,
                            Some(TextFieldVariant::Outlined) => 16.0,
                            None => 0.0,
                        };
                    let y = expanded_y + (float_y - expanded_y) * progress;
                    placements[i] = crate::layout::Placement { size: s, position: crate::unit::Offset::new(input_pos_x, y) };
                }
                TextFieldSlotRole::Placeholder => {
                    let (s, _) = measure_node(nodes, policies, c, text_c(constraints.max_width));
                    // Bare fields: center in measured content height (see above).
                    // Variants: content top (M3 zone semantics).
                    let ph_y = if self.variant.is_none() {
                        ((bare_content_h - s.height) / 2.0).max(0.0)
                    } else {
                        0.0
                    };
                    placements[i] = crate::layout::Placement { size: s, position: crate::unit::Offset::new(input_pos_x, ph_y) };
                }
                TextFieldSlotRole::Leading => {
                    // Bare fields: center in measured content height (constraint max is
                    // screen-sized under loose parents).
                    // Variants: center in container (constraint-derived, usually tight).
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(
                            0.0,
                            ((bare_content_h - placements[i].size.height) / 2.0).max(0.0),
                        );
                        continue;
                    }
                    // 垂直居中于**容器**（M3 specs：Icon alignment = vertically
                    // centered——容器中心；输入文本区在容器下部（Filled 底 8 /
                    // Outlined 居中），居中于输入区会偏下）。容器中心（内容区
                    // 坐标）= (内容区高 + pad_bottom - pad_top)/2；允许负 y
                    //（Filled 图标跨 16..40 区）。⚠ 滚动容器内约束高 =
                    // f32::MAX（有限但巨大）——降级见 text_field_content_height
                    let content_h = text_field_content_height(&constraints, self.supporting_h, input_size.height);
                    let container_center = (content_h + self.pad_bottom - self.pad_top) / 2.0;
                    placements[i].position = crate::unit::Offset::new(
                        0.0,
                        container_center - placements[i].size.height / 2.0,
                    );
                }
                TextFieldSlotRole::Trailing => {
                    // Bare fields: same measured-height centering as Leading above.
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(
                            (width - placements[i].size.width).max(0.0),
                            ((bare_content_h - placements[i].size.height) / 2.0).max(0.0),
                        );
                        continue;
                    }
                    let content_h = text_field_content_height(&constraints, self.supporting_h, input_size.height);
                    let container_center = (content_h + self.pad_bottom - self.pad_top) / 2.0;
                    placements[i].position = crate::unit::Offset::new(
                        (width - placements[i].size.width).max(0.0),
                        container_center - placements[i].size.height / 2.0,
                    );
                }
                TextFieldSlotRole::Prefix => {
                    // Bare: center in measured content (input itself centered).
                    // Variants: with input text same vertical position.
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(left, ((bare_content_h - placements[i].size.height) / 2.0).max(0.0));
                    } else {
                        placements[i].position = crate::unit::Offset::new(left, (input_size.height - placements[i].size.height).max(0.0) / 2.0);
                    }
                }
                TextFieldSlotRole::Suffix => {
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(
                            (width - right - placements[i].size.width).max(0.0),
                            ((bare_content_h - placements[i].size.height) / 2.0).max(0.0),
                        );
                    } else {
                        // M3：suffix 右端 = trailing 左端（无间距）；与输入文本
                        // 间距 2dp（PrefixSuffixTextPadding——由 input_w 预留）
                        placements[i].position = crate::unit::Offset::new(
                            (width - right - placements[i].size.width).max(0.0),
                            (input_size.height - placements[i].size.height).max(0.0) / 2.0,
                        );
                    }
                }
                TextFieldSlotRole::Input => {
                    if self.variant.is_none() {
                        placements[i].position = crate::unit::Offset::new(input_pos_x, ((bare_content_h - placements[i].size.height) / 2.0).max(0.0));
                    }
                }
            }
        }
        let _ = progress;
        // 容器尺寸：宽 = 内容（输入+前后缀+间距+图标区）受约束夹取（min_width
        // 280 兜底——超长输入撑宽）；高 = input 高 + supporting 区。
        // ⚠ supporting 恒在输入内容区外（渲染端画在容器底部 4dp 外侧，且
        // container_rect = 节点高 - supporting_h）——measure 高度必须含
        // supporting_h，否则多行换行（input 高超过 min_height 兜底）时
        // 节点总高缺 supporting 20px → 指示线/边框上移 → 穿过末行文字
        let width = constraints.constrain_width(input_size.width + left + prefix_w + AFFIX_GAP + suffix_w + right);
        let height = constraints.constrain_height(input_size.height + self.supporting_h);
        (crate::unit::Size::new(width, height), placements)
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[crate::layout::Placement]) {
        for (i, &c) in children.iter().enumerate() {
            if let Some(p) = placements.get(i) {
                nodes[c].position = p.position;
                nodes[c].measured_size = p.size;
            }
        }
    }
}


pub struct TextField {
    value: State<TextFieldValue>,
    on_value_change: Box<dyn Fn(TextFieldValue) + Send + Sync>,
    /// Search action (cf. Compose IME `ImeAction.Search` → `onSearch`): fired when
    /// Enter is pressed in single-line mode. None = Enter is swallowed (status quo).
    on_search: Option<std::sync::Arc<dyn Fn(TextFieldValue) + Send + Sync>>,
    modifier: Modifier,
    font_size: Option<crate::unit::TextUnit>,
    /// 是否启用（禁用：不聚焦不响应键盘，视觉 50% alpha——对标 Compose enabled）
    enabled: bool,
    /// 只读（可聚焦/选中，不可编辑——编辑键吞掉不生效，导航键保留）
    read_only: bool,
    /// 占位文字（值空时显示——组合内容闭包，如
    /// `|ctx| { Text::new("Enter name").build(ctx); }`；仅显示条件满足时构建）。
    /// 默认文字样式由 TextField 提供（LOCAL_TEXT_STYLE：on_surface_variant
    /// 色 + 淡入 alpha）——闭包内 Text 无需显式样式，可自行覆盖
    placeholder: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    /// 单行模式（Enter 吞掉不换行——对标 Compose singleLine）
    single_line: bool,
    /// 最大行数（对标 Compose maxLines，默认无限）
    max_lines: usize,
    /// 最小行数（对标 Compose minLines，默认 1——空内容也占位）
    min_lines: usize,
    /// 交互源（None = build 时内部 remember——对标 Compose TextField 可选注入）
    interaction_source: Option<crate::interaction::MutableInteractionSource>,
    /// 错误状态（对标 Compose isError——文字/边框错误色，供按状态取色）
    is_error: bool,
    /// 容器变体（None = 无容器视觉——裸输入；Filled/Outlined 对齐 M3）
    variant: Option<TextFieldVariant>,
    /// 状态色（None = 按 variant 从主题生成——M3 默认）
    colors: Option<TextFieldColors>,
    /// 浮动 label（聚焦或非空时悬浮到容器顶部；组合内容闭包——text-field-v2
    /// 容器化：子节点由 TextFieldLayout 定位，悬浮/展开位置动画内部控制）。
    /// 默认文字样式由 TextField 提供（LOCAL_TEXT_STYLE：展开 16sp ↔ 悬浮 12sp
    /// 动画字号 + on_surface_variant 色）——闭包内 Text 无需显式字号
    label: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    /// 支持文本（容器底部外侧 12sp）
    supporting_text: Option<String>,
    /// 视觉变换（密码掩码/格式化输入——对标 Compose visualTransformation；
    /// None = 恒等）
    visual_transformation: Option<std::sync::Arc<dyn crate::text::transformation::VisualTransformation>>,
    /// 前置图标（M3 leadingIcon——组合内容闭包：12dp 边距垂直居中，
    /// 与文本 16dp 间距）
    leading_icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    /// 后置图标（M3 trailingIcon——右 12dp 垂直居中）
    trailing_icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    /// 前缀（输入文本前——组合内容闭包）
    prefix: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    /// 后缀（右对齐——组合内容闭包）
    suffix: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
}

impl TextField {
    /// 创建文本输入框（仅绑定文本状态）。
    ///
    /// 值变化回调默认忽略（`|_| {}`）；需要监听编辑结果（受控更新外部
    /// 状态）时用 `.on_value_change(f)` 单独设置。
    pub fn new(value: State<TextFieldValue>) -> Self {
        Self {
            value,
            on_value_change: Box::new(|_| {}),
            on_search: None,
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
            // 默认 M3 Filled 容器（对齐 material3 TextField 默认变体）——
            // 裸输入（无容器视觉）用 .no_container() 显式选择
            variant: Some(TextFieldVariant::Filled),
            colors: None,
            label: None,
            supporting_text: None,
            visual_transformation: None,
            leading_icon: None,
            trailing_icon: None,
            prefix: None,
            suffix: None,
        }
    }

    /// 值变化回调（受控更新——编辑后同步外部状态；new 默认忽略）。
    /// 参数为编辑后的新 TextFieldValue（text/selection）
    pub fn on_value_change(mut self, f: impl Fn(TextFieldValue) + Send + Sync + 'static) -> Self {
        self.on_value_change = Box::new(f);
        self
    }

    /// Search action (cf. Compose `KeyboardActions(onSearch)` with forced
    /// `ImeAction.Search`): fired with the current value when Enter is pressed in
    /// single-line mode. Multi-line Enter still inserts newline.
    pub fn on_search(mut self, f: impl Fn(TextFieldValue) + Send + Sync + 'static) -> Self {
        self.on_search = Some(std::sync::Arc::new(f));
        self
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

    /// 占位文字（值空时显示——组合内容闭包；仅显示条件满足时构建）。
    /// 默认文字样式由 TextField 提供（LOCAL_TEXT_STYLE：on_surface_variant
    /// 色 + 淡入 alpha）——闭包内 Text 无需显式样式
    pub fn placeholder(mut self, placeholder: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.placeholder = Some(Box::new(placeholder));
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
    pub fn interaction_source(mut self, source: crate::interaction::MutableInteractionSource) -> Self {
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

    /// 无容器视觉（裸输入——无背景/边框/指示线，仅文本 + padding）。
    /// 默认是 Filled；`.no_container()` 显式选择裸输入（如嵌入卡片/
    /// 自定义容器内不想画 M3 容器时）。视觉变换（visual_transformation）
    /// 不受影响（offset_mapping 经 TextFieldOffsetMapping 挂载）
    pub fn no_container(mut self) -> Self {
        self.variant = None;
        self
    }

    /// 状态色（None = 按 variant 从主题生成 M3 默认色）
    pub fn colors(mut self, colors: TextFieldColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 浮动 label（聚焦或非空时悬浮到容器顶部——组合内容闭包，如
    /// `|ctx| { Text::new("Name").font_size(16.0).build(ctx); }`；
    /// 悬浮/展开位置动画由 TextFieldLayout 内部控制）
    /// 默认文字样式由 TextField 提供（LOCAL_TEXT_STYLE：展开 16sp ↔ 悬浮
    /// 12sp 动画字号 + 状态色）——闭包内 Text 无需显式字号
    pub fn label(mut self, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.label = Some(Box::new(label));
        self
    }

    /// 支持文本（容器底部外侧 12sp——对齐 M3 supportingText；错误时 error 色）
    pub fn supporting_text(mut self, text: impl Into<String>) -> Self {
        self.supporting_text = Some(text.into());
        self
    }

    /// 视觉变换（对标 Compose `visualTransformation`）——密码掩码
    /// `PasswordTransformation`、格式化输入（自定义 OffsetMapping）。
    /// 显示文本 ≠ 编辑文本；光标/选区/定位自动经 OffsetMapping 转换
    pub fn visual_transformation(mut self, t: impl Into<std::sync::Arc<dyn crate::text::transformation::VisualTransformation>>) -> Self {
        self.visual_transformation = Some(t.into());
        self
    }

    /// 前置图标（M3 leadingIcon——组合内容闭包，如
    /// `|ctx| { Icon::new(...).build(ctx); }`；12dp 边距垂直居中，
    /// 与文本 16dp 间距）
    pub fn leading_icon(mut self, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.leading_icon = Some(Box::new(icon));
        self
    }

    /// 后置图标（M3 trailingIcon——右 12dp 垂直居中）
    pub fn trailing_icon(mut self, icon: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.trailing_icon = Some(Box::new(icon));
        self
    }

    /// 前缀（$、￥ 等——输入文本前，组合内容闭包）
    pub fn prefix(mut self, text: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.prefix = Some(Box::new(text));
        self
    }

    /// 后缀（%、kg 等——右对齐，组合内容闭包）
    pub fn suffix(mut self, text: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.suffix = Some(Box::new(text));
        self
    }

    /// 组件方法宏化（新 key 系统）：scope key = 调用链（16 字段实例隔离）——
    /// 内部 remember/next_key 编译期编号（fnv(调用链, 语句id, 序号)）；
    /// slot_wrap!（macro_rules——宏扫描不深入）内部 next_key 走运行时
    /// （build 语句注入提供稳定 base）
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // Closure/branch-only params (invisible to modifier param_eq, whose
        // KbEvent arm always compares equal): declare them so a flip forces
        // the container group to Enter and rebuild kb_handler instead of
        // Skip-reusing the stale closure (docked-filter root cause).
        ctx.changed(&self.read_only);
        ctx.changed(&self.enabled);
        ctx.changed(&self.single_line);
        ctx.changed(&self.is_error);
        let key = ctx.next_key();
        let current = self.value.get();
        let content = current.text.clone();

        let theme = WiniaTheme::colors();
        let typography = WiniaTheme::typography();
        let mut input_style = typography.body_large.clone();
        if let Some(font_size) = self.font_size {
            input_style.font_size = Some(font_size);
        }
        let font_size = input_style.font_size.unwrap_or(16.0.into()).to_logical_px(crate::runtime::density::current_density());
        let input_line_height = text_style_line_height(&input_style, font_size);
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
                .unwrap_or_else(|| ctx.remember(|| crate::interaction::MutableInteractionSource::new()).get()))
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
        // 视觉变换（密码掩码/格式化输入——对标 Compose visualTransformation）：
        // 显示文本 = transform(原始文本)，偏移映射跨界转换（光标/选区绘制用
        // original→transformed，点击/拖动定位用 transformed→original）。
        // ⚠ placeholder 不经过变换（显示原样）
        let transformation = self.visual_transformation.clone()
            .unwrap_or_else(|| std::sync::Arc::new(crate::text::transformation::IdentityTransformation::new()));
        let transformed = transformation.filter(&content);
        let offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping> = transformed.offset_mapping;
        // 显示文本恒为变换结果（placeholder 为闭包子节点——text-field-v2
        // 容器化：由 TextFieldLayout 定位在输入位）
        let display_content = transformed.text.clone();
        let display_color = color;
        // label 悬浮动画进度（0 = 展开 / 1 = 悬浮——M3 FastSpatial 150ms）。
        // ⚠ 无条件调用（label 有无切换不漂移其后 remember key）；policy 与
        // 渲染缺口共用
        let label_progress = ctx.animate_float_as_state(
            if label_float { 1.0 } else { 0.0 },
            crate::animation::AnimationSpec::Tween(
                crate::animation::TweenSpec::new(
                    std::time::Duration::from_millis(150),
                    crate::animation::interpolator::EaseInOutCubic::new(),
                )
            ),
        );

        // 光标闪烁状态。⚠ `cursor_visible.get()` 必须在 start_leaf **之前**
        // 读取——此刻依赖注册到父 scope，闪烁翻转 → 父重组 → build 重跑 →
        // desc 更新 → 节点同步（start_leaf 后读取会注册到 leaf——leaf 不
        // 触发 build 重跑，光标永远停留在 build 时的值）
        let cursor_visible = ctx.remember(|| true);
        let cursor_visible_now = cursor_visible.get();
        let cv = cursor_visible.clone();
        // 最后交互时刻（点击/按键）——光标立即显示并重置闪烁周期
        // （对齐 Compose `snapToVisibleAndAnimate`：文本/选区变化时光标
        // 闪到可见并重启周期，打字时不消失）
        let last_blink = ctx.remember(|| std::sync::Arc::new(parking_lot::Mutex::new(std::time::Instant::now()))).get();
        let blink_started = ctx.remember(|| false);
        // 组合生命周期协程作用域（§3.11 修复）：blink 任务纳入 effect 生命周期——
        // TextField 移除时 ScopeState::drop 自动 abort，不再裸 tokio::spawn 泄漏。
        // ⚠ 必须在 if 外无条件调用（内部 remember 需要稳定的组合位置 key——
        // 放进条件分支会让后续 remember 错位）
        let scope = crate::effect::remember_coroutine_scope(ctx);
        if !blink_started.get() {
            blink_started.set(true);
            let cv2 = cv.clone();
            let last2 = last_blink.clone();
            // ⚠ 聚焦检查：光标只在聚焦时闪烁（对齐 Compose——非聚焦不显示
            // 光标、不触发重组）。闪烁任务捕获 interaction source——翻转前
            // 检查 is_focused()，非聚焦直接跳过（不翻转 cursor_visible →
            // 不 notify → 无重组）。修复：text_field_demo 17 字段每 ~500ms
            // 全量重组（每字段一个闪烁任务同步翻转）。
            let interaction2 = interaction.clone();
            scope.spawn(async move {
                loop {
                    // 100ms 轮询（500ms 相位粒度——交互重置精度 ±100ms）
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    // 非聚焦：不翻转（光标不可见且不触发重组）
                    let focused_now = interaction2.as_ref().map(|s| s.is_focused()).unwrap_or(false);
                    if !focused_now {
                        continue;
                    }
                    let due = last2.lock().elapsed() >= std::time::Duration::from_millis(500);
                    if due {
                        // ⚠ 翻转后必须重置计时器——否则下一次 tick（100ms 后）
                        // elapsed 仍 >= 500ms 再次翻转 → 光标每 100ms 狂闪
                        *last2.lock() = std::time::Instant::now();
                        cv2.update(|v| *v = !*v);
                    }
                }
            });
        }
        // 光标立即可见 + 计时器重置（点击定位/键盘操作后调用）
        let blink_reset = {
            let cv2 = cv.clone();
            let last2 = last_blink.clone();
            move || {
                *last2.lock() = std::time::Instant::now();
                cv2.set(true); // 相等时 set 跳过通知（无多余重组）
            }
        };

        // 键盘事件处理（read_only：编辑键吞掉不生效；enabled=false：不注册）
        let value = self.value.clone();
        let on_change = std::sync::Arc::new(std::sync::Mutex::new(self.on_value_change));
        // 内部选区 registrar——拖动选择复用 SelectionContainer 拖动管线
        // （app.rs handle_pointer_move 的 compute_selection 依赖节点
        // registrar；TextField 此前不注册 → 拖动被跳过）。选区经
        // set_on_change 同步回 value（拖动结束 fire_on_change）
        let registrar = ctx.remember(|| crate::text::selection::SelectionRegistrar::new()).get();
        {
            let v = value.clone();
            let mapping = offset_mapping.clone();
            registrar.set_on_change(move |sel: &crate::text::selection::Selection| {
                v.update(|val| {
                    // reg 空间 = 显示文本（global_offset=0 单段）——转换回
                    // 编辑偏移写入 value
                    let len = val.text.len();
                    let s = mapping.transformed_to_original(sel.start()).min(len);
                    let e = mapping.transformed_to_original(sel.end()).min(len);
                    val.selection = s..e;
                });
            });
        }
        // 注册本段移到输入 leaf 构建处（须用 leaf 的 slot_key——见输入子
        // 节点注释）。此处容器 key 注册会导致拖动定位/渲染高亮查不到段
        // （节点 slot_key ≠ 容器 key）。
        // UndoManager（组合点 remember——跨帧持久，键位处理共享）
        let undo = ctx.remember(|| std::sync::Arc::new(parking_lot::Mutex::new(UndoManager::new()))).get();
        let kb_handler = {
            let v = value.clone();
            let cb = on_change.clone();
            let on_search = self.on_search.clone();
            let undo = undo.clone();
            let registrar = registrar.clone();
            let mapping = offset_mapping.clone();
            let blink_reset = blink_reset.clone();
            // 编辑提交：value 更新后同步 reg（防 build 的 reg_leads 用旧选区
            // 拉回——Preedit/键盘删选区后 reg 残留旧选区 → 组合期间误删）
            macro_rules! commit {
                ($val:expr) => {{
                    // value（编辑偏移）→ reg（显示偏移）
                    registrar.set_selection(
                        mapping.original_to_transformed($val.selection.start),
                        mapping.original_to_transformed($val.selection.end),
                    );
                    v.set($val);
                }};
            }
            let read_only = self.read_only;
            let single_line = self.single_line;
            move |e: &crate::input::KbEvent| -> bool {
                if e.event_type != crate::input::KbEventType::KeyDown { return false; }
                // 任何按键处理前：光标立即可见 + 闪烁计时器重置
                // （用户交互时光标不消失——对齐 Compose snapToVisibleAndAnimate）
                blink_reset();
                let key = &e.key;
                // 桌面修饰键：Ctrl（macOS 用 Cmd——Compose commonKeyMapping 同款）。
                // ⚠ Meta（⊞ Win）仅 macOS 并入——Windows 上 Win+Z/V/A/← 是系统
                // 快捷键（剪贴板历史/窗口贴靠），并入会导致编辑与系统冲突
                let ctrl = e.is_ctrl_pressed || (cfg!(target_os = "macos") && e.is_meta_pressed);
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
                // 剪贴板/全选（editsText=false 命令——read_only 也允许）
                // Ctrl+A 全选
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("a")) {
                    let mut val = v.get();
                    undo.lock().push(&val.text, &val.selection, val.composing_range.clone());
                    val.selection = 0..val.text.len();
                    commit!(val);
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
                // 键盘编辑前结束组合（对齐 Compose FinishComposingTextCommand：
                // 拼音 Commit 走逐字符 KbEvent——组合文本须先移除，否则残留
                // 在正文；结束组合本身是编辑 → 快照 + 通知）。
                // ⚠ 放在 read_only 检查之后（只读不得编辑——结束组合会删文本）；
                // ⚠ Ctrl+A/C（复制/全选，editsText=false）不结束组合
                {
                    let copy_like = ctrl && matches!(key, winit::keyboard::Key::Character(c)
                        if c.eq_ignore_ascii_case("a") || c.eq_ignore_ascii_case("c"));
                    if !copy_like {
                        let mut val = v.get();
                        if val.composing_range.is_some() {
                            end_composition(&mut val);
                            undo.lock().push(&val.text, &val.selection, val.composing_range.clone());
                            commit!(val.clone());
                            if let Ok(cb) = cb.lock() { cb(val); }
                        }
                    }
                }
                // Ctrl+Z 撤销 / Ctrl+Shift+Z、Ctrl+Y 重做
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("z")) {
                    let mut val = v.get();
                    let cur = (val.text.clone(), val.selection.clone(), val.composing_range.clone());
                    let restored = if shift || e.is_meta_pressed {
                        undo.lock().redo(cur)
                    } else {
                        undo.lock().undo(cur)
                    };
                    if let Some((text, sel, composing)) = restored {
                        val.text = text;
                        val.selection = sel;
                        val.composing_range = composing;
                        commit!(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                    }
                    return true;
                }
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("y")) {
                    let mut val = v.get();
                    let cur = (val.text.clone(), val.selection.clone(), val.composing_range.clone());
                    if let Some((text, sel, composing)) = undo.lock().redo(cur) {
                        val.text = text;
                        val.selection = sel;
                        val.composing_range = composing;
                        commit!(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                    }
                    return true;
                }
                // Ctrl+X 剪切
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("x")) {
                    let mut val = v.get();
                    undo.lock().push(&val.text, &val.selection, val.composing_range.clone());
                    let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                    if s != e {
                        clipboard_set_text(&val.text[s..e]);
                        val.text.replace_range(s..e, "");
                        val.selection = s..s;
                        commit!(val.clone());
                        if let Ok(cb) = cb.lock() { cb(val); }
                    }
                    return true;
                }
                // Ctrl+V 粘贴（单行：换行符替换为空格——Compose 单行语义）
                if ctrl && matches!(key, winit::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v")) {
                    let Some(clip) = clipboard_get_text() else { return true; };
                    let clip = if single_line { clip.replace(['\n', '\r'], " ") } else { clip };
                    let mut val = v.get();
                    undo.lock().push(&val.text, &val.selection, val.composing_range.clone());
                    let (s, e) = (val.selection.start.min(val.selection.end), val.selection.start.max(val.selection.end));
                    val.text.replace_range(s..e, &clip);
                    let caret = s + clip.len();
                    val.selection = caret..caret;
                    commit!(val.clone());
                    if let Ok(cb) = cb.lock() { cb(val); }
                    return true;
                }
                let mut val = v.get();
                // 每次键处理前快照（对齐 Compose forceNextSnapshot：
                // 编辑与移动都会记录——同文本合并 selection）
                undo.lock().push(&val.text, &val.selection, val.composing_range.clone());
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
                            commit!(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            let target = word_next(&val.text, val.selection.start);
                            if shift {
                                val.selection = val.selection.start.min(target)..val.selection.end.max(target);
                            } else {
                                val.selection = target..target;
                            }
                            commit!(val);
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
                            commit!(val.clone());
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
                            commit!(val.clone());
                            if let Ok(cb) = cb.lock() { cb(val); }
                            return true;
                        }
                        winit::keyboard::NamedKey::Home => {
                            // Ctrl+Home：文首（对齐 PREV_PARAGRAPH 近似）
                            val.selection = 0..0;
                            commit!(val);
                            return true;
                        }
                        winit::keyboard::NamedKey::End => {
                            val.selection = val.text.len()..val.text.len();
                            commit!(val);
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
                                commit!(val.clone());
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
                                commit!(val.clone());
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
                                commit!(val.clone());
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
                                commit!(val.clone());
                                if let Ok(cb) = cb.lock() { cb(val); }
                            }
                            return true;
                        }
                        winit::keyboard::NamedKey::Enter => {
                            // 单行模式：Enter 吞掉不换行（对标 Compose singleLine）
                            if single_line {
                                // Search action (IME Search equivalent): fire on_search
                                // with the current value instead of inserting anything.
                                if let Some(search) = on_search.clone() {
                                    search(v.get());
                                }
                                return true;
                            }
                            let change = TextChange::Inserted { index: val.selection.start, text: "\n".into() };
                            change.apply_to(&mut val);
                            commit!(val.clone());
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
                                commit!(val);
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
                                    commit!(val);
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
                            commit!(val);
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
                            commit!(val);
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
                            commit!(val);
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
                            commit!(val);
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
                        commit!(val.clone());
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
        // ═══ text-field-v2 容器化：容器 modifier（背景/指示线/边框/padding）
        // + 输入子节点（文本/光标/选区/IME）+ 闭包子节点（图标/label/
        // placeholder/前后缀）——TextFieldLayout policy 按角色布局 ═══
        // M3 specs（m3.material.io/components/text-fields/specs）：
        // - 左右 padding：无图标 16 / 有图标 12（图标垂直居中、与文本间距 16）
        // - Filled + label：文本顶 24（label 悬浮区 8..24，label 顶 8）+ 底 8
        // - Filled 无 label：顶 16 + 底 8
        // - Outlined：四边 16（label 跨边框不占容器内空间）
        let has_leading_icon = self.leading_icon.is_some();
        let has_trailing_icon = self.trailing_icon.is_some();
        let container_modifier = self.modifier;
        // M3 padding（TextFieldLayout 用 pad_top/bottom 计算图标容器居中锚点）
        let (mut pad_top, mut pad_bottom) = (8.0, 8.0);
        let container_modifier = if let Some(variant) = visual {
            let pad_h = if has_leading_icon || has_trailing_icon { 12.0 } else { 16.0 };
            (pad_top, pad_bottom) = match variant {
                TextFieldVariant::Filled if self.label.is_some() => (24.0, 8.0),
                TextFieldVariant::Filled => (16.0, 8.0),
                _ => (16.0, 16.0),
            };
            let shape = crate::graphics::Shape::RoundedRect {
                corner_radius: 4.0,
            };
            // 支持文本：容器底部外侧 12sp（M3 supporting 色）
            let supporting_visual = self.supporting_text.as_ref().map(|s| crate::modifier::SupportingVisual {
                content: s.clone(),
                font_size: typography.body_small.font_size.unwrap_or(12.0.into()).to_logical_px(crate::runtime::density::current_density()),
                font_weight: typography.body_small.font_weight.unwrap_or_default(),
                font_style: typography.body_small.font_style.unwrap_or_default(),
                letter_spacing: typography.body_small.letter_spacing.unwrap_or(0.0),
                line_height: typography.body_small.line_height.map(|height| height.to_logical_px(crate::runtime::density::current_density())),
                color: if disabled { colors.disabled_supporting }
                    else if self.is_error { colors.error_supporting }
                    else { colors.supporting },
            });
            // The indicator colour follows M3's `FastEffects` spring — see `text_field_colour_spec`,
            // which also records where winia animates a different object than Compose does.
            let indicator_anim = ctx.animate_color_as_state(
                colors.indicator_color(self.enabled, self.is_error, focused),
                crate::animation::AnimationSpec::Spring(text_field_colour_spec()),
            );
            let focus_progress = ctx.animate_float_as_state(
                if focused { 1.0 } else { 0.0 },
                crate::animation::AnimationSpec::Tween(
                    crate::animation::TweenSpec::new(
                        std::time::Duration::from_millis(150),
                        crate::animation::interpolator::EaseInOutCubic::new(),
                    )
                ),
            );
            container_modifier
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
                    self.read_only,
                    if self.is_error { colors.error_cursor } else { colors.cursor },
                    indicator_anim,
                    focus_progress,
                    Some(offset_mapping.clone()),
                    supporting_visual,
                )
        } else {
            // 裸 TextField（无 variant）：无 TextFieldVisual 挂载点——offset_mapping
            // 挂到容器（TextFieldOffsetMapping 元素；offset_mapping_for_node 沿
            // parent 链向上查找命中），否则 visual_transformation 的显示/编辑
            // 偏移转换在点击定位/渲染光标处查不到（显示偏移直写 selection 越界）
            container_modifier
                .padding(8.0)
                .text_field_offset_mapping(offset_mapping.clone())
        };
        // 容器最小高度（supporting + min_lines + 56）——min_height 兜底
        // 占位；测量用 paragraph 实际高度（含折行）
        let supporting_h = self.supporting_text.as_ref().map_or(0.0, |_| {
            4.0 + typography.body_small.line_height
                .map(|height| height.to_logical_px(crate::runtime::density::current_density()))
                .unwrap_or_else(|| typography.body_small.font_size.unwrap_or(12.0.into()).to_logical_px(crate::runtime::density::current_density()) * 1.4)
        });
        let container_modifier = if self.supporting_text.is_some() || self.min_lines > 1 {
            let line_h = input_line_height;
            let (pt, pb) = container_modifier.get_padding_vertical();
            let pad_y = pt + pb;
            let min_content_h = self.min_lines as f32 * line_h;
            let has_visual = visual.is_some();
            let mut min_h = min_content_h + pad_y + supporting_h;
            if has_visual {
                min_h = min_h.max(56.0 + supporting_h);
            }
            container_modifier.min_height(min_h)
        } else {
            container_modifier
        };
        // 输入子节点 modifier：文本 + 点击定位 + 角色标记。
        // ⚠ 焦点/键盘在**容器**（焦点语义：点击容器任意处聚焦——padding/
        // 图标区也命中，对标 Compose 全容器可交互）；输入子节点 on_press
        // 定位光标（点击文本区域）
        let fr = ctx.remember(|| crate::modifier::FocusRequester::new()).get();
        let input_modifier = Modifier::new()
            .text_field_slot(TextFieldSlotRole::Input)
            .text_content_full(
                display_content,
                font_size,
                display_color,
                input_style.font_weight.unwrap_or_default(),
                input_style.font_style.unwrap_or_default(),
                if self.single_line { 1 } else { self.max_lines },
                crate::text::TextAlign::Left,
                crate::text::TextOverflow::Clip,
                input_style.soft_wrap.unwrap_or(true),
                input_style.letter_spacing.unwrap_or(0.0),
                input_style.line_height.map(|height| height.to_logical_px(crate::runtime::density::current_density())),
            );
        let input_modifier = if interaction.is_some() {
            let fr_click = fr.clone();
            input_modifier
                .on_press(move |_| { fr_click.request_focus(); })
        } else {
            input_modifier
        };
        // 焦点/键盘在容器（点击容器任意处聚焦——全容器可交互，对标
        // Compose BasicTextField 的 interactionSource + focus 语义）
        let container_modifier = if let Some(interaction) = interaction.as_ref() {
            let fr_click = fr.clone();
            container_modifier
                .focusable_with_source(&interaction)
                .focus_requester(&fr)
                .on_press(move |_| { fr_click.request_focus(); })
                .on_key_event(kb_handler)
        } else {
            container_modifier
        };

        // ═══ 容器组：闭包子节点 + 输入子节点 ═══
        ctx.start_restartable_group(
            key,
            container_modifier,
            TextFieldLayout::new(label_progress.clone(), visual, pad_top, pad_bottom, supporting_h, input_line_height),
        );
        // 闭包子节点包装（角色标记 + Box 层叠——内容由闭包构建）。
        // 每个槽位提供 M3 默认样式（LOCAL_TEXT_STYLE / LOCAL_CONTENT_COLOR
        // 经 CompositionLocal 传递——闭包内组件默认即 M3 规格；显式参数覆盖）：
        // - prefix/suffix：16sp + affix 色（disabled 38%）
        // - leading/trailing icon：内容色 = on_surface_variant（disabled 38%；
        //   trailing 错误态 error 色）
        // ⚠ 全部槽位用**显式 ctx.key**（不依赖共享序号流）：show_placeholder
        // 条件切换（if 分支插入/移除 remember+组）会平移后续 next_key 序号——
        // 槽位组 key 漂移 → 复用错误缓存（placeholder 测量 0×0 渲染不可见）
        macro_rules! slot_wrap {
            ($role:expr, $content:expr) => {{
                ctx.key($role, |ctx| {
                    let sk = ctx.next_key();
                    ctx.start_restartable_group(
                        sk,
                        Modifier::new().text_field_slot($role),
                        crate::layout::BoxLayout::new(),
                    );
                    match $role {
                        TextFieldSlotRole::Prefix | TextFieldSlotRole::Suffix => {
                            let affix_color = if !self.enabled { colors.disabled_affix } else { colors.affix };
                            let mut style = typography.body_large.clone();
                            style.color = Some(affix_color);
                            ProvideTextStyle(style, ctx, $content);
                        }
                        TextFieldSlotRole::Leading | TextFieldSlotRole::Trailing => {
                            let icon_color = if !self.enabled {
                                if $role == TextFieldSlotRole::Leading { colors.leading_icon_disabled }
                                else { colors.trailing_icon_disabled }
                            } else if $role == TextFieldSlotRole::Trailing && self.is_error {
                                colors.trailing_icon_error
                            } else {
                                colors.leading_icon_focused
                            };
                            crate::theme::WiniaTheme::with_content_color(icon_color, ctx, |c| $content(c));
                        }
                        _ => $content(ctx),
                    }
                    ctx.end_restartable_group();
                });
            }};
        }
        if let Some(icon) = self.leading_icon {
            slot_wrap!(TextFieldSlotRole::Leading, icon);
        }
        if let Some(label) = self.label {
            ctx.key(TextFieldSlotRole::Label, |ctx| {
                let sk = ctx.next_key();
                ctx.start_restartable_group(
                    sk,
                    Modifier::new().text_field_slot(TextFieldSlotRole::Label),
                    crate::layout::BoxLayout::new(),
                );
                // M3 默认 label 样式经 LOCAL_TEXT_STYLE 提供（闭包内 Text 默认
                // 字号 = 展开 16sp ↔ 悬浮 12sp 动画插值；色 = 状态色）：
                // - `label_progress.get()` 在组内注册组合依赖——动画推进 →
                //   组重组 → 闭包重跑 → current() 读到新字号（字号动画）
                // - 闭包内 Text 显式 .font_size()/.color() 可覆盖
                let p = label_progress.get();
                let label_color = if !self.enabled { colors.label_disabled }
                    else if self.is_error { colors.label_error }
                    else { colors.label_unfocused };
                let label_style = interpolate_text_style(
                    &typography.body_large,
                    &typography.body_small,
                    p,
                    label_color,
                );
                ProvideTextStyle(label_style, ctx, label);
                ctx.end_restartable_group();
            });
        }
        // Placeholder builds regardless of container variant (bare no_container fields
        // need it too — e.g. SearchBar input). Previously gated on has_visual (v2
        // containerization oversight); the block is self-contained (own key + alpha).
        if let Some(ph) = self.placeholder {
                // M3 placeholderAlpha：淡入**淡出**双向（150ms）——alpha 目标
                // 随显示状态（显示 1 / 隐藏 0）；构建条件 = 显示中或淡出中
                // （alpha > 0）——淡出动画期间仍构建（透明度渐低），alpha
                // 归零后移除。
                // ⚠ remember + push + 组全部包在显式 ctx.key 内——remember
                // 的 key base 固定为 role 哈希（不随帧内序号流漂移——实测
                // 无 label 字段（恒显示）alpha State 每帧新建 → 动画每帧
                // 从 0 重启永不完成 → 持续重组）
                ctx.key(TextFieldSlotRole::Placeholder, |ctx| {
                    let alpha = ctx.remember(|| 0.0f32);
                    crate::animation::push_animatable(
                        alpha.clone(),
                        if show_placeholder { 1.0 } else { 0.0 },
                        crate::animation::AnimationSpec::Tween(
                            crate::animation::TweenSpec::new(
                                std::time::Duration::from_millis(150),
                                crate::animation::interpolator::EaseOutCubic::new(),
                            )
                        ),
                    );
                    // 构建条件：alpha.get() 注册依赖——淡出动画推进 → 重组 →
                    // 条件重评估（alpha=0 时停止构建移除节点）
                    let alpha_now = alpha.get();
                    if show_placeholder || alpha_now > 0.001 {
                        let sk = ctx.next_key();
                        ctx.start_restartable_group(
                            sk,
                            Modifier::new().text_field_slot(TextFieldSlotRole::Placeholder),
                            crate::layout::BoxLayout::new(),
                        );
                        // M3 默认 placeholder 样式：16sp + on_surface_variant
                        // （disabled 38%）+ 淡入/淡出 alpha
                        let a = alpha.get();
                        let pc = if !self.enabled { colors.disabled_placeholder } else { colors.placeholder };
                        let mut style = typography.body_large.clone();
                        style.color = Some(crate::graphics::Color::from_argb((255.0 * a) as u8, pc.r, pc.g, pc.b));
                        ProvideTextStyle(style, ctx, ph);
                        ctx.end_restartable_group();
                    }
                });
            }
        if let Some(prefix) = self.prefix {
            slot_wrap!(TextFieldSlotRole::Prefix, prefix);
        }
        // 输入子节点（原 leaf 逻辑）——显式 ctx.key 稳定（show_placeholder
        // 切换不平移 key 流——输入节点/光标状态不因结构变化重建）
        ctx.key(TextFieldSlotRole::Input, |ctx| {
        let input_key = ctx.next_key();
        ctx.start_leaf(input_key, input_modifier);

        // 注册本段（显示文本——reg 空间 = 显示；content 变化时 register
        // 同步更新文本/长度）。
        // ⚠ 必须用**输入 leaf 的 slot_key**（input_key）而非容器 key：渲染
        // 高亮（selected_range(node.slot_key)）与拖动定位
        // （segment_info(node.slot_key)）都按 leaf 的 slot_key 查询——
        // 注册在容器 key 上则查不到 → 选区永不显示/拖动无法开始
        registrar.register(input_key, &transformed.text);


        // 设置光标位置和回调（desc 通道——组合期捕获，物化时应用到节点）
        ctx.set_current_node_cursor_and_callback(
            current.selection.start,
            cursor_visible_now,
            Box::new({
                let v = value.clone();
                let blink_reset = blink_reset.clone();
                move |idx| {
                    blink_reset(); // 点击定位：光标立即可见 + 计时重置
                    v.update(|val| { val.selection.start = idx; val.selection.end = idx; });
                }
            }),
        );
        // 显示聚焦标记：焦点在容器（交互移容器后本节点 focused=false）——
        // 渲染端用此标记画光标/选区（组合期聚焦状态经 interaction 源可得）
        ctx.set_current_node_display_focused(focused);
        // 注册到节点（app.rs 拖动选区定位依赖 node.registrar）
        ctx.set_current_node_registrar(registrar.clone());
        // 焦点环颜色：主题 primary（组合期捕获——渲染期 CompositionLocal 已退出）
        ctx.set_current_node_focus_color(crate::theme::WiniaTheme::colors().primary);
        // IME 组合下划线颜色：主题 primary（组合期捕获——Phase 4.2：render 阶段
        // 不能读 CompositionLocal，否则自定义主题下下划线用默认色）
        ctx.set_current_node_composing_color(crate::theme::WiniaTheme::colors().primary);
        // IME 预输入回调（旧版风格——直接修改 text 内容）
        {
            let v = value.clone();
            let registrar = registrar.clone();
            let mapping = offset_mapping.clone();
            let undo = undo.clone();
            ctx.set_current_node_ime_callback(Box::new(move |text, cursor| {
                let mut val = v.get();
                // 首次 Preedit（进入新组合，此前无 composing）：删用户选区
                // （替换语义——选中文本输入拼音时立即移除，Compose 行为）；
                // 组合更新（已有 composing）不删（组合文本替换自身）
                let first_preedit = val.composing_range.is_none();
                // §5.2：首次 Preedit 进入新组合——**修改前**快照（含当前 composing
                // =None 状态），使 Ctrl+Z 能回退到组合输入前。组合更新不 push
                // （同文本合并可吸收光标变化——避免组合期间每帧产生 undo 条目）。
                if first_preedit {
                    undo.lock().push(&val.text, &val.selection, val.composing_range.clone());
                }
                // 删除旧的 composing range（组合更新——收拢 selection，防越界）
                end_composition(&mut val);
                if first_preedit && val.selection.start != val.selection.end {
                    let (s, e) = (
                        val.selection.start.min(val.selection.end),
                        val.selection.start.max(val.selection.end),
                    );
                    let len = val.text.len();
                    let (s, e) = (s.min(len), e.min(len));
                    if s < e {
                        val.text.replace_range(s..e, "");
                        val.selection = s..s;
                    } else {
                        val.selection = s.min(len)..s.min(len);
                    }
                }
                // 插入新的预输入文本
                if !text.is_empty() {
                    // P2（review）：pos 来自 selection——undo 恢复/registrar 拉回可能
                    // 落在非字符边界，insert_str 前对齐（防 panic）
                    let pos = val.text.floor_char_boundary(val.selection.start.min(val.text.len()));
                    val.text.insert_str(pos, text);
                    let new_len = text.len();
                    val.composing_range = Some(pos..(pos + new_len));
                    // 组合内光标：**恒单点**（start == end）——IME 的 cursor
                    // 可能是组合内选中范围（拼音候选态），若采用则 selection
                    // 非零宽 → 渲染隐藏光标、且删除/替换语义混乱。Compose 中
                    // 组合文本的选中态由 composing underline 表达
                    let caret = if let Some((start, end)) = cursor {
                        // §5.3：IME cursor 是**字节索引**（winit 契约），可能落在
                        // 多字节字符（CJK/emoji）中间——对齐 char 边界防止 selection
                        // 落在非字符边界（后续 replace/drain/光标渲染错乱）
                        val.text.floor_char_boundary((pos + start.max(end)).min(val.text.len()))
                    } else {
                        pos + new_len
                    };
                    val.selection = caret..caret;
                } else {
                    val.composing_range = None;
                }
                // 同步 reg（防 build 的 reg_leads 用旧选区拉回——Preedit
                // 删选区/组合后 reg 残留旧选区会导致组合期间误删；
                // value（编辑偏移）→ reg（显示偏移））
                registrar.set_selection(
                    mapping.original_to_transformed(val.selection.start),
                    mapping.original_to_transformed(val.selection.end),
                );
                v.set(val);
            }));
        }
        // 同步 composing_range 到节点（渲染画下划线用）
        ctx.sync_composing_range(current.composing_range.clone());
        // 选区双向同步：
        // - 拖动中 reg 领先（app.rs compute_selection 直写 reg，value 未更新）——
        //   重组（闪烁翻转/其他）时若 reg ≠ value 则把 reg 拉回 value，
        //   否则 build 用旧 value 覆盖 reg → 拖动选区随光标闪烁被重置
        // - 其余情况（点击/键盘/外部）value → reg（渲染 registrar 高亮）
        let reg_sel = registrar.selected_range(input_key);
        let val_nonzero = current.selection.start != current.selection.end;
        let (vs, ve) = (
            current.selection.start.min(current.selection.end),
            current.selection.start.max(current.selection.end),
        );
        // reg 空间 = 显示偏移——与 value（编辑偏移）比较/转换须经映射
        let (vts, vte) = (
            offset_mapping.original_to_transformed(vs),
            offset_mapping.original_to_transformed(ve),
        );
        // reg 有真实选区（非零宽）时领先——包括 value 仍是单点（拖动中
        // 点击定位后 value 未更新）：此时必须拉回 value，否则 `_` 分支
        // 会用单点覆盖拖动选区（选区随闪烁翻转消失）
        let reg_leads = match &reg_sel {
            Some(r) if r.start < r.end => match val_nonzero {
                true => (r.start, r.end) != (vts, vte),
                false => true,
            },
            _ => false,
        };
        if reg_leads {
            if let Some(r) = reg_sel {
                let (s, e) = (r.start, r.end);
                value.update(|val| {
                    let len = val.text.len();
                    let s = offset_mapping.transformed_to_original(s).min(len);
                    let e = offset_mapping.transformed_to_original(e).min(len);
                    val.selection = s..e;
                });
            }
        } else {
            registrar.set_selection(vts, vte);
        }
        // 输入节点结束
        ctx.end_node();
        });
        // 后缀 / 后置图标（右对齐）
        if let Some(suffix) = self.suffix {
            slot_wrap!(TextFieldSlotRole::Suffix, suffix);
        }
        if let Some(icon) = self.trailing_icon {
            slot_wrap!(TextFieldSlotRole::Trailing, icon);
        }
        // 容器组结束
        ctx.end_restartable_group();
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
        Self::new(State::new(TextFieldValue::new("")))
    }
}


#[cfg(test)]
mod tests;
