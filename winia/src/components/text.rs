//! Text 组件 — 对齐 Compose Material3 Text
//!
//! - 颜色优先级: .color() > style.color > LocalTextStyle > WiniaTheme on_surface
//! - ProvideTextStyle 为子树设置默认文字样式
//! - 单独参数（font_size 等）优先级高于 style 参数

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
// LocalTextStyle —— 子树默认文字样式
// ═══════════════════════════════════════════════════════════

pub(crate) static LOCAL_TEXT_STYLE: LazyLock<CompositionLocal<TextStyle>> = LazyLock::new(|| {
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
// Text 组件
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
    letter_spacing: Option<f32>,
    line_height: Option<TextUnit>,
    min_lines: Option<usize>,
    text_decoration: Option<TextDecoration>,
    font_family: Option<String>,
    /// Compose's `onTextLayout`: called after every measure with the layout the text produced.
    on_text_layout: Option<Arc<dyn Fn(&TextLayoutResult) + Send + Sync>>,
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

    /// 字间距（逻辑像素，对标 Compose `TextStyle.letterSpacing`）
    pub fn letter_spacing(mut self, spacing: f32) -> Self { self.letter_spacing = Some(spacing); self }

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

    /// 行高（`TextUnit`；裸 `f32` 按 Sp 解释，对标 Compose `TextStyle.lineHeight`）
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

    /// 设置文字样式（单独参数优先级高于此样式）
    pub fn style(mut self, style: TextStyle) -> Self { self.style = Some(style); self }

    /// 设置背景色和形状（委托到 Modifier::background）
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
        let final_letter_spacing = self.letter_spacing.or(style.letter_spacing).unwrap_or(0.0);
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

        // ⚠ 颜色优先级：显式 color > LOCAL_TEXT_STYLE > **内容色**
        // （WiniaTheme::content_color——对标 Compose LocalContentColor：Text 默认色
        // 就是 LocalContentColor，with_content_color 作用域内自动跟随）> 主题
        // on_surface。此前缺 content_color 一步——Tooltip 用
        // with_content_color(inverse_on_surface) 提供文字色，Text 却落到 on_surface
        // （配色错误）；Button/Chip 的 with_content_color 传内容色对 Text 同样无效
        let final_color = self.color
            .or(style.color)
            .unwrap_or_else(|| crate::theme::WiniaTheme::content_color());

        let content_len = self.content.len();
        // 注册到选区容器（供文本拖动选中使用）——仅在 SelectionContainer 的
        // provides 作用域内注册；须在 move self.content 之前调用（借用）。
        // set_current_node_registrar 必须在 start_leaf 之后（desc 已创建）。
        let (reg_for_node, registered_off) = {
            if let Some(reg) = ctx.selection_registrar().or_else(|| crate::text::selection::LOCAL_SELECTION_REGISTRAR.try_current()) {
                let off = reg.register(key, &self.content);
                (Some(reg), Some((key, content_len, off)))
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
            final_font_size.to_logical_px(crate::runtime::density::current_density()),
            final_color,
            final_font_weight,
            final_font_style,
            final_max_lines,
            final_align,
            final_overflow,
            final_soft_wrap,
            final_letter_spacing,
            final_line_height.map(|u| u.to_logical_px(crate::runtime::density::current_density())),
            crate::modifier::TextExtras {
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
        if let Some(reg) = reg_for_node {
            ctx.set_current_node_registrar(reg);
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
        assert_eq!(text.letter_spacing, Some(2.0));
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

    /// 测量高随字号线性变化（验证 label 字号动画后测量正确——12sp 高
    /// 约为 16sp 的 3/4，若测量不随字号变则字号动画视觉无效）
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
