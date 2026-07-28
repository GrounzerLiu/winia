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

fn build_fffc_content(content: &str, drawable_positions: &[usize]) -> String {
    let mut out = String::with_capacity(content.len() + drawable_positions.len());
    let mut di = 0usize;
    for (ci, ch) in content.char_indices() {
        if di < drawable_positions.len() && drawable_positions[di] == ci / content[..ci].chars().count() {
            out.push('\u{FFFC}');
            di += 1;
        }
        out.push(ch);
    }
    // drawable 在末尾的情况
    let char_count = content.chars().count();
    while di < drawable_positions.len() && drawable_positions[di] == char_count + di {
        out.push('\u{FFFC}');
        di += 1;
    }
    out
}

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
        let content_with_fffc = build_fffc_content(&content, &drawable_positions);

        let modifier = self.modifier.push(ModifierElement::RichTextContent {
            content: content_with_fffc,
            drawables,
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

fn resolve_spans(content: &str, drawable_positions: &[usize], annotations: &[(Style, Range<usize>)]) -> Vec<RichSpanStyle> {
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
