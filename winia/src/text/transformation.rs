//! Visual text transformations — Compose's `VisualTransformation` plus `OffsetMapping`.
//!
//! Password masking and formatted input make the DISPLAYED text differ from the EDITED text.
//! Visual coordinates (caret, selection, click targeting, the IME area) are measured in display
//! offsets; editing coordinates (`value`/`text`) are measured in the editing buffer's offsets.
//! Every crossing between the two has to go through an `OffsetMapping`.
//!
//! The rules, matching Compose:
//! - original text -> `filter()` -> display text plus a two-way mapping (rebuilt on every build;
//!   `filter` is a pure function)
//! - `original_to_transformed`: editing offset -> display offset (drawing the caret/selection)
//! - `transformed_to_original`: display offset -> editing offset (click and drag targeting)

use std::sync::Arc;

/// A transformation's result: the display text plus the mapping back and forth.
pub struct TransformedText {
    pub text: String,
    pub offset_mapping: Arc<dyn OffsetMapping>,
}

/// Compose's `VisualTransformation`.
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

/// Compose's `OffsetMapping`: the two directions between editing and display offsets.
pub trait OffsetMapping: Send + Sync + std::fmt::Debug {
    /// Editing offset -> display offset.
    fn original_to_transformed(&self, offset: usize) -> usize;
    /// Display offset -> editing offset.
    fn transformed_to_original(&self, offset: usize) -> usize;
}

// ── Identity (the default: no transformation) ──

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

// ── Password masking: one codepoint becomes one mask character. The BYTE counts differ, so the
//    mapping is per character ──

#[derive(Debug)]
pub struct PasswordTransformation {
    mask: char,
}

impl PasswordTransformation {
    /// `mask` is the mask character (default `'•'` U+2022, as in Compose).
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
    /// The start byte of every character in the original text.
    orig_chars: Vec<usize>,
    orig_len: usize,
    /// How many UTF-8 bytes one mask character occupies.
    mask_len: usize,
}

impl OffsetMapping for PasswordMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        // Original byte -> characters before it -> display byte (each character takes `mask_len`).
        let chars = self.orig_chars.partition_point(|&s| s < offset.min(self.orig_len));
        chars * self.mask_len
    }

    fn transformed_to_original(&self, offset: usize) -> usize {
        // Display byte -> character count -> original byte (the character's start; past the end
        // clamps to the end).
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
        // Original "abc" (3 bytes) -> display "•••" (9 bytes): the mapping is per character.
        let t = PasswordTransformation::new('•').filter("abc");
        assert_eq!(t.text, "•••");
        let m = &*t.offset_mapping;
        // Original bytes 0/1/2/3 map to display 0/3/6/9.
        assert_eq!(m.original_to_transformed(0), 0);
        assert_eq!(m.original_to_transformed(1), 3);
        assert_eq!(m.original_to_transformed(2), 6);
        assert_eq!(m.original_to_transformed(3), 9);
        // And back.
        assert_eq!(m.transformed_to_original(0), 0);
        assert_eq!(m.transformed_to_original(3), 1);
        assert_eq!(m.transformed_to_original(9), 3);
        assert_eq!(m.transformed_to_original(99), 3, "越界钳制到末尾");
    }

    #[test]
    fn password_mapping_multibyte_original() {
        // A CJK original (three bytes per character) through the mask.
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
