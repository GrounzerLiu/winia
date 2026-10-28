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
///
/// Compose's `TextAlign`. `Start`/`End` are resolved against the layout direction when a leaf is
/// built (`Text::build`), so the payload and the paint path only ever see the physical four — which
/// is what makes them safe under RTL without every call site having to ask for the direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign { Left, Center, Right, Justify, Start, End }
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
    /// Cut off, with an ellipsis at the END of the line (Compose's `TextOverflow.Ellipsis`).
    Ellipsis,
    /// Cut off, with an ellipsis at the START of the line (Compose's `TextOverflow.StartEllipsis`).
    ///
    /// Skia's `ParagraphStyle` can only ellipsize the END of a line (`set_ellipsis`), so this is
    /// emulated: the text is truncated to the longest tail that fits with a leading ellipsis and the
    /// truncated string is what gets laid out (see `truncate_for_ellipsis`). The consequence worth
    /// knowing: the paragraph then holds the TRUNCATED text, so cursor and selection offsets map
    /// onto what is shown, not onto the original string.
    StartEllipsis,
    /// Cut off, with an ellipsis in the MIDDLE of the line (Compose's `TextOverflow.MiddleEllipsis`).
    /// Emulated the same way as [`TextOverflow::StartEllipsis`]: a prefix and a suffix are kept.
    MiddleEllipsis,
    /// NOT cut off: the glyphs are laid out at their intrinsic width and painted past the node's
    /// box (an ancestor that clips, such as a Button's shape, still clips them). This is Compose's
    /// `TextOverflow.Visible`, and it is what material3's button-group item uses for its label
    /// (`ButtonGroup.kt:1118-1124`) so that a press narrowing an item cannot cut the word it shows.
    Visible,
}
impl Default for TextOverflow { fn default() -> Self { TextOverflow::Clip } }

impl TextOverflow {
    /// Whether this overflow truncates with an ellipsis (Compose's `isEllipsis`).
    pub fn is_ellipsis(self) -> bool {
        matches!(
            self,
            TextOverflow::Ellipsis | TextOverflow::StartEllipsis | TextOverflow::MiddleEllipsis
        )
    }
}

/// How the glyphs are filled — Compose's `TextStyle.drawStyle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextDrawStyle {
    /// Filled glyphs (the default).
    Fill,
    /// Outlined glyphs: only the outline is painted, `width` logical pixels thick.
    Stroke { width: f32 },
}
impl Default for TextDrawStyle { fn default() -> Self { TextDrawStyle::Fill } }

/// A paint for the glyphs that a plain colour cannot express — Compose's `TextStyle.brush`.
///
/// Only the linear gradient is modelled: it is the brush every Material surface uses for a
/// decorative headline, and it is enough to prove the foreground-paint path works. The colours are
/// distributed evenly from `start` to `end`, in node-local logical pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBrush {
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub colors: Vec<Color>,
}

impl TextBrush {
    pub fn linear_gradient(start: (f32, f32), end: (f32, f32), colors: Vec<Color>) -> Self {
        Self { start, end, colors }
    }
}

/// Compose's `TextGeometricTransform`: a horizontal scale and skew applied to the glyph run.
///
/// Both are horizontal only, like Compose's. `scale_x` widens the run and the box it measures
/// (1.0 is identity); `skew_x` leans the glyphs, shifting the top of the run by `-skew_x * y`.
/// The paint applies them as a canvas matrix around the text origin, and every geometry reader
/// (caret, hit test, selection rectangles, the measured width) applies the same mapping — see
/// `TextGeometry` in `render::pass`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextGeometricTransform {
    pub scale_x: f32,
    pub skew_x: f32,
}

impl TextGeometricTransform {
    pub fn new(scale_x: f32, skew_x: f32) -> Self {
        Self { scale_x, skew_x }
    }
    /// Whether this transform leaves the glyphs alone.
    pub fn is_identity(&self) -> bool {
        (self.scale_x - 1.0).abs() < f32::EPSILON && self.skew_x.abs() < f32::EPSILON
    }
    /// A paragraph-space x → node-space x, for a glyph at `y` (the skew term is vertical).
    pub fn map_x(&self, x: f32, y: f32) -> f32 {
        x * self.scale_x - self.skew_x * y
    }
    /// The inverse: a node-space x → paragraph-space x, at `y`.
    pub fn unmap_x(&self, x: f32, y: f32) -> f32 {
        if self.scale_x.abs() < f32::EPSILON { return x; }
        (x + self.skew_x * y) / self.scale_x
    }
}

