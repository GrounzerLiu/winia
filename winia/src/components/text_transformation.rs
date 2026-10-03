//! 文本视觉变换（对标 Compose `VisualTransformation` + `OffsetMapping`）
//!
//! 密码掩码 / 格式化输入：显示文本 ≠ 编辑文本时，**视觉坐标**（光标/选区/
//! 点击定位/IME 区域）用显示文本偏移，**编辑坐标**（value/text）用编辑缓冲
//! 偏移——一切跨界转换必须走 `OffsetMapping`。
//!
//! 原则（对齐 Compose）：
//! - 原始文本 → `filter()` → 显示文本 + 双向映射（每次 build 重建——纯函数）
//! - `original_to_transformed`：编辑偏移 → 显示偏移（光标/选区绘制）
//! - `transformed_to_original`：显示偏移 → 编辑偏移（点击/拖动定位）

use std::sync::Arc;

/// 变换结果：显示文本 + 双向偏移映射
pub struct TransformedText {
    pub text: String,
    pub offset_mapping: Arc<dyn OffsetMapping>,
}

/// 视觉变换（对标 Compose `VisualTransformation`）
pub trait VisualTransformation: Send + Sync + std::fmt::Debug {
    fn filter(&self, text: &str) -> TransformedText;
}

impl From<PasswordTransformation> for Arc<dyn VisualTransformation> {
    fn from(t: PasswordTransformation) -> Self {
        Arc::new(t)
    }
}

impl From<IdentityTransformation> for Arc<dyn VisualTransformation> {
    fn from(t: IdentityTransformation) -> Self {
        Arc::new(t)
    }
}

/// 双向偏移映射（对标 Compose `OffsetMapping`）
pub trait OffsetMapping: Send + Sync + std::fmt::Debug {
    /// 编辑偏移 → 显示偏移
    fn original_to_transformed(&self, offset: usize) -> usize;
    /// 显示偏移 → 编辑偏移
    fn transformed_to_original(&self, offset: usize) -> usize;
}

// ── 恒等（默认——无变换）──

#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityTransformation;

impl IdentityTransformation {
    pub fn new() -> Self {
        Self
    }
}

impl VisualTransformation for IdentityTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        TransformedText {
            text: text.to_string(),
            offset_mapping: Arc::new(IdentityMapping),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityMapping;

impl OffsetMapping for IdentityMapping {
    #[inline]
    fn original_to_transformed(&self, offset: usize) -> usize {
        offset
    }
    #[inline]
    fn transformed_to_original(&self, offset: usize) -> usize {
        offset
    }
}

// ── 密码掩码（1:1 codepoint 替换为掩码字符——字节数可能不同，
//    需要 char 级映射）──

#[derive(Debug)]
pub struct PasswordTransformation {
    mask: char,
}

impl PasswordTransformation {
    /// `mask` 为掩码字符（默认 '•' U+2022——Compose 默认）
    pub fn new(mask: char) -> Self {
        Self { mask }
    }
}

impl Default for PasswordTransformation {
    fn default() -> Self {
        Self::new('•')
    }
}

impl VisualTransformation for PasswordTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        let mask_len = self.mask.len_utf8();
        let orig_chars: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let masked: String = text.chars().map(|_| self.mask).collect();
        TransformedText {
            text: masked,
            offset_mapping: Arc::new(PasswordMapping {
                orig_chars,
                orig_len: text.len(),
                mask_len,
            }),
        }
    }
}

#[derive(Debug)]
struct PasswordMapping {
    /// 原始文本每个 char 的起始字节
    orig_chars: Vec<usize>,
    orig_len: usize,
    /// 掩码字符的 UTF-8 字节数
    mask_len: usize,
}

impl OffsetMapping for PasswordMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        // 原始 byte → 之前 char 数 → 显示 byte（每 char = mask 字节）
        let chars = self.orig_chars.partition_point(|&s| s < offset.min(self.orig_len));
        chars * self.mask_len
    }

    fn transformed_to_original(&self, offset: usize) -> usize {
        // 显示 byte → char 数 → 原始 byte（char 起点；越界钳制到末尾）
        let chars = offset / self.mask_len;
        if chars == 0 {
            0
        } else if chars >= self.orig_chars.len() {
            self.orig_len
        } else {
            self.orig_chars[chars]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_mapping_is_identity() {
        let t = IdentityTransformation::new().filter("hello");
        assert_eq!(t.text, "hello");
        assert_eq!(t.offset_mapping.original_to_transformed(3), 3);
        assert_eq!(t.offset_mapping.transformed_to_original(3), 3);
    }

    #[test]
    fn password_mapping_roundtrip() {
        // 原始 "abc"（3 字节）→ 显示 "•••"（9 字节）——映射 char 级
        let t = PasswordTransformation::new('•').filter("abc");
        assert_eq!(t.text, "•••");
        let m = &*t.offset_mapping;
        // 原始 byte 0/1/2/3 → 显示 0/3/6/9
        assert_eq!(m.original_to_transformed(0), 0);
        assert_eq!(m.original_to_transformed(1), 3);
        assert_eq!(m.original_to_transformed(2), 6);
        assert_eq!(m.original_to_transformed(3), 9);
        // 反向
        assert_eq!(m.transformed_to_original(0), 0);
        assert_eq!(m.transformed_to_original(3), 1);
        assert_eq!(m.transformed_to_original(9), 3);
        assert_eq!(m.transformed_to_original(99), 3, "越界钳制到末尾");
    }

    #[test]
    fn password_mapping_multibyte_original() {
        // 中文原文（每字 3 字节）→ 掩码
        let t = PasswordTransformation::new('*').filter("你好");
        assert_eq!(t.text, "**");
        let m = &*t.offset_mapping;
        assert_eq!(m.original_to_transformed(0), 0);
        assert_eq!(m.original_to_transformed(2), 1, "'你' 内部 byte → 1 个 char");
        assert_eq!(m.original_to_transformed(3), 1);
        assert_eq!(m.original_to_transformed(6), 2);
        assert_eq!(m.transformed_to_original(1), 3);
        assert_eq!(m.transformed_to_original(2), 6);
    }
}
