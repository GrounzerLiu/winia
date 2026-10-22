// ═══════════════════════════════════════════════════════════
// 词边界（对齐 Compose TextFieldPreparedSelection 的 getWordStart/
// getWordEnd 语义——词字符 = letterOrDigit，非 UAX#29 词规则：
// 后者把 "a.b" 当一词，Compose 标点即边界）
// ═══════════════════════════════════════════════════════════

/// 词字符（对标 Compose `Character.isLetterOrDigit`）
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// 前一个词的起始字节（Compose getWordStart：
/// 先跳过 pos 前的非词字符，再跳过词字符到词首）
pub(crate) fn word_prev(text: &str, pos: usize) -> usize {
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
pub(crate) fn word_next(text: &str, pos: usize) -> usize {
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
pub(crate) fn line_start(text: &str, pos: usize) -> usize {
    let bytes = text.as_bytes();
    let pos = pos.min(bytes.len());
    bytes[..pos]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// 行尾（不含换行；pos 在行末换行符前——对齐 Compose LINE_END 语义）
pub(crate) fn line_end(text: &str, pos: usize) -> usize {
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

pub(crate) fn clipboard_get_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

pub(crate) fn clipboard_set_text(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text.to_string());
    }
}

