use super::super::*;

// ── Draw Modifier 方法 ──

/// The text parameters that are not part of the original payload — Compose's `minLines`,
/// `textDecoration`, `fontFamily` and the `onTextLayout` callback. Grouped so the builder keeps a
/// readable signature and a caller that uses none of them writes `TextExtras::default()`.
#[derive(Default)]
pub(crate) struct TextExtras {
    /// The minimum number of lines the box reserves (`TextStyle.minLines`).
    pub min_lines: usize,
    pub underline: bool,
    pub line_through: bool,
    /// A font family name, resolved by Skia's font collection.
    pub font_family: Option<String>,
    /// Called after every measure with the layout the text produced (Compose's `onTextLayout`).
    pub on_text_layout: Option<
        std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult) + Send + Sync>,
    >,
    /// Compose's `autoSize`: `(min, max)` in logical pixels, or `None` to lay out at the stated size.
    pub auto_size: Option<(f32, f32)>,
    /// Compose's `TextStyle.shadow`.
    pub shadow: Option<crate::text::TextShadow>,
    /// Compose's `TextStyle.fontFeatureSettings`.
    pub font_feature_settings: Option<String>,
    /// Compose's `TextStyle.drawStyle`.
    pub draw_style: Option<crate::text::TextDrawStyle>,
    /// Compose's `TextStyle.brush`.
    pub brush: Option<crate::text::TextBrush>,
    /// Compose's `TextStyle.textGeometricTransform`.
    pub transform: Option<crate::text::TextGeometricTransform>,
}

impl Modifier {
    /// 设置背景色和形状
    /// 设置背景色和形状。
    ///
    /// 统一入口：静态颜色与动画闭包自动适配（`impl Into<BackgroundColor>`）。
    ///
    /// **静态用法**（构建时固定颜色）：
    /// ```
    /// # use winia::prelude::*;
    /// let _m = Modifier::new().background(Color::RED, Shape::Circle);
    /// ```
    ///
    /// **动画用法**（渲染时每帧求值，不触发重组）：
    /// ```ignore
    /// .background(|| pulse.peek(), Shape::Circle)
    /// ```
    ///
    /// **原则**：静态绘制属性传 `Color`；需要动画（颜色随帧变化）时传闭包，
    /// 闭包内用 `State::peek()` 读取动画值（不要用 `get()`——会注册依赖触发重组）。
    /// 动画值由 `Visual::set` 写入（不 notify），配合动画引擎每帧 `request_redraw` 实现零重组。
    pub fn background(self, color: impl Into<BackgroundColor>, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        self.push(ModifierElement::Background {
            color: color.into(),
            shape: shape.into(),
        })
    }

    /// Fill the node with a [`crate::graphics::Brush`] — a gradient, or a solid color chosen at
    /// runtime (Compose `Modifier.background(brush, shape)`).
    ///
    /// ```ignore
    /// Modifier::new().background_brush(
    ///     Brush::linear_gradient([colors.primary, colors.tertiary]).diagonal(),
    ///     Shape::rounded(16.0),
    /// )
    /// ```
    ///
    /// Accepts a value or a closure, so a gradient can follow animated state the same way
    /// `background` accepts an animated color. Gradient coordinates are FRACTIONS of the node's
    /// bounds — see the module docs on [`crate::graphics::brush`] for why that differs from Compose.
    pub fn background_brush(
        self,
        brush: impl Into<crate::graphics::BrushSource>,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
    ) -> Self {
        self.push(ModifierElement::BackgroundBrush {
            brush: brush.into(),
            shape: shape.into(),
        })
    }

    /// 统一构造 TextContent 元素（Text/TextField 共用——字段单一来源，P3-6）
    #[allow(dead_code)] // the tests in this file call it
    pub(crate) fn text_content(
        self,
        content: String,
        font_size: f32,
        color: crate::graphics::Color,
        font_weight: crate::text::FontWeight,
        font_style: crate::text::FontSlant,
        max_lines: usize,
        align: crate::text::TextAlign,
        overflow: crate::text::TextOverflow,
        soft_wrap: bool,
    ) -> Self {
        self.text_content_full(
            content, font_size, color, font_weight, font_style,
            max_lines, align, overflow, soft_wrap,
            0.0, None,
            TextExtras::default(),
        )
    }

