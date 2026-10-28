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
use crate::text::{FontSlant, FontWeight, TextAlign, TextDecoration, TextLayoutResult, TextOverflow, TextStyle};
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
        underline: override_.underline || base.underline,
        strikethrough: override_.strikethrough || base.strikethrough,
        background: override_.background.or(base.background),
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
        // 参数声明（对标 Switch::build 的 ctx.changed）：父作用域读 State 会触发
        // 父重组并重新调用 build，但叶子槽的 dirty 由本节点参数决定——不声明
        // content 变化，物化可能走 Clean/折叠路径，cached_paragraph 保留旧文本，
        // 渲染一直画旧内容，直到下一次外部事件触发重绘。
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
        let final_decoration = self.text_decoration.unwrap_or_else(|| style.decoration());
        let final_font_family = self.font_family.clone().or_else(|| style.font_family.clone());

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
    /// `TextAlign::Start` / `End` resolve against the layout direction at build time.
    ///
    /// They never reach the payload, which is why the paint path keeps working unchanged; this
    /// pins the resolution itself.
    #[test]
    fn start_and_end_align_resolve_against_the_direction() {
        use crate::layout::LayoutDirection;
        let mut composer = crate::runtime::composer::Composer::new();
        let mut seen = Vec::new();
        for (direction, align) in [
            (LayoutDirection::Ltr, TextAlign::Start),
            (LayoutDirection::Ltr, TextAlign::End),
            (LayoutDirection::Rtl, TextAlign::Start),
            (LayoutDirection::Rtl, TextAlign::End),
        ] {
            composer.compose(|ctx| {
                crate::layout::direction::provides(direction, || {
                    Text::new("hi").align(align).build(ctx);
                });
            });
            let nodes = composer.arena_nodes();
            let node = nodes.iter().find(|n| n.has_text_content).expect("a text node");
            let resolved = node
                .modifier
                .elements()
                .iter()
                .find_map(|el| match el {
                    crate::modifier::ModifierElement::TextContent { align, .. } => Some(*align),
                    _ => None,
                })
                .expect("the payload carries an alignment");
            seen.push((direction, align, resolved));
        }
        assert_eq!(seen[0].2, TextAlign::Left, "Start is left in LTR");
        assert_eq!(seen[1].2, TextAlign::Right, "End is right in LTR");
        assert_eq!(seen[2].2, TextAlign::Right, "Start is right in RTL");
        assert_eq!(seen[3].2, TextAlign::Left, "End is left in RTL");
    }
    /// The decoration and the family reach the payload (the plain leaf used to ignore both).
    #[test]
    fn the_payload_carries_decoration_and_family() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Text::new("x")
                .text_decoration(crate::text::TextDecoration::UNDERLINE_AND_LINE_THROUGH)
                .font_family("Segoe UI")
                .build(ctx);
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let payload = nodes[root]
            .modifier
            .elements()
            .iter()
            .find_map(|el| {
                if let ModifierElement::TextContent { underline, line_through, font_family, .. } = el {
                    Some((*underline, *line_through, font_family.clone()))
                } else {
                    None
                }
            })
            .expect("a text payload");
        assert!(payload.0 && payload.1, "both decorations reached the payload");
        assert_eq!(payload.2.as_deref(), Some("Segoe UI"));
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
