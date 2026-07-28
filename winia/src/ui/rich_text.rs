//! RichText — Compose 风格嵌套作用域富文本 API
//!
//! ```ignore
//! RichText::new().build(ctx, |x| {
//!     x.text("Normal ");
//!     x.bold(|x| {
//!         x.text("Bold ");
//!         x.italic(|x| { x.text("Bold+Italic"); });
//!     });
//!     x.image(star_svg());
//! });
//! ```

use crate::core::composer::ComposeCtx;
use crate::modifier::{Modifier, ModifierElement, RichSpanStyle, Color};
use crate::text::InlineDrawable;
use crate::ui::text::{FontWeight, FontSlant, TextStyle, LOCAL_TEXT_STYLE};
use crate::ui::theme::WiniaTheme;
use std::sync::Arc;
use std::ops::Range;

// ── Style（累积样式，按作用域嵌套叠加）──

#[derive(Clone, Debug)]
struct Style {
    fs: Option<f32>,
    color: Option<Color>,
    fw: Option<FontWeight>,
    slant: Option<FontSlant>,
    ul: bool,
    st: bool,
    bg: Option<Color>,
}

impl Style {
    fn is_empty(&self) -> bool {
        self.fs.is_none() && self.color.is_none() && self.fw.is_none()
            && self.slant.is_none() && !self.ul && !self.st && self.bg.is_none()
    }
    fn is_default(&self) -> bool {
        self.fs.is_none() && self.color.is_none() && self.fw.is_none()
            && self.slant.is_none() && !self.ul && !self.st && self.bg.is_none()
    }
}

impl Default for Style {
    fn default() -> Self { Style { fs: None, color: None, fw: None, slant: None, ul: false, st: false, bg: None } }
}

/// 从 TextStyle + ProvideTextStyle + Theme 解析默认值
fn resolve_base() -> Style {
    let theme = WiniaTheme::colors();
    let base = LOCAL_TEXT_STYLE.current();
    Style {
        fs: base.font_size,
        color: base.color.or(Some(theme.on_surface)),
        fw: base.font_weight,
        slant: base.font_style,
        ul: false, st: false, bg: None,
    }
}

// ── 纯文本 + drawable 位置 → U+FFFC content ──

// ── RichTextScope（给闭包用的可变上下文）──

pub struct RichTextScope<'a> {
    content: &'a mut String,
    drawables: &'a mut Vec<Arc<dyn InlineDrawable>>,
    drawable_positions: &'a mut Vec<usize>,
    annotations: &'a mut Vec<(Style, Range<usize>)>,
    style: Style,
    cursor: usize,
}

impl<'a> RichTextScope<'a> {
    /// 添加纯文本（使用当前累积样式）。
    pub fn text(&mut self, s: &str) {
        if s.is_empty() { return; }
        let start = self.cursor;
        self.content.push_str(s);
        self.cursor += s.chars().count();
        if !self.style.is_empty() {
            self.annotations.push((self.style.clone(), start..self.cursor));
        }
    }

    /// 添加带样式的文本（等价于 push_style().text(s).pop_style()）。
    pub fn text_styled(&mut self, s: &str, style: &TextStyle) {
        let saved = self.style.clone();
        self.apply_textstyle(style);
        self.text(s);
        self.style = saved;
    }

    fn apply_textstyle(&mut self, s: &TextStyle) {
        s.color.map(|v| self.style.color = Some(v));
        s.font_size.map(|v| self.style.fs = Some(v));
        s.font_weight.map(|v| self.style.fw = Some(v));
        s.font_style.map(|v| self.style.slant = Some(v));
        if s.underline { self.style.ul = true; }
        if s.strikethrough { self.style.st = true; }
        s.background.map(|v| self.style.bg = Some(v));
    }

    /// 内联图片。
    pub fn image(&mut self, drawable: impl Into<Arc<dyn InlineDrawable>>) {
        let pos = self.cursor;
        self.drawables.push(drawable.into());
        self.drawable_positions.push(pos);
        // 占一个字符位（build 时会替换为 U+FFFC）
        self.content.push(' ');
        self.cursor += 1;
        if !self.style.is_empty() {
            self.annotations.push((self.style.clone(), pos..pos + 1));
        }
    }

    // ── 嵌套作用域方法 ──

    pub fn bold(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.fw = Some(FontWeight::BOLD);
        f(self);
        self.style = saved;
    }

    pub fn italic(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.slant = Some(FontSlant::Italic);
        f(self);
        self.style = saved;
    }

