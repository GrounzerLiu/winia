//! `RichText` — Compose-style rich text with nested scopes.
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

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::modifier::{Modifier, ModifierElement};
use crate::text::{RichSpanStyle};
use crate::graphics::{Color};
use crate::text::InlineDrawable;
use crate::components::text::{LOCAL_TEXT_STYLE};
use crate::text::{FontWeight, FontSlant, TextStyle};
use crate::unit::TextUnit;
use crate::theme::WiniaTheme;
use std::sync::Arc;
use std::ops::Range;

// ── Style: accumulates across nested scopes ──

#[derive(Clone, Debug)]
pub(crate) struct Style {
    fs: Option<f32>,
    color: Option<Color>,
    fw: Option<FontWeight>,
    slant: Option<FontSlant>,
    ul: bool,
    ol: bool,
    st: bool,
    deco_color: Option<Color>,
    deco_style: Option<crate::text::DecoStyle>,
    deco_mode: Option<crate::text::DecoMode>,
    baseline_shift: f32,
    letter_spacing: crate::unit::TextUnit,
    word_spacing: f32,
    height_multiple: f32,
    half_leading: bool,
    font_families: Vec<String>,
    font_width: i32,
    font_edging: Option<crate::text::FontEdge>,
    font_hinting: Option<crate::text::FontHint>,
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
            || self.baseline_shift != 0.0
            || self.letter_spacing != crate::unit::TextUnit::Sp(crate::unit::Sp(0.0))
            || self.word_spacing != 0.0
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
            baseline_shift: 0.0,
            letter_spacing: crate::unit::TextUnit::Sp(crate::unit::Sp(0.0)),
            word_spacing: 0.0,
            height_multiple: 0.0, half_leading: false,
            font_families: Vec::new(), font_width: 5,
            font_edging: None, font_hinting: None, subpixel: false,
            foreground_color: None, bg: None, locale: None,
        }
    }
}

/// Resolve the base from `TextStyle`, `ProvideTextStyle` and the theme.
fn resolve_base() -> Style {
    let theme = WiniaTheme::colors();
    let base = LOCAL_TEXT_STYLE.current();
    Style {
        // An `Em` in the inherited style is a multiple of the 14 sp default here (Compose resolves
        // `Em` against the enclosing size, and `LOCAL_TEXT_STYLE` is that enclosing level).
        fs: base.font_size.map(|u| match u {
            TextUnit::Em(em) => 14.0 * em,
            other => other.to_logical_px(crate::runtime::density::current_density()),
        }),
        color: base.color.or(Some(theme.on_surface)),
        fw: base.font_weight,
        slant: base.font_style,
        ul: false, ol: false, st: false,
        deco_color: None, deco_style: None, deco_mode: None,
        baseline_shift: 0.0,
        letter_spacing: crate::unit::TextUnit::Sp(crate::unit::Sp(0.0)),
        word_spacing: 0.0,
        height_multiple: 0.0, half_leading: false,
        font_families: Vec::new(), font_width: 5,
        font_edging: None, font_hinting: None, subpixel: false,
        foreground_color: None, bg: None, locale: None,
    }
}

// ── Plain text plus drawable ranges -> content with U+FFFC placeholders ──

// ── RichTextScope: the mutable context a build closure receives ──

pub struct RichTextScope<'a> {
    content: &'a mut String,
    drawables: &'a mut Vec<Arc<dyn InlineDrawable>>,
    /// Drawable ranges, one `[start, end)` per inline element.
    drawable_ranges: &'a mut Vec<Range<usize>>,
    annotations: &'a mut Vec<(Style, Range<usize>)>,
    style: Style,
    cursor: usize,
}

impl<'a> RichTextScope<'a> {
    /// Append plain text under the style accumulated so far.
    pub fn text(&mut self, s: &str) {
        if s.is_empty() { return; }
        let start = self.cursor;
        self.content.push_str(s);
        self.cursor += s.chars().count();
        if self.style.is_not_default() {
            self.annotations.push((self.style.clone(), start..self.cursor));
        }
    }