/// A text shadow — Compose's `Shadow`: a colour, an offset and a blur radius.
///
/// Compose carries one optional shadow per `TextStyle` (`shadow: Shadow?`) and appends it to the
/// glyph run, so the shadow follows the glyphs through wrapping and ellipsis. `blur` is Compose's
/// `blurRadius` in logical pixels; Skia wants a sigma, and the conversion is Skia's own
/// `SkBlurMask::ConvertRadiusToSigma` (radius * 0.57735 + 0.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShadow {
    pub color: Color,
    pub offset: (f32, f32),
    pub blur: f32,
}

impl TextShadow {
    pub fn new(color: Color, offset: (f32, f32), blur: f32) -> Self {
        Self { color, offset, blur }
    }

    /// The sigma Skia's paragraph wants for this blur radius.
    pub fn blur_sigma(&self) -> f64 {
        if self.blur <= 0.0 { 0.0 } else { (self.blur * 0.577_35 + 0.5) as f64 }
    }
}

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
    /// The minimum number of lines the box reserves — Compose's `TextStyle.minLines`. Callers must
    /// keep `1 <= min_lines <= max_lines`; `Text` enforces it the way Compose does.
    pub min_lines: Option<usize>,
    /// A font family name resolved against the platform's font collection — Compose's
    /// `TextStyle.fontFamily`, as a name here (winia has no `FontFamily` resolver type yet).
    pub font_family: Option<String>,
    /// The line decoration, or `None` for "this style says nothing about it".
    ///
    /// Compose's `TextStyle.textDecoration` is nullable, and that nullability is the whole point of
    /// D-1: a child style that specifies `TextDecoration.None` must be able to turn off an
    /// underline it inherited from `ProvideTextStyle`. Two `bool`s could not express that — "the
    /// child said nothing" and "the child said none" were the same value, so the merge ORed them.
    pub decoration: Option<TextDecoration>,
    /// 背景色（仅 RichText 生效）
    pub background: Option<Color>,
    /// Compose's `TextStyle.shadow`: one optional glyph shadow.
    pub shadow: Option<TextShadow>,
    /// Compose's `TextStyle.fontFeatureSettings`: an OpenType feature string such as `"smcp"`, or
    /// `"liga" off`. Parsed by [`parse_font_features`] into (tag, value) pairs for Skia.
    pub font_feature_settings: Option<String>,
    /// Compose's `TextStyle.drawStyle`: fill (the default) or an outline.
    pub draw_style: Option<TextDrawStyle>,
    /// Compose's `TextStyle.brush`: a gradient that replaces the flat colour.
    pub brush: Option<TextBrush>,
    /// Compose's `TextStyle.textGeometricTransform`.
    pub text_geometric_transform: Option<TextGeometricTransform>,
}