    /// 全参版（含 letter_spacing/line_height——Text 组件用，对标 Compose
    /// TextStyle.letterSpacing/lineHeight）
    pub(crate) fn text_content_full(
        self,
        content: String,
        font_size: f32,
        color: crate::graphics::Color,
        font_weight: crate::text::FontWeight,
        font_style: crate::text::FontSlant,
        max_lines: usize,
        align: crate::text::TextAlign,
        overflow: crate::text::TextOverflow,
        soft_wrap: bool,
        letter_spacing: f32,
        line_height: Option<f32>,
        extras: crate::modifier::builders::draw::TextExtras,
    ) -> Self {
        let m = self.push(ModifierElement::TextContent {
            min_lines: extras.min_lines,
            underline: extras.underline,
            line_through: extras.line_through,
            font_family: extras.font_family,
            on_text_layout: extras.on_text_layout,
            content,
            font_size,
            color,
            font_weight,
            font_style,
            max_lines,
            align,
            overflow,
            soft_wrap,
            letter_spacing,
            line_height,
            auto_size: extras.auto_size,
        });
        // Only carry the paint element when it says something: the extras are rare, and an
        // always-present element costs a comparison on every recomposition and a slot on every
        // text node in the tree.
        let paint = ModifierElement::TextPaint {
            shadow: extras.shadow,
            font_feature_settings: extras.font_feature_settings,
            draw_style: extras.draw_style,
            brush: extras.brush,
            transform: extras.transform,
        };
        let says_something = !matches!(
            &paint,
            ModifierElement::TextPaint {
                shadow: None,
                font_feature_settings: None,
                draw_style: None,
                brush: None,
                transform: None,
            }
        );
        if says_something { m.push(paint) } else { m }
    }
    /// 设置边框。颜色可以是一个 `Color`，也可以是渲染期求值的闭包/状态
    /// （`impl Into<BorderColor>`）——同一个元素存载体：静态值内联，闭包在绘制期求值
    /// （配合 `peek()` 做颜色过渡）。
    ///
    /// One element for both: a changed STATIC colour is a changed parameter (the node re-enters),
    /// while a dynamic one is a parameter read at paint time.
    pub fn border(
        self,
        width: impl Into<Dimension>,
        color: impl Into<crate::graphics::BorderColor>,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
    ) -> Self {
        self.push(ModifierElement::Border {
            width: width.into(),
            color: color.into(),
            shape: shape.into(),
        })
    }

    /// 设置裁剪形状
    pub fn clip(self, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        self.push(ModifierElement::Clip {
            shape: shape.into(),
        })
    }

    /// 内容模糊（GPU saveLayer，零回读）
    pub fn blur(self, radius: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.push(ModifierElement::Blur { radius: radius.into() })
    }

    /// 背景模糊（毛玻璃）——渲染期在节点内容绘制前即时 snapshot+blur
    pub fn backdrop_blur(self, radius: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.push(ModifierElement::BackdropBlur { radius: radius.into() })
    }

    /// 文本输入框容器视觉（TextField 组件内部使用——M3 容器绘制参数）
    pub fn text_field_visual(
        self,
        variant: crate::text::field::TextFieldVariant,
        shape: impl Into<crate::graphics::Dyn<Shape>>,
        colors: crate::text::field::TextFieldColors,
        enabled: bool,
        read_only: bool,
        cursor_color: Color,
        indicator_color: crate::runtime::state::State<crate::graphics::Color>,
        focus_progress: crate::runtime::state::State<f32>,
        offset_mapping: Option<std::sync::Arc<dyn crate::text::transformation::OffsetMapping>>,
        supporting: Option<SupportingVisual>,
    ) -> Self {
        self.push(ModifierElement::TextFieldVisual {
            variant,
            shape: shape.into(),
            colors,
            enabled,
            read_only,
            cursor_color,
            indicator_color,
            focus_progress,
            offset_mapping,
            supporting,
        })
    }

    /// 文本输入框偏移映射（TextField 组件内部使用——裸 TextField 无
    /// TextFieldVisual 时挂载；`offset_mapping_for_node` 沿 parent 链向上
    /// 查找 TextFieldVisual.offset_mapping 或本元素）
    pub fn text_field_offset_mapping(
        self,
        offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping>,
    ) -> Self {
        self.push(ModifierElement::TextFieldOffsetMapping { offset_mapping })
    }
}

// ── Input Modifier 方法 ──