    /// Append styled text; equivalent to `push_style(..).text(s).pop_style()`.
    pub fn text_styled(&mut self, s: &str, style: &TextStyle) {
        let saved = self.style.clone();
        self.apply_textstyle(style);
        self.text(s);
        self.style = saved;
    }

    fn apply_textstyle(&mut self, s: &TextStyle) {
        s.color.map(|v| self.style.color = Some(v));
        s.font_size.map(|v| self.style.fs = Some(v.to_logical_px(crate::runtime::density::current_density())));
        s.font_weight.map(|v| self.style.fw = Some(v));
        s.font_style.map(|v| self.style.slant = Some(v));
        if let Some(d) = s.decoration {
            // A span-level style REPLACES the decoration, the way `TextStyle.merge` does (D-1).
            self.style.ul = d.underline;
            self.style.st = d.line_through;
        }
        s.background.map(|v| self.style.bg = Some(v));
    }

    /// An inline picture: a U+FFFC placeholder goes into the content.
    pub fn image(&mut self, drawable: impl Into<Arc<dyn InlineDrawable>>) {
        let pos = self.cursor;
        self.drawables.push(drawable.into());
        self.drawable_ranges.push(pos..pos + 1);
        self.content.push('\u{FFFC}');
        self.cursor += 1;
        if self.style.is_not_default() {
            self.annotations.push((self.style.clone(), pos..pos + 1));
        }
    }

    /// A placeholder: the text lands in the content but is NOT drawn — the drawable replaces it
    /// visually (the same contract as winia v1).
    pub fn placeholder(&mut self, text: impl Into<String>, drawable: impl Into<Arc<dyn InlineDrawable>>) {
        let pos = self.cursor;
        let t = text.into();
        let len = t.chars().count();
        if len == 0 { return; }
        self.content.push_str(&t);
        self.drawables.push(drawable.into());
        self.drawable_ranges.push(pos..pos + len);
        self.cursor += len;
        if self.style.is_not_default() {
            self.annotations.push((self.style.clone(), pos..self.cursor));
        }
    }

    // ── Nested-scope methods ──

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

