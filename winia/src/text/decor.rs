//! Text decoration and font rendering options, and the resolved per-span style rich text
//! carries.

use crate::graphics::Color;

/// 装饰线样式（对应 Skia TextDecorationStyle）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoStyle { Solid, Double, Dotted, Dashed, Wavy }

/// 装饰线模式（对应 Skia TextDecorationMode）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoMode { Gaps, Through }

/// 字体渲染边缘
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontEdge { Alias, AntiAlias, SubpixelAntiAlias }

/// 字体提示
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontHint { None, Slight, Normal, Full }

/// 富文本中每段的已解析样式（含范围）。
/// 存储在 RichTextContent modifier 中供测量/渲染使用。
#[derive(Debug, Clone, PartialEq)]
pub struct RichSpanStyle {
    /// 范围起（字符索引，含）
    pub start: usize,
    /// 范围止（字符索引，不含）
    pub end: usize,
    pub font_size: f32,
    pub color: Color,
    pub font_weight: crate::text::FontWeight,
    pub font_style: crate::text::FontSlant,
    // ── 装饰线 ──
    pub underline: bool,
    pub overline: bool,
    pub strikethrough: bool,
    pub decoration_color: Option<Color>,
    pub decoration_style: Option<DecoStyle>,
    pub decoration_mode: Option<DecoMode>,
    // ── 基线 ──
    pub baseline_shift: f32,
    // ── 间距 ──
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub height_multiple: f32,
    pub half_leading: bool,
    // ── 字体 ──
    pub font_families: Vec<String>,
    pub font_width: i32,
    pub font_edging: Option<FontEdge>,
    pub font_hinting: Option<FontHint>,
    pub subpixel: bool,
    // ── 前景/背景 ──
    pub foreground_color: Option<Color>,
    pub background: Option<Color>,
    // ── 其他 ──
    pub locale: Option<String>,
}
