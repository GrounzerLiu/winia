//! `Text` — Compose Material3's `Text`.
//!
//! - Colour precedence: `.color()` > `style.color` > `LocalTextStyle` > the theme's on_surface.
//! - `ProvideTextStyle` sets the default style for a subtree.
//! - A dedicated parameter (`font_size` and friends) beats the same field on `style`.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::composition_local::CompositionLocal;
use crate::modifier::Modifier;
use crate::graphics::{Color};
use crate::unit::TextUnit;
use crate::text::{
    FontSlant, FontWeight, LineHeightStyle, TextAlign, TextBrush, TextDecoration, TextDrawStyle,
    TextGeometricTransform, TextLayoutResult, TextOverflow, TextShadow, TextStyle,
};
use std::sync::{Arc, LazyLock};


// ═══════════════════════════════════════════════════════════
// LocalTextStyle — the subtree's default text style.
// ═══════════════════════════════════════════════════════════

pub(crate) static LOCAL_TEXT_STYLE: LazyLock<CompositionLocal<TextStyle>> = LazyLock::new(|| {
    CompositionLocal::new(|| TextStyle::default())
});

/// Provide a default text style to a subtree, MERGED with the inherited one rather than replacing
/// it — Compose's `ProvideTextStyle`.
#[allow(non_snake_case)]
pub fn ProvideTextStyle(style: TextStyle, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    let merged = merge_text_styles(&LOCAL_TEXT_STYLE.current(), &style);
    LOCAL_TEXT_STYLE.provides(merged, || {
        content(ctx);
    });
}

/// Merge two styles: a `Some` on the right wins over the left, so the right is the override.
fn merge_text_styles(base: &TextStyle, override_: &TextStyle) -> TextStyle {
    TextStyle {
        color: override_.color.or(base.color),
        font_size: override_.font_size.or(base.font_size),
        font_weight: override_.font_weight.or(base.font_weight),
        font_style: override_.font_style.or(base.font_style),
        text_align: override_.text_align.or(base.text_align),
        overflow: override_.overflow.or(base.overflow),
        max_lines: override_.max_lines.or(base.max_lines),
        soft_wrap: override_.soft_wrap.or(base.soft_wrap),
        letter_spacing: override_.letter_spacing.or(base.letter_spacing),
        line_height: override_.line_height.or(base.line_height),
        min_lines: override_.min_lines.or(base.min_lines),
        font_family: override_.font_family.clone().or_else(|| base.font_family.clone()),
        // Compose's `TextStyle.merge` REPLACES the decoration when the child specifies one, so a
        // child that says `TextDecoration.NONE` clears an inherited underline (D-1).
        decoration: override_.decoration.or(base.decoration),
        background: override_.background.or(base.background),
        shadow: override_.shadow.or(base.shadow),
        font_feature_settings: override_.font_feature_settings.clone().or_else(|| base.font_feature_settings.clone()),
        draw_style: override_.draw_style.or(base.draw_style),
        brush: override_.brush.clone().or_else(|| base.brush.clone()),
        text_geometric_transform: override_
            .text_geometric_transform
            .or(base.text_geometric_transform),
        line_height_style: override_.line_height_style.or(base.line_height_style),
    }
}

// ═══════════════════════════════════════════════════════════
// The `Text` component.
// ═══════════════════════════════════════════════════════════