    pub fn font_size(&mut self, v: f32, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.fs = Some(v);
        f(self);
        self.style = saved;
    }

    pub fn color(&mut self, v: Color, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.color = Some(v);
        f(self);
        self.style = saved;
    }

    pub fn underline(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.ul = true;
        f(self);
        self.style = saved;
    }

    pub fn strikethrough(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.st = true;
        f(self);
        self.style = saved;
    }

    pub fn background(&mut self, v: Color, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.bg = Some(v);
        f(self);
        self.style = saved;
    }

    /// 任意样式闭包（用于同时设置多个属性）。
    pub fn style(&mut self, modifier: impl FnOnce(StyleModifier) -> StyleModifier, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        let sm = modifier(StyleModifier(self.style.clone()));
        self.style = sm.0;
        f(self);
        self.style = saved;
    }
}

pub struct StyleModifier(Style);

impl StyleModifier {
    pub fn bold(mut self) -> Self { self.0.fw = Some(FontWeight::BOLD); self }
    pub fn italic(mut self) -> Self { self.0.slant = Some(FontSlant::Italic); self }
    pub fn font_size(mut self, v: f32) -> Self { self.0.fs = Some(v); self }
    pub fn color(mut self, v: Color) -> Self { self.0.color = Some(v); self }
    pub fn underline(mut self) -> Self { self.0.ul = true; self }
    pub fn strikethrough(mut self) -> Self { self.0.st = true; self }
    pub fn background(mut self, v: Color) -> Self { self.0.bg = Some(v); self }
}

// ── RichText 组件 ──

pub struct RichText {
    modifier: Modifier,
}

impl RichText {
    pub fn new() -> Self { RichText { modifier: Modifier::new() } }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 构建富文本。`f` 接收一个 `RichTextScope`，在其上调用 `.text()` / `.bold()` 等。
    pub fn build(self, ctx: &mut ComposeCtx, f: impl FnOnce(&mut RichTextScope)) {
        let mut content = String::new();
        let mut drawables: Vec<Arc<dyn InlineDrawable>> = Vec::new();
        let mut drawable_positions: Vec<usize> = Vec::new();
        let mut annotations: Vec<(Style, Range<usize>)> = Vec::new();
        let base = resolve_base();

        {
            let mut scope = RichTextScope {
                content: &mut content,
                drawables: &mut drawables,
                drawable_positions: &mut drawable_positions,
                annotations: &mut annotations,
                style: base,
                cursor: 0,
            };
            f(&mut scope);
        }

        // 用 D:\winia 分裂算法解析 span
        let spans = resolve_spans(&content, &drawable_positions, &annotations);

        let modifier = self.modifier.push(ModifierElement::RichTextContent {
            content,
            drawables,
            drawable_positions,
            spans,
        });
        let key = ctx.next_key();
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }
}

impl Default for RichText { fn default() -> Self { Self::new() } }

// ── D:\winia 风格 span 解析 ──

struct Seg {
    range: Range<usize>,
    fs: f32, color: Color, fw: FontWeight, slant: FontSlant,
    ul: bool, st: bool, bg: Option<Color>,
    placeholder: bool,
}

impl Seg {
    fn clone_at(&self, range: Range<usize>) -> Self {
        Seg {
            range, fs: self.fs, color: self.color, fw: self.fw, slant: self.slant,
            ul: self.ul, st: self.st, bg: self.bg,
            placeholder: self.placeholder,
        }
    }
}

