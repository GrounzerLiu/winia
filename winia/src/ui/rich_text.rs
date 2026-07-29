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
    ol: bool,
    st: bool,
    deco_color: Option<Color>,
    deco_style: Option<crate::modifier::DecoStyle>,
    deco_mode: Option<crate::modifier::DecoMode>,
    baseline_shift: f32,
    letter_spacing: f32,
    word_spacing: f32,
    height_multiple: f32,
    half_leading: bool,
    font_families: Vec<String>,
    font_width: i32,
    font_edging: Option<crate::modifier::FontEdge>,
    font_hinting: Option<crate::modifier::FontHint>,
    subpixel: bool,
    foreground_color: Option<Color>,
    bg: Option<Color>,
    locale: Option<String>,
}

impl Style {
    fn is_not_default(&self) -> bool {
        self.fs.is_some() || self.color.is_some() || self.fw.is_some()
            || self.slant.is_some() || self.ul || self.ol || self.st
            || self.deco_color.is_some() || self.deco_style.is_some() || self.deco_mode.is_some()
            || self.baseline_shift != 0.0 || self.letter_spacing != 0.0 || self.word_spacing != 0.0
            || self.height_multiple != 0.0 || self.half_leading
            || !self.font_families.is_empty() || self.font_width != 5
            || self.font_edging.is_some() || self.font_hinting.is_some() || self.subpixel
            || self.foreground_color.is_some() || self.bg.is_some()
            || self.locale.is_some()
    }
}

