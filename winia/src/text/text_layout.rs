//! TextLayout — 文本排版布局，提供光标定位、选中区域、命中测试

use super::Paragraph;
use skia_safe::textlayout::{RectHeightStyle, RectWidthStyle, TextBox, TextDirection};
use skia_safe::{Canvas, Point};
use std::ops::Range;

/// 文本布局封装。
///
/// 基于已排版的 Paragraph 提供：
/// - `get_cursor_position()` — 获取光标位置 (x, y, height)
/// - `get_rects_for_range()` — 获取选中区域矩形
/// - `get_closest_grapheme_cluster_cluster_at()` — 命中测试
pub struct TextLayout<'a> {
    paragraph: &'a Paragraph,
    length: usize,
}

impl<'a> TextLayout<'a> {
    pub(crate) fn new(paragraph: &'a Paragraph, length: usize) -> TextLayout<'a> {
        TextLayout { paragraph, length }
    }

    pub fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        self.paragraph.paint(canvas, x, y);
    }

    pub fn width(&self) -> f32 {
        self.paragraph.max_intrinsic_width()
    }

    pub fn height(&self) -> f32 {
        self.paragraph.height()
    }

    pub fn base_line(&self) -> f32 {
        if let Some(line_metrics) = self.paragraph.get_line_metrics_at(0) {
            line_metrics.baseline as f32
        } else {
            0.0
        }
    }

    /// 获取光标在指定 index 处的 (x, y, height) 坐标。
    /// 兼容空文本（length == 0）情况。
    pub fn get_cursor_position(&self, index: usize) -> Option<(f32, f32, f32)> {
        let para = self.paragraph.inner_paragraph();

        // 空文本：取第一个 glyph cluster 位置（如果有）
        if self.length == 0 {
            if let Some(gc) = para.get_glyph_cluster_at(0) {
                return Some((gc.bounds.left, gc.bounds.top, gc.bounds.height()));
            }
            return None;
        }

        // 通过 paragraph_byte_to_real_indices 找到对应的 Skia 段落字节位置
        let paragraph_byte = self.paragraph.paragraph_byte_to_real_indices.get_by_right(&index)?;
        let glyph_cluster = para.get_glyph_cluster_at(*paragraph_byte)?;

        // 判断光标位置取字符前侧还是后侧
        let prev_line_break = if index == 0 {
            true
        } else {
            let prev = self.paragraph.prev_glyph_byte_index(index);
            match prev {
                Some(prev_idx) => self.paragraph.is_line_break(prev_idx..index),
                None => false,
            }
        };

        if index == 0 || prev_line_break {
            // 行首 → 取字符左侧
            if glyph_cluster.position == TextDirection::LTR {
                Some((glyph_cluster.bounds.left, glyph_cluster.bounds.top, glyph_cluster.bounds.height()))
            } else {
                Some((glyph_cluster.bounds.right, glyph_cluster.bounds.top, glyph_cluster.bounds.height()))
            }
        } else {
            // 非行首 → 取前一个字符右侧
            let prev_byte = self.paragraph.paragraph_byte_to_real_indices.get_by_right(&(index.checked_sub(1)?))?;
            if let Some(prev_gc) = para.get_glyph_cluster_at(*prev_byte) {
                if prev_gc.position == TextDirection::LTR {
                    Some((prev_gc.bounds.right, prev_gc.bounds.top, prev_gc.bounds.height()))
                } else {
                    Some((prev_gc.bounds.left, prev_gc.bounds.top, prev_gc.bounds.height()))
                }
            } else {
                None
            }
        }
    }

    /// 获取指定范围的选中矩形
    pub fn get_rects_for_range(&self, range: Range<usize>) -> Vec<TextBox> {
        self.paragraph.get_rects_for_range(range, RectHeightStyle::Max, RectWidthStyle::Tight)
    }

    /// 通过坐标命中测试，返回最接近的 grapheme cluster 的 real index
    pub fn get_closest_grapheme_cluster_cluster_at(&self, point: impl Into<Point>) -> usize {
        let point = point.into();
        if let Some(glyph_info) = self.paragraph.get_closest_glyph_cluster_at(point) {
            let bounds = glyph_info.bounds;
            let center_x = (bounds.left + bounds.right) / 2.0;

            if self.paragraph.is_line_break(glyph_info.text_range.clone()) {
                return glyph_info.text_range.start;
            }

            // 通过 paragraph_byte_to_real_indices 反向查找 real index
            let start = glyph_info.text_range.start;
            let end = glyph_info.text_range.end;

            if point.x < center_x {
                start
            } else {
                end
            }
        } else {
            0
        }
    }

    pub fn inner_paragraph(&self) -> &Paragraph {
        self.paragraph
    }
}