    pub fn font_size(&mut self, v: impl Into<crate::unit::TextUnit>, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.fs = Some(v.into().to_logical_px(crate::runtime::density::current_density()));
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
        self.style.baseline_shift = 0.259; // 0.15 / 0.58，补偿字号缩小后保持旧版偏移量
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

    /// Compose's `SpanStyle.letterSpacing`: a `TextUnit`, so an `em` means a multiple of THIS
    /// span's font size (resolved when the span is expanded).
    pub fn letter_spacing(&mut self, v: impl Into<crate::unit::TextUnit>, f: impl FnOnce(&mut Self)) {
        let saved = self.style.clone();
        self.style.letter_spacing = v.into();
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

    /// A scope over an arbitrary style, to set several properties at once.
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
    pub fn font_size(mut self, v: impl Into<crate::unit::TextUnit>) -> Self { self.0.fs = Some(v.into().to_logical_px(crate::runtime::density::current_density())); self }
    pub fn color(mut self, v: Color) -> Self { self.0.color = Some(v); self }
    pub fn underline(mut self) -> Self { self.0.ul = true; self }
    pub fn overline(mut self) -> Self { self.0.ol = true; self }
    pub fn strikethrough(mut self) -> Self { self.0.st = true; self }
    pub fn background(mut self, v: Color) -> Self { self.0.bg = Some(v); self }
    pub fn decoration_color(mut self, v: Color) -> Self { self.0.deco_color = Some(v); self }
    pub fn decoration_style(mut self, v: crate::text::DecoStyle) -> Self { self.0.deco_style = Some(v); self }
    pub fn decoration_mode(mut self, v: crate::text::DecoMode) -> Self { self.0.deco_mode = Some(v); self }
    pub fn subscript(mut self) -> Self { self.0.baseline_shift = -0.5; self }
    pub fn superscript(mut self) -> Self { self.0.baseline_shift = 0.5; self }
    pub fn baseline_shift(mut self, v: f32) -> Self { self.0.baseline_shift = v; self }
    /// Compose's `SpanStyle.letterSpacing`, as a unit (`sp`, or an `em` of this span's size).
    pub fn letter_spacing(mut self, v: impl Into<crate::unit::TextUnit>) -> Self { self.0.letter_spacing = v.into(); self }
    pub fn word_spacing(mut self, v: f32) -> Self { self.0.word_spacing = v; self }
    pub fn height_multiple(mut self, v: f32) -> Self { self.0.height_multiple = v; self }
    pub fn half_leading(mut self) -> Self { self.0.half_leading = true; self }
    pub fn font_family(mut self, v: impl Into<String>) -> Self { self.0.font_families.push(v.into()); self }
    pub fn font_width(mut self, v: i32) -> Self { self.0.font_width = v; self }
    pub fn font_edging(mut self, v: crate::text::FontEdge) -> Self { self.0.font_edging = Some(v); self }
    pub fn font_hinting(mut self, v: crate::text::FontHint) -> Self { self.0.font_hinting = Some(v); self }
    pub fn subpixel(mut self) -> Self { self.0.subpixel = true; self }
    pub fn foreground_color(mut self, v: Color) -> Self { self.0.foreground_color = Some(v); self }
    pub fn locale(mut self, v: impl Into<String>) -> Self { self.0.locale = Some(v.into()); self }
}

// ── The RichText component ──

pub struct RichText {
    modifier: Modifier,
    align: crate::text::TextAlign,
    max_lines: usize,
    overflow: crate::text::TextOverflow,
    soft_wrap: bool,
    line_height: Option<f32>,
    min_lines: usize,
    line_height_style: Option<crate::text::LineHeightStyle>,
    /// A value built ahead of time (`Text::annotated`'s Compose counterpart); its text is laid out
    /// BEFORE anything the build closure appends.
    annotated: Option<crate::text::AnnotatedString>,
}

impl RichText {
    pub fn new() -> Self {
        RichText {
            modifier: Modifier::new(),
            // Compose's defaults for `BasicText`
            align: crate::text::TextAlign::Left,
            max_lines: usize::MAX,
            overflow: crate::text::TextOverflow::Clip,
            soft_wrap: true,
            line_height: None,
            min_lines: 1,
            line_height_style: None,
            annotated: None,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// Horizontal alignment of each line (Compose's `textAlign`).
    pub fn align(mut self, align: crate::text::TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Maximum number of lines; the rest is cut (or ellipsized — see [`Self::overflow`]).
    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = max_lines;
        self
    }

    /// What to do when the text does not fit: `Clip`, `Ellipsis` or `Visible`.
    pub fn overflow(mut self, overflow: crate::text::TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    /// Whether the text wraps at the box width. `false` lays every line out at its own width.
    pub fn soft_wrap(mut self, soft_wrap: bool) -> Self {
        self.soft_wrap = soft_wrap;
        self
    }

    /// A fixed line height in logical pixels (Compose's `lineHeight`).
    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = Some(line_height);
        self
    }

    /// Minimum number of lines the box reserves room for.
    /// Compose's `lineHeightStyle`: where the run sits in a taller line, and what the ends trim.
    pub fn line_height_style(mut self, style: crate::text::LineHeightStyle) -> Self {
        self.line_height_style = Some(style);
        self
    }

    pub fn min_lines(mut self, min_lines: usize) -> Self {
        self.min_lines = min_lines;
        self
    }

    /// Lay out a pre-built [`crate::text::AnnotatedString`] — Compose's `Text(AnnotatedString)`.
    ///
    /// The value carries text plus style ranges, so it can be stored, sliced and reused; the build
    /// closure can still append more text after it.
    pub fn annotated(mut self, text: &crate::text::AnnotatedString) -> Self {
        self.annotated = Some(text.clone());
        self
    }

    /// Build the rich text. `f` receives a [`RichTextScope`] to call `.text()` / `.bold()` on.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, f: impl FnOnce(&mut RichTextScope)) {
        let mut content = String::new();
        let mut drawables: Vec<Arc<dyn InlineDrawable>> = Vec::new();
        let mut drawable_ranges: Vec<Range<usize>> = Vec::new();
        let mut annotations: Vec<(Style, Range<usize>)> = Vec::new();
        let base = resolve_base();

        // A pre-built annotated value comes first; the closure appends after it (the cursor starts
        // at its length, so the appended text's spans are offset correctly).
        let mut cursor_start = 0;
        if let Some(annotated) = &self.annotated {
            content.push_str(annotated.text());
            cursor_start = annotated.len();
            for (range, span) in annotated.span_styles() {
                annotations.push((span_style_to_style(span), range.clone()));
            }
        }

        {
            let mut scope = RichTextScope {
                content: &mut content,
                drawables: &mut drawables,
                drawable_ranges: &mut drawable_ranges,
                annotations: &mut annotations,
                style: base,
                cursor: cursor_start,
            };
            f(&mut scope);
        }

        // Resolve the span runs with winia v1's split algorithm.
        let spans = resolve_spans(&content, &drawable_ranges, &annotations);

        // Register with the selection container (drag-to-select), only inside a SelectionContainer's
        // provides scope. Registering borrows the content, so it happens before the move;
        // `set_current_node_registrar` has to come after `start_leaf`, when the desc exists.
        let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
        let key = ctx.next_key();
        let selection_disabled = crate::text::selection::LOCAL_SELECTION_DISABLED.current();
        ctx.changed(&selection_disabled);
        let reg_for_node = {
            if !selection_disabled {
                if let Some(reg) = ctx.selection_registrar().or_else(|| crate::text::selection::LOCAL_SELECTION_REGISTRAR.try_current()) {
                    reg.register(key, &content);
                    Some(reg)
                } else { None }
            } else { None }
        };

        let modifier = self.modifier.push(ModifierElement::RichTextContent {
            content,
            drawables,
            drawable_ranges,
            spans,
            align: match self.align {
                crate::text::TextAlign::Start => if rtl { crate::text::TextAlign::Right } else { crate::text::TextAlign::Left },
                crate::text::TextAlign::End => if rtl { crate::text::TextAlign::Left } else { crate::text::TextAlign::Right },
                other => other,
            },
            max_lines: self.max_lines,
            overflow: self.overflow,
            soft_wrap: self.soft_wrap,
            line_height: self.line_height,
            min_lines: self.min_lines,
            line_height_style: self.line_height_style,
        });
        ctx.start_leaf(key, modifier);
        ctx.set_current_node_selection_color(
            crate::text::selection::LOCAL_TEXT_SELECTION_COLORS.current().background,
        );
        if let Some(reg) = reg_for_node {
            ctx.set_current_node_registrar(reg);
        } else {
            ctx.clear_current_node_registrar();
        }
        ctx.end_node();
    }
}

impl Default for RichText { fn default() -> Self { Self::new() } }

// ── Span resolution, winia v1 style ──

#[derive(Clone)]
struct Seg {
    range: Range<usize>,
    placeholder: bool,
    /// A resolved style: `None` means "use the default", and a later `apply` overrides.
    /// It EMBEDS a `Style` instead of mirroring its fields, so a new style field is added in one
    /// place only.
    style: Style,
}

impl Seg {
    fn default_base(fs: f32, color: Color, fw: FontWeight, slant: FontSlant) -> Self {
        Seg {
            range: 0..0,
            placeholder: false,
            style: Style {
                fs: Some(fs), color: Some(color), fw: Some(fw), slant: Some(slant),
                ..Style::default()
            },
        }
    }

    fn clone_at(&self, range: Range<usize>) -> Self {
        Seg { range, ..self.clone() }
    }
}

/// Map one [`crate::text::SpanStyle`] onto the internal `Style` the span resolver consumes.
///
/// `font_size` arrives as a `TextUnit`, which needs the density to become the logical pixels the
/// resolver works in; everything else is a direct field.
fn span_style_to_style(span: &crate::text::SpanStyle) -> Style {
    let mut style = Style::default();
    style.fs = span
        .font_size
        .map(|u| u.to_logical_px(crate::runtime::density::current_density()));
    style.color = span.color;
    style.fw = span.font_weight;
    style.slant = span.font_style;
    style.letter_spacing = span.letter_spacing.unwrap_or(crate::unit::TextUnit::Sp(crate::unit::Sp(0.0)));
    style.bg = span.background;
    if let Some(decoration) = span.decoration {
        style.ul = decoration.underline;
        style.st = decoration.line_through;
    }
    style
}

#[cfg(test)]
pub(crate) fn resolve_spans_for_test(
    content: &str,
    drawable_ranges: &[std::ops::Range<usize>],
    annotations: &[(Style, std::ops::Range<usize>)],
) -> Vec<crate::text::RichSpanStyle> {
    resolve_spans(content, drawable_ranges, annotations)
}

#[cfg(test)]
pub(crate) fn style_for_test(font_size: f32, letter_spacing: crate::unit::TextUnit) -> Style {
    let mut style = Style::default();
    style.fs = Some(font_size);
    style.letter_spacing = letter_spacing;
    style
}

pub(crate) fn resolve_spans(content: &str, drawable_ranges: &[Range<usize>], annotations: &[(Style, Range<usize>)]) -> Vec<RichSpanStyle> {
    let base = resolve_base();
    let d_color = base.color.unwrap_or(Color::from_argb(255, 255, 255, 255));
    let d_fs = base.fs.unwrap_or(14.0);
    let d_fw = base.fw.unwrap_or(FontWeight::NORMAL);
    let d_sl = base.slant.unwrap_or(FontSlant::Upright);

    // 1) Build the public segments.
    let mut segs: Vec<Seg> = Vec::new();
    let mut di = 0usize;
    let total = content.chars().count();
    let mut ci = 0usize;
    while ci < total {
        // Is `ci` inside the current drawable range?
        if di < drawable_ranges.len() && ci >= drawable_ranges[di].start && ci < drawable_ranges[di].end {
            // The whole drawable range becomes one placeholder segment.
            let range = &drawable_ranges[di];
            let mut s = Seg::default_base(d_fs, d_color, d_fw, d_sl);
            s.range = range.start..range.end;
            s.placeholder = true;
            segs.push(s);
            ci = range.end;
            di += 1;
        } else {
            let run_start = ci;
            while ci < total {
                if di < drawable_ranges.len() && ci >= drawable_ranges[di].start {
                    break;
                }
                ci += 1;
            }
            let run_len = ci - run_start;
            if run_len > 0 {
                let mut s = Seg::default_base(d_fs, d_color, d_fw, d_sl);
                s.range = run_start..run_start+run_len;
                segs.push(s);
            }
        }
    }

    // 2) v1's split pass.
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
                segs[i].style.apply(attr);
                i += 1;
            } else if a_start > s_start && a_start < s_end && a_end < s_end {
                let mut mid = segs[i].clone_at(a_start..a_end);
                mid.style.apply(attr);
                let right = segs[i].clone_at(a_end..s_end);
                segs[i].range.end = a_start;
                segs.insert(i+1, mid);
                segs.insert(i+2, right);
                i += 3;
            } else if a_start > s_start && a_start < s_end {
                let mut right = segs[i].clone_at(a_start..s_end);
                right.style.apply(attr);
                segs[i].range.end = a_start;
                segs.insert(i+1, right);
                i += 2;
            } else if a_end > s_start && a_end < s_end {
                let mut left = segs[i].clone_at(s_start..a_end);
                left.style.apply(attr);
                segs[i].range.start = a_end;
                segs.insert(i, left);
                i += 2;
            } else { i += 1; }
        }
    }