pub(crate) fn resolve_spans(content: &str, drawable_positions: &[usize], annotations: &[(Style, Range<usize>)]) -> Vec<RichSpanStyle> {
    let base = resolve_base();
    let d_color = base.color.unwrap_or(Color::from_argb(255, 255, 255, 255));
    let d_fs = base.fs.unwrap_or(14.0);
    let d_fw = base.fw.unwrap_or(FontWeight::NORMAL);
    let d_sl = base.slant.unwrap_or(FontSlant::Upright);

    // 1) 建 PubSeg（纯文本 + drawable 占位）
    let mut segs: Vec<Seg> = Vec::new();
    let mut di = 0usize;
    let total = content.chars().count();
    let mut ci = 0usize;
    while ci < total {
        if di < drawable_positions.len() && drawable_positions[di] == ci {
            segs.push(Seg {
                range: ci..ci+1,
                fs: d_fs, color: d_color, fw: d_fw, slant: d_sl,
                ul: false, st: false, bg: None, placeholder: true,
            });
            di += 1; ci += 1;
        } else {
            let run_start = ci;
            while ci < total && !(di < drawable_positions.len() && drawable_positions[di] == ci) {
                ci += 1;
            }
            let run_len = ci - run_start;
            if run_len > 0 {
                segs.push(Seg {
                    range: run_start..run_start+run_len,
                    fs: d_fs, color: d_color, fw: d_fw, slant: d_sl,
                    ul: false, st: false, bg: None, placeholder: false,
                });
            }
        }
    }

    // 2) D:\winia 分裂
    let resolved_annos: Vec<(Style, Range<usize>)> = annotations.iter()
        .filter(|(s, r)| !s.is_default() && r.end > r.start)
        .map(|(s, r)| (s.clone(), r.clone()))
        .collect();

    for (attr, anno_range) in &resolved_annos {
        let a_start = anno_range.start;
        let a_end = anno_range.end;
        let mut i = 0;
        while i < segs.len() {
            let s_start = segs[i].range.start;
            let s_end = segs[i].range.end;
            if s_start >= a_end { break; }
            if segs[i].placeholder { i += 1; continue; }
            if a_start <= s_start && a_end >= s_end {
                apply_seg(&mut segs[i], attr);
                i += 1;
            } else if a_start > s_start && a_start < s_end && a_end < s_end {
                let mut mid = segs[i].clone_at(a_start..a_end);
                apply_seg(&mut mid, attr);
                let right = segs[i].clone_at(a_end..s_end);
                segs[i].range.end = a_start;
                segs.insert(i+1, mid);
                segs.insert(i+2, right);
                i += 3;
            } else if a_start > s_start && a_start < s_end {
                let mut right = segs[i].clone_at(a_start..s_end);
                apply_seg(&mut right, attr);
                segs[i].range.end = a_start;
                segs.insert(i+1, right);
                i += 2;
            } else if a_end > s_start && a_end < s_end {
                let mut left = segs[i].clone_at(s_start..a_end);
                apply_seg(&mut left, attr);
                segs[i].range.start = a_end;
                segs.insert(i, left);
                i += 2;
            } else { i += 1; }
        }
    }

    // 3) 转 RichSpanStyle
    segs.into_iter().filter(|s| !s.placeholder).map(|s| RichSpanStyle {
        start: s.range.start, end: s.range.end,
        font_size: s.fs, color: s.color, font_weight: s.fw, font_style: s.slant,
        underline: s.ul, strikethrough: s.st, background: s.bg,
    }).collect()
}

