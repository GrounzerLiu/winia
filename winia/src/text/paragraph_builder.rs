//! ParagraphBuilder — 构建自定义 Paragraph，同时构建 UTF-8 ↔ UTF-16 索引映射

use super::index_bimap::IndexBiMap;
use super::paragraph::Paragraph;
use skia_safe::textlayout::{FontCollection, ParagraphBuilder as SkParagraphBuilder, ParagraphStyle, TextStyle};
use std::collections::HashSet;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// 自定义 Paragraph 构建器。
///
/// 在调用 Skia 的 ParagraphBuilder 添加文本的同时，
/// 同步构建 `paragraph_byte_to_real_indices` 和 `byte_to_utf16_indices` 映射表，
/// 供后续 TextLayout 中的光标定位和命中测试使用。
pub struct ParagraphBuilder {
    paragraph_builder: SkParagraphBuilder,
    last_byte_index: usize,
    last_real_index: usize,
    last_utf16_index: usize,
    line_breaks: HashSet<Range<usize>>,
    paragraph_byte_to_real_indices: IndexBiMap,
    byte_to_utf16_indices: IndexBiMap,
}

impl ParagraphBuilder {
    pub fn new(style: &ParagraphStyle, font_collection: impl Into<FontCollection>) -> Self {
        let paragraph_builder = SkParagraphBuilder::new(style, font_collection);
        ParagraphBuilder {
            paragraph_builder,
            last_byte_index: 0,
            last_real_index: 0,
            last_utf16_index: 0,
            line_breaks: HashSet::new(),
            paragraph_byte_to_real_indices: IndexBiMap::new(),
            byte_to_utf16_indices: IndexBiMap::new(),
        }
    }

    pub fn push_style(&mut self, style: &TextStyle) -> &mut Self {
        self.paragraph_builder.push_style(style);
        self
    }

    pub fn pop(&mut self) -> &mut Self {
        self.paragraph_builder.pop();
        self
    }

    /// 添加文本，同时构建索引映射。
    ///
    /// 以 grapheme cluster 为单位遍历，对每个字符记录：
    /// - `paragraph_byte_to_real_indices`: Skia(UTF-16) 字节位置 → Rust(UTF-8) 字节位置
    /// - `byte_to_utf16_indices`: Rust 字节位置 → UTF-16 单元位置
    pub fn add_text(&mut self, str: impl AsRef<str>) {
        let str = str.as_ref();
        if str.is_empty() {
            return;
        }

        let mut last_byte_index = 0;
        let mut last_real_index = 0;
        let mut last_utf16_index = 0;

        // 遍历 grapheme cluster（用户感知的字符单元）
        str.grapheme_indices(true).for_each(|(byte_offset, grapheme)| {
            // 记录换行符位置
            if grapheme == "\r\n" || grapheme == "\n" || grapheme == "\r" {
                let index = self.last_real_index + byte_offset;
                self.line_breaks.insert(index..index + grapheme.len());
            }

            // 遍历 grapheme 中的每个 Unicode 标量值（char）
            grapheme.char_indices().for_each(|(char_offset, ch)| {
                let m_byte_index = self.last_byte_index + byte_offset + char_offset;
                let real_index = self.last_real_index + byte_offset + char_offset;

                self.byte_to_utf16_indices.insert(m_byte_index, self.last_utf16_index + last_utf16_index);
                self.paragraph_byte_to_real_indices.insert(m_byte_index, real_index);

                last_utf16_index += ch.len_utf16();
                last_byte_index = m_byte_index + ch.len_utf8();
                last_real_index = real_index + ch.len_utf8();
            });
        });

        self.last_real_index += last_real_index;
        self.last_byte_index += last_byte_index;
        self.last_utf16_index += last_utf16_index;

        self.paragraph_builder.add_text(str);
    }

    /// 构建 Paragraph。在返回前插入最后一个终止位置的映射。
    pub fn build(&mut self) -> Paragraph {
        // 插入终止位置映射（用于表示文本结束）
        self.paragraph_byte_to_real_indices.insert(self.last_byte_index, self.last_real_index);
        self.byte_to_utf16_indices.insert(self.last_byte_index, self.last_utf16_index);

        let paragraph = self.paragraph_builder.build();
        Paragraph::new(
            paragraph,
            &self.line_breaks,
            &self.paragraph_byte_to_real_indices,
            &self.byte_to_utf16_indices,
        )
    }

    pub fn reset(&mut self) {
        self.paragraph_builder.reset();
        self.last_byte_index = 0;
        self.last_real_index = 0;
        self.last_utf16_index = 0;
        self.line_breaks.clear();
        self.paragraph_byte_to_real_indices.clear();
        self.byte_to_utf16_indices.clear();
    }
}

