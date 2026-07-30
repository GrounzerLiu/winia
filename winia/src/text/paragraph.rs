//! Paragraph — 封装 Skia Paragraph，集成 UTF-8 ↔ UTF-16 索引映射和内联元素

use super::index_bimap::IndexBiMap;
use super::inline_drawable::InlineDrawable;
use skia_safe::textlayout::paragraph::{GlyphClusterInfo, Paragraph as SkParagraph};
use skia_safe::textlayout::{Affinity, LineMetrics, RectHeightStyle, RectWidthStyle, TextBox};
use skia_safe::{scalar, Canvas, Point};
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

/// 自定义 Paragraph 封装。
///
/// 在 Skia Paragraph 之上叠加索引映射层，解决 UTF-8(Rust) ↔ UTF-16(Skia) 差异。
/// 同时支持内联 drawable（图片/SVG）的存储与绘制。
pub struct Paragraph {
    paragraph: SkParagraph,
    drawables: Vec<Arc<dyn InlineDrawable>>,
    line_breaks: HashSet<Range<usize>>,
    pub(crate) paragraph_byte_to_real_indices: IndexBiMap,
    pub(crate) byte_to_utf16_indices: IndexBiMap,
}

impl Paragraph {
    pub(crate) fn new(
        paragraph: SkParagraph,
        drawables: &[Arc<dyn InlineDrawable>],
        line_breaks: &HashSet<Range<usize>>,
        paragraph_byte_to_real_indices: &IndexBiMap,
        byte_to_utf16_indices: &IndexBiMap,
    ) -> Self {
        Self {
            paragraph,
            drawables: drawables.to_vec(),
            line_breaks: line_breaks.clone(),
            paragraph_byte_to_real_indices: paragraph_byte_to_real_indices.clone(),
            byte_to_utf16_indices: byte_to_utf16_indices.clone(),
        }
    }

    pub fn inner_paragraph(&self) -> &SkParagraph {
        &self.paragraph
    }

    /// 获取 Skia 内部的 UTF-16 索引
    fn get_utf16_index(&self, real_index: usize) -> Option<usize> {
        let byte_index = self.paragraph_byte_to_real_indices.get_by_right(&real_index)?;
        self.byte_to_utf16_indices.get_by_left(byte_index).copied()
    }

    /// 将 UTF-16 索引转换为 real (Rust) 索引
    fn real_from_utf16(&self, utf16_index: usize) -> usize {
        let byte_index = self.byte_to_utf16_indices.get_by_right(&utf16_index);
        if let Some(&bi) = byte_index {
            self.paragraph_byte_to_real_indices.get_by_left(&bi).copied().unwrap_or(0)
        } else {
            0
        }
    }

    pub fn is_line_break(&self, range: Range<usize>) -> bool {
        self.line_breaks.contains(&range)
    }

    /// 前一个 glyph 的 real byte index
    pub fn prev_glyph_byte_index(&self, index: usize) -> Option<usize> {
        let byte_index = self.paragraph_byte_to_real_indices.get_by_right(&index)?;
        let prev_real = self.paragraph_byte_to_real_indices.left_keys()
            .iter()
            .rev()
            .find(|&&b| b < *byte_index)?;
        self.paragraph_byte_to_real_indices.get_by_left(prev_real).copied()
    }

    pub fn max_width(&self) -> scalar {
        self.paragraph.max_width()
    }

    pub fn height(&self) -> scalar {
        self.paragraph.height()
    }

    pub fn max_intrinsic_width(&self) -> scalar {
        self.paragraph.max_intrinsic_width()
    }

    pub fn layout(&mut self, width: scalar) {
        self.paragraph.layout(width);
    }

    /// 绘制文本内容及内联 drawable。
    ///
    /// 先绘制 Skia Paragraph（含文本），再遍历 placeholder rectangles，
    /// 在对应位置绘制每个内联元素。
    pub fn paint(&self, canvas: &Canvas, x: f32, y: f32) {
        self.paragraph.paint(canvas, (x, y));

        // 绘制内联 drawable（图片/SVG）
        for (i, text_box) in self.paragraph.get_rects_for_placeholders().iter().enumerate() {
            if let Some(drawable) = self.drawables.get(i) {
                drawable.draw(canvas, x + text_box.rect.left, y + text_box.rect.top);
            }
        }
    }

    /// 获取指定范围的选中区域矩形
    pub fn get_rects_for_range(&self, range: Range<usize>, rect_height_style: RectHeightStyle, rect_width_style: RectWidthStyle) -> Vec<TextBox> {
        let start = self.get_utf16_index(range.start).unwrap_or(0);
        let end = self.get_utf16_index(range.end).unwrap_or(0);
        self.paragraph.get_rects_for_range(start..end, rect_height_style, rect_width_style)
    }

    pub fn get_rects_for_placeholders(&self) -> Vec<TextBox> {
        self.paragraph.get_rects_for_placeholders()
    }

    /// 通过坐标命中测试，返回 real index
    pub fn get_glyph_position_at_coordinate(&self, p: impl Into<Point>) -> (usize, Affinity) {
        let p_with_a = self.paragraph.get_glyph_position_at_coordinate(p);
        (self.real_from_utf16(p_with_a.position as usize), p_with_a.affinity)
    }

    /// 获取 word boundary
    pub fn get_word_boundary(&self, offset: usize) -> Range<usize> {
        let utf16_off = self.get_utf16_index(offset).unwrap_or(0) as u32;
        let range = self.paragraph.get_word_boundary(utf16_off);
        self.real_from_utf16(range.start)..self.real_from_utf16(range.end)
    }

    pub fn get_line_metrics(&self) -> Vec<LineMetrics<'_>> {
        self.paragraph.get_line_metrics()
    }

    pub fn get_line_metrics_at(&self, line_number: usize) -> Option<LineMetrics<'_>> {
        self.paragraph.get_line_metrics_at(line_number)
    }

    /// 获取指定 index 处的 glyph cluster 信息
    pub fn get_glyph_cluster_at(&self, index: usize) -> Option<GlyphClusterInfo> {
        let utf16_idx = self.get_utf16_index(index)?;
        let mut info = self.paragraph.get_glyph_cluster_at(utf16_idx)?;
        info.text_range.start = self.real_from_utf16(info.text_range.start);
        info.text_range.end = self.real_from_utf16(info.text_range.end);
        Some(info)
    }

    /// 通过坐标获取最近的 glyph cluster
    pub fn get_closest_glyph_cluster_at(&self, d: impl Into<Point>) -> Option<GlyphClusterInfo> {
        let mut info = self.paragraph.get_closest_glyph_cluster_at(d)?;
        info.text_range.start = self.real_from_utf16(info.text_range.start);
        info.text_range.end = self.real_from_utf16(info.text_range.end);
        Some(info)
    }

    pub fn get_line_number_at(&self, index: usize) -> Option<usize> {
        if let Some(utf16_idx) = self.get_utf16_index(index) {
            self.paragraph.get_line_number_at(utf16_idx)
        } else {
            None
        }
    }
}