#[derive(Clone)]
pub struct Text {
    content: String,
    modifier: Modifier,
    font_size: Option<TextUnit>,
    color: Option<Color>,
    font_weight: Option<FontWeight>,
    font_style: Option<FontSlant>,
    max_lines: Option<usize>,
    text_align: Option<TextAlign>,
    overflow: Option<TextOverflow>,
    style: Option<TextStyle>,
    soft_wrap: Option<bool>,
    letter_spacing: Option<TextUnit>,
    line_height: Option<TextUnit>,
    min_lines: Option<usize>,
    text_decoration: Option<TextDecoration>,
    font_family: Option<String>,
    /// Compose's `onTextLayout`: called after every measure with the layout the text produced.
    on_text_layout: Option<Arc<dyn Fn(&TextLayoutResult) + Send + Sync>>,
    /// The richer sink: the snapshot plus the paragraph, for the per-line queries.
    on_text_layout_layout: Option<Arc<dyn Fn(&TextLayoutResult, &crate::text::TextLayout<'_>) + Send + Sync>>,
    /// Compose's `autoSize`: the (min, max) logical-pixel window the fit search runs in.
    auto_size: Option<(f32, f32)>,
    /// Compose's `TextStyle.shadow`.
    shadow: Option<TextShadow>,
    /// Compose's `TextStyle.fontFeatureSettings`.
    font_feature_settings: Option<String>,
    /// Compose's `TextStyle.drawStyle`.
    draw_style: Option<TextDrawStyle>,
    /// Compose's `TextStyle.brush`.
    brush: Option<TextBrush>,
    /// Compose's `TextStyle.textGeometricTransform`.
    text_geometry: Option<TextGeometricTransform>,
    /// Compose's `TextStyle.lineHeightStyle`.
    line_height_style: Option<LineHeightStyle>,
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Text")
            .field("content", &self.content)
            .field("font_size", &self.font_size)
            .field("color", &self.color)
            .field("max_lines", &self.max_lines)
            .field("min_lines", &self.min_lines)
            .field("on_text_layout", &self.on_text_layout.is_some())
            .finish()
    }
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
            soft_wrap: None,
            letter_spacing: None,
            line_height: None,
            min_lines: None,
            text_decoration: None,
            font_family: None,
            on_text_layout: None,
            on_text_layout_layout: None,
            auto_size: None,
            shadow: None,
            font_feature_settings: None,
            draw_style: None,
            brush: None,
            text_geometry: None,
            line_height_style: None,
        }
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self { self.modifier = self.modifier.then(modifier); self }

    // The options follow Compose's parameter order (`material3/Text.kt:40-58`): colour, size, the face,
    // letter spacing, alignment, line height, overflow, wrapping, the line cap, then the style.

    /// Compose's `color` parameter — it beats the style and the content colour.
    pub fn color(mut self, color: Color) -> Self { self.color = Some(color); self }
    pub fn font_size(mut self, size: impl Into<TextUnit>) -> Self { self.font_size = Some(size.into()); self }
    pub fn font_weight(mut self, w: FontWeight) -> Self { self.font_weight = Some(w); self }
    pub fn bold(mut self) -> Self { self.font_weight = Some(FontWeight::BOLD); self }
    pub fn italic(mut self) -> Self { self.font_style = Some(FontSlant::Italic); self }
    pub fn oblique(mut self) -> Self { self.font_style = Some(FontSlant::Oblique); self }

    /// Compose's `fontFamily`: a family name resolved against the platform's font collection.
    pub fn font_family(mut self, family: impl Into<String>) -> Self { self.font_family = Some(family.into()); self }
    /// Compose's `TextStyle.shadow`: a colour, an offset and a blur radius, drawn under the glyphs.
    pub fn shadow(mut self, shadow: TextShadow) -> Self { self.shadow = Some(shadow); self }
    /// Compose's `TextStyle.fontFeatureSettings`, e.g. `"smcp"` or `"liga" off`.
    pub fn font_feature_settings(mut self, settings: impl Into<String>) -> Self { self.font_feature_settings = Some(settings.into()); self }
    /// Compose's `TextStyle.drawStyle` — `Fill` or an outline `Stroke { width }`.
    pub fn draw_style(mut self, style: TextDrawStyle) -> Self { self.draw_style = Some(style); self }
    /// Compose's `TextStyle.brush` — a linear gradient over the glyphs.
    pub fn brush(mut self, brush: TextBrush) -> Self { self.brush = Some(brush); self }
    /// Compose's `TextStyle.textGeometricTransform` — horizontal scale and skew of the glyph run.
    pub fn text_geometry(mut self, transform: TextGeometricTransform) -> Self {
        self.text_geometry = Some(transform);
        self
    }
    /// Compose's `TextStyle.lineHeightStyle` — where the text sits in a taller line box, and whether
    /// the leading at the ends is trimmed. Needs `.line_height(..)` to have something to distribute.
    pub fn line_height_style(mut self, style: LineHeightStyle) -> Self {
        self.line_height_style = Some(style);
        self
    }

    /// Letter spacing, a `TextUnit` (`sp`, or an `em` of the leaf's own size) — Compose's
    /// `TextStyle.letterSpacing`.
    /// Compose's `TextStyle.letterSpacing`: a `TextUnit`, so `0.1.em` and an `sp` both work.
    pub fn letter_spacing(mut self, spacing: impl Into<TextUnit>) -> Self {
        self.letter_spacing = Some(spacing.into());
        self
    }

    /// Compose's `textDecoration` — the plain leaf draws it now (it used to be RichText-only).
    pub fn text_decoration(mut self, decoration: TextDecoration) -> Self {
        self.text_decoration = Some(decoration);
        self
    }

    pub fn align(mut self, align: TextAlign) -> Self { self.text_align = Some(align); self }

    /// Compose's `autoSize`: shrink the font from `max` towards `min` until the text fits the box.
    ///
    /// The search runs at measure time, where the constraints are known; the chosen size is the
    /// largest one whose laid-out box fits both axes.
    pub fn auto_size(mut self, min: f32, max: f32) -> Self {
        self.auto_size = Some((min, max));
        self
    }

    /// Line height, a `TextUnit` (a bare `f32` means `sp`) — Compose's `TextStyle.lineHeight`.
    pub fn line_height(mut self, height: impl Into<TextUnit>) -> Self { self.line_height = Some(height.into()); self }

    pub fn overflow(mut self, overflow: TextOverflow) -> Self { self.overflow = Some(overflow); self }
    pub fn soft_wrap(mut self, wrap: bool) -> Self { self.soft_wrap = Some(wrap); self }
    pub fn max_lines(mut self, lines: usize) -> Self { self.max_lines = Some(lines); self }

    /// Compose's `minLines`: the box reserves at least this many lines even when the text is shorter.
    pub fn min_lines(mut self, lines: usize) -> Self { self.min_lines = Some(lines); self }

    /// Compose's `onTextLayout`: called after every measure with what the text laid out to.
    ///
    /// The payload compares callbacks by IDENTITY, so a closure built per rebuild is a changed
    /// parameter and the leaf re-measures. A caller that rebuilds often should create the sink once
    /// and hand it to `on_text_layout_shared` — that keeps the parameter equal, so the slot can Skip
    /// (`a_shared_callback_lets_the_leaf_skip` pins both halves).
    pub fn on_text_layout(
        mut self,
        callback: impl Fn(&TextLayoutResult) + Send + Sync + 'static,
    ) -> Self {
        self.on_text_layout = Some(Arc::new(callback));
        self
    }

    /// The same sink, shared: build the `Arc` once (e.g. with `ctx.remember`) and the payload stays
    /// equal across rebuilds.
    pub fn on_text_layout_shared(
        mut self,
        callback: Arc<dyn Fn(&TextLayoutResult) + Send + Sync>,
    ) -> Self {
        self.on_text_layout = Some(callback);
        self
    }

    /// Compose's richer `onTextLayout`: the same snapshot, PLUS a [`crate::text::TextLayout`] for the
    /// per-line queries Compose exposes on `TextLayoutResult` (`getLineForOffset`,
    /// `getLineStart`/`getLineEnd`, `getBoundingBox`, `getCursorRect`, `getOffsetForPosition`,
    /// `getWordBoundary`).
    ///
    /// The paragraph cannot ride the `Copy` snapshot — its Skia handle is not `Send` (measured by
    /// compiling `assert_send::<Paragraph>()`) — so those queries are answered while the paragraph is
    /// borrowed, and this callback is that window. It fires at the same point as `on_text_layout`
    /// (after the constraint clamp, once per measure) and does NOT replace it: a leaf may carry both.
    pub fn on_text_layout_with_layout(
        mut self,
        callback: impl Fn(&TextLayoutResult, &crate::text::TextLayout<'_>) + Send + Sync + 'static,
    ) -> Self {
        self.on_text_layout_layout = Some(Arc::new(callback));
        self
    }

    /// A base style; a dedicated parameter still wins over the same field here.
    pub fn style(mut self, style: TextStyle) -> Self { self.style = Some(style); self }

    /// Background colour and shape, delegated to `Modifier::background`.
    pub fn background(
        mut self,
        color: Color,
        shape: impl Into<crate::graphics::Dyn<crate::graphics::Shape>>,
    ) -> Self {
        self.modifier = self.modifier.background(color, shape);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_TEXT_TRACE").is_ok() {
                eprintln!("[text-build] content={:?}", self.content.get(..16.min(self.content.len())));
            }
        }
        ctx.changed(&self.content);
        let key = ctx.next_key();

        let base = LOCAL_TEXT_STYLE.current();
        let style = self.style.as_ref().map(|s| merge_text_styles(&base, s)).unwrap_or(base);

        // Compose's `TextUnitType.Em`: a multiple of the ENCLOSING size, so it resolves against
        // whatever `ProvideTextStyle` supplied (14 sp when nobody did), not against itself.
        let inherited_font_size = style
            .font_size
            .map(|u| u.to_logical_px(crate::runtime::density::current_density()))
            .unwrap_or(14.0);
        let final_font_size = match self.font_size.or(style.font_size) {
            Some(TextUnit::Em(em)) => TextUnit::Sp(crate::unit::Sp(inherited_font_size * em)),
            Some(other) => other,
            None => TextUnit::Sp(crate::unit::Sp(14.0)),
        };
        let final_font_weight = self.font_weight.or(style.font_weight).unwrap_or_default();
        let final_font_style = self.font_style.or(style.font_style).unwrap_or_default();
        let rtl = crate::layout::direction::current() == crate::layout::LayoutDirection::Rtl;
        let final_align = self.text_align.or(style.text_align).map(|a| match a {
            // Compose's `TextAlign.Start` / `End` are direction-relative; the measurement and paint
            // layers only understand physical edges, and the direction is known here.
            TextAlign::Start => if rtl { TextAlign::Right } else { TextAlign::Left },
            TextAlign::End => if rtl { TextAlign::Left } else { TextAlign::Right },
            other => other,
        }).unwrap_or_else(|| if rtl { TextAlign::Right } else { TextAlign::Left });
        let final_overflow = self.overflow.or(style.overflow).unwrap_or_default();
        let final_max_lines = self.max_lines.or(style.max_lines).unwrap_or(usize::MAX);
        let final_soft_wrap = self.soft_wrap.or(style.soft_wrap).unwrap_or(true);
        // `Em` has no absolute value until the size is known, so resolve in the same order the size
        // was: the enclosing size is the leaf's own final font size.
        let density = crate::runtime::density::current_density();
        let final_letter_spacing = self
            .letter_spacing
            .or(style.letter_spacing)
            .map(|u| match u {
                TextUnit::Em(em) => final_font_size.to_logical_px(density) * em,
                other => other.to_logical_px(density),
            })
            .unwrap_or(0.0);
        let final_line_height = self.line_height.or(style.line_height);
        let final_min_lines = self.min_lines.or(style.min_lines).unwrap_or(1);
        // Compose's `validateMinMaxLines` (`HeightInLinesModifier.kt:454-461`, called from
        // `BasicText.kt:102`): both caps have to be positive and `min <= max`. Without it a bad pair
        // silently produces a box that lies — `max_lines(0)` was measured at 44 x 27 (one line, with
        // `did_exceed_max_lines` set), and `min_lines(5)` with `max_lines(2)` reserved three lines the
        // text can never fill.
        assert!(
            final_min_lines > 0 && final_max_lines > 0,
            "both min_lines ({final_min_lines}) and max_lines ({final_max_lines}) must be greater than zero"
        );
        assert!(
            final_min_lines <= final_max_lines,
            "min_lines ({final_min_lines}) must be less than or equal to max_lines ({final_max_lines})"
        );
        let final_decoration = self
            .text_decoration
            .or(style.decoration)
            .unwrap_or(TextDecoration::NONE);
        let final_font_family = self.font_family.clone().or_else(|| style.font_family.clone());
        let final_shadow = self.shadow.or(style.shadow);
        let final_font_features = self
            .font_feature_settings
            .clone()
            .or_else(|| style.font_feature_settings.clone());
        let final_draw_style = self.draw_style.or(style.draw_style);
        let final_transform = self
            .text_geometry
            .or(style.text_geometric_transform);
        let final_line_height_style = self.line_height_style.or(style.line_height_style);
        let final_brush = self.brush.clone().or_else(|| style.brush.clone());

        // Colour precedence: an explicit `color` > `LOCAL_TEXT_STYLE` > the CONTENT colour
        // (`WiniaTheme::content_color`, Compose's `LocalContentColor`: a `Text` defaults to it, so
        // `with_content_color` carries into a subtree) > the theme's on_surface. The content-colour
        // step was missing: a Tooltip supplies `with_content_color(inverse_on_surface)` and the
        // Text still fell through to on_surface, and the same was true of Button/Chip.
        let final_color = self.color
            .or(style.color)
            .unwrap_or_else(|| crate::theme::WiniaTheme::content_color());

        let content_len = self.content.len();
        let selection_disabled = crate::text::selection::LOCAL_SELECTION_DISABLED.current();
        ctx.changed(&selection_disabled);
        // Register with the enclosing container unless DisableSelection is in scope.
        let (reg_for_node, registered_off) = {
            if !selection_disabled {
                if let Some(reg) = ctx.selection_registrar().or_else(|| crate::text::selection::LOCAL_SELECTION_REGISTRAR.try_current()) {
                    let off = reg.register(key, &self.content);
                    (Some(reg), Some((key, content_len, off)))
                } else { (None, None) }
            } else { (None, None) }
        };
        // Declared parameters, one per visual input: Colour, size, weight, slant, family, spacing,
        // line height, the line caps, alignment, overflow, wrapping, decoration and the auto-size
        // window. The leaf's payload is compared too (see `Modifier::param_eq`), so this list is the
        // explicit half of the same contract — a parameter that is only in the payload works through
        // the materialiser's comparison, and one declared here also marks the slot.
        ctx.changed(&self.content);
        ctx.changed(&final_font_size);
        ctx.changed(&final_color);
        ctx.changed(&final_font_weight);
        ctx.changed(&final_font_style);
        ctx.changed(&final_font_family);
        ctx.changed(&final_letter_spacing);
        ctx.changed(&final_line_height);
        ctx.changed(&final_max_lines);
        ctx.changed(&final_min_lines);
        ctx.changed(&final_align);
        ctx.changed(&final_overflow);
        ctx.changed(&final_soft_wrap);
        ctx.changed(&final_decoration);
        ctx.changed(&self.auto_size);
        ctx.changed(&final_shadow);
        ctx.changed(&final_font_features);
        ctx.changed(&final_draw_style);
        ctx.changed(&final_brush);
        ctx.changed(&final_transform);
        ctx.changed(&final_line_height_style);

        let modifier = self.modifier.text_content_full(
            self.content,
            final_font_size.to_logical_px(density),
            final_color,
            final_font_weight,
            final_font_style,
            final_max_lines,
            final_align,
            final_overflow,
            final_soft_wrap,
            final_letter_spacing,
            final_line_height.map(|u| u.to_logical_px(density)),
            crate::modifier::TextExtras {
                on_text_layout_layout: self.on_text_layout_layout.clone(),
                shadow: final_shadow,
                font_feature_settings: final_font_features.clone(),
                draw_style: final_draw_style,
                brush: final_brush.clone(),
                transform: final_transform,
                line_height_style: final_line_height_style,
                min_lines: final_min_lines,
                underline: final_decoration.underline,
                line_through: final_decoration.line_through,
                font_family: final_font_family,
                on_text_layout: self.on_text_layout.clone(),
                auto_size: self.auto_size,
            },
        );

        ctx.start_leaf(key, modifier);
        // The highlight colour, from the nearest provider (Compose's `LocalTextSelectionColors`).
        ctx.set_current_node_selection_color(
            crate::text::selection::LOCAL_TEXT_SELECTION_COLORS.current().background,
        );
        if let Some(reg) = reg_for_node {
            ctx.set_current_node_registrar(reg);
        } else {
            ctx.clear_current_node_registrar();
        }
        if let Some((_k, _len, _off)) = registered_off {
            debug_log!("[selection] Text registered: slot_key={} len={} global_off={}", _k, _len, _off);
        }
        ctx.end_node();
    }

    // ── Getters ──
    pub fn get_content(&self) -> &str { &self.content }
    pub fn get_font_size(&self) -> Option<TextUnit> { self.font_size }
    pub fn get_color(&self) -> Option<Color> { self.color }
    pub fn get_text_align(&self) -> Option<TextAlign> { self.text_align }
    pub fn get_overflow(&self) -> Option<TextOverflow> { self.overflow }
    pub fn get_max_lines(&self) -> Option<usize> { self.max_lines }
    pub fn get_min_lines(&self) -> Option<usize> { self.min_lines }
    pub fn get_font_family(&self) -> Option<&str> { self.font_family.as_deref() }
    pub fn get_text_decoration(&self) -> Option<TextDecoration> { self.text_decoration }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::ModifierElement;

