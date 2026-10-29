use super::super::*;

// ── Draw modifier methods ──

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
    /// The richer `onTextLayout` sink — carried here so the leaf can hand it the paragraph.
    pub on_text_layout_layout: Option<std::sync::Arc<dyn Fn(&crate::text::TextLayoutResult, &crate::text::TextLayout<'_>) + Send + Sync>>,
    /// Compose's `TextStyle.textGeometricTransform`.
    pub transform: Option<crate::text::TextGeometricTransform>,
    /// Compose's `TextStyle.lineHeightStyle`.
    pub line_height_style: Option<crate::text::LineHeightStyle>,
}

impl Modifier {
    /// Set the background colour and shape.
    ///
    /// One entry point that takes a static colour or an animated closure (`impl Into<BackgroundColor>`).
    ///
    /// Static: a colour fixed at build time.
    /// ```
    /// # use winia::prelude::*;
    /// let _m = Modifier::new().background(Color::RED, Shape::Circle);
    /// ```
    ///
    /// Animated: evaluated per frame at render time, without recomposing.
    /// ```ignore
    /// .background(|| pulse.peek(), Shape::Circle)
    /// ```
    ///
    /// The rule: a static paint property takes a `Color`; anything that animates takes a closure,
    /// and inside it read the animated value with `State::peek()` — `get()` would register a
    /// dependency and trigger a recomposition. The animation engine writes through `Visual::set`
    /// (no notification) and requests a redraw per frame, so the whole path recomposes nothing.
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

    /// Build the `TextContent` element, shared by `Text` and `TextField` so its field list lives in
    /// one place.
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

    /// The full-parameter form (letter spacing, line height, …) that the `Text` component uses —
    /// Compose's
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
            line_height_style: extras.line_height_style,
        };
        let says_something = !matches!(
            &paint,
            ModifierElement::TextPaint {
                shadow: None,
                font_feature_settings: None,
                draw_style: None,
                brush: None,
                transform: None,
                line_height_style: None,
            }
        );
        let m = if says_something { m.push(paint) } else { m };
        // The richer `onTextLayout` sink rides its own element, so a leaf that does not use it carries
        // nothing extra.
        match extras.on_text_layout_layout.clone() {
            Some(callback) => m.push(ModifierElement::TextLayoutCallback { callback }),
            None => m,
        }
    }
    /// Set a border. The colour is a `Color` or a closure/state evaluated at render time
    /// (`impl Into<BorderColor>`): one element carries either, a static value inline and a closure
    /// evaluated while painting (which is how a colour transition works with `peek()`).
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

    /// Clip to a shape.
    pub fn clip(self, shape: impl Into<crate::graphics::Dyn<Shape>>) -> Self {
        self.push(ModifierElement::Clip {
            shape: shape.into(),
        })
    }

    /// Blur the content (a GPU `saveLayer`; nothing is read back).
    pub fn blur(self, radius: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.push(ModifierElement::Blur { radius: radius.into() })
    }

    /// Blur what is behind the node (frosted glass): a snapshot and blur taken at render time,
    /// before the node's own content is drawn.
    pub fn backdrop_blur(self, radius: impl Into<crate::graphics::Dyn<f32>>) -> Self {
        self.push(ModifierElement::BackdropBlur { radius: radius.into() })
    }

    /// The text-field container's visuals (internal to `TextField`: the M3 container paint
    /// parameters).
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

    /// A text field's offset mapping (internal to `TextField`): what a bare field carries when it has
    /// no `TextFieldVisual`. `offset_mapping_for_node` walks the parent chain for either this element
    /// or `TextFieldVisual::offset_mapping`.
    pub fn text_field_offset_mapping(
        self,
        offset_mapping: std::sync::Arc<dyn crate::text::transformation::OffsetMapping>,
    ) -> Self {
        self.push(ModifierElement::TextFieldOffsetMapping { offset_mapping })
    }
}

// ── Input modifier methods ──