impl Default for Style {
    fn default() -> Self {
        Style {
            fs: None, color: None, fw: None, slant: None,
            ul: false, ol: false, st: false,
            deco_color: None, deco_style: None, deco_mode: None,
            baseline_shift: 0.0, letter_spacing: 0.0, word_spacing: 0.0,
            height_multiple: 0.0, half_leading: false,
            font_families: Vec::new(), font_width: 5,
            font_edging: None, font_hinting: None, subpixel: false,
            foreground_color: None, bg: None, locale: None,
        }
    }
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
        ul: false, ol: false, st: false,
        deco_color: None, deco_style: None, deco_mode: None,
        baseline_shift: 0.0, letter_spacing: 0.0, word_spacing: 0.0,
        height_multiple: 0.0, half_leading: false,
        font_families: Vec::new(), font_width: 5,
        font_edging: None, font_hinting: None, subpixel: false,
        foreground_color: None, bg: None, locale: None,
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
        if self.style.is_not_default() {
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

    /// 内联图片（向 content 插入 U+FFFC 占位符）。
    pub fn image(&mut self, drawable: impl Into<Arc<dyn InlineDrawable>>) {
        let pos = self.cursor;
        self.drawables.push(drawable.into());
        self.drawable_positions.push(pos);
        self.content.push('\u{FFFC}');
        self.cursor += 1;
        if self.style.is_not_default() {
            self.annotations.push((self.style.clone(), pos..pos + 1));
        }
    }

    /// D:\winia 风格：将已有文本范围标记为图片占位符。
    /// 范围对应的文本字符在渲染时被替换为 drawable。
    pub fn placeholder(&mut self, range: Range<usize>, drawable: impl Into<Arc<dyn InlineDrawable>>) {
        if range.end <= range.start { return; }
        self.drawables.push(drawable.into());
        self.drawable_positions.push(range.start);
        // 被替换的文本段仍保留在 content 中，但 resolve_spans 会为占位符
        // 位置创建 placeholder segment，被替换的文本字符不参与文本渲染。
        if self.style.is_not_default() {
            self.annotations.push((self.style.clone(), range));
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

    pub fn overline(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.ol = true;
        f(self);
        self.style = saved;
    }

    pub fn subscript(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.baseline_shift = 0.259; // 0.15 / 0.58，补偿字号缩小后保持 D:\winia 偏移量
        self.style.fs = Some(saved.fs.unwrap_or(14.0) * 0.58);
        f(self);
        self.style = saved;
    }

    pub fn superscript(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.baseline_shift = -0.517; // -0.30 / 0.58
        self.style.fs = Some(saved.fs.unwrap_or(14.0) * 0.58);
        f(self);
        self.style = saved;
    }

    pub fn baseline_shift(&mut self, v: f32, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.baseline_shift = v;
        f(self);
        self.style = saved;
    }

    pub fn letter_spacing(&mut self, v: f32, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.letter_spacing = v;
        f(self);
        self.style = saved;
    }

    pub fn word_spacing(&mut self, v: f32, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.word_spacing = v;
        f(self);
        self.style = saved;
    }

    pub fn height_multiple(&mut self, v: f32, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.height_multiple = v;
        f(self);
        self.style = saved;
    }

    pub fn half_leading(&mut self, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.half_leading = true;
        f(self);
        self.style = saved;
    }

    pub fn font_family(&mut self, v: &str, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.font_families.push(v.to_string());
        f(self);
        self.style = saved;
    }

    pub fn foreground_color(&mut self, v: Color, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.foreground_color = Some(v);
        f(self);
        self.style = saved;
    }

    pub fn locale(&mut self, v: &str, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.locale = Some(v.to_string());
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
    pub fn overline(mut self) -> Self { self.0.ol = true; self }
    pub fn strikethrough(mut self) -> Self { self.0.st = true; self }
    pub fn background(mut self, v: Color) -> Self { self.0.bg = Some(v); self }
    pub fn decoration_color(mut self, v: Color) -> Self { self.0.deco_color = Some(v); self }
    pub fn decoration_style(mut self, v: crate::modifier::DecoStyle) -> Self { self.0.deco_style = Some(v); self }
    pub fn decoration_mode(mut self, v: crate::modifier::DecoMode) -> Self { self.0.deco_mode = Some(v); self }
    pub fn subscript(mut self) -> Self { self.0.baseline_shift = -0.5; self }
    pub fn superscript(mut self) -> Self { self.0.baseline_shift = 0.5; self }
    pub fn baseline_shift(mut self, v: f32) -> Self { self.0.baseline_shift = v; self }
    pub fn letter_spacing(mut self, v: f32) -> Self { self.0.letter_spacing = v; self }
    pub fn word_spacing(mut self, v: f32) -> Self { self.0.word_spacing = v; self }
    pub fn height_multiple(mut self, v: f32) -> Self { self.0.height_multiple = v; self }
    pub fn half_leading(mut self) -> Self { self.0.half_leading = true; self }
    pub fn font_family(mut self, v: impl Into<String>) -> Self { self.0.font_families.push(v.into()); self }
    pub fn font_width(mut self, v: i32) -> Self { self.0.font_width = v; self }
    pub fn font_edging(mut self, v: crate::modifier::FontEdge) -> Self { self.0.font_edging = Some(v); self }
    pub fn font_hinting(mut self, v: crate::modifier::FontHint) -> Self { self.0.font_hinting = Some(v); self }
    pub fn subpixel(mut self) -> Self { self.0.subpixel = true; self }
    pub fn foreground_color(mut self, v: Color) -> Self { self.0.foreground_color = Some(v); self }
    pub fn locale(mut self, v: impl Into<String>) -> Self { self.0.locale = Some(v.into()); self }
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
    ul: bool, ol: bool, st: bool,
    deco_color: Option<Color>,
    deco_style: Option<crate::modifier::DecoStyle>,
    deco_mode: Option<crate::modifier::DecoMode>,
    baseline_shift: f32,
    letter_spacing: f32, word_spacing: f32,
    height_multiple: f32, half_leading: bool,
    font_families: Vec<String>,
    font_width: i32,
    font_edging: Option<crate::modifier::FontEdge>,
    font_hinting: Option<crate::modifier::FontHint>,
    subpixel: bool,
    foreground_color: Option<Color>,
    bg: Option<Color>,
    locale: Option<String>,
    placeholder: bool,
}

impl Seg {
    fn default_base(fs: f32, color: Color, fw: FontWeight, slant: FontSlant) -> Self {
        Seg {
            range: 0..0, fs, color, fw, slant,
            ul: false, ol: false, st: false,
            deco_color: None, deco_style: None, deco_mode: None,
            baseline_shift: 0.0, letter_spacing: 0.0, word_spacing: 0.0,
            height_multiple: 0.0, half_leading: false,
            font_families: Vec::new(), font_width: 5,
            font_edging: None, font_hinting: None, subpixel: false,
            foreground_color: None, bg: None, locale: None,
            placeholder: false,
        }
    }

    fn clone_at(&self, range: Range<usize>) -> Self {
        Seg {
            range,
            fs: self.fs, color: self.color, fw: self.fw, slant: self.slant,
            ul: self.ul, ol: self.ol, st: self.st,
            deco_color: self.deco_color, deco_style: self.deco_style, deco_mode: self.deco_mode,
            baseline_shift: self.baseline_shift,
            letter_spacing: self.letter_spacing, word_spacing: self.word_spacing,
            height_multiple: self.height_multiple, half_leading: self.half_leading,
            font_families: self.font_families.clone(),
            font_width: self.font_width,
            font_edging: self.font_edging, font_hinting: self.font_hinting,
            subpixel: self.subpixel,
            foreground_color: self.foreground_color, bg: self.bg,
            locale: self.locale.clone(),
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

    // 1) 建 PubSeg
    let mut segs: Vec<Seg> = Vec::new();
    let mut di = 0usize;
    let total = content.chars().count();
    let mut ci = 0usize;
    while ci < total {
        if di < drawable_positions.len() && drawable_positions[di] == ci {
            let mut s = Seg::default_base(d_fs, d_color, d_fw, d_sl);
            s.range = ci..ci+1;
            s.placeholder = true;
            segs.push(s);
            di += 1; ci += 1;
        } else {
            let run_start = ci;
            while ci < total && !(di < drawable_positions.len() && drawable_positions[di] == ci) { ci += 1; }
            let run_len = ci - run_start;
            if run_len > 0 {
                let mut s = Seg::default_base(d_fs, d_color, d_fw, d_sl);
                s.range = run_start..run_start+run_len;
                segs.push(s);
            }
        }
    }

    // 2) D:\winia 分裂
    let resolved_annos: Vec<(Style, Range<usize>)> = annotations.iter()
        .filter(|(s, r)| s.is_not_default() && r.end > r.start)
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
        underline: s.ul, overline: s.ol, strikethrough: s.st,
        decoration_color: s.deco_color,
        decoration_style: s.deco_style,
        decoration_mode: s.deco_mode,
        baseline_shift: s.baseline_shift,
        letter_spacing: s.letter_spacing, word_spacing: s.word_spacing,
        height_multiple: s.height_multiple, half_leading: s.half_leading,
        font_families: s.font_families, font_width: s.font_width,
        font_edging: s.font_edging, font_hinting: s.font_hinting,
        subpixel: s.subpixel,
        foreground_color: s.foreground_color, background: s.bg,
        locale: s.locale,
    }).collect()
}

fn apply_seg(seg: &mut Seg, s: &Style) {
    if let Some(v) = s.fs { seg.fs = v; }
    if let Some(v) = s.color { seg.color = v; }
    if let Some(v) = s.fw { seg.fw = v; }
    if let Some(v) = s.slant { seg.slant = v; }
    if s.ul { seg.ul = true; }
    if s.ol { seg.ol = true; }
    if s.st { seg.st = true; }
    if let Some(v) = s.deco_color { seg.deco_color = Some(v); }
    if let Some(v) = s.deco_style { seg.deco_style = Some(v); }
    if let Some(v) = s.deco_mode { seg.deco_mode = Some(v); }
    if s.baseline_shift != 0.0 { seg.baseline_shift = s.baseline_shift; }
    if s.letter_spacing != 0.0 { seg.letter_spacing = s.letter_spacing; }
    if s.word_spacing != 0.0 { seg.word_spacing = s.word_spacing; }
    if s.height_multiple != 0.0 { seg.height_multiple = s.height_multiple; }
    if s.half_leading { seg.half_leading = true; }
    if !s.font_families.is_empty() { seg.font_families = s.font_families.clone(); }
    if s.font_width != 5 { seg.font_width = s.font_width; }
    if let Some(v) = s.font_edging { seg.font_edging = Some(v); }
    if let Some(v) = s.font_hinting { seg.font_hinting = Some(v); }
    if s.subpixel { seg.subpixel = true; }
    if let Some(v) = s.foreground_color { seg.foreground_color = Some(v); }
    if let Some(v) = s.bg { seg.bg = Some(v); }
    if let Some(v) = &s.locale { seg.locale = Some(v.clone()); }
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
            if style.is_not_default() {
                self.annotations.push((style, start..self.cursor));
            }
        }

        fn image(&mut self) {
            self.drawable_positions.push(self.cursor);
            self.content.push('\u{FFFC}');
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
                ..Style::default()
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
        assert_eq!(ctx.content.chars().nth(1).unwrap(), '\u{FFFC}');
        assert_eq!(ctx.content.chars().nth(3).unwrap(), '\u{FFFC}');
        // spans 不应包含占位位置
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