    // 3) Turn them into `RichSpanStyle`.
    segs.into_iter().filter(|s| !s.placeholder).map(|s| RichSpanStyle {
        start: s.range.start, end: s.range.end,
        font_size: s.style.fs.unwrap_or(d_fs),
        color: s.style.color.unwrap_or(d_color),
        font_weight: s.style.fw.unwrap_or(d_fw),
        font_style: s.style.slant.unwrap_or(d_sl),
        underline: s.style.ul, overline: s.style.ol, strikethrough: s.style.st,
        decoration_color: s.style.deco_color,
        decoration_style: s.style.deco_style,
        decoration_mode: s.style.deco_mode,
        baseline_shift: s.style.baseline_shift,
        // Resolved against THIS span's size: an `em` is a multiple of it (Compose's semantics).
        letter_spacing: match s.style.letter_spacing {
            crate::unit::TextUnit::Em(em) => s.style.fs.unwrap_or(d_fs) * em,
            other => other.to_logical_px(crate::runtime::density::current_density()),
        },
        word_spacing: s.style.word_spacing,
        height_multiple: s.style.height_multiple, half_leading: s.style.half_leading,
        font_families: s.style.font_families, font_width: s.style.font_width,
        font_edging: s.style.font_edging, font_hinting: s.style.font_hinting,
        subpixel: s.style.subpixel,
        foreground_color: s.style.foreground_color, background: s.style.bg,
        locale: s.style.locale,
    }).collect()
}