impl TextStyle {
    pub fn new() -> Self {
        Self { color: None, font_size: None, font_weight: None, font_style: None, text_align: None, overflow: None, max_lines: None, soft_wrap: None, letter_spacing: None, line_height: None, min_lines: None, font_family: None, decoration: None, background: None, shadow: None, font_feature_settings: None, draw_style: None, brush: None, text_geometric_transform: None }
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
    /// The minimum number of lines the box reserves — Compose's `TextStyle.minLines`. Callers must
    /// keep `1 <= min_lines <= max_lines`; `Text` enforces it the way Compose does.
    pub fn min_lines(mut self, lines: usize) -> Self { self.min_lines = Some(lines); self }
    /// A font family name, resolved against the platform's font collection.
    pub fn font_family(mut self, family: impl Into<String>) -> Self { self.font_family = Some(family.into()); self }
    /// Compose's `TextStyle.shadow`.
    pub fn shadow(mut self, shadow: TextShadow) -> Self { self.shadow = Some(shadow); self }
    /// Compose's `TextStyle.fontFeatureSettings`, e.g. `"smcp"` or `"liga" off`.
    pub fn font_feature_settings(mut self, settings: impl Into<String>) -> Self { self.font_feature_settings = Some(settings.into()); self }
    /// Compose's `TextStyle.drawStyle` — `Fill` or an outline `Stroke { width }`.
    pub fn draw_style(mut self, style: TextDrawStyle) -> Self { self.draw_style = Some(style); self }
    /// Compose's `TextStyle.brush` — a linear gradient over the glyphs.
    pub fn brush(mut self, brush: TextBrush) -> Self { self.brush = Some(brush); self }
    /// Compose's `TextStyle.textGeometricTransform` — horizontal scale and skew.
    pub fn text_geometric_transform(mut self, t: TextGeometricTransform) -> Self { self.text_geometric_transform = Some(t); self }
    /// Compose's `TextStyle.textDecoration`; it drives the `underline` / `strikethrough` flags.
    pub fn text_decoration(mut self, decoration: TextDecoration) -> Self {
        self.decoration = Some(decoration);
        self
    }
    /// The decoration this style specifies, with `NONE` when it says nothing.
    pub fn decoration(&self) -> TextDecoration {
        self.decoration.unwrap_or(TextDecoration::NONE)
    }
    pub fn line_height(mut self, height: impl Into<TextUnit>) -> Self { self.line_height = Some(height.into()); self }
    /// Adds an underline to whatever decoration this style already specifies.
    pub fn underline(mut self) -> Self {
        let d = self.decoration.unwrap_or(TextDecoration::NONE);
        self.decoration = Some(d | TextDecoration::UNDERLINE);
        self
    }

    /// Adds a line-through to whatever decoration this style already specifies.
    pub fn strikethrough(mut self) -> Self {
        let d = self.decoration.unwrap_or(TextDecoration::NONE);
        self.decoration = Some(d | TextDecoration::LINE_THROUGH);
        self
    }
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
}

impl Default for TextStyle {
    fn default() -> Self { Self::new() }
}

/// Parse Compose's `fontFeatureSettings` string into the pairs Skia takes.
///
/// The syntax is a comma or space separated list. Each entry is a four-character OpenType tag,
/// optionally with a value: `"smcp"` (on, i.e. 1), `"liga" off` / `"liga" on`, `"tnum" 0`, and the
/// quoted form `"\"ss01\"" 1`. Tags longer than four characters are truncated the way CSS does it,
/// and an entry with no letters is skipped; the order is preserved because Skia applies the list in
/// order.
pub fn parse_font_features(settings: &str) -> Vec<(String, i32)> {
    let mut out: Vec<(String, i32)> = Vec::new();
    for part in settings.split([',', ' ']).filter(|p| !p.trim().is_empty()) {
        let token = part.trim();
        // A token is either a tag, or a value that belongs to the tag before it.
        match token.to_ascii_lowercase().as_str() {
            "on" => {
                if let Some(last) = out.last_mut() { last.1 = 1; }
                continue;
            }
            "off" => {
                if let Some(last) = out.last_mut() { last.1 = 0; }
                continue;
            }
            _ => {}
        }
        if let Ok(value) = token.parse::<i32>() {
            if let Some(last) = out.last_mut() { last.1 = value; }
            continue;
        }
        let tag = token.trim_matches('"');
        if tag.is_empty() {
            continue;
        }
        let tag: String = tag.chars().take(4).collect();
        out.push((tag, 1));
    }
    out
}

#[cfg(test)]
mod style_tests {
    use super::*;

    #[test]
    fn font_feature_settings_parse_the_compose_syntax() {
        // A bare tag is ON; `off`/`on`/a number apply to the tag before it; quotes are allowed.
        assert_eq!(parse_font_features("smcp"), vec![("smcp".to_string(), 1)]);
        assert_eq!(
            parse_font_features("liga off, smcp on"),
            vec![("liga".to_string(), 0), ("smcp".to_string(), 1)]
        );
        assert_eq!(parse_font_features("tnum 0"), vec![("tnum".to_string(), 0)]);
        assert_eq!(
            parse_font_features("\"ss01\" 1"),
            vec![("ss01".to_string(), 1)]
        );
        assert!(parse_font_features("").is_empty());
        assert!(parse_font_features("  ,  ").is_empty());
    }

    #[test]
    fn a_shadow_converts_its_radius_to_a_sigma() {
        // Skia's ConvertRadiusToSigma: radius * 0.57735 + 0.5; a zero radius stays a hard shadow.
        let hard = TextShadow::new(Color::BLACK, (1.0, 2.0), 0.0);
        assert_eq!(hard.blur_sigma(), 0.0);
        let soft = TextShadow::new(Color::BLACK, (1.0, 2.0), 4.0);
        let sigma = soft.blur_sigma();
        assert!((sigma - 2.8094).abs() < 0.01, "sigma for a 4 px radius: {sigma}");
    }
}
