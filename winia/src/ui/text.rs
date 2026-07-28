//! Text 组件 — 对齐 Compose Material3 Text
//!
//! - 颜色优先级: .color() > style.color > LocalTextStyle > WiniaTheme on_surface
//! - ProvideTextStyle 为子树设置默认文字样式
//! - 单独参数（font_size 等）优先级高于 style 参数

use crate::core::composer::ComposeCtx;
use crate::core::composition_local::CompositionLocal;
use crate::modifier::{Color, Modifier, ModifierElement};
use std::sync::LazyLock;

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

/// 文本溢出处理
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow { Clip, Ellipsis, Fade }
impl Default for TextOverflow { fn default() -> Self { TextOverflow::Clip } }

/// 文本样式——对标 Compose TextStyle
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontSlant>,
    pub text_align: Option<TextAlign>,
    pub overflow: Option<TextOverflow>,
    pub max_lines: Option<usize>,
}

impl TextStyle {
    pub fn new() -> Self {
        Self { color: None, font_size: None, font_weight: None, font_style: None, text_align: None, overflow: None, max_lines: None }
    }

    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = Some(s); self }
    pub fn font_weight(mut self, w: FontWeight) -> Self { self.font_weight = Some(w); self }
    pub fn font_style(mut self, s: FontSlant) -> Self { self.font_style = Some(s); self }
    pub fn italic(mut self) -> Self { self.font_style = Some(FontSlant::Italic); self }
    pub fn bold(mut self) -> Self { self.font_weight = Some(FontWeight::BOLD); self }
    pub fn oblique(mut self) -> Self { self.font_style = Some(FontSlant::Oblique); self }
    pub fn align(mut self, a: TextAlign) -> Self { self.text_align = Some(a); self }
    pub fn overflow(mut self, overflow: TextOverflow) -> Self { self.overflow = Some(overflow); self }
    pub fn max_lines(mut self, lines: usize) -> Self { self.max_lines = Some(lines); self }
}

impl Default for TextStyle {
    fn default() -> Self { Self::new() }
}

// ═══════════════════════════════════════════════════════════
// LocalTextStyle —— 子树默认文字样式
// ═══════════════════════════════════════════════════════════

static LOCAL_TEXT_STYLE: LazyLock<CompositionLocal<TextStyle>> = LazyLock::new(|| {
    CompositionLocal::new(|| TextStyle::default())
});

/// 在子树中提供默认文字样式（和现有样式合并，不是替换）。
/// 类似 Compose 的 ProvideTextStyle。
#[allow(non_snake_case)]
pub fn ProvideTextStyle(style: TextStyle, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    let merged = merge_text_styles(&LOCAL_TEXT_STYLE.current(), &style);
    LOCAL_TEXT_STYLE.provides(merged, || {
        content(ctx);
    });
}

/// 合并两个 TextStyle——right 中的 Some 覆盖 left（即 right 优先级更高）
fn merge_text_styles(base: &TextStyle, override_: &TextStyle) -> TextStyle {
    TextStyle {
        color: override_.color.or(base.color),
        font_size: override_.font_size.or(base.font_size),
        font_weight: override_.font_weight.or(base.font_weight),
        font_style: override_.font_style.or(base.font_style),
        text_align: override_.text_align.or(base.text_align),
        overflow: override_.overflow.or(base.overflow),
        max_lines: override_.max_lines.or(base.max_lines),
    }
}