impl Style {
    /// Overlay another style's SET fields onto this one — how nested `bold`/`italic` scopes compose.
    /// The old `apply_seg` copied 22 fields by hand; with `Style` embedded this is the only merge
    /// point left.
    fn apply(&mut self, other: &Style) {
        if let Some(v) = other.fs { self.fs = Some(v); }
        if let Some(v) = other.color { self.color = Some(v); }
        if let Some(v) = other.fw { self.fw = Some(v); }
        if let Some(v) = other.slant { self.slant = Some(v); }
        if other.ul { self.ul = true; }
        if other.ol { self.ol = true; }
        if other.st { self.st = true; }
        if let Some(v) = other.deco_color { self.deco_color = Some(v); }
        if let Some(v) = other.deco_style { self.deco_style = Some(v); }
        if let Some(v) = other.deco_mode { self.deco_mode = Some(v); }
        if other.baseline_shift != 0.0 { self.baseline_shift = other.baseline_shift; }
        if other.letter_spacing != crate::unit::TextUnit::Sp(crate::unit::Sp(0.0)) {
            self.letter_spacing = other.letter_spacing;
        }
        if other.word_spacing != 0.0 { self.word_spacing = other.word_spacing; }
        if other.height_multiple != 0.0 { self.height_multiple = other.height_multiple; }
        if other.half_leading { self.half_leading = true; }
        if !other.font_families.is_empty() { self.font_families = other.font_families.clone(); }
        if other.font_width != 5 { self.font_width = other.font_width; }
        if let Some(v) = other.font_edging { self.font_edging = Some(v); }
        if let Some(v) = other.font_hinting { self.font_hinting = Some(v); }
        if other.subpixel { self.subpixel = true; }
        if let Some(v) = other.foreground_color { self.foreground_color = Some(v); }
        if let Some(v) = other.bg { self.bg = Some(v); }
        if let Some(v) = &other.locale { self.locale = Some(v.clone()); }
    }
}


#[cfg(test)]
mod tests;
