//! The text types — Compose's `androidx.compose.ui.text` vocabulary.
//!
//! These are VALUES the toolkit and every component share: a font weight, a slant, an alignment, an
//! overflow policy and the style bundle they travel in. They used to live in `ui/text.rs` next to the
//! `Text` component, which made the layout engine depend on a component to name a font weight (see
//! `layout/node.rs`). They belong with the rest of the text stack.
//!
//! The `Text` component re-exports nothing: it imports from here.

use crate::graphics::Color;
use crate::unit::TextUnit;

// ═══════════════════════════════════════════════════════════
// 字体属性类型
// ═══════════════════════════════════════════════════════════

/// 字重 — 对标 Skia FontStyle::Weight
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontWeight(i32);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100);
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200);
    pub const LIGHT: FontWeight = FontWeight(300);
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const SEMI_BOLD: FontWeight = FontWeight(600);
    pub const BOLD: FontWeight = FontWeight(700);
    pub const EXTRA_BOLD: FontWeight = FontWeight(800);
    pub const BLACK: FontWeight = FontWeight(900);

    pub fn new(weight: i32) -> Self { FontWeight(weight.clamp(1, 1000)) }
    pub fn value(self) -> i32 { self.0 }
}

impl Default for FontWeight { fn default() -> Self { FontWeight::NORMAL } }

/// 字体倾斜
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSlant { Upright, Italic, Oblique }
impl Default for FontSlant { fn default() -> Self { FontSlant::Upright } }

// ═══════════════════════════════════════════════════════════
// 文本样式
// ═══════════════════════════════════════════════════════════

/// 文本对齐方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign { Left, Center, Right, Justify }
impl Default for TextAlign { fn default() -> Self { TextAlign::Left } }

/// Compose's `TextDecoration`: a set of line decorations, combinable with `|` (`+` in Kotlin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextDecoration {
    pub underline: bool,
    pub line_through: bool,
}

impl TextDecoration {
    pub const NONE: Self = Self { underline: false, line_through: false };
    pub const UNDERLINE: Self = Self { underline: true, line_through: false };
    pub const LINE_THROUGH: Self = Self { underline: false, line_through: true };
    pub const UNDERLINE_AND_LINE_THROUGH: Self = Self { underline: true, line_through: true };
}

impl std::ops::BitOr for TextDecoration {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self {
            underline: self.underline || other.underline,
            line_through: self.line_through || other.line_through,
        }
    }
}

/// 文本溢出处理
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    /// Cut off at the node's own box.
    Clip,
    /// Cut off, with an ellipsis.
    Ellipsis,
    /// NOT cut off: the glyphs are laid out at their intrinsic width and painted past the node's
    /// box (an ancestor that clips, such as a Button's shape, still clips them). This is Compose's
    /// `TextOverflow.Visible`, and it is what material3's button-group item uses for its label
    /// (`ButtonGroup.kt:1118-1124`) so that a press narrowing an item cannot cut the word it shows.
    Visible,
}
impl Default for TextOverflow { fn default() -> Self { TextOverflow::Clip } }

/// 文本样式——对标 Compose TextStyle
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub color: Option<Color>,
    pub font_size: Option<TextUnit>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontSlant>,
    pub text_align: Option<TextAlign>,
    pub overflow: Option<TextOverflow>,
    pub max_lines: Option<usize>,
    pub soft_wrap: Option<bool>,
    pub letter_spacing: Option<f32>,
    pub line_height: Option<TextUnit>,
    /// The minimum number of lines the box reserves — Compose's `TextStyle.minLines`.
    pub min_lines: Option<usize>,
    /// A font family name resolved against the platform's font collection — Compose's
    /// `TextStyle.fontFamily`, as a name here (winia has no `FontFamily` resolver type yet).
    pub font_family: Option<String>,
    /// 下划线（仅 RichText 生效）
    pub underline: bool,
    /// 删除线（仅 RichText 生效）
    pub strikethrough: bool,
    /// 背景色（仅 RichText 生效）
    pub background: Option<Color>,
}

impl TextStyle {
    pub fn new() -> Self {
        Self { color: None, font_size: None, font_weight: None, font_style: None, text_align: None, overflow: None, max_lines: None, soft_wrap: None, letter_spacing: None, line_height: None, min_lines: None, font_family: None, underline: false, strikethrough: false, background: None }
    }

    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn font_size(mut self, s: impl Into<TextUnit>) -> Self { self.font_size = Some(s.into()); self }
    pub fn font_weight(mut self, w: FontWeight) -> Self { self.font_weight = Some(w); self }
    pub fn font_style(mut self, s: FontSlant) -> Self { self.font_style = Some(s); self }
    pub fn italic(mut self) -> Self { self.font_style = Some(FontSlant::Italic); self }
    pub fn bold(mut self) -> Self { self.font_weight = Some(FontWeight::BOLD); self }
    pub fn oblique(mut self) -> Self { self.font_style = Some(FontSlant::Oblique); self }
    pub fn align(mut self, a: TextAlign) -> Self { self.text_align = Some(a); self }
    pub fn overflow(mut self, overflow: TextOverflow) -> Self { self.overflow = Some(overflow); self }
    pub fn max_lines(mut self, lines: usize) -> Self { self.max_lines = Some(lines); self }
    pub fn soft_wrap(mut self, wrap: bool) -> Self { self.soft_wrap = Some(wrap); self }
    pub fn letter_spacing(mut self, spacing: f32) -> Self { self.letter_spacing = Some(spacing); self }
    /// The minimum number of lines the box reserves — Compose's `TextStyle.minLines`.
    pub fn min_lines(mut self, lines: usize) -> Self { self.min_lines = Some(lines); self }
    /// A font family name, resolved against the platform's font collection.
    pub fn font_family(mut self, family: impl Into<String>) -> Self { self.font_family = Some(family.into()); self }
    /// Compose's `TextStyle.textDecoration`; it drives the `underline` / `strikethrough` flags.
    pub fn text_decoration(mut self, decoration: TextDecoration) -> Self {
        self.underline = decoration.underline;
        self.strikethrough = decoration.line_through;
        self
    }
    /// The decoration these flags describe.
    pub fn decoration(&self) -> TextDecoration {
        TextDecoration { underline: self.underline, line_through: self.strikethrough }
    }
    pub fn line_height(mut self, height: impl Into<TextUnit>) -> Self { self.line_height = Some(height.into()); self }
    pub fn underline(mut self) -> Self { self.underline = true; self }
    pub fn strikethrough(mut self) -> Self { self.strikethrough = true; self }
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
}

impl Default for TextStyle {
    fn default() -> Self { Self::new() }
}