// ═══════════════════════════════════════════════════════════
// Text 组件
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Text {
    content: String,
    modifier: Modifier,
    font_size: Option<f32>,
    color: Option<Color>,
    font_weight: Option<FontWeight>,
    font_style: Option<FontSlant>,
    max_lines: Option<usize>,
    text_align: Option<TextAlign>,
    overflow: Option<TextOverflow>,
    style: Option<TextStyle>,
    soft_wrap: bool,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Text {
            content: content.into(),
            modifier: Modifier::new(),
            font_size: None,
            color: None,
            font_weight: None,
            font_style: None,
            max_lines: None,
            text_align: None,
            overflow: None,
            style: None,
            soft_wrap: true,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self { self.modifier = self.modifier.then(modifier); self }
    pub fn font_size(mut self, size: f32) -> Self { self.font_size = Some(size); self }
    pub fn color(mut self, color: Color) -> Self { self.color = Some(color); self }
    pub fn font_weight(mut self, w: FontWeight) -> Self { self.font_weight = Some(w); self }
    pub fn bold(mut self) -> Self { self.font_weight = Some(FontWeight::BOLD); self }
    pub fn italic(mut self) -> Self { self.font_style = Some(FontSlant::Italic); self }
    pub fn oblique(mut self) -> Self { self.font_style = Some(FontSlant::Oblique); self }
    pub fn max_lines(mut self, lines: usize) -> Self { self.max_lines = Some(lines); self }
    pub fn align(mut self, align: TextAlign) -> Self { self.text_align = Some(align); self }
    pub fn overflow(mut self, overflow: TextOverflow) -> Self { self.overflow = Some(overflow); self }
    pub fn soft_wrap(mut self, wrap: bool) -> Self { self.soft_wrap = wrap; self }

    /// 设置文字样式（单独参数优先级高于此样式）
    pub fn style(mut self, style: TextStyle) -> Self { self.style = Some(style); self }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();

        let base = LOCAL_TEXT_STYLE.current();
        let style = self.style.as_ref().map(|s| merge_text_styles(&base, s)).unwrap_or(base);

        let final_font_size = self.font_size.or(style.font_size).unwrap_or(14.0);
        let final_font_weight = self.font_weight.or(style.font_weight).unwrap_or_default();
        let final_font_style = self.font_style.or(style.font_style).unwrap_or_default();
        let final_align = self.text_align.or(style.text_align).unwrap_or_default();
        let final_overflow = self.overflow.or(style.overflow).unwrap_or_default();
        let final_max_lines = self.max_lines.or(style.max_lines).unwrap_or(usize::MAX);

        let final_color = self.color
            .or(style.color)
            .unwrap_or_else(|| crate::ui::theme::WiniaTheme::colors().on_surface);

        let modifier = self.modifier.push(ModifierElement::TextContent {
            content: self.content,
            font_size: final_font_size,
            color: final_color,
            font_weight: final_font_weight,
            font_style: final_font_style,
            max_lines: final_max_lines,
            align: final_align,
            overflow: final_overflow,
            soft_wrap: self.soft_wrap,
        });

        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }

    // ── Getters ──
    pub fn get_content(&self) -> &str { &self.content }
    pub fn get_font_size(&self) -> Option<f32> { self.font_size }
    pub fn get_color(&self) -> Option<Color> { self.color }
    pub fn get_text_align(&self) -> Option<TextAlign> { self.text_align }
    pub fn get_overflow(&self) -> Option<TextOverflow> { self.overflow }
    pub fn get_max_lines(&self) -> Option<usize> { self.max_lines }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_defaults() {
        let text = Text::new("hello");
        assert_eq!(text.get_content(), "hello");
        assert_eq!(text.get_font_size(), None);
        assert_eq!(text.get_color(), None);
        assert_eq!(text.get_text_align(), None);
        assert_eq!(text.get_max_lines(), None);
    }

    #[test]
    fn test_text_builder() {
        let text = Text::new("hello world")
            .font_size(24.0)
            .color(Color::RED)
            .bold()
            .italic()
            .align(TextAlign::Center)
            .max_lines(3)
            .overflow(TextOverflow::Ellipsis)
            .modifier(Modifier::new().padding(8.0));

        assert_eq!(text.get_content(), "hello world");
        assert_eq!(text.get_font_size(), Some(24.0));
        assert_eq!(text.get_color(), Some(Color::RED));
        assert_eq!(text.get_text_align(), Some(TextAlign::Center));
        assert_eq!(text.get_overflow(), Some(TextOverflow::Ellipsis));
        assert_eq!(text.get_max_lines(), Some(3));
        assert_eq!(text.get_modifier().elements().len(), 1);
    }

    #[test]
    fn test_text_bold_italic() {
        let text = Text::new("bold italic")
            .bold()
            .italic()
            .font_size(16.0);
        assert!(text.font_weight.is_some());
        assert_eq!(text.font_weight.unwrap(), FontWeight::BOLD);
        assert!(text.font_style.is_some());
        assert_eq!(text.font_style.unwrap(), FontSlant::Italic);
    }
}
