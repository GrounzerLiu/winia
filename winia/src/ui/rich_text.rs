//! RichText 组件 — 带范围属性注解的内联样式富文本
//!
//! 支持：
//! - 通过 `.with_style(|s| s.bold().font_size(20.0))` 范围化标注样式
//! - 内联图片/SVG 嵌入
//! - 下划线、删除线、背景色等装饰属性
//!
//! # 示例
//! ```ignore
//! RichText::new()
//!     .text("Normal text. ")
//!     .with_style(|s| s.font_size(20.0).bold().color(Color::RED))
//!     .text("Big red bold. ")
//!     .with_style(|s| s.underline())
//!     .text("Underlined. ", &TextStyle::new().font_size(15.0))
//!     .image(checkmark_svg())
//!     .text(" Done!", &TextStyle::new())
//!     .build(ctx);
//! ```

use crate::core::composer::ComposeCtx;
use crate::modifier::{Modifier, ModifierElement, RichSpanStyle, Color};
use crate::text::InlineDrawable;
use crate::ui::text::{FontWeight, FontSlant, TextStyle, LOCAL_TEXT_STYLE};
use crate::ui::theme::WiniaTheme;
use std::sync::Arc;

/// 富文本组件构建器。
///
/// 收集文本段和累加样式标注，构建时合并为不重叠的 `RichSpanStyle` 列表，
/// 通过 `RichTextContent` modifier 传递给测量和渲染系统。
pub struct RichText {
    /// 最终的内容字符序列（drawable 用 U+FFFC 占位）
    content: String,
    /// 内联 drawable 列表
    drawables: Vec<Arc<dyn InlineDrawable>>,
    /// 当前生效的累加样式（尚未 commit 为范围）
    current: StyleAccum,
    /// 已 commit 的范围（含当前范围的起）
    spans: Vec<RichSpanStyle>,
    /// 字符计数器：等于 content.chars().count()
    cursor: usize,
    /// 外部 modifier
    modifier: Modifier,
}

/// 样式的暂存器，用于连续添加多段文本共用同一组属性。
#[derive(Clone, Debug)]
struct StyleAccum {
    font_size: Option<f32>,
    color: Option<Color>,
    font_weight: Option<FontWeight>,
    font_style: Option<FontSlant>,
    underline: bool,
    strikethrough: bool,
    background: Option<Color>,
}

impl Default for StyleAccum {
    fn default() -> Self {
        StyleAccum {
            font_size: None,
            color: None,
            font_weight: None,
            font_style: None,
            underline: false,
            strikethrough: false,
            background: None,
        }
    }
}

impl RichText {
    pub fn new() -> Self {
        RichText {
            content: String::new(),
            drawables: Vec::new(),
            current: StyleAccum::default(),
            spans: Vec::new(),
            cursor: 0,
            modifier: Modifier::new(),
        }
    }

    /// 添加文本（使用当前累加样式）。
    pub fn text(mut self, text: impl Into<String>) -> Self {
        let t = text.into();
        if !t.is_empty() {
            let start = self.cursor;
            let end = start + t.chars().count();
            self.push_span(start, end);
            self.content.push_str(&t);
            self.cursor = end;
        }
        self
    }

    /// 添加文本并覆盖当前样式（此段应用 `style`，之后恢复）。
    pub fn text_styled(mut self, text: impl Into<String>, style: &TextStyle) -> Self {
        let saved = self.current.clone();
        self.apply_textstyle(style);
        self = self.text(text);
        self.current = saved;
        self
    }

    /// 用 `TextStyle` 覆盖当前累加样式的某些属性。
    fn apply_textstyle(&mut self, style: &TextStyle) {
        if let Some(v) = style.color { self.current.color = Some(v); }
        if let Some(v) = style.font_size { self.current.font_size = Some(v); }
        if let Some(v) = style.font_weight { self.current.font_weight = Some(v); }
        if let Some(v) = style.font_style { self.current.font_style = Some(v); }
        if style.underline { self.current.underline = true; }
        if style.strikethrough { self.current.strikethrough = true; }
        if let Some(v) = style.background { self.current.background = Some(v); }
    }

    /// 通过闭包修改当前累加样式（从空白起始，仅保留闭包中明确设置的属性）。
    ///
    /// ```ignore
    /// .with_style(|s| s.bold().font_size(20.0))
    /// ```
    pub fn with_style(mut self, f: impl FnOnce(StyleModifier) -> StyleModifier) -> Self {
        self.current = f(StyleModifier(StyleAccum::default())).0;
        self
    }

    /// 添加内联图片/SVG（使用当前累加样式确定占位尺寸）。
    pub fn image(mut self, drawable: impl Into<Arc<dyn InlineDrawable>>) -> Self {
        let drawable: Arc<dyn InlineDrawable> = drawable.into();
        // 用占位符替代 drawable，并记录样式
        let start = self.cursor;
        let end = start + 1; // 一个 U+FFFC 占 1 字符位
        self.push_span(start, end);
        self.content.push('\u{FFFC}');
        self.drawables.push(drawable);
        self.cursor = end;
        self
    }

