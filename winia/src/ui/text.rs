//! Text 组件 — 文本显示
//!
//! 用法:
//! ```ignore
//! Text::new("hello")
//!     .font_size(16.0)
//!     .color(Color::BLACK)
//!     .build(ctx);
//! ```

use crate::core::composer::ComposeCtx;
use crate::modifier::{Color, Modifier, ModifierElement};

/// 文本对齐方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

impl Default for TextAlign {
    fn default() -> Self {
        TextAlign::Left
    }
}

/// 文本溢出处理
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
    Fade,
}

impl Default for TextOverflow {
    fn default() -> Self {
        TextOverflow::Clip
    }
}

/// Text 组件 Builder
///
/// 声明式文本显示组件。可通过链式方法配置字体大小、颜色、对齐等。
///
/// # 示例
/// ```ignore
/// Text::new("Hello World")
///     .font_size(24.0)
///     .color(Color::RED)
///     .align(TextAlign::Center)
///     .max_lines(2)
///     .overflow(TextOverflow::Ellipsis)
///     .modifier(Modifier::new().padding(8.0))
///     .build(ctx);
/// ```
#[derive(Debug, Clone)]
pub struct Text {
    /// 文本内容
    content: String,
    /// 修饰符链
    modifier: Modifier,
    /// 字体大小（逻辑像素）
    font_size: f32,
    /// 文本颜色
    color: Color,
    /// 最大行数（超出按 overflow 处理）
    max_lines: usize,
    /// 文本对齐
    text_align: TextAlign,
    /// 溢出处理方式
    overflow: TextOverflow,
}

impl Text {
    /// 创建新的 Text 组件
    pub fn new(content: impl Into<String>) -> Self {
        Text {
            content: content.into(),
            modifier: Modifier::new(),
            font_size: 14.0,
            color: Color::BLACK,
            max_lines: usize::MAX,
            text_align: TextAlign::default(),
            overflow: TextOverflow::default(),
        }
    }

    /// 设置修饰符链
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// 设置字体大小
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// 设置文本颜色
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// 设置最大行数
    pub fn max_lines(mut self, lines: usize) -> Self {
        self.max_lines = lines;
        self
    }

    /// 设置文本对齐方式
    pub fn align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    /// 设置溢出处理方式
    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// 注册到组合树。
    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();

        // 将文本内容附加到 modifier（每次组合都使用最新的 self.content）
        let modifier = self.modifier.push(ModifierElement::TextContent {
            content: self.content,
            font_size: self.font_size,
            color: self.color,
        });

        // 注册为叶子布局节点
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }

    // ── Getters（测试用）──
    pub fn get_content(&self) -> &str {
        &self.content
    }
    pub fn get_font_size(&self) -> f32 {
        self.font_size
    }
    pub fn get_color(&self) -> Color {
        self.color
    }
    pub fn get_text_align(&self) -> TextAlign {
        self.text_align
    }
    pub fn get_overflow(&self) -> TextOverflow {
        self.overflow
    }
    pub fn get_max_lines(&self) -> usize {
        self.max_lines
    }
    pub fn get_modifier(&self) -> &Modifier {
        &self.modifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_defaults() {
        let text = Text::new("hello");
        assert_eq!(text.get_content(), "hello");
        assert_eq!(text.get_font_size(), 14.0);
        assert_eq!(text.get_color(), Color::BLACK);
        assert_eq!(text.get_text_align(), TextAlign::Left);
        assert_eq!(text.get_max_lines(), usize::MAX);
    }

    #[test]
    fn test_text_builder() {
        let text = Text::new("hello world")
            .font_size(24.0)
            .color(Color::RED)
            .align(TextAlign::Center)
            .max_lines(3)
            .overflow(TextOverflow::Ellipsis)
            .modifier(Modifier::new().padding(8.0));

        assert_eq!(text.get_content(), "hello world");
        assert_eq!(text.get_font_size(), 24.0);
        assert_eq!(text.get_color(), Color::RED);
        assert_eq!(text.get_text_align(), TextAlign::Center);
        assert_eq!(text.get_overflow(), TextOverflow::Ellipsis);
        assert_eq!(text.get_max_lines(), 3);
        assert_eq!(text.get_modifier().elements().len(), 1);
    }
}