/// A plain `Text` must not carry the paint element: it says nothing, and an always-present
    /// element costs a parameter comparison on every recomposition and a slot on every text node.
    #[test]
    fn a_plain_text_carries_no_paint_element() {
        fn has_paint(text: Text) -> bool {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(move |ctx| { text.build(ctx); });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root]
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, ModifierElement::TextPaint { .. }))
        }
        assert!(!has_paint(Text::new("plain")), "a plain text carries only its content element");
        assert!(has_paint(Text::new("shadowed").shadow(TextShadow::new(Color::BLACK, (1.0, 1.0), 2.0))), "a shadow needs the element");
        assert!(
            has_paint(Text::new("scaled").text_geometry(TextGeometricTransform::new(2.0, 0.0))),
            "a transform needs the element"
        );
    }
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
        assert_eq!(text.get_font_size(), Some(crate::unit::TextUnit::Sp(crate::unit::Sp(24.0))));
        assert_eq!(text.get_color(), Some(Color::RED));
        assert_eq!(text.get_text_align(), Some(TextAlign::Center));
        assert_eq!(text.get_overflow(), Some(TextOverflow::Ellipsis));
        assert_eq!(text.get_max_lines(), Some(3));
        assert_eq!(text.get_modifier().elements().len(), 1);
    }

    #[test]
    fn provide_text_style_inherits_and_builder_overrides_typography() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            ProvideTextStyle(
                TextStyle::new()
                    .font_size(16.0)
                    .letter_spacing(0.5)
                    .line_height(24.0)
                    .soft_wrap(false),
                ctx,
                |ctx| {
                    ProvideTextStyle(TextStyle::new().letter_spacing(0.2), ctx, |ctx| {
                        Text::new("styled")
                            .letter_spacing(1.0)
                            .build(ctx);
                    });
                },
            );
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let (font_size, soft_wrap, letter_spacing, line_height) = nodes[root].modifier.elements().iter().find_map(|el| {
            if let ModifierElement::TextContent { font_size, soft_wrap, letter_spacing, line_height, .. } = el {
                Some((*font_size, *soft_wrap, *letter_spacing, *line_height))
            } else {
                None
            }
        }).unwrap();
        assert_eq!(font_size, 16.0, "子 provider 应继承父级字号");
        assert!(!soft_wrap, "子 provider 应继承父级 soft_wrap");
        assert_eq!(letter_spacing, 1.0, "Text builder 字距优先级最高");
        assert_eq!(line_height, Some(24.0), "子 provider 应继承父级行高");
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

    #[test]
    fn test_text_letter_spacing_and_line_height() {
        let text = Text::new("spacing").letter_spacing(2.0).line_height(28.0);
        assert_eq!(text.letter_spacing, Some(TextUnit::Sp(crate::unit::Sp(2.0))));
        // The same knob in `em`: a multiple of the leaf's own font size, resolved at build time.
        let em = Text::new("x").font_size(20.0).letter_spacing(TextUnit::Em(0.1));
        assert_eq!(em.letter_spacing, Some(TextUnit::Em(0.1)));
        assert_eq!(text.line_height, Some(TextUnit::Sp(crate::unit::Sp(28.0))));
    }

    #[test]
    fn test_text_content_full_carries_typography() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("styled").letter_spacing(1.5).line_height(30.0).build(ctx);
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let m = &nodes[root].modifier;
        let (ls, lh) = m.elements().iter().find_map(|el| {
            if let ModifierElement::TextContent { letter_spacing, line_height, .. } = el {
                Some((*letter_spacing, *line_height))
            } else {
                None
            }
        }).unwrap();
        assert_eq!(ls, 1.5, "letter_spacing 传入 TextContent 元素");
        assert_eq!(lh, Some(30.0), "line_height 传入 TextContent 元素");
    }

    /// The measured height follows the font size linearly, which is what makes a label size
    /// animation visible at all: a 12 sp label is about three quarters the height of a 16 sp one,
    /// and a measure that ignored the size would animate nothing.
    #[test]
    fn test_measured_height_scales_with_font_size() {
        use crate::modifier::ModifierElement;
        let measure = |font_size: f32| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Text::new("Name").font_size(font_size).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 500.0, 0.0, 500.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size.height
        };
        let h16 = measure(16.0);
        let h12 = measure(12.0);
        assert!(
            h12 < h16 && (h12 / h16 - 0.75).abs() < 0.15,
            "12sp 高度应约为 16sp 的 3/4（实际 h16={} h12={}）",
            h16, h12
        );
    }

    /// Compose's `minLines`: the box reserves the lines even though the text is shorter.
    #[test]
    fn min_lines_reserves_the_box_height() {
        let measure = |lines: usize| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Text::new("Name").font_size(20.0).min_lines(lines).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 500.0, 0.0, 500.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size.height
        };
        let one = measure(1);
        let three = measure(3);
        assert!(
            one > 0.0 && (three - one * 3.0).abs() < 1.0,
            "three reserved lines are three line boxes tall: one={one} three={three}"
        );
    }

    /// Compose's `onTextLayout`: the callback sees what the measure pass laid out.
    #[test]
    fn on_text_layout_reports_what_was_laid_out() {
        let seen: std::sync::Arc<std::sync::Mutex<Option<crate::text::TextLayoutResult>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = seen.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("one line")
                .font_size(20.0)
                .on_text_layout(move |result| {
                    *sink.lock().unwrap() = Some(*result);
                })
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 500.0, 0.0, 500.0));
        let result = seen.lock().unwrap().expect("the callback ran during measure");
        assert_eq!(result.line_count, 1, "one line of text: {result:?}");
        assert!(result.size.height > 0.0 && result.size.width > 0.0, "{result:?}");
        assert!(!result.has_visual_overflow(), "nothing was cut off: {result:?}");
    }

    /// Compose's `TextLayoutResult.size` is the CONSTRAINED box, not the paragraph's own width.
    ///
    /// Measured before the fix: a `fill_max_width` text in a 300 dp box reported width 12, because
    /// the callback fired inside `measure_and_cache_text` — before the engine clamped the size.
    #[test]
    fn on_text_layout_reports_the_constrained_size() {
        let seen: std::sync::Arc<std::sync::Mutex<Option<crate::text::TextLayoutResult>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = seen.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("hi")
                .on_text_layout(move |result| {
                    *sink.lock().unwrap() = Some(*result);
                })
                .modifier(Modifier::new().fill_max_width())
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let measured = composer.arena_nodes()[root].measured_size;
        let result = seen.lock().unwrap().expect("the callback ran during measure");
        assert!(
            (result.size.width - measured.width).abs() < 0.01,
            "the callback reports the box the node measured: result={result:?} node={measured:?}"
        );
        assert_eq!(result.size.width, 300.0, "fill_max_width fills the constraint: {result:?}");
        assert_eq!(
            result.text_size.width, 12.0,
            "text_size stays the paragraph's own width: {result:?}"
        );
    }

    /// Compose's `TextStyle.shadow` is paint-only: it must not change the measured box.
    ///
    /// Skia draws shadows behind the glyphs and keeps them out of the metrics, which is what makes a
    /// shadow safe to add to a layout that already fits.
    #[test]
    fn a_text_shadow_does_not_change_the_measured_box() {
        fn measure(with_shadow: bool) -> crate::unit::Size {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(move |ctx| {
                let text = Text::new("hello shadow").font_size(18.0);
                let text = if with_shadow {
                    text.shadow(TextShadow::new(Color::BLACK, (3.0, 3.0), 2.0))
                } else {
                    text
                };
                text.build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size
        }
        let plain = measure(false);
        let shadowed = measure(true);
        assert_eq!(plain, shadowed, "a shadow is paint-only: plain={plain:?} shadowed={shadowed:?}");
    }

    /// A shadow change is a changed parameter, so the leaf re-enters and the cached paragraph (which
    /// carries the shadow) is rebuilt. Compared through `Modifier::param_eq`, the Skip gate.
    #[test]
    fn a_shadow_and_a_feature_string_are_part_of_the_skip_parameter_comparison() {
        let base = Modifier::new().text_content_full(
            "x".to_string(), 16.0, Color::BLACK, FontWeight::NORMAL, FontSlant::Upright,
            1, TextAlign::Left, TextOverflow::Clip, true, 0.0, None,
            crate::modifier::TextExtras {
                shadow: None,
                font_feature_settings: None,
                ..Default::default()
            },
        );
        let shadow = Modifier::new().text_content_full(
            "x".to_string(), 16.0, Color::BLACK, FontWeight::NORMAL, FontSlant::Upright,
            1, TextAlign::Left, TextOverflow::Clip, true, 0.0, None,
            crate::modifier::TextExtras {
                shadow: Some(TextShadow::new(Color::BLACK, (1.0, 1.0), 0.0)),
                font_feature_settings: None,
                ..Default::default()
            },
        );
        let features = Modifier::new().text_content_full(
            "x".to_string(), 16.0, Color::BLACK, FontWeight::NORMAL, FontSlant::Upright,
            1, TextAlign::Left, TextOverflow::Clip, true, 0.0, None,
            crate::modifier::TextExtras {
                shadow: None,
                font_feature_settings: Some("smcp".to_string()),
                ..Default::default()
            },
        );
        assert!(base.param_eq(&base), "identical modifiers are the same parameter set");
        assert!(!base.param_eq(&shadow), "a new shadow must re-enter the leaf");
        assert!(!base.param_eq(&features), "a new feature string must re-enter the leaf");
    }

    /// Compose's `textGeometricTransform` — the horizontal mapping is an invertible pair, and a
    /// scale widens the MEASURED box (the run draws wider, so the box must know it).
    #[test]
    fn a_geometric_transform_scales_the_measured_width_and_maps_back() {
        use crate::text::TextGeometricTransform;
        let plain = {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Text::new("wide").font_size(20.0).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].measured_size
        };
        let scaled = {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Text::new("wide")
                    .font_size(20.0)
                    .text_geometry(TextGeometricTransform::new(2.0, 0.0))
                    .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let node = &composer.arena_nodes()[root];
            let para = node.cached_paragraph.borrow();
            let para = para.as_ref().expect("measured");
            (node.measured_size, para.max_intrinsic_width())
        };
        assert_eq!(plain.height, scaled.0.height, "a horizontal transform leaves the height alone");
        assert!(
            (scaled.0.width - plain.width * 2.0).abs() <= 2.0,
            "scale 2 doubles the measured box: plain={:?} scaled={:?} (paragraph at {:.1})",
            plain, scaled.0, scaled.1
        );
        let t = TextGeometricTransform::new(2.0, 0.3);        for x in [0.0, 5.0, 40.0] {
            for y in [0.0, 8.0, 20.0] {
                let mapped = t.map_x(x, y);
                let back = t.unmap_x(mapped, y);
                assert!((back - x).abs() < 0.001, "map/unmap must invert: x={x} y={y} back={back}");
            }
        }
        assert!(t.is_identity() == false);
        assert!(TextGeometricTransform::new(1.0, 0.0).is_identity());
    }

    /// `LineHeightStyle.Trim` removes the extra leading from the box HEIGHT; the alignment decides
    /// how the extra is split. Both come out of the same helper, so this pins all four combinations.
    #[test]
    fn a_line_height_style_trims_the_box_and_places_the_run() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T};
        fn geometry(style: LineHeightStyle) -> (f32, f32, f32, f32) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(move |ctx| {
                Text::new("Ag")
                    .font_size(16.0)
                    .line_height(24.0)
                    .line_height_style(style)
                    .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let node = &composer.arena_nodes()[root];
            let para = node.cached_paragraph.borrow();
            let geo = crate::layout::node::line_geometry(node, para.as_ref().unwrap());
            (geo.paint_dy, geo.trim_top, geo.trim_bottom, node.measured_size.height)
        }
        let (centre_dy, centre_top, centre_bottom, centre_h) =
            geometry(LineHeightStyle::new(A::Center, T::None));
        assert_eq!((centre_dy, centre_top, centre_bottom), (0.0, 0.0, 0.0), "the default is the centred build");
        assert!((centre_h - 24.0).abs() < 1.0, "an untrimmed line keeps its line height: {centre_h}");
        let (_, _, _, trimmed_h) = geometry(LineHeightStyle::new(A::Center, T::Both));
        assert!(
            trimmed_h < centre_h - 2.0,
            "trimming both ends shrinks the box by the extra leading: none={centre_h} both={trimmed_h}"
        );

        let (top_dy, _, _, _) = geometry(LineHeightStyle::new(A::Top, T::None));
        let (bottom_dy, _, _, _) = geometry(LineHeightStyle::new(A::Bottom, T::None));
        assert!(top_dy < 0.0 && bottom_dy > 0.0, "Top lifts, Bottom drops: {top_dy} {bottom_dy}");
        assert!((top_dy + bottom_dy).abs() < 0.01, "the two are symmetric about centre: {top_dy} {bottom_dy}");

        let (both_dy, both_top, both_bottom, _) = geometry(LineHeightStyle::new(A::Center, T::Both));
        assert!(both_top > 0.0 && both_bottom > 0.0, "Both trims each end: {both_top} {both_bottom}");
        assert!((both_dy + both_top).abs() < 0.01, "the paint follows the top trim: {both_dy} {both_top}");
        assert!((both_top + both_bottom - (centre_top + centre_bottom + both_top + both_bottom)).abs() < 0.01);

        let (_, first_top, first_bottom, _) = geometry(LineHeightStyle::new(A::Center, T::FirstLineTop));
        assert!(first_top > 0.0 && first_bottom == 0.0, "only the top end: {first_top} {first_bottom}");
        let (_, last_top, last_bottom, _) = geometry(LineHeightStyle::new(A::Center, T::LastLineBottom));
        assert!(last_top == 0.0 && last_bottom > 0.0, "only the bottom end: {last_top} {last_bottom}");
    }

    /// The hit test and the paint must agree about where the glyphs are: a click lands in the
    /// paragraph's own space, so the line-height offset has to come back out before asking Skia.
    ///
    /// A three-line paragraph in a `Bottom`-aligned 34 px line discriminates at the paragraph's own
    /// line boundary (y = 34): with the offset removed the click is still inside the FIRST line
    /// (cluster 0), and without it the click jumps to the second line (cluster 4).
    #[test]
    fn a_line_height_offset_is_taken_back_out_of_the_pointer() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T};
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("one\ntwo\nthree")
                .font_size(18.0)
                .line_height(34.0)
                .line_height_style(LineHeightStyle::new(A::Bottom, T::None))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let node = &composer.arena_nodes()[root];
        let para = node.cached_paragraph.borrow();
        let para = para.as_ref().unwrap();
        let dy = crate::layout::node::line_geometry(node, para).paint_dy;
        assert!(dy > 1.0, "Bottom alignment drops the run inside the box: dy={dy}");
        let layout = crate::text::TextLayout::new(para);
        let click_y = 34.0;
        let mapped = layout.get_closest_grapheme_cluster_at(skia_safe::Point::new(2.0, click_y - dy));
        let unmapped = layout.get_closest_grapheme_cluster_at(skia_safe::Point::new(2.0, click_y));
        assert!(mapped < 4, "with the offset removed the click stays on the first line: {mapped}");
        assert!(unmapped >= 4, "without it the same click jumps a line: {unmapped}");
    }

    /// The fallback path (no cached paragraph) must agree with the measured one about the
    /// line-height offset; both call `line_geometry_from`, and this pins the arithmetic itself.
    #[test]
    fn the_line_height_arithmetic_is_one_function() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T};
        let natural = 21.0;
        let centre = crate::layout::node::line_geometry_from(Some(24.0), Some(LineHeightStyle::new(A::Center, T::None)), natural);
        assert_eq!(centre.paint_dy, 0.0, "centred is the build's own position");
        let top = crate::layout::node::line_geometry_from(Some(24.0), Some(LineHeightStyle::new(A::Top, T::None)), natural);
        assert!((top.paint_dy + 1.5).abs() < 0.01, "Top lifts by half the extra: {}", top.paint_dy);
        let both = crate::layout::node::line_geometry_from(Some(24.0), Some(LineHeightStyle::new(A::Center, T::Both)), natural);
        assert!((both.trim_top - 1.5).abs() < 0.01 && (both.trim_bottom - 1.5).abs() < 0.01, "Both trims half each end");
        assert!((both.paint_dy + 1.5).abs() < 0.01, "and the paint follows the top trim: {}", both.paint_dy);
        // The guards: no style, a no-op style, no line height, or a line height under the font.
        assert_eq!(crate::layout::node::line_geometry_from(Some(24.0), None, natural).paint_dy, 0.0);
        assert_eq!(crate::layout::node::line_geometry_from(None, Some(LineHeightStyle::new(A::Top, T::None)), natural).paint_dy, 0.0);
        assert_eq!(crate::layout::node::line_geometry_from(Some(20.0), Some(LineHeightStyle::new(A::Top, T::None)), natural).paint_dy, 0.0, "a line height below the font has no extra leading");
    }

    /// An EMPTY paragraph has no glyphs, so `get_font_at(0)` is the only thing that can answer —
    /// and it must not panic, because every empty text field carries the style too.
    #[test]
    fn an_empty_text_survives_a_line_height_style() {
        use crate::text::{LineHeightAlignment as A, LineHeightStyle, LineHeightTrim as T};
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("")
                .line_height(30.0)
                .line_height_style(LineHeightStyle::new(A::Bottom, T::Both))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let node = &composer.arena_nodes()[root];
        let para = node.cached_paragraph.borrow();
        let geo = para.as_ref().map(|p| crate::layout::node::line_geometry(node, p));
        assert!(geo.is_some(), "an empty text still measures");
        assert!(node.measured_size.height >= 0.0);
    }