    /// 应用 Modifier。
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 将 [start, end) 的当前样式提交为 span（如果 current 非默认则提交）。
    fn push_span(&mut self, start: usize, end: usize) {
        if start >= end { return; }
        // 如果当前样式都是默认值而且没有装饰属性，不添加 span（用 base 样式渲染）
        if self.current.font_size.is_none()
            && self.current.color.is_none()
            && self.current.font_weight.is_none()
            && self.current.font_style.is_none()
            && !self.current.underline
            && !self.current.strikethrough
            && self.current.background.is_none() { return; }
        self.spans.push(RichSpanStyle {
            start,
            end,
            font_size: self.current.font_size.unwrap_or(0.0),
            color: self.current.color.unwrap_or(Color::from_argb(0, 0, 0, 0)),
            font_weight: self.current.font_weight.unwrap_or(crate::ui::text::FontWeight::NORMAL),
            font_style: self.current.font_style.unwrap_or(crate::ui::text::FontSlant::Upright),
            underline: self.current.underline,
            strikethrough: self.current.strikethrough,
            background: self.current.background,
        });
    }

    fn is_default(&self) -> bool {
        self.current.font_size.is_none()
            && self.current.color.is_none()
            && self.current.font_weight.is_none()
            && self.current.font_style.is_none()
            && !self.current.underline
            && !self.current.strikethrough
            && self.current.background.is_none()
    }

    /// 构建并注册到组合树。
    ///
    /// 解析所有样式的最终值（继承 ProvideTextStyle + Theme），
    /// 合并相邻重叠的 span，存入 modifier 后注册为叶子节点。
    pub fn build(mut self, ctx: &mut ComposeCtx) {
        // 最终 span 解析：合并 + 继承
        let spans = self.resolve_and_merge();

        let modifier = self.modifier.push(ModifierElement::RichTextContent {
            content: self.content,
            drawables: self.drawables,
            spans,
        });
        let key = ctx.next_key();
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }

    /// 解析所有悬挂 span + 继承默认值，合并重叠范围。
    fn resolve_and_merge(&mut self) -> Vec<RichSpanStyle> {
        let base = crate::ui::text::LOCAL_TEXT_STYLE.current();
        let theme = crate::ui::theme::WiniaTheme::colors();
        let d_color = base.color.unwrap_or(theme.on_surface);
        let d_font_size = base.font_size.unwrap_or(14.0);
        let d_weight = base.font_weight.unwrap_or_default();
        let d_slant = base.font_style.unwrap_or_default();

        let total = self.cursor;

        // 1. 每个 span 独立解析（用 span 自己的值补全缺省）
        let mut resolved: Vec<RichSpanStyle> = self.spans.drain(..).map(|s| {
            RichSpanStyle {
                start: s.start,
                end: s.end.min(total),
                font_size: if s.font_size > 0.0 { s.font_size } else { base.font_size.unwrap_or(d_font_size) },
                color: if s.color.a > 0 || s.color.r > 0 || s.color.g > 0 || s.color.b > 0 { s.color } else { base.color.unwrap_or(d_color) },
                font_weight: if s.font_weight != crate::ui::text::FontWeight::NORMAL || s.strikethrough { s.font_weight } else { base.font_weight.unwrap_or(d_weight) },
                font_style: if s.font_style != crate::ui::text::FontSlant::Upright || s.underline { s.font_style } else { base.font_style.unwrap_or(d_slant) },
                underline: s.underline,
                strikethrough: s.strikethrough,
                background: s.background.map(|c| if c.a > 0 || c.r > 0 || c.g > 0 || c.b > 0 { c } else { d_color }).or(base.background),
            }
        }).collect();

        // 2. 排序、合并相邻重叠
        resolved.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        let mut merged: Vec<RichSpanStyle> = Vec::new();
        for span in resolved {
            if let Some(last) = merged.last_mut() {
                if span.start < last.end {
                    // 真重叠（非相邻）时扩展 end
                    last.end = last.end.max(span.end);
                    continue;
                }
            }
            merged.push(span);
        }

        merged
    }
}

/// 样式修饰器（用于 `.with_style(|s| s.bold().color(RED))`）。
pub struct StyleModifier(StyleAccum);

impl StyleModifier {
    pub fn bold(mut self) -> Self {
        self.0.font_weight = Some(crate::ui::text::FontWeight::BOLD);
        self
    }
    pub fn italic(mut self) -> Self {
        self.0.font_style = Some(crate::ui::text::FontSlant::Italic);
        self
    }
    pub fn font_size(mut self, v: f32) -> Self {
        self.0.font_size = Some(v);
        self
    }
    pub fn color(mut self, v: Color) -> Self {
        self.0.color = Some(v);
        self
    }
    pub fn underline(mut self) -> Self {
        self.0.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.0.strikethrough = true;
        self
    }
    pub fn background(mut self, v: Color) -> Self {
        self.0.background = Some(v);
        self
    }
}

impl Default for RichText {
    fn default() -> Self { Self::new() }
}