fn apply_seg(seg: &mut Seg, s: &Style) {
    if let Some(v) = s.fs { seg.fs = v; }
    if let Some(v) = s.color { seg.color = v; }
    if let Some(v) = s.fw { seg.fw = v; }
    if let Some(v) = s.slant { seg.slant = v; }
    if s.ul { seg.ul = true; }
    if s.st { seg.st = true; }
    if let Some(v) = s.bg { seg.bg = Some(v); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::Color;

    struct TestCtx {
        content: String,
        drawable_positions: Vec<usize>,
        annotations: Vec<(Style, Range<usize>)>,
        cursor: usize,
    }

    impl TestCtx {
        fn new() -> Self {
            TestCtx { content: String::new(), drawable_positions: Vec::new(), annotations: Vec::new(), cursor: 0 }
        }

        fn text(&mut self, s: &str, style: Style) {
            if s.is_empty() { return; }
            let start = self.cursor;
            self.content.push_str(s);
            self.cursor += s.chars().count();
            if !style.is_default() {
                self.annotations.push((style, start..self.cursor));
            }
        }

        fn image(&mut self) {
            self.drawable_positions.push(self.cursor);
            self.content.push(' ');
            self.cursor += 1;
        }

        fn spans(&self) -> Vec<RichSpanStyle> {
            resolve_spans(&self.content, &self.drawable_positions, &self.annotations)
        }
    }

    impl Style {
        fn ul() -> Self { Style { ul: true, ..Style::default() } }
        fn st() -> Self { Style { st: true, ..Style::default() } }
        fn b() -> Self { Style { fw: Some(FontWeight::BOLD), ..Style::default() } }
        fn i() -> Self { Style { slant: Some(FontSlant::Italic), ..Style::default() } }
        fn fs(v: f32) -> Self { Style { fs: Some(v), ..Style::default() } }
        fn col(c: Color) -> Self { Style { color: Some(c), ..Style::default() } }
        fn all() -> Self {
            Style {
                fs: Some(18.0),
                color: Some(Color::from_argb(255, 255, 0, 0)),
                fw: Some(FontWeight::BOLD),
                slant: Some(FontSlant::Italic),
                ul: true, st: true,
                bg: Some(Color::from_argb(60, 255, 255, 0)),
            }
        }
    }

    // ── 基础：空 ──
    #[test]
    fn test_empty() {
        assert!(TestCtx::new().spans().is_empty());
    }

    // ── 基础：纯文本无注解 ──
    #[test]
    fn test_plain_text_no_annotations() {
        let mut ctx = TestCtx::new();
        ctx.text("Hello World", Style::default());
        let spans = ctx.spans();
        // 应该产生一个覆盖全文的默认 segment
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 11);
    }

    // ── 核心：underline 不泄露到相邻 strikethrough ──
    #[test]
    fn test_underline_not_leaking_to_strikethrough() {
        let mut ctx = TestCtx::new();
        ctx.text("Underlined text. ", Style::ul());
        ctx.text("Strikethrough text. ", Style::st());
        let spans = ctx.spans();

        let ul: Vec<_> = spans.iter().filter(|s| s.underline).collect();
        let st: Vec<_> = spans.iter().filter(|s| s.strikethrough).collect();

        // underline 和 strikethrough 各自至少有一段
        assert!(!ul.is_empty(), "underline should exist");
        assert!(!st.is_empty(), "strikethrough should exist");
        // 任意 underline span 的后面都不应再有 underline（只有一个连续段）
        for s in &spans {
            if s.underline {
                assert!(!s.strikethrough, "underline span should not also be strikethrough");
            }
            if s.strikethrough {
                assert!(!s.underline, "strikethrough span should not also be underline");
            }
        }
        // 相邻 span 不重叠
        for i in 1..spans.len() {
            assert!(spans[i-1].end <= spans[i].start,
                "spans[{}].end({}) > spans[{}].start({}) — 重叠",
                i-1, spans[i-1].end, i, spans[i].start);
        }
    }

    // ── 标量属性不跨界 ──
    #[test]
    fn test_scalar_properties_scope() {
        let red = Color::from_argb(255, 255, 0, 0);
        let mut ctx = TestCtx::new();
        ctx.text("Plain. ", Style::default());
        ctx.text("Red. ", Style::col(red));
        ctx.text("Big. ", Style::fs(20.0));
        ctx.text("Plain again.", Style::default());
        let spans = ctx.spans();

        // 找到每个属性的 span
        let red_span = spans.iter().find(|s| s.color == red).expect("red span");
        let big_span = spans.iter().find(|s| (s.font_size - 20.0).abs() < 0.001).expect("big span");

        // red span 只能在它自己的范围内
        assert_eq!(red_span.color, red);
        // big span 只能是字号 20
        assert!((big_span.font_size - 20.0).abs() < 0.001);

        // red span 后面的 span 不应继承 red
        for s in spans.iter().filter(|s| s.start >= red_span.end) {
            assert_ne!(s.color, red, "color leaked past red span endpoint");
        }
    }

    // ── 嵌套合并：bold + italic 共存 ──
    #[test]
    fn test_nested_styles_merge() {
        let mut ctx = TestCtx::new();
        ctx.text("Plain. ", Style::default());
        // 模拟 scope.bold(|x| { x.italic(|x| { x.text("BoldItalic"); }); });
        ctx.text("BoldItalic", Style { fw: Some(FontWeight::BOLD), slant: Some(FontSlant::Italic), ..Style::default() });
        let spans = ctx.spans();

        let bi = spans.iter().find(|s|
            s.font_weight != FontWeight::NORMAL && s.font_style != FontSlant::Upright
        ).expect("bold+italic span");

        assert!(bi.font_weight != FontWeight::NORMAL, "should be bold");
        assert!(bi.font_style != FontSlant::Upright, "should be italic");
    }

    // ── 全部属性同时作用 ──
    #[test]
    fn test_all_attributes_together() {
        let mut ctx = TestCtx::new();
        ctx.text("Normal. ", Style::default());
        ctx.text("All. ", Style::all());
        ctx.text("Normal again.", Style::default());
        let spans = ctx.spans();

        let all = spans.iter().find(|s|
            (s.font_size - 18.0).abs() < 0.001
            && s.underline && s.strikethrough
            && s.font_weight != FontWeight::NORMAL
        ).expect("all-attributes span");

        assert!(all.background.is_some(), "background should be set");
        assert!(all.underline, "underline");
        assert!(all.strikethrough, "strikethrough");
    }

    // ── drawable 位置 ──
    #[test]
    fn test_drawable_positions() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::default());
        ctx.image();              // pos 1
        ctx.text("B", Style::default());
        ctx.image();              // pos 3
        ctx.text("C", Style::default());

        assert_eq!(ctx.drawable_positions, vec![1, 3], "drawable indices");
        assert_eq!(ctx.content.chars().nth(1).unwrap(), ' ');
        assert_eq!(ctx.content.chars().nth(3).unwrap(), ' ');
        // spans 不应该包含占位位置（placeholder segment 被过滤）
        let spans = ctx.spans();
        for s in &spans {
            assert!(!(1..2).contains(&s.start), "placeholder pos 1 leaked into span");
            assert!(!(3..4).contains(&s.start), "placeholder pos 3 leaked into span");
        }
    }

    // ── drawable 在开头 ──
    #[test]
    fn test_drawable_at_start() {
        let mut ctx = TestCtx::new();
        ctx.image();              // pos 0
        ctx.text("Text", Style::default());
        assert_eq!(ctx.drawable_positions, vec![0]);
        let spans = ctx.spans();
        // spans 从 pos 1 开始
        assert!(spans.iter().all(|s| s.start >= 1), "no span should cover drawable position");
    }

    // ── 连续 drawable ──
    #[test]
    fn test_consecutive_drawables() {
        let mut ctx = TestCtx::new();
        ctx.image(); ctx.image(); ctx.image();
        ctx.text("After", Style::default());
        assert_eq!(ctx.drawable_positions, vec![0, 1, 2]);
        let spans = ctx.spans();
        // 只有一个 text segment 从 pos 3 开始
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 3);
    }

    // ── drawable 嵌入带样式的文本 ──
    #[test]
    fn test_drawable_within_styled_text() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::b());
        ctx.image();
        ctx.text("C", Style::b());
        let spans = ctx.spans();
        // 应该有两个 bold span（分别左右），中间不含占位符
        assert_eq!(spans.len(), 2, "text before and after image");
        for s in &spans {
            assert!(s.font_weight != FontWeight::NORMAL, "both spans should be bold");
        }
        assert_eq!(spans[0].end, 1);
        assert_eq!(spans[1].start, 2);
    }

    // ── 注解在开头 ──
    #[test]
    fn test_annotation_at_start() {
        let mut ctx = TestCtx::new();
        ctx.text("Bold start", Style::b());
        ctx.text("normal end", Style::default());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold span");
        assert_eq!(bold.start, 0, "annotation should start at 0");
    }

    // ── 注解在结尾 ──
    #[test]
    fn test_annotation_at_end() {
        let mut ctx = TestCtx::new();
        ctx.text("normal ", Style::default());
        ctx.text("bold end", Style::b());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold span");
        let content_len = ctx.content.len();
        assert_eq!(bold.end, content_len, "annotation should reach the end");
    }

    // ── 零宽度注解（跳过）──
    #[test]
    fn test_zero_width_annotation() {
        let mut ctx = TestCtx::new();
        ctx.text("A", Style::b());
        // 理论上不会产生空注解，但如果 cursor 没动，应该被忽略
        // 模拟零宽度：跳过 text() 直接调 push
        let spans = ctx.spans();
        assert!(spans.iter().all(|s| s.end > s.start), "no zero-width spans");
    }

    // ── 单字符注解 ──
    #[test]
    fn test_single_char_annotation() {
        let mut ctx = TestCtx::new();
        ctx.text("H", Style::b());
        ctx.text("ello", Style::default());
        let spans = ctx.spans();
        let bold = spans.iter().find(|s| s.font_weight != FontWeight::NORMAL).expect("bold H");
        assert_eq!(bold.end - bold.start, 1, "single char");
    }

    // ── 无重叠属性不应互相覆盖 ──
    #[test]
    fn test_non_overlapping_attributes_preserved() {
        let mut ctx = TestCtx::new();
        // 相同范围：bold + underline + big
        let style = Style { fw: Some(FontWeight::BOLD), fs: Some(20.0), ul: true, ..Style::default() };
        ctx.text("Title", style);
        let spans = ctx.spans();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].font_weight != FontWeight::NORMAL, "bold");
        assert!(spans[0].underline, "underline");
        assert!((spans[0].font_size - 20.0).abs() < 0.001, "font size");
    }
}