/// A truncated paragraph answers hit tests in DISPLAY bytes; the consumers that store content
    /// offsets (caret, drag selection) have to translate. Measured before the fix on a 20-byte
    /// content truncated to 16: the left edge answered 0 where the content offset is 4, and the right
    /// edge answered 16 where the content offset is 20.
    #[test]
    fn a_hit_test_on_a_truncated_run_maps_back_to_the_content() {
        let content = "0123456789abcdefghij";
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            Text::new(content)
                .font_size(16.0)
                .overflow(crate::text::TextOverflow::StartEllipsis)
                .soft_wrap(false)
                .modifier(Modifier::new().size(120.0, 30.0))
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let node = &composer.arena_nodes()[root];
        let para = node.cached_paragraph.borrow();
        let para = para.as_ref().unwrap();
        let span = para.display_span().expect("the run was truncated");
        assert!(span.head_bytes == 0, "start ellipsis keeps no head: {span:?}");
        assert!(para.text_len() < content.len(), "the paragraph holds the truncated text");
        let layout = crate::text::TextLayout::new(para);
        let left = layout.get_closest_grapheme_cluster_at(skia_safe::Point::new(2.0, 10.0));
        let right = layout.get_closest_grapheme_cluster_at(skia_safe::Point::new(110.0, 10.0));
        assert_eq!(left, 0, "the display offset at the left edge");
        assert_eq!(para.content_offset(left), span.tail_start_byte, "the marker means the tail start");
        assert_eq!(para.content_offset(right), content.len(), "the right edge is the content end");
        // Identity when nothing was truncated.
        let plain = crate::text::build_plain_paragraph("abc", 16.0, crate::graphics::Color::BLACK, 400.0);
        assert_eq!(plain.content_offset(2), 2);
    }


    /// Compose's `TextAlign.Start` / `End` are direction-relative: the resolution happens in
    /// `Text::build`, where the layout direction is known, because the measure and paint layers only
    /// speak physical edges.
    #[test]
    fn start_and_end_align_resolve_against_the_direction() {
        use crate::layout::LayoutDirection;
        fn align_of(align: TextAlign, direction: LayoutDirection) -> TextAlign {
            crate::layout::direction::provides(direction, || {
                let mut composer = crate::runtime::composer::Composer::new();
                composer.compose(move |ctx| { Text::new("x").align(align).build(ctx); });
                composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
                let root = composer.layout_root_idx().unwrap();
                let node = &composer.arena_nodes()[root];
                node.modifier
                    .elements()
                    .iter()
                    .find_map(|el| match el {
                        ModifierElement::TextContent { align, .. } => Some(*align),
                        _ => None,
                    })
                    .expect("the leaf carries its alignment")
            })
        }
        assert_eq!(align_of(TextAlign::Start, LayoutDirection::Ltr), TextAlign::Left);
        assert_eq!(align_of(TextAlign::Start, LayoutDirection::Rtl), TextAlign::Right);
        assert_eq!(align_of(TextAlign::End, LayoutDirection::Ltr), TextAlign::Right);
        assert_eq!(align_of(TextAlign::End, LayoutDirection::Rtl), TextAlign::Left);
        assert_eq!(align_of(TextAlign::Center, LayoutDirection::Rtl), TextAlign::Center, "an explicit physical value survives");
    }

    /// The user's text-size setting (`Density.font_scale`) must grow the GLYPHS and not the boxes:
    /// Compose resolves `sp` through it and leaves `dp` alone. This is the whole contract, measured on
    /// the same leaf at two scales.
    #[test]
    fn a_system_text_scale_grows_the_type_and_not_the_layout() {
        use crate::runtime::density::with_density;
        use crate::unit::Density;
        fn measure(font_scale: f32) -> (f32, f32, f32) {
            with_density(Density::from_density_and_font_scale(1.0, font_scale), || {
                let mut composer = crate::runtime::composer::Composer::new();
                composer.compose(|ctx| {
                    Text::new("Ag")
                        .font_size(14.0)
                        .modifier(Modifier::new().size(40.0, 20.0))
                        .build(ctx);
                });
                composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
                let root = composer.layout_root_idx().unwrap();
                let node = &composer.arena_nodes()[root];
                let para = node.cached_paragraph.borrow();
                let para = para.as_ref().unwrap();
                (node.measured_size.width, node.measured_size.height, para.height())
            })
        }
        let (small_w, small_h, small_para) = measure(1.0);
        let (big_w, big_h, big_para) = measure(2.0);
        // 14 sp at scale 2 is 28 px of type: the paragraph is taller, and the 20 dp box it was
        // constrained to does not grow with it.
        assert!(
            big_para > small_para * 1.8,
            "the paragraph grows with the scale: {small_para} -> {big_para}"
        );
        // The measured BOX does not, because it was pinned in dp.
        assert!(small_h < big_para, "the unresolved box stays at its dp height: {small_h} vs {big_para}");
        assert_eq!(small_w, 40.0, "the 40 dp width is a dp, not an sp");
        assert_eq!(big_w, 40.0, "and it does not grow with the text scale");
        assert_eq!(small_h, 20.0);
        assert_eq!(big_h, 20.0);
    }

    /// The chain the frame path relies on: what the platform reports lands in every window's
    /// `Density`, and a `Text` measured under it grows — the whole accessibility path, minus the
    /// registry read itself (covered in `runtime::platform`).
    #[test]
    fn the_installed_text_scale_reaches_a_measured_text() {
        use crate::runtime::density::{
            density_with_system_font_scale, install_measured_text_scale, system_font_scale,
            with_density,
        };
        let previous = system_font_scale();
        // Whatever this machine reports, a REPEAT is not a change and a different factor is.
        let _ = install_measured_text_scale(1.5);
        assert!(!install_measured_text_scale(1.5), "installing the same factor reports nothing");
        assert!(install_measured_text_scale(2.0), "a different factor reports a change");
        assert_eq!(density_with_system_font_scale(1.5).font_scale, 2.0, "the window density carries it");
        assert_eq!(density_with_system_font_scale(1.5).density, 1.5, "the pixel density is untouched");
        let height_at = |scale: f32| {
            with_density(crate::unit::Density::from_density_and_font_scale(1.0, scale), || {
                let mut composer = crate::runtime::composer::Composer::new();
                composer.compose(|ctx| { Text::new("Ag").font_size(14.0).build(ctx); });
                composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
                let root = composer.layout_root_idx().unwrap();
                composer.arena_nodes()[root].measured_size.height
            })
        };
        let plain = height_at(1.0);
        let doubled = height_at(2.0);
        assert!(doubled > plain * 1.8, "14 sp at scale 2 is 28 px of type: {plain} -> {doubled}");
        let _ = install_measured_text_scale(previous);
    }

    /// `letter_spacing` is a `TextUnit` now: an `em` resolves against the leaf's own font size, and
    /// an `sp` resolves through the density's text scale. Both must reach the payload as pixels,
    /// because that is what the paragraph builder consumes.
    #[test]
    fn a_letter_spacing_unit_resolves_against_size_and_scale() {
        use crate::runtime::density::with_density;
        use crate::unit::Density;
        fn payload_spacing(font_size: f32, spacing: TextUnit, scale: f32) -> f32 {
            with_density(Density::from_density_and_font_scale(1.0, scale), || {
                let mut composer = crate::runtime::composer::Composer::new();
                composer.compose(move |ctx| {
                    Text::new("x").font_size(font_size).letter_spacing(spacing).build(ctx);
                });
                composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
                let root = composer.layout_root_idx().unwrap();
                composer.arena_nodes()[root]
                    .modifier
                    .elements()
                    .iter()
                    .find_map(|el| match el {
                        ModifierElement::TextContent { letter_spacing, .. } => Some(*letter_spacing),
                        _ => None,
                    })
                    .unwrap()
            })
        }
        // 0.1 em at 20 px of type is 2 px; at 40 px it is 4 px.
        assert!((payload_spacing(20.0, TextUnit::Em(0.1), 1.0) - 2.0).abs() < 0.01);
        assert!((payload_spacing(40.0, TextUnit::Em(0.1), 1.0) - 4.0).abs() < 0.01);
        // 2 sp at a 1.5 text scale is 3 px, and at 1.0 it is 2 px.
        assert!((payload_spacing(20.0, TextUnit::Sp(crate::unit::Sp(2.0)), 1.5) - 3.0).abs() < 0.01);
        assert!((payload_spacing(20.0, TextUnit::Sp(crate::unit::Sp(2.0)), 1.0) - 2.0).abs() < 0.01);
        // A bare f32 is an `sp`, so the old call shape still means the same thing at scale 1.
        assert!((payload_spacing(20.0, 2.0.into(), 1.0) - 2.0).abs() < 0.01);
    }

    /// The richer `onTextLayout`: the same snapshot plus the paragraph, so Compose's per-line
    /// queries answer. Measured on a two-line run: offset 0 is on the first line and offset 3 (after
    /// the newline) on the second, the first line ends at the newline, and every range has a box.
    ///
    /// The callback fires once per measure — including the narrow first pass — so the assertions use
    /// the LAST snapshot, which is the one the frame drew.
    #[test]
    fn the_layout_callback_answers_the_per_line_queries() {
        use std::sync::{Arc, Mutex};
        #[derive(Clone, Debug)]
        #[allow(dead_code)]
        struct Seen {
            line_of_0: usize,
            line_of_3: usize,
            line_count: usize,
            line0: Option<std::ops::Range<usize>>,
            box0: Option<f32>,
            caret0: Option<f32>,
            word0: std::ops::Range<usize>,
            hit: usize,
        }
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let sink = sink.clone();
            Text::new("ab\ncd")
                .font_size(16.0)
                .modifier(Modifier::new().fill_max_width())
                .on_text_layout_with_layout(
                    move |result: &TextLayoutResult, layout: &crate::text::TextLayout<'_>| {
                        let row = Seen {
                            line_of_0: layout.get_line_for_offset(0),
                            line_of_3: layout.get_line_for_offset(3),
                            line_count: result.line_count,
                            line0: layout.get_line_range(0),
                            box0: layout.get_bounding_box(0..2).map(|b| b.top),
                            caret0: layout.get_cursor_rect(0).map(|c| c.top),
                            word0: layout.get_word_boundary(0),
                            hit: layout.get_offset_for_position(skia_safe::Point::new(2.0, 4.0)),
                        };
                        sink.lock().unwrap().push(row);
                    },
                )
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let rows = seen.lock().unwrap().clone();
        let last = rows.last().expect("the callback fired").clone();
        assert!(!rows.is_empty());
        assert_eq!(last.line_count, 2, "two lines: {last:?}");
        assert_eq!(last.line_of_0, 0, "offset 0 is on the first line: {last:?}");
        assert_eq!(last.line_of_3, 1, "offset 3 is past the newline: {last:?}");
        assert_eq!(last.line0, Some(0..2), "line 0 runs to the newline: {last:?}");
        assert!(last.box0.is_some(), "the range has a box: {last:?}");
        assert!(last.caret0.is_some(), "the caret has a rect: {last:?}");
        assert_eq!(last.word0, 0..2, "the word at 0 is `ab`: {last:?}");
        assert!(last.hit <= 2, "a click at the top-left answers the first characters: {last:?}");
    }
    /// Compose's `autoSize`: the font shrinks until the text fits the box.
    ///
    /// The payload carries the window and the search runs at measure time, where the constraints
    /// exist. Before this the only way to fit text was to guess a size and read `on_text_layout`.
    #[test]
    fn auto_size_shrinks_the_font_until_the_text_fits() {
        fn measure(auto: bool) -> (f32, f32) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                let text = Text::new("The quick brown fox jumps over the lazy dog").font_size(24.0);
                let text = if auto { text.auto_size(8.0, 24.0) } else { text };
                text.modifier(Modifier::new().size(120.0, 40.0)).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let nodes = composer.arena_nodes();
            let node = nodes.iter().find(|n| n.has_text_content).expect("a text node");
            let paragraph = node.cached_paragraph.borrow();
            let paragraph = paragraph.as_ref().expect("measured");
            (paragraph.height(), node.measured_size.height)
        }
        let (natural, natural_box) = measure(false);
        let (shrunk, shrunk_box) = measure(true);
        assert!(
            natural > 40.0,
            "24 sp of that sentence is taller than the 40 dp box: {natural}"
        );
        assert!(
            shrunk < natural,
            "autoSize must shrink the layout: {shrunk} vs {natural}"
        );
        assert!(
            shrunk <= 40.5,
            "the shrunk layout fits the box: {shrunk} in {shrunk_box}"
        );
    }
    /// A child style can CLEAR an inherited decoration (D-1).
    ///
    /// `merge_text_styles` used to OR the two `bool` flags, so `TextDecoration::NONE` on the child
    /// was indistinguishable from "the child said nothing" and the underline survived. Compose's
    /// `TextStyle.merge` replaces the decoration when the child specifies one, and the field is an
    /// `Option` now so that "none" is a value the child can state.
    #[test]
    fn a_child_style_can_clear_an_inherited_decoration() {
        let base = crate::text::TextStyle::new()
            .text_decoration(crate::text::TextDecoration::UNDERLINE);
        let cleared = merge_text_styles(&base, &crate::text::TextStyle::new().text_decoration(crate::text::TextDecoration::NONE));
        assert!(
            !cleared.decoration().underline,
            "an explicit NONE must clear the inherited underline: {:?}",
            cleared.decoration()
        );

        let kept = merge_text_styles(&base, &crate::text::TextStyle::new().font_size(20.0));
        assert!(
            kept.decoration().underline,
            "a child that says nothing keeps what it inherited: {:?}",
            kept.decoration()
        );
    }
    /// `TextUnit::Em` is a multiple of the ENCLOSING size, not an absolute one.
    ///
    /// Compose's `TextUnitType.Em`: `2.em` under a 16 sp style is 32 sp. Before this variant
    /// existed there was no way to say it at all (`Sp` and `Px` only), and the payload stores
    /// resolved logical pixels, so the resolution has to happen in `Text::build` where the
    /// inherited style is in hand.
    #[test]
    fn an_em_font_size_resolves_against_the_inherited_size() {
        use crate::text::TextStyle;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            crate::components::ProvideTextStyle(
                TextStyle::new().font_size(crate::unit::TextUnit::Sp(crate::unit::Sp(16.0))),
                ctx,
                |ctx| {
                    Text::new("hi")
                        .font_size(crate::unit::TextUnit::Em(2.0))
                        .build(ctx);
                },
            );
        });
        let nodes = composer.arena_nodes();
        let node = nodes.iter().find(|n| n.has_text_content).expect("a text node");
        let size = node.modifier.elements().iter().find_map(|el| match el {
            crate::modifier::ModifierElement::TextContent { font_size, .. } => Some(*font_size),
            _ => None,
        }).expect("the payload carries a size");
        assert!((size - 32.0).abs() < 0.01, "2 em of 16 sp is 32 logical px, got {size}");
    }
    /// Compose's `validateMinMaxLines` (`HeightInLinesModifier.kt:454-461`): a non-positive cap is a
    /// bad parameter, not a silent one.
    #[test]
    #[should_panic(expected = "must be greater than zero")]
    fn a_zero_line_cap_is_rejected() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("x").max_lines(0).build(ctx);
        });
    }

    /// And `min_lines` may not exceed `max_lines` — otherwise the box reserves lines the cap can
    /// never fill.
    #[test]
    #[should_panic(expected = "less than or equal to max_lines")]
    fn min_lines_above_max_lines_is_rejected() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("x").min_lines(5).max_lines(2).build(ctx);
        });
    }

    /// The payload compares callbacks by identity: a SHARED one keeps two rebuilds equal (so the slot
    /// can Skip), a fresh closure each build does not.
    #[test]
    fn a_shared_callback_lets_the_leaf_skip() {
        let payload_shared = |callback: std::sync::Arc<dyn Fn(&TextLayoutResult) + Send + Sync>| {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Text::new("x").font_size(14.0).on_text_layout_shared(callback).build(ctx);
            });
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].modifier.clone()
        };
        let payload_closure = || {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                Text::new("x").font_size(14.0).on_text_layout(|_: &TextLayoutResult| {}).build(ctx);
            });
            let root = composer.layout_root_idx().unwrap();
            composer.arena_nodes()[root].modifier.clone()
        };
        let shared = std::sync::Arc::new(|_: &TextLayoutResult| {});
        assert!(
            payload_shared(shared.clone()).param_eq(&payload_shared(shared.clone())),
            "one shared sink compares equal across rebuilds"
        );
        assert!(
            !payload_closure().param_eq(&payload_closure()),
            "a closure built per frame is a changed parameter"
        );
        assert!(
            !payload_shared(shared).param_eq(&payload_closure()),
            "a shared sink and a closure are different parameters"
        );
    }
}
