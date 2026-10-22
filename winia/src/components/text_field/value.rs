use super::*;

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
pub(crate) struct UndoManager {
    pub(crate) undo_stack: Vec<(String, Range<usize>, Option<Range<usize>>)>,
    pub(crate) redo_stack: Vec<(String, Range<usize>, Option<Range<usize>>)>,
}

pub(crate) const UNDO_MAX_SNAPSHOTS: usize = 100;

impl UndoManager {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 编辑前调用：记录 (text, selection, composing_range) 快照；新编辑丢弃 redo 分支
    pub(crate) fn push(&mut self, text: &str, selection: &Range<usize>, composing: Option<Range<usize>>) {
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
    pub(crate) fn undo(&mut self, current: (String, Range<usize>, Option<Range<usize>>)) -> Option<(String, Range<usize>, Option<Range<usize>>)> {
        let s = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(s)
    }

    /// 重做：当前状态入 undo 栈，返回 redo 快照
    pub(crate) fn redo(&mut self, current: (String, Range<usize>, Option<Range<usize>>)) -> Option<(String, Range<usize>, Option<Range<usize>>)> {
        let s = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(s)
    }
}

/// 结束组合（删除组合文本并收拢 selection——对齐 Compose
/// `FinishComposingTextCommand`：键盘编辑前先移除组合文本，否则
/// 拼音 Commit 时 preedit 文本残留在正文中）。clamp 防删组合后越界。
pub(crate) fn end_composition(val: &mut TextFieldValue) {
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

